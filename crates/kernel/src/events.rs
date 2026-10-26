//! Durable domain events: the outbox.
//!
//! Modules never call each other; they publish events and subscribe to the
//! ones they care about. Publishing writes the event to `kernel.events`
//! **in the caller's transaction**, together with one job per registered
//! consumer. If the transaction rolls back, neither the event nor its
//! deliveries exist; once it commits, every consumer runs it at least once,
//! through the job queue, even across restarts.
//!
//! Payloads carry ids, kinds and counts, never personal data.

use std::{future::Future, sync::Arc};

use serde::{Serialize, de::DeserializeOwned};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::jobs::{self, JobContext, JobError, Registry};

/// A fact a module announces to the rest of the system.
pub trait DomainEvent: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Stable wire name, `<module>.<event>`; never rename it.
    const NAME: &'static str;
    /// Payload schema version, starting at 1; bump it on an incompatible
    /// change.
    const VERSION: u16;
}

/// A published event as a consumer receives it.
#[derive(Debug, Clone)]
pub struct Event<E> {
    /// The event's id in the log; stable across retries, so a handler can
    /// use it as an idempotency key.
    pub id: Uuid,
    /// The event itself.
    pub payload: E,
}

/// A consumer of one event type, registered with
/// [`Background::on_event`](crate::jobs::Background::on_event). Each
/// delivery is a job of kind [`NAME`](Self::NAME).
pub trait EventHandler: Send + Sync + 'static {
    /// The event this handler consumes.
    type Event: DomainEvent;
    /// Stable consumer name, `<module>.<handler>`.
    const NAME: &'static str;
    /// The queue its deliveries run on.
    const QUEUE: &'static str = "events";
    /// Attempts before a delivery is marked failed.
    const MAX_ATTEMPTS: u32 = jobs::DEFAULT_MAX_ATTEMPTS;

    /// Handle one event.
    fn handle(
        ctx: JobContext,
        event: Event<Self::Event>,
    ) -> impl Future<Output = Result<(), JobError>> + Send;
}

/// Publishes events and their deliveries. Cheap to clone.
#[derive(Clone, Default)]
pub struct EventBus {
    registry: Arc<Registry>,
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("registry", &self.registry)
            .finish()
    }
}

impl EventBus {
    /// A bus delivering to the consumers in `registry`.
    pub fn new(registry: Arc<Registry>) -> Self {
        Self { registry }
    }

    /// Append `event` to the log and enqueue one delivery per consumer, in
    /// `conn`'s transaction. The event's id.
    pub async fn publish<E: DomainEvent>(
        &self,
        conn: &mut PgConnection,
        event: &E,
    ) -> Result<Uuid, sqlx::Error> {
        let payload = serde_json::to_value(event).map_err(|e| sqlx::Error::Encode(e.into()))?;
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO kernel.events (id, name, version, payload) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(E::NAME)
        .bind(i32::from(E::VERSION))
        .bind(&payload)
        .execute(&mut *conn)
        .await?;
        for consumer in self.registry.consumers_of(E::NAME) {
            sqlx::query(
                "INSERT INTO kernel.jobs (id, kind, queue, payload, event_id, max_attempts)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(Uuid::now_v7())
            .bind(consumer.kind)
            .bind(consumer.queue)
            .bind(payload.clone())
            .bind(id)
            .bind(jobs::attempts_to_db(consumer.max_attempts))
            .execute(&mut *conn)
            .await?;
        }
        Ok(id)
    }
}
