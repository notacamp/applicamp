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

    assert!(remove(&db, "SOMEONE@example.com").await.unwrap());
    assert!(!remove(&db, "someone@example.com").await.unwrap());
    assert!(list(&db).await.unwrap().is_empty());
    assert!(add(&db, "not an address", None, now).await.is_err());
}
