//! SQLite connection and embedded migrations.

use std::{str::FromStr, time::Duration};

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
