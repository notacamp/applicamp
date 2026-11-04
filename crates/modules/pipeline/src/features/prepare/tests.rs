//! Preparing a job by itself: a fake neighbour reads the company's site and
//! holds the profile, a fake model answers the prompts, and the job runs in
//! the test. All data is invented.

use std::sync::Arc;

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::run;
use crate::{
    PipelineDeps,
    features::cv::tests::{CV, add, answer, profile},
    ports::{Autonomy, Neighbours, ProfileDigest, RoleDigest, SitePage, SiteReading},
    testkit::{Fake, Harness, Model},
};

const HOME: &str = "Example Co builds invented ledgers for small shops.";

fn reading() -> SiteReading {
    SiteReading {
        pages: vec![SitePage {
            url: "http://example-co.test/".into(),
            fetched_at: Utc::now(),
            kind: "home".into(),
            text: HOME.into(),
        }],
        blocked: 0,
    }
}

fn facts() -> Value {
    json!({ "facts": [
        { "text": "Builds ledgers for small shops",
          "quote": "builds invented ledgers for small shops", "url": "http://example-co.test/" },
    ]})
}

fn analysis(questions: Value) -> Value {
    json!({
        "wants": [{ "label": "Rust", "quote": "Rust and PostgreSQL required", "level": "must" }],
        "questions": questions,
    })
}

struct Stack {
    h: Harness,
    fake: Arc<Fake>,
    model: Model,
    deps: PipelineDeps,
}

async fn stack(db: PgPool, questions: Value) -> Stack {
    let fake = Arc::new(Fake::default());
    fake.reads(reading());
    *fake.cv_profile.lock().unwrap() = Some(profile());
    *fake.digest.lock().unwrap() = Some(ProfileDigest {
        roles: vec![RoleDigest {
            index: 0,
            position: Some("Senior Engineer".into()),
            company: Some("Example Org".into()),
            start_date: Some("2021-03".into()),
            ..RoleDigest::default()
        }],
        skills: vec!["Rust".into()],
        latest: Some(0),
        dismissed_skills: Vec::new(),
    });
    let model = Model::with_cv(facts(), analysis(questions), answer()).await;
    let h = Harness::with_ai(db, Uuid::now_v7(), Arc::clone(&fake), Some(model.gateway()));
    let neighbours: Arc<dyn Neighbours> = fake.clone();
    let deps = PipelineDeps {
        neighbours: Some(neighbours),
        ai: Some(model.gateway()),
    };
    Stack {
        h,
        fake,
        model,
        deps,
    }
}

impl Stack {
    async fn run(&self, opp: &str) {
        run(&self.deps, &self.h.db, Uuid::parse_str(opp).unwrap())
            .await
            .unwrap();
    }

    async fn brief(&self, opp: &str) -> Value {
        let (status, body) = self
            .h
            .get(&format!("/api/v1/opportunities/{opp}/brief"))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    async fn post(&self, opp: &str, tail: &str) -> (StatusCode, Value) {
        self.h
            .json(
                "POST",
                &format!("/api/v1/opportunities/{opp}{tail}"),
                &json!({}),
            )
            .await
    }
}

fn steps(brief: &Value) -> Vec<String> {
    brief["preparation"]["done"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["step"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_shortlisted_job_is_read_compared_and_drafted_then_waits_for_the_approval(db: PgPool) {
    let s = stack(db, json!([])).await;
    let opp = add(&s.h, "shortlisted").await;

    // Shortlisting queued it; nothing ran yet.
    let brief = s.brief(&opp).await;
    assert_eq!(brief["prepares_by_itself"], true);
    assert_eq!(brief["preparation"]["state"], "queued");
    assert_eq!(brief["preparation"]["step"], "read");
    assert_eq!(s.model.calls(CV).await, 0);

    s.run(&opp).await;
    let brief = s.brief(&opp).await;
    assert_eq!(brief["preparation"]["state"], "waiting", "{brief}");
    assert_eq!(brief["preparation"]["waiting_on"], "cv_approval");
    assert_eq!(steps(&brief), ["read", "compare", "draft"]);
    let spent = brief["preparation"]["spent_eur"].as_f64().unwrap();
    // The read, the comparison, and the draft with its fix round at worst.
    let expected = crate::features::brief::COMPANY_READ_EUR
        + crate::features::brief::ANALYSIS_EUR
        + crate::features::cv::DRAFT_EUR;
    assert!((spent - expected).abs() < 1e-9, "{spent}");
    assert_eq!(brief["facts"].as_array().unwrap().len(), 1);
    assert_eq!(s.model.calls(CV).await, 1);
    let (_, cv) = s.h.get(&format!("/api/v1/opportunities/{opp}/cv")).await;
    assert_eq!(
        cv["draft"]["status"], "draft",
        "the approval stays the person's"
    );

    // Running it again does nothing: nothing is queued.
    s.run(&opp).await;
    assert_eq!(s.model.calls(CV).await, 1);

    // Approving the CV (here, by hand) queues the letter.
    sqlx::query("UPDATE pipeline.cv_drafts SET status = 'approved' WHERE opportunity_id = $1")
        .bind(Uuid::parse_str(&opp).unwrap())
        .execute(&s.h.db)
        .await
        .unwrap();
    let brief = s.brief(&opp).await;
    assert_eq!(brief["preparation"]["state"], "queued");
    assert_eq!(brief["preparation"]["step"], "letter");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_open_question_holds_the_cv_until_it_is_answered_or_skipped(db: PgPool) {
    let question = json!([{ "question": "The posting asks you to own billing. What did you own?",
                             "about": null, "skill": null }]);
    let s = stack(db, question).await;
    let opp = add(&s.h, "shortlisted").await;
    s.run(&opp).await;
    let brief = s.brief(&opp).await;
    assert_eq!(brief["preparation"]["waiting_on"], "answers", "{brief}");
    assert_eq!(s.model.calls(CV).await, 0);

    let question = brief["questions"][0]["id"].as_str().unwrap().to_owned();
    let (status, body) = s
        .post(&opp, &format!("/brief/questions/{question}/skip"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["preparation"]["state"], "queued");
    assert_eq!(body["preparation"]["step"], "draft");
    s.run(&opp).await;
    assert_eq!(s.model.calls(CV).await, 1);
    assert_eq!(
        s.brief(&opp).await["preparation"]["waiting_on"],
        "cv_approval"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn facts_from_an_answer_hold_the_cv_until_they_are_kept_or_dropped(db: PgPool) {
    let question = json!([{ "question": "What did you own?", "about": null, "skill": null }]);
    let s = stack(db, question).await;
    let opp = add(&s.h, "shortlisted").await;
    s.run(&opp).await;
    let brief = s.brief(&opp).await;
    let question = brief["questions"][0]["id"].as_str().unwrap();
    let pending = Uuid::now_v7();
    sqlx::query(
        "UPDATE pipeline.brief_questions SET state = 'answered', proposals = $2 WHERE id = $1",
    )
    .bind(Uuid::parse_str(question).unwrap())
    .bind(vec![pending])
    .execute(&s.h.db)
    .await
    .unwrap();
    s.fake.waiting.lock().unwrap().push(pending);
    let brief = s.brief(&opp).await;
    assert_eq!(brief["preparation"]["waiting_on"], "answers");

    // Kept: the next look queues the draft.
    s.fake.waiting.lock().unwrap().clear();
    let brief = s.brief(&opp).await;
    assert_eq!(brief["preparation"]["state"], "queued");
    assert_eq!(brief["preparation"]["step"], "draft");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn find_only_prepares_nothing_by_itself(db: PgPool) {
    let s = stack(db, json!([])).await;
    *s.fake.autonomy.lock().unwrap() = Some(Autonomy::FindOnly);
    let opp = add(&s.h, "shortlisted").await;
    let brief = s.brief(&opp).await;
    assert_eq!(brief["prepares_by_itself"], false);
    assert!(brief["preparation"].is_null(), "{brief}");
    let (status, body) = s.post(&opp, "/prepare/start").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "find_only");
    s.run(&opp).await;
    assert!(
        s.fake.sites_read.lock().unwrap().is_empty(),
        "nothing is fetched"
    );
    assert_eq!(s.model.calls(CV).await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn stop_and_moving_the_job_back_cancel_what_is_still_to_run(db: PgPool) {
    let s = stack(db, json!([])).await;
    let opp = add(&s.h, "shortlisted").await;

    let (status, body) = s.post(&opp, "/prepare/stop").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["preparation"]["state"], "stopped");
    assert_eq!(body["preparation"]["reason"], "stopped");
    s.run(&opp).await;
    assert!(s.fake.sites_read.lock().unwrap().is_empty());
    // A look does not start it again; the person does.
    assert_eq!(s.brief(&opp).await["preparation"]["state"], "stopped");

    let (status, body) = s.post(&opp, "/prepare/start").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["preparation"]["state"], "queued");
    let (status, _) =
        s.h.json(
            "PATCH",
            &format!("/api/v1/opportunities/{opp}"),
            &json!({ "stage": "found" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let brief = s.brief(&opp).await;
    assert_eq!(brief["preparation"]["state"], "stopped");
    assert_eq!(brief["preparation"]["reason"], "not_shortlisted");
    s.run(&opp).await;
    assert_eq!(s.model.calls(CV).await, 0);

    // Another person's job is not found.
    let other = Harness::as_account(s.h.db.clone(), Uuid::now_v7(), Arc::clone(&s.fake));
    for tail in ["/prepare/stop", "/prepare/start"] {
        let (status, _) = other
            .json(
                "POST",
                &format!("/api/v1/opportunities/{opp}{tail}"),
                &json!({}),
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{tail}");
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_cap_stops_it_with_its_reason_and_nothing_retries(db: PgPool) {
    let s = stack(db, json!([])).await;
    let other = add(&s.h, "found").await;
    for n in 1..=20 {
        sqlx::query(
            "INSERT INTO pipeline.cv_drafts
                (id, account_id, opportunity_id, n, language, status, content, frame,
                 created_at, updated_at)
             VALUES ($1, $2, $3, $4, 'en', 'discarded', '{}', '{}', now(), now())",
        )
        .bind(Uuid::now_v7())
        .bind(s.h.account)
        .bind(Uuid::parse_str(&other).unwrap())
        .bind(n)
        .execute(&s.h.db)
        .await
        .unwrap();
    }
    let opp = add(&s.h, "shortlisted").await;
    s.run(&opp).await;
    let brief = s.brief(&opp).await;
    assert_eq!(brief["preparation"]["state"], "stopped", "{brief}");
    assert_eq!(brief["preparation"]["step"], "draft");
    assert_eq!(brief["preparation"]["reason"], "cv_drafts_daily_cap");
    assert_eq!(steps(&brief), ["read", "compare"]);
    assert_eq!(s.model.calls(CV).await, 0);
    // A look does not retry it.
    s.run(&opp).await;
    assert_eq!(s.brief(&opp).await["preparation"]["state"], "stopped");
}

/// An opportunity added by a tracking system's link: its host is never the
/// company's site.
async fn tracked(h: &Harness, company: &str, posting: &str) -> String {
    let (status, body) = h
        .json(
            "POST",
            "/api/v1/opportunities",
            &json!({ "url": format!("https://jobs.lever.co/example/{}", Uuid::now_v7()),
                     "title": "Senior Developer", "company": company }),
        )
        .await;
    assert!(
        status == StatusCode::CREATED || status == StatusCode::OK,
        "{body}"
    );
    let id = body["id"].as_str().unwrap().to_owned();
    sqlx::query("UPDATE pipeline.opportunities SET description = $2 WHERE id = $1")
        .bind(Uuid::parse_str(&id).unwrap())
        .bind(posting)
        .execute(&h.db)
        .await
        .unwrap();
    id
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_site_is_found_in_the_postings_email_address_without_asking(db: PgPool) {
    let s = stack(db, json!([])).await;
    let opp = tracked(
        &s.h,
        "Example Apps Software",
        "Rust and PostgreSQL required. Questions? jobs@exampleapps.test",
    )
    .await;
    let brief = s.brief(&opp).await;
    assert_eq!(brief["site"], "https://exampleapps.test/");
    assert_eq!(brief["site_source"], "posting_email");
    assert_eq!(brief["site_needed"], false);
    assert!(
        s.fake.homes_asked.lock().unwrap().is_empty(),
        "no page asked"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_site_is_found_from_the_name_when_its_home_page_names_the_company(db: PgPool) {
    let s = stack(db, json!([])).await;
    s.fake.homes.lock().unwrap().push((
        "https://samplewidgets.com/".into(),
        Ok("Sample Widgets makes invented widgets.".into()),
    ));
    let opp = tracked(&s.h, "Sample Widgets", "Rust and PostgreSQL required.").await;
    let (status, _) =
        s.h.json(
            "PATCH",
            &format!("/api/v1/opportunities/{opp}"),
            &json!({ "stage": "shortlisted" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    s.run(&opp).await;
    let brief = s.brief(&opp).await;
    assert_eq!(brief["site"], "https://samplewidgets.com/", "{brief}");
    assert_eq!(brief["site_source"], "name_guess");
    assert_eq!(steps(&brief)[0], "site");
    assert_eq!(
        s.fake.homes_asked.lock().unwrap().as_slice(),
        ["https://samplewidgets.com/"]
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_guess_that_does_not_name_the_company_is_not_used_and_the_person_is_asked(db: PgPool) {
    let s = stack(db, json!([])).await;
    s.fake.homes.lock().unwrap().push((
        "https://otherthing.com/".into(),
        Ok("A parked domain.".into()),
    ));
    let opp = tracked(&s.h, "Other Thing", "Rust and PostgreSQL required.").await;
    let (status, _) =
        s.h.json(
            "PATCH",
            &format!("/api/v1/opportunities/{opp}"),
            &json!({ "stage": "shortlisted" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    s.run(&opp).await;
    let brief = s.brief(&opp).await;
    assert_eq!(brief["preparation"]["waiting_on"], "site", "{brief}");
    assert_eq!(brief["site_needed"], true);
    assert_eq!(s.fake.homes_asked.lock().unwrap().len(), 2, "two guesses");

    // Not looked for again; typing it starts the read.
    let (status, body) =
        s.h.json(
            "PUT",
            &format!("/api/v1/opportunities/{opp}/brief/site"),
            &json!({ "url": "http://example-co.test/" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["preparation"]["state"], "queued");
    s.run(&opp).await;
    assert_eq!(s.fake.homes_asked.lock().unwrap().len(), 2);
    assert_eq!(
        s.brief(&opp).await["preparation"]["waiting_on"],
        "cv_approval"
    );
}
