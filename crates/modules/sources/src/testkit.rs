//! The module over a real Postgres (`#[sqlx::test]`), fake job boards and a
//! fake model (wiremock), and fake neighbours standing in for the profile
//! and the pipeline.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use applicamp_ai::{AiConfig, Gateway, ProviderConfig, RetryPolicy};
use applicamp_kernel::{
    AppState, BoxFuture, Config, EventBus, Result,
    auth::Principal,
    clock::FixedClock,
    crypto::SecretKey,
    jobs::{Registry, Worker},
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware::{self, Next},
};
use chrono::{DateTime, TimeZone as _, Utc};
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex},
};

use crate::{
    SourcesConfig, SourcesDeps,
    job::WorkMode,
    matching::MatchInput,
    module,
    ports::{Neighbours, Scored, Unchecked},
    readers::Board,
    requirements::{Excluded, Facts, Reach},
};

pub(crate) const GREENHOUSE_LIST: &str = include_str!("readers/fixtures/greenhouse_list.json");
pub(crate) const GREENHOUSE_JOB: &str = include_str!("readers/fixtures/greenhouse_job.json");

/// 26 October 2026, noon UTC: the fixtures' "today".
pub(crate) fn today() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap()
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A profile and a pipeline in memory.
#[derive(Default)]
pub(crate) struct Fake {
    pub(crate) input: Mutex<Option<MatchInput>>,
    pub(crate) accounts: Mutex<Vec<Uuid>>,
    pub(crate) known: Mutex<HashSet<(String, String)>>,
    pub(crate) added: Mutex<Vec<Scored>>,
    /// Opportunities found before the rules, not yet checked.
    pub(crate) unchecked: Mutex<Vec<Unchecked>>,
    /// The checks kept: the opportunity, its facts, why it is hidden.
    pub(crate) checked: Mutex<Vec<(Uuid, Facts, Option<Excluded>)>>,
}

impl Fake {
    /// Someone with a profile who wants remote staff or founding roles.
    pub(crate) fn ready() -> Arc<Self> {
        let fake = Self::default();
        *lock(&fake.input) = Some(MatchInput {
            profile_version: "2026-10-01T00:00:00Z".into(),
            goals_version: 1,
            profile: json!({ "headline": "Backend engineer", "skills": ["Rust", "Postgres"] }),
            goals: json!({ "roles": ["Senior backend engineer"], "work_mode": ["remote"] }),
            roles: vec!["Senior backend engineer".into()],
            work_modes: vec![WorkMode::Remote],
            ..MatchInput::default()
        });
        Arc::new(fake)
    }

    /// Someone in Lisbon who works remote only, in English and Portuguese,
    /// and would not move: the owner's goals when #19 was reported.
    pub(crate) fn lisbon_remote_only() -> Arc<Self> {
        let fake = Self::default();
        *lock(&fake.input) = Some(MatchInput {
            profile_version: "2026-10-01T00:00:00Z".into(),
            goals_version: 1,
            profile: json!({ "headline": "Backend engineer", "location": "Lisbon, Portugal" }),
            goals: json!({ "roles": ["Senior backend engineer"], "work_mode": ["remote"],
                           "locations": ["Lisbon"], "relocate": "no" }),
            roles: vec!["Senior backend engineer".into()],
            work_modes: vec![WorkMode::Remote],
            locations: vec!["Lisbon".into()],
            will_not_relocate: true,
            home: Some("Lisbon".into()),
            relocate: Some(Reach::No),
            languages: vec!["en".into(), "pt".into()],
            ..MatchInput::default()
        });
        Arc::new(fake)
    }

    pub(crate) fn added(&self) -> Vec<Scored> {
        lock(&self.added).clone()
    }

    pub(crate) fn checked(&self) -> Vec<(Uuid, Facts, Option<Excluded>)> {
        lock(&self.checked).clone()
    }
}

impl Neighbours for Fake {
    fn matching_input<'a>(
        &'a self,
        _db: &'a PgPool,
        _account: Uuid,
        _now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<Option<MatchInput>>> {
        let input = lock(&self.input).clone();
        Box::pin(async move { Ok(input) })
    }

    fn accounts_with_goals<'a>(&'a self, _db: &'a PgPool) -> BoxFuture<'a, Result<Vec<Uuid>>> {
        let accounts = lock(&self.accounts).clone();
        Box::pin(async move { Ok(accounts) })
    }

    fn known_jobs<'a>(
        &'a self,
        _db: &'a PgPool,
        _account: Uuid,
    ) -> BoxFuture<'a, Result<HashSet<(String, String)>>> {
        let known = lock(&self.known).clone();
        Box::pin(async move { Ok(known) })
    }

    fn add_found<'a>(
        &'a self,
        _db: &'a PgPool,
        _events: &'a EventBus,
        _account: Uuid,
        found: Vec<Scored>,
        _now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<u32>> {
        let mut added = 0;
        {
            let mut known = lock(&self.known);
            let mut list = lock(&self.added);
            for scored in found {
                if known.insert(scored.job.key()) {
                    list.push(scored);
                    added += 1;
                }
            }
        }
        Box::pin(async move { Ok(added) })
    }

    fn unchecked<'a>(
        &'a self,
        _db: &'a PgPool,
        most: usize,
    ) -> BoxFuture<'a, Result<Vec<Unchecked>>> {
        let unchecked: Vec<Unchecked> = lock(&self.unchecked).iter().take(most).cloned().collect();
        Box::pin(async move { Ok(unchecked) })
    }

    fn set_requirements<'a>(
        &'a self,
        _db: &'a PgPool,
        _account: Uuid,
        opportunity: Uuid,
        facts: Facts,
        hidden: Option<Excluded>,
    ) -> BoxFuture<'a, Result<bool>> {
        let set = {
            let mut unchecked = lock(&self.unchecked);
            let before = unchecked.len();
            unchecked.retain(|item| item.opportunity != opportunity);
            unchecked.len() < before
        };
        if set {
            lock(&self.checked).push((opportunity, facts, hidden));
        }
        Box::pin(async move { Ok(set) })
    }
}

/// Settings for fake boards at `boards` (Greenhouse only), with no cache,
/// pace or manual limits unless a test sets them.
pub(crate) fn config(boards: &MockServer) -> SourcesConfig {
    let url = boards.uri().parse().unwrap();
    SourcesConfig {
        enabled: Some(vec!["greenhouse".into()]),
        greenhouse_url: url,
        cache_secs: Some(0),
        requests_per_second: Some(1_000),
        manual_per_day: Some(0),
        manual_every_secs: Some(0),
        nightly: Some(false),
        ..SourcesConfig::default()
    }
}

/// A gateway to `model`, at $1 per million tokens either way, with the
/// limits of `budget`.
pub(crate) fn gateway(model: &MockServer, budget: applicamp_ai::BudgetConfig) -> Gateway {
    Gateway::new(AiConfig {
        complete: Some(
            ProviderConfig::new(format!("{}/v1", model.uri()).parse().unwrap(), "test/model")
                .with_price(1.0, 1.0),
        ),
        budget,
        ..AiConfig::default()
    })
    .unwrap()
    .with_retry(RetryPolicy {
        attempts: 1,
        ..RetryPolicy::default()
    })
}

/// A chat completion whose answer is `answer`, costing `cost` dollars.
pub(crate) fn completion(answer: &Value, cost: f64) -> Value {
    json!({
        "model": "test/model",
        "choices": [{
            "message": { "role": "assistant", "content": answer.to_string() },
            "finish_reason": "stop",
        }],
        "usage": { "prompt_tokens": 3000, "completion_tokens": 600, "cost": cost },
    })
}

/// The facts of a remote job open to anyone in Europe, in English.
pub(crate) fn open_facts() -> Value {
    json!({
        "remote": "yes",
        "open_to_candidate": "yes",
        "region_note": "Europe",
        "working_languages": ["en"],
        "salary": null,
    })
}

/// One job's answer: `match`, a reason, a gap and `facts`.
pub(crate) fn answer_for(reference: &str, score: u8, facts: &Value) -> Value {
    let mut item = json!({
        "ref": reference,
        "match": score,
        "why": ["Rust and Postgres, as on your profile"],
        "gaps": ["Kafka is not on your profile"],
    });
    if let (Some(item), Some(facts)) = (item.as_object_mut(), facts.as_object()) {
        item.extend(facts.clone());
    }
    item
}

/// The postings of a scoring request's `<jobs>` block, by `ref`.
pub(crate) fn postings(request: &wiremock::Request) -> Vec<Value> {
    let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
    let user = body["messages"][1]["content"].as_str().unwrap_or("");
    let block = user
        .split("<jobs>")
        .nth(1)
        .and_then(|rest| rest.split("</jobs>").next())
        .unwrap_or("[]");
    serde_json::from_str::<Value>(block.trim())
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

/// A fake model that answers every job 85 and reads the facts the way a
/// model would from the fixtures' wording: "Remote-first aus Deutschland
/// oder Österreich" is remote but not open to Lisbon, a posting in German
/// needs German, a posting that never says remote is unclear, and "fully
/// remote, anywhere in Europe" is open to them.
pub(crate) async fn model_reading_facts() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(|request: &wiremock::Request| {
            let jobs: Vec<Value> = postings(request)
                .iter()
                .map(|posting| {
                    let text = posting["posting"].as_str().unwrap_or("");
                    let location = posting["location"].as_str().unwrap_or("");
                    let facts = if text.contains("Deutschland oder Österreich") {
                        json!({ "remote": "yes", "open_to_candidate": "no",
                                "region_note": "Germany or Austria only",
                                "working_languages": ["en"], "salary": null })
                    } else if text.contains("Wir suchen") {
                        json!({ "remote": "yes", "open_to_candidate": "yes", "region_note": "EU",
                                "working_languages": ["de"], "salary": null })
                    } else if format!("{text} {location}")
                        .to_lowercase()
                        .contains("remote")
                    {
                        open_facts()
                    } else {
                        json!({ "remote": "unclear", "open_to_candidate": "unclear",
                                "region_note": "", "working_languages": ["en"], "salary": null })
                    };
                    answer_for(posting["ref"].as_str().unwrap_or(""), 85, &facts)
                })
                .collect();
            ResponseTemplate::new(200).set_body_json(completion(&json!({ "jobs": jobs }), 0.000_4))
        })
        .mount(&server)
        .await;
    server
}

/// A fake model scoring every job of a batch `score`; every job remote and
/// open to anyone in Europe, in English.
pub(crate) async fn model_scoring(score: u8) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let user = body["messages"][1]["content"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            let jobs: Vec<Value> = (1..=10)
                .filter(|i| user.contains(&format!("\"ref\":\"j{i}\"")))
                .map(|i| answer_for(&format!("j{i}"), score, &open_facts()))
                .collect();
            ResponseTemplate::new(200).set_body_json(completion(&json!({ "jobs": jobs }), 0.000_4))
        })
        .mount(&server)
        .await;
    server
}

/// Fake boards serving the Greenhouse fixtures for board `exampleco`.
pub(crate) async fn greenhouse_boards() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/boards/exampleco/jobs"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(GREENHOUSE_LIST, "application/json"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/v1/boards/exampleco/jobs/\d+$"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(GREENHOUSE_JOB, "application/json"))
        .mount(&server)
        .await;
    server
}

/// The `exampleco` Greenhouse board.
pub(crate) fn exampleco() -> Board {
    Board {
        kind: crate::job::Source::Greenhouse,
        token: "exampleco".into(),
        company: "Example Co".into(),
        eu: false,
    }
}

/// The module as one signed-in account sees it, and its jobs.
pub(crate) struct Harness {
    app: Router,
    pub(crate) db: PgPool,
    pub(crate) account: Uuid,
    pub(crate) deps: SourcesDeps,
    pub(crate) fake: Arc<Fake>,
    pub(crate) clock: Arc<FixedClock>,
}

impl Harness {
    pub(crate) async fn new(
        db: PgPool,
        config: SourcesConfig,
        ai: Option<Gateway>,
        fake: Arc<Fake>,
    ) -> Self {
        let clock = Arc::new(FixedClock::new(today()));
        let neighbours: Arc<dyn Neighbours> = fake.clone();
        let deps = SourcesDeps::new(config, ai, neighbours)
            .unwrap()
            .with_clock(clock.clone());
        crate::boards::sync(&db, &[exampleco()]).await.unwrap();
        let state = AppState::new(
            db.clone(),
            Config::for_database("postgres://unused"),
            SecretKey::from_bytes([7; 32]),
        )
        .with_clock(clock.clone());
        let (router, _) = module(deps.clone()).router.split_for_parts();
        let account = Uuid::now_v7();
        let principal = Principal {
            account_id: account,
            session_id: Uuid::now_v7(),
        };
        let app = router
            .layer(middleware::from_fn(
                move |mut request: Request<Body>, next: Next| async move {
                    request.extensions_mut().insert(principal);
                    next.run(request).await
                },
            ))
            .with_state(state);
        Self {
            app,
            db,
            account,
            deps,
            fake,
            clock,
        }
    }

    pub(crate) async fn send(&self, request: Request<Body>) -> (StatusCode, Value) {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    pub(crate) async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.send(Request::get(uri).body(Body::empty()).unwrap())
            .await
    }

    pub(crate) async fn post(&self, uri: &str) -> (StatusCode, Value) {
        self.send(Request::post(uri).body(Body::empty()).unwrap())
            .await
    }

    /// Run every job that is due.
    pub(crate) async fn work(&self) {
        let registry = Arc::new(
            Registry::build(vec![(crate::NAME, module(self.deps.clone()).background)]).unwrap(),
        );
        let worker = Worker::new(
            self.db.clone(),
            Arc::clone(&registry),
            EventBus::new(registry),
        );
        while worker.run_once().await.unwrap() {}
    }
}
