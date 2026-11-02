//! "Tell me about you" over HTTP, against a fake model that answers in turn
//! and a fake employer research port. All data is invented.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use applicamp_kernel::{AppState, BoxFuture, Error, Result};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::MockServer;

use super::{
    DAILY_CAP,
    gaps::{self, Gap, GapKind, Inputs},
    judge::{
        self, Change, Field, RawChanges, RawPortrait, Sentence, SentenceKind, Source, SourceKind,
    },
    prompt, questions,
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
    /// Reads that worked.
    runs: AtomicUsize,
    /// Reads tried, whatever came of them.
    attempts: AtomicUsize,
    /// How many of the next reads fail (the site is unreachable).
    fail: AtomicUsize,
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

    fn known_facts<'a>(
        &'a self,
        _: &'a PgPool,
        _: Uuid,
        company: &'a str,
    ) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            Ok(if company == "Example Org" {
                self.facts.iter().map(|f| f.claim.clone()).collect()
            } else {
                Vec::new()
            })
        })
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
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if self
                .fail
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                return Err(Error::Unavailable("site_unreachable"));
            }
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
        attempts: AtomicUsize::new(0),
        fail: AtomicUsize::new(0),
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
async fn one_employer_that_cannot_be_read_does_not_stop_the_portrait_and_try_again_reads_only_it(
    db: PgPool,
) {
    let server = scripted_model(vec![portrait_reply()]).await;
    let (port, fake) = research(vec![sourced(
        "Builds invented ledgers",
        "builds invented ledgers",
    )]);
    fake.fail.store(1, Ordering::SeqCst);
    let h = Harness::with_research(db, Some(gateway(&server, None)), port);
    saved(&h, &profile()).await;

    // The read fails; the portrait is made all the same, from the profile.
    let (status, view) = h.post("/api/v1/profile/about/portrait").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["built"], true);
    assert_eq!(sentences(&view).len(), 6);
    assert_eq!(view["employers"][0]["state"], "failed");
    assert_eq!(view["employers"][0]["error"], "site_unreachable");
    assert_eq!(calls(&server).await, 1, "no model call for the failed read");
    // It was tried, and charged to the day's limit, with the portrait.
    assert_eq!(h.count("SELECT count(*) FROM profile.about_calls").await, 2);
    assert!(
        !String::from_utf8(server.received_requests().await.unwrap()[0].body.clone())
            .unwrap()
            .contains("Builds invented ledgers")
    );

    // Making the portrait again does not read the failed one by itself.
    let (_, again) = h.post("/api/v1/profile/about/portrait?again=true").await;
    assert_eq!(again["employers"][0]["state"], "failed");
    assert_eq!(fake.attempts.load(Ordering::SeqCst), 1);

    // Try again reads that employer, and only it; the portrait is untouched.
    let (status, retried) = h.post("/api/v1/profile/about/employers/0/retry").await;
    assert_eq!(status, 200, "{retried}");
    assert_eq!(retried["employers"][0]["state"], "read");
    assert_eq!(retried["employers"][0]["facts"], 1);
    assert_eq!(retried["employers"][0]["error"], Value::Null);
    assert_eq!(fake.attempts.load(Ordering::SeqCst), 2);
    assert_eq!(retried["employers"][1]["state"], "no_site");
    // The retry counts against the cap like any read: the failed read, two
    // portraits and the retry.
    assert_eq!(h.count("SELECT count(*) FROM profile.about_calls").await, 4);

    // A position with no site, or none at all, is not found.
    let (status, _) = h.post("/api/v1/profile/about/employers/1/retry").await;
    assert_eq!(status, 404);
    let (status, _) = h.post("/api/v1/profile/about/employers/9/retry").await;
    assert_eq!(status, 404);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_retry_that_fails_again_says_so_on_that_employer_and_still_counts(db: PgPool) {
    let server = scripted_model(vec![portrait_reply()]).await;
    let (port, fake) = research(Vec::new());
    fake.fail.store(2, Ordering::SeqCst);
    let h = Harness::with_research(db, Some(gateway(&server, None)), port);
    saved(&h, &profile()).await;
    h.post("/api/v1/profile/about/portrait").await;

    let (status, view) = h.post("/api/v1/profile/about/employers/0/retry").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["employers"][0]["state"], "failed");
    assert_eq!(view["employers"][0]["error"], "site_unreachable");
    assert_eq!(h.count("SELECT count(*) FROM profile.about_calls").await, 3);
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

// ---- the questions about the gaps (#44) -------------------------------------------

fn question_reply(
    question: &str,
    why: &str,
    tag: &str,
    choices: &[&str],
) -> (&'static str, String) {
    reply(&json!({ "question": question, "why": why, "tag": tag, "choices": choices }))
}

fn answer_reply(changes: Value) -> (&'static str, String) {
    reply(&json!({ "changes": changes }))
}

async fn body_of(server: &MockServer, call: usize) -> String {
    let sent = server.received_requests().await.unwrap();
    String::from_utf8(sent[call].body.clone()).unwrap()
}

/// A sentence of a portrait.
fn sent(id: &str, kind: SentenceKind, text: &str, position: Option<u32>) -> Sentence {
    Sentence {
        id: id.to_owned(),
        text: text.to_owned(),
        kind,
        sources: Vec::new(),
        why: Some("It is usual.".to_owned()),
        claim: None,
        downgraded: false,
        topic: crate::schema::Area::Context,
        position,
        role_key: None,
        urls: Vec::new(),
        new: false,
    }
}

fn three_roles() -> crate::schema::Profile {
    serde_json::from_value(json!({
        "basics": { "label": "Lead Engineer", "summary": "I build calm backends." },
        "work": [
            { "name": "Example Org", "position": "Lead Engineer", "startDate": "2021-03",
              "summary": "Runs the billing platform in Rust and Kafka." },
            { "name": "Sample Labs", "position": "Engineer", "startDate": "2017-06",
              "endDate": "2021-02", "highlights": ["Cut run time by 40%"] },
            { "name": "Third Co", "position": "Engineer", "startDate": "2014-01",
              "endDate": "2017-05" },
        ],
        "skills": [],
    }))
    .unwrap()
}

fn ids(list: &[Gap]) -> Vec<&str> {
    list.iter().map(|g| g.id.as_str()).collect()
}

#[test]
fn the_gaps_are_ranked_assumptions_first_then_results_then_skills_then_the_rest() {
    use std::collections::BTreeMap;
    let profile = three_roles();
    let goals = crate::goals::Goals::default();
    let portrait = vec![
        sent(
            "s1",
            SentenceKind::Assumed,
            "I assume you led 3 to 6 engineers.",
            Some(0),
        ),
        sent(
            "s2",
            SentenceKind::Assumed,
            "I assume you shipped pipelines.",
            Some(1),
        ),
        sent("s3", SentenceKind::Known, "You were Lead Engineer.", None),
    ];
    let choices = BTreeMap::from([("s1".to_owned(), super::Choice::Later)]);
    let inputs = Inputs {
        profile: &profile,
        goals: &goals,
        portrait: &portrait,
        choices: &choices,
        changes: &[],
        accuracy: None,
        dismissed: &[],
    };
    let list = gaps::gaps(&inputs);
    let order = ids(&list);
    // Assumptions to settle, the latest role's results, empty skills.
    assert_eq!(
        &order[..3],
        ["assumed:s1", "results:0", "skills"],
        "{order:?}"
    );
    // The rest follows: a team size for a lead (the title is a lead), the
    // technologies the role names, results on the third role (the second has a
    // number), then the goals.
    for expected in ["team:0", "skills_found", "results:2", "goals"] {
        assert!(order.contains(&expected), "{expected} in {order:?}");
    }
    assert!(
        !order.contains(&"results:1"),
        "a number is a result: {order:?}"
    );
    let at = |id: &str| order.iter().position(|o| *o == id).unwrap();
    assert!(at("team:0") < at("results:2") && at("results:2") < at("skills_found"));
    assert!(at("skills_found") < at("goals"));
    // No summary or headline gap: the profile has them. A basis says what is
    // missing, in words.
    assert!(!order.contains(&"summary") && !order.contains(&"headline"));
    let team = list.iter().find(|g| g.id == "team:0").unwrap();
    assert!(team.basis.contains("Lead Engineer") && team.basis.contains("does not say how many"));
    let found = list.iter().find(|g| g.id == "skills_found").unwrap();
    assert_eq!(found.names, ["Kafka", "Rust"]);
    // Context is never asked.
    assert!(
        list.iter()
            .all(|g| g.kind == GapKind::Assumed || g.area != Some(crate::schema::Area::Context)),
        "{order:?}"
    );
    // The assumption left for later is a question about its own position.
    assert_eq!(list[0].position, Some(0));
    assert!(list[0].basis.contains("ask me later") && list[0].basis.contains("I assume you led"));
}

#[test]
fn the_questions_stop_when_the_gaps_are_out_or_at_the_limit_the_accuracy_sets() {
    use std::collections::BTreeMap;
    let profile = three_roles();
    let goals = crate::goals::Goals::default();
    let no_choices = BTreeMap::new();
    let inputs = |accuracy| Inputs {
        profile: &profile,
        goals: &goals,
        portrait: &[],
        choices: &no_choices,
        changes: &[],
        accuracy,
        dismissed: &[],
    };
    assert_eq!(gaps::limit(None), 12);
    assert_eq!(gaps::limit(Some(super::Accuracy::Yes)), 8);
    assert_eq!(gaps::limit(Some(super::Accuracy::No)), 16);
    let everything = gaps::gaps(&inputs(None));
    assert!(
        everything.len() > 12,
        "more gaps than questions: {}",
        everything.len()
    );
    let mut progress = questions::Progress::default();
    assert_eq!(questions::remaining(&inputs(None), &progress).len(), 12);
    assert_eq!(
        questions::remaining(&inputs(Some(super::Accuracy::Yes)), &progress).len(),
        8
    );
    assert_eq!(
        questions::remaining(&inputs(Some(super::Accuracy::No)), &progress).len(),
        16
    );
    // Answered and skipped gaps are not asked again, and count toward the limit.
    for (n, gap) in everything.iter().take(12).enumerate() {
        if n % 2 == 0 {
            progress.answered.push(gap.id.clone());
        } else {
            progress.skipped.push(gap.id.clone());
        }
    }
    assert!(
        questions::remaining(&inputs(None), &progress).is_empty(),
        "12 asked: stop"
    );
    assert_eq!(
        questions::remaining(&inputs(Some(super::Accuracy::No)), &progress).len(),
        4
    );
    // What the person has told closes a gap: a number on the latest role's
    // results, a team size, a skill.
    let told = |field, value: &str, area, position| Change {
        id: "c1".into(),
        field,
        value: value.into(),
        position,
        role_key: None,
        area,
        quote: value.into(),
        sentence: String::new(),
        flags: Vec::new(),
        resolution: None,
        origin: None,
    };
    let area = crate::schema::Area::Results;
    let changes = vec![
        told(Field::RoleFact, "Cut costs by 30%", area, Some(0)),
        told(
            Field::RoleFact,
            "Led 4 engineers",
            crate::schema::Area::People,
            Some(0),
        ),
        told(Field::Skill, "Elixir", area, None),
        told(Field::GoalRole, "Staff engineer", area, None),
    ];
    let with = Inputs {
        changes: &changes,
        ..inputs(None)
    };
    let list = gaps::gaps(&with);
    let order = ids(&list);
    for closed in ["results:0", "team:0", "skills", "goals"] {
        assert!(!order.contains(&closed), "{closed} is answered: {order:?}");
    }
    // A skill they told, which no role mentions, is a question about it.
    let claim = list.iter().find(|g| g.kind == GapKind::Claim).unwrap();
    assert!(claim.basis.contains("Elixir") && claim.basis.contains("none of their roles"));
}

#[test]
fn not_really_also_asks_about_the_assumptions_that_were_wrong() {
    use std::collections::BTreeMap;
    let profile = three_roles();
    let goals = crate::goals::Goals::default();
    let portrait = vec![sent(
        "s1",
        SentenceKind::Assumed,
        "I assume you led 3 to 6 engineers.",
        Some(0),
    )];
    let choices = BTreeMap::from([("s1".to_owned(), super::Choice::Wrong)]);
    let of = |accuracy| {
        gaps::gaps(&Inputs {
            profile: &profile,
            goals: &goals,
            portrait: &portrait,
            choices: &choices,
            changes: &[],
            accuracy,
            dismissed: &[],
        })
    };
    assert!(!ids(&of(Some(super::Accuracy::Some))).contains(&"assumed:s1"));
    let list = of(Some(super::Accuracy::No));
    assert_eq!(list[0].id, "assumed:s1");
    assert!(list[0].basis.contains("wrong"));
}

#[test]
fn the_result_of_a_question_is_worth_points_of_strength_only_where_the_score_counts_it() {
    let profile = three_roles();
    let goals = crate::goals::Goals::default();
    let now = chrono::Utc::now();
    let gap = |id: &str, kind, position| Gap {
        id: id.to_owned(),
        kind,
        position,
        area: None,
        basis: String::new(),
        names: Vec::new(),
    };
    // Role 0 has no result: a numbered one earns the third of 24 the role is worth.
    assert_eq!(
        gaps::gain(
            &gap("results:0", GapKind::Results, Some(0)),
            &profile,
            &goals,
            now
        ),
        8
    );
    // Empty skills: three skills are 3 of 5 of 12.
    assert_eq!(
        gaps::gain(&gap("skills", GapKind::Skills, None), &profile, &goals, now),
        7
    );
    // A team size is not a part of the score.
    assert_eq!(
        gaps::gain(
            &gap("team:0", GapKind::TeamSize, Some(0)),
            &profile,
            &goals,
            now
        ),
        0
    );
}

#[test]
fn the_fallback_reason_for_a_results_question_is_plain() {
    let profile: crate::schema::Profile = serde_json::from_value(profile()).unwrap();
    let work = &profile.work[0];
    let said = gaps::results_basis(work, true);
    assert_eq!(
        said,
        "Your role at Example Org says what you did, but not what changed because of it."
    );
    let none = gaps::results_basis(work, false);
    assert!(none.starts_with("Your role at Example Org"), "{none}");
    for basis in [&said, &none] {
        assert!(
            !basis.contains("number") && !basis.contains("states"),
            "{basis}"
        );
    }
    // The question that has no reason of its own is asked with this one.
    let gap = Gap {
        id: "results:0".into(),
        kind: GapKind::Results,
        position: Some(0),
        area: Some(crate::schema::Area::Results),
        basis: said.clone(),
        names: Vec::new(),
    };
    let asked = questions::judge_question(
        questions::RawQuestion {
            question: "What got better because of your work?".into(),
            why: String::new(),
            tag: String::new(),
            choices: Vec::new(),
        },
        &gap,
    )
    .unwrap();
    assert_eq!(asked.why, said);
}

#[test]
fn a_question_always_says_why_and_an_assumption_is_tagged_as_one() {
    let gap = Gap {
        id: "assumed:s1".into(),
        kind: GapKind::Assumed,
        position: Some(0),
        area: None,
        basis: "I assumed you led a team.".into(),
        names: Vec::new(),
    };
    let raw = |why: &str, tag: &str| questions::RawQuestion {
        question: "  Did you lead a team?  ".into(),
        why: why.into(),
        tag: tag.into(),
        choices: ["1 to 2", "3 to 6", "3 to 6", " "]
            .into_iter()
            .map(String::from)
            .chain((0..9).map(|n| format!("choice {n}")))
            .collect(),
    };
    let asked = questions::judge_question(raw("", "unknown"), &gap).unwrap();
    assert_eq!(asked.question, "Did you lead a team?");
    assert_eq!(
        asked.why, "I assumed you led a team.",
        "never without a reason"
    );
    assert_eq!(
        asked.tag,
        questions::AboutQuestionTag::Assumed,
        "an assumption is tagged as one"
    );
    assert_eq!(
        asked.choices.len(),
        6,
        "at most six, no repeats, none empty"
    );
    assert_eq!(asked.choices[..2], ["1 to 2", "3 to 6"]);
    let unknown = Gap {
        kind: GapKind::TeamSize,
        ..gap.clone()
    };
    assert_eq!(
        questions::judge_question(raw("Why.", "told"), &unknown)
            .unwrap()
            .tag,
        questions::AboutQuestionTag::Told
    );
    assert_eq!(
        questions::judge_question(raw("Why.", "nonsense"), &unknown)
            .unwrap()
            .tag,
        questions::AboutQuestionTag::Unknown
    );
    let empty = questions::RawQuestion {
        question: " ".into(),
        why: String::new(),
        tag: String::new(),
        choices: Vec::new(),
    };
    assert!(questions::judge_question(empty, &gap).is_none());
}

#[test]
fn the_prompt_holds_the_gaps_context_and_no_contact_details() {
    let profile: crate::schema::Profile = serde_json::from_value(profile()).unwrap();
    let gap = Gap {
        id: "results:1".into(),
        kind: GapKind::Results,
        position: Some(1),
        area: Some(crate::schema::Area::Results),
        basis: "Engineer at Sample Labs states no results.".into(),
        names: Vec::new(),
    };
    let message = prompt::question_message(
        &profile,
        &gap,
        None,
        &["Sells door-access systems to property managers".to_owned()],
        Some("en"),
        "2026-10-31",
    );
    // What is known about the employer is shown, so it is never asked.
    assert!(message.contains("door-access systems"), "{message}");
    // This position's words and nothing of the other one's, nor of the person.
    assert!(message.contains("Built the data pipeline."), "{message}");
    assert!(!message.contains("Runs the billing platform"), "{message}");
    for private in [
        "Alex Demo",
        "alex.demo@example.com",
        "Porto",
        "alex.example.com",
        "github.com",
    ] {
        assert!(!message.contains(private), "{private}");
    }
    // The model is told never to ask what it knows or can look up.
    assert!(prompt::QUESTION_INSTRUCTIONS.contains("Never ask what the context already answers"));
    assert!(prompt::QUESTION_INSTRUCTIONS.contains("what a company sells"));
    // Plain words, with outcomes to pick from for the results question.
    for needle in [
        "friendly recruiter",
        "Never say \"result\"",
        "never write \"how do you know it\"",
        "2 to 4 concrete",
        "Never add a \"Something else\" choice",
    ] {
        assert!(prompt::QUESTION_INSTRUCTIONS.contains(needle), "{needle}");
    }
    let answer = prompt::answer_message(
        &profile,
        &crate::goals::Goals::default(),
        &gap,
        "What was one result?",
        "Cut runs by 40%",
        Some("en"),
    );
    for private in [
        "Alex Demo",
        "alex.demo@example.com",
        "Porto",
        "alex.example.com",
        "github.com",
    ] {
        assert!(!answer.contains(private), "{private}");
    }
}

/// A portrait, then the person leaves "I assume you shipped pipelines." for
/// later; the wizard is at the questions.
async fn at_the_questions(
    db: PgPool,
    script: Vec<(&'static str, String)>,
) -> (Harness, MockServer, String) {
    let mut replies = vec![portrait_reply()];
    replies.extend(script);
    let server = scripted_model(replies).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let (_, view) = h.post("/api/v1/profile/about/portrait").await;
    let id = find(&view, "I assume you shipped pipelines.")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, _) = h
        .json(
            "PUT",
            &format!("/api/v1/profile/about/sentences/{id}"),
            &json!({ "choice": "later" }),
        )
        .await;
    assert_eq!(status, 200);
    (h, server, id)
}

const NEXT: &str = "/api/v1/profile/about/questions/next";

#[sqlx::test(migrations = "../../../migrations")]
async fn an_assumption_left_for_later_becomes_the_first_question_with_its_reason(db: PgPool) {
    let (h, server, id) = at_the_questions(
        db,
        vec![question_reply(
            "Did you build the pipelines at Sample Labs?",
            "I assumed you shipped pipelines at Sample Labs because data teams do. Your profile doesn't say.",
            "assumed",
            &["Yes, I built them", "I only used them", "None of these"],
        )],
    )
    .await;
    let (status, view) = h.post(NEXT).await;
    assert_eq!(status, 200, "{view}");
    let q = &view["questions"];
    assert_eq!(q["current"]["id"], format!("assumed:{id}"));
    assert_eq!(q["current"]["tag"], "assumed");
    assert!(
        q["current"]["why"]
            .as_str()
            .unwrap()
            .contains("I assumed you shipped pipelines")
    );
    assert_eq!(q["current"]["choices"].as_array().unwrap().len(), 3);
    assert_eq!(q["current"]["position"], 1);
    assert_eq!(q["answered"], 0);
    assert!(q["left"].as_u64().unwrap() >= 2);
    assert_eq!(q["done"], false);
    assert!(q["current"]["strength_after"].as_u64().unwrap() >= q["strength"].as_u64().unwrap());

    // The model saw the gap, and only that position and that assumption.
    let sent = body_of(&server, 1).await;
    assert!(sent.contains("ask me later"), "{sent}");
    assert!(sent.contains("I assume you shipped pipelines."), "{sent}");
    assert!(sent.contains("Built the data pipeline."), "{sent}");
    assert!(
        !sent.contains("Runs the billing platform"),
        "the other role is not shown: {sent}"
    );
    for private in [
        "Alex Demo",
        "alex.demo@example.com",
        "Porto",
        "alex.example.com",
        "github.com",
    ] {
        assert!(!sent.contains(private), "{private} reached the model");
    }
    // The page reloading asks nothing again.
    let (_, again) = h.post(NEXT).await;
    assert_eq!(again["questions"]["current"], q["current"]);
    assert_eq!(calls(&server).await, 2);
    assert_eq!(
        h.count("SELECT count(*) FROM profile.about_calls WHERE kind = 'question'")
            .await,
        1
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_answer_becomes_changes_flagged_and_never_dropped_and_the_portrait_gains_their_sentence(
    db: PgPool,
) {
    let (h, server, id) = at_the_questions(
        db,
        vec![
            question_reply(
                "Did you build them?",
                "I assumed you shipped pipelines.",
                "assumed",
                &["Yes, I built them", "None of these"],
            ),
            answer_reply(json!([
                // Acme is not in their words: flagged, kept.
                { "field": "role_fact", "value": "Led the pipeline work at Acme", "position": null,
                  "area": "your_part", "quote": "I led the pipeline work",
                  "sentence": "You led the pipeline work." },
                { "field": "skill", "value": "Airflow", "position": null, "area": "context",
                  "quote": "Airflow", "sentence": "" },
            ])),
        ],
    )
    .await;
    h.post(NEXT).await;
    let gap = format!("assumed:{id}");

    // A choice the question did not offer is refused.
    let (status, body) = h
        .json(
            "POST",
            &format!("/api/v1/profile/about/questions/{gap}/answer"),
            &json!({ "text": "x", "choices": ["Elsewhere"] }),
        )
        .await;
    assert_eq!(status, 400, "{body}");
    // So is an answer to a question that is not on screen.
    let (status, _) = h
        .json(
            "POST",
            "/api/v1/profile/about/questions/results:0/answer",
            &json!({ "text": "x" }),
        )
        .await;
    assert_eq!(status, 404);

    let (status, view) = h
        .json(
            "POST",
            &format!("/api/v1/profile/about/questions/{gap}/answer"),
            &json!({ "text": "I led the pipeline work. Airflow was the scheduler.", "choices": ["Yes, I built them"] }),
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let changes = view["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2, "nothing dropped: {view}");
    let fact = &changes[0];
    assert_eq!(fact["id"], "q1");
    assert_eq!(
        fact["position"], 1,
        "a fact belongs to the position asked about"
    );
    assert!(
        fact["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["claim"] == "Acme"),
        "{fact}"
    );
    assert!(changes[1]["flags"].as_array().unwrap().is_empty());
    let q = &view["questions"];
    assert_eq!(q["answered"], 1);
    assert_eq!(q["updated"], json!(["q1", "q2"]));
    assert_eq!(q["current"], Value::Null);
    // The portrait gains their sentence at once, marked new.
    let told = find(&view, "You led the pipeline work.");
    assert_eq!(
        (told["kind"].as_str(), told["new"].as_bool()),
        (Some("told"), Some(true))
    );
    // Their picked choice reached the model as their words.
    assert!(body_of(&server, 2).await.contains("Yes, I built them"));
    assert_eq!(
        h.count("SELECT count(*) FROM profile.about_calls WHERE kind = 'answer'")
            .await,
        1
    );

    // A flag holds Save back, as for "Anything else"; nothing is written yet.
    let (status, held) = h.post("/api/v1/profile/about/save").await;
    assert_eq!(status, 409, "{held}");
    assert_eq!(held["code"], "changes_flagged");
    let (_, now) = h.get("/api/v1/profile").await;
    assert!(now["profile"]["work"][1].get("facts").is_none());
    // "It's true" settles it; Save writes the fact on the position asked about.
    h.json(
        "PUT",
        "/api/v1/profile/about/changes/q1",
        &json!({ "action": "true" }),
    )
    .await;
    let (status, done) = h.post("/api/v1/profile/about/save").await;
    assert_eq!(status, 200, "{done}");
    let (_, now) = h.get("/api/v1/profile").await;
    assert_eq!(
        now["profile"]["work"][1]["facts"][0]["text"],
        "Led the pipeline work at Acme"
    );
    assert_eq!(now["profile"]["work"][1]["facts"][0]["area"], "your_part");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn picking_outcomes_without_typing_anything_is_an_answer_and_makes_facts_in_their_pick(
    db: PgPool,
) {
    let (h, server, id) = at_the_questions(
        db,
        vec![
            question_reply(
                "At Example Org you were an engineer on a billing platform. What got better \
                 because of your work?",
                "Your Example Org role says what you did, but not what changed because of it.",
                "assumed",
                &["Fewer failed payments", "Faster invoicing"],
            ),
            answer_reply(json!([
                { "field": "role_fact", "value": "Fewer failed payments", "position": null,
                  "area": "results", "quote": "Fewer failed payments",
                  "sentence": "You cut failed payments." },
            ])),
        ],
    )
    .await;
    let (_, asked) = h.post(NEXT).await;
    assert_eq!(
        asked["questions"]["current"]["choices"],
        json!(["Fewer failed payments", "Faster invoicing"])
    );
    let gap = format!("assumed:{id}");
    let (status, view) = h
        .json(
            "POST",
            &format!("/api/v1/profile/about/questions/{gap}/answer"),
            &json!({ "text": "", "choices": ["Fewer failed payments"] }),
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let changes = view["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1, "{view}");
    assert_eq!(changes[0]["value"], "Fewer failed payments");
    assert!(changes[0]["flags"].as_array().unwrap().is_empty(), "{view}");
    assert_eq!(view["questions"]["answered"], 1);
    // The pick, and only the pick, reached the model as their words.
    assert!(body_of(&server, 2).await.contains("Fewer failed payments"));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn skip_marks_the_gap_skipped_and_asks_the_next_without_a_call_of_its_own(db: PgPool) {
    let (h, server, id) = at_the_questions(
        db,
        vec![
            question_reply(
                "Did you build them?",
                "I assumed you shipped pipelines.",
                "assumed",
                &[],
            ),
            question_reply(
                "What was one result at Example Org?",
                "Your profile states no results for Senior Engineer at Example Org.",
                "unknown",
                &[],
            ),
        ],
    )
    .await;
    h.post(NEXT).await;
    let gap = format!("assumed:{id}");
    let (status, view) = h
        .post(&format!("/api/v1/profile/about/questions/{gap}/skip"))
        .await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["questions"]["skipped"], 1);
    assert_eq!(view["questions"]["current"], Value::Null);
    assert_eq!(
        calls(&server).await,
        2,
        "skipping asks nothing of the model"
    );
    // Skipping what is not on screen is a 404.
    let (status, _) = h
        .post(&format!("/api/v1/profile/about/questions/{gap}/skip"))
        .await;
    assert_eq!(status, 404);

    let (_, next) = h.post(NEXT).await;
    assert_eq!(next["questions"]["current"]["id"], "results:0");
    assert_eq!(next["questions"]["current"]["tag"], "unknown");
    assert!(
        next["questions"]["current"]["why"]
            .as_str()
            .unwrap()
            .contains("no results")
    );
    assert_eq!(calls(&server).await, 3);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn that_is_enough_for_now_ends_the_questions_for_free(db: PgPool) {
    let (h, server, _) = at_the_questions(db, vec![]).await;
    let (status, view) = h.post("/api/v1/profile/about/questions/enough").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["questions"]["done"], true);
    let (_, after) = h.post(NEXT).await;
    assert_eq!(after["questions"]["current"], Value::Null);
    assert_eq!(calls(&server).await, 1, "only the portrait");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_question_needs_the_portrait_and_obeys_the_daily_cap(db: PgPool) {
    let server = scripted_model(vec![
        portrait_reply(),
        question_reply("Q?", "Why.", "unknown", &[]),
    ])
    .await;
    let h = Harness::new(db.clone(), Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let (status, body) = h.post(NEXT).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["code"], "portrait_needed");

    h.post("/api/v1/profile/about/portrait").await;
    for _ in 0..DAILY_CAP {
        sqlx::query(
            "INSERT INTO profile.about_calls (id, account_id, kind, created_at)
             VALUES ($1, $2, 'question', now())",
        )
        .bind(Uuid::now_v7())
        .bind(h.account)
        .execute(&db)
        .await
        .unwrap();
    }
    let (status, body) = h.post(NEXT).await;
    assert_eq!(status, 429, "{body}");
    assert_eq!(body["code"], "about_daily_cap");
    assert_eq!(calls(&server).await, 1, "refused before the call");
    assert_eq!(
        DAILY_CAP, 60,
        "a full run (a portrait, five employers, a text, 16 questions and their answers) fits twice over"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn anything_else_again_keeps_what_the_answers_proposed(db: PgPool) {
    let (h, _, id) = at_the_questions(
        db,
        vec![
            question_reply("Did you build them?", "I assumed you shipped pipelines.", "assumed", &[]),
            answer_reply(json!([
                { "field": "role_fact", "value": "Built the pipelines", "position": 1, "area": "your_part",
                  "quote": "I built them", "sentence": "You built the pipelines." },
            ])),
            changes_reply(),
        ],
    )
    .await;
    h.post(NEXT).await;
    h.json(
        "POST",
        &format!("/api/v1/profile/about/questions/assumed:{id}/answer"),
        &json!({ "text": "I built them" }),
    )
    .await;
    let (_, view) = h
        .json(
            "POST",
            "/api/v1/profile/about/extra",
            &json!({ "text": EXTRA }),
        )
        .await;
    let changes = view["changes"].as_array().unwrap();
    assert_eq!(
        changes.len(),
        8,
        "the seven from the text and the answer's: {view}"
    );
    assert!(
        changes
            .iter()
            .any(|c| c["id"] == "q1" && c["value"] == "Built the pipelines")
    );
}

#[test]
fn merging_answers_gives_each_change_an_id_of_its_own_and_a_newer_summary_replaces_the_older() {
    let change = |field, value: &str| Change {
        id: "c1".into(),
        field,
        value: value.into(),
        position: None,
        role_key: None,
        area: crate::schema::Area::Context,
        quote: value.into(),
        sentence: String::new(),
        flags: Vec::new(),
        resolution: None,
        origin: Some("g".into()),
    };
    let mut pending = vec![
        change(Field::Summary, "Old summary"),
        change(Field::Skill, "Rust"),
    ];
    pending[0].id = "q1".into();
    pending[1].id = "c1".into();
    let added = questions::merge(
        &mut pending,
        vec![
            change(Field::Summary, "New summary"),
            change(Field::Skill, "rust"),
            change(Field::Skill, "Elixir"),
        ],
    );
    assert_eq!(
        added,
        ["q2", "q3"],
        "a skill already there is not added twice"
    );
    let values: Vec<&str> = pending.iter().map(|c| c.value.as_str()).collect();
    assert_eq!(values, ["Rust", "New summary", "Elixir"]);
}

#[test]
fn a_role_nobody_described_is_asked_about_before_its_results() {
    use std::collections::BTreeMap;
    let profile: crate::schema::Profile = serde_json::from_value(json!({
        "basics": {},
        "work": [
            { "name": "Example Org", "position": "Full Stack Developer", "startDate": "2023-07" },
            { "name": "Sample Labs", "position": "Full Stack Developer", "startDate": "2021-01",
              "endDate": "2023-06" },
        ],
        "skills": [{ "name": "Ruby" }, { "name": "TypeScript" }],
    }))
    .unwrap();
    let goals = crate::goals::Goals {
        languages_for_work: vec!["en".to_owned(), "pt-PT".to_owned()],
        ..crate::goals::Goals::default()
    };
    let choices = BTreeMap::new();
    let inputs = Inputs {
        profile: &profile,
        goals: &goals,
        portrait: &[],
        choices: &choices,
        changes: &[],
        accuracy: None,
        dismissed: &[],
    };
    let list = gaps::gaps(&inputs);
    let order = ids(&list);
    let at = |id: &str| {
        order
            .iter()
            .position(|o| *o == id)
            .unwrap_or_else(|| panic!("{id} in {order:?}"))
    };
    // What they did there first, then what changed, for each recent role.
    assert_eq!(&order[..2], ["area:0:your_part", "results:0"], "{order:?}");
    assert!(at("area:1:your_part") < at("results:1"));
    // Languages early (searches check a posting's languages), naming what the
    // goals say they work in; a short skills list; education after the basics.
    assert!(at("results:0") < at("languages") && at("languages") < at("results:1"));
    let languages = list.iter().find(|g| g.id == "languages").unwrap();
    assert_eq!(languages.kind, GapKind::Languages);
    assert!(languages.basis.contains("en, pt-PT"), "{}", languages.basis);
    let few = list.iter().find(|g| g.id == "skills_few").unwrap();
    assert_eq!(few.names, ["Ruby", "TypeScript"]);
    assert!(few.basis.contains("only 2 skills"), "{}", few.basis);
    assert!(at("headline") < at("education"));
    // Context is still never asked, and nothing asks twice.
    assert!(!order.iter().any(|o| o.ends_with(":context")));
    let mut unique = order.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), order.len());
}

#[test]
fn languages_education_and_enough_skills_close_their_gaps() {
    use std::collections::BTreeMap;
    let skills: Vec<Value> = ["Ruby", "Rails", "TypeScript", "React", "Postgres", "Redis"]
        .iter()
        .map(|n| json!({ "name": n }))
        .collect();
    let profile: crate::schema::Profile = serde_json::from_value(json!({
        "basics": {},
        "work": [{ "name": "Example Org", "position": "Engineer", "startDate": "2023-07",
                   "summary": "Builds the billing platform, its API and its admin screens." }],
        "skills": skills,
        "languages": [{ "language": "Portuguese", "fluency": "Native" }],
        "education": [{ "institution": "University of Lisbon" }],
    }))
    .unwrap();
    let goals = crate::goals::Goals::default();
    let choices = BTreeMap::new();
    let list = gaps::gaps(&Inputs {
        profile: &profile,
        goals: &goals,
        portrait: &[],
        choices: &choices,
        changes: &[],
        accuracy: None,
        dismissed: &[],
    });
    let order = ids(&list);
    for gone in ["languages", "education", "skills_few"] {
        assert!(!order.contains(&gone), "{gone} in {order:?}");
    }
    // A described role is asked about its results first, as before; what
    // they did there keeps its place among the areas.
    assert_eq!(order[0], "results:0", "{order:?}");
    let at = |id: &str| order.iter().position(|o| *o == id).unwrap();
    assert!(at("summary") < at("area:0:your_part"), "{order:?}");
}

#[test]
fn a_language_or_education_answer_is_saved_in_its_own_shape() {
    let language = super::language_of("Portuguese (native)");
    assert_eq!(language.language, "Portuguese");
    assert_eq!(language.fluency.as_deref(), Some("native"));
    let plain = super::language_of("English");
    assert_eq!((plain.language.as_str(), plain.fluency), ("English", None));
    let degree = super::education_of("BSc in Computer Science, University of Lisbon");
    assert_eq!(degree.institution, "University of Lisbon");
    assert_eq!(
        degree.study_type.as_deref(),
        Some("BSc in Computer Science")
    );
    let alone = super::education_of("Self-taught");
    assert_eq!(
        (alone.institution.as_str(), alone.study_type),
        ("Self-taught", None)
    );
}

/// A research port that knows no employer's site, and reads the person's
/// own site as `site` (or fails with `error`).
struct OwnSiteFake {
    site: crate::ports::OwnSite,
    asked: std::sync::Mutex<Vec<String>>,
}

impl EmployerResearch for OwnSiteFake {
    fn known_site<'a>(
        &'a self,
        _: &'a PgPool,
        _: Uuid,
        _: &'a str,
    ) -> BoxFuture<'a, Result<Option<String>>> {
        Box::pin(async { Ok(None) })
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
        _: &'a str,
        _: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<Research>> {
        Box::pin(async { panic!("no employer has a known site") })
    }

    fn own_site<'a>(
        &'a self,
        url: &'a str,
        _: DateTime<Utc>,
    ) -> BoxFuture<'a, crate::ports::OwnSite> {
        Box::pin(async move {
            if let Ok(mut asked) = self.asked.lock() {
                asked.push(url.to_owned());
            }
            self.site.clone()
        })
    }
}

fn own_site_port(site: crate::ports::OwnSite) -> (Researcher, Arc<OwnSiteFake>) {
    let fake = Arc::new(OwnSiteFake {
        site,
        asked: std::sync::Mutex::new(Vec::new()),
    });
    (Researcher(Some(fake.clone())), fake)
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_persons_own_website_is_read_and_only_quoted_facts_reach_the_portrait(db: PgPool) {
    let page = "https://alex.example.com/projects";
    let site_reply = reply(&json!({ "facts": [
        { "claim": "Built an open-source billing toolkit", "url": page,
          "quote": "I built   Ledgerly, an open-source billing toolkit" },
        // Not on the page: never passed on.
        { "claim": "Spoke at a large conference", "url": page,
          "quote": "keynote speaker at a large conference" },
        // A page that was not read: never passed on.
        { "claim": "Wrote a book", "url": "https://elsewhere.example/book",
          "quote": "I wrote a book about billing" },
    ]}));
    let portrait = reply(&json!({ "sentences": [
        sentence("You run the billing platform in Rust on PostgreSQL.", "known", &["w0"], "", None),
        sentence("I assume, from your website, that you built an open-source billing toolkit.",
                 "assumed", &["site0"], "Your website says so; it may be old.", None),
    ]}));
    let server = scripted_model(vec![site_reply, portrait]).await;
    let (port, fake) = own_site_port(crate::ports::OwnSite {
        pages: vec![crate::ports::OwnPage {
            url: page.to_owned(),
            text: "Projects. I built Ledgerly, an open-source billing toolkit used by small shops."
                .to_owned(),
        }],
        error: None,
    });
    let h = Harness::with_research(db, Some(gateway(&server, None)), port);
    saved(&h, &profile()).await;

    let (status, view) = h.post("/api/v1/profile/about/portrait").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(
        *fake.asked.lock().unwrap(),
        ["https://alex.example.com"],
        "the profile's own link, nothing else"
    );
    let sent = server.received_requests().await.unwrap();
    assert_eq!(sent.len(), 2, "the site's facts, then the portrait");
    let site_call = String::from_utf8(sent[0].body.clone()).unwrap();
    assert!(site_call.contains("Ledgerly") && site_call.contains("<pages>"));
    let portrait_call = String::from_utf8(sent[1].body.clone()).unwrap();
    assert!(portrait_call.contains("open-source billing toolkit"));
    assert!(
        portrait_call.contains("site0"),
        "the fact is a citable source"
    );
    assert!(!portrait_call.contains("large conference") && !portrait_call.contains("a book"));
    // An assumption to confirm, linking the page; nothing is saved yet.
    let assumed = find(
        &view,
        "I assume, from your website, that you built an open-source billing toolkit.",
    );
    assert_eq!(assumed["kind"], "assumed");
    assert!(assumed.to_string().contains(page), "{assumed}");
    // The read counts toward the daily cap, like an employer's.
    assert_eq!(
        h.count("SELECT count(*) FROM profile.about_calls WHERE kind = 'research'")
            .await,
        1
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_website_that_cannot_be_read_leaves_the_portrait_without_it(db: PgPool) {
    let server = scripted_model(vec![portrait_reply()]).await;
    let (port, _) = own_site_port(crate::ports::OwnSite {
        pages: Vec::new(),
        error: Some("robots_disallows"),
    });
    let h = Harness::with_research(db, Some(gateway(&server, None)), port);
    saved(&h, &profile()).await;
    let (status, view) = h.post("/api/v1/profile/about/portrait").await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(calls(&server).await, 1, "only the portrait's call");
}

#[test]
fn site_facts_are_kept_only_with_their_quote_on_their_page() {
    let pages = vec![crate::ports::OwnPage {
        url: "https://a.example/".to_owned(),
        text: "Hi. I build Rails apps for clinics and teach Ruby.".to_owned(),
    }];
    let raw = |claim: &str, quote: &str, url: &str| prompt::RawSiteFact {
        claim: claim.to_owned(),
        quote: quote.to_owned(),
        url: url.to_owned(),
    };
    let kept = prompt::judge_site(
        vec![
            raw(
                "Builds Rails apps for clinics",
                "I BUILD rails apps for clinics",
                "https://a.example/",
            ),
            raw(
                "Builds Rails apps for clinics",
                "build Rails apps for clinics",
                "https://a.example/",
            ),
            raw("Teaches Go", "teach Go", "https://a.example/"),
            raw("Teaches Ruby", "teach Ruby", "https://b.example/"),
            raw("", "and teach Ruby", "https://a.example/"),
        ],
        &pages,
    );
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].claim, "Builds Rails apps for clinics");
    let sources = prompt::site_sources(&kept);
    assert_eq!(sources[0].id, "site0");
    assert_eq!(sources[0].url.as_deref(), Some("https://a.example/"));
}

#[test]
fn the_site_call_never_sees_how_to_reach_the_person() {
    let text = "Contact: inbox@example.com or (+351) 912 345 678. Rails since 2017 - 2021, \
                at Example Co from 2019-2023.";
    let shown = prompt::without_contact(text);
    assert!(
        !shown.contains("inbox@example.com") && !shown.contains("912 345 678"),
        "{shown}"
    );
    assert!(
        shown.contains("[email]") && shown.contains("[phone]"),
        "{shown}"
    );
    assert!(
        shown.contains("2017 - 2021") && shown.contains("2019-2023"),
        "dates stay: {shown}"
    );
    let pages = vec![crate::ports::OwnPage {
        url: "https://a.example/".to_owned(),
        text: text.to_owned(),
    }];
    let message = prompt::site_message(&pages);
    assert!(!message.contains("inbox@example.com") && !message.contains("912 345 678"));
    // A fact that names a way to reach them is never kept.
    let kept = prompt::judge_site(
        vec![
            prompt::RawSiteFact {
                claim: "Reachable at inbox@example.com".to_owned(),
                quote: "Contact: [email]".to_owned(),
                url: "https://a.example/".to_owned(),
            },
            prompt::RawSiteFact {
                claim: "Worked with Rails from 2017 to 2021".to_owned(),
                quote: "Rails since 2017 - 2021".to_owned(),
                url: "https://a.example/".to_owned(),
            },
        ],
        &pages,
    );
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].claim, "Worked with Rails from 2017 to 2021");
}
