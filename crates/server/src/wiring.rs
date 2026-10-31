//! The ports modules declare for each other, implemented here: the server
//! is the only crate that knows every module, so it is where one module's
//! question meets another's answer. Each adapter only translates between
//! the two modules' types.

use std::collections::HashSet;

use applicamp_kernel::{AppState, BoxFuture, EventBus, Result};
use applicamp_pipeline::{
    ports::{
        AnswerFacts, CvContact, CvEducation, CvFact, CvLanguage, CvPosition, CvProfile, Posting,
        ProfileDigest, ProfileStatus, ProposedFact, RoleDigest, SearchLine, SiteFailure, SitePage,
        SiteReading,
    },
    store::{self as pipeline_store, Eligibility, Found, UncheckedRow},
};
use applicamp_profile::{
    goals::{Period, Relocate, WorkMode as GoalWorkMode},
    matching,
};
use applicamp_sources::{
    SourcesDeps, dedupe,
    job::{BoardJob, Job, Source, SourceLink, WorkMode},
    matching::MatchInput,
    ports::{Attach, KnownLinks, Scored, Unchecked, Unreasoned},
    requirements::{self, Excluded, Facts, Floor, Per, Reach},
};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

/// What "Tell me about you" asks of the pipeline: a past employer is read as
/// any job's company is, with the same site choice, reader, quote check and
/// 30-day cache.
#[derive(Debug, Clone)]
pub struct ResearchWiring {
    /// The pipeline as composed (its reader and its gateway).
    pub pipeline: applicamp_pipeline::PipelineDeps,
}

impl applicamp_profile::ports::EmployerResearch for ResearchWiring {
    fn known_site<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        company: &'a str,
    ) -> BoxFuture<'a, Result<Option<String>>> {
        Box::pin(applicamp_pipeline::features::brief::employer::site_of(
            db, account, company,
        ))
    }

    fn set_site<'a>(
        &'a self,
        state: &'a AppState,
        account: Uuid,
        company: &'a str,
        url: &'a str,
    ) -> BoxFuture<'a, Result<String>> {
        Box::pin(applicamp_pipeline::features::brief::employer::set_site(
            &self.pipeline,
            state,
            account,
            company,
            url,
        ))
    }

    fn run<'a>(
        &'a self,
        state: &'a AppState,
        account: Uuid,
        company: &'a str,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<applicamp_profile::ports::Research>> {
        use applicamp_profile::ports::{FactBasis, ProposedFact, Research};
        Box::pin(async move {
            let found = applicamp_pipeline::features::brief::employer::facts(
                &self.pipeline,
                state,
                account,
                company,
                now,
            )
            .await?;
            Ok(Research {
                site: found.site,
                read: found.read,
                facts: found
                    .facts
                    .into_iter()
                    .map(|fact| ProposedFact {
                        claim: fact.text,
                        status: FactBasis::Sourced,
                        quote: Some(fact.quote),
                        source_url: Some(fact.url),
                        reason: "Read on the company's own website.".to_owned(),
                    })
                    .collect(),
            })
        })
    }
}

/// What the search asks of the profile and the pipeline.
#[derive(Debug, Clone, Copy, Default)]
pub struct SourcesWiring;

/// The profile and goals as a search matches against them.
fn match_input(m: matching::Matching, now: DateTime<Utc>) -> MatchInput {
    let titles = m
        .profile
        .basics
        .label
        .iter()
        .cloned()
        .chain(m.profile.work.iter().filter_map(|w| w.position.clone()))
        .take(6)
        .collect();
    MatchInput {
        profile_version: m.profile_updated_at.to_rfc3339(),
        goals_version: m.goals_version,
        profile: matching::summary(&m.profile, now),
        goals: serde_json::to_value(&m.goals).unwrap_or_default(),
        roles: m.goals.roles.clone(),
        titles,
        work_modes: m
            .goals
            .work_mode
            .iter()
            .map(|mode| match mode {
                GoalWorkMode::Remote => WorkMode::Remote,
                GoalWorkMode::Hybrid => WorkMode::Hybrid,
                GoalWorkMode::Onsite => WorkMode::Onsite,
            })
            .collect(),
        locations: m.goals.locations.clone(),
        will_not_relocate: m.goals.relocate == Some(Relocate::No),
        home: m
            .goals
            .locations
            .first()
            .cloned()
            .or_else(|| m.profile.basics.location.clone()),
        relocate: m.goals.relocate.map(|relocate| match relocate {
            Relocate::No => Reach::No,
            Relocate::WithinCountry => Reach::WithinCountry,
            Relocate::WithinEu => Reach::WithinEu,
            Relocate::Anywhere => Reach::Anywhere,
        }),
        languages: {
            let mut codes: Vec<String> = Vec::new();
            for tag in matching::work_languages(&m.goals, &m.profile) {
                if let Some(code) = requirements::language_code(&tag)
                    && !codes.contains(&code)
                {
                    codes.push(code);
                }
            }
            codes
        },
        skills: m
            .profile
            .skills
            .iter()
            .map(|skill| skill.name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .take(40)
            .collect(),
        cash_floor: m.goals.cash_floor.as_ref().map(|floor| Floor {
            amount: f64::from(floor.amount),
            currency: floor.currency.clone(),
            per: match floor.period {
                Period::Year => Per::Year,
                Period::Month => Per::Month,
            },
        }),
    }
}

/// The facts a search read, as the pipeline keeps them.
fn eligibility(facts: Facts) -> Eligibility {
    Eligibility {
        remote: facts.remote.as_str().to_owned(),
        open_to_candidate: facts.open_to_candidate.as_str().to_owned(),
        region_note: facts.region_note,
        working_languages: facts.working_languages,
    }
}

/// An opportunity found before the rules, as a search reads a job: a
/// reader's source, else a job board's.
fn unchecked(row: UncheckedRow) -> Option<Unchecked> {
    let (source, board) = match Source::parse(&row.source) {
        Some(source) => (source, None),
        None => (
            Source::Board,
            Some(BoardJob {
                slug: row.source.clone(),
                ..BoardJob::default()
            }),
        ),
    };
    Some(Unchecked {
        account: row.account_id,
        opportunity: row.id,
        job: Job {
            board,
            source,
            source_ref: row.source_ref,
            title: row.title,
            company: row.company,
            location: row.location,
            work_mode: row.work_mode.as_deref().and_then(WorkMode::parse),
            board_remote: None,
            url: row.url,
            posted_at: row.posted_at,
            salary_text: row.salary_text,
            description: row.description.unwrap_or_default(),
        },
    })
}

/// A job board's link as the pipeline keeps it.
fn link(link: SourceLink) -> pipeline_store::SourceLink {
    pipeline_store::SourceLink {
        board: link.board,
        name: link.name,
        url: link.url,
    }
}

/// A scored job as the pipeline adds it.
fn found(scored: Scored) -> Found {
    let Scored { job, score } = scored;
    let canonical_url = dedupe::job_url(&job);
    let dedupe_key = dedupe::dedupe_key(&job);
    let source = job.source_name().to_owned();
    let board = job.board;
    Found {
        source,
        apply_url: board.as_ref().and_then(|b| b.apply_url.clone()),
        expires_at: board.as_ref().and_then(|b| b.expires_at),
        canonical_url,
        dedupe_key,
        company_site: board.as_ref().and_then(|b| b.company_url.clone()),
        sources: board
            .map(|b| b.links.into_iter().map(link).collect())
            .unwrap_or_default(),
        source_ref: job.source_ref,
        title: job.title,
        company: job.company,
        location: job.location,
        work_mode: job.work_mode.map(|mode| mode.as_str().to_owned()),
        url: job.url,
        posted_at: job.posted_at,
        salary_text: job.salary_text,
        description: Some(job.description).filter(|d| !d.is_empty()),
        match_score: score.match_score,
        why: score.why,
        gaps: score.gaps,
        eligibility: Some(eligibility(score.facts)),
    }
}

impl applicamp_sources::Neighbours for SourcesWiring {
    fn matching_input<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<Option<MatchInput>>> {
        Box::pin(async move {
            Ok(matching::matching(db, account)
                .await?
                .map(|m| match_input(m, now)))
        })
    }

    fn accounts_with_goals<'a>(&'a self, db: &'a PgPool) -> BoxFuture<'a, Result<Vec<Uuid>>> {
        Box::pin(matching::accounts_with_goals(db))
    }

    fn known_jobs<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<HashSet<(String, String)>>> {
        Box::pin(applicamp_pipeline::store::known(db, account))
    }

    fn known_links<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        since: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<KnownLinks>> {
        Box::pin(async move {
            let (urls, keys) = pipeline_store::known_links(db, account, since).await?;
            Ok(KnownLinks { urls, keys })
        })
    }

    fn attach_sources<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        items: Vec<Attach>,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<u32>> {
        let items = items
            .into_iter()
            .map(|item| pipeline_store::Attach {
                canonical_url: item.canonical_url,
                dedupe_key: item.dedupe_key,
                sources: item.links.into_iter().map(link).collect(),
            })
            .collect();
        Box::pin(pipeline_store::attach_sources(db, account, items, now))
    }

    fn account_email<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<Option<String>>> {
        Box::pin(applicamp_identity::email_of(db, account))
    }

    fn add_found<'a>(
        &'a self,
        db: &'a PgPool,
        events: &'a EventBus,
        account: Uuid,
        found_jobs: Vec<Scored>,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<u32>> {
        let rows: Vec<Found> = found_jobs.into_iter().map(found).collect();
        Box::pin(applicamp_pipeline::store::add_found(
            db, events, account, rows, now,
        ))
    }

    fn unchecked<'a>(
        &'a self,
        db: &'a PgPool,
        most: usize,
    ) -> BoxFuture<'a, Result<Vec<Unchecked>>> {
        Box::pin(async move {
            Ok(applicamp_pipeline::store::unchecked(db, most)
                .await?
                .into_iter()
                .filter_map(unchecked)
                .collect())
        })
    }

    fn set_requirements<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        opportunity: Uuid,
        facts: Facts,
        hidden: Option<Excluded>,
    ) -> BoxFuture<'a, Result<bool>> {
        Box::pin(applicamp_pipeline::store::set_eligibility(
            db,
            account,
            opportunity,
            eligibility(facts),
            hidden.map(Excluded::as_str),
        ))
    }

    fn unreasoned<'a>(
        &'a self,
        db: &'a PgPool,
        after: Uuid,
        most: usize,
    ) -> BoxFuture<'a, Result<Vec<Unreasoned>>> {
        Box::pin(async move {
            Ok(applicamp_pipeline::store::unreasoned(db, after, most)
                .await?
                .into_iter()
                .map(|row| Unreasoned {
                    account: row.account_id,
                    opportunity: row.id,
                    why: row.why.0,
                })
                .collect())
        })
    }

    fn set_reasons<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        opportunity: Uuid,
        why: Vec<String>,
        updated: bool,
    ) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move {
            applicamp_pipeline::store::set_reasons(db, account, opportunity, &why, updated).await
        })
    }
}

/// What the pipeline asks of the sources and the profile.
#[derive(Debug, Clone)]
pub struct PipelineWiring {
    /// The sources module's reader and settings, to read pasted links.
    pub sources: SourcesDeps,
}

impl applicamp_pipeline::Neighbours for PipelineWiring {
    fn read_posting<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        url: &'a Url,
    ) -> BoxFuture<'a, Result<Option<Posting>>> {
        Box::pin(async move {
            let job = applicamp_sources::read_posting(&self.sources, db, account, url).await?;
            Ok(job.map(|job| Posting {
                source: job.source.as_str().to_owned(),
                source_ref: job.source_ref,
                title: job.title,
                company: job.company,
                location: job.location,
                work_mode: job.work_mode.map(|mode| mode.as_str().to_owned()),
                url: job.url,
                posted_at: job.posted_at,
                salary_text: job.salary_text,
                description: Some(job.description).filter(|d| !d.is_empty()),
            }))
        })
    }

    fn profile_status<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<ProfileStatus>> {
        Box::pin(async move {
            let status = matching::status(db, account).await?;
            Ok(ProfileStatus {
                profile_done: status.profile_done,
                strength_hint: status.strength_hint.map(|h| h.as_str().to_owned()),
                no_skills: status.no_skills,
                goals_answered: status.goals_answered,
                goals_total: status.goals_total,
                strength: status.strength,
                next_action: status.next_action.map(|a| a.as_str().to_owned()),
                questions_left: status.questions_left,
            })
        })
    }

    fn latest_search<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<Option<SearchLine>>> {
        Box::pin(async move {
            let row = applicamp_sources::latest_row(db, account).await?;
            Ok(row.map(|row| SearchLine {
                id: row.id,
                status: row.status,
                trigger: row.trigger,
                started_at: row.started_at,
                finished_at: row.finished_at,
                fetched: row.fetched,
                added: row.added,
                stopped: row.error_kind,
            }))
        })
    }

    fn site_allowed(&self, site: &Url) -> bool {
        applicamp_sources::net::check(site, self.sources.config.allow_private_hosts()).is_ok()
    }

    fn read_site<'a>(
        &'a self,
        site: &'a Url,
        posting: Option<&'a Url>,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<SiteReading, SiteFailure>> {
        Box::pin(async move {
            let Some(reader) = &self.sources.reader else {
                return Err(SiteFailure::Unreachable);
            };
            match applicamp_sources::pages::read_site(reader, site, posting, now).await {
                Ok(read) => Ok(SiteReading {
                    pages: read
                        .pages
                        .into_iter()
                        .map(|page| SitePage {
                            url: page.url.to_string(),
                            fetched_at: page.fetched_at,
                            kind: page.kind.as_str().to_owned(),
                            text: page.text,
                        })
                        .collect(),
                    blocked: read.blocked,
                }),
                Err(applicamp_sources::pages::Failure::Unreachable) => {
                    Err(SiteFailure::Unreachable)
                }
                Err(applicamp_sources::pages::Failure::RobotsDisallows) => {
                    Err(SiteFailure::RobotsDisallows)
                }
                Err(applicamp_sources::pages::Failure::NothingReadable) => {
                    Err(SiteFailure::NothingReadable)
                }
            }
        })
    }

    fn profile_digest<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<Option<ProfileDigest>>> {
        Box::pin(async move {
            Ok(matching::digest(db, account)
                .await?
                .map(|digest| ProfileDigest {
                    roles: digest
                        .roles
                        .into_iter()
                        .map(|role| RoleDigest {
                            index: role.index,
                            position: role.position,
                            company: role.company,
                            start_date: role.start_date,
                            end_date: role.end_date,
                            facts: role.facts,
                        })
                        .collect(),
                    skills: digest.skills,
                    latest: digest.latest,
                    dismissed_skills: digest.dismissed_skills,
                }))
        })
    }

    fn answer_about<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        now: DateTime<Utc>,
        position: usize,
        question: &'a str,
        answer: &'a str,
        locale: Option<&'a str>,
    ) -> BoxFuture<'a, Result<AnswerFacts>> {
        Box::pin(async move {
            let gateway = self
                .sources
                .ai
                .as_ref()
                .filter(|ai| ai.is_configured(applicamp_ai::Role::Complete))
                .ok_or(applicamp_kernel::Error::Unavailable(
                    applicamp_ai::AI_UNAVAILABLE,
                ))?;
            let answered = applicamp_profile::features::grill::about::propose(
                db, gateway, account, now, position, question, answer, locale,
            )
            .await?;
            Ok(AnswerFacts {
                position: usize::try_from(answered.position)
                    .map_err(applicamp_kernel::Error::internal)?,
                title: answered.role.position,
                company: answered.role.company,
                facts: answered
                    .proposals
                    .into_iter()
                    .map(|proposal| ProposedFact {
                        id: proposal.id,
                        text: proposal.text,
                        quote: proposal.quote,
                        area: proposal.area.as_str().to_owned(),
                        expires_at: proposal.expires_at,
                    })
                    .collect(),
                dropped: answered.dropped,
            })
        })
    }

    fn dismiss_skill<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        skill: &'a str,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(matching::dismiss_skill(db, account, skill, now))
    }

    fn cv_profile<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
    ) -> BoxFuture<'a, Result<Option<CvProfile>>> {
        Box::pin(async move {
            let Some(profile) = matching::current_profile(db, account).await? else {
                return Ok(None);
            };
            Ok(Some(cv_profile_of(profile)))
        })
    }

    fn keep_fact<'a>(
        &'a self,
        state: &'a AppState,
        account: Uuid,
        position: usize,
        title: Option<&'a str>,
        company: Option<&'a str>,
        text: &'a str,
        area: &'a str,
    ) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            let area = applicamp_profile::schema::Area::parse(area)
                .unwrap_or(applicamp_profile::schema::Area::Results);
            matching::keep_fact(state, account, position, title, company, text, area).await
        })
    }

    fn add_skill<'a>(
        &'a self,
        state: &'a AppState,
        account: Uuid,
        skill: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(matching::add_skill(state, account, skill))
    }
}

/// A profile as the CV is made from it: the contact apart from the rest.
fn cv_profile_of(profile: applicamp_profile::schema::Profile) -> CvProfile {
    let latest = matching::latest_position(&profile.work);
    CvProfile {
        contact: CvContact {
            name: profile.basics.name,
            email: profile.basics.email,
            location: profile.basics.location,
            url: profile.basics.url,
            links: profile
                .basics
                .profiles
                .into_iter()
                .map(|link| (link.network, link.url))
                .collect(),
        },
        headline: profile.basics.label,
        summary: profile.basics.summary,
        positions: profile
            .work
            .into_iter()
            .enumerate()
            .map(|(index, work)| CvPosition {
                index,
                title: work.position,
                company: work.name,
                location: work.location,
                start_date: work.start_date,
                end_date: work.end_date,
                summary: work.summary,
                highlights: work.highlights,
                facts: work
                    .facts
                    .into_iter()
                    .map(|fact| CvFact {
                        id: fact.id,
                        text: fact.text,
                    })
                    .collect(),
            })
            .collect(),
        skills: profile
            .skills
            .into_iter()
            .map(|skill| skill.name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect(),
        education: profile
            .education
            .into_iter()
            .map(|e| CvEducation {
                institution: e.institution,
                area: e.area,
                study_type: e.study_type,
                start_date: e.start_date,
                end_date: e.end_date,
            })
            .collect(),
        languages: profile
            .languages
            .into_iter()
            .map(|l| CvLanguage {
                language: l.language,
                fluency: l.fluency,
            })
            .collect(),
        latest,
    }
}

/// Starting runs for any module (the kernel's `RunStarter` port), over the
/// runs module: a run of a kind this instance executes, queued in the
/// caller's transaction.
#[derive(Debug, Clone)]
pub struct RunsWiring {
    kinds: std::sync::Arc<applicamp_runs::Kinds>,
}

impl RunsWiring {
    /// Starting runs of the kinds `runs` executes.
    pub fn new(runs: &applicamp_runs::RunsDeps) -> Self {
        Self {
            kinds: std::sync::Arc::clone(&runs.kinds),
        }
    }
}

impl applicamp_kernel::agent::RunStarter for RunsWiring {
    fn start_run<'a>(
        &'a self,
        conn: &'a mut sqlx::PgConnection,
        run: applicamp_kernel::agent::StartRun,
    ) -> BoxFuture<'a, Result<Uuid>> {
        Box::pin(async move {
            if self.kinds.get(&run.kind).is_none() {
                return Err(applicamp_kernel::Error::BadRequest(
                    "unknown_kind",
                    format!("No run kind `{}` here.", run.kind),
                ));
            }
            Ok(applicamp_runs::store::queue(
                conn,
                run.account,
                &run.kind,
                run.subject,
                &run.inputs,
                Utc::now(),
            )
            .await?)
        })
    }
}

#[cfg(test)]
mod tests {
    use applicamp_profile::{
        goals::{CashFloor, Goals},
        schema::{Basics, Language, Profile, Work},
    };
    use applicamp_sources::{requirements::Answer, scoring::Score};

    use super::*;

    #[test]
    fn goals_become_the_searchs_hard_limits() {
        let now = Utc::now();
        let input = match_input(
            matching::Matching {
                profile: Profile {
                    basics: Basics {
                        name: Some("Alex Demo".into()),
                        label: Some("Engineer".into()),
                        location: Some("Porto, Portugal".into()),
                        ..Basics::default()
                    },
                    work: vec![Work {
                        position: Some("Senior Engineer".into()),
                        ..Work::default()
                    }],
                    languages: vec![
                        Language {
                            language: "Portuguese".into(),
                            fluency: Some("Native or bilingual proficiency".into()),
                        },
                        Language {
                            language: "English".into(),
                            fluency: Some("Full professional proficiency".into()),
                        },
                        Language {
                            language: "German".into(),
                            fluency: Some("Elementary proficiency".into()),
                        },
                    ],
                    ..Profile::default()
                },
                profile_updated_at: now,
                goals: Goals {
                    roles: vec!["Staff engineer".into()],
                    work_mode: vec![GoalWorkMode::Remote],
                    locations: vec!["Lisbon".into()],
                    relocate: Some(Relocate::No),
                    cash_floor: Some(CashFloor {
                        amount: 4_000,
                        currency: "EUR".into(),
                        period: Period::Month,
                    }),
                    ..Goals::default()
                },
                goals_version: 4,
            },
            now,
        );
        assert_eq!(input.goals_version, 4);
        assert_eq!(input.work_modes, [WorkMode::Remote]);
        assert!(input.will_not_relocate);
        assert_eq!(input.titles, ["Engineer", "Senior Engineer"]);
        assert!(!input.profile.to_string().contains("Alex"));
        // The requirements: where they live (the goals' first place over
        // the profile's), how far they would move, the languages they work
        // in (the profile's, at a professional level), the floor.
        assert_eq!(input.home.as_deref(), Some("Lisbon"));
        assert_eq!(input.relocate, Some(Reach::No));
        assert_eq!(input.languages, ["pt", "en"]);
        assert_eq!(
            input.cash_floor,
            Some(Floor {
                amount: 4_000.0,
                currency: "EUR".into(),
                per: Per::Month
            })
        );
        assert!(input.remote_only());
    }

    #[test]
    fn a_scored_job_keeps_its_reasons() {
        let found = found(Scored {
            job: Job {
                board: None,
                source: Source::Ashby,
                source_ref: "a".into(),
                title: "Staff Engineer".into(),
                company: "Example Co".into(),
                location: None,
                work_mode: Some(WorkMode::Hybrid),
                board_remote: Some(false),
                url: "https://example.com".into(),
                posted_at: None,
                salary_text: None,
                description: String::new(),
            },
            score: Score {
                match_score: 77,
                why: vec!["Rust".into()],
                gaps: vec![],
                read: None,
                facts: Facts {
                    remote: Answer::No,
                    open_to_candidate: Answer::Yes,
                    region_note: Some("Lisbon office".into()),
                    working_languages: vec!["en".into()],
                    salary: None,
                },
                ungrounded: false,
            },
        });
        assert_eq!(found.source, "ashby");
        assert!(
            found.sources.is_empty(),
            "a company board credits no job board"
        );
        assert_eq!(found.work_mode.as_deref(), Some("hybrid"));
        assert_eq!(found.description, None);
        assert_eq!(found.match_score, 77);
        assert_eq!(found.why, ["Rust"]);
        let eligibility = found.eligibility.unwrap();
        assert_eq!(eligibility.remote, "no");
        assert_eq!(eligibility.open_to_candidate, "yes");
        assert_eq!(eligibility.working_languages, ["en"]);
    }
}
