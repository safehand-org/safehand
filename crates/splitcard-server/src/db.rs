//! SQLite connection and embedded migrations.

use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

use crate::slug;

/// The schema migrations, embedded at compile time.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Opens `database_url` (an SQLite file path or `sqlite:` URL), creating the
/// file if needed, and runs the migrations.
///
/// # Errors
///
/// Returns an error if the database cannot be opened or migrated.
pub async fn connect(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = if database_url.starts_with("sqlite:") {
        SqliteConnectOptions::from_str(database_url)?
    } else {
        SqliteConnectOptions::new().filename(database_url)
    };
    let options = options
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    migrate(&pool).await?;
    Ok(pool)
}

/// Why the database file's directory cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum DatabaseDirError {
    /// The directory does not exist.
    #[error("database directory {} does not exist", dir.display())]
    Missing {
        /// The directory.
        dir: PathBuf,
    },
    /// The path exists but is not a directory.
    #[error("database directory {} is not a directory", dir.display())]
    NotADirectory {
        /// The path.
        dir: PathBuf,
    },
    /// A file cannot be created there.
    #[error("database directory {} is not writable: {source}", dir.display())]
    NotWritable {
        /// The directory.
        dir: PathBuf,
        /// The error creating a test file.
        source: std::io::Error,
    },
}

/// The file `database_url` names, or `None` for an in-memory database.
fn database_file(database_url: &str) -> Option<PathBuf> {
    let path = match database_url.strip_prefix("sqlite:") {
        Some(rest) => rest.strip_prefix("//").unwrap_or(rest),
        None => database_url,
    };
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let in_memory = path.is_empty() || path == ":memory:" || query.contains("mode=memory");
    (!in_memory).then(|| PathBuf::from(path))
}

/// Checks that the directory holding `database_url`'s file exists and is
/// writable, so a missing or read-only disk fails at startup with a clear
/// message instead of on the first write. In-memory databases pass.
///
/// # Errors
///
/// Returns why the directory cannot be used.
pub fn check_database_dir(database_url: &str) -> Result<(), DatabaseDirError> {
    let Some(file) = database_file(database_url) else {
        return Ok(());
    };
    let dir = match file.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => Path::new(".").to_path_buf(),
    };
    if !dir.exists() {
        return Err(DatabaseDirError::Missing { dir });
    }
    if !dir.is_dir() {
        return Err(DatabaseDirError::NotADirectory { dir });
    }
    let probe = dir.join(format!(".splitcard-write-check-{}", std::process::id()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .and_then(|_| std::fs::remove_file(&probe))
        .map_err(|source| DatabaseDirError::NotWritable { dir, source })
}

/// The migration that turns event teams into standalone teams with slugs.
pub const STANDALONE_TEAMS_VERSION: i64 = 6;

/// Runs every pending migration on one connection.
///
/// Before [`STANDALONE_TEAMS_VERSION`] runs, each existing team's slug is
/// derived from its name with [`slug::derive_slugs`] and handed to the
/// migration in a temporary table on that connection; each one is logged once
/// the migration commits.
///
/// # Errors
///
/// Returns an error if a migration fails; a failed migration changes nothing.
pub async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut conn = pool.acquire().await?;
    MIGRATOR
        .run_to(STANDALONE_TEAMS_VERSION - 1, &mut *conn)
        .await?;
    let applied: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM _sqlx_migrations WHERE version = ?")
            .bind(STANDALONE_TEAMS_VERSION)
            .fetch_optional(&mut *conn)
            .await?;
    let derived = if applied.is_none() {
        let teams: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM teams ORDER BY id")
            .fetch_all(&mut *conn)
            .await?;
        let derived = slug::derive_slugs(&teams);
        sqlx::query(
            "CREATE TEMP TABLE IF NOT EXISTS migration_team_slugs \
             (team_id INTEGER PRIMARY KEY, slug TEXT NOT NULL)",
        )
        .execute(&mut *conn)
        .await?;
        for (team_id, slug) in &derived {
            sqlx::query("INSERT INTO migration_team_slugs (team_id, slug) VALUES (?, ?)")
                .bind(team_id)
                .bind(slug)
                .execute(&mut *conn)
                .await?;
        }
        teams.into_iter().zip(derived).collect()
    } else {
        Vec::new()
    };
    let migrated = MIGRATOR.run(&mut *conn).await;
    sqlx::query("DROP TABLE IF EXISTS temp.migration_team_slugs")
        .execute(&mut *conn)
        .await?;
    migrated?;
    for ((team_id, name), (_, slug)) in derived {
        tracing::info!(team_id, %name, %slug, "derived a slug for an existing team");
    }
    Ok(())
}

/// Opens a fresh, private in-memory database shared by every connection in
/// the pool, and runs the migrations. Used by tests.
///
/// # Errors
///
/// Returns an error if the database cannot be created or migrated.
pub async fn connect_in_memory() -> Result<SqlitePool, sqlx::Error> {
    // `:memory:` gets a unique shared-cache name, so pool connections share it.
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);
    // The database lives only while a connection is open, so keep them all.
    let pool = SqlitePoolOptions::new()
        .min_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

/// Starts a write transaction that takes the database write lock up front, so
/// concurrent writers queue instead of failing to upgrade a read lock.
///
/// # Errors
///
/// Returns an error if a connection or the lock cannot be acquired.
pub async fn begin_write(
    pool: &SqlitePool,
) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>, sqlx::Error> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{DatabaseDirError, check_database_dir, database_file};

    #[test]
    fn database_urls_name_their_file() {
        for (url, file) in [
            ("/var/data/splitcard.db", Some("/var/data/splitcard.db")),
            (
                "sqlite:///var/data/splitcard.db",
                Some("/var/data/splitcard.db"),
            ),
            (
                "sqlite:data/splitcard.db?mode=rwc",
                Some("data/splitcard.db"),
            ),
            ("splitcard.db", Some("splitcard.db")),
            ("sqlite::memory:", None),
            ("sqlite://file.db?mode=memory", None),
        ] {
            assert_eq!(database_file(url), file.map(PathBuf::from), "{url}");
        }
    }

    #[test]
    fn missing_and_unwritable_directories_are_refused() {
        let base = std::env::temp_dir().join(format!("splitcard-dir-check-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("temp dir");
        let ok = base.join("splitcard.db");
        check_database_dir(ok.to_str().expect("utf-8")).expect("writable");
        assert_eq!(
            std::fs::read_dir(&base).expect("list").count(),
            0,
            "no probe left behind"
        );

        let missing = base.join("absent/splitcard.db");
        assert!(matches!(
            check_database_dir(missing.to_str().expect("utf-8")),
            Err(DatabaseDirError::Missing { .. })
        ));
        let file = base.join("plain-file");
        std::fs::write(&file, b"").expect("file");
        let under_file = file.join("splitcard.db");
        assert!(matches!(
            check_database_dir(under_file.to_str().expect("utf-8")),
            Err(DatabaseDirError::NotADirectory { .. })
        ));
        check_database_dir("sqlite::memory:").expect("in-memory needs no directory");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let read_only = base.join("read-only");
            std::fs::create_dir(&read_only).expect("dir");
            std::fs::set_permissions(&read_only, std::fs::Permissions::from_mode(0o555))
                .expect("chmod");
            let result =
                check_database_dir(read_only.join("splitcard.db").to_str().expect("utf-8"));
            // Root can write anywhere; check only where the mode is enforced.
            let enforced = std::fs::write(read_only.join("probe"), b"").is_err();
            std::fs::set_permissions(&read_only, std::fs::Permissions::from_mode(0o755))
                .expect("chmod back");
            if enforced {
                assert!(matches!(result, Err(DatabaseDirError::NotWritable { .. })));
            }
        }
        std::fs::remove_dir_all(&base).expect("clean up");
    }
}
