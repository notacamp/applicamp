//! Complete your profile over HTTP: facts proposed from Tell me about you, an
//! import and the backfill, then kept, dropped, kept all and undone.

use chrono::{TimeDelta, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    features::{
        about::judge::{Change, Field, Sentence, SentenceKind},
        grill::role_key,
        imports::testkit::export_zip,
    },
    schema::{Area, Work},
    testkit::{Harness, Part},
};

/// An invented profile: a lead role with one fact kept, an earlier role.
fn profile() -> Value {
    json!({
        "basics": { "name": "Alex Demo", "profiles": [] },
        "work": [
            {
                "name": "Example Co", "position": "Lead Engineer", "startDate": "2021-03",
                "summary": "Runs the billing platform.", "highlights": [],
                "facts": [{ "id": "f1", "text": "Moved invoicing onto PostgreSQL",
                    "quote": "Moved invoicing onto PostgreSQL", "area": "results",
                    "kept_at": "2026-10-30T09:00:00Z" }],
            },
            {
                "name": "Sample Labs", "position": "Engineer", "startDate": "2017-06",
                "endDate": "2021-02", "summary": "Built the data pipeline.", "highlights": [],
            },
        ],
        "skills": [{ "name": "PostgreSQL" }],
    })
}

fn key(company: &str, title: &str, start: &str) -> String {
    role_key(&Work {
        name: Some(company.into()),
        position: Some(title.into()),
        start_date: Some(start.into()),
        ..Work::default()
    })
}

fn sentence(
    id: &str,
    kind: SentenceKind,
    text: &str,
    claim: Option<&str>,
    sources: &[&str],
) -> Sentence {
    let assumed = kind == SentenceKind::Assumed;
    Sentence {
        id: id.into(),
        text: text.into(),
        kind,
        sources: sources.iter().map(|s| (*s).to_owned()).collect(),
        why: assumed.then(|| "Lead titles usually mean a small team.".to_owned()),
        claim: claim.map(str::to_owned),
        downgraded: false,
        topic: Area::People,
        position: assumed.then_some(0),
        role_key: assumed.then(|| key("Example Co", "Lead Engineer", "2021-03")),
        urls: if sources.iter().any(|s| s.starts_with("site")) {
            vec!["https://alex.example.com/projects".into()]
        } else {
            Vec::new()
        },
        new: false,
    }
}

fn change(field: Field, value: &str, quote: &str, on_lead: bool) -> Change {
    Change {
        id: format!("c-{value}"),
        field,
        value: value.into(),
        position: on_lead.then_some(0),
        role_key: on_lead.then(|| key("Example Co", "Lead Engineer", "2021-03")),
        area: Area::People,
        quote: quote.into(),
        sentence: value.into(),
        flags: Vec::new(),
        resolution: None,
        origin: None,
    }
}

/// Tell me about you as the person left it: a portrait with an assumption
/// marked right, one left unanswered, one from their website and a sentence
/// that adds a tool; what they wrote; and "Is this accurate?" not answered.
async fn seed_about(h: &Harness) {
    let version: i32 =
        sqlx::query_scalar("SELECT version FROM profile.profiles WHERE account_id = $1")
            .bind(h.account)
            .fetch_one(&h.db)
            .await
            .unwrap();
    let portrait = vec![
        sentence(
            "s1",
            SentenceKind::Assumed,
            "I assume you led a team of 3 to 6 engineers",
            Some("Led a team of 3 to 6 engineers"),
            &[],
        ),
        // No claim of its own: the sentence without "I assume".
        sentence(
            "s2",
            SentenceKind::Assumed,
            "I assume you ran the on-call rota",
            None,
            &[],
        ),
        sentence(
            "s3",
            SentenceKind::Assumed,
            "I assume, from your website, that you built an open-source billing toolkit",
            Some("Built an open-source billing toolkit"),
            &["site0"],
        ),
        sentence(
            "s4",
            SentenceKind::Known,
            "At Example Co you run the billing platform on Kafka.",
            None,
            &["w0", "p.summary"],
        ),
    ];
    let changes = vec![
        change(
            Field::RoleFact,
            "Led 4 engineers",
            "I led 4 engineers",
            true,
        ),
        change(
            Field::Skill,
            "Elixir",
            "I use Rails, Elixir and Postgres",
            false,
        ),
        change(
            Field::Background,
            "Ran a restaurant for 6 years",
            "I ran a restaurant for 6 years",
            false,
        ),
    ];
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO profile.about_you (account_id, profile_version, portrait, choices, changes,
            extra, created_at, updated_at, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8)",
    )
    .bind(h.account)
    .bind(version)
    .bind(sqlx::types::Json(&portrait))
    .bind(json!({ "s1": "right" }))
    .bind(sqlx::types::Json(&changes))
    .bind("I led 4 engineers. I use Rails, Elixir and Postgres. I ran a restaurant for 6 years.")
    .bind(now)
    .bind(now + TimeDelta::days(7))
    .execute(&h.db)
    .await
    .unwrap();
}

const PROPOSALS: &str = "/api/v1/profile/facts/proposals";

fn texts(view: &Value) -> Vec<String> {
    view["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["proposals"].as_array().unwrap().iter())
        .map(|p| p["text"].as_str().unwrap().to_owned())
        .collect()
}

fn find<'a>(view: &'a Value, text: &str) -> &'a Value {
    view["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["proposals"].as_array().unwrap().iter())
        .find(|p| p["text"] == text)
        .unwrap_or_else(|| panic!("{text} in {view}"))
}

async fn version(h: &Harness) -> i32 {
    sqlx::query_scalar("SELECT version FROM profile.profiles WHERE account_id = $1")
        .bind(h.account)
        .fetch_one(&h.db)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn what_tell_me_about_you_learned_is_proposed_then_kept_dropped_and_undone(db: PgPool) {
    let h = Harness::new(db.clone(), None);
    assert_eq!(h.json("PUT", "/api/v1/profile", &profile()).await.0, 200);
    seed_about(&h).await;

    // Only the assumption marked right, and what they wrote, until they say
    // the portrait is accurate.
    let (status, _) = h
        .json(
            "PUT",
            "/api/v1/profile/about/sentences/s1",
            &json!({ "choice": "right" }),
        )
        .await;
    assert_eq!(status, 200);
    h.work().await;
    let (status, view) = h.get(PROPOSALS).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(
        texts(&view),
        [
            "Led a team of 3 to 6 engineers",
            "Led 4 engineers",
            "Elixir",
            "Ran a restaurant for 6 years"
        ]
    );
    assert_eq!(view["open"], 4);

    // "Yes, mostly": the rest of what the portrait said.
    let (status, _) = h
        .json(
            "PUT",
            "/api/v1/profile/about/accuracy",
            &json!({ "accuracy": "yes" }),
        )
        .await;
    assert_eq!(status, 200);
    h.work().await;
    let (_, view) = h.get(PROPOSALS).await;
    assert_eq!(view["open"], 7, "{view}");
    let groups = view["groups"].as_array().unwrap();
    assert_eq!(groups[0]["target"], "position");
    assert_eq!(groups[0]["company"], "Example Co");
    assert_eq!(groups[0]["position"], 0);
    assert_eq!(groups[1]["target"], "skills");
    assert_eq!(groups[2]["target"], "background");
    let site = find(&view, "Built an open-source billing toolkit");
    assert_eq!(site["source"], "your_website");
    assert_eq!(site["basis"], "https://alex.example.com/projects");
    let kafka = find(
        &view,
        "At Example Co you run the billing platform on Kafka.",
    );
    assert_eq!(kafka["source"], "you_confirmed");
    let team = find(&view, "Led a team of 3 to 6 engineers");
    assert_eq!(team["source"], "you_confirmed");
    assert_eq!(team["basis"], "Lead titles usually mean a small team.");
    assert_eq!(find(&view, "Led 4 engineers")["quote"], "I led 4 engineers");

    // Keep one: it is on the position, as a confirmed assumption with its
    // reason, in one new profile version.
    let before = version(&h).await;
    let id = team["id"].as_str().unwrap().to_owned();
    let (status, view) = h.post(&format!("{PROPOSALS}/{id}/keep")).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(
        find(&view, "Led a team of 3 to 6 engineers")["state"],
        "kept"
    );
    assert!(find(&view, "Led a team of 3 to 6 engineers")["undo_until"].is_string());
    assert_eq!(version(&h).await, before + 1);
    let (_, saved) = h.get("/api/v1/profile").await;
    let facts = saved["profile"]["work"][0]["facts"].as_array().unwrap();
    let kept = facts
        .iter()
        .find(|f| f["text"] == "Led a team of 3 to 6 engineers")
        .unwrap();
    assert_eq!(kept["status"], "confirmed_assumption");
    assert_eq!(kept["basis"], "Lead titles usually mean a small team.");
    // Keeping twice is refused.
    let (status, problem) = h.post(&format!("{PROPOSALS}/{id}/keep")).await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (409, Some("proposal_not_open"))
    );

    // Drop one; it stays on the page, dropped.
    let elixir = find(&view, "Elixir")["id"].as_str().unwrap().to_owned();
    let (status, view) = h.post(&format!("{PROPOSALS}/{elixir}/drop")).await;
    assert_eq!(status, 200);
    assert_eq!(find(&view, "Elixir")["state"], "dropped");
    assert_eq!(view["dropped"], 1);

    // Undo both: the fact leaves the position again, Elixir is open again.
    let (status, view) = h.post(&format!("{PROPOSALS}/{id}/undo")).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(
        find(&view, "Led a team of 3 to 6 engineers")["state"],
        "open"
    );
    let (_, saved) = h.get("/api/v1/profile").await;
    assert!(
        !saved["profile"]["work"][0]["facts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["text"] == "Led a team of 3 to 6 engineers")
    );
    let (_, view) = h.post(&format!("{PROPOSALS}/{elixir}/undo")).await;
    assert_eq!(find(&view, "Elixir")["state"], "open");
    let (status, problem) = h.post(&format!("{PROPOSALS}/{elixir}/undo")).await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (409, Some("proposal_not_decided"))
    );

    // Someone else sees none of it and decides nothing.
    let other = Harness::new(db, None);
    assert_eq!(other.post(&format!("{PROPOSALS}/{id}/keep")).await.0, 404);
    assert_eq!(other.get(PROPOSALS).await.1["open"], 0);

    // Keep all: every one, in one new profile version.
    let before = version(&h).await;
    let (status, view) = h.post(&format!("{PROPOSALS}/keep-all")).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(
        (view["open"].as_u64(), view["kept"].as_u64()),
        (Some(0), Some(7))
    );
    assert_eq!(view["batch"]["kept"], 7, "one Undo for all of them");
    assert_eq!(version(&h).await, before + 1);
    let (_, saved) = h.get("/api/v1/profile").await;
    let profile = &saved["profile"];
    let skills: Vec<&str> = profile["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(skills, ["PostgreSQL", "Elixir"]);
    assert_eq!(
        profile["basics"]["background"],
        "Ran a restaurant for 6 years"
    );
    let told = profile["work"][0]["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["text"] == "Led 4 engineers")
        .unwrap();
    assert_eq!(told["quote"], "I led 4 engineers");
    assert_eq!(told.get("status"), None, "told, the default");

    // Proposing again proposes nothing twice, and nothing the profile holds.
    sqlx::query("UPDATE profile.fact_proposals SET state = 'dropped' WHERE text = 'Elixir'")
        .execute(&h.db)
        .await
        .unwrap();
    h.json(
        "PUT",
        "/api/v1/profile/about/accuracy",
        &json!({ "accuracy": "yes" }),
    )
    .await;
    h.work().await;
    assert_eq!(
        h.count("SELECT count(*) FROM profile.fact_proposals").await,
        7
    );

    // A decision more than a day old cannot be undone.
    sqlx::query("UPDATE profile.fact_proposals SET decided_at = now() - interval '25 hours'")
        .execute(&h.db)
        .await
        .unwrap();
    let (status, problem) = h.post(&format!("{PROPOSALS}/{id}/undo")).await;
    assert_eq!(
        (status.as_u16(), problem["code"].as_str()),
        (409, Some("undo_expired"))
    );
    assert_eq!(h.get(PROPOSALS).await.1["groups"], json!([]), "a day later");
}

#[sqlx::test(migrations = "../../../migrations")]
async fn save_proposes_what_it_does_not_write_itself(db: PgPool) {
    let h = Harness::new(db, None);
    assert_eq!(h.json("PUT", "/api/v1/profile", &profile()).await.0, 200);
    seed_about(&h).await;
    sqlx::query("UPDATE profile.about_you SET accuracy = 'yes' WHERE account_id = $1")
        .bind(h.account)
        .execute(&h.db)
        .await
        .unwrap();
    let (status, saved) = h.post("/api/v1/profile/about/save").await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(
        h.count("SELECT count(*) FROM profile.about_you").await,
        0,
        "the pending state is gone"
    );
    // Save wrote what was right and what they wrote; what it does not write
    // (the sentence that adds a tool, the assumptions left unanswered) waits
    // on Complete your profile.
    let (_, view) = h.get(PROPOSALS).await;
    let mut proposed = texts(&view);
    proposed.sort();
    assert_eq!(
        proposed,
        [
            "At Example Co you run the billing platform on Kafka.",
            "Built an open-source billing toolkit",
            "You ran the on-call rota",
        ]
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_import_applied_queues_proposals_and_the_page_says_it_read_the_cv(db: PgPool) {
    let h = Harness::new(db, None);
    let (status, import) = h
        .upload(&[
            Part::Text("kind", "linkedin_zip"),
            Part::File("file", "Basic_LinkedInDataExport.zip", &export_zip()),
        ])
        .await;
    assert_eq!(status, 201, "{import}");
    let (status, _) = h
        .json(
            "POST",
            &format!(
                "/api/v1/profile/imports/{}/apply",
                import["id"].as_str().unwrap()
            ),
            &json!({ "mode": "replace" }),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        h.count("SELECT count(*) FROM kernel.jobs WHERE kind = 'profile.propose_facts'")
            .await,
        1
    );
    h.work().await;
    let (status, view) = h.get(PROPOSALS).await;
    assert_eq!(status, 200);
    let read = &view["cv_read"];
    assert_eq!(read["jobs"], 2, "{view}");
    assert_eq!(read["skills"], 3);
    assert_eq!(read["degrees"], 1);
    assert_eq!(read["languages"], 2);
    assert_eq!(
        view["open"], 0,
        "the profile holds everything the export said"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_accounts_that_existed_before_are_proposed_for_once(db: PgPool) {
    let h = Harness::new(db.clone(), None);
    assert_eq!(h.json("PUT", "/api/v1/profile", &profile()).await.0, 200);
    seed_about(&h).await;
    let other = Harness::new(db.clone(), None);
    assert_eq!(
        other.json("PUT", "/api/v1/profile", &profile()).await.0,
        200
    );

    assert_eq!(super::backfill(&db, Utc::now()).await.unwrap(), 2);
    let (_, view) = h.get(PROPOSALS).await;
    assert_eq!(view["open"], 4, "{view}");
    assert_eq!(
        other.get(PROPOSALS).await.1["open"],
        0,
        "nothing to propose"
    );
    // Done once: the marker keeps it from running again.
    assert_eq!(super::backfill(&db, Utc::now()).await.unwrap(), 0);
    // Scheduling it at start is once too.
    assert!(super::schedule_backfill(&db).await.unwrap());
    assert!(!super::schedule_backfill(&db).await.unwrap());
}

async fn proposals(h: &Harness) -> Value {
    h.get(PROPOSALS).await.1
}

#[sqlx::test(migrations = "../../../migrations")]
async fn what_is_taken_back_in_tell_me_about_you_is_closed(db: PgPool) {
    let h = Harness::new(db, None);
    assert_eq!(h.json("PUT", "/api/v1/profile", &profile()).await.0, 200);
    seed_about(&h).await;
    h.json(
        "PUT",
        "/api/v1/profile/about/accuracy",
        &json!({ "accuracy": "yes" }),
    )
    .await;
    h.work().await;
    let view = proposals(&h).await;
    assert!(
        texts(&view).contains(&"Led 4 engineers".to_owned()),
        "{view}"
    );
    assert!(texts(&view).contains(&"You ran the on-call rota".to_owned()));

    // A change edited: the old words are closed, the new ones proposed.
    let (status, _) = h
        .json(
            "PUT",
            "/api/v1/profile/about/changes/c-Led%204%20engineers",
            &json!({ "action": "edit", "value": "Led 4 people" }),
        )
        .await;
    assert_eq!(status, 200);
    // An assumption switched to wrong, another to later.
    for (id, choice) in [("s2", "wrong"), ("s1", "later")] {
        let (status, _) = h
            .json(
                "PUT",
                &format!("/api/v1/profile/about/sentences/{id}"),
                &json!({ "choice": choice }),
            )
            .await;
        assert_eq!(status, 200);
    }
    // A change removed.
    let (status, _) = h
        .send(
            axum::http::Request::delete("/api/v1/profile/about/changes/c-Elixir")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, 200);
    h.work().await;
    let left = texts(&proposals(&h).await);
    for gone in [
        "Led 4 engineers",
        "You ran the on-call rota",
        "Led a team of 3 to 6 engineers",
        "Elixir",
    ] {
        assert!(!left.contains(&gone.to_owned()), "{gone} in {left:?}");
    }
    assert!(left.contains(&"Led 4 people".to_owned()), "{left:?}");

    // "Not really": what was only accurate goes too, and so does the sentence
    // taken from the profile.
    h.json(
        "PUT",
        "/api/v1/profile/about/accuracy",
        &json!({ "accuracy": "no" }),
    )
    .await;
    h.work().await;
    let left = texts(&proposals(&h).await);
    assert!(!left.contains(&"Built an open-source billing toolkit".to_owned()));
    assert!(
        !left.contains(&"At Example Co you run the billing platform on Kafka.".to_owned()),
        "{left:?}"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn save_never_writes_what_was_dropped_and_undo_keeps_what_save_wrote(db: PgPool) {
    let h = Harness::new(db, None);
    assert_eq!(h.json("PUT", "/api/v1/profile", &profile()).await.0, 200);
    seed_about(&h).await;
    h.json(
        "PUT",
        "/api/v1/profile/about/sentences/s1",
        &json!({ "choice": "right" }),
    )
    .await;
    h.work().await;
    let view = proposals(&h).await;
    // Drop two facts and a skill, keep the background.
    for text in [
        "Led 4 engineers",
        "Elixir",
        "Led a team of 3 to 6 engineers",
    ] {
        let id = find(&view, text)["id"].as_str().unwrap().to_owned();
        assert_eq!(h.post(&format!("{PROPOSALS}/{id}/drop")).await.0, 200);
    }
    let background = find(&view, "Ran a restaurant for 6 years")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        h.post(&format!("{PROPOSALS}/{background}/keep")).await.0,
        200
    );

    // Save writes the wizard's changes, but not what was dropped here.
    let (status, saved) = h.post("/api/v1/profile/about/save").await;
    assert_eq!(status, 200, "{saved}");
    let (_, now) = h.get("/api/v1/profile").await;
    let facts: Vec<&str> = now["profile"]["work"][0]["facts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["text"].as_str().unwrap())
        .collect();
    assert!(!facts.contains(&"Led 4 engineers"), "{facts:?}");
    assert!(
        !facts.contains(&"Led a team of 3 to 6 engineers"),
        "{facts:?}"
    );
    let skills: Vec<&str> = now["profile"]["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(skills, ["PostgreSQL"]);
    assert_eq!(
        now["profile"]["basics"]["background"],
        "Ran a restaurant for 6 years"
    );

    // The background changed since (Save, or a hand edit): Undo of the Keep
    // leaves it as it is, and the fact waits again.
    sqlx::query(
        "UPDATE profile.profiles
            SET data = jsonb_set(data, '{basics,background}',
                '\"Ran a restaurant for 6 years and a bar\"')
          WHERE account_id = $1",
    )
    .bind(h.account)
    .execute(&h.db)
    .await
    .unwrap();
    let (status, view) = h.post(&format!("{PROPOSALS}/{background}/undo")).await;
    assert_eq!(status, 200, "{view}");
    let (_, now) = h.get("/api/v1/profile").await;
    assert_eq!(
        now["profile"]["basics"]["background"],
        "Ran a restaurant for 6 years and a bar"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn keep_all_is_undone_at_once_and_a_held_fact_is_closed(db: PgPool) {
    let h = Harness::new(db, None);
    assert_eq!(h.json("PUT", "/api/v1/profile", &profile()).await.0, 200);
    seed_about(&h).await;
    // Elixir was said not to be theirs, once.
    sqlx::query(
        "INSERT INTO profile.skill_dismissals (account_id, name_key, created_at)
         VALUES ($1, 'elixir', now())",
    )
    .bind(h.account)
    .execute(&h.db)
    .await
    .unwrap();
    h.json(
        "PUT",
        "/api/v1/profile/about/sentences/s1",
        &json!({ "choice": "right" }),
    )
    .await;
    h.work().await;
    let view = proposals(&h).await;
    assert_eq!(view["open"], 4, "{view}");

    // The profile gains one of the facts by hand: keeping it writes nothing.
    let mut edited = profile();
    edited["work"][0]["facts"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": "f2", "text": "Led 4 engineers.", "quote": "I led 4 engineers",
            "area": "people", "kept_at": "2026-10-30T09:00:00Z"
        }));
    assert_eq!(h.json("PUT", "/api/v1/profile", &edited).await.0, 200);
    let before = version(&h).await;
    let led = find(&view, "Led 4 engineers")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, view) = h.post(&format!("{PROPOSALS}/{led}/keep")).await;
    assert_eq!(status, 200, "{view}");
    assert!(
        !texts(&view).contains(&"Led 4 engineers".to_owned()),
        "closed"
    );
    assert_eq!(view["kept"], 0, "not counted as added");
    assert_eq!(version(&h).await, before, "no new version");

    // Keep all, then one Undo for all of them.
    let (_, view) = h.post(&format!("{PROPOSALS}/keep-all")).await;
    assert_eq!(view["kept"], 3, "{view}");
    let batch = view["batch"]["id"].as_str().unwrap().to_owned();
    assert_eq!(view["batch"]["kept"], 3);
    assert_eq!(
        h.count("SELECT count(*) FROM profile.skill_dismissals")
            .await,
        0,
        "keeping Elixir forgot the \"not mine\""
    );
    let before = version(&h).await;
    let (status, view) = h.post(&format!("{PROPOSALS}/batches/{batch}/undo")).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(
        (view["open"].as_u64(), view["kept"].as_u64()),
        (Some(3), Some(0))
    );
    assert_eq!(view["batch"], Value::Null);
    assert_eq!(version(&h).await, before + 1, "one version for the undo");
    let (_, now) = h.get("/api/v1/profile").await;
    assert_eq!(now["profile"]["skills"].as_array().unwrap().len(), 1);
    assert_eq!(now["profile"]["basics"].get("background"), None);
    assert_eq!(
        h.count("SELECT count(*) FROM profile.skill_dismissals")
            .await,
        1,
        "and Undo says it again"
    );
    assert_eq!(
        h.post(&format!("{PROPOSALS}/batches/{batch}/undo")).await.0,
        404,
        "nothing left to undo"
    );
}
