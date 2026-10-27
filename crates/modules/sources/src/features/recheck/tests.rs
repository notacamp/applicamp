use applicamp_ai::BudgetConfig;
use chrono::TimeDelta;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    job::{Job, Source, WorkMode},
    ports::Unchecked,
    requirements::{Answer, Excluded},
    testkit::{Fake, Harness, config, gateway, greenhouse_boards, model_reading_facts},
};

use super::{Rechecked, recheck, schedule};

/// A posting found by a search before the rules, as the pipeline keeps it.
fn found(id: u32, title: &str, location: &str, description: &str) -> Job {
    Job {
        source: Source::Arbeitnow,
        source_ref: format!("found-{id}"),
        title: title.into(),
        company: "Beispiel Software GmbH".into(),
        location: Some(location.into()),
        work_mode: Some(WorkMode::Remote),
        board_remote: None,
        url: format!("https://www.arbeitnow.com/jobs/{id}"),
        posted_at: None,
        salary_text: None,
        description: description.into(),
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn matches_from_before_the_rules_are_checked_once_and_hidden_when_they_fail(db: PgPool) {
    let boards = greenhouse_boards().await;
    let model = model_reading_facts().await;
    let h = Harness::new(
        db,
        config(&boards),
        Some(gateway(&model, BudgetConfig::default())),
        Fake::lisbon_remote_only(),
    )
    .await;
    let (dach, europe) = (Uuid::now_v7(), Uuid::now_v7());
    h.fake.unchecked.lock().unwrap().extend([
        Unchecked {
            account: h.account,
            opportunity: dach,
            job: found(
                1,
                "Senior Backend Engineer (m/w/d)",
                "Berlin",
                "Standort: Remote-first aus Deutschland oder Österreich",
            ),
        },
        Unchecked {
            account: h.account,
            opportunity: europe,
            job: found(
                2,
                "Staff Engineer",
                "Remote (EU)",
                "Fully remote, anywhere in Europe.",
            ),
        },
    ]);

    // At start, once: a waiting recheck is not doubled.
    assert!(schedule(&h.db, &h.deps).await.unwrap());
    assert!(!schedule(&h.db, &h.deps).await.unwrap());
    h.work().await;

    let checked = h.fake.checked();
    assert_eq!(checked.len(), 2);
    let (_, facts, hidden) = checked.iter().find(|(id, ..)| *id == dach).unwrap();
    assert_eq!(*hidden, Some(Excluded::NotOpenToLocation));
    assert_eq!(facts.remote, Answer::Yes);
    assert_eq!(
        facts.region_note.as_deref(),
        Some("Germany or Austria only")
    );
    let (_, _, hidden) = checked.iter().find(|(id, ..)| *id == europe).unwrap();
    assert_eq!(*hidden, None, "kept in view");
    assert_eq!(
        model.received_requests().await.unwrap().len(),
        1,
        "one call for both"
    );

    // Again: nothing is left to check, nothing is asked.
    h.clock.advance(TimeDelta::hours(1));
    assert_eq!(recheck(&h.deps, &h.db).await.unwrap(), Rechecked::default());
    assert_eq!(model.received_requests().await.unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn without_a_model_nothing_is_checked(db: PgPool) {
    let boards = greenhouse_boards().await;
    let h = Harness::new(db, config(&boards), None, Fake::lisbon_remote_only()).await;
    h.fake.unchecked.lock().unwrap().push(Unchecked {
        account: h.account,
        opportunity: Uuid::now_v7(),
        job: found(1, "Staff Engineer", "Portugal", "Build things."),
    });
    assert_eq!(recheck(&h.deps, &h.db).await.unwrap(), Rechecked::default());
    assert_eq!(h.fake.unchecked.lock().unwrap().len(), 1, "left for later");
}
