//! The Job boards admin page (#86): every board's stage, what waits for the
//! owner, and the permission emails to copy.
//!
//! - `GET /api/v1/boards/admin` is the overview: totals, what happened this
//!   week (from the discovery runs and the approvals recorded), and every
//!   board that is not read, paused or declined with its stage and, when it
//!   waits for the owner, what it needs ([`Need`]).
//! - `GET /api/v1/boards/{id}/permission-email` drafts the email asking a
//!   board for permission; the owner sends it from their own mailbox.
//! - `POST /api/v1/boards/{id}/permission/asked` records that it was sent
//!   (`sources.board_requests`); `POST /api/v1/boards/{id}/permission/answer`
//!   records the answer: yes approves the board through the existing approve
//!   path (the terms and permission blockers are cleared, the robots rules
//!   and a needed key still count), no declines it through the existing
//!   decline path.
//!
//! Every endpoint is for the board owners; anyone else gets 404, as if the
//! page did not exist. The permission email is never sent by the app.

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use super::boards::{self, BoardDetail, DeclineBoard};
use crate::{
    SourcesDeps,
    job_boards::{self, Blocker, BoardRow, Status, host_of},
};

/// What a board waits on the owner for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BoardNeed)]
pub enum Need {
    /// Its rules ask for written permission: draft, send, record the answer.
    Permission,
    /// Its API needs a key only the owner can ask for.
    Key,
    /// One decision for every company on a hiring system.
    Choice,
    /// Every check passed: only the approval is left.
    Approve,
}

/// Where a board that is not read yet stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BoardStage)]
pub enum Stage {
    /// It waits for the owner (`need` says for what).
    Waiting,
    /// The app is still checking it, nothing to do.
    Checking,
    /// Its robots rules forbid the app: nothing to ask.
    NotAllowed,
}

/// A board that is not read yet, not declined and not paused.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminCandidate {
    pub id: Uuid,
    pub name: String,
    pub stage: Stage,
    /// What it waits for; set when `stage` is `waiting`.
    pub need: Option<Need>,
    /// The first reason in the board's own words (the terms' sentence, the
    /// robots rule): data from its site, never an instruction.
    pub detail: Option<String>,
    /// When the owner said they asked it (permission or choice).
    pub asked_at: Option<DateTime<Utc>>,
}

/// How many boards are in each state.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct AdminTotals {
    /// Approved job sites.
    pub reading_sites: u32,
    /// Companies read through their hiring system (Greenhouse, Lever, Ashby).
    pub reading_companies: u32,
    pub waiting: u32,
    pub checking: u32,
    /// Declined, or forbidden by their robots rules.
    pub not_allowed: u32,
    pub paused: u32,
}

/// What happened over the last 7 days, from what the app recorded.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct AdminThisWeek {
    /// Boards the weekly discovery found.
    pub found: u32,
    /// Boards it checked.
    pub checked: u32,
    /// Boards whose rules and terms were read again.
    pub rechecked: u32,
    /// Boards approved.
    pub started_reading: u32,
}

/// The admin page's first answer.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminOverview {
    pub totals: AdminTotals,
    pub this_week: AdminThisWeek,
    /// Every board that is not read, paused or declined.
    pub candidates: Vec<AdminCandidate>,
}

/// What a board said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestAnswer {
    Yes,
    No,
}

impl RequestAnswer {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "yes" => Some(Self::Yes),
            "no" => Some(Self::No),
            _ => None,
        }
    }
}

/// The email asking a board for permission, as a draft.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PermissionEmail {
    /// The board's contact when its check found one.
    pub to: Option<String>,
    pub subject: String,
    pub body: String,
    /// When the owner said they sent it.
    pub asked_at: Option<DateTime<Utc>>,
    /// When it was answered, and how.
    pub answered_at: Option<DateTime<Utc>>,
    pub answer: Option<RequestAnswer>,
}

/// What a request asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    /// Written permission from the board.
    #[default]
    Permission,
    /// The owner's one decision for a hiring system.
    Choice,
}

impl RequestKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Permission => "permission",
            Self::Choice => "choice",
        }
    }
}

/// The answer to record.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardAnswer {
    pub answer: RequestAnswer,
    /// Which request; permission when left out.
    #[serde(default)]
    pub kind: RequestKind,
    /// What they said, in up to 500 characters; kept with the answer.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct OpenRequest {
    board_id: Uuid,
    kind: String,
    asked_at: Option<DateTime<Utc>>,
}

/// A board owner, or 404: this page does not exist for anyone else.
async fn owner_or_missing(deps: &SourcesDeps, db: &PgPool, account: Uuid) -> Result<()> {
    if boards::is_owner(deps, db, account).await? {
        Ok(())
    } else {
        Err(Error::NotFound)
    }
}

fn has(blockers: &[Blocker], code: &str) -> bool {
    blockers.iter().any(|b| b.code == code)
}

type Classified = (Stage, Option<Need>, Option<String>, Option<DateTime<Utc>>);

/// Where a candidate stands and what it waits for.
fn classify(row: &BoardRow, open: &[&OpenRequest]) -> Classified {
    let blockers = boards::blockers(row);
    let asked = |kind: &str| {
        open.iter()
            .find(|r| r.kind == kind)
            .and_then(|r| r.asked_at)
    };
    let detail_of = |code: &str| {
        blockers
            .iter()
            .find(|b| b.code == code)
            .map(|b| b.detail.clone())
    };
    if open.iter().any(|r| r.kind == "choice") {
        return (Stage::Waiting, Some(Need::Choice), None, asked("choice"));
    }
    if has(&blockers, "terms_forbid") || has(&blockers, "needs_permission") {
        let detail = detail_of("terms_forbid").or_else(|| detail_of("needs_permission"));
        return (
            Stage::Waiting,
            Some(Need::Permission),
            detail,
            asked("permission"),
        );
    }
    if has(&blockers, "needs_key") {
        return (
            Stage::Waiting,
            Some(Need::Key),
            detail_of("needs_key"),
            None,
        );
    }
    if has(&blockers, "robots_disallowed") {
        return (
            Stage::NotAllowed,
            None,
            detail_of("robots_disallowed"),
            None,
        );
    }
    if blockers.is_empty() {
        return (Stage::Waiting, Some(Need::Approve), None, None);
    }
    (
        Stage::Checking,
        None,
        blockers.first().map(|b| b.detail.clone()),
        None,
    )
}

async fn open_requests(db: &PgPool) -> Result<Vec<OpenRequest>> {
    Ok(sqlx::query_as(
        "SELECT board_id, kind, asked_at FROM sources.board_requests WHERE answered_at IS NULL",
    )
    .fetch_all(db)
    .await?)
}

fn n(v: i64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// The admin overview of the job boards.
///
/// Totals (being read as job sites and companies, waiting for you, being
/// checked, not allowed, paused), what happened over the last 7 days from the
/// discovery runs and approvals the app recorded, and every board that is not
/// read, paused or declined with its stage and what it waits for. Board owners
/// only: anyone else gets 404.
#[utoipa::path(
    get,
    path = "/api/v1/boards/admin",
    tag = "sources",
    operation_id = "sources.boards_admin",
    security(("session" = [])),
    responses(
        (status = 200, description = "The overview.", body = AdminOverview),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: not a board owner", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn overview(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
) -> Result<Json<AdminOverview>> {
    owner_or_missing(&deps, &state.db, principal.account_id).await?;
    let since = state.clock.now() - TimeDelta::days(7);
    let rows = job_boards::all(&state.db).await?;
    let requests = open_requests(&state.db).await?;
    let mut by_board: HashMap<Uuid, Vec<&OpenRequest>> = HashMap::new();
    for request in &requests {
        by_board.entry(request.board_id).or_default().push(request);
    }
    let (mut sites, mut waiting, mut checking, mut not_allowed, mut paused) = (0, 0, 0, 0, 0);
    let mut started = 0;
    let mut candidates = Vec::new();
    for row in &rows {
        match row.status() {
            Status::Approved => {
                sites += 1;
                if row.approved_at.is_some_and(|at| at > since) {
                    started += 1;
                }
            }
            Status::Paused => paused += 1,
            Status::Declined => not_allowed += 1,
            Status::Candidate => {
                let open = by_board.get(&row.id).map(Vec::as_slice).unwrap_or(&[]);
                let (stage, need, detail, asked_at) = classify(row, open);
                match stage {
                    Stage::Waiting => waiting += 1,
                    Stage::Checking => checking += 1,
                    Stage::NotAllowed => not_allowed += 1,
                }
                candidates.push(AdminCandidate {
                    id: row.id,
                    name: row.name.clone(),
                    stage,
                    need,
                    detail,
                    asked_at,
                });
            }
        }
    }
    let companies: i64 = sqlx::query_scalar("SELECT count(*) FROM sources.boards")
        .fetch_one(&state.db)
        .await?;
    let (found, checked, rechecked): (i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(sum(found), 0)::bigint, COALESCE(sum(checked), 0)::bigint,
                COALESCE(sum(rechecked), 0)::bigint
           FROM sources.discovery_runs WHERE started_at > $1",
    )
    .bind(since)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(AdminOverview {
        totals: AdminTotals {
            reading_sites: sites,
            reading_companies: n(companies),
            waiting,
            checking,
            not_allowed,
            paused,
        },
        this_week: AdminThisWeek {
            found: n(found),
            checked: n(checked),
            rechecked: n(rechecked),
            started_reading: started,
        },
        candidates,
    }))
}

/// The board's contact, read straight from its row when the column is
/// there: the board finder (#75) adds `contact_email`, and this page reads it
/// without depending on that migration.
async fn contact_of(db: &PgPool, id: Uuid) -> Result<Option<String>> {
    let row: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT to_jsonb(b) FROM sources.job_boards b WHERE b.id = $1")
            .bind(id)
            .fetch_optional(db)
            .await?;
    Ok(row
        .and_then(|v| {
            v.get("contact_email")
                .and_then(|e| e.as_str().map(|e| e.trim().to_owned()))
        })
        .filter(|e| e.contains('@') && e.len() <= 254 && !e.contains(char::is_whitespace)))
}

/// The email's subject and body, filled with what the board is and what
/// Applicamp does.
pub fn draft_for(row: &BoardRow) -> (String, String) {
    let name = row.name.trim();
    let reads = row
        .config()
        .ok()
        .and_then(|c| row.kind().and_then(|k| c.start(k).cloned()))
        .and_then(|url| host_of(&url))
        .map_or_else(
            || format!("the jobs listed on {}", row.homepage),
            |host| format!("the list of jobs your site publishes at {host}"),
        );
    let subject = format!("May Applicamp show jobs from {name}?");
    let body = format!(
        "Hi {name} team,\n\n\
         I run Applicamp, a small app that helps people find jobs that fit them and get their \
         applications ready. We would like to show your jobs to the people who use it.\n\n\
         We would read {reads}. We would always name {name} as the source and link back to the \
         job on your site, read it at most once an hour, never repost your jobs anywhere else, \
         and drop jobs as soon as they close.\n\n\
         Your rules ask for permission first, so: may we? Happy to follow any conditions you \
         have.\n\n\
         Thanks,\n\
         Applicamp"
    );
    (subject, body)
}

async fn email_of(db: &PgPool, id: Uuid) -> Result<PermissionEmail> {
    let row = boards::fetch(db, id).await?;
    let to = contact_of(db, id).await?;
    let (subject, body) = draft_for(&row);
    let latest: Option<(Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<String>)> =
        sqlx::query_as(
            "SELECT asked_at, answered_at, answer FROM sources.board_requests
          WHERE board_id = $1 AND kind = 'permission'
          ORDER BY created_at DESC LIMIT 1",
        )
        .bind(id)
        .fetch_optional(db)
        .await?;
    let (asked_at, answered_at, answer) = latest.unwrap_or((None, None, None));
    Ok(PermissionEmail {
        to,
        subject,
        body,
        asked_at,
        answered_at,
        answer: answer.as_deref().and_then(RequestAnswer::parse),
    })
}

/// The permission email for a board.
///
/// Addressed to the board's contact when its check found one. Fills in the
/// board's name, what Applicamp would read, the link back, the reading
/// frequency (at most hourly), no reposting and dropping closed jobs. It is
/// signed "Applicamp": the app does not know the owner's name. The app never
/// sends it: the owner sends it from their own mailbox. Board owners only.
#[utoipa::path(
    get,
    path = "/api/v1/boards/{id}/permission-email",
    tag = "sources",
    operation_id = "sources.board_permission_email",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The board.")),
    responses(
        (status = 200, description = "The draft.", body = PermissionEmail),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such board, or not a board owner",
            body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn draft(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
) -> Result<Json<PermissionEmail>> {
    owner_or_missing(&deps, &state.db, principal.account_id).await?;
    Ok(Json(email_of(&state.db, id).await?))
}

/// Record that the permission email was sent.
///
/// Stores the date; asking twice keeps the first date. The app sends
/// nothing. Board owners only.
#[utoipa::path(
    post,
    path = "/api/v1/boards/{id}/permission/asked",
    tag = "sources",
    operation_id = "sources.board_permission_asked",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The board.")),
    responses(
        (status = 200, description = "Recorded.", body = PermissionEmail),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such board, or not a board owner",
            body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`not_waiting`: the board is read or declined",
            body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn asked(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
) -> Result<Json<PermissionEmail>> {
    let account = principal.account_id;
    owner_or_missing(&deps, &state.db, account).await?;
    let row = boards::fetch(&state.db, id).await?;
    if !matches!(row.status(), Status::Candidate | Status::Paused) {
        return Err(Error::Conflict(
            "not_waiting",
            "Only a board waiting for you can be asked for permission.".to_owned(),
        ));
    }
    let now = state.clock.now();
    sqlx::query(
        "INSERT INTO sources.board_requests
            (id, board_id, kind, asked_at, asked_by, created_at, updated_at)
         VALUES ($1, $2, 'permission', $3, $4, $3, $3)
         ON CONFLICT (board_id, kind) WHERE answered_at IS NULL
         DO UPDATE SET asked_at = COALESCE(sources.board_requests.asked_at, $3),
                       asked_by = COALESCE(sources.board_requests.asked_by, $4),
                       updated_at = $3",
    )
    .bind(Uuid::now_v7())
    .bind(id)
    .bind(now)
    .bind(account)
    .execute(&state.db)
    .await?;
    tracing::info!(board = %row.slug, "job board permission asked");
    Ok(Json(email_of(&state.db, id).await?))
}

/// Record a board's answer.
///
/// Only after it was asked. "yes" approves the board through the approve
/// path (the note "They said yes on <date>" is kept): the terms and
/// permission blockers are cleared, but robots rules that forbid the feed and
/// a missing key still refuse it (409, nothing recorded). "no" declines it
/// through the decline path with the reason "They said no on <date>". Board
/// owners only.
#[utoipa::path(
    post,
    path = "/api/v1/boards/{id}/permission/answer",
    tag = "sources",
    operation_id = "sources.board_permission_answer",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The board.")),
    request_body = BoardAnswer,
    responses(
        (status = 200, description = "Recorded: the board, approved or declined.",
            body = BoardDetail),
        (status = 400, description = "`invalid_request`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such board, or not a board owner",
            body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`not_asked`: nothing was asked yet; `not_waiting`; or the \
            first blocker that remains when the answer is yes", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn answer(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
    body: std::result::Result<Json<BoardAnswer>, JsonRejection>,
) -> Result<Json<BoardDetail>> {
    let account = principal.account_id;
    owner_or_missing(&deps, &state.db, account).await?;
    let Json(body) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());
    if note.is_some_and(|note| note.chars().count() > 400) {
        return Err(Error::BadRequest(
            "invalid_request",
            "Keep the note to 400 characters.".to_owned(),
        ));
    }
    let row = boards::fetch(&state.db, id).await?;
    let request: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM sources.board_requests
          WHERE board_id = $1 AND kind = $2 AND answered_at IS NULL AND asked_at IS NOT NULL",
    )
    .bind(id)
    .bind(body.kind.as_str())
    .fetch_optional(&state.db)
    .await?;
    let Some(request) = request else {
        return Err(Error::Conflict(
            "not_asked",
            "Say you sent it before recording the answer.".to_owned(),
        ));
    };
    let now = state.clock.now();
    let date = now.format("%-d %b %Y").to_string();
    let said = if body.answer == RequestAnswer::Yes {
        "yes"
    } else {
        "no"
    };
    let recorded = match note {
        Some(note) => format!("They said {said} on {date}: {note}"),
        None => format!("They said {said} on {date}"),
    };
    let mut tx = state.db.begin().await?;
    if body.answer == RequestAnswer::Yes {
        if !matches!(row.status(), Status::Candidate | Status::Paused) {
            return Err(Error::Conflict(
                "not_waiting",
                "Only a board waiting for you, or paused, can be approved.".to_owned(),
            ));
        }
        let kept: Vec<Blocker> = row
            .blockers()
            .into_iter()
            .filter(|b| !matches!(b.code.as_str(), "terms_forbid" | "needs_permission"))
            .collect();
        let cleared = BoardRow {
            blockers: sqlx::types::Json(serde_json::to_value(&kept).map_err(Error::internal)?),
            ..row.clone()
        };
        if let Some(first) = boards::blockers(&cleared).into_iter().next() {
            return Err(Error::Conflict(
                boards::conflict_code(&first.code),
                first.detail,
            ));
        }
        sqlx::query("UPDATE sources.job_boards SET blockers = $2, updated_at = $3 WHERE id = $1")
            .bind(id)
            .bind(sqlx::types::Json(&kept))
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "UPDATE sources.board_requests
            SET answered_at = $2, answer = $3, note = $4, updated_at = $2
          WHERE id = $1",
    )
    .bind(request)
    .bind(now)
    .bind(body.answer.as_str())
    .bind(&recorded)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(board = %row.slug, answer = body.answer.as_str(), "job board answer recorded");
    match body.answer {
        RequestAnswer::Yes => {
            boards::approve(State(state), SignedIn(principal), Extension(deps), Path(id)).await
        }
        RequestAnswer::No => {
            boards::decline(
                State(state),
                SignedIn(principal),
                Extension(deps),
                Path(id),
                Ok(Json(DeclineBoard { reason: recorded })),
            )
            .await
        }
    }
}
