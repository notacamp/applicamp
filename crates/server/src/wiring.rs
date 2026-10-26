//! The ports modules declare for each other, implemented here: the server
//! is the only crate that knows every module, so it is where one module's
//! question meets another's answer. Each adapter only translates between
//! the two modules' types.

use std::collections::HashSet;

use applicamp_kernel::{BoxFuture, EventBus, Result};
use applicamp_pipeline::{
    ports::{Posting, ProfileStatus, SearchLine},
    store::Found,
};
use applicamp_profile::{
    goals::{Relocate, WorkMode as GoalWorkMode},
    matching,
};
use applicamp_sources::{SourcesDeps, job::WorkMode, matching::MatchInput, ports::Scored};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

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
    }
}

/// A scored job as the pipeline adds it.
fn found(scored: Scored) -> Found {
    let Scored { job, score } = scored;
    Found {
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
        match_score: score.match_score,
        why: score.why,
        gaps: score.gaps,
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
                goals_answered: status.goals_answered,
                goals_total: status.goals_total,
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
}

#[cfg(test)]
mod tests {
    use applicamp_profile::{
        goals::Goals,
        schema::{Basics, Profile, Work},
    };
    use applicamp_sources::{
        job::{Job, Source},
        scoring::Score,
    };

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
                        ..Basics::default()
                    },
                    work: vec![Work {
                        position: Some("Senior Engineer".into()),
                        ..Work::default()
                    }],
                    ..Profile::default()
                },
                profile_updated_at: now,
                goals: Goals {
                    roles: vec!["Staff engineer".into()],
                    work_mode: vec![GoalWorkMode::Remote],
                    locations: vec!["Lisbon".into()],
                    relocate: Some(Relocate::No),
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
    }

    #[test]
    fn a_scored_job_keeps_its_reasons() {
        let found = found(Scored {
            job: Job {
                source: Source::Ashby,
                source_ref: "a".into(),
                title: "Staff Engineer".into(),
                company: "Example Co".into(),
                location: None,
                work_mode: Some(WorkMode::Hybrid),
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
            },
        });
        assert_eq!(found.source, "ashby");
        assert_eq!(found.work_mode.as_deref(), Some("hybrid"));
        assert_eq!(found.description, None);
        assert_eq!(found.match_score, 77);
        assert_eq!(found.why, ["Rust"]);
    }
}
