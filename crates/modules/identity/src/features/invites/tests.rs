use applicamp_kernel::events::EventBus;
use chrono::Utc;
use sqlx::PgPool;

use super::*;

#[test]
fn addresses_are_normalized_or_refused() {
    assert_eq!(
        normalize_email("  Someone@Example.COM ").unwrap(),
        "someone@example.com"
    );
    for bad in [
        "",
        "someone",
        "@example.com",
        "someone@",
        "someone@example",
        "some one@example.com",
        "a@b@example.com",
        "someone@.example.com",
        "someone@example.com.",
    ] {
        assert!(normalize_email(bad).is_err(), "{bad:?}");
    }
}

#[sqlx::test(migrations = "../../../migrations")]
async fn adding_listing_and_removing(db: PgPool) {
    let now = Utc::now();
    let events = EventBus::default();
    assert_eq!(
        add(&db, "Someone@example.com", Some("friend"), now)
            .await
            .unwrap(),
        Added::New
    );
    assert_eq!(
        add(&db, "someone@example.com", None, now).await.unwrap(),
        Added::AlreadyInvited
    );
    let invites = list(&db).await.unwrap();
    assert_eq!(invites.len(), 1);
    assert_eq!(invites[0].email, "someone@example.com");
    assert_eq!(invites[0].note.as_deref(), Some("friend"));

    assert!(remove(&db, &events, "SOMEONE@example.com").await.unwrap());
    assert!(!remove(&db, &events, "someone@example.com").await.unwrap());
    assert!(list(&db).await.unwrap().is_empty());
    assert!(add(&db, "not an address", None, now).await.is_err());
}

async fn revocations(db: &PgPool) -> Vec<serde_json::Value> {
    sqlx::query_scalar("SELECT payload FROM kernel.events WHERE name = 'identity.invite_revoked'")
        .fetch_all(db)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn removing_an_invite_revokes_the_account_with_that_address(db: PgPool) {
    let events = EventBus::default();
    let account = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO identity.accounts (id, email, email_verified, created_at)
         VALUES ($1, 'ana@example.com', true, now())",
    )
    .bind(account)
    .execute(&db)
    .await
    .unwrap();
    add(&db, "ana@example.com", None, Utc::now()).await.unwrap();
    add(&db, "bo@example.com", None, Utc::now()).await.unwrap();

    // An address with no account announces nothing.
    assert!(remove(&db, &events, "bo@example.com").await.unwrap());
    assert!(revocations(&db).await.is_empty());

    assert!(remove(&db, &events, "ana@example.com").await.unwrap());
    let published = revocations(&db).await;
    assert_eq!(published.len(), 1);
    // Ids only: no address in the event.
    assert_eq!(published[0], serde_json::json!({ "account": account }));
    // The account is kept.
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM identity.accounts")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(kept, 1);
}
