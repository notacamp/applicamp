//! Imports over HTTP: the LinkedIn export, a CV as PDF and pasted text
//! (against a fake model), reviewing, applying and discarding drafts.

use applicamp_ai::usage;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::testkit::{CV_LINES, export_zip, export_zip_with, pdf_saying};
use crate::testkit::{Harness, Part, fake_model, gateway};

async fn import_zip(h: &Harness, zip: &[u8]) -> Value {
    let (status, import) = h
        .upload(&[
            Part::Text("kind", "linkedin_zip"),
            Part::File("file", "Basic_LinkedInDataExport.zip", zip),
        ])
        .await;
    assert_eq!(status, 201, "{import}");
    import
}

fn id_of(import: &Value) -> String {
    import["id"].as_str().unwrap().to_owned()
}

/// What the fake model answers for the invented CV: what it says, plus an
/// employer and a skill it does not.
fn model_answer() -> Value {
    json!({
        "basics": {
            "name": "Alex Demo",
            "label": "Backend engineer",
            "email": "alex.demo@example.com",
            "location": "Lisbon, Portugal",
        },
        "work": [
            { "name": "Example Org", "position": "Senior Engineer", "startDate": "2021-03",
              "summary": "Runs the billing platform." },
            { "name": "Invented Corp", "position": "Staff Engineer", "startDate": "2015-01" },
            { "name": "Sample Labs", "position": "Engineer", "startDate": "2017-06",
              "endDate": "2021-02" },
        ],
        "education": [
            { "institution": "Sample University", "studyType": "Master's degree",
              "startDate": "2012", "endDate": "2017" },
        ],
        "skills": [{ "name": "Rust" }, { "name": "PostgreSQL" }, { "name": "Kubernetes" }],
        "languages": [{ "language": "Portuguese" }, { "language": "English" }],
    })
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_linkedin_export_becomes_a_draft_and_nothing_else(db: PgPool) {
    let h = Harness::new(db.clone(), None);
    let import = import_zip(&h, &export_zip()).await;

    assert_eq!(import["kind"], "linkedin_zip");
    assert_eq!(import["status"], "ready");
    assert_eq!(import["dropped"], 0);
    let draft = &import["draft"];
    assert_eq!(draft["basics"]["name"], "Alex Demo");
    assert_eq!(draft["work"].as_array().unwrap().len(), 2);
    assert_eq!(draft["work"][0]["startDate"], "2021-03");
    assert_eq!(draft["education"][0]["institution"], "Sample University");
    assert_eq!(draft["skills"].as_array().unwrap().len(), 3);
    assert_eq!(draft["languages"].as_array().unwrap().len(), 2);

    // The profile is untouched until the draft is applied.
    let (status, view) = h.get("/api/v1/profile").await;
    assert_eq!(status, 200);
    assert_eq!(view["source"], Value::Null);
    assert_eq!(view["profile"]["work"], json!([]));
    assert_eq!(view["import_kinds"], json!(["linkedin_zip"]));
    assert_eq!(h.count("SELECT count(*) FROM profile.profiles").await, 0);

    // The draft can be read again by its owner, by nobody else.
    let uri = format!("/api/v1/profile/imports/{}", id_of(&import));
    let (status, again) = h.get(&uri).await;
    assert_eq!(status, 200);
    assert_eq!(again["draft"], import["draft"]);
    let someone_else = Harness::new(db, None);
    assert_eq!(someone_else.get(&uri).await.0, 404);
    assert_eq!(someone_else.post(&format!("{uri}/discard")).await.0, 404);
    let (status, _) = someone_else
        .json(
            "POST",
            &format!("{uri}/apply"),
            &json!({ "mode": "replace" }),
        )
        .await;
    assert_eq!(status, 404);

    // No model was called for it, and its expiry is queued.
    assert_eq!(h.count("SELECT count(*) FROM ai.usage").await, 0);
    let due: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT run_at FROM kernel.jobs WHERE kind = 'profile.expire_imports'")
            .fetch_one(&h.db)
            .await
            .unwrap();
    let expires = chrono::DateTime::parse_from_rfc3339(import["expires_at"].as_str().unwrap())
        .unwrap()
        .with_timezone(&Utc);
    assert!(
        (due - expires).num_milliseconds().abs() < 1,
        "{due} vs {expires}"
    );
    assert_eq!((expires - Utc::now()).num_hours(), 23, "a day to review it");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn replace_then_merge_keeps_hand_edits(db: PgPool) {
    let h = Harness::new(db, None);
    let first = import_zip(&h, &export_zip()).await;
    let apply = format!("/api/v1/profile/imports/{}/apply", id_of(&first));
    let (status, view) = h.json("POST", &apply, &json!({ "mode": "replace" })).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["source"], "linkedin_zip");
    assert_eq!(view["profile"]["work"][0]["name"], "Example Org");
    // Once applied, it cannot be applied or discarded again.
    let (status, problem) = h.json("POST", &apply, &json!({ "mode": "merge" })).await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (409, Some("import_not_ready"))
    );
    let discard = format!("/api/v1/profile/imports/{}/discard", id_of(&first));
    assert_eq!(h.post(&discard).await.0, 409);

    // Edit by hand: a new headline, a renamed title.
    let mut edited = view["profile"].clone();
    edited["basics"]["label"] = json!("Staff engineer, by hand");
    edited["work"][0]["position"] = json!("Principal Engineer");
    let (status, saved) = h.json("PUT", "/api/v1/profile", &edited).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(
        saved["source"], "linkedin_zip",
        "the source stays the last import's"
    );

    // A newer export, with one more position: merging adds only that.
    let newer = export_zip_with(&[(
        "Positions.csv",
        b"Company Name,Title,Description,Location,Started On,Finished On\n\
          Example Org,Senior Engineer,Runs the billing platform.,,Mar 2021,\n\
          Sample Labs,Engineer,Built the data pipeline.,,Jun 2017,Feb 2021\n\
          Demo Studio,Intern,,,Jan 2016,Dec 2016\n",
    )]);
    let second = import_zip(&h, &newer).await;
    let (status, merged) = h
        .json(
            "POST",
            &format!("/api/v1/profile/imports/{}/apply", id_of(&second)),
            &json!({ "mode": "merge" }),
        )
        .await;
    assert_eq!(status, 200, "{merged}");
    let profile = &merged["profile"];
    assert_eq!(profile["basics"]["label"], "Staff engineer, by hand");
    let titles: Vec<&str> = profile["work"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["position"].as_str().unwrap())
        .collect();
    // The edited position is kept as edited (same company, same start), and
    // only the new one is added.
    assert_eq!(titles, ["Principal Engineer", "Engineer", "Intern"]);
    assert_eq!(
        profile["skills"].as_array().unwrap().len(),
        3,
        "no duplicates"
    );

    // Every change announced, by id only.
    let events: Vec<Value> =
        sqlx::query_scalar("SELECT payload FROM kernel.events WHERE name = 'profile.updated'")
            .fetch_all(&h.db)
            .await
            .unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0], json!({ "account": h.account }));
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_discarded_draft_changes_nothing(db: PgPool) {
    let h = Harness::new(db, None);
    let import = import_zip(&h, &export_zip()).await;
    let uri = format!("/api/v1/profile/imports/{}", id_of(&import));
    assert_eq!(h.post(&format!("{uri}/discard")).await.0, 204);
    assert_eq!(
        h.post(&format!("{uri}/discard")).await.0,
        204,
        "twice is fine"
    );
    let (status, problem) = h
        .json(
            "POST",
            &format!("{uri}/apply"),
            &json!({ "mode": "replace" }),
        )
        .await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (409, Some("import_not_ready"))
    );
    assert_eq!(h.get(&uri).await.1["status"], "discarded");
    assert_eq!(h.count("SELECT count(*) FROM profile.profiles").await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_bad_mode_is_a_problem(db: PgPool) {
    let h = Harness::new(db, None);
    let import = import_zip(&h, &export_zip()).await;
    let (status, problem) = h
        .json(
            "POST",
            &format!("/api/v1/profile/imports/{}/apply", id_of(&import)),
            &json!({ "mode": "overwrite" }),
        )
        .await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (400, Some("invalid_request"))
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn uploads_that_are_not_an_export_are_refused(db: PgPool) {
    let h = Harness::new(db, None);
    for (parts, code) in [
        (
            vec![Part::File("file", "x.zip", &b"PK"[..])],
            "invalid_kind",
        ),
        (
            vec![
                Part::Text("kind", "linkedin_zip"),
                Part::File("file", "cv.pdf", &b"%PDF-1.7"[..]),
            ],
            "not_a_zip",
        ),
        (vec![Part::Text("kind", "linkedin_zip")], "invalid_upload"),
        (vec![Part::Text("kind", "spreadsheet")], "invalid_kind"),
    ] {
        let (status, problem) = h.upload(&parts).await;
        assert_eq!(
            (status.as_u16(), problem["code"].as_str()),
            (400, Some(code))
        );
    }
    // A ZIP with none of LinkedIn's files.
    let empty = super::testkit::zip_of(&[("notes.txt", b"hi")], zip::CompressionMethod::Deflated);
    let (status, problem) = h
        .upload(&[
            Part::Text("kind", "linkedin_zip"),
            Part::File("file", "x.zip", &empty),
        ])
        .await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (400, Some("nothing_found"))
    );
    // A hostile archive is refused over HTTP too, and leaves nothing behind.
    let slip = super::testkit::zip_of(
        &[("../Positions.csv", b"Title\nx\n")],
        zip::CompressionMethod::Deflated,
    );
    let (status, problem) = h
        .upload(&[
            Part::Text("kind", "linkedin_zip"),
            Part::File("file", "x.zip", &slip),
        ])
        .await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (400, Some("archive_unsafe_path"))
    );
    assert_eq!(h.count("SELECT count(*) FROM profile.imports").await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn without_a_model_pdfs_and_text_are_unavailable(db: PgPool) {
    let h = Harness::new(db, None);
    let pdf = pdf_saying(&CV_LINES);
    for parts in [
        vec![
            Part::Text("kind", "pdf"),
            Part::File("file", "cv.pdf", &pdf),
        ],
        vec![
            Part::Text("kind", "text"),
            Part::Text("text", "Alex Demo, engineer"),
        ],
    ] {
        let (status, problem) = h.upload(&parts).await;
        assert_eq!(
            (status.as_u16(), problem["code"].as_str()),
            (503, Some("ai_unavailable"))
        );
    }
    assert_eq!(h.count("SELECT count(*) FROM profile.imports").await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_cv_pdf_is_structured_and_what_it_does_not_say_is_left_out(db: PgPool) {
    let model = fake_model(&model_answer()).await;
    let h = Harness::new(db, Some(gateway(&model, None)));
    assert_eq!(
        h.get("/api/v1/profile").await.1["import_kinds"],
        json!(["linkedin_zip", "pdf", "text"])
    );

    let pdf = pdf_saying(&CV_LINES);
    let (status, import) = h
        .upload(&[
            Part::Text("kind", "pdf"),
            Part::File("file", "cv.pdf", &pdf),
        ])
        .await;
    assert_eq!(status, 201, "{import}");
    let draft = &import["draft"];
    let companies: Vec<&str> = draft["work"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        companies,
        ["Example Org", "Sample Labs"],
        "Invented Corp is not in the CV"
    );
    assert_eq!(draft["work"][1]["endDate"], "2021-02");
    assert_eq!(draft["education"][0]["studyType"], "Master's degree");
    let skills: Vec<&str> = draft["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(skills, ["Rust", "PostgreSQL"], "nor is Kubernetes");
    assert_eq!(import["dropped"], 2);

    // One call, with the CV as data and the profile's schema; counted.
    let requests = model.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let sent: Value = serde_json::from_slice(&requests[0].body).unwrap();
    let document = sent["messages"][1]["content"].as_str().unwrap();
    assert!(document.starts_with("<document>"), "{document}");
    assert!(document.contains("billing platform"), "{document}");
    assert!(
        sent["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("data, not instructions")
    );
    assert_eq!(sent["response_format"]["json_schema"]["name"], "profile");
    assert_eq!(
        usage::used_today(&h.db, h.account, Utc::now())
            .await
            .unwrap(),
        1_200
    );
    // Still nothing in the profile.
    assert_eq!(h.count("SELECT count(*) FROM profile.profiles").await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn pasted_text_is_structured_too(db: PgPool) {
    let model = fake_model(&model_answer()).await;
    let h = Harness::new(db, Some(gateway(&model, None)));
    let text = CV_LINES.join("\n");
    let (status, import) = h
        .upload(&[Part::Text("kind", "text"), Part::Text("text", &text)])
        .await;
    assert_eq!(status, 201, "{import}");
    assert_eq!(import["kind"], "text");
    assert_eq!(import["draft"]["work"].as_array().unwrap().len(), 2);
    assert_eq!(import["dropped"], 2);

    let (status, problem) = h
        .upload(&[Part::Text("kind", "text"), Part::Text("text", "   ")])
        .await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (400, Some("invalid_upload"))
    );
    let long = "a".repeat(super::MAX_TEXT_CHARS + 1);
    let (status, problem) = h
        .upload(&[Part::Text("kind", "text"), Part::Text("text", &long)])
        .await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (400, Some("text_too_long"))
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_scanned_pdf_fails_and_creates_nothing(db: PgPool) {
    let model = fake_model(&model_answer()).await;
    let h = Harness::new(db, Some(gateway(&model, None)));
    let scan = pdf_saying(&[]);
    let (status, problem) = h
        .upload(&[
            Part::Text("kind", "pdf"),
            Part::File("file", "scan.pdf", &scan),
        ])
        .await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (400, Some("pdf_no_text"))
    );
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("no text we can read"),
        "{problem}"
    );
    assert_eq!(h.count("SELECT count(*) FROM profile.imports").await, 0);
    assert!(model.received_requests().await.unwrap().is_empty());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_unusable_answer_is_a_problem(db: PgPool) {
    let model = fake_model(&json!({ "basics": { "name": "Alex" }, "hobbies": ["chess"] })).await;
    let h = Harness::new(db, Some(gateway(&model, None)));
    let (status, problem) = h
        .upload(&[
            Part::Text("kind", "text"),
            Part::Text("text", "Alex, chess"),
        ])
        .await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (503, Some("ai_failed"))
    );
    assert_eq!(h.count("SELECT count(*) FROM profile.imports").await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn over_the_daily_limit_the_model_is_not_called(db: PgPool) {
    let model = fake_model(&model_answer()).await;
    let h = Harness::new(db, Some(gateway(&model, Some(1_000))));
    usage::record(
        &h.db,
        usage::Entry {
            account: h.account,
            role: applicamp_ai::Role::Complete,
            model: "test/model",
            tokens_in: 900,
            tokens_out: 200,
            at: Utc::now(),
        },
    )
    .await
    .unwrap();
    let (status, problem) = h
        .upload(&[
            Part::Text("kind", "text"),
            Part::Text("text", &CV_LINES.join("\n")),
        ])
        .await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (503, Some("ai_daily_limit_reached"))
    );
    assert!(model.received_requests().await.unwrap().is_empty());
    // Someone else still has their day.
    let other = Harness::as_account(
        h.db.clone(),
        Some(gateway(&model, Some(1_000))),
        Uuid::now_v7(),
    );
    let (status, _) = other
        .upload(&[
            Part::Text("kind", "text"),
            Part::Text("text", &CV_LINES.join("\n")),
        ])
        .await;
    assert_eq!(status, 201);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn drafts_expire(db: PgPool) {
    let h = Harness::new(db, None);
    let import = import_zip(&h, &export_zip()).await;
    let uri = format!("/api/v1/profile/imports/{}", id_of(&import));
    // Not before its time.
    h.work().await;
    assert_eq!(h.count("SELECT count(*) FROM profile.imports").await, 1);

    sqlx::query("UPDATE profile.imports SET expires_at = now() - interval '1 minute'")
        .execute(&h.db)
        .await
        .unwrap();
    assert_eq!(h.get(&uri).await.0, 404, "an expired draft is gone");
    sqlx::query("UPDATE kernel.jobs SET run_at = now() WHERE kind = 'profile.expire_imports'")
        .execute(&h.db)
        .await
        .unwrap();
    h.work().await;
    assert_eq!(h.count("SELECT count(*) FROM profile.imports").await, 0);
}
