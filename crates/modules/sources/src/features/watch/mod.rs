//! Company watchers (#20): companies whose jobs keep fitting a person are
//! suggested, the person says Watch, the company's own job board is found,
//! and every day it is read for new jobs that fit.
//!
//! - `GET /api/v1/companies` lists the suggestions (worked out from the
//!   person's matches, [`suggest`], again when older than an hour) and the
//!   companies they watch.
//! - `POST /api/v1/companies/{id}/watch` watches a suggestion;
//!   `POST /api/v1/companies/{id}/not-interested` hides it for good;
//!   `POST /api/v1/companies/{id}/stop` stops watching.
//! - `POST /api/v1/companies` watches a company by a pasted careers page (or
//!   a link to its board); `POST /api/v1/companies/{id}/careers` gives a
//!   watch with no board found a careers page to look at.
//! - Looking for a board runs in the background (`sources.find_board`,
//!   [`detect`]): a posting's own link, else the company's site, robots.txt
//!   respected; the board found is read once to confirm it lists jobs and
//!   its postings are noted as seen, so the watch is "from today".
//! - The daily check ([`check`]) runs inside the search (see its docs).
//!
//! Up to [`MAX_WATCHED`] companies are watched at once. Nothing is watched
//! without a yes. Never LinkedIn, Indeed or Glassdoor (ADR 0006).

pub mod check;
pub mod detect;
pub mod store;
pub mod suggest;

#[cfg(test)]
mod tests;

use applicamp_kernel::{
    AppState, Error, JobContext, JobError, Problem, Result, auth::SignedIn, jobs,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use url::Url;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{SourcesDeps, net};

pub use detect::BoardKind;
use detect::{AtsBoard, Start, Who};
pub use suggest::{Reason, ReasonKind, ReasonSource, SourceKind};

/// Companies one person may watch at once.
pub const MAX_WATCHED: i64 = 20;

/// A company worth watching.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Suggestion {
    /// The company.
    pub id: Uuid,
    pub name: String,
    /// Its own site.
    pub website: Option<String>,
    /// Why, each traced to a posting (or a fact read on its site).
    pub reasons: Vec<Reason>,
}

/// A company the person watches.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Watching {
    /// The company.
    pub id: Uuid,
    pub name: String,
    pub website: Option<String>,
    /// The page that lists its jobs.
    pub careers_url: Option<String>,
    /// How its jobs are read; `none` when no board was found.
    pub board: BoardKind,
    /// A board is being looked for.
    pub finding: bool,
    /// No board was found: the person can paste the careers page.
    pub needs_board: bool,
    /// Jobs open on the board when it was last read.
    pub open_jobs: Option<u32>,
    pub watching_since: Option<DateTime<Utc>>,
    /// When its board was last read (or looked for).
    pub last_checked_at: Option<DateTime<Utc>>,
    /// Why the last check failed (`http_503`, `unreachable`, `no_board`…).
    pub last_error: Option<String>,
}

/// The suggestions and the watched companies.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CompanyList {
    pub suggested: Vec<Suggestion>,
    pub watching: Vec<Watching>,
    /// Companies one person may watch at once.
    pub limit: u32,
}

/// A careers page (or a link to a job board) someone pasted.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CareersPage {
    /// Its address.
    pub url: String,
}

async fn list_of(db: &PgPool, account: Uuid, now: DateTime<Utc>) -> Result<CompanyList> {
    let mut suggested = Vec::new();
    let mut watching = Vec::new();
    for row in store::shown(db, account).await? {
        if row.state == "suggested" {
            suggested.push(Suggestion {
                id: row.company,
                name: row.name,
                website: row.website,
                reasons: row.reasons.0,
            });
        } else {
            let finding = row.finding(now);
            let board = row.kind();
            watching.push(Watching {
                id: row.company,
                name: row.name.clone(),
                website: row.website.clone(),
                careers_url: row.careers_url.clone(),
                board,
                finding,
                needs_board: board == BoardKind::None && !finding,
                open_jobs: row.open_jobs.and_then(|n| u32::try_from(n).ok()),
                watching_since: row.watching_since,
                last_checked_at: row.last_checked_at,
                last_error: row.last_error.clone(),
            });
        }
    }
    Ok(CompanyList {
        suggested,
        watching,
        limit: u32::try_from(MAX_WATCHED).unwrap_or(20),
    })
}

/// The companies.
///
/// The companies whose jobs keep fitting the signed-in person (at least two
/// matches, or one they shortlisted, in the last eight weeks), each with
/// reasons traced to the postings they come from, and the companies they
/// watch, with their job board, open jobs and last check. Suggestions are
/// worked out from the person's matches, with no model.
#[utoipa::path(
    get,
    path = "/api/v1/companies",
    tag = "sources",
    operation_id = "sources.companies",
    security(("session" = [])),
    responses(
        (status = 200, description = "The suggestions and the watched companies.", body = CompanyList),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
) -> Result<Json<CompanyList>> {
    let account = principal.account_id;
    let now = state.clock.now();
    suggest::refresh_if_stale(&deps, &state.db, account, now).await?;
    Ok(Json(list_of(&state.db, account, now).await?))
}

/// Room for `account` to watch `company` (already watching it counts).
async fn ensure_room(db: &PgPool, account: Uuid, company: Uuid) -> Result<()> {
    let already = store::of_company(db, account, company)
        .await?
        .is_some_and(|row| row.state == "watching");
    if !already && store::watching(db, account).await? >= MAX_WATCHED {
        return Err(Error::Conflict(
            "watch_limit_reached",
            format!(
                "You can watch up to {MAX_WATCHED} companies. Stop watching one to add another."
            ),
        ));
    }
    Ok(())
}

/// Start watching `company` for `account` and look for its board in the
/// background (`careers`: a page to start from).
async fn start(
    db: &PgPool,
    account: Uuid,
    company: Uuid,
    careers: Option<String>,
    now: DateTime<Utc>,
) -> Result<()> {
    ensure_room(db, account, company).await?;
    let mut tx = db.begin().await?;
    let watch = store::start_watching(&mut tx, account, company, now).await?;
    jobs::enqueue(&mut tx, &FindBoard { watch, careers }).await?;
    tx.commit().await?;
    Ok(())
}

/// Watch a suggested company.
///
/// Moves it to the watched companies and looks for its job board in the
/// background: the board a posting came from, else the careers page on its
/// own site (robots.txt respected). At most 20 watched companies (409
/// `watch_limit_reached`).
#[utoipa::path(
    post,
    path = "/api/v1/companies/{id}/watch",
    tag = "sources",
    operation_id = "sources.watch_company",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The company.")),
    responses(
        (status = 202, description = "Watching; its board is being looked for.", body = CompanyList),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`watch_limit_reached`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn watch(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<CompanyList>)> {
    let account = principal.account_id;
    let now = state.clock.now();
    let row = store::of_company(&state.db, account, id)
        .await?
        .filter(|row| row.state != "not_interested")
        .ok_or(Error::NotFound)?;
    start(&state.db, account, row.company, None, now).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(list_of(&state.db, account, now).await?),
    ))
}

/// Not interested in a company.
///
/// Hides it for good: it is never suggested again.
#[utoipa::path(
    post,
    path = "/api/v1/companies/{id}/not-interested",
    tag = "sources",
    operation_id = "sources.dismiss_company",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The company.")),
    responses(
        (status = 200, description = "Hidden.", body = CompanyList),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn not_interested(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<Json<CompanyList>> {
    let account = principal.account_id;
    let now = state.clock.now();
    if !store::set_state(&state.db, account, id, "not_interested", now).await? {
        return Err(Error::NotFound);
    }
    Ok(Json(list_of(&state.db, account, now).await?))
}

/// Stop watching a company.
///
/// Its job board is not read any more. It is not suggested again; watch it
/// again by pasting its careers page.
#[utoipa::path(
    post,
    path = "/api/v1/companies/{id}/stop",
    tag = "sources",
    operation_id = "sources.stop_watching_company",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The company.")),
    responses(
        (status = 200, description = "Stopped.", body = CompanyList),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn stop(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<Json<CompanyList>> {
    let account = principal.account_id;
    let now = state.clock.now();
    let watching = store::of_company(&state.db, account, id)
        .await?
        .is_some_and(|row| row.state == "watching");
    if !watching || !store::set_state(&state.db, account, id, "stopped", now).await? {
        return Err(Error::NotFound);
    }
    Ok(Json(list_of(&state.db, account, now).await?))
}

fn invalid_url() -> Error {
    Error::BadRequest(
        "invalid_url",
        "Paste the company's careers page, starting with https://.".to_owned(),
    )
}

/// A pasted address as a public web address.
fn parse_pasted(deps: &SourcesDeps, raw: &str) -> Result<Url> {
    let raw = raw.trim();
    if raw.is_empty() || raw.chars().count() > 500 {
        return Err(invalid_url());
    }
    let with_scheme = if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("https://{raw}")
    };
    let mut url = Url::parse(&with_scheme).map_err(|_| invalid_url())?;
    net::check(&url, deps.config.allow_private_hosts()).map_err(|_| invalid_url())?;
    url.set_fragment(None);
    Ok(url)
}

/// Watch a company yourself.
///
/// Paste the company's careers page (or a link to its Greenhouse, Lever or
/// Ashby board): the company is added as one you watch, and its job board is
/// looked for the same way as for a suggestion. The page is read only where
/// its robots.txt allows. 409 `watch_limit_reached` past 20.
#[utoipa::path(
    post,
    path = "/api/v1/companies",
    tag = "sources",
    operation_id = "sources.add_company",
    security(("session" = [])),
    request_body = CareersPage,
    responses(
        (status = 202, description = "Watching; its board is being looked for.", body = CompanyList),
        (status = 400, description = "`invalid_url`: a public https address", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`watch_limit_reached`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn add(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    body: std::result::Result<Json<CareersPage>, JsonRejection>,
) -> Result<(StatusCode, Json<CompanyList>)> {
    let account = principal.account_id;
    let now = state.clock.now();
    let Json(body) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    let url = parse_pasted(&deps, &body.url)?;
    let mut conn = state.db.acquire().await?;
    let ats = detect::ats_in(url.as_str()).into_iter().next();
    let id = match &ats {
        // A link to a board: the board is the company.
        Some(AtsBoard { kind, token, eu }) => {
            let key = format!("ats:{}:{}", kind.as_str(), token.to_ascii_lowercase());
            store::ensure_company(
                &mut conn,
                &store::NewCompany {
                    key: &key,
                    name: &detect::name_of_host(&format!("{token}.board")),
                    website: None,
                    careers_url: Some(url.as_str()),
                    origin: "pasted",
                    hint: Some((*kind, token.as_str(), *eu)),
                },
                now,
            )
            .await?
        }
        None => {
            let key = detect::site_key(&url).ok_or_else(invalid_url)?;
            let host = crate::job_boards::host_of(&url).ok_or_else(invalid_url)?;
            if detect::is_not_a_company_site(&host) {
                return Err(invalid_url());
            }
            let mut origin = url.clone();
            origin.set_path("/");
            origin.set_query(None);
            store::ensure_company(
                &mut conn,
                &store::NewCompany {
                    key: &key,
                    name: &detect::name_of_host(&host),
                    website: Some(origin.as_str()),
                    careers_url: None,
                    origin: "pasted",
                    hint: None,
                },
                now,
            )
            .await?
        }
    };
    drop(conn);
    start(&state.db, account, id, Some(url.to_string()), now).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(list_of(&state.db, account, now).await?),
    ))
}

/// Give a watched company a careers page.
///
/// For a watch whose job board could not be found: the page is looked at the
/// same way (a Greenhouse, Lever or Ashby board, or schema.org `JobPosting`
/// data), where its robots.txt allows.
#[utoipa::path(
    post,
    path = "/api/v1/companies/{id}/careers",
    tag = "sources",
    operation_id = "sources.company_careers",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The company.")),
    request_body = CareersPage,
    responses(
        (status = 202, description = "Looking at the page.", body = CompanyList),
        (status = 400, description = "`invalid_url`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn careers(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    Path(id): Path<Uuid>,
    body: std::result::Result<Json<CareersPage>, JsonRejection>,
) -> Result<(StatusCode, Json<CompanyList>)> {
    let account = principal.account_id;
    let now = state.clock.now();
    let Json(body) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    let url = parse_pasted(&deps, &body.url)?;
    let row = store::of_company(&state.db, account, id)
        .await?
        .filter(|row| row.state == "watching")
        .ok_or(Error::NotFound)?;
    let mut tx = state.db.begin().await?;
    store::finding(&mut tx, row.watch, now).await?;
    jobs::enqueue(
        &mut tx,
        &FindBoard {
            watch: row.watch,
            careers: Some(url.to_string()),
        },
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(list_of(&state.db, account, now).await?),
    ))
}

/// Looks for a watched company's job board and reads it once. Enqueued by
/// Watch, a pasted careers page and a page given to a watch with no board.
#[derive(Debug, Serialize, Deserialize)]
pub struct FindBoard {
    /// The watch.
    pub watch: Uuid,
    /// A careers page to start from, when someone pasted one.
    pub careers: Option<String>,
}

impl applicamp_kernel::Job for FindBoard {
    const NAME: &'static str = "sources.find_board";
    const QUEUE: &'static str = "boards";
    // Never retried: the person can paste a careers page.
    const MAX_ATTEMPTS: u32 = 1;
    const LEASE: std::time::Duration = std::time::Duration::from_secs(5 * 60);

    async fn run(self, ctx: JobContext) -> std::result::Result<(), JobError> {
        let deps = ctx
            .deps::<SourcesDeps>()
            .ok_or_else(|| JobError::Permanent("the sources module is not composed".into()))?;
        find_board(deps, &ctx.db, self.watch, self.careers.as_deref())
            .await
            .map_err(JobError::from)
    }
}

/// Look for the board of `watch`, and keep what was found.
pub(crate) async fn find_board(
    deps: &SourcesDeps,
    db: &PgPool,
    watch: Uuid,
    careers: Option<&str>,
) -> Result<()> {
    let Some(row) = store::by_watch(db, watch)
        .await?
        .filter(|row| row.state == "watching")
    else {
        return Ok(());
    };
    let now = deps.clock.now();
    let who = Who {
        id: row.company,
        name: row.name.clone(),
    };
    let pasted = careers.and_then(|c| Url::parse(c).ok());
    let known = (row.kind() != BoardKind::None)
        .then(|| {
            row.board_token.clone().map(|token| AtsBoard {
                kind: row.kind(),
                token,
                eu: row.board_eu,
            })
        })
        .flatten()
        .filter(|ats| ats.kind.source().is_some());
    let site = row.website.as_deref().and_then(|w| Url::parse(w).ok());
    let page = row.careers_url.as_deref().and_then(|w| Url::parse(w).ok());
    // A page someone pasted first; else a board a posting came from; else
    // the careers page found before; else the company's site.
    let mut starts: Vec<Start> = Vec::new();
    if let Some(url) = pasted {
        starts.push(Start::Careers(url));
    } else if let Some(ats) = known {
        starts.push(Start::Board(ats));
    } else if row.kind() == BoardKind::Jsonld
        && let Some(url) = page
    {
        starts.push(Start::Careers(url));
    }
    if let Some(url) = site {
        starts.push(Start::Site(url));
    }
    let mut outcome = detect::Outcome::Nothing(detect::Why::NoBoard);
    for start in starts {
        outcome = detect::find(deps, db, &who, start).await;
        if matches!(outcome, detect::Outcome::Found(_)) {
            break;
        }
    }
    match outcome {
        detect::Outcome::Found(detected) => {
            let seen: Vec<(String, String)> =
                detected.jobs.iter().map(crate::job::Job::key).collect();
            store::board_found(db, &row, &detected, &seen, now).await?;
            tracing::info!(
                kind = detected.kind.as_str(),
                jobs = seen.len(),
                "a company board found"
            );
        }
        detect::Outcome::Nothing(why) => {
            store::board_not_found(db, &row, why.as_str(), now).await?;
            tracing::info!(why = why.as_str(), "no company board found");
        }
    }
    Ok(())
}
