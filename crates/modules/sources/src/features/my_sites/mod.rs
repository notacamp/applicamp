//! `/api/v1/my-sites`: the job sites a person has On (#94).
//!
//! One list of the sites the app can read for them, in groups by what the
//! sites offer, each On or Off.
//!
//! - **The app sets them first.** Every approved site the person has no
//!   choice on gets On or Off from their goals ([`rules::fits`]): where they
//!   want to work and whether remote, where they live, and their roles and
//!   skills against the site's tags. It is set again whenever the list is
//!   read, so a change of goals, and a site approved later, are picked up;
//!   a site the person turned On or Off themselves is never touched.
//! - **Only sites count.** Hiring systems and company pages are not listed
//!   one by one: the page says how many company pages are also read.
//! - **A site that needs a key** is listed with `needs_key` and cannot be
//!   turned On until the key is set. **Sites that do not allow apps** (their
//!   terms or robots rules forbid it) are named in one sentence, not listed.
//!
//! Searches do not read this yet: #89 makes them read the person's On sites
//! only, through [`ensure_defaults`] and [`on_sites`].

pub mod rules;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use applicamp_kernel::{AppState, Error, Problem, Result, auth::SignedIn};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    SourcesDeps,
    features::boards,
    job_boards::{self, BoardRow, Priority, Status},
    matching::MatchInput,
    pay,
};
use rules::{Group, Offer, Remote, Wants};

/// Names of blocked sites the answer lists, at most (the count is whole).
const MOST_NAMES: usize = 20;

/// What the list shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SitesShow)]
pub enum Show {
    /// Every site.
    #[default]
    All,
    /// The sites that are On.
    On,
    /// Remote-first sites.
    Remote,
    /// Sites with jobs in Europe.
    Europe,
    /// Sites with jobs in the person's country.
    Country,
    /// Sites for one trade or stack.
    Trade,
}

/// What the person asks to see.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct MySitesQuery {
    /// Only sites whose name has this in it, ignoring case.
    pub q: Option<String>,
    /// What to show; all sites when left out.
    #[param(inline)]
    pub show: Option<Show>,
}

/// How a site's typical salary is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SiteSalaryBasis)]
pub enum SalaryBasis {
    /// From the salaries its jobs state.
    Listed,
    /// From its country's typical pay.
    Country,
}

/// Who last set a site On or Off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SiteSetBy)]
pub enum SetBy {
    /// The app, from the person's goals.
    App,
    /// The person.
    Person,
}

impl SetBy {
    fn parse(raw: &str) -> Self {
        if raw == "person" {
            Self::Person
        } else {
            Self::App
        }
    }
}

/// One site, as a person sees it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MySite {
    pub id: Uuid,
    pub name: String,
    pub group: Group,
    /// The kinds of jobs it has (`tech`, `design`...); none: every kind.
    pub kinds: Vec<String>,
    /// Where its jobs are: `worldwide`, `europe`, a region (`north-america`)
    /// or a country's code; none when not known.
    pub place: Option<String>,
    pub remote: Remote,
    /// The stack or trade it is built around, as plain words.
    pub specialities: Vec<String>,
    /// Its typical salary in euros a year, rounded to a thousand; none when
    /// not known.
    pub salary_eur_year: Option<f64>,
    pub salary_basis: Option<SalaryBasis>,
    /// Jobs new on the site in the last 7 days, from the job store; none
    /// when the store has no count for it yet.
    pub new_this_week: Option<u32>,
    /// It is On for the person.
    pub on: bool,
    /// Who set it.
    pub set_by: SetBy,
    /// It needs an API key before it can be read: the owner adds it.
    pub needs_key: bool,
}

/// A group of the list.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MySiteGroup {
    pub group: Group,
    pub sites: Vec<MySite>,
}

/// The sites that do not allow apps to read them.
#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct BlockedSites {
    /// How many.
    pub count: u32,
    /// Their names (the first ones).
    pub names: Vec<String>,
}

/// The person's job sites.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MySites {
    /// Sites the app can read for them, whatever is searched or shown.
    pub available: u32,
    /// Of those, the ones that are On.
    pub on: u32,
    /// The country they live in (ISO 3166-1), when known.
    pub country: Option<String>,
    /// The groups in order, with the sites that match what is searched and
    /// shown; a group with none is left out.
    pub groups: Vec<MySiteGroup>,
    pub blocked: BlockedSites,
    /// Company hiring pages the app also reads, one by one unlisted.
    pub company_pages: u32,
}

/// The person's choice for a site.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MySiteChange {
    /// On or Off.
    pub on: bool,
}

/// A site as the person now has it.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct MySiteChoice {
    pub id: Uuid,
    pub on: bool,
    pub set_by: SetBy,
}

fn neighbours(deps: &SourcesDeps) -> Result<&std::sync::Arc<dyn crate::Neighbours>> {
    deps.neighbours
        .as_ref()
        .ok_or_else(|| Error::internal("the sources module is not composed"))
}

/// What a row is on the list.
enum Class {
    /// Listed; it needs a key before it can be On.
    Listed { needs_key: bool },
    /// Its terms or robots rules do not allow apps.
    Blocked,
    /// Not for the list (paused, declined, or not ready).
    Hidden,
}

fn classify(row: &BoardRow) -> Class {
    let forbids = boards::blockers(row).iter().any(|b| {
        matches!(
            b.code.as_str(),
            "terms_forbid" | "needs_permission" | "robots_disallowed"
        )
    });
    let needs_key = row
        .config()
        .ok()
        .is_some_and(|config| config.key.is_some() && !row.key_set_for(&config));
    match row.status() {
        Status::Approved => Class::Listed { needs_key },
        Status::Candidate | Status::Declined if forbids => Class::Blocked,
        Status::Candidate if needs_key => Class::Listed { needs_key: true },
        _ => Class::Hidden,
    }
}

/// The registry's sites: not the hiring systems, not the company pages.
async fn site_rows(db: &PgPool) -> Result<Vec<BoardRow>> {
    Ok(sqlx::query_as(concat!(
        "SELECT ",
        job_boards::board_columns!(),
        " FROM sources.job_boards
          WHERE kind NOT IN ('hiring_system', 'company_page')
            AND status IN ('approved', 'candidate', 'declined')
          ORDER BY name, slug"
    ))
    .fetch_all(db)
    .await?)
}

/// A site with what the person's list needs of it.
struct Entry<'a> {
    row: &'a BoardRow,
    needs_key: bool,
    offer: Offer,
    group: Group,
}

/// The person's choices, by site: on, and who set it.
async fn stored(db: &PgPool, account: Uuid) -> Result<HashMap<Uuid, (bool, SetBy)>> {
    let rows: Vec<(Uuid, bool, String)> = sqlx::query_as(
        "SELECT board_id, is_on, set_by FROM sources.my_sites WHERE account_id = $1",
    )
    .bind(account)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(board, on, by)| (board, (on, SetBy::parse(&by))))
        .collect())
}

/// Set the app's default (On or Off) for each of `defaults`: a site with no
/// choice yet gets one; one the app set before is set again when it changed;
/// one the person chose is left as it is.
async fn write_defaults(
    db: &PgPool,
    account: Uuid,
    defaults: &[(Uuid, bool)],
    now: DateTime<Utc>,
) -> Result<()> {
    if defaults.is_empty() {
        return Ok(());
    }
    let boards: Vec<Uuid> = defaults.iter().map(|(board, _)| *board).collect();
    let ons: Vec<bool> = defaults.iter().map(|(_, on)| *on).collect();
    sqlx::query(
        "INSERT INTO sources.my_sites AS m (account_id, board_id, is_on, set_by, updated_at)
         SELECT $1, t.board_id, t.is_on, 'app', $4
           FROM unnest($2::uuid[], $3::bool[]) AS t(board_id, is_on)
         ON CONFLICT (account_id, board_id) DO UPDATE
            SET is_on = EXCLUDED.is_on, updated_at = EXCLUDED.updated_at
          WHERE m.set_by = 'app' AND m.is_on <> EXCLUDED.is_on",
    )
    .bind(account)
    .bind(&boards)
    .bind(&ons)
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

/// The app's defaults for `account`: every approved site that needs no key
/// is set On or Off from `input` (their profile and goals), unless the person
/// chose. Cheap, and safe to repeat; searches call it before they read
/// [`on_sites`].
pub async fn ensure_defaults(
    db: &PgPool,
    account: Uuid,
    input: &MatchInput,
    now: DateTime<Utc>,
) -> Result<()> {
    let rows = site_rows(db).await?;
    let wants = rules::wants_of(input);
    let entries = listed(&rows, wants.home.as_deref());
    write_defaults(db, account, &defaults_of(&entries, &wants), now).await
}

/// The approved sites `account` has On.
pub async fn on_sites(db: &PgPool, account: Uuid) -> Result<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT m.board_id FROM sources.my_sites m
           JOIN sources.job_boards b ON b.id = m.board_id
          WHERE m.account_id = $1 AND m.is_on AND b.status = 'approved'
            AND b.kind NOT IN ('hiring_system', 'company_page')
          ORDER BY b.created_at, b.slug",
    )
    .bind(account)
    .fetch_all(db)
    .await?)
}

fn listed<'a>(rows: &'a [BoardRow], home: Option<&str>) -> Vec<Entry<'a>> {
    rows.iter()
        .filter_map(|row| match classify(row) {
            Class::Listed { needs_key } => {
                let offer = rules::offer_of(
                    &row.tags,
                    row.country.as_deref(),
                    row.priority() == Priority::High,
                );
                let group = rules::group_of(&offer, home);
                Some(Entry {
                    row,
                    needs_key,
                    offer,
                    group,
                })
            }
            Class::Blocked | Class::Hidden => None,
        })
        .collect()
}

/// The app's choice for each approved site that needs no key.
fn defaults_of(entries: &[Entry<'_>], wants: &Wants) -> Vec<(Uuid, bool)> {
    entries
        .iter()
        .filter(|e| !e.needs_key && e.row.status() == Status::Approved)
        .map(|e| (e.row.id, rules::fits(&e.offer, e.group, wants)))
        .collect()
}

/// Average stated salary and how many jobs state one, by site, over the
/// last 30 days (engineering jobs, as the Boards page ranks them).
async fn salaries(db: &PgPool, since: DateTime<Utc>) -> Result<HashMap<Uuid, (f64, i64)>> {
    let rows: Vec<(Uuid, Option<f64>, i64)> = sqlx::query_as(
        "SELECT board_id,
                avg(salary_eur_year) FILTER (WHERE engineering AND salary_eur_year IS NOT NULL),
                count(*) FILTER (WHERE engineering AND salary_eur_year IS NOT NULL)
           FROM sources.board_jobs
          WHERE coalesce(posted_at, seen_at) >= $1
          GROUP BY board_id",
    )
    .bind(since)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(board, avg, count)| avg.map(|avg| (board, (avg, count))))
        .collect())
}

/// Jobs first seen in the job store in the last 7 days, by site: one grouped
/// query. A site with none is missing.
async fn new_jobs(db: &PgPool, since: DateTime<Utc>) -> Result<HashMap<Uuid, u32>> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT board_id, count(*) FROM sources.store_jobs
          WHERE first_seen_at > $1 GROUP BY board_id",
    )
    .bind(since)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(board, n)| (board, u32::try_from(n).unwrap_or(u32::MAX)))
        .collect())
}

/// A site's typical salary: its own jobs' average from enough salaries,
/// else its country's typical pay; rounded to a thousand euros.
fn typical_salary(row: &BoardRow, own: Option<&(f64, i64)>) -> (Option<f64>, Option<SalaryBasis>) {
    let thousand = |eur: f64| (eur / 1000.0).round() * 1000.0;
    if let Some((avg, count)) = own
        && *count >= i64::from(pay::MIN_SALARIES)
    {
        return (Some(thousand(*avg)), Some(SalaryBasis::Listed));
    }
    let benchmark = row
        .country
        .as_deref()
        .and_then(|code| pay::COUNTRIES.iter().find(|b| b.code == code));
    match benchmark {
        Some(b) => (Some(thousand(b.eur_year)), Some(SalaryBasis::Country)),
        None => (None, None),
    }
}

fn shown(site: &MySite, show: Show) -> bool {
    match show {
        Show::All => true,
        Show::On => site.on,
        Show::Remote => site.group == Group::RemoteWorldwide || site.remote == Remote::Mostly,
        Show::Europe => site.group == Group::Europe,
        Show::Country => site.group == Group::Country,
        Show::Trade => site.group == Group::Trade,
    }
}

async fn view(
    deps: &SourcesDeps,
    state: &AppState,
    account: Uuid,
    query: &MySitesQuery,
) -> Result<MySites> {
    let now = state.clock.now();
    let rows = site_rows(&state.db).await?;
    let input = neighbours(deps)?
        .matching_input(&state.db, account, now)
        .await?;
    let wants = input.as_ref().map(rules::wants_of);
    let home = wants.as_ref().and_then(|w| w.home.clone());
    let entries = listed(&rows, home.as_deref());
    if let Some(wants) = &wants {
        write_defaults(&state.db, account, &defaults_of(&entries, wants), now).await?;
    }
    let mut blocked: Vec<String> = rows
        .iter()
        .filter(|row| matches!(classify(row), Class::Blocked))
        .map(|row| row.name.clone())
        .collect();
    blocked.sort();
    let chosen = stored(&state.db, account).await?;
    let fresh = new_jobs(&state.db, now - TimeDelta::days(7)).await?;
    let salary_by = salaries(&state.db, now - TimeDelta::days(30)).await?;
    let needle = query
        .q
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_lowercase();
    let show = query.show.unwrap_or_default();
    let mut sites: Vec<MySite> = entries
        .iter()
        .map(|e| {
            let (on, set_by) = chosen
                .get(&e.row.id)
                .copied()
                .unwrap_or((false, SetBy::App));
            let (salary_eur_year, salary_basis) = typical_salary(e.row, salary_by.get(&e.row.id));
            MySite {
                id: e.row.id,
                name: e.row.name.clone(),
                group: e.group,
                kinds: e.offer.kinds.clone(),
                place: e.offer.place.code(),
                remote: e.offer.remote,
                specialities: e.offer.specialities.clone(),
                salary_eur_year,
                salary_basis,
                new_this_week: fresh.get(&e.row.id).copied().filter(|n| *n > 0),
                on: on && !e.needs_key,
                set_by,
                needs_key: e.needs_key,
            }
        })
        .collect();
    let available = u32::try_from(sites.len()).unwrap_or(u32::MAX);
    let on = u32::try_from(sites.iter().filter(|s| s.on).count()).unwrap_or(u32::MAX);
    sites.retain(|s| {
        shown(s, show) && (needle.is_empty() || s.name.to_lowercase().contains(&needle))
    });
    // The best paid first, the ones with no figure last.
    sites.sort_by(|a, b| {
        let key = |s: &MySite| s.salary_eur_year.unwrap_or(f64::NEG_INFINITY);
        key(b).total_cmp(&key(a)).then_with(|| a.name.cmp(&b.name))
    });
    let groups = Group::ALL
        .into_iter()
        .filter_map(|group| {
            let in_group: Vec<MySite> =
                sites.iter().filter(|s| s.group == group).cloned().collect();
            (!in_group.is_empty()).then_some(MySiteGroup {
                group,
                sites: in_group,
            })
        })
        .collect();
    let company_pages: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sources.job_boards WHERE kind = 'company_page' AND status = 'approved'",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(MySites {
        available,
        on,
        country: home,
        groups,
        blocked: BlockedSites {
            count: u32::try_from(blocked.len()).unwrap_or(u32::MAX),
            names: blocked.into_iter().take(MOST_NAMES).collect(),
        },
        company_pages: u32::try_from(company_pages).unwrap_or(u32::MAX),
    })
}

/// My job sites.
///
/// The sites the app can read for the signed-in person, in groups: Remote,
/// worldwide; Europe; their country; By trade; Startups and all-jobs sites.
/// Each is On or Off: the app sets it from their goals (where they want to
/// work, remote or not, where they live, their roles and skills against what
/// the site offers) until they choose themselves. `q` finds a site by name;
/// `show` keeps the sites that are On, remote, in Europe, in their country or
/// for one trade. Also: the sites that do not allow apps to read them, and
/// how many company hiring pages the app reads besides.
#[utoipa::path(
    get,
    path = "/api/v1/my-sites",
    tag = "sources",
    operation_id = "sources.my_sites",
    security(("session" = [])),
    params(MySitesQuery),
    responses(
        (status = 200, description = "The person's sites.", body = MySites),
        (status = 400, description = "`invalid_query`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Extension(deps): Extension<SourcesDeps>,
    query: std::result::Result<Query<MySitesQuery>, QueryRejection>,
) -> Result<Json<MySites>> {
    let Query(query) =
        query.map_err(|rejection| Error::BadRequest("invalid_query", rejection.body_text()))?;
    Ok(Json(
        view(&deps, &state, principal.account_id, &query).await?,
    ))
}

/// Turn a site On or Off.
///
/// The person's own choice: the app does not change it again, even when
/// their goals change. A site that needs a key cannot be turned On until the
/// key is set (409 `needs_key`); a site that is not on the list is 404.
#[utoipa::path(
    put,
    path = "/api/v1/my-sites/{board_id}",
    tag = "sources",
    operation_id = "sources.set_my_site",
    security(("session" = [])),
    params(("board_id" = Uuid, Path, description = "The site.")),
    request_body = MySiteChange,
    responses(
        (status = 200, description = "Set.", body = MySiteChoice),
        (status = 400, description = "`invalid_site`", body = Problem,
            content_type = "application/problem+json"),
        (status = 401, description = "`session_required`", body = Problem,
            content_type = "application/problem+json"),
        (status = 404, description = "`not_found`", body = Problem,
            content_type = "application/problem+json"),
        (status = 409, description = "`needs_key`", body = Problem,
            content_type = "application/problem+json"),
    ),
)]
pub async fn set(
    State(state): State<AppState>,
    SignedIn(principal): SignedIn,
    Path(board_id): Path<Uuid>,
    body: std::result::Result<Json<MySiteChange>, JsonRejection>,
) -> Result<Json<MySiteChoice>> {
    let Json(change) =
        body.map_err(|rejection| Error::BadRequest("invalid_site", rejection.body_text()))?;
    let row = job_boards::get(&state.db, board_id)
        .await?
        .filter(|row| !row.kind().is_some_and(|k| k.is_hiring()))
        .ok_or(Error::NotFound)?;
    let needs_key = match classify(&row) {
        Class::Listed { needs_key } => needs_key,
        Class::Blocked | Class::Hidden => return Err(Error::NotFound),
    };
    if change.on && needs_key {
        return Err(Error::Conflict(
            "needs_key",
            "This site needs a key before it can be turned on.".to_owned(),
        ));
    }
    if row.status() != Status::Approved {
        // A site still waiting (it needs a key) cannot be chosen yet.
        return Err(Error::NotFound);
    }
    sqlx::query(
        "INSERT INTO sources.my_sites (account_id, board_id, is_on, set_by, updated_at)
         VALUES ($1, $2, $3, 'person', $4)
         ON CONFLICT (account_id, board_id) DO UPDATE
            SET is_on = EXCLUDED.is_on, set_by = 'person', updated_at = EXCLUDED.updated_at",
    )
    .bind(principal.account_id)
    .bind(board_id)
    .bind(change.on)
    .bind(state.clock.now())
    .execute(&state.db)
    .await?;
    Ok(Json(MySiteChoice {
        id: board_id,
        on: change.on,
        set_by: SetBy::Person,
    }))
}
