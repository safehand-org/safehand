//! Teams, joining and submission URLs.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use nostr::key::SecretKey;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, SqliteConnection};
use url::Url;

use super::{
    conflict, forbidden, is_unique_violation, parse_json, parse_team_id,
    profiles::require_payable_profile, require_text, team_not_found, unprocessable,
};
use crate::{
    AppState,
    auth::{Authenticated, OptionalAuth},
    db::begin_write,
    db_time,
    error::ApiError,
    slug::check_slug,
};

/// Longest accepted submission URL.
pub const MAX_SUBMISSION_URL_LEN: usize = 2048;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTeam {
    slug: String,
    name: String,
    event_label: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JoinTeam {
    join_code: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetSubmission {
    submission_url: String,
}

/// A stored team.
#[derive(Debug, FromRow)]
pub(crate) struct TeamRow {
    pub(crate) id: i64,
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) event_label: String,
    pub(crate) join_code: String,
    pub(crate) submission_url: Option<String>,
    pub(crate) active_card_hash: Option<String>,
    pub(crate) created_at: i64,
}

/// A participant with their profile's current name and Lightning Address.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct ParticipantRow {
    pub(crate) pubkey: String,
    pub(crate) name: String,
    pub(crate) lightning_address: String,
    pub(crate) joined_at: i64,
}

/// Someone who joined a team, as their profile shows them now.
#[derive(Debug, Clone, Serialize)]
pub struct Participant {
    /// Nostr public key in lowercase hex.
    pub pubkey: String,
    /// Their profile name.
    pub name: String,
    /// Their profile's Lightning Address, shown only to the team's
    /// participants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lightning_address: Option<String>,
    /// Unix seconds.
    pub joined_at: i64,
}

/// A team in API responses.
#[derive(Debug, Serialize)]
pub struct TeamView {
    /// Team id, as a string.
    pub id: String,
    /// Unique across the server; later the team's Lightning Address username.
    pub slug: String,
    /// Team name; cards name it in their `team` field.
    pub name: String,
    /// Free text such as the hackathon's name; cards name it in their
    /// `event` field.
    pub event_label: String,
    /// Present only when the caller is a participant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_code: Option<String>,
    /// Project submission URL, if set.
    pub submission_url: Option<String>,
    /// Hash of the active card, if any.
    pub active_card_hash: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
    /// Participants in join order.
    pub participants: Vec<Participant>,
}

pub(crate) async fn load_team(
    conn: &mut SqliteConnection,
    id: i64,
) -> Result<Option<TeamRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, slug, name, event_label, join_code, submission_url, active_card_hash, \
         created_at FROM teams WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(conn)
    .await
}

pub(crate) async fn load_team_by_slug(
    conn: &mut SqliteConnection,
    slug: &str,
) -> Result<Option<TeamRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, slug, name, event_label, join_code, submission_url, active_card_hash, \
         created_at FROM teams WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(conn)
    .await
}

pub(crate) fn team_slug_not_found() -> ApiError {
    ApiError::not_found("team_not_found", "no team with this slug")
}

pub(crate) async fn load_participants(
    conn: &mut SqliteConnection,
    team_id: i64,
) -> Result<Vec<ParticipantRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT participants.pubkey, profiles.name, profiles.lightning_address, \
         participants.joined_at FROM participants \
         JOIN profiles ON profiles.pubkey = participants.pubkey \
         WHERE participants.team_id = ? ORDER BY participants.joined_at, participants.rowid",
    )
    .bind(team_id)
    .fetch_all(conn)
    .await
}

async fn team_view(
    conn: &mut SqliteConnection,
    team: TeamRow,
    viewer: Option<&str>,
) -> Result<TeamView, sqlx::Error> {
    let participants = load_participants(conn, team.id).await?;
    let is_participant =
        viewer.is_some_and(|viewer| participants.iter().any(|p| p.pubkey == viewer));
    Ok(TeamView {
        id: team.id.to_string(),
        slug: team.slug,
        name: team.name,
        event_label: team.event_label,
        join_code: is_participant.then_some(team.join_code),
        submission_url: team.submission_url,
        active_card_hash: team.active_card_hash,
        created_at: team.created_at,
        participants: participants
            .into_iter()
            .map(|participant| Participant {
                pubkey: participant.pubkey,
                name: participant.name,
                lightning_address: is_participant.then_some(participant.lightning_address),
                joined_at: participant.joined_at,
            })
            .collect(),
    })
}

/// Whether `presented` is the team's join code.
pub(crate) fn join_code_matches(team: &TeamRow, presented: &str) -> bool {
    constant_time_eq(team.join_code.as_bytes(), presented.as_bytes())
}

/// 128 random bits from the OS RNG, as 32 hex characters.
fn new_join_code() -> String {
    let bytes = SecretKey::generate().to_secret_bytes();
    hex::encode(&bytes[..16])
}

/// Compares without an early exit, so timing does not reveal a matching prefix.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

/// `POST /teams`: creates a team from `{ slug, name, event_label }` and
/// joins the caller, who must have a profile, as its first participant. The
/// response includes the join code.
///
/// # Errors
///
/// `invalid_request_json` (400), `invalid_slug` or `empty_field` (422),
/// `profile_required`, `lightning_address_required` or `team_slug_taken` (409).
pub async fn create(
    State(state): State<AppState>,
    auth: Authenticated,
) -> Result<(StatusCode, Json<TeamView>), ApiError> {
    let request: CreateTeam = parse_json(&auth.body, "invalid_request_json")?;
    if let Err(error) = check_slug(&request.slug) {
        return Err(unprocessable(
            "invalid_slug",
            "slug must be 3-32 characters of a-z, 0-9, -, _, . and +, and not reserved",
        )
        .with_details(json!({ "field": "slug", "reason": error.reason() })));
    }
    require_text("name", &request.name)?;
    require_text("event_label", &request.event_label)?;
    let now = db_time(state.clock.now_unix());

    let mut tx = begin_write(&state.db).await?;
    require_payable_profile(&mut tx, &auth.pubkey).await?;
    let inserted = sqlx::query_scalar(
        "INSERT INTO teams (slug, name, event_label, join_code, created_at) \
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&request.slug)
    .bind(&request.name)
    .bind(&request.event_label)
    .bind(new_join_code())
    .bind(now)
    .fetch_one(&mut *tx)
    .await;
    let team_id: i64 = match inserted {
        Ok(id) => id,
        Err(error) if is_unique_violation(&error) => {
            return Err(conflict("team_slug_taken", "a team with this slug exists"));
        }
        Err(error) => return Err(error.into()),
    };
    sqlx::query("INSERT INTO participants (team_id, pubkey, joined_at) VALUES (?, ?, ?)")
        .bind(team_id)
        .bind(&auth.pubkey)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let team = load_team(&mut tx, team_id)
        .await?
        .ok_or_else(team_not_found)?;
    let view = team_view(&mut tx, team, Some(&auth.pubkey)).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// `POST /teams/{id}/join`: joins with `{ join_code }`. The caller must have
/// a profile; their name and Lightning Address on the team are its.
///
/// Idempotent: joining again changes nothing.
///
/// # Errors
///
/// `invalid_request_json` (400), `team_not_found` (404), `invalid_join_code`
/// (403), `profile_required` or `lightning_address_required` (409).
pub async fn join(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: Authenticated,
) -> Result<Json<TeamView>, ApiError> {
    let team_id = parse_team_id(&id)?;
    let request: JoinTeam = parse_json(&auth.body, "invalid_request_json")?;

    let mut tx = begin_write(&state.db).await?;
    let team = load_team(&mut tx, team_id)
        .await?
        .ok_or_else(team_not_found)?;
    if !join_code_matches(&team, &request.join_code) {
        return Err(forbidden("invalid_join_code", "join code is wrong"));
    }
    require_payable_profile(&mut tx, &auth.pubkey).await?;
    sqlx::query(
        "INSERT INTO participants (team_id, pubkey, joined_at) VALUES (?, ?, ?) \
         ON CONFLICT DO NOTHING",
    )
    .bind(team_id)
    .bind(&auth.pubkey)
    .bind(db_time(state.clock.now_unix()))
    .execute(&mut *tx)
    .await?;
    let view = team_view(&mut tx, team, Some(&auth.pubkey)).await?;
    tx.commit().await?;
    Ok(Json(view))
}

/// `GET /teams/{id}`: the team and its participants. The join code and
/// participants' Lightning Addresses are shown only to a participant who
/// sends a valid NIP-98 header.
///
/// # Errors
///
/// `team_not_found` (404), or an `auth_*` code (401) for a bad header.
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
    OptionalAuth(viewer): OptionalAuth,
) -> Result<Json<TeamView>, ApiError> {
    let team_id = parse_team_id(&id)?;
    let mut conn = state.db.acquire().await?;
    let team = load_team(&mut conn, team_id)
        .await?
        .ok_or_else(team_not_found)?;
    Ok(Json(team_view(&mut conn, team, viewer.as_deref()).await?))
}

/// `GET /teams/by-slug/{slug}`: the same view as `GET /teams/{id}`, found by
/// slug.
///
/// # Errors
///
/// `team_not_found` (404), or an `auth_*` code (401) for a bad header.
pub async fn get_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    OptionalAuth(viewer): OptionalAuth,
) -> Result<Json<TeamView>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let team = load_team_by_slug(&mut conn, &slug)
        .await?
        .ok_or_else(team_slug_not_found)?;
    Ok(Json(team_view(&mut conn, team, viewer.as_deref()).await?))
}

/// `PUT /teams/{id}/submission`: a participant sets `{ submission_url }`, an
/// `http` or `https` URL.
///
/// # Errors
///
/// `invalid_request_json` (400), `team_not_found` (404), `not_a_participant`
/// (403), `invalid_submission_url` (422).
pub async fn set_submission(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: Authenticated,
) -> Result<Json<TeamView>, ApiError> {
    let team_id = parse_team_id(&id)?;
    let request: SetSubmission = parse_json(&auth.body, "invalid_request_json")?;
    let valid_url = request.submission_url.len() <= MAX_SUBMISSION_URL_LEN
        && Url::parse(&request.submission_url)
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host().is_some());
    if !valid_url {
        return Err(unprocessable(
            "invalid_submission_url",
            "submission_url must be an http or https URL of at most 2048 characters",
        ));
    }

    let mut tx = begin_write(&state.db).await?;
    load_team(&mut tx, team_id)
        .await?
        .ok_or_else(team_not_found)?;
    require_participant(&mut tx, team_id, &auth.pubkey).await?;
    sqlx::query("UPDATE teams SET submission_url = ? WHERE id = ?")
        .bind(&request.submission_url)
        .bind(team_id)
        .execute(&mut *tx)
        .await?;
    let team = load_team(&mut tx, team_id)
        .await?
        .ok_or_else(team_not_found)?;
    let view = team_view(&mut tx, team, Some(&auth.pubkey)).await?;
    tx.commit().await?;
    Ok(Json(view))
}

/// Fails with `not_a_participant` (403) unless `pubkey` joined the team.
pub(crate) async fn require_participant(
    conn: &mut SqliteConnection,
    team_id: i64,
    pubkey: &str,
) -> Result<(), ApiError> {
    let joined: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM participants WHERE team_id = ? AND pubkey = ?")
            .bind(team_id)
            .bind(pubkey)
            .fetch_optional(conn)
            .await?;
    if joined.is_none() {
        return Err(forbidden(
            "not_a_participant",
            "only team participants can do this",
        ));
    }
    Ok(())
}
