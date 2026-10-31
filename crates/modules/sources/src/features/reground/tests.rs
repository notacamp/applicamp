use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    ports::Unreasoned,
    testkit::{Fake, Harness, config, greenhouse_boards},
};

use super::{Regrounded, reground, schedule};

fn found(account: Uuid, why: &[&str]) -> Unreasoned {
    Unreasoned {
        account,
        opportunity: Uuid::now_v7(),
        why: why.iter().map(|r| (*r).to_owned()).collect(),
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn old_reasons_the_profile_never_supported_are_stripped_once(db: PgPool) {
    let boards = greenhouse_boards().await;
    let h = Harness::new(db, config(&boards), None, Fake::no_skills()).await;
    let invented = found(h.account, &["Full-stack TypeScript matches profile"]);
    let mixed = found(
        h.account,
        &[
            "Senior backend engineer is the role you want",
            "Deep TypeScript expertise",
        ],
    );
    let clean = found(h.account, &["Remote, as you asked"]);
    h.fake
        .unreasoned
        .lock()
        .unwrap()
        .extend([invented.clone(), mixed.clone(), clean.clone()]);

    // At start, once: a waiting check is not doubled.
    assert!(schedule(&h.db, &h.deps).await.unwrap());
    assert!(!schedule(&h.db, &h.deps).await.unwrap());
    h.work().await;

    let done = h.fake.reasoned();
    assert_eq!(done.len(), 3, "every one is marked checked");
    let of = |id: Uuid| {
        done.iter()
            .find(|(opportunity, ..)| *opportunity == id)
            .unwrap()
    };
    let (_, why, updated) = of(invented.opportunity);
    assert!(why.is_empty());
    assert!(*updated, "the peek says Reasons updated");
    let (_, why, updated) = of(mixed.opportunity);
    assert_eq!(why, &["Senior backend engineer is the role you want"]);
    assert!(*updated);
    let (_, why, updated) = of(clean.opportunity);
    assert_eq!(why, &["Remote, as you asked"]);
    assert!(!*updated, "nothing changed: nothing to say");

    // Again: nothing is left.
    assert_eq!(
        reground(&h.deps, &h.db).await.unwrap(),
        Regrounded::default()
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn someone_without_a_profile_waits_for_a_later_start(db: PgPool) {
    let boards = greenhouse_boards().await;
    let h = Harness::new(db, config(&boards), None, Fake::default().into()).await;
    h.fake
        .unreasoned
        .lock()
        .unwrap()
        .push(found(h.account, &["Rails experience matches"]));
    assert_eq!(
        reground(&h.deps, &h.db).await.unwrap(),
        Regrounded::default()
    );
    assert_eq!(h.fake.unreasoned.lock().unwrap().len(), 1, "left for later");
}
