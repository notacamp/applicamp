//! The cover letter over HTTP and as a PDF: fake neighbours hold the profile,
//! a fake model answers the CV prompt and the letter prompt. All data is
//! invented.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    CAP_PER_DAY, CAP_PER_OPPORTUNITY,
    model::{Answer, Content, Paragraph, Sentence},
    pdf,
};
use crate::{
    features::cv::{
        model::Contact,
        pdf::{Refusal, readback},
        tests::{COMPANY_FACT, CV, add, answer as cv_answer, company_fact, profile},
    },
    testkit::{Fake, Harness, Model},
};

const LETTER: &str = "You write a cover letter";
const POSTING: &str = "You will own billing. Rust and PostgreSQL required.\n\n\
                       Application questions\n\
                       - Why do you want to work at Example Co?\n\
                       - What's your notice period?\n";

/// What the fake model answers to the letter prompt: a paragraph citing a
/// company fact and the profile, one with a superlative, one that cites only a
/// company fact yet claims something about the person, a closing that claims
/// nothing, one job question it can support and one ("notice period") it can't.
fn answer() -> Value {
    json!({
        "paragraphs": [
            { "text": "", "sentences": [
                { "text": "Example Co builds ledgers for small shops, and I have spent years on billing in Rust.",
                  "sources": ["cab12cd34", "w0.h1"] },
                { "text": "I cut invoice runs from 6 hours to 40 minutes.", "sources": ["w0.h1"] },
            ]},
            { "text": "", "sentences": [
                { "text": "I'm the go-to person for the hardest billing problems.", "sources": [] },
            ]},
            { "text": "", "sentences": [
                { "text": "I led our migration, as Example Co did for its shops.",
                  "sources": ["cab12cd34"] },
            ]},
            { "text": "", "sentences": [
                { "text": "I'd like to bring that experience to your team.", "sources": [] },
            ]},
        ],
        "answers": [
            { "question": "Why do you want to work at Example Co?",
              "text": "Because you build ledgers for small shops, which is what I have built in Rust.",
              "sources": ["cab12cd34", "w0.h1"] },
            { "question": "What's your notice period?", "text": "", "sources": [] },
        ],
    })
}

struct Stack {
    h: Harness,
    fake: Arc<Fake>,
    model: Model,
    opp: String,
}

async fn stack_with(db: PgPool, stage: &str) -> Stack {
    let fake = Arc::new(Fake::default());
    *fake.cv_profile.lock().unwrap() = Some(profile());
    let model = Model::with_documents(
        json!({"facts": []}),
        json!({"wants": [], "questions": []}),
        cv_answer(),
        answer(),
    )
    .await;
    let h = Harness::with_ai(db, Uuid::now_v7(), Arc::clone(&fake), Some(model.gateway()));
    company_fact(&h).await;
    let opp = add(&h, stage).await;
    sqlx::query("UPDATE pipeline.opportunities SET description = $2 WHERE id = $1")
        .bind(Uuid::parse_str(&opp).unwrap())
        .bind(POSTING)
        .execute(&h.db)
        .await
        .unwrap();
    Stack {
        h,
        fake,
        model,
        opp,
    }
}

async fn stack(db: PgPool) -> Stack {
    stack_with(db, "shortlisted").await
}

fn url(opp: &str, tail: &str) -> String {
    format!("/api/v1/opportunities/{opp}/letter{tail}")
}

fn cv_url(opp: &str, tail: &str) -> String {
    format!("/api/v1/opportunities/{opp}/cv{tail}")
}

/// Approve the CV of the job: draft, remove what is flagged, approve.
async fn approve_cv(s: &Stack) {
    let (status, mut cv) =
        s.h.json("POST", &cv_url(&s.opp, "/drafts"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
    loop {
        let draft = &cv["draft"];
        let mut flagged: Vec<String> = Vec::new();
        for line in [&draft["headline"], &draft["summary"]]
            .into_iter()
            .chain(
                draft["experience"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|r| r["bullets"].as_array().unwrap().iter()),
            )
            .chain(draft["skills"].as_array().unwrap().iter())
        {
            if !line["flags"].as_array().unwrap().is_empty() {
                flagged.push(line["id"].as_str().unwrap().to_owned());
            }
        }
        let Some(id) = flagged.first() else { break };
        let (status, next) =
            s.h.json(
                "PATCH",
                &cv_url(&s.opp, &format!("/drafts/1/lines/{id}")),
                &json!({ "remove": true }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{next}");
        cv = next;
    }
    let (status, cv) =
        s.h.json("POST", &cv_url(&s.opp, "/drafts/1/approve"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::OK, "{cv}");
}

async fn draft(s: &Stack) -> Value {
    let (status, letter) = s.h.json("POST", &url(&s.opp, "/drafts"), &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{letter}");
    letter
}

/// The sentence whose text starts with `start`.
fn sentence<'a>(letter: &'a Value, start: &str) -> &'a Value {
    letter["draft"]["paragraphs"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["sentences"].as_array().unwrap().iter())
        .find(|s| s["text"].as_str().unwrap().starts_with(start))
        .unwrap_or_else(|| panic!("no sentence {start:?} in {letter}"))
}

fn answer_of<'a>(letter: &'a Value, question: &str) -> &'a Value {
    letter["draft"]["answers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["question"] == question)
        .unwrap_or_else(|| panic!("no answer to {question:?} in {letter}"))
}

fn sentences_url(s: &Stack, id: &Value, tail: &str) -> String {
    url(
        &s.opp,
        &format!("/drafts/1/sentences/{}{tail}", id.as_str().unwrap()),
    )
}

const NOTICE: &str = "What's your notice period?";

#[sqlx::test(migrations = "../../../migrations")]
async fn with_no_language_asked_the_letter_is_in_the_approved_cvs(db: PgPool) {
    let s = stack(db).await;
    approve_cv(&s).await;
    sqlx::query("UPDATE pipeline.cv_drafts SET language = 'pt-PT' WHERE status = 'approved'")
        .execute(&s.h.db)
        .await
        .unwrap();
    let letter = draft(&s).await;
    assert_eq!(letter["draft"]["language"], "pt-PT", "{letter}");
    assert!(s.model.user_messages(LETTER).await[0].contains("<language>pt-PT</language>"));
    // Asked for, another.
    let (status, letter) =
        s.h.json(
            "POST",
            &url(&s.opp, "/drafts"),
            &json!({ "language": "en" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{letter}");
    assert_eq!(letter["draft"]["language"], "en");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_letter_needs_an_approved_cv_and_says_why(db: PgPool) {
    let s = stack(db).await;
    let (status, letter) = s.h.get(&url(&s.opp, "")).await;
    assert_eq!(status, StatusCode::OK, "{letter}");
    assert_eq!(letter["can_write"]["allowed"], false);
    assert_eq!(letter["can_write"]["reason"], "no_approved_cv");
    let (status, body) = s.h.json("POST", &url(&s.opp, "/drafts"), &json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "no_approved_cv");
    assert_eq!(s.model.calls(LETTER).await, 0);

    approve_cv(&s).await;
    let (_, letter) = s.h.get(&url(&s.opp, "")).await;
    assert_eq!(letter["can_write"]["allowed"], true);
    assert_eq!(letter["cv_version"], 1);
    // The job's own questions, from its posting.
    assert_eq!(
        letter["questions"],
        json!(["Why do you want to work at Example Co?", NOTICE])
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_job_not_yet_shortlisted_has_no_letter(db: PgPool) {
    let s = stack_with(db, "found").await;
    let (_, letter) = s.h.get(&url(&s.opp, "")).await;
    assert_eq!(letter["can_write"]["reason"], "stage_too_early");
    let (status, body) = s.h.json("POST", &url(&s.opp, "/drafts"), &json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "stage_too_early");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_draft_is_tagged_by_what_it_rests_on_and_the_check_flags_what_nothing_backs(db: PgPool) {
    let s = stack(db).await;
    approve_cv(&s).await;
    let letter = draft(&s).await;
    assert_eq!(s.model.calls(LETTER).await, 1);
    let d = &letter["draft"];
    assert_eq!(d["n"], 1);
    assert_eq!(d["status"], "draft");
    assert_eq!(d["size"], "short");
    assert_eq!(d["cv_version"], 1);
    // The server filled in the contact and the greeting; the model had neither.
    assert_eq!(d["contact"]["name"], "Alex Demo");
    assert_eq!(d["greeting"], "Dear hiring team at Example Co,");

    // Paragraph tags: a company fact and the profile; no claims.
    let paragraphs = d["paragraphs"].as_array().unwrap();
    assert_eq!(paragraphs.len(), 4);
    assert_eq!(
        paragraphs[0]["tags"],
        json!(["company_fact", "from_profile"])
    );
    assert_eq!(paragraphs[3]["tags"], json!(["no_claims"]));
    assert_eq!(paragraphs[0]["sources"].as_array().unwrap().len(), 2);
    assert_eq!(d["company_facts"].as_array().unwrap().len(), 1);
    assert_eq!(d["company_facts"][0]["text"], COMPANY_FACT);

    // Flagged, never dropped: the superlative, the company-only claim, and
    // the question the profile can't answer.
    let superlative = sentence(&letter, "I'm the go-to");
    assert_eq!(superlative["flags"][0]["kind"], "superlative");
    assert_eq!(superlative["flags"][0]["word"], "go-to");
    let company_only = sentence(&letter, "I led our migration");
    assert_eq!(company_only["flags"][0]["kind"], "company_only_about_you");
    assert_eq!(sentence(&letter, "Example Co builds")["flags"], json!([]));
    assert_eq!(sentence(&letter, "I'd like")["flags"], json!([]));
    let notice = answer_of(&letter, NOTICE);
    assert_eq!(notice["text"], "");
    assert_eq!(notice["flags"][0]["kind"], "not_in_profile");
    let why = answer_of(&letter, "Why do you want to work at Example Co?");
    assert_eq!(why["flags"], json!([]));
    assert_eq!(why["tags"], json!(["company_fact", "from_profile"]));
    assert_eq!(d["flagged"], 3);
    assert_eq!(d["can_approve"], false);
    assert!(d["first_flagged"].is_string());
    assert!(d["words"].as_u64().unwrap() > 20);

    // Approve is refused while anything is open, and nothing is kept.
    let (status, body) =
        s.h.json("POST", &url(&s.opp, "/drafts/1/approve"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "letter_flagged");
    let (_, letter) = s.h.get(&url(&s.opp, "")).await;
    assert_eq!(letter["versions"], json!([]));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_model_is_never_shown_who_the_person_is_and_is_given_the_cv_and_the_questions(
    db: PgPool,
) {
    let s = stack(db).await;
    approve_cv(&s).await;
    draft(&s).await;
    let messages = s.model.user_messages(LETTER).await;
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    for needle in [
        "[w0.h1] Rebuilt the billing pipeline in Rust",
        "[cab12cd34] Builds ledgers for small shops",
        "<language>en</language>",
        "<length>about 180 words</length>",
        "<cv>\n- Backend engineer who ships billing systems",
        "1. Why do you want to work at Example Co?\n2. What's your notice period?",
    ] {
        assert!(message.contains(needle), "{needle}\n{message}");
    }
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
    // The standard length is one tap away.
    let (status, _) =
        s.h.json(
            "POST",
            &url(&s.opp, "/drafts"),
            &json!({ "size": "standard" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let messages = s.model.user_messages(LETTER).await;
    assert!(messages[1].contains("<length>about 300 words</length>"));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn each_flag_is_fixed_by_the_persons_words_an_edit_a_removal_or_their_own_answer_and_then_it_is_approved(
    db: PgPool,
) {
    let s = stack(db).await;
    approve_cv(&s).await;
    let letter = draft(&s).await;

    // "It's true": the words become a fact on the profile and a source.
    let go_to = sentence(&letter, "I'm the go-to")["id"].clone();
    let (status, letter) =
        s.h.json(
            "POST",
            &sentences_url(&s, &go_to, "/confirm"),
            &json!({ "text": "Known as the go-to person for the hardest billing problems" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{letter}");
    assert_eq!(
        *s.fake.kept.lock().unwrap(),
        [(
            0,
            "Known as the go-to person for the hardest billing problems".to_owned(),
            "results".to_owned()
        )]
    );
    assert_eq!(sentence(&letter, "I'm the go-to")["flags"], json!([]));
    assert_eq!(
        sentence(&letter, "I'm the go-to")["sources"][0]["kind"],
        "fact"
    );

    // An edit that is still about the person but cites only the company is
    // still flagged: it is checked again.
    let company_only = sentence(&letter, "I led our migration")["id"].clone();
    let (status, letter) =
        s.h.json(
            "PATCH",
            &sentences_url(&s, &company_only, ""),
            &json!({ "text": "I led the migration for Example Co." }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{letter}");
    assert_eq!(
        sentence(&letter, "I led the migration")["flags"][0]["kind"],
        "company_only_about_you"
    );
    assert_eq!(sentence(&letter, "I led the migration")["edited"], true);
    // Said about the company, it passes.
    let (status, letter) =
        s.h.json(
            "PATCH",
            &sentences_url(&s, &company_only, ""),
            &json!({ "text": "Example Co builds ledgers for small shops." }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{letter}");
    assert_eq!(
        sentence(&letter, "Example Co builds ledgers for small shops.")["flags"],
        json!([])
    );

    // The question the profile can't answer: the person's own words.
    let notice = answer_of(&letter, NOTICE)["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(letter["draft"]["can_approve"], false);
    let (status, letter) =
        s.h.json(
            "PATCH",
            &url(&s.opp, &format!("/drafts/1/answers/{notice}")),
            &json!({ "text": "One month." }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{letter}");
    assert_eq!(answer_of(&letter, NOTICE)["own"], true);
    assert_eq!(answer_of(&letter, NOTICE)["text"], "One month.");
    assert_eq!(letter["draft"]["flagged"], 0);
    assert_eq!(letter["draft"]["can_approve"], true);

    let (status, letter) =
        s.h.json("POST", &url(&s.opp, "/drafts/1/approve"), &json!({}))
            .await;
    assert_eq!(status, StatusCode::OK, "{letter}");
    assert_eq!(letter["draft"]["status"], "approved");
    assert_eq!(letter["draft"]["version"], 1);
    assert_eq!(letter["versions"][0]["version"], 1);
    assert_eq!(letter["versions"][0]["pages"], 1);
    assert_eq!(
        letter["versions"][0]["checks"]["lines"]["missing"],
        json!([])
    );
    // One model call in all.
    assert_eq!(s.model.calls(LETTER).await, 1);

    // Frozen: neither a change nor a second approval.
    let (status, body) =
        s.h.json(
            "PATCH",
            &sentences_url(&s, &go_to, ""),
            &json!({ "text": "x" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "draft_frozen");

    // The PDF is the letter: inline, then as a download; the answers are not in it.
    let (status, headers, bytes) = s.h.get_raw(&url(&s.opp, "/versions/1/pdf")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/pdf");
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("inline; filename=\"Alex Demo Cover letter - Example Co.pdf\"")
    );
    assert!(bytes.starts_with(b"%PDF-"));
    let read = readback::extract(&bytes).unwrap();
    let flat = readback::flat(&read.text);
    assert!(flat.contains(&readback::flat("Dear hiring team at Example Co,")));
    assert!(flat.contains(&readback::flat(
        "I cut invoice runs from 6 hours to 40 minutes."
    )));
    assert!(!flat.contains(&readback::flat("One month.")));
    assert!(!flat.contains(&readback::flat("Because you build ledgers")));
    let (status, headers, _) =
        s.h.get_raw(&url(&s.opp, "/versions/1/pdf?download=1"))
            .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );

    // The CVs page lists the letter next to the CV.
    let (_, list) = s.h.get("/api/v1/cvs").await;
    let items = list["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .any(|i| i["document"] == "letter" && i["kind"] == "version" && i["version"] == 1),
        "{list}"
    );
    assert!(items.iter().any(|i| i["document"] == "cv"));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_removed_sentence_goes_and_its_empty_paragraph_with_it(db: PgPool) {
    let s = stack(db).await;
    approve_cv(&s).await;
    let letter = draft(&s).await;
    let go_to = sentence(&letter, "I'm the go-to")["id"].clone();
    let (status, letter) =
        s.h.json(
            "PATCH",
            &sentences_url(&s, &go_to, ""),
            &json!({ "remove": true }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{letter}");
    assert_eq!(letter["draft"]["paragraphs"].as_array().unwrap().len(), 3);
    assert_eq!(letter["draft"]["flagged"], 2);
    // Neither or both of text and remove is no change.
    let (status, body) =
        s.h.json("PATCH", &sentences_url(&s, &go_to, ""), &json!({}))
            .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn letter_drafts_are_capped_apart_from_cv_drafts(db: PgPool) {
    let s = stack(db).await;
    approve_cv(&s).await;
    let fill = |table: &'static str, opp: String, from: i32, count: i32| {
        let db = s.h.db.clone();
        let account = s.h.account;
        async move {
            for n in from..from + count {
                let sql = if table == "cv_drafts" {
                    "INSERT INTO pipeline.cv_drafts
                        (id, account_id, opportunity_id, n, language, status, content, frame,
                         created_at, updated_at)
                     VALUES ($1, $2, $3, $4, 'en', 'discarded', '{}', '{}', now(), now())"
                } else {
                    "INSERT INTO pipeline.letter_drafts
                        (id, account_id, opportunity_id, n, language, status, content, frame,
                         created_at, updated_at)
                     VALUES ($1, $2, $3, $4, 'en', 'discarded', '{}', '{}', now(), now())"
                };
                sqlx::query(sql)
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
    // The CV's cap is spent for this job and for the day: a letter still goes.
    fill("cv_drafts", s.opp.clone(), 2, 9).await;
    let other = add(&s.h, "shortlisted").await;
    fill("cv_drafts", other.clone(), 1, 10).await;
    let (_, letter) = s.h.get(&url(&s.opp, "")).await;
    assert_eq!(letter["caps"]["drafts_here"], 0);
    assert_eq!(letter["caps"]["drafts_today"], 0);
    assert_eq!(letter["can_write"]["allowed"], true);
    draft(&s).await;

    // 10 letters for this job: the 11th is refused before the model is asked.
    fill("letter_drafts", s.opp.clone(), 2, 9).await;
    let (status, body) = s.h.json("POST", &url(&s.opp, "/drafts"), &json!({})).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "letter_drafts_per_opportunity_cap");
    let (_, letter) = s.h.get(&url(&s.opp, "")).await;
    assert_eq!(letter["can_write"]["reason"], "per_opportunity_cap");
    assert_eq!(letter["caps"]["drafts_here_cap"], CAP_PER_OPPORTUNITY);

    // 20 for the day across jobs (10 are counted): another job takes 10 more
    // and then a third is refused too.
    approve_cv_for(&s, &other).await;
    fill("letter_drafts", other.clone(), 1, 10).await;
    let third = add(&s.h, "shortlisted").await;
    approve_cv_for(&s, &third).await;
    let (status, body) = s.h.json("POST", &url(&third, "/drafts"), &json!({})).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "letter_drafts_daily_cap");
    assert_eq!(letter["caps"]["drafts_today_cap"], CAP_PER_DAY);
    assert_eq!(s.model.calls(LETTER).await, 1, "no call past a limit");
    // And the CV's own count is untouched by the letters.
    assert_eq!(s.model.calls(CV).await, 1);
}

/// An approved CV for another job of the same account, made from the
/// database (the CV drafts cap is spent by then).
async fn approve_cv_for(s: &Stack, opp: &str) {
    let draft_id = Uuid::now_v7();
    let content = serde_json::to_value(crate::features::cv::model::Content::default()).unwrap();
    sqlx::query(
        "INSERT INTO pipeline.cv_drafts
            (id, account_id, opportunity_id, n, language, status, version, content, frame,
             created_at, updated_at, approved_at)
         VALUES ($1, $2, $3, 99, 'en', 'approved', 1, $4, '{}', now(), now(), now())",
    )
    .bind(draft_id)
    .bind(s.h.account)
    .bind(Uuid::parse_str(opp).unwrap())
    .bind(content)
    .execute(&s.h.db)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../../migrations")]
async fn another_accounts_letter_is_not_found(db: PgPool) {
    let s = stack(db.clone()).await;
    approve_cv(&s).await;
    draft(&s).await;
    let other = Harness::as_account(db, Uuid::now_v7(), Arc::clone(&s.fake));
    let (status, _) = other.get(&url(&s.opp, "")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = other
        .json("POST", &url(&s.opp, "/drafts"), &json!({}))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = other.get_raw(&url(&s.opp, "/versions/1/pdf")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---- the PDF ----------------------------------------------------------------

fn content(paragraphs: &[&str]) -> Content {
    let mut content = Content {
        contact: Contact {
            name: Some("Alex Demo".into()),
            email: Some("alex.demo@example.com".into()),
            location: Some("Porto, Portugal".into()),
            url: Some("https://alex.example.com".into()),
            links: vec![(Some("GitHub".into()), "https://github.com/alex-demo".into())],
        },
        ..Content::default()
    };
    for text in paragraphs {
        let id = content.new_id("s");
        let paragraph_id = content.new_id("p");
        content.paragraphs.push(Paragraph {
            id: paragraph_id,
            sentences: vec![Sentence {
                id,
                text: (*text).to_owned(),
                sources: vec!["w0.h1".into()],
                edited: false,
            }],
        });
    }
    content.answers.push(Answer {
        id: "q9".into(),
        question: "Notice?".into(),
        text: "ANSWER-NOT-IN-THE-PDF".into(),
        sources: vec![],
        own: true,
    });
    content
}

const BODY: [&str; 3] = [
    "Example Co builds ledgers for small shops, and I have spent years on billing in Rust.",
    "I cut invoice runs from 6 hours to 40 minutes by rebuilding the pipeline.",
    "I would be glad to talk about how that could help your team.",
];

#[tokio::test]
async fn the_letter_renders_on_one_page_and_the_read_back_finds_every_paragraph() {
    let made = pdf::make(&content(&BODY), "Example Co", "en")
        .await
        .unwrap_or_else(|r| panic!("{r:?}"));
    assert!(made.bytes.starts_with(b"%PDF-"));
    assert_eq!(made.pages, 1);
    let checks = &made.checks;
    assert!(checks.passed(), "{checks:?}");
    assert!(checks.lines.missing.is_empty());
    // The greeting, three paragraphs and the sign-off.
    assert_eq!(checks.lines.total, 5);
    assert!(checks.text_layer);
    assert!(checks.contact.name && checks.contact.email && checks.contact.links);
    let read = readback::extract(&made.bytes).unwrap();
    assert!(read.text.contains("alex.demo@example.com"), "{}", read.text);
    assert!(read.text.contains("Alex Demo"));
    assert_eq!(read.link_targets, 3);
    assert_eq!(read.images, 0);
    let flat = readback::flat(&read.text);
    for paragraph in BODY {
        assert!(flat.contains(&readback::flat(paragraph)), "{paragraph}");
    }
    // The answers to the job's questions are text to copy, not part of it.
    assert!(!read.text.contains("ANSWER-NOT-IN-THE-PDF"));
}

#[tokio::test]
async fn the_letter_is_set_in_the_language_of_the_job() {
    for (tag, greeting, closing) in [
        (
            "pt-PT",
            "Caros responsáveis pelo recrutamento na Example Co,",
            "Com os melhores cumprimentos,",
        ),
        (
            "es-ES",
            "Estimado equipo de selección de Example Co,",
            "Atentamente,",
        ),
        (
            "fr",
            "Bonjour à l'équipe de recrutement de Example Co,",
            "Cordialement,",
        ),
    ] {
        let made = pdf::make(&content(&BODY), "Example Co", tag)
            .await
            .unwrap_or_else(|r| panic!("{tag}: {r:?}"));
        let flat = readback::flat(&readback::extract(&made.bytes).unwrap().text);
        assert!(flat.contains(&readback::flat(greeting)), "{tag}");
        assert!(flat.contains(&readback::flat(closing)), "{tag}");
        assert!(made.checks.passed(), "{tag}: {:?}", made.checks);
    }
}

/// What a person could type that Typst would read as something else.
const HOSTILE: [&str; 8] = [
    "#import \"@preview/x:0.1.0\": *",
    "a ] closing bracket and a [ opening one",
    "Costs $5 and $x^2$ here",
    "*not bold* and _not italic_",
    "<x> and <label> are not labels",
    "see @ref and @fig-1",
    "#read(\"/etc/passwd\")",
    "= not a heading, - not a list, + not numbered",
];

#[tokio::test]
async fn what_the_person_types_is_text_never_typst() {
    let mut letter = content(&HOSTILE);
    letter.contact.name = Some("Alex #Demo *Star* _U_ $m$".into());
    let made = pdf::make(&letter, "Co #set text(size: 40pt)", "en")
        .await
        .unwrap_or_else(|r| panic!("{r:?}"));
    let read = readback::extract(&made.bytes).unwrap();
    let flat = readback::flat(&read.text);
    for text in HOSTILE {
        assert!(
            flat.contains(&readback::flat(text)),
            "{text:?} did not come out literally of {}",
            read.text
        );
    }
    assert!(flat.contains(&readback::flat("Alex #Demo *Star* _U_ $m$")));
    assert!(flat.contains(&readback::flat(
        "Dear hiring team at Co #set text(size: 40pt),"
    )));
    assert!(made.checks.passed(), "{:?}", made.checks);
    assert!(!read.text.contains("passwd:"));
}

#[tokio::test]
async fn a_letter_that_does_not_fit_one_page_is_not_made() {
    let long: Vec<String> = (0..40)
        .map(|i| format!("Delivered project number {i} for a partner, on time and within budget, with a long tail of detail."))
        .collect();
    let long: Vec<&str> = long.iter().map(String::as_str).collect();
    match pdf::make(&content(&long), "Example Co", "en").await {
        Err(Refusal::DoesntFit { pages, length }) => {
            assert_eq!(length, 1);
            assert!(pages >= 2);
        }
        other => panic!("expected the letter not to fit, got {other:?}"),
    }
}

#[test]
fn a_lost_paragraph_fails_the_read_back() {
    let letter = content(&BODY);
    let prepared = pdf::prepare(&letter, "Example Co", "en").unwrap();
    let text = "Alex Demo\nalex.demo@example.com\nDear hiring team at Example Co,\nSincerely,\n";
    let checks = readback::check(
        &prepared.expect,
        &[],
        &readback::Extracted {
            text: text.into(),
            pages: 1,
            link_targets: 3,
            images: 0,
        },
        1,
        1000,
    );
    assert!(!checks.passed());
    assert_eq!(checks.lines.missing.len(), 3);
}
