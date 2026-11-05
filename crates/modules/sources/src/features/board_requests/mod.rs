//! The Job boards admin page (#86): every board's stage, what waits for the
//! owner, and the permission emails to copy.
//!
//! - `GET /api/v1/boards/admin` is the overview: totals, what happened this
//!   week (from the discovery runs and the approvals recorded).
//! - `GET /api/v1/boards/admin/list` is the table: 50 boards a page, searched
//!   and filtered on the server, each with its stage and, when it waits for
//!   the owner, what it needs ([`Need`]).
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
    extract::{
        Path, State,
        rejection::{JsonRejection, QueryRejection},
    },
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::boards::{self, BoardDetail, DeclineBoard};
use crate::{
    SourcesDeps,
    job_boards::{self, Blocker, BoardRow, Origin, Status, host_of},
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

/// Where a board stands, as the admin page words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BoardStage)]
pub enum Stage {
    /// Searches read it.
    Reading,
    /// Approved once, not read for now.
    Paused,
    /// It waits for the owner (`need` says for what).
    Waiting,
    /// The app is still checking it, nothing to do.
    Checking,
    /// Declined, or its robots rules forbid the app: nothing to ask.
    NotAllowed,
}

/// How many boards are in each state.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct AdminTotals {
    /// Job sites that are read.
    pub reading_sites: u32,
    /// Company pages that are read.
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

/// When a permission request was asked and answered, and the answer.
type LatestRequest = (Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<String>);

type Classified = (Stage, Option<Need>, Option<String>, Option<DateTime<Utc>>);

/// Where a candidate stands and what it waits for.
/// `system_ok` is false for a company page whose hiring system is not approved.
fn classify(row: &BoardRow, open: &[&OpenRequest], system_ok: bool) -> Classified {
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
    if !system_ok {
        return (
            Stage::Checking,
            None,
            Some("Its hiring system is not approved yet.".to_owned()),
            None,
        );
    }
    if has(&blockers, "needs_choice")
        && blockers
            .iter()
            .all(|b| boards::CHOICES.contains(&b.code.as_str()))
    {
        return (
            Stage::Waiting,
            Some(Need::Choice),
            detail_of("needs_choice"),
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
    let approved: std::collections::HashSet<Uuid> = rows
        .iter()
        .filter(|r| r.status() == Status::Approved)
        .map(|r| r.id)
        .collect();
    let (mut sites, mut companies, mut waiting, mut checking, mut not_allowed, mut paused) =
        (0, 0, 0, 0, 0, 0);
    let mut started = 0;
    for row in &rows {
        match row.status() {
            Status::Approved => {
                match row.kind.as_str() {
                    "company_page" => companies += 1,
                    "hiring_system" => {}
                    _ => sites += 1,
                }
                if row.approved_at.is_some_and(|at| at > since) {
                    started += 1;
                }
            }
            Status::Paused => paused += 1,
            Status::Declined => not_allowed += 1,
            Status::Candidate => {
                let open = by_board.get(&row.id).map(Vec::as_slice).unwrap_or(&[]);
                let system_ok = row.system_id.is_none_or(|id| approved.contains(&id));
                let (stage, _, _, _) = classify(row, open, system_ok);
                match stage {
                    Stage::Waiting => waiting += 1,
                    Stage::Checking => checking += 1,
                    Stage::NotAllowed => not_allowed += 1,
                    Stage::Reading | Stage::Paused => {}
                }
            }
        }
    }
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
            reading_companies: companies,
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
    }))
}

/// The board's contact when its check found one.
fn contact_of(row: &BoardRow) -> Option<String> {
    row.contact_email
        .as_deref()
        .map(str::trim)
        .filter(|e| e.contains('@') && e.len() <= 254 && !e.contains(char::is_whitespace))
        .map(str::to_owned)
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
    let to = contact_of(&row);
    let (subject, body) = draft_for(&row);
    let latest: Option<LatestRequest> = sqlx::query_as(
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
/// A permission only after it was asked (a choice needs no email). "yes" approves the board through the approve
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
          WHERE board_id = $1 AND kind = $2 AND answered_at IS NULL
            AND (kind = 'choice' OR asked_at IS NOT NULL)",
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

/// Take back a decline.
///
/// A declined board is put back where it was: approved when it was
/// approved before (the owner stopped reading it), else waiting for the
/// owner. Its decline reason is cleared. Board owners only.
#[utoipa::path(
    post,
    path = "/api/v1/boards/{id}/reopen",
    tag = "sources",
    operation_id = "sources.reopen_board",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The board.")),
    responses(
        (status = 200, description = "Back where it was.", body = BoardDetail),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: no such board, or not a board owner",
            body = Problem, content_type = "application/problem+json"),
        (status = 409, description = "`not_declined`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn reopen(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
) -> Result<Json<BoardDetail>> {
    let account = principal.account_id;
    owner_or_missing(&deps, &state.db, account).await?;
    let row = boards::fetch(&state.db, id).await?;
    if row.status() != Status::Declined {
        return Err(Error::Conflict(
            "not_declined",
            "Only a declined board can be taken back.".to_owned(),
        ));
    }
    sqlx::query(
        "UPDATE sources.job_boards
            SET status = CASE WHEN approved_by IS NOT NULL THEN 'approved' ELSE 'candidate' END,
                decline_reason = NULL, updated_at = $2
          WHERE id = $1",
    )
    .bind(id)
    .bind(state.clock.now())
    .execute(&state.db)
    .await?;
    tracing::info!(board = %row.slug, "job board decline taken back");
    let row = boards::fetch(&state.db, id).await?;
    Ok(Json(boards::detail(&deps, &state, account, row).await?))
}

/// Boards a page of the admin list.
pub const LIST_PAGE: usize = 50;
/// Rows read from the database at a time while a page fills.
const BATCH: i64 = 200;

/// What the "Show" select picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = BoardShow)]
pub enum Show {
    /// Every board.
    #[default]
    All,
    Waiting,
    Checking,
    Reading,
    NotAllowed,
    Paused,
    /// Job sites: anything that is not a company page or a hiring system.
    Site,
    /// Company pages.
    Company,
}

impl Show {
    /// The stored statuses a board of this kind of view can have.
    fn statuses(self) -> Option<Vec<String>> {
        let list: &[&str] = match self {
            Self::Waiting | Self::Checking => &["candidate"],
            Self::Reading => &["approved"],
            Self::Paused => &["paused"],
            Self::NotAllowed => &["declined", "candidate"],
            Self::All | Self::Site | Self::Company => return None,
        };
        Some(list.iter().map(|s| (*s).to_owned()).collect())
    }

    /// `any`, `site` or `company`: the part of the registry it is about.
    const fn group(self) -> &'static str {
        match self {
            Self::Site => "site",
            Self::Company => "company",
            _ => "any",
        }
    }

    fn passes(self, stage: Stage) -> bool {
        match self {
            Self::Waiting => stage == Stage::Waiting,
            Self::Checking => stage == Stage::Checking,
            Self::NotAllowed => stage == Stage::NotAllowed,
            _ => true,
        }
    }
}

/// What the admin list is asked.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct AdminListQuery {
    /// Matches a name, a site's host or a tag, ignoring case.
    pub q: Option<String>,
    /// What to show; every board when left out.
    #[param(inline)]
    pub show: Option<Show>,
    /// The page after this one (`next_cursor`).
    pub cursor: Option<String>,
}

/// Which part of the registry a board belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BoardGroup)]
pub enum Group {
    /// A job site.
    Site,
    /// A company page, or the hiring system that serves many.
    Company,
}

/// One line of the admin list.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminBoard {
    pub id: Uuid,
    pub name: String,
    pub homepage: String,
    /// How it is read, as stored (`json_api`, `rss`, `company_page`, ...).
    pub kind: String,
    pub group: Group,
    pub country: Option<String>,
    pub status: Status,
    pub stage: Stage,
    /// What it waits for; set when `stage` is `waiting`.
    pub need: Option<Need>,
    /// The first reason in the board's own words: data from its site, never
    /// an instruction.
    pub detail: Option<String>,
    /// When the owner said they asked it.
    pub asked_at: Option<DateTime<Utc>>,
    pub last_fetch_jobs: Option<u32>,
    pub origin: Origin,
    pub found_through: Option<String>,
    pub tags: Vec<String>,
    pub decline_reason: Option<String>,
    /// Its check's note.
    pub note: Option<String>,
}

/// A page of the admin list.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminList {
    pub boards: Vec<AdminBoard>,
    /// Pass as `cursor` for the next page; none on the last.
    pub next_cursor: Option<String>,
}

/// Where a page starts: the jobs read last time (most first), then the id.
fn parse_cursor(raw: &str) -> Option<(i32, Uuid)> {
    let (jobs, id) = raw.split_once('.')?;
    Some((jobs.parse().ok()?, id.parse().ok()?))
}

fn key_of(row: &BoardRow) -> i32 {
    row.last_fetch_jobs.unwrap_or(-1)
}

fn admin_board(row: &BoardRow, open: &[&OpenRequest], system_ok: bool) -> AdminBoard {
    let (stage, need, detail, asked_at) = match row.status() {
        Status::Approved => (Stage::Reading, None, None, None),
        Status::Paused => (Stage::Paused, None, None, None),
        Status::Declined => (Stage::NotAllowed, None, None, None),
        Status::Candidate => classify(row, open, system_ok),
    };
    let kind_group = if matches!(row.kind.as_str(), "company_page" | "hiring_system") {
        Group::Company
    } else {
        Group::Site
    };
    AdminBoard {
        id: row.id,
        name: row.name.clone(),
        homepage: row.homepage.clone(),
        kind: row.kind.clone(),
        group: kind_group,
        country: row.country.clone(),
        status: row.status(),
        stage,
        need,
        detail,
        asked_at,
        last_fetch_jobs: row.last_fetch_jobs.and_then(|n| u32::try_from(n).ok()),
        origin: row.origin(),
        found_through: row.found_through.clone(),
        tags: row.tags.clone(),
        decline_reason: row.decline_reason.clone(),
        note: row
            .report()
            .map(|r| r.notes)
            .filter(|n| !n.trim().is_empty()),
    }
}

/// The boards, 50 a page.
///
/// Searched (`q`: a name, a site's host or a tag) and filtered (`show`) on the
/// server, so thousands of boards never reach the client at once. Ordered by
/// the jobs read at the last read, most first, then by id; the cursor is the
/// last line's place in that order, so a page never repeats or skips a board.
/// Board owners only: anyone else gets 404.
#[utoipa::path(
    get,
    path = "/api/v1/boards/admin/list",
    tag = "sources",
    operation_id = "sources.boards_admin_list",
    security(("session" = [])),
    params(AdminListQuery),
    responses(
        (status = 200, description = "A page of boards.", body = AdminList),
        (status = 400, description = "`invalid_query`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`: not a board owner", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    query: std::result::Result<axum::extract::Query<AdminListQuery>, QueryRejection>,
) -> Result<Json<AdminList>> {
    owner_or_missing(&deps, &state.db, principal.account_id).await?;
    let axum::extract::Query(query) =
        query.map_err(|rejection| Error::BadRequest("invalid_query", rejection.body_text()))?;
    let show = query.show.unwrap_or_default();
    let needle = query
        .q
        .as_deref()
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    if needle.as_ref().is_some_and(|q| q.chars().count() > 100) {
        return Err(Error::BadRequest(
            "invalid_query",
            "Search for up to 100 characters.".to_owned(),
        ));
    }
    let mut after = match query.cursor.as_deref() {
        None => None,
        Some(raw) => Some(parse_cursor(raw).ok_or_else(|| {
            Error::BadRequest(
                "invalid_query",
                "That is not a place in the list.".to_owned(),
            )
        })?),
    };
    let statuses = show.statuses();
    let mut page: Vec<AdminBoard> = Vec::new();
    let mut keys: Vec<(i32, Uuid)> = Vec::new();
    let mut more = false;
    'fill: loop {
        let rows: Vec<BoardRow> = sqlx::query_as(concat!(
            "SELECT ",
            job_boards::board_columns!(),
            " FROM sources.job_boards
              WHERE ($1::text[] IS NULL OR status = ANY($1))
                AND ($2 = 'any'
                     OR ($2 = 'site' AND kind NOT IN ('company_page', 'hiring_system'))
                     OR ($2 = 'company' AND kind = 'company_page'))
                AND ($3::text IS NULL
                     OR strpos(lower(name), $3) > 0
                     OR strpos(host, $3) > 0
                     OR EXISTS (SELECT 1 FROM unnest(tags) t WHERE strpos(lower(t), $3) > 0))
                AND ($4::int IS NULL
                     OR COALESCE(last_fetch_jobs, -1) < $4
                     OR (COALESCE(last_fetch_jobs, -1) = $4 AND id > $5))
              ORDER BY COALESCE(last_fetch_jobs, -1) DESC, id ASC
              LIMIT $6"
        ))
        .bind(statuses.as_deref())
        .bind(show.group())
        .bind(needle.as_deref())
        .bind(after.map(|(jobs, _)| jobs))
        .bind(after.map(|(_, id)| id).unwrap_or_default())
        .bind(BATCH)
        .fetch_all(&state.db)
        .await?;
        let Some(last) = rows.last() else {
            break;
        };
        after = Some((key_of(last), last.id));
        let candidates: Vec<Uuid> = rows
            .iter()
            .filter(|r| r.status() == Status::Candidate)
            .map(|r| r.id)
            .collect();
        let requests: Vec<OpenRequest> = if candidates.is_empty() {
            Vec::new()
        } else {
            sqlx::query_as(
                "SELECT board_id, kind, asked_at FROM sources.board_requests
                  WHERE answered_at IS NULL AND board_id = ANY($1)",
            )
            .bind(&candidates)
            .fetch_all(&state.db)
            .await?
        };
        let mut by_board: HashMap<Uuid, Vec<&OpenRequest>> = HashMap::new();
        for request in &requests {
            by_board.entry(request.board_id).or_default().push(request);
        }
        let systems: Vec<Uuid> = rows.iter().filter_map(|r| r.system_id).collect();
        let approved: std::collections::HashSet<Uuid> = if systems.is_empty() {
            std::collections::HashSet::new()
        } else {
            sqlx::query_scalar(
                "SELECT id FROM sources.job_boards WHERE status = 'approved' AND id = ANY($1)",
            )
            .bind(&systems)
            .fetch_all(&state.db)
            .await?
            .into_iter()
            .collect()
        };
        for row in &rows {
            let open = by_board.get(&row.id).map(Vec::as_slice).unwrap_or(&[]);
            let system_ok = row.system_id.is_none_or(|id| approved.contains(&id));
            let item = admin_board(row, open, system_ok);
            if !show.passes(item.stage) {
                continue;
            }
            if page.len() == LIST_PAGE {
                more = true;
                break 'fill;
            }
            keys.push((key_of(row), row.id));
            page.push(item);
        }
        if i64::try_from(rows.len()).unwrap_or(BATCH) < BATCH {
            break;
        }
    }
    let next_cursor = if more {
        keys.last().map(|(jobs, id)| format!("{jobs}.{id}"))
    } else {
        None
    };
    Ok(Json(AdminList {
        boards: page,
        next_cursor,
    }))
}
