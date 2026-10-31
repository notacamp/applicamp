//! "Tell me about you" over HTTP, against a fake model that answers in turn
//! and a fake employer research port. All data is invented.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use applicamp_kernel::{AppState, BoxFuture, Result};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::MockServer;

use super::{
    DAILY_CAP,
    judge::{self, RawChanges, RawPortrait, Source, SourceKind},
    prompt,
};
use crate::{
    ports::{EmployerResearch, FactBasis, ProposedFact, Research, Researcher},
    testkit::{Harness, gateway, scripted_model},
};

fn profile() -> Value {
    json!({
        "basics": {
            "name": "Alex Demo", "label": "Senior Engineer",
            "email": "alex.demo@example.com", "location": "Porto, Portugal",
            "url": "https://alex.example.com",
            "profiles": [{ "network": "GitHub", "url": "https://github.com/alexdemo" }],
        },
        "work": [
            { "name": "Example Org", "position": "Senior Engineer", "startDate": "2021-03",
              "summary": "Runs the billing platform in Rust on PostgreSQL." },
            { "name": "Sample Labs", "position": "Engineer", "startDate": "2017-06",
              "endDate": "2021-02", "summary": "Built the data pipeline." },
        ],
        "skills": [{ "name": "PostgreSQL" }],
    })
}

fn reply(value: &Value) -> (&'static str, String) {
    ("stop", value.to_string())
}

fn sentence(text: &str, kind: &str, sources: &[&str], why: &str, position: Option<u32>) -> Value {
    // The claim of "I assume you led a small team." is its plain statement; one
    // with a name the sentence never says must fall back to the sentence.
    let claim = match text {
        "I assume you led a small team." => "Led a small team",
        "I assume you built invoicing." => "Built invoicing at Acme",
        _ => "",
    };
    json!({ "text": text, "kind": kind, "sources": sources, "why": why, "claim": claim,
            "topic": "context", "position": position })
}

fn portrait_reply() -> (&'static str, String) {
    reply(&json!({ "sentences": [
        sentence("You run the billing platform in Rust on PostgreSQL.", "known", &["w0"], "", None),
        // Not in the profile: becomes an assumption, is not dropped.
        sentence("You run billing on Kubernetes.", "known", &["w0"], "", None),
        // Told, but cites no fact the person wrote.
        sentence("You told me you lead the team.", "told", &["w0"], "", None),
        sentence("I assume you led a small team.", "assumed", &[], "Senior Engineers often lead people.", Some(0)),
        sentence("I assume you built invoicing.", "assumed", &[], "Billing platforms are made of invoicing.", Some(0)),
        sentence("I assume you shipped pipelines.", "assumed", &[], "Data teams ship pipelines.", Some(1)),
    ]}))
}

async fn calls(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

async fn saved(h: &Harness, body: &Value) {
    let (status, saved) = h.json("PUT", "/api/v1/profile", body).await;
    assert_eq!(status, 200, "{saved}");
}

fn sentences(view: &Value) -> &Vec<Value> {
    view["sentences"].as_array().unwrap()
}

fn find<'a>(view: &'a Value, text: &str) -> &'a Value {
    sentences(view)
        .iter()
        .find(|s| s["text"] == text)
        .unwrap_or_else(|| panic!("no sentence {text}: {view}"))
}

/// An employer research port that knows `site` for Example Org only.
struct Fake {
    site: Option<&'static str>,
    facts: Vec<ProposedFact>,
    runs: AtomicUsize,
}

impl EmployerResearch for Fake {
    fn known_site<'a>(
        &'a self,
        _: &'a PgPool,
        _: Uuid,
        company: &'a str,
    ) -> BoxFuture<'a, Result<Option<String>>> {
        Box::pin(async move {
            Ok(self
                .site
                .filter(|_| company == "Example Org")
                .map(str::to_owned))
        })
    }

    fn set_site<'a>(
        &'a self,
        _: &'a AppState,
        _: Uuid,
        _: &'a str,
        url: &'a str,
    ) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { Ok(url.to_owned()) })
    }

    fn run<'a>(
        &'a self,
        _: &'a AppState,
        _: Uuid,
        company: &'a str,
        _: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<Research>> {
        Box::pin(async move {
            assert_eq!(company, "Example Org", "only a known site is read");
            self.runs.fetch_add(1, Ordering::SeqCst);
            Ok(Research {
                site: self.site.map(str::to_owned),
                read: true,
                facts: self.facts.clone(),
            })
        })
    }
}

fn research(facts: Vec<ProposedFact>) -> (Researcher, Arc<Fake>) {
    let fake = Arc::new(Fake {
        site: Some("https://example-org.test/"),
        facts,
        runs: AtomicUsize::new(0),
    });
    (Researcher(Some(fake.clone())), fake)
}

fn sourced(claim: &str, quote: &str) -> ProposedFact {
    ProposedFact {
        claim: claim.to_owned(),
        status: FactBasis::Sourced,
        quote: Some(quote.to_owned()),
        source_url: Some("https://example-org.test/about".to_owned()),
        reason: "Read on its site".to_owned(),
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_claim_the_profile_does_not_make_is_downgraded_never_dropped(db: PgPool) {
    let server = scripted_model(vec![portrait_reply()]).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;

    let (status, view) = h.post("/api/v1/profile/about/portrait").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["built"], true);
    assert_eq!(sentences(&view).len(), 6, "nothing dropped: {view}");
    assert_eq!(
        find(&view, "You run the billing platform in Rust on PostgreSQL.")["kind"],
        "known"
    );
    for text in [
        "You run billing on Kubernetes.",
        "You told me you lead the team.",
    ] {
        let s = find(&view, text);
        assert_eq!(s["kind"], "assumed", "{text}");
        assert_eq!(s["downgraded"], true, "{text}");
    }
    let led = find(&view, "I assume you led a small team.");
    assert_eq!(led["kind"], "assumed");
    assert_eq!(led["downgraded"], false);
    assert_eq!(led["why"], "Senior Engineers often lead people.");

    // One call per profile version: the second ask is the cached portrait.
    let (_, again) = h.post("/api/v1/profile/about/portrait").await;
    assert_eq!(again["sentences"], view["sentences"]);
    assert_eq!(calls(&server).await, 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_prompt_carries_no_contact_details(db: PgPool) {
    let server = scripted_model(vec![portrait_reply()]).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    h.post("/api/v1/profile/about/portrait").await;

    let sent = server.received_requests().await.unwrap();
    let body = String::from_utf8(sent[0].body.clone()).unwrap();
    for private in [
        "Alex Demo",
        "alex.demo@example.com",
        "Porto",
        "alex.example.com",
        "github.com",
        "alexdemo",
    ] {
        assert!(!body.contains(private), "{private} reached the model");
    }
    assert!(body.contains("Runs the billing platform"), "{body}");
}

#[test]
fn the_built_messages_leave_out_who_they_are() {
    let profile: crate::schema::Profile = serde_json::from_value(profile()).unwrap();
    let goals = crate::goals::Goals::default();
    let sources = prompt::sources_of(&profile, &goals, &[], 2026);
    let message = prompt::portrait_message(
        &sources,
        &prompt::roles_of(&profile),
        Some("en"),
        "2026-10-31",
    );
    let extra = prompt::extra_message(&profile, &goals, "I like trains.", Some("en"));
    for text in [message, extra] {
        for private in [
            "Alex Demo",
            "alex.demo@example.com",
            "Porto",
            "alex.example.com",
            "github.com",
        ] {
            assert!(!text.contains(private), "{private}");
        }
    }
    assert!(sources.iter().any(|s| s.id == "p.years"));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn only_sourced_facts_become_citable_sources_and_only_a_known_site_is_read(db: PgPool) {
    let server = scripted_model(vec![portrait_reply()]).await;
    let (port, fake) = research(vec![
        sourced("Builds invented ledgers", "builds invented ledgers"),
        // A port that says "sourced" without a page is held to the rule.
        ProposedFact {
            claim: "Has 900 employees".to_owned(),
            status: FactBasis::Sourced,
            quote: None,
            source_url: None,
            reason: String::new(),
        },
        ProposedFact {
            claim: "Is probably a startup".to_owned(),
            status: FactBasis::Assumed,
            quote: None,
            source_url: None,
            reason: "Typical".to_owned(),
        },
    ]);
    let h = Harness::with_research(db, Some(gateway(&server, None)), port);
    saved(&h, &profile()).await;

    let (_, view) = h.post("/api/v1/profile/about/portrait").await;
    // Example Org has a site and was read; Sample Labs has none, so nothing
    // was fetched for it.
    assert_eq!(fake.runs.load(Ordering::SeqCst), 1);
    let employers = view["employers"].as_array().unwrap();
    assert_eq!(employers[0]["company"], "Example Org");
    assert_eq!(employers[0]["state"], "read");
    assert_eq!(employers[0]["facts"], 1, "only the sourced fact counts");
    assert_eq!(employers[1]["company"], "Sample Labs");
    assert_eq!(employers[1]["state"], "no_site");

    let sent = server.received_requests().await.unwrap();
    let body = String::from_utf8(sent[0].body.clone()).unwrap();
    assert!(body.contains("Builds invented ledgers"));
    assert!(!body.contains("900 employees") && !body.contains("probably a startup"));
    // The portrait and the read are counted against the wizard's daily cap.
    assert_eq!(h.count("SELECT count(*) FROM profile.about_calls").await, 2);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn right_wrong_and_later_are_kept_and_only_right_is_saved(db: PgPool) {
    let server = scripted_model(vec![portrait_reply()]).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let (_, view) = h.post("/api/v1/profile/about/portrait").await;
    let before = h
        .count("SELECT version::bigint FROM profile.profiles")
        .await;

    let id = |text: &str| find(&view, text)["id"].as_str().unwrap().to_owned();
    let (led, built, shipped) = (
        id("I assume you led a small team."),
        id("I assume you built invoicing."),
        id("I assume you shipped pipelines."),
    );
    let (status, chosen) = h
        .json(
            "PUT",
            &format!("/api/v1/profile/about/sentences/{led}"),
            &json!({ "choice": "right" }),
        )
        .await;
    assert_eq!(status, 200, "{chosen}");
    h.json(
        "PUT",
        &format!("/api/v1/profile/about/sentences/{built}"),
        &json!({ "choice": "wrong" }),
    )
    .await;
    let (_, chosen) = h
        .json(
            "PUT",
            &format!("/api/v1/profile/about/sentences/{shipped}"),
            &json!({ "choice": "later" }),
        )
        .await;
    assert_eq!(
        find(&chosen, "I assume you led a small team.")["choice"],
        "right"
    );
    assert_eq!(
        find(&chosen, "I assume you built invoicing.")["choice"],
        "wrong"
    );
    assert_eq!(
        find(&chosen, "I assume you shipped pipelines.")["choice"],
        "later"
    );
    // A known sentence is not an assumption.
    let known = id("You run the billing platform in Rust on PostgreSQL.");
    let (status, _) = h
        .json(
            "PUT",
            &format!("/api/v1/profile/about/sentences/{known}"),
            &json!({ "choice": "right" }),
        )
        .await;
    assert_eq!(status, 404);

    // Nothing is in the profile before Save.
    let (_, profile_now) = h.get("/api/v1/profile").await;
    assert!(profile_now["profile"]["work"][0].get("facts").is_none());

    let (_, review) = h.get("/api/v1/profile/about/review").await;
    // The two downgraded sentences, the wrong one and the later one.
    assert_eq!(
        review["left_out"], 4,
        "unconfirmed assumptions are left out: {review}"
    );
    assert_eq!(review["groups"][0]["key"], "position");
    assert_eq!(review["groups"][0]["rows"][0]["source"], "confirmed");
    // The review shows what will be saved: the claim, not the sentence.
    assert_eq!(review["groups"][0]["rows"][0]["text"], "Led a small team");

    let (status, done) = h.post("/api/v1/profile/about/save").await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["saved"], 1);
    let (_, profile_now) = h.get("/api/v1/profile").await;
    let facts = profile_now["profile"]["work"][0]["facts"]
        .as_array()
        .unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0]["text"], "Led a small team");
    assert_eq!(facts[0]["quote"], "Led a small team");
    assert_eq!(facts[0]["status"], "confirmed_assumption");
    assert_eq!(facts[0]["basis"], "Senior Engineers often lead people.");
    assert!(profile_now["profile"]["work"][1].get("facts").is_none());
    let after = h
        .count("SELECT version::bigint FROM profile.profiles")
        .await;
    assert_eq!(after, before + 1, "one write, one new version");
    assert_eq!(done["profile_version"], after);

    // The pending state is gone.
    assert_eq!(h.count("SELECT count(*) FROM profile.about_you").await, 0);
    let (_, view) = h.get("/api/v1/profile/about").await;
    assert_eq!(view["built"], false);
}

fn changes_reply() -> (&'static str, String) {
    reply(&json!({ "changes": [
        { "field": "summary", "value": "Rails engineer who ran events, now building for restaurants and Kubernetes.",
          "position": null, "area": "context",
          "quote": "I ran events and a restaurant for 6 years", "sentence": "You ran events and a restaurant for 6 years." },
        { "field": "headline", "value": "Senior Rails engineer", "position": null, "area": "context",
          "quote": "a senior Rails role", "sentence": "You want a senior Rails role." },
        { "field": "skill", "value": "Postgres", "position": null, "area": "context",
          "quote": "Postgres", "sentence": "" },
        { "field": "skill", "value": "Elixir", "position": null, "area": "context",
          "quote": "Elixir", "sentence": "" },
        { "field": "background", "value": "6 years running events and a restaurant", "position": null,
          "area": "context", "quote": "I ran events and a restaurant for 6 years", "sentence": "Before engineering you ran events and a restaurant." },
        { "field": "goal_role", "value": "Senior Rails engineer", "position": null, "area": "context",
          "quote": "a senior Rails role", "sentence": "You want a senior Rails role." },
        { "field": "role_fact", "value": "Led 4 engineers", "position": 0, "area": "people",
          "quote": "I led 4 engineers at Example Org", "sentence": "At Example Org you led 4 engineers." },
    ]}))
}

const EXTRA: &str = "I ran events and a restaurant for 6 years. I want a senior Rails role, \
    ideally for restaurants. I use Postgres and Elixir. I led 4 engineers at Example Org.";

#[sqlx::test(migrations = "../../../migrations")]
async fn what_the_person_wrote_is_flagged_not_dropped_and_saved_in_one_write(db: PgPool) {
    let server = scripted_model(vec![changes_reply()]).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let before = h
        .count("SELECT version::bigint FROM profile.profiles")
        .await;

    let (status, view) = h
        .json(
            "POST",
            "/api/v1/profile/about/extra",
            &json!({ "text": EXTRA }),
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let changes = view["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 7, "every proposal kept");
    // Kubernetes is not in their words: flagged, still there.
    let summary = changes.iter().find(|c| c["field"] == "summary").unwrap();
    let flags = summary["flags"].as_array().unwrap();
    assert!(
        flags.iter().any(|f| f["claim"] == "Kubernetes"),
        "{summary}"
    );
    assert!(
        changes
            .iter()
            .filter(|c| c["field"] != "summary")
            .all(|c| c["flags"].as_array().unwrap().is_empty())
    );
    // They also show up in the portrait as told, highlighted as new.
    let told = find(&view, "You want a senior Rails role.");
    assert_eq!(told["kind"], "told");
    assert_eq!(told["new"], true);
    // Nothing is saved yet, and a flag holds Save back.
    let (status, held) = h.post("/api/v1/profile/about/save").await;
    assert_eq!(status, 409, "{held}");
    assert_eq!(held["code"], "changes_flagged");

    // "Use my own words": the whole answer, exactly.
    let id = summary["id"].as_str().unwrap();
    let (_, view) = h
        .json(
            "PUT",
            &format!("/api/v1/profile/about/changes/{id}"),
            &json!({ "action": "own_words" }),
        )
        .await;
    let summary = view["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["field"] == "summary")
        .unwrap()
        .clone();
    assert_eq!(summary["value"], EXTRA);
    assert_eq!(summary["resolution"], "own_words");
    assert!(summary["flags"].as_array().unwrap().is_empty());

    // Edit re-checks: a new claim is flagged again; "It's true" settles it.
    let (_, view) = h
        .json(
            "PUT",
            &format!("/api/v1/profile/about/changes/{id}"),
            &json!({ "action": "edit", "value": "Ran events at Acme" }),
        )
        .await;
    let again = view["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["field"] == "summary")
        .unwrap()
        .clone();
    assert!(
        again["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["claim"] == "Acme"),
        "{again}"
    );
    let (_, view) = h
        .json(
            "PUT",
            &format!("/api/v1/profile/about/changes/{id}"),
            &json!({ "action": "true" }),
        )
        .await;
    let settled = view["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["field"] == "summary")
        .unwrap()
        .clone();
    assert_eq!(settled["resolution"], "true");

    // Remove one, then Save.
    let background = view["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["field"] == "background")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, _) = h
        .json(
            "DELETE",
            &format!("/api/v1/profile/about/changes/{background}"),
            &json!({}),
        )
        .await;
    assert_eq!(status, 200);
    let (status, done) = h.post("/api/v1/profile/about/save").await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["saved"], 6);

    let (_, now) = h.get("/api/v1/profile").await;
    let p = &now["profile"];
    assert_eq!(p["basics"]["summary"], "Ran events at Acme");
    assert_eq!(p["basics"]["label"], "Senior Rails engineer");
    assert!(
        p["basics"].get("background").is_none(),
        "removed, so not saved"
    );
    let skills: Vec<&str> = p["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        skills,
        ["PostgreSQL", "Elixir"],
        "Postgres is PostgreSQL, not a second skill"
    );
    assert_eq!(p["work"][0]["facts"][0]["text"], "Led 4 engineers");
    assert_eq!(
        p["work"][0]["facts"][0]["quote"],
        "I led 4 engineers at Example Org"
    );
    let (_, goals) = h.get("/api/v1/goals").await;
    assert_eq!(goals["answers"]["roles"], json!(["Senior Rails engineer"]));
    let after = h
        .count("SELECT version::bigint FROM profile.profiles")
        .await;
    assert_eq!(after, before + 1, "seven changes, one version");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn accuracy_is_stored(db: PgPool) {
    let server = scripted_model(vec![portrait_reply()]).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let (status, view) = h
        .json(
            "PUT",
            "/api/v1/profile/about/accuracy",
            &json!({ "accuracy": "some" }),
        )
        .await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["accuracy"], "some");
    let (_, again) = h.get("/api/v1/profile/about").await;
    assert_eq!(again["accuracy"], "some");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_daily_cap_stops_a_call_before_it_is_made(db: PgPool) {
    let server = scripted_model(vec![portrait_reply()]).await;
    let h = Harness::new(db.clone(), Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    for _ in 0..DAILY_CAP {
        sqlx::query(
            "INSERT INTO profile.about_calls (id, account_id, kind, created_at)
             VALUES ($1, $2, 'portrait', now())",
        )
        .bind(Uuid::now_v7())
        .bind(h.account)
        .execute(&db)
        .await
        .unwrap();
    }
    let (status, body) = h.post("/api/v1/profile/about/portrait").await;
    assert_eq!(status, 429, "{body}");
    assert_eq!(body["code"], "about_daily_cap");
    let (status, body) = h
        .json(
            "POST",
            "/api/v1/profile/about/extra",
            &json!({ "text": "Hello" }),
        )
        .await;
    assert_eq!(status, 429, "{body}");
    assert_eq!(calls(&server).await, 0);
    let (_, view) = h.get("/api/v1/profile/about").await;
    assert_eq!(view["calls_today"], DAILY_CAP);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn it_needs_a_profile_and_a_model(db: PgPool) {
    let server = scripted_model(vec![portrait_reply()]).await;
    let h = Harness::new(db.clone(), Some(gateway(&server, None)));
    let (status, body) = h.post("/api/v1/profile/about/portrait").await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["code"], "profile_needed");
    let none = Harness::new(db, None);
    saved(&none, &profile()).await;
    let (status, body) = none.post("/api/v1/profile/about/portrait").await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["code"], "ai_unavailable");
}

#[test]
fn a_told_sentence_must_cite_what_the_person_wrote() {
    let sources = vec![
        Source {
            id: "w0".into(),
            kind: SourceKind::Profile,
            text: "Engineer at Example Org".into(),
            quote: None,
            url: None,
        },
        Source {
            id: "w0f0".into(),
            kind: SourceKind::Told,
            text: "Led 4 engineers".into(),
            quote: None,
            url: None,
        },
    ];
    let raw: RawPortrait = serde_json::from_value(json!({ "sentences": [
        { "text": "You led 4 engineers.", "kind": "told", "sources": ["w0f0"], "why": "", "topic": "people", "position": null },
        { "text": "You are an Engineer at Example Org.", "kind": "told", "sources": ["w0"], "why": "", "topic": "context", "position": null },
        { "text": "You are an Engineer at Example Org.", "kind": "known", "sources": ["nope"], "why": "", "topic": "context", "position": null },
    ]})).unwrap();
    let judged = judge::judge_portrait(raw, &sources, &[]);
    let kinds: Vec<_> = judged.sentences.iter().map(|s| s.kind).collect();
    use judge::SentenceKind::{Assumed, Told};
    assert_eq!(kinds, [Told, Assumed, Assumed]);
    assert_eq!(judged.downgraded, 2);
}

#[test]
fn a_change_whose_quote_is_not_theirs_is_flagged() {
    let raw: RawChanges = serde_json::from_value(json!({ "changes": [
        { "field": "headline", "value": "Chef", "position": null, "area": "context",
          "quote": "I am a chef", "sentence": "You are a chef." },
    ]}))
    .unwrap();
    let changes = judge::judge_changes(raw, "I cook at home.", &[]);
    assert_eq!(changes.len(), 1);
    assert!(changes[0].open());
    assert_eq!(changes[0].flags[0].kind, judge::FlagKind::Quote);
}

#[test]
fn a_claim_saying_more_than_its_sentence_falls_back_to_the_sentence_without_its_hedge() {
    let raw: RawPortrait = serde_json::from_value(json!({ "sentences": [
        { "text": "I assume that you led 4 engineers.", "kind": "assumed", "sources": [],
          "why": "Usual.", "claim": "Led 4 engineers", "topic": "people", "position": null },
        { "text": "I assume that you led 4 engineers.", "kind": "assumed", "sources": [],
          "why": "Usual.", "claim": "Led 9 engineers at Acme", "topic": "people", "position": null },
        { "text": "Je suppose que vous dirigiez une équipe.", "kind": "assumed", "sources": [],
          "why": "", "claim": "", "topic": "people", "position": null },
    ]})).unwrap();
    let judged = judge::judge_portrait(raw, &[], &[]);
    let claims: Vec<_> = judged
        .sentences
        .iter()
        .map(|s| s.claim.clone().unwrap())
        .collect();
    assert_eq!(
        claims,
        [
            "Led 4 engineers",
            "You led 4 engineers.",
            "Vous dirigiez une équipe."
        ]
    );
    assert_eq!(judge::without_hedge("I assume it"), "It");
}
