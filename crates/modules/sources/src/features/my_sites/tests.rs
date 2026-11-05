use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    SourcesConfig,
    job::WorkMode,
    testkit::{Fake, Harness, boards_config, today},
};

async fn harness(db: PgPool) -> Harness {
    let fake = Fake::lisbon_remote_only();
    fake.signed_in_as("someone@example.com");
    Harness::new(
        db,
        SourcesConfig {
            board_owners: Some(vec!["owner@example.com".into()]),
            ..boards_config()
        },
        None,
        fake,
    )
    .await
}

/// A site straight into the registry.
#[allow(clippy::too_many_arguments)]
async fn site(
    db: &PgPool,
    slug: &str,
    status: &str,
    tags: &[&str],
    country: Option<&str>,
    priority: &str,
    config: Value,
    terms: Option<(&str, &str)>,
) -> Uuid {
    let id = Uuid::now_v7();
    let tags: Vec<String> = tags.iter().map(|t| (*t).to_owned()).collect();
    sqlx::query(
        "INSERT INTO sources.job_boards (id, slug, name, homepage, host, kind, priority, status,
            origin, country, config, tags, terms_verdict, terms_quote, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 'json_api', $6, $7, 'discovery', $8, $9, $10, $11, $12, $13,
                 $13)",
    )
    .bind(id)
    .bind(slug)
    .bind(format!("Site {slug}"))
    .bind(format!("https://{slug}.example"))
    .bind(format!("{slug}.example"))
    .bind(priority)
    .bind(status)
    .bind(country)
    .bind(sqlx::types::Json(config))
    .bind(&tags)
    .bind(terms.map(|(verdict, _)| verdict))
    .bind(terms.map(|(_, quote)| quote))
    .bind(today())
    .execute(db)
    .await
    .unwrap();
    id
}

async fn plain(db: &PgPool, slug: &str, tags: &[&str], country: Option<&str>) -> Uuid {
    site(db, slug, "approved", tags, country, "low", json!({}), None).await
}

async fn put(h: &Harness, id: Uuid, on: bool) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/my-sites/{id}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "on": on }).to_string()))
        .unwrap();
    h.send(request).await
}

/// The sites of a list as `slug-less name -> on`, in the order shown.
fn names(list: &Value) -> Vec<(String, String, bool)> {
    list["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| {
            g["sites"].as_array().unwrap().iter().map(|s| {
                (
                    g["group"].as_str().unwrap().to_owned(),
                    s["name"].as_str().unwrap().to_owned(),
                    s["on"].as_bool().unwrap(),
                )
            })
        })
        .collect()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_app_turns_on_the_sites_that_fit_the_goals(db: PgPool) {
    let h = harness(db).await;
    plain(&h.db, "global", &["tech", "worldwide", "remote"], None).await;
    plain(&h.db, "eu", &["tech", "europe", "some-remote"], None).await;
    plain(&h.db, "pt", &["tech", "portugal", "some-remote"], None).await;
    plain(&h.db, "ptoffice", &["tech", "portugal"], None).await;
    plain(
        &h.db,
        "ruby",
        &["tech", "worldwide", "remote", "ruby"],
        None,
    )
    .await;
    plain(&h.db, "design", &["design", "worldwide", "remote"], None).await;
    plain(
        &h.db,
        "everything",
        &["tech", "worldwide", "startups"],
        None,
    )
    .await;
    // Hiring systems and company pages are not sites.
    sqlx::query(
        "INSERT INTO sources.job_boards (id, slug, name, homepage, host, kind, priority, status,
            origin, system, created_at, updated_at)
         VALUES ($1, 'greenhouse', 'Greenhouse', 'https://boards.greenhouse.io', 'boards.greenhouse.io',
                 'hiring_system', 'low', 'approved', 'seed', 'greenhouse', $2, $2)",
    )
    .bind(Uuid::now_v7())
    .bind(today())
    .execute(&h.db)
    .await
    .unwrap();

    let (status, list) = h.get("/api/v1/my-sites").await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["country"], "PT");
    assert_eq!(list["available"], 7);
    // Remote-only, in Lisbon, a backend engineer.
    assert_eq!(list["on"], 4);
    let by_name: std::collections::HashMap<String, (String, bool)> = names(&list)
        .into_iter()
        .map(|(group, name, on)| (name, (group, on)))
        .collect();
    let at = |slug: &str| by_name[&format!("Site {slug}")].clone();
    assert_eq!(at("global"), ("remote_worldwide".to_owned(), true));
    assert_eq!(at("eu"), ("europe".to_owned(), true));
    assert_eq!(at("pt"), ("country".to_owned(), true));
    assert_eq!(at("ptoffice"), ("country".to_owned(), false));
    assert_eq!(at("ruby"), ("trade".to_owned(), false));
    assert_eq!(at("design"), ("trade".to_owned(), false));
    assert_eq!(at("everything"), ("everything".to_owned(), true));
    // The groups come in order.
    let groups: Vec<&str> = list["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["group"].as_str().unwrap())
        .collect();
    assert_eq!(
        groups,
        [
            "remote_worldwide",
            "europe",
            "country",
            "trade",
            "everything"
        ]
    );
    assert_eq!(list["company_pages"], 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_persons_own_choice_is_never_overwritten(db: PgPool) {
    let h = harness(db).await;
    let global = plain(&h.db, "global", &["tech", "worldwide", "remote"], None).await;
    let other = plain(&h.db, "other", &["tech", "worldwide", "remote"], None).await;

    let (_, list) = h.get("/api/v1/my-sites").await;
    assert_eq!(list["on"], 2);

    let (status, choice) = put(&h, global, false).await;
    assert_eq!(status, StatusCode::OK, "{choice}");
    assert_eq!(choice["on"], false);
    assert_eq!(choice["set_by"], "person");

    // Their goals now say office work: the app turns off what it set, and
    // keeps what they chose, and the other way round.
    {
        let mut input = h.fake.input.lock().unwrap();
        input.as_mut().unwrap().work_modes = vec![WorkMode::Onsite];
    }
    let (_, list) = h.get("/api/v1/my-sites").await;
    assert_eq!(list["on"], 0);
    let sites = names(&list);
    assert!(sites.iter().all(|(_, _, on)| !on), "{sites:?}");

    let (status, _) = put(&h, global, true).await;
    assert_eq!(status, StatusCode::OK);
    {
        let mut input = h.fake.input.lock().unwrap();
        input.as_mut().unwrap().work_modes = vec![WorkMode::Remote];
    }
    let (_, list) = h.get("/api/v1/my-sites").await;
    assert_eq!(list["on"], 2);
    // Turned off again by the person: it stays off after the goals change.
    put(&h, other, false).await;
    {
        let mut input = h.fake.input.lock().unwrap();
        input.as_mut().unwrap().work_modes = vec![WorkMode::Onsite, WorkMode::Remote];
    }
    let (_, list) = h.get("/api/v1/my-sites").await;
    let sites = names(&list);
    assert!(sites.contains(&(
        "remote_worldwide".to_owned(),
        "Site other".to_owned(),
        false
    )));
    assert!(sites.contains(&(
        "remote_worldwide".to_owned(),
        "Site global".to_owned(),
        true
    )));
    let on = super::on_sites(&h.db, h.account).await.unwrap();
    assert_eq!(on, [global]);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_site_approved_later_gets_a_default_and_new_goals_are_picked_up(db: PgPool) {
    let h = harness(db).await;
    let first = plain(&h.db, "first", &["tech", "worldwide", "remote"], None).await;
    let (_, list) = h.get("/api/v1/my-sites").await;
    assert_eq!(list["on"], 1);

    // A candidate is not listed until it is approved; then the app decides.
    let later = site(
        &h.db,
        "later",
        "candidate",
        &["tech", "worldwide", "remote"],
        None,
        "high",
        json!({}),
        None,
    )
    .await;
    let (_, list) = h.get("/api/v1/my-sites").await;
    assert_eq!(list["available"], 1);
    sqlx::query("UPDATE sources.job_boards SET status = 'approved' WHERE id = $1")
        .bind(later)
        .execute(&h.db)
        .await
        .unwrap();
    let (_, list) = h.get("/api/v1/my-sites").await;
    assert_eq!(list["available"], 2);
    assert_eq!(list["on"], 2);

    // The searches' entry point.
    {
        let mut input = h.fake.input.lock().unwrap();
        input.as_mut().unwrap().work_modes = vec![WorkMode::Onsite];
    }
    let input = h.fake.input.lock().unwrap().clone().unwrap();
    super::ensure_defaults(&h.db, h.account, &input, h.clock_now())
        .await
        .unwrap();
    assert!(super::on_sites(&h.db, h.account).await.unwrap().is_empty());
    let _ = first;
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_site_that_needs_a_key_waits_and_sites_that_forbid_apps_are_named(db: PgPool) {
    let h = harness(db).await;
    let keyed = site(
        &h.db,
        "keyed",
        "candidate",
        &["tech", "portugal"],
        Some("PT"),
        "low",
        json!({
            "endpoint": "https://keyed.example/api",
            "fields": { "title": "/title", "url": "/url" },
            "key": { "params": ["api_key"], "how": "Sign up, it is free.",
                     "url": "https://keyed.example/key" }
        }),
        None,
    )
    .await;
    site(
        &h.db,
        "forbids",
        "candidate",
        &["tech", "worldwide"],
        None,
        "low",
        json!({}),
        Some(("forbid", "You may not use bots.")),
    )
    .await;
    site(
        &h.db,
        "declined",
        "declined",
        &["tech"],
        None,
        "low",
        json!({}),
        None,
    )
    .await;
    site(
        &h.db,
        "paused",
        "paused",
        &["tech"],
        None,
        "low",
        json!({}),
        None,
    )
    .await;

    let (_, list) = h.get("/api/v1/my-sites").await;
    assert_eq!(list["available"], 1);
    assert_eq!(list["on"], 0);
    assert_eq!(list["blocked"]["count"], 1);
    assert_eq!(list["blocked"]["names"], json!(["Site forbids"]));
    let listed = &list["groups"][0]["sites"][0];
    assert_eq!(listed["name"], "Site keyed");
    assert_eq!(listed["needs_key"], true);
    assert_eq!(listed["on"], false);
    // A country's typical pay stands in for a figure of its own.
    assert_eq!(listed["salary_basis"], "country");
    assert!(listed["salary_eur_year"].as_f64().unwrap() > 0.0);

    let (status, problem) = put(&h, keyed, true).await;
    assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    assert_eq!(problem["code"], "needs_key");
    let (status, _) = put(&h, Uuid::now_v7(), true).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn the_list_is_searched_and_filtered(db: PgPool) {
    let h = harness(db).await;
    let global = plain(&h.db, "global", &["tech", "worldwide", "remote"], None).await;
    plain(&h.db, "eu", &["tech", "europe", "some-remote"], None).await;
    plain(
        &h.db,
        "ruby",
        &["tech", "worldwide", "remote", "ruby"],
        None,
    )
    .await;
    // Salaries: ten stated, a typical figure of its own.
    for i in 0..10 {
        sqlx::query(
            "INSERT INTO sources.board_jobs (board_id, source_ref, title, regions, engineering,
                salary_eur_year, posted_at, seen_at)
             VALUES ($1, $2, 'Engineer', '{}', true, 96400, $3, $3)",
        )
        .bind(global)
        .bind(format!("j{i}"))
        .bind(today())
        .execute(&h.db)
        .await
        .unwrap();
    }

    let (_, all) = h.get("/api/v1/my-sites").await;
    let first = &all["groups"][0]["sites"][0];
    assert_eq!(first["name"], "Site global");
    assert_eq!(first["salary_eur_year"], 96000.0);
    assert_eq!(first["salary_basis"], "listed");

    let (_, found) = h.get("/api/v1/my-sites?q=RUB").await;
    assert_eq!(names(&found).len(), 1);
    // The counts are of the whole list.
    assert_eq!(found["available"], 3);

    let (_, europe) = h.get("/api/v1/my-sites?show=europe").await;
    assert_eq!(names(&europe).len(), 1);
    let (_, trade) = h.get("/api/v1/my-sites?show=trade").await;
    assert_eq!(names(&trade).len(), 1);
    let (_, remote) = h.get("/api/v1/my-sites?show=remote").await;
    assert_eq!(names(&remote).len(), 2);
    let (_, on) = h.get("/api/v1/my-sites?show=on").await;
    assert!(names(&on).iter().all(|(_, _, on)| *on));
    let (status, _) = h.get("/api/v1/my-sites?show=everything").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_site_says_how_many_jobs_are_new_this_week(db: PgPool) {
    let h = harness(db).await;
    let busy = plain(&h.db, "busy", &["tech", "worldwide", "remote"], None).await;
    plain(&h.db, "quiet", &["tech", "worldwide", "remote"], None).await;
    // Two new in the week, one a month old: the count is of the new ones.
    for (i, days) in [1, 3, 30].into_iter().enumerate() {
        let seen = today() - chrono::TimeDelta::days(days);
        sqlx::query(
            "INSERT INTO sources.store_jobs (id, board_id, source_ref, title, url, content_hash,
                first_seen_at, last_seen_at)
             VALUES ($1, $2, $3, 'Engineer', 'https://busy.example/1', 'h', $4, $4)",
        )
        .bind(Uuid::now_v7())
        .bind(busy)
        .bind(format!("job-{i}"))
        .bind(seen)
        .execute(&h.db)
        .await
        .unwrap();
    }
    let (_, list) = h.get("/api/v1/my-sites").await;
    let sites = list["groups"][0]["sites"].as_array().unwrap();
    let get = |name: &str| sites.iter().find(|s| s["name"] == name).unwrap().clone();
    assert_eq!(get("Site busy")["new_this_week"], 2);
    assert!(get("Site quiet")["new_this_week"].is_null());
}
