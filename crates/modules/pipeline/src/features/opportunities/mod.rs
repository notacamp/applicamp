//! `/api/v1/opportunities`: the pipeline, its opportunities, adding one by
//! link, and moving one along (stage, next step, notes, Not for me).
//!
//! Opportunities that fail a hard requirement (found by a search before the
//! rules, checked once after) are hidden: out of the list, never deleted,
//! listed on their own with `?hidden=true` with the reason.

#[cfg(test)]
mod tests;

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use url::Url;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    PipelineDeps,
    store::{self, MAX_NOTES, MAX_SHORT, MAX_URL, NewRow, tidy},
};

/// Where an opportunity is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityStage {
    Found,
    Shortlisted,
    Applying,
    Applied,
    Talking,
    Interview,
    Offer,
    Closed,
}

impl OpportunityStage {
    /// Every stage, in order.
    pub const ALL: [Self; 8] = [
        Self::Found,
        Self::Shortlisted,
        Self::Applying,
        Self::Applied,
        Self::Talking,
        Self::Interview,
        Self::Offer,
        Self::Closed,
    ];

    /// As stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Found => "found",
            Self::Shortlisted => "shortlisted",
            Self::Applying => "applying",
            Self::Applied => "applied",
            Self::Talking => "talking",
            Self::Interview => "interview",
            Self::Offer => "offer",
            Self::Closed => "closed",
        }
    }

    fn parse(raw: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|stage| stage.as_str() == raw)
            .unwrap_or(Self::Found)
    }
}

/// Where an opportunity came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpportunitySource {
    Greenhouse,
    Lever,
    Ashby,
    Arbeitnow,
    /// HN "Who is hiring".
    Hn,
    /// Added by link, from any site.
    Link,
}

impl OpportunitySource {
    fn parse(raw: &str) -> Self {
        match raw {
            "greenhouse" => Self::Greenhouse,
            "lever" => Self::Lever,
            "ashby" => Self::Ashby,
            "arbeitnow" => Self::Arbeitnow,
            "hn" => Self::Hn,
            _ => Self::Link,
        }
    }
}

/// How the job is worked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobWorkMode {
    Remote,
    Hybrid,
    Onsite,
}

impl JobWorkMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "remote" => Some(Self::Remote),
            "hybrid" => Some(Self::Hybrid),
            "onsite" => Some(Self::Onsite),
            _ => None,
        }
    }
}

/// What a posting says about a fact a requirement is checked on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fact {
    Yes,
    No,
    /// It does not say.
    Unclear,
}

impl Fact {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "yes" => Some(Self::Yes),
            "no" => Some(Self::No),
            "unclear" => Some(Self::Unclear),
            _ => None,
        }
    }
}

/// Why an opportunity is hidden: it fails a hard requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HiddenReason {
    /// They work remote only, and it is not remote (or does not say).
    NotRemote,
    /// It cannot be done from where they live (remote only from elsewhere,
    /// or in a place they cannot be).
    NotOpenToLocation,
    /// It needs a language they did not list.
    Language,
    /// Its stated pay is under their floor.
    BelowFloor,
}

impl HiddenReason {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "not_remote" => Some(Self::NotRemote),
            "not_open_to_location" => Some(Self::NotOpenToLocation),
            "language" => Some(Self::Language),
            "below_floor" => Some(Self::BelowFloor),
            _ => None,
        }
    }
}

/// What the posting says about the person's requirements, as their search
/// read it: "Remote · open to you · English".
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct JobFacts {
    /// It can be done fully remote.
    pub remote: Fact,
    /// Someone where the person lives can do it without moving.
    pub open_to_candidate: Fact,
    /// Where it is open, as the posting says ("Germany or Austria only").
    pub region_note: Option<String>,
    /// The languages it needs, ISO 639-1 (`en`).
    pub working_languages: Vec<String>,
}

/// An opportunity, as the table, the board and the list show it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OpportunitySummary {
    pub id: Uuid,
    /// The role.
    pub title: String,
    pub company: String,
    /// Where, as the posting says.
    pub location: Option<String>,
    pub work_mode: Option<JobWorkMode>,
    /// The posting.
    pub url: String,
    pub source: OpportunitySource,
    /// How well it matches the profile and goals, 0–100; none for one added
    /// by link.
    #[serde(rename = "match")]
    pub match_score: Option<u8>,
    pub stage: OpportunityStage,
    pub next_step: Option<String>,
    /// Pay, as the posting says.
    pub salary_text: Option<String>,
    /// Found and never opened.
    pub new: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Why it is hidden, if it is: it fails a hard requirement.
    pub hidden_reason: Option<HiddenReason>,
    /// Where it is open, as the posting says, to go with the reason.
    pub region_note: Option<String>,
}

/// An opportunity, whole: what the side peek shows.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Opportunity {
    pub id: Uuid,
    /// The role.
    pub title: String,
    pub company: String,
    /// Where, as the posting says.
    pub location: Option<String>,
    pub work_mode: Option<JobWorkMode>,
    /// The posting.
    pub url: String,
    pub source: OpportunitySource,
    /// How well it matches the profile and goals, 0–100; none for one added
    /// by link.
    #[serde(rename = "match")]
    pub match_score: Option<u8>,
    pub stage: OpportunityStage,
    pub next_step: Option<String>,
    /// Pay, as the posting says.
    pub salary_text: Option<String>,
    /// Found and never opened.
    pub new: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// The posting's description, as plain text (up to 20 000 characters).
    pub description: Option<String>,
    /// Why it matches, citing the profile (up to 3).
    pub why: Vec<String>,
    /// What the profile or goals miss (up to 3).
    pub gaps: Vec<String>,
    /// What the posting says about their requirements; none when never
    /// checked (added by link, or found before the rules and not yet
    /// checked).
    pub facts: Option<JobFacts>,
    /// Why it is hidden, if it is: it fails a hard requirement.
    pub hidden_reason: Option<HiddenReason>,
    /// The person's notes.
    pub notes: Option<String>,
    /// When the posting was published.
    pub posted_at: Option<DateTime<Utc>>,
    /// "Not for me": hidden from the pipeline.
    pub dismissed: bool,
    /// Title, company and location can be typed (an opportunity added by a
    /// link to a site that is not a job board).
    pub editable: bool,
}

/// The pipeline.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OpportunityList {
    /// Newest first; never the dismissed ones, and the hidden ones only when
    /// asked for.
    pub opportunities: Vec<OpportunitySummary>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct Row {
    id: Uuid,
    title: String,
    company: String,
    location: Option<String>,
    work_mode: Option<String>,
    url: String,
    source: String,
    posted_at: Option<DateTime<Utc>>,
    salary_text: Option<String>,
    description: Option<String>,
    match_score: Option<i32>,
    match_why: sqlx::types::Json<Vec<String>>,
    match_gaps: sqlx::types::Json<Vec<String>>,
    stage: String,
    next_step: Option<String>,
    notes: Option<String>,
    dismissed: bool,
    opened_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    remote: Option<String>,
    open_to_candidate: Option<String>,
    region_note: Option<String>,
    working_languages: Option<Vec<String>>,
    hidden_reason: Option<String>,
}

/// The columns a [`Row`] reads, as a literal (queries are static strings).
macro_rules! columns {
    () => {
        "id, title, company, location, work_mode, url, source, posted_at, salary_text, \
         description, match_score, match_why, match_gaps, stage, next_step, notes, dismissed, \
         opened_at, created_at, updated_at, remote, open_to_candidate, region_note, \
         working_languages, hidden_reason"
    };
}

impl Row {
    fn is_new(&self) -> bool {
        self.stage == "found"
            && self.opened_at.is_none()
            && !self.dismissed
            && self.hidden_reason.is_none()
    }

    fn hidden_reason(&self) -> Option<HiddenReason> {
        self.hidden_reason.as_deref().and_then(HiddenReason::parse)
    }

    fn facts(&self) -> Option<JobFacts> {
        Some(JobFacts {
            remote: Fact::parse(self.remote.as_deref()?)?,
            open_to_candidate: Fact::parse(self.open_to_candidate.as_deref()?)?,
            region_note: self.region_note.clone(),
            working_languages: self.working_languages.clone().unwrap_or_default(),
        })
    }

    fn match_score(&self) -> Option<u8> {
        self.match_score
            .and_then(|m| u8::try_from(m.clamp(0, 100)).ok())
    }

    fn summary(self) -> OpportunitySummary {
        OpportunitySummary {
            new: self.is_new(),
            match_score: self.match_score(),
            hidden_reason: self.hidden_reason(),
            id: self.id,
            work_mode: self.work_mode.as_deref().and_then(JobWorkMode::parse),
            source: OpportunitySource::parse(&self.source),
            stage: OpportunityStage::parse(&self.stage),
            title: self.title,
            company: self.company,
            location: self.location,
            url: self.url,
            next_step: self.next_step,
            salary_text: self.salary_text,
            created_at: self.created_at,
            updated_at: self.updated_at,
            region_note: self.region_note,
        }
    }

    pub(crate) fn opportunity(self) -> Opportunity {
        let source = OpportunitySource::parse(&self.source);
        Opportunity {
            new: self.is_new(),
            match_score: self.match_score(),
            facts: self.facts(),
            hidden_reason: self.hidden_reason(),
            id: self.id,
            work_mode: self.work_mode.as_deref().and_then(JobWorkMode::parse),
            source,
            stage: OpportunityStage::parse(&self.stage),
            title: self.title,
            company: self.company,
            location: self.location,
            url: self.url,
            next_step: self.next_step,
            salary_text: self.salary_text,
            created_at: self.created_at,
            updated_at: self.updated_at,
            description: self.description,
            why: self.match_why.0,
            gaps: self.match_gaps.0,
            notes: self.notes,
            posted_at: self.posted_at,
            dismissed: self.dismissed,
            editable: source == OpportunitySource::Link,
        }
    }
}

/// Which opportunities to list.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    /// Only this stage.
    pub stage: Option<OpportunityStage>,
    /// Only those whose role, company or place contains this.
    pub q: Option<String>,
    /// Only the new ones (Found, never opened).
    pub new: Option<bool>,
    /// Only the hidden ones: found before the requirement rules, they fail
    /// one. Left out otherwise.
    pub hidden: Option<bool>,
}

/// `text` as a `LIKE` pattern matching it anywhere.
fn contains_pattern(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// The pipeline.
///
/// Every opportunity of the signed-in person, newest first, except the
/// dismissed ones ("Not for me") and the hidden ones (they fail a hard
/// requirement; `hidden=true` lists only those, with the reason). Filter by
/// stage, by text in the role, company or place, or to the new ones.
#[utoipa::path(
    get,
    path = "/api/v1/opportunities",
    tag = "pipeline",
    operation_id = "pipeline.list",
    security(("session" = [])),
    params(ListQuery),
    responses(
        (status = 200, description = "The opportunities.", body = OpportunityList),
        (status = 400, description = "`invalid_query`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    query: std::result::Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<OpportunityList>> {
    let Query(query) =
        query.map_err(|rejection| Error::BadRequest("invalid_query", rejection.body_text()))?;
    let pattern = query
        .q
        .as_deref()
        .and_then(|q| tidy(Some(q), MAX_SHORT))
        .map(|q| contains_pattern(&q));
    let rows: Vec<Row> = sqlx::query_as(concat!(
        "SELECT ",
        columns!(),
        " FROM pipeline.opportunities
          WHERE account_id = $1 AND NOT dismissed
            AND ($2::text IS NULL OR stage = $2)
            AND ($3::text IS NULL OR title ILIKE $3 OR company ILIKE $3
                 OR coalesce(location, '') ILIKE $3)
            AND (NOT $4 OR (stage = 'found' AND opened_at IS NULL))
            AND (hidden_reason IS NOT NULL) = $5
          ORDER BY created_at DESC, id DESC
          LIMIT 1000"
    ))
    .bind(principal.account_id)
    .bind(query.stage.map(OpportunityStage::as_str))
    .bind(pattern)
    .bind(query.new.unwrap_or(false))
    .bind(query.hidden.unwrap_or(false))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(OpportunityList {
        opportunities: rows.into_iter().map(Row::summary).collect(),
    }))
}

pub(crate) async fn fetch(db: &sqlx::PgPool, account: Uuid, id: Uuid) -> Result<Option<Row>> {
    Ok(sqlx::query_as(concat!(
        "SELECT ",
        columns!(),
        " FROM pipeline.opportunities WHERE id = $1 AND account_id = $2"
    ))
    .bind(id)
    .bind(account)
    .fetch_optional(db)
    .await?)
}

/// An opportunity.
///
/// One of the signed-in person's, dismissed ones included. Anyone else's is
/// not found.
#[utoipa::path(
    get,
    path = "/api/v1/opportunities/{id}",
    tag = "pipeline",
    operation_id = "pipeline.get",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity.")),
    responses(
        (status = 200, description = "The opportunity.", body = Opportunity),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn get(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
) -> Result<Json<Opportunity>> {
    let row = fetch(&state.db, principal.account_id, id)
        .await?
        .ok_or(Error::NotFound)?;
    Ok(Json(row.opportunity()))
}

/// A job to add by its link.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewOpportunity {
    /// The posting's address (http or https, up to 2 000 characters).
    pub url: String,
    /// The role, for a link to a site that is not a job board.
    #[serde(default)]
    pub title: Option<String>,
    /// The company, for a link to a site that is not a job board.
    #[serde(default)]
    pub company: Option<String>,
}

fn invalid_url() -> Error {
    Error::BadRequest(
        "invalid_url",
        "Paste the posting's address, starting with https://.".to_owned(),
    )
}

/// `raw` as a posting's address: http or https, with a host, at most
/// [`MAX_URL`] characters, without its fragment.
fn posting_url(raw: &str) -> Result<Url> {
    let raw = raw.trim();
    if raw.is_empty() || raw.chars().count() > MAX_URL {
        return Err(invalid_url());
    }
    let mut url = Url::parse(raw).map_err(|_| invalid_url())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid_url());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid_url());
    }
    url.set_fragment(None);
    Ok(url)
}

/// Add a job by its link.
///
/// A Greenhouse, Lever or Ashby posting is read from the board's public API
/// (title, company, place, pay, description), and that company's board is
/// searched from then on. Any other link becomes an opportunity with the
/// link and the title and company given (typed later if not): the server
/// never fetches it. The same posting twice answers the one already there
/// (200).
#[utoipa::path(
    post,
    path = "/api/v1/opportunities",
    tag = "pipeline",
    operation_id = "pipeline.add",
    security(("session" = [])),
    request_body = NewOpportunity,
    responses(
        (status = 201, description = "Added.", body = Opportunity),
        (status = 200, description = "That posting was already in the pipeline.", body = Opportunity),
        (status = 400, description = "`invalid_url`, `invalid_request`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn add(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<PipelineDeps>,
    body: std::result::Result<Json<NewOpportunity>, JsonRejection>,
) -> Result<(StatusCode, Json<Opportunity>)> {
    let Json(new) =
        body.map_err(|rejection| Error::BadRequest("invalid_request", rejection.body_text()))?;
    let url = posting_url(&new.url)?;
    let account = principal.account_id;
    let posting = match &deps.neighbours {
        Some(neighbours) => neighbours.read_posting(&state.db, account, &url).await?,
        None => None,
    };
    let row = match posting {
        Some(posting) => NewRow {
            source_ref: posting.source_ref,
            title: tidy(Some(&posting.title), MAX_SHORT).unwrap_or_default(),
            company: tidy(Some(&posting.company), MAX_SHORT).unwrap_or_default(),
            location: tidy(posting.location.as_deref(), MAX_SHORT),
            work_mode: posting
                .work_mode
                .filter(|mode| store::WORK_MODES.contains(&mode.as_str())),
            url: tidy(Some(&posting.url), MAX_URL).unwrap_or_else(|| url.to_string()),
            posted_at: posting.posted_at,
            salary_text: tidy(posting.salary_text.as_deref(), MAX_SHORT),
            description: tidy(posting.description.as_deref(), store::MAX_DESCRIPTION),
            source: posting.source,
            ..NewRow::default()
        },
        None => NewRow {
            source: "link".to_owned(),
            source_ref: url.to_string(),
            title: tidy(new.title.as_deref(), MAX_SHORT).unwrap_or_default(),
            company: tidy(new.company.as_deref(), MAX_SHORT).unwrap_or_default(),
            url: url.to_string(),
            ..NewRow::default()
        },
    };
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let inserted = store::insert(&mut tx, &state.events, account, &row, now).await?;
    tx.commit().await?;
    let (status, id) = match inserted {
        Some(id) => (StatusCode::CREATED, id),
        None => {
            let id: Uuid = sqlx::query_scalar(
                "SELECT id FROM pipeline.opportunities
                  WHERE account_id = $1 AND source = $2 AND source_ref = $3",
            )
            .bind(account)
            .bind(&row.source)
            .bind(&row.source_ref)
            .fetch_one(&state.db)
            .await?;
            (StatusCode::OK, id)
        }
    };
    let row = fetch(&state.db, account, id)
        .await?
        .ok_or(Error::NotFound)?;
    tracing::info!(opportunity = %id, source = %row.source, "opportunity added by link");
    Ok((status, Json(row.opportunity())))
}

/// A field that may be set, cleared (`null`) or left out.
fn present<'de, D, T>(d: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// What to change; anything left out stays as it is.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OpportunityChanges {
    /// Move it to this stage.
    #[serde(default)]
    pub stage: Option<OpportunityStage>,
    /// The next step (up to 300 characters); `null` clears it.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    pub next_step: Option<Option<String>>,
    /// The notes (up to 10 000 characters); `null` clears them.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    pub notes: Option<Option<String>>,
    /// "Not for me" (`true`) or back in the pipeline (`false`).
    #[serde(default)]
    pub dismissed: Option<bool>,
    /// It was opened: it is no longer new.
    #[serde(default)]
    pub opened: Option<bool>,
    /// The role; only for one added by link to another site.
    #[serde(default)]
    pub title: Option<String>,
    /// The company; only for one added by link to another site.
    #[serde(default)]
    pub company: Option<String>,
    /// Where; only for one added by link to another site.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    pub location: Option<Option<String>>,
}

fn invalid(detail: &str) -> Error {
    Error::BadRequest("invalid_opportunity", detail.to_owned())
}

/// `text` within `most` characters, trimmed; blank is none.
fn checked(text: Option<&str>, most: usize, name: &str) -> Result<Option<String>> {
    if text.is_some_and(|t| t.trim().chars().count() > most) {
        return Err(invalid(&format!(
            "`{name}` is longer than {most} characters."
        )));
    }
    Ok(tidy(text, most))
}

/// Change an opportunity.
///
/// Its stage, next step, notes, "Not for me" (`dismissed`), or that it was
/// opened (no longer new). Title, company and place can be typed only for
/// one added by a link to a site that is not a job board.
#[utoipa::path(
    patch,
    path = "/api/v1/opportunities/{id}",
    tag = "pipeline",
    operation_id = "pipeline.update",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "The opportunity.")),
    request_body = OpportunityChanges,
    responses(
        (status = 200, description = "The opportunity, changed.", body = Opportunity),
        (status = 400, description = "`invalid_opportunity`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`not_editable`: its title, company and place come from \
            the job board", body = Problem, content_type = "application/problem+json"),
    ),
)]
pub async fn update(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(id): Path<Uuid>,
    body: std::result::Result<Json<OpportunityChanges>, JsonRejection>,
) -> Result<Json<Opportunity>> {
    let Json(changes) =
        body.map_err(|rejection| Error::BadRequest("invalid_opportunity", rejection.body_text()))?;
    let account = principal.account_id;
    let now = state.clock.now();
    let mut tx = state.db.begin().await?;
    let row: Row = sqlx::query_as(concat!(
        "SELECT ",
        columns!(),
        " FROM pipeline.opportunities WHERE id = $1 AND account_id = $2 FOR UPDATE"
    ))
    .bind(id)
    .bind(account)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error::NotFound)?;
    let typing = changes.title.is_some() || changes.company.is_some() || changes.location.is_some();
    if typing && row.source != "link" {
        return Err(Error::Conflict(
            "not_editable",
            "This one's role, company and place come from the job board.".to_owned(),
        ));
    }
    let next_step = match &changes.next_step {
        Some(value) => checked(value.as_deref(), MAX_SHORT, "next_step")?,
        None => row.next_step.clone(),
    };
    let notes = match &changes.notes {
        Some(value) => checked(value.as_deref(), MAX_NOTES, "notes")?,
        None => row.notes.clone(),
    };
    let location = match &changes.location {
        Some(value) => checked(value.as_deref(), MAX_SHORT, "location")?,
        None => row.location.clone(),
    };
    let title = match &changes.title {
        Some(value) => checked(Some(value), MAX_SHORT, "title")?.unwrap_or_default(),
        None => row.title.clone(),
    };
    let company = match &changes.company {
        Some(value) => checked(Some(value), MAX_SHORT, "company")?.unwrap_or_default(),
        None => row.company.clone(),
    };
    let stage = changes
        .stage
        .map_or_else(|| row.stage.clone(), |s| s.as_str().to_owned());
    let dismissed = changes.dismissed.unwrap_or(row.dismissed);
    let opened_at = match changes.opened {
        Some(true) => row.opened_at.or(Some(now)),
        _ => row.opened_at,
    };
    let changed = stage != row.stage
        || dismissed != row.dismissed
        || next_step != row.next_step
        || notes != row.notes
        || location != row.location
        || title != row.title
        || company != row.company;
    let updated_at = if changed { now } else { row.updated_at };
    let row: Row = sqlx::query_as(concat!(
        "UPDATE pipeline.opportunities
            SET stage = $3, dismissed = $4, next_step = $5, notes = $6, location = $7,
                title = $8, company = $9, opened_at = $10, updated_at = $11
          WHERE id = $1 AND account_id = $2
      RETURNING ",
        columns!()
    ))
    .bind(id)
    .bind(account)
    .bind(&stage)
    .bind(dismissed)
    .bind(&next_step)
    .bind(&notes)
    .bind(&location)
    .bind(&title)
    .bind(&company)
    .bind(opened_at)
    .bind(updated_at)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    if changed {
        tracing::info!(opportunity = %id, stage = %stage, dismissed, "opportunity changed");
    }
    Ok(Json(row.opportunity()))
}
