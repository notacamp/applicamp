//! Which companies to suggest, and why, without a model.
//!
//! A company is suggested when, in the last [`WEEKS`] weeks, the person has
//! at least [`MIN_MATCHES`] matches from it, or one they shortlisted. Every
//! reason is worked out from the postings themselves (their own fields) and
//! points at the posting it comes from; company facts already read for a CV
//! are cited by their id. Nothing is written by a model.

use std::collections::{BTreeMap, HashMap};

use applicamp_kernel::Result;
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use url::Url;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    SourcesDeps,
    matching::{job_words, names_skill, skill_words},
    ports::{CompanyFact, MatchedPosting},
};

use super::{
    detect::{self, BoardKind},
    store,
};

/// How far back a match counts.
pub const WEEKS: i64 = 8;
/// Matches from one company that make it a suggestion (one shortlisted job
/// is enough on its own).
pub const MIN_MATCHES: usize = 2;
/// Suggestions are worked out again when older than this.
pub const STALE: TimeDelta = TimeDelta::hours(1);
/// Postings cited by one reason, at most.
const MOST_CITED: usize = 5;
/// Skills named by the stack reason, at most.
const MOST_SKILLS: usize = 3;

/// What a reason says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompanyReasonKind {
    /// You shortlisted one of their jobs.
    Shortlisted,
    /// `count` of their jobs fit you in `weeks` weeks.
    Matches,
    /// Their postings name `values` (skills on your profile).
    Stack,
    /// `count` of `total` postings are remote.
    Remote,
    /// The places the postings say they are open to (`values`).
    Region,
    /// The pay the postings state (`values`).
    Pay,
    /// A fact read on their own site for a CV (`values`).
    Fact,
}

/// What a reason is traced to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompanyReasonSourceKind {
    /// One of the person's matches (an opportunity).
    Posting,
    /// A fact read on the company's own site.
    Fact,
}

/// Where a reason comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompanyReasonSource {
    pub kind: CompanyReasonSourceKind,
    /// The opportunity, or the fact.
    pub id: Uuid,
    /// The posting's role, or the company.
    pub label: String,
    /// How well the posting fits, 0 to 100.
    pub score: Option<u8>,
}

/// One reason a company is suggested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompanyReason {
    pub kind: CompanyReasonKind,
    pub count: u32,
    pub total: u32,
    pub weeks: u32,
    pub values: Vec<String>,
    pub sources: Vec<CompanyReasonSource>,
}

/// A company with enough matches, and what they say.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// `site:<host>`.
    pub key: String,
    pub name: String,
    /// The company's own site (an origin).
    pub site: Url,
    /// Its matches in the window, best first.
    pub postings: Vec<MatchedPosting>,
}

/// Group `postings` by company site and keep the companies with enough
/// matches in the last [`WEEKS`] weeks. Companies with no site, whose site
/// is a job board or a network (LinkedIn, Indeed, Glassdoor, an applicant
/// tracking system), or that are one of those themselves, are left out.
pub fn candidates(postings: Vec<MatchedPosting>, now: DateTime<Utc>) -> Vec<Candidate> {
    let since = now - TimeDelta::weeks(WEEKS);
    let mut by_site: BTreeMap<String, (Url, Vec<MatchedPosting>)> = BTreeMap::new();
    for posting in postings.into_iter().filter(|p| p.found_at >= since) {
        let Some(site) = posting.site.as_deref().and_then(|s| Url::parse(s).ok()) else {
            continue;
        };
        let Some(key) = detect::site_key(&site) else {
            continue;
        };
        if crate::job_boards::host_of(&site).is_none_or(|h| detect::is_not_a_company_site(&h))
            || is_a_banned_name(&posting.company)
        {
            continue;
        }
        by_site
            .entry(key)
            .or_insert_with(|| (site, Vec::new()))
            .1
            .push(posting);
    }
    by_site
        .into_iter()
        .filter(|(_, (_, found))| found.len() >= MIN_MATCHES || found.iter().any(|p| p.shortlisted))
        .map(|(key, (site, mut found))| {
            found.sort_by(|a, b| {
                b.match_score
                    .cmp(&a.match_score)
                    .then(b.found_at.cmp(&a.found_at))
            });
            // The newest posting names the company.
            let name = found
                .iter()
                .max_by_key(|p| p.found_at)
                .map_or_else(String::new, |p| p.company.trim().to_owned());
            Candidate {
                key,
                name,
                site,
                postings: found,
            }
        })
        .filter(|c| !c.name.is_empty())
        .collect()
}

/// A company that is itself a site the app never reads (ADR 0006).
fn is_a_banned_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["linkedin", "indeed", "glassdoor"]
        .iter()
        .any(|banned| name.contains(banned))
}

fn cite(posting: &MatchedPosting) -> CompanyReasonSource {
    CompanyReasonSource {
        kind: CompanyReasonSourceKind::Posting,
        id: posting.opportunity,
        label: posting.title.clone(),
        score: Some(posting.match_score),
    }
}

fn blank(kind: CompanyReasonKind) -> CompanyReason {
    CompanyReason {
        kind,
        count: 0,
        total: 0,
        weeks: 0,
        values: Vec::new(),
        sources: Vec::new(),
    }
}

fn distinct(values: impl Iterator<Item = String>, most: usize) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for value in values {
        let value = value.trim().to_owned();
        if !value.is_empty() && !seen.iter().any(|s| s.eq_ignore_ascii_case(&value)) {
            seen.push(value);
        }
        if seen.len() == most {
            break;
        }
    }
    seen
}

/// The reasons `candidate` is suggested: each from its postings' own fields
/// (or a fact already read on its site), each citing where it comes from.
/// `skills` are the person's, in their order.
pub fn reasons(
    candidate: &Candidate,
    skills: &[String],
    facts: &[CompanyFact],
    now: DateTime<Utc>,
) -> Vec<CompanyReason> {
    let postings = &candidate.postings;
    let total = u32::try_from(postings.len()).unwrap_or(u32::MAX);
    let mut out = Vec::new();

    let shortlisted: Vec<&MatchedPosting> = postings.iter().filter(|p| p.shortlisted).collect();
    if !shortlisted.is_empty() {
        out.push(CompanyReason {
            count: u32::try_from(shortlisted.len()).unwrap_or(u32::MAX),
            sources: shortlisted
                .iter()
                .take(MOST_CITED)
                .map(|p| cite(p))
                .collect(),
            ..blank(CompanyReasonKind::Shortlisted)
        });
    }

    if postings.len() >= MIN_MATCHES {
        let oldest = postings.iter().map(|p| p.found_at).min().unwrap_or(now);
        let days = (now - oldest).num_days().max(0);
        let weeks = u32::try_from((days + 6) / 7)
            .unwrap_or(1)
            .clamp(1, WEEKS as u32);
        out.push(CompanyReason {
            count: total,
            weeks,
            sources: postings.iter().take(MOST_CITED).map(cite).collect(),
            ..blank(CompanyReasonKind::Matches)
        });
    }

    // The person's skills the postings name (in at least half of them).
    let words: Vec<_> = postings
        .iter()
        .map(|p| job_words(&p.title, &p.description))
        .collect();
    let mut named: Vec<(String, Vec<usize>)> = Vec::new();
    for skill in skills.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if named.iter().any(|(n, _)| n.eq_ignore_ascii_case(skill)) {
            continue;
        }
        let Some(wanted) = skill_words(&[skill]).into_iter().next() else {
            continue;
        };
        let in_postings: Vec<usize> = words
            .iter()
            .enumerate()
            .filter(|(_, w)| names_skill(w, &wanted))
            .map(|(i, _)| i)
            .collect();
        if !in_postings.is_empty() && in_postings.len() * 2 >= postings.len() {
            named.push((skill.to_owned(), in_postings));
        }
        if named.len() == MOST_SKILLS {
            break;
        }
    }
    if !named.is_empty() {
        let mut cited: Vec<usize> = named.iter().flat_map(|(_, i)| i.iter().copied()).collect();
        cited.sort_unstable();
        cited.dedup();
        out.push(CompanyReason {
            values: named.iter().map(|(s, _)| s.clone()).collect(),
            total,
            count: u32::try_from(cited.len()).unwrap_or(u32::MAX),
            sources: cited
                .into_iter()
                .take(MOST_CITED)
                .filter_map(|i| postings.get(i).map(cite))
                .collect(),
            ..blank(CompanyReasonKind::Stack)
        });
    }

    let remote: Vec<&MatchedPosting> = postings
        .iter()
        .filter(|p| p.work_mode.as_deref() == Some("remote"))
        .collect();
    if !remote.is_empty() {
        out.push(CompanyReason {
            count: u32::try_from(remote.len()).unwrap_or(u32::MAX),
            total,
            sources: remote.iter().take(MOST_CITED).map(|p| cite(p)).collect(),
            ..blank(CompanyReasonKind::Remote)
        });
    }

    let place = |p: &MatchedPosting| p.region_note.clone().or_else(|| p.location.clone());
    let regions = distinct(postings.iter().filter_map(place), 3);
    if !regions.is_empty() {
        out.push(CompanyReason {
            sources: postings
                .iter()
                .filter(|p| {
                    place(p)
                        .is_some_and(|v| regions.iter().any(|r| r.eq_ignore_ascii_case(v.trim())))
                })
                .take(MOST_CITED)
                .map(cite)
                .collect(),
            values: regions,
            ..blank(CompanyReasonKind::Region)
        });
    }

    let pay = distinct(postings.iter().filter_map(|p| p.salary_text.clone()), 2);
    if !pay.is_empty() {
        out.push(CompanyReason {
            sources: postings
                .iter()
                .filter(|p| {
                    p.salary_text
                        .as_deref()
                        .is_some_and(|v| pay.iter().any(|x| x.eq_ignore_ascii_case(v.trim())))
                })
                .take(MOST_CITED)
                .map(cite)
                .collect(),
            values: pay,
            ..blank(CompanyReasonKind::Pay)
        });
    }

    for fact in facts.iter().take(2) {
        out.push(CompanyReason {
            values: vec![fact.text.clone()],
            sources: vec![CompanyReasonSource {
                kind: CompanyReasonSourceKind::Fact,
                id: fact.id,
                label: fact.company.clone(),
                score: None,
            }],
            ..blank(CompanyReasonKind::Fact)
        });
    }
    out
}

/// Work out `account`'s suggestions from their matches now: companies with
/// enough matches become suggestions (with reasons), the suggestions whose
/// matches aged out go. A company already watched, dismissed or stopped
/// stays as it is. Whether anything was worked out (a person without a
/// profile has no matches to read).
pub async fn refresh(
    deps: &SourcesDeps,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<bool> {
    let Some(neighbours) = &deps.neighbours else {
        return Ok(false);
    };
    let input = neighbours.matching_input(db, account, now).await?;
    let postings = neighbours
        .matched_postings(db, account, now - TimeDelta::weeks(WEEKS))
        .await?;
    let found = candidates(postings, now);
    let names: Vec<String> = found.iter().map(|c| c.name.clone()).collect();
    let facts = if names.is_empty() {
        Vec::new()
    } else {
        neighbours.company_facts(db, account, names).await?
    };
    let skills = input.map(|i| i.skills).unwrap_or_default();
    let mut by_company: HashMap<String, Vec<CompanyFact>> = HashMap::new();
    for fact in facts {
        by_company
            .entry(fact.company.to_lowercase())
            .or_default()
            .push(fact);
    }
    let mut tx = db.begin().await?;
    let mut keep = Vec::new();
    for candidate in &found {
        let own = by_company
            .get(&candidate.name.to_lowercase())
            .map_or(&[][..], Vec::as_slice);
        let reasons = reasons(candidate, &skills, own, now);
        // A posting that came from a Greenhouse, Lever or Ashby board says
        // which board the company has.
        let links: Vec<&str> = candidate
            .postings
            .iter()
            .flat_map(|p| [Some(p.url.as_str()), p.apply_url.as_deref()])
            .flatten()
            .collect();
        let hint = detect::hint_of(&links);
        let id = store::ensure_company(
            &mut tx,
            &store::NewCompany {
                key: &candidate.key,
                name: &candidate.name,
                website: Some(candidate.site.as_str()),
                careers_url: None,
                origin: "matches",
                hint: hint
                    .as_ref()
                    .map(|h| (h.kind, h.token.as_str(), h.eu))
                    .filter(|(kind, _, _)| *kind != BoardKind::None),
            },
            now,
        )
        .await?;
        if store::suggest(&mut tx, account, id, &reasons, now).await? {
            keep.push(id);
        }
    }
    store::drop_stale_suggestions(&mut tx, account, &keep).await?;
    store::set_refreshed(&mut tx, account, now).await?;
    tx.commit().await?;
    Ok(true)
}

/// [`refresh`], when the last one is older than [`STALE`].
pub async fn refresh_if_stale(
    deps: &SourcesDeps,
    db: &PgPool,
    account: Uuid,
    now: DateTime<Utc>,
) -> Result<()> {
    let fresh = store::refreshed_at(db, account)
        .await?
        .is_some_and(|at| now - at < STALE);
    if !fresh {
        refresh(deps, db, account, now).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 2, 12, 0, 0).unwrap()
    }

    fn posting(company: &str, site: &str, days_ago: i64, score: u8) -> MatchedPosting {
        MatchedPosting {
            opportunity: Uuid::now_v7(),
            company: company.to_owned(),
            site: Some(site.to_owned()),
            title: format!("Senior Rails Engineer {days_ago}"),
            url: "https://example.com/jobs/1".into(),
            apply_url: None,
            match_score: score,
            shortlisted: false,
            found_at: now() - TimeDelta::days(days_ago),
            work_mode: Some("remote".into()),
            location: Some("Remote - Europe".into()),
            region_note: None,
            salary_text: Some("EUR 85000–100000 per year".into()),
            description: "Ruby on Rails and PostgreSQL, in a small team.".into(),
        }
    }

    #[test]
    fn two_matches_in_eight_weeks_make_a_suggestion_not_one() {
        let two = vec![
            posting("Fabrikam Pay", "https://fabrikam.test/", 3, 84),
            posting("Fabrikam Pay", "https://fabrikam.test/", 20, 81),
        ];
        assert_eq!(candidates(two, now()).len(), 1);
        let one = vec![posting("Fabrikam Pay", "https://fabrikam.test/", 3, 84)];
        assert!(candidates(one, now()).is_empty());
    }

    #[test]
    fn nine_weeks_ago_is_too_long_ago() {
        let old = vec![
            posting("Fabrikam Pay", "https://fabrikam.test/", 3, 84),
            posting("Fabrikam Pay", "https://fabrikam.test/", 9 * 7, 81),
        ];
        assert!(candidates(old, now()).is_empty(), "one inside, one outside");
        let edge = vec![
            posting("Fabrikam Pay", "https://fabrikam.test/", 3, 84),
            posting("Fabrikam Pay", "https://fabrikam.test/", 8 * 7 - 1, 81),
        ];
        assert_eq!(candidates(edge, now()).len(), 1);
    }

    #[test]
    fn one_shortlisted_job_is_enough() {
        let mut one = posting("Example Co", "https://example.test/", 5, 80);
        one.shortlisted = true;
        let found = candidates(vec![one], now());
        assert_eq!(found.len(), 1);
        let reasons = reasons(&found[0], &[], &[], now());
        assert_eq!(reasons[0].kind, CompanyReasonKind::Shortlisted);
        assert_eq!(reasons[0].sources[0].kind, CompanyReasonSourceKind::Posting);
    }

    #[test]
    fn no_site_no_suggestion_and_never_linkedin_indeed_or_glassdoor() {
        let mut nosite = vec![
            posting("Nosite", "https://nosite.test/", 3, 84),
            posting("Nosite", "https://nosite.test/", 4, 84),
        ];
        for p in &mut nosite {
            p.site = None;
        }
        assert!(candidates(nosite, now()).is_empty());
        for (name, site) in [
            ("LinkedIn", "https://www.linkedin.com/"),
            ("Indeed", "https://www.indeed.com/"),
            ("Glassdoor", "https://www.glassdoor.com/"),
            ("Acme", "https://boards.greenhouse.io/"),
            ("Acme via LinkedIn", "https://acme.test/"),
        ] {
            let found = vec![posting(name, site, 3, 90), posting(name, site, 4, 90)];
            assert!(candidates(found, now()).is_empty(), "{name}");
        }
    }

    #[test]
    fn reasons_are_traced_to_the_postings_own_fields() {
        let found = candidates(
            vec![
                posting("Fabrikam Pay", "https://fabrikam.test/", 3, 84),
                posting("Fabrikam Pay", "https://fabrikam.test/", 30, 81),
                posting("Fabrikam Pay", "https://fabrikam.test/", 10, 72),
            ],
            now(),
        );
        let skills = vec![
            "Ruby on Rails".to_owned(),
            "Kafka".to_owned(),
            "PostgreSQL".to_owned(),
        ];
        let facts = [CompanyFact {
            id: Uuid::now_v7(),
            company: "Fabrikam Pay".into(),
            text: "Builds payments for small shops".into(),
        }];
        let reasons = reasons(&found[0], &skills, &facts, now());
        let kinds: Vec<CompanyReasonKind> = reasons.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            [
                CompanyReasonKind::Matches,
                CompanyReasonKind::Stack,
                CompanyReasonKind::Remote,
                CompanyReasonKind::Region,
                CompanyReasonKind::Pay,
                CompanyReasonKind::Fact,
            ]
        );
        assert_eq!(reasons[0].count, 3);
        assert_eq!(reasons[0].weeks, 5, "30 days is 5 weeks");
        // Kafka is on the profile but in no posting: never claimed.
        assert_eq!(reasons[1].values, ["Ruby on Rails", "PostgreSQL"]);
        assert_eq!(reasons[2].count, 3);
        assert_eq!(reasons[3].values, ["Remote - Europe"]);
        assert_eq!(reasons[4].values, ["EUR 85000–100000 per year"]);
        // Every reason cites something that exists.
        let ids: Vec<Uuid> = found[0].postings.iter().map(|p| p.opportunity).collect();
        for reason in &reasons[..5] {
            assert!(!reason.sources.is_empty());
            assert!(reason.sources.iter().all(|s| ids.contains(&s.id)));
        }
        assert_eq!(reasons[5].sources[0].kind, CompanyReasonSourceKind::Fact);
        assert_eq!(reasons[5].sources[0].id, facts[0].id);
    }
}
