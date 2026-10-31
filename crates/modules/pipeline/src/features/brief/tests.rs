//! The brief over HTTP: a fake neighbour reads the company's site and the
//! profile, a fake model answers the two prompts. All data is invented.

use std::sync::Arc;

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::DAILY_READS_CAP;
use crate::{
    ports::{
        AnswerFacts, ProfileDigest, ProposedFact, RoleDigest, SiteFailure, SitePage, SiteReading,
    },
    testkit::{Fake, Harness, Model},
};

const POSTING: &str =
    "Staff Engineer\nYou will own billing. 5+ years of Ruby required.\nKafka is a plus.";
const HOME: &str = "Example Co builds invented ledgers for small shops.";
const ABOUT: &str = "We are 40 people in Porto.";
const BLOG: &str = "In March 2026 we started moving from React to Hotwire.";

fn page(url: &str, kind: &str, text: &str) -> SitePage {
    SitePage {
        url: url.into(),
        fetched_at: Utc::now(),
        kind: kind.into(),
        text: text.into(),
    }
}

fn reading() -> SiteReading {
    SiteReading {
        pages: vec![
            page("http://example-co.test/jobs/staff", "posting", POSTING),
            page("http://example-co.test/", "home", HOME),
            page("http://example-co.test/about", "linked", ABOUT),
            page("http://example-co.test/blog/hotwire", "linked", BLOG),
        ],
        blocked: 1,
    }
}

fn facts_reply() -> Value {
    json!({ "facts": [
        { "text": "Builds ledgers for small shops",
          "quote": "builds invented ledgers for small shops", "url": "http://example-co.test/" },
        { "text": "Is moving from React to Hotwire",
          "quote": "moving from React to Hotwire", "url": "http://example-co.test/blog/hotwire" },
        // The quote is on no page.
        { "text": "Has 900 staff", "quote": "We are 900 people",
          "url": "http://example-co.test/about" },
        // A page that was never read.
        { "text": "Won an award", "quote": "award", "url": "https://news.example/story" },
    ]})
}

fn analysis_reply() -> Value {
    json!({
        "wants": [
            { "label": "Ruby", "quote": "5+ years of Ruby required", "level": "must" },
            { "label": "Kafka", "quote": "Kafka is a plus", "level": "nice" },
            { "label": "Go", "quote": "Go experience", "level": "must" },
        ],
        "questions": [
            { "question": "They are moving from React to Hotwire (their blog). Have you worked with Hotwire?",
              "about": 1, "skill": "Hotwire" },
            { "question": "The posting asks you to own billing. What did you own at your last job?",
              "about": null, "skill": null },
        ],
    })
}

fn digest() -> ProfileDigest {
    ProfileDigest {
        roles: vec![
            RoleDigest {
                index: 0,
                position: Some("Senior Engineer".into()),
                company: Some("Example Org".into()),
                start_date: Some("2021-03".into()),
                ..RoleDigest::default()
            },
            RoleDigest {
                index: 1,
                position: Some("Engineer".into()),
                company: Some("Sample Labs".into()),
                start_date: Some("2017-06".into()),
                end_date: Some("2021-02".into()),
                ..RoleDigest::default()
            },
        ],
        skills: vec!["Rust".into()],
        latest: Some(0),
        dismissed_skills: Vec::new(),
    }
}

struct Stack {
    h: Harness,
    fake: Arc<Fake>,
    model: Model,
}

async fn stack(db: PgPool) -> Stack {
    let fake = Arc::new(Fake::default());
    fake.reads(reading());
    *fake.digest.lock().unwrap() = Some(digest());
    let model = Model::new(facts_reply(), analysis_reply()).await;
    let h = Harness::with_ai(db, Uuid::now_v7(), Arc::clone(&fake), Some(model.gateway()));
    Stack { h, fake, model }
}

/// A posting the person pasted, on the company's own site.
async fn add(h: &Harness, url: &str) -> String {
    let (status, body) = h
        .json(
            "POST",
            "/api/v1/opportunities",
            &json!({ "url": url, "title": "Staff Engineer", "company": "Example Co" }),
        )
        .await;
    assert!(
        status == StatusCode::CREATED || status == StatusCode::OK,
        "{body}"
    );
    body["id"].as_str().unwrap().to_owned()
}

const FACTS: &str = "You read pages from a company";
const ANALYSIS: &str = "You compare a job posting";

#[sqlx::test(migrations = "../../../migrations")]
async fn reading_a_company_makes_facts_that_quote_their_pages_and_says_what_the_job_wants(
    db: PgPool,
) {
    let Stack { h, fake, model } = stack(db).await;
    let id = add(&h, "http://example-co.test/jobs/staff").await;
    let brief = format!("/api/v1/opportunities/{id}/brief");

    // Before anything: the site is the posting's own host, nothing is read.
    let (status, body) = h.get(&brief).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["site"], "http://example-co.test/");
    assert_eq!(body["site_source"], "apply_host");
    assert_eq!(body["site_needed"], false);
    assert!(body["read"].is_null() && body["wants"].is_null());
    assert_eq!(body["facts"].as_array().unwrap().len(), 0);
    assert_eq!(body["estimates"]["company_read_eur"], 0.002);
    assert_eq!(body["spend"]["reads_today"], 0);
    assert_eq!(body["spend"]["reads_cap"], DAILY_READS_CAP);
    assert_eq!(model.calls(FACTS).await, 0, "a look never calls the model");

    // Read: two facts stand, two are left out and counted.
    let (status, body) = h
        .json("POST", &format!("{brief}/read?locale=pt-PT"), &json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let facts = body["facts"].as_array().unwrap();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0]["url"], "http://example-co.test/");
    assert!(facts[0]["fetched_at"].is_string());
    assert_eq!(facts[1]["quote"], "moving from React to Hotwire");
    assert_eq!(body["read"]["facts_dropped"], 2);
    assert_eq!(body["read"]["pages_blocked"], 1);
    assert_eq!(body["read"]["pages"].as_array().unwrap().len(), 4);
    assert!(body["read"]["fresh_until"].is_string());
    assert_eq!(body["spend"]["reads_today"], 1);

    // What the job wants: the quote of "Go" is not in the posting.
    let wants = body["wants"].as_array().unwrap();
    assert_eq!(wants.len(), 2);
    assert_eq!(wants[0]["level"], "must");
    assert_eq!(wants[1]["label"], "Kafka");
    let questions = body["questions"].as_array().unwrap();
    assert_eq!(questions.len(), 2);
    assert_eq!(questions[0]["about"]["title"], "Engineer");
    assert_eq!(questions[0]["skill"], "Hotwire");
    assert!(questions[1]["about"].is_null());
    assert_eq!(body["latest_role"]["position"], 0);
    assert_eq!(questions[0]["state"], "open");

    // The site and the posting were what was read, nothing else.
    assert_eq!(
        *fake.sites_read.lock().unwrap(),
        [(
            "http://example-co.test/".to_owned(),
            Some("http://example-co.test/jobs/staff".to_owned())
        )]
    );
    // A pasted link has no text: the page just read is its description.
    let (_, opp) = h.get(&format!("/api/v1/opportunities/{id}")).await;
    assert!(opp["description"].as_str().unwrap().contains("own billing"));
    assert_eq!(
        (model.calls(FACTS).await, model.calls(ANALYSIS).await),
        (1, 1)
    );

    // Again within 30 days: nothing is fetched and no model is called.
    let (status, _) = h.json("POST", &format!("{brief}/read"), &json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fake.sites_read.lock().unwrap().len(), 1);
    assert_eq!(
        (model.calls(FACTS).await, model.calls(ANALYSIS).await),
        (1, 1)
    );

    // "Read again" fetches and asks for the company's facts again, but the
    // posting did not change, so the job is not compared again.
    let (status, body) = h
        .json("POST", &format!("{brief}/read?again=true"), &json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(fake.sites_read.lock().unwrap().len(), 2);
    assert_eq!(
        (model.calls(FACTS).await, model.calls(ANALYSIS).await),
        (2, 1)
    );
    assert_eq!(body["facts"].as_array().unwrap().len(), 2, "not doubled");
    assert_eq!(body["spend"]["reads_today"], 2);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_removed_fact_stays_removed_and_undo_brings_it_back(db: PgPool) {
    let Stack { h, fake: _, model } = stack(db).await;
    let id = add(&h, "http://example-co.test/jobs/staff").await;
    let brief = format!("/api/v1/opportunities/{id}/brief");
    let (_, body) = h.json("POST", &format!("{brief}/read"), &json!({})).await;
    let wrong = body["facts"][1]["id"].as_str().unwrap().to_owned();

    let (status, body) = h.send_delete(&format!("{brief}/facts/{wrong}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["facts"][1]["removed"], true);
    assert_eq!(body["facts"][0]["removed"], false);

    // A later read does not make it again, and does not hand it to the
    // job's comparison.
    let (_, body) = h
        .json("POST", &format!("{brief}/read?again=true"), &json!({}))
        .await;
    let facts = body["facts"].as_array().unwrap();
    assert_eq!(facts.len(), 2, "not made again");
    let removed = facts.iter().find(|f| f["id"] == wrong).unwrap();
    assert_eq!(removed["removed"], true);

    let (status, body) = h
        .json(
            "POST",
            &format!("{brief}/facts/{wrong}/restore"),
            &json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["facts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["removed"] == false)
    );

    // A fact that is not this company's is not found.
    let (status, _) = h
        .send_delete(&format!("{brief}/facts/{}", Uuid::now_v7()))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(model.calls(ANALYSIS).await, 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_person_types_the_site_once_when_nothing_gives_it(db: PgPool) {
    let Stack {
        h,
        fake,
        model: _model,
    } = stack(db).await;
    // A posting on a tracking system says nothing about the company's site.
    let id = add(&h, "https://boards.greenhouse.io/exampleco/jobs/42").await;
    let brief = format!("/api/v1/opportunities/{id}/brief");
    let (_, body) = h.get(&brief).await;
    assert_eq!(body["site_needed"], true);
    assert!(body["site"].is_null() && body["site_source"].is_null());

    let (status, body) = h.json("POST", &format!("{brief}/read"), &json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "site_needed");
    assert!(
        fake.sites_read.lock().unwrap().is_empty(),
        "nothing was fetched"
    );

    for (url, code) in [
        ("not a link", "invalid_site"),
        ("ftp://example.com/", "invalid_site"),
        ("https://user:pw@example.com/", "invalid_site"),
    ] {
        let (status, body) = h
            .json("PUT", &format!("{brief}/site"), &json!({ "url": url }))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
        assert_eq!(body["code"], code);
    }
    fake.refused.lock().unwrap().push("localhost".into());
    let (status, body) = h
        .json(
            "PUT",
            &format!("{brief}/site"),
            &json!({ "url": "https://localhost/" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "site_not_allowed");

    // Only the host is kept; it is the company's from then on, on any of its jobs.
    let (status, body) = h
        .json(
            "PUT",
            &format!("{brief}/site"),
            &json!({ "url": "https://www.example.com/about?x=1" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["site"], "https://www.example.com/");
    assert_eq!(body["site_source"], "person");
    assert_eq!(body["site_needed"], false);
    let other = add(&h, "https://jobs.lever.co/exampleco/abc").await;
    let (_, body) = h.get(&format!("/api/v1/opportunities/{other}/brief")).await;
    assert_eq!(body["site"], "https://www.example.com/");

    let (status, body) = h.json("POST", &format!("{brief}/read"), &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        fake.sites_read.lock().unwrap()[0].0,
        "https://www.example.com/"
    );

    // A different site forgets what was read on the old one.
    let (_, body) = h
        .json(
            "PUT",
            &format!("{brief}/site"),
            &json!({ "url": "https://example.org" }),
        )
        .await;
    assert!(body["read"].is_null());
    assert_eq!(body["facts"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_site_that_cannot_be_read_says_why_and_still_counts(db: PgPool) {
    let Stack { h, fake, model } = stack(db).await;
    let id = add(&h, "http://example-co.test/jobs/staff").await;
    let read = format!("/api/v1/opportunities/{id}/brief/read");
    for (failure, status, code) in [
        (
            SiteFailure::Unreachable,
            StatusCode::SERVICE_UNAVAILABLE,
            "site_unreachable",
        ),
        (
            SiteFailure::RobotsDisallows,
            StatusCode::CONFLICT,
            "robots_disallows",
        ),
        (
            SiteFailure::NothingReadable,
            StatusCode::CONFLICT,
            "no_pages_readable",
        ),
    ] {
        *fake.site.lock().unwrap() = Some(Err(failure));
        let (got, body) = h.json("POST", &read, &json!({})).await;
        assert_eq!(got, status, "{body}");
        assert_eq!(body["code"], code);
    }
    assert_eq!(model.calls(FACTS).await, 0, "no model call without pages");
    let (_, body) = h.get(&format!("/api/v1/opportunities/{id}/brief")).await;
    assert_eq!(
        body["spend"]["reads_today"], 3,
        "fetches count toward the cap"
    );
    assert!(body["read"].is_null());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn twenty_company_reads_a_day_is_the_limit(db: PgPool) {
    let Stack { h, fake, model } = stack(db).await;
    let id = add(&h, "http://example-co.test/jobs/staff").await;
    for _ in 0..DAILY_READS_CAP {
        sqlx::query(
            "INSERT INTO pipeline.company_reads (id, account_id, company_id, started_at)
             VALUES ($1, $2, $3, now())",
        )
        .bind(Uuid::now_v7())
        .bind(h.account)
        .bind(Uuid::now_v7())
        .execute(&h.db)
        .await
        .unwrap();
    }
    let (status, body) = h
        .json(
            "POST",
            &format!("/api/v1/opportunities/{id}/brief/read"),
            &json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "company_reads_daily_cap");
    assert!(fake.sites_read.lock().unwrap().is_empty());
    assert_eq!(model.calls(FACTS).await, 0);
    // Yesterday's reads do not count.
    sqlx::query("UPDATE pipeline.company_reads SET started_at = now() - interval '2 days'")
        .execute(&h.db)
        .await
        .unwrap();
    let (status, _) = h
        .json(
            "POST",
            &format!("/api/v1/opportunities/{id}/brief/read"),
            &json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn without_a_model_nothing_is_fetched(db: PgPool) {
    let fake = Arc::new(Fake::default());
    fake.reads(reading());
    let h = Harness::with_ai(db, Uuid::now_v7(), Arc::clone(&fake), None);
    let id = add(&h, "http://example-co.test/jobs/staff").await;
    let (status, body) = h
        .json(
            "POST",
            &format!("/api/v1/opportunities/{id}/brief/read"),
            &json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "ai_unavailable");
    assert!(fake.sites_read.lock().unwrap().is_empty());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn someone_elses_opportunity_is_not_found(db: PgPool) {
    let Stack { h, fake, model } = stack(db.clone()).await;
    let id = add(&h, "http://example-co.test/jobs/staff").await;
    let other = Harness::with_ai(db, Uuid::now_v7(), Arc::clone(&fake), Some(model.gateway()));
    let brief = format!("/api/v1/opportunities/{id}/brief");
    let (status, _) = other.get(&brief).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for (method, path, body) in [
        (
            "PUT",
            format!("{brief}/site"),
            json!({ "url": "https://example.com" }),
        ),
        ("POST", format!("{brief}/read"), json!({})),
        (
            "DELETE",
            format!("{brief}/facts/{}", Uuid::now_v7()),
            json!({}),
        ),
        (
            "POST",
            format!("{brief}/facts/{}/restore", Uuid::now_v7()),
            json!({}),
        ),
        (
            "POST",
            format!("{brief}/questions/{}/answer", Uuid::now_v7()),
            json!({ "text": "x" }),
        ),
        (
            "POST",
            format!("{brief}/questions/{}/dont-have", Uuid::now_v7()),
            json!({}),
        ),
        (
            "POST",
            format!("{brief}/questions/{}/skip", Uuid::now_v7()),
            json!({}),
        ),
    ] {
        let (status, _) = other.json(method, &path, &body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
    }
    assert!(
        fake.sites_read.lock().unwrap().is_empty(),
        "nothing was fetched for them"
    );
    assert_eq!(model.calls(FACTS).await, 0);
}

fn answered(position: usize) -> AnswerFacts {
    AnswerFacts {
        position,
        title: Some("Engineer".into()),
        company: Some("Sample Labs".into()),
        facts: vec![ProposedFact {
            id: Uuid::now_v7(),
            text: "Ran the Hotwire migration".into(),
            quote: "I ran the Hotwire migration".into(),
            area: "your_part".into(),
            expires_at: Utc::now() + chrono::TimeDelta::hours(24),
        }],
        dropped: 1,
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_answer_goes_to_the_position_the_question_names_or_the_most_recent(db: PgPool) {
    let Stack {
        h,
        fake,
        model: _model,
    } = stack(db).await;
    let id = add(&h, "http://example-co.test/jobs/staff").await;
    let brief = format!("/api/v1/opportunities/{id}/brief");
    let (_, body) = h.json("POST", &format!("{brief}/read"), &json!({})).await;
    let named = body["questions"][0]["id"].as_str().unwrap().to_owned();
    let unnamed = body["questions"][1]["id"].as_str().unwrap().to_owned();

    *fake.answered.lock().unwrap() = Some(answered(1));
    let (status, body) = h
        .json(
            "POST",
            &format!("{brief}/questions/{named}/answer"),
            &json!({ "text": "I ran the Hotwire migration. 3 of us." }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["defaulted"], false);
    assert_eq!(body["role"]["position"], 1);
    assert_eq!(body["proposals"].as_array().unwrap().len(), 1);
    assert_eq!(body["proposals"][0]["area"], "your_part");
    assert_eq!(body["dropped"], 1);
    assert_eq!(body["question"]["state"], "answered");
    assert_eq!(fake.answers.lock().unwrap()[0].0, 1);

    // No position named: the most recent one, and it says so.
    *fake.answered.lock().unwrap() = Some(answered(0));
    let (status, body) = h
        .json(
            "POST",
            &format!("{brief}/questions/{unnamed}/answer"),
            &json!({ "text": "I owned invoicing." }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["defaulted"], true);
    assert_eq!(fake.answers.lock().unwrap()[1].0, 0);

    // A question is answered once.
    let (status, body) = h
        .json(
            "POST",
            &format!("{brief}/questions/{named}/answer"),
            &json!({ "text": "Again." }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "question_closed");

    let (_, body) = h.get(&brief).await;
    assert_eq!(body["questions"][0]["state"], "answered");
    for text in ["", "   "] {
        let (status, body) = h
            .json(
                "POST",
                &format!("{brief}/questions/{unnamed}/answer"),
                &json!({ "text": text }),
            )
            .await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::CONFLICT,
            "{body}"
        );
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn i_dont_have_it_dismisses_the_skill_without_a_call_and_skip_hides_the_question(db: PgPool) {
    let Stack { h, fake, model } = stack(db).await;
    let id = add(&h, "http://example-co.test/jobs/staff").await;
    let brief = format!("/api/v1/opportunities/{id}/brief");
    let (_, body) = h.json("POST", &format!("{brief}/read"), &json!({})).await;
    let skill = body["questions"][0]["id"].as_str().unwrap().to_owned();
    let plain = body["questions"][1]["id"].as_str().unwrap().to_owned();
    let calls = (model.calls(FACTS).await, model.calls(ANALYSIS).await);

    // A question that is not about a skill has nothing to put away.
    let (status, body) = h
        .json(
            "POST",
            &format!("{brief}/questions/{plain}/dont-have"),
            &json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "no_skill");

    let (status, body) = h
        .json(
            "POST",
            &format!("{brief}/questions/{skill}/dont-have"),
            &json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["questions"][0]["state"], "dont_have");
    assert_eq!(*fake.dismissed.lock().unwrap(), ["hotwire"]);
    assert_eq!(
        (model.calls(FACTS).await, model.calls(ANALYSIS).await),
        calls,
        "no call"
    );

    let (status, body) = h
        .json(
            "POST",
            &format!("{brief}/questions/{plain}/skip"),
            &json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["questions"][1]["state"], "skipped");
    let (status, _) = h
        .json(
            "POST",
            &format!("{brief}/questions/{plain}/skip"),
            &json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // The skill, dismissed on another job, is not asked about here either.
    let fresh = add(&h, "http://example-co.test/jobs/other").await;
    let fresh_brief = format!("/api/v1/opportunities/{fresh}/brief");
    h.json("POST", &format!("{fresh_brief}/read"), &json!({}))
        .await;
    fake.digest
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .dismissed_skills = vec!["hotwire".into()];
    let (_, body) = h.get(&fresh_brief).await;
    let asked: Vec<&str> = body["questions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|q| q["skill"].as_str())
        .collect();
    assert!(asked.is_empty(), "{asked:?}");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_changed_posting_is_compared_again_and_a_skipped_question_stays_skipped(db: PgPool) {
    let Stack { h, fake: _, model } = stack(db).await;
    let id = add(&h, "http://example-co.test/jobs/staff").await;
    let brief = format!("/api/v1/opportunities/{id}/brief");
    let (_, body) = h.json("POST", &format!("{brief}/read"), &json!({})).await;
    let plain = body["questions"][1]["id"].as_str().unwrap().to_owned();
    h.json(
        "POST",
        &format!("{brief}/questions/{plain}/skip"),
        &json!({}),
    )
    .await;
    assert_eq!(model.calls(ANALYSIS).await, 1);

    sqlx::query("UPDATE pipeline.opportunities SET description = $2 WHERE id = $1")
        .bind(Uuid::parse_str(&id).unwrap())
        .bind(format!("{POSTING}\nKafka is a plus. Updated."))
        .execute(&h.db)
        .await
        .unwrap();
    let (status, body) = h.json("POST", &format!("{brief}/read"), &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(model.calls(ANALYSIS).await, 2);
    assert_eq!(body["questions"][1]["state"], "skipped");
    assert_eq!(
        model.calls(FACTS).await,
        1,
        "the company was not read again"
    );
}
