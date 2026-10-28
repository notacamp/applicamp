//! Profile completion over HTTP, against a fake model that tells the gap
//! review and the draft apart by their instructions. All data is invented.

use applicamp_ai::usage;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

use crate::testkit::{Harness, gateway};

/// A profile of three positions without results, a skill and no headline.
fn profile() -> Value {
    json!({
        "basics": { "name": "Alex Demo" },
        "work": [
            { "name": "Example Org", "position": "Senior Engineer", "startDate": "2021-03",
              "summary": "Runs the billing platform in Rust on PostgreSQL." },
            { "name": "Sample Labs", "position": "Engineer", "startDate": "2017-06",
              "endDate": "2021-02",
              "summary": "Built the data pipeline with Kafka and Rust." },
            { "name": "Third Co", "position": "Developer", "startDate": "2014-01",
              "endDate": "2017-05", "summary": "Wrote internal tools." },
        ],
        "skills": [{ "name": "PostgreSQL" }],
    })
}

fn review_answer() -> Value {
    json!({
        "questions": [
            { "section": "basics.label", "role_ref": null,
              "question": "In one line, what do you want to be known for?",
              "why": "There is no headline yet." },
            { "section": "work", "role_ref": 0,
              "question": "What did you ship at Example Org that you are proud of?",
              "why": "Your latest role has no results yet." },
            { "section": "work", "role_ref": 7,
              "question": "A position that does not exist", "why": "dropped" },
        ],
        "skills_in_roles": [
            { "name": "Rust", "roles": [0, 1] },
            { "name": "Kafka", "roles": [1] },
            { "name": "Kubernetes", "roles": [0] },
            { "name": "PostgreSQL", "roles": [0] },
        ],
    })
}

fn reply(content: &Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "model": "test/model",
        "choices": [{
            "message": { "role": "assistant", "content": content.to_string() },
            "finish_reason": "stop",
        }],
        "usage": { "prompt_tokens": 900, "completion_tokens": 300 },
    }))
}

/// A fake model: the review answers `review`, a draft answers `draft`.
async fn model(review: &Value, draft: &Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("You review a person"))
        .respond_with(reply(review))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("You turn a person"))
        .respond_with(reply(draft))
        .mount(&server)
        .await;
    server
}

async fn calls(server: &MockServer, needle: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| String::from_utf8_lossy(&r.body).contains(needle))
        .count()
}

async fn saved(h: &Harness, body: &Value) {
    let (status, saved) = h.json("PUT", "/api/v1/profile", body).await;
    assert_eq!(status, 200, "{saved}");
}

/// The questions once the review has run.
async fn ready_questions(h: &Harness) -> Value {
    let (status, running) = h.get("/api/v1/profile/questions").await;
    assert_eq!(status, 202, "{running}");
    assert_eq!(running["status"], "running");
    h.work().await;
    let (status, ready) = h.get("/api/v1/profile/questions").await;
    assert_eq!(status, 200, "{ready}");
    assert_eq!(ready["status"], "ready", "{ready}");
    ready
}

#[sqlx::test(migrations = "../../../migrations")]
async fn one_review_per_version_and_only_valid_questions_kept(db: PgPool) {
    let server = model(&review_answer(), &json!({})).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;

    let ready = ready_questions(&h).await;
    let questions = ready["questions"].as_array().unwrap();
    // The invented position is dropped; the work question is worth more
    // (8: a result on one of the three recent roles) than the headline (6).
    let got: Vec<(&str, u64)> = questions
        .iter()
        .map(|q| {
            (
                q["section"].as_str().unwrap(),
                q["expected_gain"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(got, [("work", 8), ("basics.label", 6)]);
    assert_eq!(questions[0]["context"]["company"], "Example Org");
    assert_eq!(ready["total"], 2);
    assert_eq!(ready["answered"], 0);

    // Asking again, or asking for the skills, is not another call.
    h.get("/api/v1/profile/questions").await;
    h.get("/api/v1/profile/skills/suggested").await;
    h.work().await;
    assert_eq!(calls(&server, "You review a person").await, 1);

    // The model saw no name.
    let seen = server.received_requests().await.unwrap();
    let body = String::from_utf8_lossy(&seen[0].body);
    assert!(!body.contains("Alex Demo"));
    assert!(body.contains("<profile>"));

    // A change by hand is a new version: one more call.
    saved(&h, &profile()).await;
    let mut edited = profile();
    edited["basics"]["label"] = json!("Backend engineer");
    saved(&h, &edited).await;
    h.get("/api/v1/profile/questions").await;
    h.work().await;
    assert_eq!(calls(&server, "You review a person").await, 2);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_answer_becomes_a_draft_and_keeping_it_writes_the_role(db: PgPool) {
    let draft = json!({
        "items": [
            "Rebuilt the billing pipeline in Rust, cutting invoice runs from 6 hours to 40 minutes",
            "Won the Acme Innovation Award",
            "Led a team of 4 through the migration",
        ],
        "education": null,
    });
    let server = model(&review_answer(), &draft).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let ready = ready_questions(&h).await;
    let question = ready["questions"][0]["id"].as_str().unwrap().to_owned();
    let before = ready["score"].as_u64().unwrap();

    let (status, made) = h
        .json(
            "POST",
            &format!("/api/v1/profile/questions/{question}/draft"),
            &json!({ "answer": "Rebuilt billing in Rust; runs went from 6 hours to 40 minutes. \
                                A team of 4." }),
        )
        .await;
    assert_eq!(status, 201, "{made}");
    assert_eq!(made["kind"], "highlights");
    // The award names an Acme nobody wrote: left out, and counted.
    assert_eq!(made["dropped"], 1);
    assert_eq!(made["items"].as_array().unwrap().len(), 2);
    assert!(
        !made["items"].to_string().contains("Acme"),
        "an invented award never reaches the person"
    );
    // Nothing is saved yet.
    let (_, view) = h.get("/api/v1/profile").await;
    assert_eq!(view["profile"]["work"][0]["highlights"], json!([]));

    // Edited lines are the person's own words.
    let id = made["id"].as_str().unwrap();
    let (status, kept) = h
        .json(
            "POST",
            &format!("/api/v1/profile/drafts/{id}/keep"),
            &json!({ "items": [
                "Rebuilt the billing pipeline in Rust, cutting invoice runs from 6 hours to 40 minutes",
                "Mentored two engineers",
            ] }),
        )
        .await;
    assert_eq!(status, 200, "{kept}");
    assert_eq!(kept["source"], "answer");
    assert_eq!(
        kept["profile"]["work"][0]["highlights"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        h.count("SELECT version::bigint FROM profile.profiles")
            .await,
        2
    );

    // The strength rose, the question is done and the next one is open, with
    // no new review.
    let (_, strength) = h.get("/api/v1/profile/strength").await;
    assert!(strength["score"].as_u64().unwrap() > before, "{strength}");
    let (status, after) = h.get("/api/v1/profile/questions").await;
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["answered"], 1);
    assert_eq!(after["questions"].as_array().unwrap().len(), 1);
    assert_eq!(after["questions"][0]["section"], "basics.label");
    assert_eq!(calls(&server, "You review a person").await, 1);

    // A kept draft cannot be kept or discarded again.
    let (status, again) = h
        .json(
            "POST",
            &format!("/api/v1/profile/drafts/{id}/keep"),
            &json!({}),
        )
        .await;
    assert_eq!(
        (status.as_u16(), again["code"].as_str()),
        (409, Some("draft_not_ready"))
    );
    let (status, _) = h
        .post(&format!("/api/v1/profile/drafts/{id}/discard"))
        .await;
    assert_eq!(status, 409);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_draft_for_a_profile_that_changed_is_refused(db: PgPool) {
    let draft = json!({ "items": ["Backend engineer"], "education": null });
    let server = model(&review_answer(), &draft).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let ready = ready_questions(&h).await;
    let label = ready["questions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["section"] == "basics.label")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, made) = h
        .json(
            "POST",
            &format!("/api/v1/profile/questions/{label}/draft"),
            &json!({ "answer": "A backend engineer." }),
        )
        .await;
    assert_eq!(status, 201, "{made}");
    assert_eq!(made["dropped"], 0);

    let mut edited = profile();
    edited["skills"] = json!([{ "name": "Go" }]);
    saved(&h, &edited).await;
    let id = made["id"].as_str().unwrap();
    let (status, problem) = h
        .json(
            "POST",
            &format!("/api/v1/profile/drafts/{id}/keep"),
            &json!({}),
        )
        .await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (409, Some("profile_changed"))
    );

    // Discarding is fine, twice.
    for _ in 0..2 {
        let (status, _) = h
            .post(&format!("/api/v1/profile/drafts/{id}/discard"))
            .await;
        assert_eq!(status, 204);
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_headline_and_a_school_are_kept_into_their_places(db: PgPool) {
    let review = json!({
        "questions": [
            { "section": "basics.label", "role_ref": null,
              "question": "What do you want to be known for?", "why": "No headline." },
            { "section": "education", "role_ref": null,
              "question": "Where did you study?", "why": "No education yet." },
            { "section": "links", "role_ref": null,
              "question": "Is there a GitHub or a site to show?", "why": "No links." },
        ],
        "skills_in_roles": [],
    });
    let draft = json!({
        "items": ["Backend engineer · Rust, PostgreSQL"],
        "education": null,
    });
    let school = json!({
        "items": [],
        "education": { "institution": "Sample University", "studyType": "Master's degree",
                       "area": "Computer science", "startDate": "2012", "endDate": "2019" },
    });
    let links = json!({ "items": ["https://github.com/alex-demo", "https://evil.example.com/x"],
                        "education": null });
    // One fake model answering a draft by what the question asked.
    let server = MockServer::start().await;
    for (needle, answer) in [
        ("You review a person", &review),
        ("Where did you study", &school),
        ("GitHub or a site", &links),
        ("known for", &draft),
    ] {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains(needle))
            .respond_with(reply(answer))
            .mount(&server)
            .await;
    }
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let ready = ready_questions(&h).await;
    let id_of = |section: &str| {
        ready["questions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["section"] == section)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };

    for (section, answer) in [
        (
            "basics.label",
            "A backend engineer, mostly Rust and PostgreSQL.",
        ),
        (
            "education",
            "Master's degree in Computer science at Sample University, 2012 to 2019.",
        ),
        ("links", "My GitHub is github.com/alex-demo"),
    ] {
        let (status, made) = h
            .json(
                "POST",
                &format!("/api/v1/profile/questions/{}/draft", id_of(section)),
                &json!({ "answer": answer }),
            )
            .await;
        assert_eq!(status, 201, "{section}: {made}");
        let id = made["id"].as_str().unwrap();
        let (status, kept) = h
            .json(
                "POST",
                &format!("/api/v1/profile/drafts/{id}/keep"),
                &json!({}),
            )
            .await;
        assert_eq!(status, 200, "{section}: {kept}");
        if section == "links" {
            assert_eq!(
                made["dropped"], 1,
                "the other address is not theirs: {made}"
            );
        }
    }
    let (_, view) = h.get("/api/v1/profile").await;
    let p = &view["profile"];
    assert_eq!(p["basics"]["label"], "Backend engineer · Rust, PostgreSQL");
    assert_eq!(p["education"][0]["institution"], "Sample University");
    assert_eq!(p["education"][0]["startDate"], "2012");
    assert_eq!(p["basics"]["profiles"].as_array().unwrap().len(), 1);
    assert_eq!(p["basics"]["profiles"][0]["network"], "GitHub");
    assert_eq!(calls(&server, "You review a person").await, 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn skills_found_in_the_roles_can_be_added_dismissed_and_reordered(db: PgPool) {
    let server = model(&review_answer(), &json!({})).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let (status, running) = h.get("/api/v1/profile/skills/suggested").await;
    assert_eq!(
        (status.as_u16(), running["status"].as_str()),
        (202, Some("running"))
    );
    h.work().await;

    let (status, suggested) = h.get("/api/v1/profile/skills/suggested").await;
    assert_eq!(status, 200, "{suggested}");
    // Kubernetes is in no role; PostgreSQL is already listed; Kafka is in one.
    let names: Vec<(&str, u64)> = suggested["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["name"].as_str().unwrap(), s["roles"].as_u64().unwrap()))
        .collect();
    assert_eq!(names, [("Rust", 2), ("Kafka", 1)]);

    let (status, left) = h.post("/api/v1/profile/skills/suggested/Rust/add").await;
    assert_eq!(status, 200, "{left}");
    assert_eq!(left["skills"].as_array().unwrap().len(), 1);
    let (status, left) = h
        .post("/api/v1/profile/skills/suggested/Kafka/dismiss")
        .await;
    assert_eq!(status, 200, "{left}");
    assert!(left["skills"].as_array().unwrap().is_empty());
    let (status, _) = h.post("/api/v1/profile/skills/suggested/Kafka/add").await;
    assert_eq!(status, 404, "a dismissed skill is no suggestion");
    let (status, _) = h.post("/api/v1/profile/skills/suggested/Haskell/add").await;
    assert_eq!(status, 404);

    // Added skills follow in the order added, and the order can change.
    let (_, view) = h.get("/api/v1/profile").await;
    assert_eq!(
        view["profile"]["skills"],
        json!([{ "name": "PostgreSQL" }, { "name": "Rust" }])
    );
    let (status, ordered) = h
        .json(
            "PUT",
            "/api/v1/profile/skills/order",
            &json!({ "names": ["Rust", "PostgreSQL"] }),
        )
        .await;
    assert_eq!(status, 200, "{ordered}");
    assert_eq!(ordered["profile"]["skills"][0]["name"], "Rust");
    for bad in [
        json!(["Rust"]),
        json!(["Rust", "Rust"]),
        json!(["Rust", "Go"]),
    ] {
        let (status, problem) = h
            .json(
                "PUT",
                "/api/v1/profile/skills/order",
                &json!({ "names": bad }),
            )
            .await;
        assert_eq!(
            (status.as_u16(), problem["code"].as_str()),
            (400, Some("invalid_order")),
            "{bad}"
        );
    }
    // None of this was a new review.
    h.get("/api/v1/profile/questions").await;
    h.work().await;
    assert_eq!(calls(&server, "You review a person").await, 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn skipping_a_question_does_not_ask_it_again(db: PgPool) {
    let server = model(&review_answer(), &json!({})).await;
    let h = Harness::new(db, Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let ready = ready_questions(&h).await;
    let id = ready["questions"][0]["id"].as_str().unwrap();
    let (status, _) = h
        .post(&format!("/api/v1/profile/questions/{id}/skip"))
        .await;
    assert_eq!(status, 204);
    let (_, after) = h.get("/api/v1/profile/questions").await;
    assert_eq!(after["answered"], 1);
    assert_eq!(after["questions"].as_array().unwrap().len(), 1);
    let (status, _) = h.post("/api/v1/profile/questions/nope/skip").await;
    assert_eq!(status, 404);
    // A skipped question cannot be drafted.
    let (status, _) = h
        .json(
            "POST",
            &format!("/api/v1/profile/questions/{id}/draft"),
            &json!({ "answer": "x" }),
        )
        .await;
    assert_eq!(status, 404);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_strength_names_what_to_do_and_needs_no_model(db: PgPool) {
    let h = Harness::new(db, None);
    let (_, empty) = h.get("/api/v1/profile/strength").await;
    assert_eq!(empty["score"], 0);
    assert_eq!(empty["strong"], false);

    saved(&h, &profile()).await;
    let (_, strength) = h.get("/api/v1/profile/strength").await;
    let items = strength["items"].as_array().unwrap();
    let action = |key: &str| {
        items.iter().find(|i| i["key"] == key).unwrap()["action"]
            .as_str()
            .map(str::to_owned)
    };
    // No review yet: the checklist still works.
    assert_eq!(action("headline").as_deref(), Some("edit:basics"));
    assert_eq!(action("skills").as_deref(), Some("skills"));
    assert_eq!(action("positions"), None, "three dated positions are full");
    assert_eq!(action("goals").as_deref(), Some("goals"));
    assert_eq!(strength["questions_left"], Value::Null);

    // Without a model the questions say so.
    let (status, none) = h.get("/api/v1/profile/questions").await;
    assert_eq!(status, 200);
    assert_eq!(none["status"], "unavailable");
    assert_eq!(none["reason"], "ai_unavailable");
    assert_eq!(h.count("SELECT count(*) FROM profile.gap_reviews").await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn over_the_limit_the_review_fails_once_and_is_not_retried_by_asking(db: PgPool) {
    let server = model(&review_answer(), &json!({})).await;
    let h = Harness::new(db, Some(gateway(&server, Some(2_000))));
    usage::record(
        &h.db,
        usage::Entry {
            account: h.account,
            role: applicamp_ai::Role::Complete,
            model: "test/model",
            tokens_in: 1_900,
            tokens_out: 200,
            at: Utc::now(),
        },
    )
    .await
    .unwrap();
    saved(&h, &profile()).await;
    h.get("/api/v1/profile/questions").await;
    h.work().await;
    for _ in 0..2 {
        let (status, failed) = h.get("/api/v1/profile/questions").await;
        assert_eq!(status, 200, "{failed}");
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["reason"], "ai_daily_limit_reached");
    }
    h.work().await;
    assert!(server.received_requests().await.unwrap().is_empty());

    // The draft says so too, with its own problem.
    let (status, problem) = h
        .json(
            "POST",
            "/api/v1/profile/questions/q1/draft",
            &json!({ "answer": "x" }),
        )
        .await;
    assert_eq!(status, 404, "no ready review, no question: {problem}");

    // Asking to try again starts it again; the limit holds, still no call.
    let (status, _) = h.post("/api/v1/profile/questions/review").await;
    assert_eq!(status, 202);
    h.work().await;
    let (_, again) = h.get("/api/v1/profile/questions").await;
    assert_eq!(again["status"], "failed");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[sqlx::test(migrations = "../../../migrations")]
async fn answers_are_validated_and_belong_to_one_person(db: PgPool) {
    let server = model(
        &review_answer(),
        &json!({ "items": ["x"], "education": null }),
    )
    .await;
    let h = Harness::new(db.clone(), Some(gateway(&server, None)));
    saved(&h, &profile()).await;
    let ready = ready_questions(&h).await;
    let id = ready["questions"][0]["id"].as_str().unwrap();
    let uri = format!("/api/v1/profile/questions/{id}/draft");
    for answer in ["", "   ", &"x".repeat(2_001)] {
        let (status, problem) = h.json("POST", &uri, &json!({ "answer": answer })).await;
        assert_eq!(
            (status.as_u16(), problem["code"].as_str()),
            (400, Some("invalid_answer"))
        );
    }
    // Somebody else sees none of it.
    let other = Harness::new(db, Some(gateway(&server, None)));
    let (status, _) = other
        .json("POST", &uri, &json!({ "answer": "Rebuilt billing" }))
        .await;
    assert_eq!(status, 404);
    let (_, theirs) = other.get("/api/v1/profile/questions").await;
    assert_eq!(theirs["status"], "unavailable");
}
