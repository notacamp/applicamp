//! The job queue: durable background work in Postgres (`kernel.jobs`).
//!
//! A job is enqueued in the caller's transaction, so it exists only if the
//! change that asked for it commits. Workers claim one job at a time with
//! `FOR UPDATE SKIP LOCKED` and a lease, run it, and mark it succeeded, or
//! retry it with exponential backoff until `MAX_ATTEMPTS`, then mark it
//! failed. A job whose worker died is claimed again once its lease ends, so
//! delivery is at least once: handlers must be idempotent.
//!
//! Event deliveries ([`crate::events`]) are jobs too: one per consumer,
//! enqueued with the event.

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc, time::Duration};

use serde::{Serialize, de::DeserializeOwned};
use sqlx::{PgConnection, PgPool};
use tokio::sync::watch;
use uuid::Uuid;

use crate::events::{DomainEvent, Event, EventBus, EventHandler};

/// Attempts before a job is marked failed, unless it says otherwise.
pub const DEFAULT_MAX_ATTEMPTS: u32 = 10;
/// The queue of jobs that name none.
pub const DEFAULT_QUEUE: &str = "default";
/// How long a claimed job is leased to its worker (and may run).
pub const LEASE: Duration = Duration::from_secs(60);
/// How long an idle worker waits before it looks again.
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Longest wait between two attempts.
pub const MAX_BACKOFF: Duration = Duration::from_secs(3600);
/// How long succeeded jobs are kept.
pub const RETENTION: Duration = Duration::from_secs(7 * 24 * 3600);

/// Why a job failed.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// Worth another attempt (a dependency was down).
    #[error("{0}")]
    Retry(String),
    /// Retrying cannot help (a payload that does not parse): failed now.
    #[error("{0}")]
    Permanent(String),
}

impl From<sqlx::Error> for JobError {
    fn from(err: sqlx::Error) -> Self {
        Self::Retry(format!("database: {err}"))
    }
}

impl From<crate::Error> for JobError {
    fn from(err: crate::Error) -> Self {
        Self::Retry(err.code().to_owned())
    }
}

/// What a running job can reach.
#[derive(Debug, Clone)]
pub struct JobContext {
    /// The database pool.
    pub db: PgPool,
    /// Publishes events (from the job's own transactions).
    pub events: EventBus,
    /// The job's id.
    pub job_id: Uuid,
    /// Which attempt this is, from 1.
    pub attempt: u32,
    /// The event a delivery carries.
    pub event_id: Option<Uuid>,
}

/// A kind of background work. The value is the payload, stored as JSON:
/// ids and counts only, never personal data.
pub trait Job: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Stable kind name, `<module>.<job>`; never rename it.
    const NAME: &'static str;
    /// The queue it runs on.
    const QUEUE: &'static str = DEFAULT_QUEUE;
    /// Attempts before it is marked failed.
    const MAX_ATTEMPTS: u32 = DEFAULT_MAX_ATTEMPTS;

    /// Do the work.
    fn run(self, ctx: JobContext) -> impl Future<Output = Result<(), JobError>> + Send;
}

type BoxFuture = Pin<Box<dyn Future<Output = Result<(), JobError>> + Send>>;
type Runner = Arc<dyn Fn(JobContext, serde_json::Value) -> BoxFuture + Send + Sync>;

/// One registered kind of work.
#[derive(Clone)]
struct Entry {
    kind: &'static str,
    queue: &'static str,
    max_attempts: u32,
    /// For an event handler, the event it consumes.
    event: Option<&'static str>,
    runner: Runner,
}

/// A module's background work: its jobs and event handlers.
#[derive(Clone, Default)]
pub struct Background {
    entries: Vec<Entry>,
    publishes: Vec<&'static str>,
}

impl std::fmt::Debug for Background {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Background")
            .field(
                "kinds",
                &self.entries.iter().map(|e| e.kind).collect::<Vec<_>>(),
            )
            .field("publishes", &self.publishes)
            .finish()
    }
}

impl Background {
    /// No background work.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register job kind `J`.
    #[must_use]
    pub fn job<J: Job>(mut self) -> Self {
        let runner: Runner = Arc::new(|ctx: JobContext, payload: serde_json::Value| -> BoxFuture {
            Box::pin(async move {
                let job: J = serde_json::from_value(payload)
                    .map_err(|e| JobError::Permanent(format!("payload: {e}")))?;
                job.run(ctx).await
            })
        });
        self.entries.push(Entry {
            kind: J::NAME,
            queue: J::QUEUE,
            max_attempts: J::MAX_ATTEMPTS,
            event: None,
            runner,
        });
        self
    }

    /// Register `H` as a consumer of its event.
    #[must_use]
    pub fn on_event<H: EventHandler>(mut self) -> Self {
        let runner: Runner = Arc::new(|ctx: JobContext, payload: serde_json::Value| -> BoxFuture {
            Box::pin(async move {
                let id = ctx
                    .event_id
                    .ok_or_else(|| JobError::Permanent("a delivery without its event".into()))?;
                let payload: H::Event = serde_json::from_value(payload)
                    .map_err(|e| JobError::Permanent(format!("event payload: {e}")))?;
                H::handle(ctx, Event { id, payload }).await
            })
        });
        self.entries.push(Entry {
            kind: H::NAME,
            queue: H::QUEUE,
            max_attempts: H::MAX_ATTEMPTS,
            event: Some(<H::Event as DomainEvent>::NAME),
            runner,
        });
        self
    }

    /// Declare that the module publishes `E` (an event nothing consumes yet
    /// is fine; a consumer of an event nobody publishes is not).
    #[must_use]
    pub fn publishes<E: DomainEvent>(mut self) -> Self {
        self.publishes.push(E::NAME);
        self
    }
}

/// A consumer of an event, as publishing sees it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Consumer {
    pub kind: &'static str,
    pub queue: &'static str,
    pub max_attempts: u32,
}

/// Every module's background work, by kind.
#[derive(Clone, Default)]
pub struct Registry {
    entries: HashMap<&'static str, Entry>,
    consumers: HashMap<&'static str, Vec<Consumer>>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut kinds: Vec<_> = self.entries.keys().collect();
        kinds.sort();
        f.debug_struct("Registry").field("kinds", &kinds).finish()
    }
}

impl Registry {
    /// The registry of `modules` (`(name, background)`). Every kind must be
    /// named `<module>.<…>` after the module declaring it and be unique, and
    /// every consumed event must be published by some module.
    pub fn build(modules: Vec<(&'static str, Background)>) -> Result<Self, Vec<String>> {
        let mut registry = Self::default();
        let mut problems = Vec::new();
        let mut published = Vec::new();
        for (module, background) in modules {
            published.extend(background.publishes.iter().copied());
            for entry in background.entries {
                if !entry
                    .kind
                    .strip_prefix(module)
                    .is_some_and(|rest| rest.starts_with('.') && rest.len() > 1)
                {
                    problems.push(format!(
                        "`{}` (module `{module}`) must be named `{module}.<name>`",
                        entry.kind
                    ));
                }
                if let Some(event) = entry.event {
                    registry.consumers.entry(event).or_default().push(Consumer {
                        kind: entry.kind,
                        queue: entry.queue,
                        max_attempts: entry.max_attempts,
                    });
                }
                if registry.entries.insert(entry.kind, entry.clone()).is_some() {
                    problems.push(format!("`{}` is registered twice", entry.kind));
                }
            }
        }
        for event in registry.consumers.keys() {
            if !published.contains(event) {
                problems.push(format!("`{event}` is consumed but no module publishes it"));
            }
        }
        if problems.is_empty() {
            Ok(registry)
        } else {
            problems.sort();
            Err(problems)
        }
    }

    /// The consumers of event `name`.
    pub(crate) fn consumers_of(&self, name: &str) -> &[Consumer] {
        self.consumers.get(name).map_or(&[], Vec::as_slice)
    }

    /// Every registered kind.
    pub fn kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self.entries.keys().map(|k| (*k).to_owned()).collect();
        kinds.sort();
        kinds
    }
}

/// `attempts` as the database's integer.
pub(crate) fn attempts_to_db(attempts: u32) -> i32 {
    i32::try_from(attempts.max(1)).unwrap_or(i32::MAX)
}

/// Enqueue `job` in `conn`'s transaction; it runs once that commits.
pub async fn enqueue<J: Job>(conn: &mut PgConnection, job: &J) -> Result<Uuid, sqlx::Error> {
    let payload = serde_json::to_value(job).map_err(|e| sqlx::Error::Encode(e.into()))?;
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO kernel.jobs (id, kind, queue, payload, max_attempts)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(J::NAME)
    .bind(J::QUEUE)
    .bind(payload)
    .bind(attempts_to_db(J::MAX_ATTEMPTS))
    .execute(conn)
    .await?;
    Ok(id)
}

/// A claimed job.
#[derive(sqlx::FromRow)]
struct Claimed {
    id: Uuid,
    kind: String,
    payload: serde_json::Value,
    event_id: Option<Uuid>,
    attempts: i32,
    max_attempts: i32,
}

/// Runs registered jobs.
#[derive(Debug, Clone)]
pub struct Worker {
    db: PgPool,
    registry: Arc<Registry>,
    events: EventBus,
}

impl Worker {
    /// A worker running `registry`'s kinds, publishing through `events`.
    pub fn new(db: PgPool, registry: Arc<Registry>, events: EventBus) -> Self {
        Self {
            db,
            registry,
            events,
        }
    }

    /// Claim and run one ready job. `Ok(false)` when there was none.
    pub async fn run_once(&self) -> Result<bool, sqlx::Error> {
        sqlx::query(
            "UPDATE kernel.jobs SET state = 'pending', locked_until = NULL
              WHERE state = 'running' AND locked_until < now()",
        )
        .execute(&self.db)
        .await?;
        let claimed: Option<Claimed> = sqlx::query_as(
            "UPDATE kernel.jobs
                SET state = 'running', attempts = attempts + 1,
                    locked_until = now() + make_interval(secs => $2)
              WHERE id = (SELECT id FROM kernel.jobs
                           WHERE state = 'pending' AND run_at <= now() AND kind = ANY($1)
                           ORDER BY run_at, id
                           LIMIT 1
                             FOR UPDATE SKIP LOCKED)
          RETURNING id, kind, payload, event_id, attempts, max_attempts",
        )
        .bind(self.registry.kinds())
        .bind(LEASE.as_secs_f64())
        .fetch_optional(&self.db)
        .await?;
        let Some(job) = claimed else {
            return Ok(false);
        };
        let Some(entry) = self.registry.entries.get(job.kind.as_str()) else {
            // Claimed only among registered kinds; nothing to run.
            return Ok(true);
        };
        let ctx = JobContext {
            db: self.db.clone(),
            events: self.events.clone(),
            job_id: job.id,
            attempt: u32::try_from(job.attempts).unwrap_or(1),
            event_id: job.event_id,
        };
        let outcome = tokio::time::timeout(LEASE, (entry.runner)(ctx, job.payload))
            .await
            .unwrap_or_else(|_| Err(JobError::Retry("timed out".into())));
        self.finish(&job, entry.kind, outcome).await?;
        Ok(true)
    }

    async fn finish(
        &self,
        job: &Claimed,
        kind: &str,
        outcome: Result<(), JobError>,
    ) -> Result<(), sqlx::Error> {
        let error = match outcome {
            Ok(()) => {
                sqlx::query(
                    "UPDATE kernel.jobs
                        SET state = 'succeeded', locked_until = NULL, last_error = NULL,
                            finished_at = now()
                      WHERE id = $1",
                )
                .bind(job.id)
                .execute(&self.db)
                .await?;
                return Ok(());
            }
            Err(error) => error,
        };
        let retry = matches!(error, JobError::Retry(_)) && job.attempts < job.max_attempts;
        let message: String = error.to_string().chars().take(500).collect();
        if retry {
            let backoff = backoff(job.attempts);
            tracing::warn!(kind, attempt = job.attempts, "job failed; retrying later");
            sqlx::query(
                "UPDATE kernel.jobs
                    SET state = 'pending', locked_until = NULL, last_error = $2,
                        run_at = now() + make_interval(secs => $3)
                  WHERE id = $1",
            )
            .bind(job.id)
            .bind(message)
            .bind(backoff.as_secs_f64())
            .execute(&self.db)
            .await?;
        } else {
            tracing::error!(kind, attempt = job.attempts, "job failed for good");
            sqlx::query(
                "UPDATE kernel.jobs
                    SET state = 'failed', locked_until = NULL, last_error = $2,
                        finished_at = now()
                  WHERE id = $1",
            )
            .bind(job.id)
            .bind(message)
            .execute(&self.db)
            .await?;
        }
        Ok(())
    }

    /// Delete succeeded jobs older than [`RETENTION`]. How many.
    pub async fn prune(&self) -> Result<u64, sqlx::Error> {
        let done = sqlx::query(
            "DELETE FROM kernel.jobs
              WHERE state = 'succeeded' AND finished_at < now() - make_interval(secs => $1)",
        )
        .bind(RETENTION.as_secs_f64())
        .execute(&self.db)
        .await?;
        Ok(done.rows_affected())
    }

    /// Run jobs until `shutdown` turns `true`.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut last_prune = tokio::time::Instant::now();
        loop {
            if *shutdown.borrow() {
                return;
            }
            let ran = match self.run_once().await {
                Ok(ran) => ran,
                Err(e) => {
                    tracing::warn!(error = %e, "job worker could not reach the database");
                    false
                }
            };
            if last_prune.elapsed() > Duration::from_secs(3600) {
                last_prune = tokio::time::Instant::now();
                if let Err(e) = self.prune().await {
                    tracing::warn!(error = %e, "could not prune finished jobs");
                }
            }
            if !ran {
                tokio::select! {
                    () = tokio::time::sleep(POLL_INTERVAL) => {}
                    _ = shutdown.changed() => {}
                }
            }
        }
    }
}

/// The wait after failed attempt `attempt`: 2^attempt seconds, at most
/// [`MAX_BACKOFF`].
fn backoff(attempt: i32) -> Duration {
    let exponent = u32::try_from(attempt.clamp(0, 12)).unwrap_or(12);
    Duration::from_secs(2_u64.pow(exponent)).min(MAX_BACKOFF)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde::Deserialize;

    use super::*;

    static RAN: AtomicUsize = AtomicUsize::new(0);
    static HANDLED: AtomicUsize = AtomicUsize::new(0);

    #[derive(Serialize, Deserialize)]
    struct Count {
        by: usize,
    }

    impl Job for Count {
        const NAME: &'static str = "test.count";
        async fn run(self, _ctx: JobContext) -> Result<(), JobError> {
            RAN.fetch_add(self.by, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Serialize, Deserialize)]
    struct Flaky;

    impl Job for Flaky {
        const NAME: &'static str = "test.flaky";
        const MAX_ATTEMPTS: u32 = 2;
        async fn run(self, _ctx: JobContext) -> Result<(), JobError> {
            Err(JobError::Retry("down".into()))
        }
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Happened {
        n: usize,
    }

    impl DomainEvent for Happened {
        const NAME: &'static str = "test.happened";
        const VERSION: u16 = 1;
    }

    struct OnHappened;

    impl EventHandler for OnHappened {
        type Event = Happened;
        const NAME: &'static str = "test.on_happened";
        async fn handle(_ctx: JobContext, event: Event<Happened>) -> Result<(), JobError> {
            HANDLED.fetch_add(event.payload.n, Ordering::SeqCst);
            Ok(())
        }
    }

    fn registry() -> Arc<Registry> {
        let background = Background::new()
            .job::<Count>()
            .job::<Flaky>()
            .on_event::<OnHappened>()
            .publishes::<Happened>();
        Arc::new(Registry::build(vec![("test", background)]).unwrap())
    }

    async fn state_of(db: &PgPool, id: Uuid) -> (String, i32) {
        sqlx::query_as("SELECT state, attempts FROM kernel.jobs WHERE id = $1")
            .bind(id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_job_runs_once_its_transaction_commits(db: PgPool) {
        let registry = registry();
        let events = EventBus::new(Arc::clone(&registry));
        let worker = Worker::new(db.clone(), registry, events);

        let mut tx = db.begin().await.unwrap();
        enqueue(&mut tx, &Count { by: 3 }).await.unwrap();
        tx.rollback().await.unwrap();
        assert!(
            !worker.run_once().await.unwrap(),
            "a rolled-back job never runs"
        );

        let mut tx = db.begin().await.unwrap();
        let id = enqueue(&mut tx, &Count { by: 3 }).await.unwrap();
        tx.commit().await.unwrap();
        let before = RAN.load(Ordering::SeqCst);
        assert!(worker.run_once().await.unwrap());
        assert_eq!(RAN.load(Ordering::SeqCst), before + 3);
        assert_eq!(state_of(&db, id).await, ("succeeded".to_owned(), 1));
        assert!(!worker.run_once().await.unwrap());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_failing_job_is_retried_then_marked_failed(db: PgPool) {
        let registry = registry();
        let worker = Worker::new(db.clone(), Arc::clone(&registry), EventBus::new(registry));
        let mut conn = db.acquire().await.unwrap();
        let id = enqueue(&mut conn, &Flaky).await.unwrap();
        drop(conn);

        assert!(worker.run_once().await.unwrap());
        assert_eq!(state_of(&db, id).await, ("pending".to_owned(), 1));
        // Not before its backoff.
        assert!(!worker.run_once().await.unwrap());
        sqlx::query("UPDATE kernel.jobs SET run_at = now() WHERE id = $1")
            .bind(id)
            .execute(&db)
            .await
            .unwrap();
        assert!(worker.run_once().await.unwrap());
        assert_eq!(state_of(&db, id).await, ("failed".to_owned(), 2));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn a_published_event_reaches_its_consumer(db: PgPool) {
        let registry = registry();
        let events = EventBus::new(Arc::clone(&registry));
        let worker = Worker::new(db.clone(), registry, events.clone());

        let mut tx = db.begin().await.unwrap();
        let event = events.publish(&mut tx, &Happened { n: 5 }).await.unwrap();
        tx.commit().await.unwrap();

        let (name, version): (String, i32) =
            sqlx::query_as("SELECT name, version FROM kernel.events WHERE id = $1")
                .bind(event)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!((name.as_str(), version), ("test.happened", 1));
        let before = HANDLED.load(Ordering::SeqCst);
        assert!(worker.run_once().await.unwrap());
        assert_eq!(HANDLED.load(Ordering::SeqCst), before + 5);
    }

    #[test]
    fn registrations_are_named_after_their_module_and_unique() {
        let err = Registry::build(vec![
            ("other", Background::new().job::<Count>()),
            (
                "test",
                Background::new().job::<Count>().on_event::<OnHappened>(),
            ),
        ])
        .unwrap_err();
        assert!(
            err.iter().any(|e| e.contains("must be named `other.")),
            "{err:?}"
        );
        assert!(
            err.iter().any(|e| e.contains("registered twice")),
            "{err:?}"
        );
        assert!(
            err.iter().any(|e| e.contains("no module publishes")),
            "{err:?}"
        );
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        assert_eq!(backoff(1), Duration::from_secs(2));
        assert_eq!(backoff(3), Duration::from_secs(8));
        assert_eq!(backoff(40), MAX_BACKOFF);
    }
}
