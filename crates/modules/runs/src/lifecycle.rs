//! The run lifecycle end to end with the fake runtime: success (the real
//! agent runner), failure, timeout, Stop, the startup sweep, and the
//! concurrency limits.

use std::{sync::Arc, time::Duration};

use chrono::{TimeDelta, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    probe,
    runtime::{SandboxRuntime, SandboxSpec, fake::FakeRuntime},
    store::{self, Layer, Level},
    testkit::{Harness, agent_binary, kind, orchestrator, scripted, serve},
};

async fn queue(harness: &Harness, account: Uuid, kind: &str) -> Uuid {
    let mut conn = harness.db.acquire().await.unwrap();
    store::queue(&mut conn, account, kind, None, &json!({}), Utc::now())
        .await
        .unwrap()
}

/// Wait until run `id` has status `status` (at most ten seconds).
async fn until_status(harness: &Harness, id: Uuid, status: &str) {
    for _ in 0..100 {
        if harness.run(id).await.status == status {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("run {id} never became {status}");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_probe_succeeds_with_its_events_and_its_sandbox_is_gone(db: PgPool) {
    let runtime = Arc::new(FakeRuntime::new(agent_binary()));
    let harness = Harness::new(db, runtime.clone());
    let api = serve(harness.agent_router()).await;
    let orchestrator = orchestrator(&harness, runtime.clone(), api, vec![], 3);
    let id = queue(&harness, harness.account, probe::KIND).await;

    let task = orchestrator
        .tick()
        .await
        .unwrap()
        .expect("the run is claimed");
    tokio::time::timeout(Duration::from_secs(60), task)
        .await
        .unwrap()
        .unwrap();

    let run = harness.run(id).await;
    assert_eq!(run.status, "finished");
    assert_eq!(
        run.outcome.as_deref(),
        Some("succeeded"),
        "{:?}",
        run.error_kind
    );
    assert_eq!(
        run.summary,
        Some(json!({ "checks": ["context", "tool", "cap", "network"] }))
    );
    assert!(run.sandbox.is_some());
    assert_eq!(run.policy.as_deref(), Some("policies/probe.yaml"));
    assert!(runtime.is_empty(), "its sandbox is gone");
    assert!(!harness.has_token(id).await, "its token is gone");
    assert_eq!(harness.finished().await, [(id, "succeeded".to_owned())]);

    let events = harness.events(id).await;
    let agent: Vec<_> = events.iter().filter(|e| e.layer == Layer::Agent).collect();
    // Read from its output and sent through the API, each kept once.
    assert_eq!(agent.len(), 4, "{agent:?}");
    assert!(
        agent
            .iter()
            .any(|e| e.level == Level::Tool && e.tool_name.as_deref() == Some("probe_echo"))
    );
    assert!(
        agent
            .iter()
            .any(|e| e.level == Level::Deny && e.message.contains("past its cap"))
    );
    assert!(
        agent
            .iter()
            .any(|e| e.level == Level::Deny && e.message.contains("example.com"))
    );
    let blocked = events
        .iter()
        .find(|e| e.layer == Layer::Sandbox && e.level == Level::Deny)
        .expect("the sandbox log shows the blocked request");
    assert!(
        blocked.message.contains("example.com:443"),
        "{}",
        blocked.message
    );
    assert_eq!(blocked.operation.as_deref(), Some("NET:OPEN"));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_agent_that_exits_without_finishing_fails_the_run(db: PgPool) {
    let runtime = scripted("exit 3");
    let harness = Harness::new(db, runtime.clone());
    let api = serve(harness.agent_router()).await;
    let orchestrator = orchestrator(&harness, runtime.clone(), api, vec![], 3);
    let id = queue(&harness, harness.account, probe::KIND).await;

    orchestrator.tick().await.unwrap().unwrap().await.unwrap();

    let run = harness.run(id).await;
    assert_eq!(run.outcome.as_deref(), Some("failed"));
    assert_eq!(run.error_kind.as_deref(), Some("agent_exited"));
    assert!(runtime.is_empty());
    assert!(!harness.has_token(id).await);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_run_past_its_deadline_ends_timed_out(db: PgPool) {
    let runtime = scripted("exec sleep 30");
    let harness = Harness::new(db, runtime.clone());
    let api = serve(harness.agent_router()).await;
    let slow = kind("runs.slow", Duration::from_secs(1), 0);
    let orchestrator = orchestrator(&harness, runtime.clone(), api, vec![slow], 3);
    let id = queue(&harness, harness.account, "runs.slow").await;

    let started = std::time::Instant::now();
    orchestrator.tick().await.unwrap().unwrap().await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(20));

    let run = harness.run(id).await;
    assert_eq!(run.outcome.as_deref(), Some("ended_timeout"));
    assert!(runtime.is_empty(), "its sandbox is removed");
    assert_eq!(harness.finished().await, [(id, "ended_timeout".to_owned())]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn stopping_a_run_removes_its_sandbox_and_ends_it_cancelled(db: PgPool) {
    let runtime = scripted("exec sleep 30");
    let harness = Harness::new(db, runtime.clone());
    let api = serve(harness.agent_router()).await;
    let orchestrator = orchestrator(&harness, runtime.clone(), api, vec![], 3);
    let id = queue(&harness, harness.account, probe::KIND).await;

    let task = orchestrator.tick().await.unwrap().unwrap();
    until_status(&harness, id, "running").await;
    sqlx::query("UPDATE runs.runs SET tokens_in = 12, tokens_out = 3 WHERE id = $1")
        .bind(id)
        .execute(&harness.db)
        .await
        .unwrap();
    let (status, body) = harness
        .json("POST", &format!("/api/v1/runs/{id}/cancel"), &json!({}))
        .await;
    assert_eq!(status.as_u16(), 202, "{body}");
    assert_eq!(body["stopping"], true);
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();

    let run = harness.run(id).await;
    assert_eq!(run.outcome.as_deref(), Some("cancelled"));
    assert_eq!(
        (run.tokens_in, run.tokens_out),
        (12, 3),
        "with what it had used"
    );
    assert!(runtime.is_empty());
    let (_, body) = harness.get(&format!("/api/v1/runs/{id}")).await;
    assert_eq!(body["outcome"], "cancelled");
    assert_eq!(body["stopping"], false);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_startup_sweep_ends_a_dead_servers_runs_and_removes_their_sandboxes(db: PgPool) {
    let runtime = scripted("exec sleep 30");
    let harness = Harness::new(db, runtime.clone());
    let api = url::Url::parse("http://127.0.0.1:9").unwrap();
    // A run a server left going, and a sandbox of a run nobody knows.
    let (left, _) = harness.going(harness.account, &probe::kind()).await;
    sqlx::query("UPDATE runs.runs SET claimed_until = now() - interval '1 minute' WHERE id = $1")
        .bind(left)
        .execute(&harness.db)
        .await
        .unwrap();
    for run in [left, Uuid::now_v7()] {
        runtime
            .create(SandboxSpec {
                run,
                kind: probe::KIND,
                image: "agent".to_owned(),
                policy: probe::POLICY,
                api: api.clone(),
                token: "unused".to_owned(),
            })
            .await
            .unwrap();
    }
    // And one still going on a live server: kept.
    let (live, _) = harness.going(Uuid::now_v7(), &probe::kind()).await;
    runtime
        .create(SandboxSpec {
            run: live,
            kind: probe::KIND,
            image: "agent".to_owned(),
            policy: probe::POLICY,
            api: api.clone(),
            token: "unused".to_owned(),
        })
        .await
        .unwrap();
    assert_eq!(runtime.len(), 3);

    orchestrator(&harness, runtime.clone(), api, vec![], 3)
        .startup()
        .await;

    let run = harness.run(left).await;
    assert_eq!(run.outcome.as_deref(), Some("failed"));
    assert_eq!(run.error_kind.as_deref(), Some("server_restarted"));
    assert!(!harness.has_token(left).await);
    assert_eq!(runtime.len(), 1, "only the live run's sandbox is left");
    assert_eq!(harness.run(live).await.status, "running");
    assert_eq!(harness.finished().await, [(left, "failed".to_owned())]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn runs_wait_their_turn(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    let now = Utc::now();
    let later = now + TimeDelta::seconds(30);
    let (a, b) = (Uuid::now_v7(), Uuid::now_v7());
    let first = queue(&harness, a, probe::KIND).await;
    let second = queue(&harness, a, probe::KIND).await;
    let other = queue(&harness, b, probe::KIND).await;

    let claimed = store::claim(&harness.db, "s1", 3, now, later)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, first);
    // The account's second waits; another account's goes.
    let claimed = store::claim(&harness.db, "s1", 3, now, later)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, other);
    assert!(
        store::claim(&harness.db, "s1", 3, now, later)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(harness.run(second).await.status, "queued");

    // Once the first ends, the second starts.
    store::finish(
        &harness.db,
        &harness.state.events,
        first,
        applicamp_kernel::agent::Outcome::Succeeded,
        None,
        now,
    )
    .await
    .unwrap();
    let claimed = store::claim(&harness.db, "s1", 3, now, later)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, second);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_instance_runs_at_most_its_limit(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    let now = Utc::now();
    let later = now + TimeDelta::seconds(30);
    for _ in 0..3 {
        queue(&harness, Uuid::now_v7(), probe::KIND).await;
    }
    assert!(
        store::claim(&harness.db, "s1", 2, now, later)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::claim(&harness.db, "s1", 2, now, later)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::claim(&harness.db, "s1", 2, now, later)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_sandbox_that_cannot_be_created_fails_the_run(db: PgPool) {
    let runtime = Arc::new(FakeRuntime::new("/nonexistent/applicamp-agent".into()));
    let harness = Harness::new(db, runtime.clone());
    let orchestrator = orchestrator(
        &harness,
        runtime.clone(),
        url::Url::parse("http://127.0.0.1:9").unwrap(),
        vec![],
        3,
    );
    let id = queue(&harness, harness.account, probe::KIND).await;

    orchestrator.tick().await.unwrap().unwrap().await.unwrap();

    let run = harness.run(id).await;
    assert_eq!(run.outcome.as_deref(), Some("failed"));
    assert_eq!(run.error_kind.as_deref(), Some("sandbox_create_failed"));
    assert!(!harness.has_token(id).await);
}

/// The fake runtime, taking `delay` to make each sandbox, and noting the
/// run's claim once it has (so a test sees it was renewed meanwhile).
struct Slow {
    inner: Arc<FakeRuntime>,
    delay: Duration,
    db: PgPool,
    claimed_until: std::sync::Mutex<Option<chrono::DateTime<Utc>>>,
}

#[async_trait::async_trait]
impl SandboxRuntime for Slow {
    fn name(&self) -> &'static str {
        "slow"
    }
    async fn prepare(&self, kinds: &crate::kinds::Kinds) -> crate::runtime::Result<()> {
        self.inner.prepare(kinds).await
    }
    async fn create(
        &self,
        spec: SandboxSpec,
    ) -> crate::runtime::Result<crate::runtime::SandboxHandle> {
        tokio::time::sleep(self.delay).await;
        let until: Option<chrono::DateTime<Utc>> =
            sqlx::query_scalar("SELECT claimed_until FROM runs.runs WHERE id = $1")
                .bind(spec.run)
                .fetch_one(&self.db)
                .await
                .unwrap();
        *self.claimed_until.lock().unwrap() = until;
        self.inner.create(spec).await
    }
    fn logs(
        &self,
        handle: &crate::runtime::SandboxHandle,
    ) -> futures::stream::BoxStream<'static, crate::runtime::Result<crate::runtime::LogLine>> {
        self.inner.logs(handle)
    }
    async fn wait(
        &self,
        handle: &crate::runtime::SandboxHandle,
        until: tokio::time::Instant,
    ) -> crate::runtime::Result<crate::runtime::Exit> {
        self.inner.wait(handle, until).await
    }
    async fn delete(&self, handle: &crate::runtime::SandboxHandle) -> crate::runtime::Result<()> {
        self.inner.delete(handle).await
    }
    async fn sweep(&self, keep: &std::collections::HashSet<Uuid>) -> crate::runtime::Result<usize> {
        self.inner.sweep(keep).await
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_claim_is_renewed_while_the_sandbox_is_made(db: PgPool) {
    let inner = scripted("exit 0");
    let slow = Arc::new(Slow {
        inner: inner.clone(),
        delay: Duration::from_millis(2_500),
        db: db.clone(),
        claimed_until: std::sync::Mutex::new(None),
    });
    let harness = Harness::new(db, slow.clone());
    let api = serve(harness.agent_router()).await;
    let orchestrator = orchestrator(&harness, slow.clone(), api, vec![], 3);
    let id = queue(&harness, harness.account, probe::KIND).await;

    let claimed_at = Utc::now();
    orchestrator.tick().await.unwrap().unwrap().await.unwrap();

    let renewed = slow
        .claimed_until
        .lock()
        .unwrap()
        .expect("a claim while making it");
    assert!(
        renewed > claimed_at + crate::orchestrator::LEASE + TimeDelta::seconds(1),
        "renewed while the sandbox was made: {renewed} vs {claimed_at}"
    );
    assert_eq!(
        harness.run(id).await.error_kind.as_deref(),
        Some("agent_exited")
    );
    assert!(inner.is_empty());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_sandbox_that_takes_too_long_to_make_fails_the_run(db: PgPool) {
    let inner = scripted("exit 0");
    let slow = Arc::new(Slow {
        inner: inner.clone(),
        delay: Duration::from_secs(30),
        db: db.clone(),
        claimed_until: std::sync::Mutex::new(None),
    });
    let harness = Harness::new(db, slow.clone());
    let orchestrator = orchestrator(
        &harness,
        slow,
        url::Url::parse("http://127.0.0.1:9").unwrap(),
        vec![],
        3,
    )
    .with_create_timeout(Duration::from_millis(300));
    let id = queue(&harness, harness.account, probe::KIND).await;

    tokio::time::timeout(
        Duration::from_secs(10),
        orchestrator.tick().await.unwrap().unwrap(),
    )
    .await
    .unwrap()
    .unwrap();

    let run = harness.run(id).await;
    assert_eq!(run.outcome.as_deref(), Some("failed"));
    assert_eq!(run.error_kind.as_deref(), Some("sandbox_create_timeout"));
    assert!(!harness.has_token(id).await);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_run_ended_elsewhere_gets_no_token(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    let id = queue(&harness, harness.account, probe::KIND).await;
    let now = Utc::now();
    store::claim(&harness.db, "s1", 3, now, now + TimeDelta::seconds(30))
        .await
        .unwrap()
        .unwrap();
    let start = |instance: &'static str| {
        let db = harness.db.clone();
        async move {
            store::start(
                &db,
                id,
                instance,
                now,
                "agent",
                "policies/probe.yaml",
                b"hash",
                &json!({}),
                now,
            )
            .await
            .unwrap()
        }
    };
    assert!(!start("s2").await, "not claimed by s2");
    assert!(!harness.has_token(id).await);
    // Reaped meanwhile: its claimer cannot start it either.
    store::finish(
        &harness.db,
        &harness.state.events,
        id,
        applicamp_kernel::agent::Outcome::Failed,
        Some("server_restarted"),
        now,
    )
    .await
    .unwrap();
    assert!(!start("s1").await, "already ended");
    assert!(!harness.has_token(id).await);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_run_asked_to_stop_is_never_claimed(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    let id = queue(&harness, harness.account, probe::KIND).await;
    sqlx::query("UPDATE runs.runs SET cancel_requested = true WHERE id = $1")
        .bind(id)
        .execute(&harness.db)
        .await
        .unwrap();
    let now = Utc::now();
    assert!(
        store::claim(&harness.db, "s1", 3, now, now + TimeDelta::seconds(30))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store::cancel_queued(&harness.db, &harness.state.events, id, now)
            .await
            .unwrap()
    );
    assert!(
        !store::cancel_queued(&harness.db, &harness.state.events, id, now)
            .await
            .unwrap(),
        "once"
    );
    assert_eq!(harness.run(id).await.outcome.as_deref(), Some("cancelled"));
    assert_eq!(harness.finished().await, [(id, "cancelled".to_owned())]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_run_keeps_at_most_its_cap_of_events(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    let (run, _) = harness.going(harness.account, &probe::kind()).await;
    // As if it had written all but one of its lines already.
    sqlx::query("UPDATE runs.runs SET last_seq = $2 WHERE id = $1")
        .bind(run)
        .bind(store::MAX_EVENTS_PER_RUN - 1)
        .execute(&harness.db)
        .await
        .unwrap();
    let line = |seq: i64| store::NewEvent {
        layer: Layer::Agent,
        level: Level::Info,
        agent_seq: Some(seq),
        operation: None,
        tool_name: None,
        message: format!("line {seq}"),
        data: None,
    };
    let now = Utc::now();
    assert!(
        store::insert_event(&harness.db, run, now, &line(1))
            .await
            .unwrap(),
        "the last one kept"
    );
    assert!(
        !store::insert_event(&harness.db, run, now, &line(2))
            .await
            .unwrap()
    );
    assert!(
        !store::insert_event(&harness.db, run, now, &line(3))
            .await
            .unwrap()
    );
    let events = harness.events(run).await;
    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(events[0].seq, store::MAX_EVENTS_PER_RUN);
    assert_eq!(events[1].seq, store::MAX_EVENTS_PER_RUN + 1);
    assert_eq!(events[1].operation.as_deref(), Some("truncated"));
    assert_eq!(events[1].message, store::TRUNCATED);
    assert_eq!(events[1].level, Level::Error);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn nul_characters_from_a_runners_output_are_dropped(db: PgPool) {
    let harness = Harness::new(db, scripted("true"));
    let (run, _) = harness.going(harness.account, &probe::kind()).await;
    let event = store::NewEvent {
        layer: Layer::Agent,
        level: Level::Info,
        agent_seq: Some(1),
        operation: Some("op\u{0000}".to_owned()),
        tool_name: None,
        message: "a\u{0000}b".to_owned(),
        data: Some(json!({ "x": "\u{0000}" })),
    };
    assert!(
        store::insert_event(&harness.db, run, Utc::now(), &event)
            .await
            .unwrap()
    );
    let events = harness.events(run).await;
    assert_eq!(events[0].message, "ab");
    assert_eq!(events[0].operation.as_deref(), Some("op"));
    assert!(events[0].data.is_none());
}
