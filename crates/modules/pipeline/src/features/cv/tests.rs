//! The tailored CV over HTTP: fake neighbours hold the profile, a fake model
//! answers the CV prompt. All data is invented.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::{CAP_PER_DAY, CAP_PER_OPPORTUNITY};
use crate::{
    ports::{CvContact, CvEducation, CvFact, CvLanguage, CvPosition, CvProfile},
    testkit::{Fake, Harness, Model},
};

const FACT_ID: &str = "aaaa1111-0000-4000-8000-000000000001";
const COMPANY_FACT: &str = "Builds ledgers for small shops";
const CV: &str = "You write a one-page CV";

fn profile() -> CvProfile {
    CvProfile {
        contact: CvContact {
            name: Some("Alex Demo".into()),
            email: Some("alex@example.com".into()),
            location: Some("Porto, Portugal".into()),
            url: Some("https://alex.example.com".into()),
            links: vec![(Some("GitHub".into()), "https://github.com/alex-demo".into())],
        },
        headline: Some("Backend engineer who ships billing systems".into()),
        summary: Some("Eight years of backend work on billing.".into()),
        positions: vec![
            CvPosition {
                index: 0,
                title: Some("Senior Engineer".into()),
                company: Some("Example Org".into()),
                start_date: Some("2021-03".into()),
                summary: Some("Runs the billing platform.".into()),
                highlights: vec![
                    "Rebuilt the billing pipeline in Rust, cutting invoice runs from 6 hours to 40 minutes"
                        .into(),
                    "Led a team of 4 engineers".into(),
                ],
                facts: vec![CvFact {
                    id: FACT_ID.into(),
                    text: "Moved invoicing onto PostgreSQL".into(),
                }],
                ..CvPosition::default()
            },
            CvPosition {
                index: 1,
                title: Some("Engineer".into()),
                company: Some("Sample Labs".into()),
                start_date: Some("2017-06".into()),
                end_date: Some("2021-02".into()),
                highlights: vec!["Built ingestion for 3 partner feeds".into()],
                ..CvPosition::default()
            },
            CvPosition {
                index: 2,
                title: Some("Junior Developer".into()),
                company: Some("Tiny Co".into()),
                start_date: Some("2015".into()),
                end_date: Some("2017".into()),
                highlights: vec!["Wrote internal tools".into()],
                ..CvPosition::default()
            },
        ],
        skills: vec!["Rust".into(), "PostgreSQL".into()],
        education: vec![CvEducation {
            institution: "Example University".into(),
            study_type: Some("BSc".into()),
            area: Some("Computer science".into()),
            ..CvEducation::default()
        }],
        languages: vec![CvLanguage {
            language: "English".into(),
            fluency: Some("Fluent".into()),
        }],
        latest: Some(0),
    }
}

/// What the fake model answers: a reworded bullet citing a kept fact, a
/// highlight unchanged, a number nobody wrote, a company fact as an
/// achievement, a skill off the profile, and a role left out (the third).
fn answer() -> Value {
    json!({
        "headline": { "text": "Backend engineer who ships billing systems", "sources": ["hl"] },
        "summary": {
            "text": "Rust engineer on billing, close to what Example Co builds for small shops.",
            "sources": ["w0.h1", "cab12cd34"],
        },
        "experience": [
            { "position": 0, "bullets": [
                { "text": "Moved invoicing over to PostgreSQL", "sources": ["w0.faaaa1111"] },
                { "text": "Led a team of 4 engineers", "sources": ["w0.h2"] },
                { "text": "Cut invoice runs from 6 hours to 20 minutes", "sources": ["w0.h1"] },
                { "text": COMPANY_FACT, "sources": ["cab12cd34"] },
            ]},
            { "position": 1, "bullets": [
                { "text": "Built ingestion for 3 partner feeds", "sources": ["w1.h1"] },
            ]},
        ],
        "skills": ["Rust", "PostgreSQL", "Kubernetes"],
    })
}

struct Stack {
    h: Harness,
    fake: Arc<Fake>,
    model: Model,
    opp: String,
}

async fn add(h: &Harness, stage: &str) -> String {
    let (status, body) = h
        .json(
            "POST",
            "/api/v1/opportunities",
            &json!({ "url": format!("http://example-co.test/jobs/{}", Uuid::now_v7()),
                     "title": "Staff Engineer", "company": "Example Co" }),
        )
        .await;
    assert!(
        status == StatusCode::CREATED || status == StatusCode::OK,
        "{body}"
    );
    let id = body["id"].as_str().unwrap().to_owned();
    sqlx::query("UPDATE pipeline.opportunities SET description = $2 WHERE id = $1")
        .bind(Uuid::parse_str(&id).unwrap())
        .bind("You will own billing. Rust and PostgreSQL required.")
        .execute(&h.db)
        .await
        .unwrap();
    if stage != "found" {
        let (status, body) = h
            .json(
                "PATCH",
                &format!("/api/v1/opportunities/{id}"),
                &json!({ "stage": stage }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    id
}

/// A company fact on the opportunity's company, so the model can cite it
/// (its source id is `cab12cd34`).
async fn company_fact(h: &Harness) {
    let company = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO pipeline.companies (id, account_id, key, name, created_at, updated_at)
         VALUES ($1, $2, 'example co', 'Example Co', now(), now())",
    )
    .bind(company)
    .bind(h.account)
    .execute(&h.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO pipeline.company_facts
            (id, company_id, account_id, text, quote, url, text_key, created_at)
         VALUES ($1, $2, $3, $4, $4, 'https://example-co.test/', 'k', now())",
    )
    .bind(Uuid::parse_str("ab12cd34-0000-4000-8000-000000000000").unwrap())
    .bind(company)
    .bind(h.account)
    .bind(COMPANY_FACT)
    .execute(&h.db)
    .await
    .unwrap();
}

async fn stack(db: PgPool) -> Stack {
    let fake = Arc::new(Fake::default());
    *fake.cv_profile.lock().unwrap() = Some(profile());
    let model = Model::with_cv(
        json!({"facts": []}),
        json!({"wants": [], "questions": []}),
        answer(),
    )
    .await;
    let h = Harness::with_ai(db, Uuid::now_v7(), Arc::clone(&fake), Some(model.gateway()));
    company_fact(&h).await;
    let opp = add(&h, "shortlisted").await;
    Stack {
        h,
        fake,
        model,
        opp,
    }
}

fn url(opp: &str, tail: &str) -> String {
    format!("/api/v1/opportunities/{opp}/cv{tail}")
}

/// The line `id` of the draft in `cv`, wherever it is.
fn line<'a>(cv: &'a Value, id: &str) -> &'a Value {
    let draft = &cv["draft"];
    [&draft["headline"], &draft["summary"]]
        .into_iter()
        .chain(
            draft["experience"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|r| r["bullets"].as_array().unwrap().iter()),
        )
        .chain(draft["skills"].as_array().unwrap().iter())
        .find(|l| l["id"] == id)
        .unwrap_or_else(|| panic!("no line {id} in {cv}"))
}

/// The ids of the flagged lines (and skills), with their first reason's kind.
fn flagged(cv: &Value) -> Vec<(String, String)> {
    let draft = &cv["draft"];
    let mut found = Vec::new();
    let mut each = |l: &Value| {
        if let Some(first) = l["flags"].as_array().and_then(|f| f.first()) {
            found.push((
                l["id"].as_str().unwrap().to_owned(),
                first["kind"].as_str().unwrap().to_owned(),
            ));
        }
    };
    each(&draft["headline"]);
    each(&draft["summary"]);
    for role in draft["experience"].as_array().unwrap() {
        role["bullets"]
            .as_array()
            .unwrap()
            .iter()
            .for_each(&mut each);
    }
    draft["skills"]
        .as_array()
        .unwrap()
        .iter()
        .for_each(&mut each);
    found
}

async fn draft(s: &Stack) -> Value {
    let (status, cv) = s.h.json("POST", &url(&s.opp, "/drafts"), &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    cv
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_draft_cites_its_sources_and_the_check_flags_what_they_do_not_say(db: PgPool) {
    let s = stack(db).await;

    // Before a draft: nothing, and a draft is allowed.
    let (status, cv) = s.h.get(&url(&s.opp, "")).await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    assert!(cv["draft"].is_null());
    assert_eq!(cv["can_draft"]["allowed"], true);
    assert_eq!(cv["caps"]["drafts_today_cap"], CAP_PER_DAY);
    assert_eq!(cv["caps"]["drafts_here_cap"], CAP_PER_OPPORTUNITY);
    assert_eq!(s.model.calls(CV).await, 0, "a look never calls the model");

    let cv = draft(&s).await;
    assert_eq!(s.model.calls(CV).await, 1);
    let d = &cv["draft"];
    assert_eq!(d["n"], 1);
    assert_eq!(d["status"], "draft");
    assert_eq!(d["language"], "en");
    // The server filled in the contact; the model never had it.
    assert_eq!(d["contact"]["name"], "Alex Demo");
    assert_eq!(d["contact"]["email"], "alex@example.com");
    assert_eq!(d["education"][0]["institution"], "Example University");
    assert_eq!(d["languages"][0]["language"], "English");

    // Three things are flagged: the number, the company fact as an
    // achievement, the skill.
    let flags = flagged(&cv);
    assert_eq!(flags.len(), 3, "{flags:?}");
    assert!(flags.iter().any(|(_, k)| k == "claim_not_in_sources"));
    assert!(
        flags
            .iter()
            .any(|(_, k)| k == "company_fact_outside_summary")
    );
    assert!(flags.iter().any(|(_, k)| k == "skill_not_on_profile"));
    assert_eq!(d["flagged"], 3);
    assert_eq!(d["can_approve"], false);
    assert!(d["first_flagged"].is_string());

    // The number: the reason names it and what the line cites.
    let numbered = flags
        .iter()
        .find(|(_, k)| k == "claim_not_in_sources")
        .unwrap();
    let reason = &line(&cv, &numbered.0)["flags"][0];
    assert_eq!(reason["claim"], "20");
    assert_eq!(reason["cites"], json!(["w0.h1"]));

    // Marks: verbatim, reworded.
    let bullets = d["experience"][0]["bullets"].as_array().unwrap();
    assert_eq!(bullets[0]["mark"], "reworded");
    assert_eq!(bullets[1]["mark"], "as_in_profile");
    assert_eq!(bullets[0]["sources"][0]["kind"], "fact");
    assert_eq!(
        bullets[0]["sources"][0]["role"],
        "Senior Engineer · Example Org"
    );
    // The company fact in the summary is cited with its page.
    assert_eq!(d["summary"]["sources"][1]["kind"], "company");
    assert!(d["summary"]["sources"][1]["url"].is_string());

    // The third role was left out, with its line.
    assert_eq!(d["left_out"]["roles"].as_array().unwrap().len(), 1);
    assert_eq!(d["left_out"]["roles"][0]["position"], 2);
    assert_eq!(d["changes"]["left_out_roles"], 1);

    // Keywords, without the model: the posting names Rust and PostgreSQL.
    let keywords = d["keywords"]["items"].as_array().unwrap();
    let rust = keywords.iter().find(|k| k["label"] == "Rust").unwrap();
    assert_eq!(rust["state"], "on_cv");
    assert_eq!(d["keywords"]["total"], keywords.len());

    // Approve is refused while anything is flagged.
    let (status, body) =
        s.h.json("POST", &url(&s.opp, "/drafts/1/approve"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "lines_flagged");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_model_is_never_shown_who_the_person_is(db: PgPool) {
    let s = stack(db).await;
    draft(&s).await;
    let messages = s.model.user_messages(CV).await;
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    // What it needs is there...
    for needle in [
        "[w0.h1] Rebuilt the billing pipeline in Rust",
        "[w0.faaaa1111] Moved invoicing onto PostgreSQL",
        "[cab12cd34] Builds ledgers for small shops",
        "[hl] Backend engineer",
        "[sk] Rust, PostgreSQL",
        "Rust and PostgreSQL required",
        "<language>en</language>",
    ] {
        assert!(message.contains(needle), "{needle}\n{message}");
    }
    // ...and nothing about how to reach them.
    for needle in [
        "Alex",
        "Demo",
        "alex@example.com",
        "example.com/alex",
        "alex.example.com",
        "github.com",
        "Porto, Portugal",
    ] {
        assert!(!message.contains(needle), "{needle}\n{message}");
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn each_flag_is_fixed_by_an_edit_the_persons_words_or_a_removal_and_then_it_is_approved(
    db: PgPool,
) {
    let s = stack(db).await;
    let cv = draft(&s).await;
    let flags = flagged(&cv);
    let claim = flags
        .iter()
        .find(|(_, k)| k == "claim_not_in_sources")
        .unwrap()
        .0
        .clone();
    let company = flags
        .iter()
        .find(|(_, k)| k == "company_fact_outside_summary")
        .unwrap()
        .0
        .clone();
    let skill = flags
        .iter()
        .find(|(_, k)| k == "skill_not_on_profile")
        .unwrap()
        .0
        .clone();
    let lines = |id: &str| url(&s.opp, &format!("/drafts/1/lines/{id}"));

    // An edit that still says 20 is still flagged: it is checked again.
    let (status, cv) =
        s.h.json(
            "PATCH",
            &lines(&claim),
            &json!({ "text": "Cut runs to 20 minutes" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    assert_eq!(line(&cv, &claim)["mark"], "edited");
    assert_eq!(line(&cv, &claim)["flags"][0]["claim"], "20");

    // "It's true": the words become a fact on the line's position (0) and the
    // line passes with it as a source.
    let (status, cv) =
        s.h.json(
            "POST",
            &format!("{}/confirm", lines(&claim)),
            &json!({ "text": "Cut invoice runs to 20 minutes in 2023", "area": "results" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    assert_eq!(
        *s.fake.kept.lock().unwrap(),
        [(
            0,
            "Cut invoice runs to 20 minutes in 2023".to_owned(),
            "results".to_owned()
        )]
    );
    assert_eq!(line(&cv, &claim)["flags"], json!([]));
    assert_eq!(line(&cv, &claim)["sources"].as_array().unwrap().len(), 2);

    // The company fact as an achievement: editing it drops the citation.
    let (status, cv) =
        s.h.json(
            "PATCH",
            &lines(&company),
            &json!({ "text": "Built ledgers for shops" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    // Nothing else cited: it has no source left.
    assert_eq!(line(&cv, &company)["flags"][0]["kind"], "no_source");
    let (status, cv) =
        s.h.json("PATCH", &lines(&company), &json!({ "remove": true }))
            .await;
    assert_eq!(status, StatusCode::OK, "{cv}");

    // The skill: removed.
    let (status, cv) =
        s.h.json("PATCH", &lines(&skill), &json!({ "remove": true }))
            .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    assert_eq!(flagged(&cv), []);
    assert_eq!(cv["draft"]["can_approve"], true);

    let (status, cv) =
        s.h.json("POST", &url(&s.opp, "/drafts/1/approve"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    assert_eq!(cv["draft"]["status"], "approved");
    assert_eq!(cv["draft"]["version"], 1);
    assert_eq!(cv["versions"][0]["version"], 1);
    // One model call in all.
    assert_eq!(s.model.calls(CV).await, 1);

    // Frozen: neither a change nor a second approval.
    let (status, body) =
        s.h.json("PATCH", &lines(&claim), &json!({ "text": "x" }))
            .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "draft_frozen");
    let (status, _) =
        s.h.json("POST", &url(&s.opp, "/drafts/1/approve"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Making it again makes draft 2; approving it makes version 2.
    let (_, cv) = s.h.json("POST", &url(&s.opp, "/drafts"), &json!({})).await;
    assert_eq!(cv["draft"]["n"], 2);
    assert_eq!(cv["versions"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_skill_the_person_has_is_added_to_the_profile(db: PgPool) {
    let s = stack(db).await;
    let cv = draft(&s).await;
    let skill = flagged(&cv)
        .into_iter()
        .find(|(_, k)| k == "skill_not_on_profile")
        .unwrap()
        .0;
    let (status, cv) =
        s.h.json(
            "POST",
            &url(&s.opp, &format!("/drafts/1/lines/{skill}/confirm")),
            &json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    assert_eq!(*s.fake.added_skills.lock().unwrap(), ["Kubernetes"]);
    assert!(!flagged(&cv).iter().any(|(id, _)| *id == skill));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn what_was_left_out_is_put_back_as_the_profile_has_it_and_passes(db: PgPool) {
    let s = stack(db).await;
    let cv = draft(&s).await;
    let before = flagged(&cv).len();

    // The left-out role, with its lines word for word.
    let (status, cv) =
        s.h.json(
            "POST",
            &url(&s.opp, "/drafts/1/put-back"),
            &json!({ "position": 2 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    let experience = cv["draft"]["experience"].as_array().unwrap();
    assert_eq!(experience.len(), 3);
    assert_eq!(experience[2]["position"], 2);
    assert_eq!(experience[2]["bullets"][0]["text"], "Wrote internal tools");
    assert_eq!(experience[2]["bullets"][0]["mark"], "as_in_profile");
    assert_eq!(cv["draft"]["left_out"]["roles"], json!([]));
    assert_eq!(flagged(&cv).len(), before, "it passes");
    // Twice: no.
    let (status, body) =
        s.h.json(
            "POST",
            &url(&s.opp, "/drafts/1/put-back"),
            &json!({ "position": 2 }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // A line of a role that is on the CV: the unused highlight shows as left out.
    let (_, cv) = s.h.get(&url(&s.opp, "")).await;
    let unused: Vec<&Value> = cv["draft"]["left_out"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .collect();
    // w0.h1 is cited by the summary and a bullet; every other line is used.
    assert!(unused.is_empty(), "{unused:?}");
    // Put a used one back by hand: refused as already there.
    let (status, _) =
        s.h.json(
            "POST",
            &url(&s.opp, "/drafts/1/put-back"),
            &json!({ "source": "w0.h2" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) =
        s.h.json(
            "POST",
            &url(&s.opp, "/drafts/1/put-back"),
            &json!({ "source": "nope" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) =
        s.h.json("POST", &url(&s.opp, "/drafts/1/put-back"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_removed_line_can_be_put_back_from_the_profile(db: PgPool) {
    let s = stack(db).await;
    let cv = draft(&s).await;
    let led = cv["draft"]["experience"][0]["bullets"][1]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, cv) =
        s.h.json(
            "PATCH",
            &url(&s.opp, &format!("/drafts/1/lines/{led}")),
            &json!({ "remove": true }),
        )
        .await;
    let left = cv["draft"]["left_out"]["lines"].as_array().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["source"], "w0.h2");
    let (status, cv) =
        s.h.json(
            "POST",
            &url(&s.opp, "/drafts/1/put-back"),
            &json!({ "source": "w0.h2" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    let bullets = cv["draft"]["experience"][0]["bullets"].as_array().unwrap();
    assert_eq!(bullets.last().unwrap()["text"], "Led a team of 4 engineers");
    assert_eq!(cv["draft"]["left_out"]["lines"], json!([]));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_job_not_yet_shortlisted_has_no_draft(db: PgPool) {
    let s = stack(db).await;
    let found = add(&s.h, "found").await;
    let (status, cv) = s.h.get(&url(&found, "")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        cv["can_draft"],
        json!({ "allowed": false, "reason": "stage_too_early" })
    );
    let (status, body) = s.h.json("POST", &url(&found, "/drafts"), &json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "stage_too_early");
    assert_eq!(s.model.calls(CV).await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_caps_stop_a_draft_before_the_model_is_asked(db: PgPool) {
    let s = stack(db).await;
    let fill = |opp: String, from: i32, count: i32| {
        let db = s.h.db.clone();
        let account = s.h.account;
        async move {
            for n in from..from + count {
                sqlx::query(
                    "INSERT INTO pipeline.cv_drafts
                        (id, account_id, opportunity_id, n, language, status, content, frame,
                         created_at, updated_at)
                     VALUES ($1, $2, $3, $4, 'en', 'discarded', '{}', '{}', now(), now())",
                )
                .bind(Uuid::now_v7())
                .bind(account)
                .bind(Uuid::parse_str(&opp).unwrap())
                .bind(n)
                .execute(&db)
                .await
                .unwrap();
            }
        }
    };
    // 10 for this job: the 11th is refused.
    fill(s.opp.clone(), 1, 10).await;
    let (status, body) = s.h.json("POST", &url(&s.opp, "/drafts"), &json!({})).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "cv_drafts_per_opportunity_cap");
    let (_, cv) = s.h.get(&url(&s.opp, "")).await;
    assert_eq!(cv["can_draft"]["reason"], "per_opportunity_cap");

    // 20 for the day across jobs (10 are counted already): another job
    // takes 10 more, and then a third is refused too.
    let second = add(&s.h, "shortlisted").await;
    fill(second, 1, 10).await;
    let third = add(&s.h, "shortlisted").await;
    let (status, body) = s.h.json("POST", &url(&third, "/drafts"), &json!({})).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "cv_drafts_daily_cap");
    assert_eq!(s.model.calls(CV).await, 0, "no call past a limit");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn another_accounts_opportunity_is_not_found(db: PgPool) {
    let s = stack(db.clone()).await;
    draft(&s).await;
    let other = Harness::as_account(db, Uuid::now_v7(), Arc::clone(&s.fake));
    for (method, tail, body) in [
        ("GET", "", json!(null)),
        ("POST", "/drafts", json!({})),
        ("PATCH", "/drafts/1/lines/l1", json!({ "remove": true })),
        ("POST", "/drafts/1/lines/l1/confirm", json!({ "text": "x" })),
        ("POST", "/drafts/1/put-back", json!({ "position": 2 })),
        ("POST", "/drafts/1/approve", json!({})),
        ("POST", "/drafts/1/discard", json!({})),
        ("POST", "/versions", json!({ "template": "compact" })),
    ] {
        let (status, got) = if method == "GET" {
            other.get(&url(&s.opp, tail)).await
        } else {
            other.json(method, &url(&s.opp, tail), &body).await
        };
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {tail}: {got}");
    }
    // And its own account sees it untouched.
    let (_, cv) = s.h.get(&url(&s.opp, "")).await;
    assert_eq!(cv["draft"]["status"], "draft");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn making_it_again_puts_the_earlier_draft_away_and_a_draft_can_be_discarded(db: PgPool) {
    let s = stack(db).await;
    draft(&s).await;
    let cv = draft(&s).await;
    assert_eq!(cv["draft"]["n"], 2);
    // Draft 1 was put away: it cannot change.
    let (status, body) =
        s.h.json("POST", &url(&s.opp, "/drafts/1/approve"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, cv) =
        s.h.json("POST", &url(&s.opp, "/drafts/2/discard"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::OK);
    assert!(cv["draft"].is_null());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_sweep_deletes_drafts_nobody_approved_but_keeps_versions(db: PgPool) {
    let s = stack(db).await;
    draft(&s).await;
    sqlx::query("UPDATE pipeline.cv_drafts SET expires_at = now() - interval '1 minute'")
        .execute(&s.h.db)
        .await
        .unwrap();
    // Expired: not shown, even before the sweep runs.
    let (_, cv) = s.h.get(&url(&s.opp, "")).await;
    assert!(cv["draft"].is_null());
    let ctx = applicamp_kernel::JobContext {
        db: s.h.db.clone(),
        events: applicamp_kernel::EventBus::default(),
        job_id: Uuid::now_v7(),
        attempt: 1,
        event_id: None,
        deps: None,
    };
    applicamp_kernel::Job::run(super::ExpireCvDrafts {}, ctx)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM pipeline.cv_drafts")
        .fetch_one(&s.h.db)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

// ---- the PDF (#38) --------------------------------------------------------------

/// A draft with every flag removed, ready to approve.
async fn ready(s: &Stack) -> Value {
    let cv = draft(s).await;
    let mut cv = cv;
    for (id, _) in flagged(&cv) {
        let (status, next) =
            s.h.json(
                "PATCH",
                &url(&s.opp, &format!("/drafts/1/lines/{id}")),
                &json!({ "remove": true }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{next}");
        cv = next;
    }
    assert_eq!(cv["draft"]["can_approve"], true, "{cv}");
    cv
}

async fn approved(s: &Stack, layout: Value) -> Value {
    ready(s).await;
    let (status, cv) =
        s.h.json("POST", &url(&s.opp, "/drafts/1/approve"), &layout)
            .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    cv
}

fn version_pdf(opp: &str, version: u32, tail: &str) -> String {
    url(opp, &format!("/versions/{version}/pdf{tail}"))
}

async fn stored(s: &Stack, version: i32) -> Vec<u8> {
    sqlx::query_scalar("SELECT pdf FROM pipeline.cv_versions WHERE version = $1")
        .bind(version)
        .fetch_one(&s.h.db)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn approving_keeps_a_pdf_that_was_read_back_and_serves_it_inline_then_as_a_download(
    db: PgPool,
) {
    let s = stack(db).await;
    let cv = approved(&s, json!({})).await;
    assert_eq!(cv["draft"]["status"], "approved");
    assert_eq!(cv["draft"]["version"], 1);
    let v = &cv["versions"][0];
    assert_eq!(v["version"], 1);
    assert_eq!(v["template"], "clean");
    assert_eq!(v["length"], 1);
    assert_eq!(v["pages"], 1);
    assert_eq!(v["current"], true);
    assert!(v["size_bytes"].as_u64().unwrap() > 1000);
    assert!(v["downloaded_at"].is_null());
    // What the read-back found.
    let checks = &v["checks"];
    assert_eq!(checks["sections"]["missing"], json!([]));
    assert_eq!(
        checks["sections"]["expected"],
        json!(["Summary", "Experience", "Skills", "Education", "Languages"])
    );
    assert_eq!(checks["lines"]["missing"], json!([]));
    assert_eq!(checks["keywords"]["missing"], json!([]));
    assert_eq!(checks["text_layer"], true);
    assert_eq!(checks["contact"]["links"], true);
    assert_eq!(checks["size"]["pages"], 1);

    // Viewing: inline, never cached or sniffed.
    let (status, headers, bytes) = s.h.get_raw(&version_pdf(&s.opp, 1, "")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(bytes, stored(&s, 1).await);
    let header = |name: &str| headers[name].to_str().unwrap().to_owned();
    assert_eq!(header("content-type"), "application/pdf");
    assert_eq!(header("cache-control"), "private, no-store");
    assert_eq!(header("x-content-type-options"), "nosniff");
    let disposition = header("content-disposition");
    assert!(
        disposition
            .starts_with("inline; filename=\"Alex Demo CV - Example Co.pdf\"; filename*=UTF-8''"),
        "{disposition}"
    );
    let (_, cv) = s.h.get(&url(&s.opp, "")).await;
    assert!(
        cv["versions"][0]["downloaded_at"].is_null(),
        "viewing is not downloading"
    );

    // Downloading: an attachment, and the time is kept.
    let (status, headers, _) = s.h.get_raw(&version_pdf(&s.opp, 1, "?download=1")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment; filename=\"Alex Demo CV - Example Co.pdf\"")
    );
    let (_, cv) = s.h.get(&url(&s.opp, "")).await;
    assert!(cv["versions"][0]["downloaded_at"].is_string());

    // No such version.
    let (status, _, _) = s.h.get_raw(&version_pdf(&s.opp, 2, "")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // The PDF text is the CV's, for an ATS: read it back here too.
    let read = super::pdf::readback::extract(&bytes).unwrap();
    assert!(
        read.text.contains("Moved invoicing over to PostgreSQL"),
        "{}",
        read.text
    );
    assert!(read.text.contains("alex@example.com"));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_new_template_or_length_is_the_next_version_with_no_model_and_the_old_bytes_stay(
    db: PgPool,
) {
    let s = stack(db).await;
    approved(&s, json!({ "template": "clean", "length": 1 })).await;
    let before = stored(&s, 1).await;
    assert_eq!(s.model.calls(CV).await, 1);

    let (status, cv) =
        s.h.json(
            "POST",
            &url(&s.opp, "/versions"),
            &json!({ "template": "compact" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    let versions = cv["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0]["version"], 2);
    assert_eq!(versions[0]["template"], "compact");
    assert_eq!(versions[0]["current"], true);
    assert_eq!(versions[1]["version"], 1);
    assert_eq!(versions[1]["template"], "clean");
    assert_eq!(versions[1]["current"], false);
    // The approved draft is shown as its newest version.
    assert_eq!(cv["draft"]["version"], 2);
    // The same text: both say the same line.
    assert_eq!(
        stored(&s, 1).await,
        before,
        "what was sent is never overwritten"
    );
    assert_ne!(stored(&s, 2).await, before);
    assert_eq!(
        s.model.calls(CV).await,
        1,
        "a layout change never calls the model"
    );

    // Two pages, from the newest version's template.
    let (status, cv) =
        s.h.json("POST", &url(&s.opp, "/versions"), &json!({ "length": 2 }))
            .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    assert_eq!(cv["versions"][0]["version"], 3);
    assert_eq!(cv["versions"][0]["template"], "compact");
    assert_eq!(cv["versions"][0]["length"], 2);

    // The layout it already has, and what cannot be asked for.
    let (status, body) =
        s.h.json(
            "POST",
            &url(&s.opp, "/versions"),
            &json!({ "template": "compact" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "same_layout");
    let (status, body) =
        s.h.json("POST", &url(&s.opp, "/versions"), &json!({ "length": 3 }))
            .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "invalid_length");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_pdf_needs_an_approved_cv_and_the_versions_are_capped(db: PgPool) {
    let s = stack(db).await;
    let (status, body) =
        s.h.json(
            "POST",
            &url(&s.opp, "/versions"),
            &json!({ "template": "compact" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "not_approved");

    ready(&s).await;
    let (status, _) =
        s.h.json("POST", &url(&s.opp, "/drafts/1/approve"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::OK);
    // Filling the cap without rendering: the rows are what is counted.
    sqlx::query(
        "INSERT INTO pipeline.cv_versions
            (id, account_id, opportunity_id, draft_id, version, template, length, language,
             pdf, size_bytes, pages, checks, created_at)
         SELECT gen_random_uuid(), account_id, opportunity_id, draft_id, 1 + g, 'clean', 1, language,
                pdf, size_bytes, pages, checks, created_at
           FROM pipeline.cv_versions, generate_series(1, 19) g",
    )
    .execute(&s.h.db)
    .await
    .unwrap();
    let (status, body) =
        s.h.json(
            "POST",
            &url(&s.opp, "/versions"),
            &json!({ "template": "compact" }),
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "cv_versions_cap");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_cv_that_does_not_fit_the_length_is_not_approved_and_nothing_is_kept(db: PgPool) {
    let s = stack(db).await;
    ready(&s).await;
    // Seventy lines in the first role, each word for word what the profile says.
    let bullets: Vec<Value> = (0..70)
        .map(|i| {
            json!({ "id": format!("x{i}"), "text": "Led a team of 4 engineers",
                          "sources": ["w0.h2"], "edited": false })
        })
        .collect();
    sqlx::query(
        "UPDATE pipeline.cv_drafts SET content = jsonb_set(content, '{experience,0,bullets}', $1)",
    )
    .bind(sqlx::types::Json(bullets))
    .execute(&s.h.db)
    .await
    .unwrap();

    let (status, body) =
        s.h.json("POST", &url(&s.opp, "/drafts/1/approve"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "cv_doesnt_fit");
    assert_eq!(
        body["detail"],
        "Doesn't fit on 1 page: switch to 2 pages or remove a few lines"
    );
    let (_, cv) = s.h.get(&url(&s.opp, "")).await;
    assert_eq!(cv["draft"]["status"], "draft", "nothing was approved");
    assert_eq!(cv["versions"], json!([]));
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM pipeline.cv_versions")
        .fetch_one(&s.h.db)
        .await
        .unwrap();
    assert_eq!(kept, 0);

    // Two pages take it.
    let (status, cv) =
        s.h.json(
            "POST",
            &url(&s.opp, "/drafts/1/approve"),
            &json!({ "length": 2 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    assert_eq!(cv["versions"][0]["pages"], 2);
    assert_eq!(cv["versions"][0]["length"], 2);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_cvs_page_lists_every_version_and_live_draft_of_the_account_and_no_one_elses(
    db: PgPool,
) {
    let s = stack(db.clone()).await;
    // Nothing yet.
    let (status, list) = s.h.get("/api/v1/cvs").await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["items"], json!([]));

    approved(&s, json!({})).await;
    let (status, _) =
        s.h.json(
            "POST",
            &url(&s.opp, "/versions"),
            &json!({ "template": "compact" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    // A draft on another job.
    let second = add(&s.h, "shortlisted").await;
    let (status, _) = s.h.json("POST", &url(&second, "/drafts"), &json!({})).await;
    assert_eq!(status, StatusCode::OK);

    let (_, list) = s.h.get("/api/v1/cvs").await;
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "{list}");
    // Newest first: the draft, v2, v1.
    assert_eq!(items[0]["kind"], "draft");
    assert_eq!(items[0]["status"], "draft");
    assert_eq!(items[0]["opportunity_id"], second);
    assert!(items[0]["version"].is_null());
    assert_eq!(items[1]["kind"], "version");
    assert_eq!(items[1]["version"], 2);
    assert_eq!(items[1]["template"], "compact");
    assert_eq!(items[1]["current"], true);
    assert_eq!(items[1]["company"], "Example Co");
    assert_eq!(items[1]["title"], "Staff Engineer");
    assert_eq!(items[1]["opportunity_id"], s.opp);
    assert_eq!(items[2]["version"], 1);
    assert_eq!(items[2]["current"], false);
    assert_eq!(items[2]["pages"], 1);
    assert!(items[2]["size_bytes"].as_u64().unwrap() > 1000);

    // Someone else sees none of it, and cannot open its PDF.
    let other = Harness::as_account(db, Uuid::now_v7(), Arc::clone(&s.fake));
    let (_, list) = other.get("/api/v1/cvs").await;
    assert_eq!(list["items"], json!([]));
    let (status, _, _) = other.get_raw(&version_pdf(&s.opp, 1, "")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = other.get_raw(&version_pdf(&s.opp, 1, "?download=1")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let downloaded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pipeline.cv_versions WHERE downloaded_at IS NOT NULL",
    )
    .fetch_one(&s.h.db)
    .await
    .unwrap();
    assert_eq!(downloaded, 0, "a refused download is not a download");
}
