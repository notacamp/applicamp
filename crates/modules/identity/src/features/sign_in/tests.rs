use applicamp_kernel::{AppState, Config, crypto::SecretKey};
use chrono::Utc;
use secrecy::SecretString;
use sqlx::PgPool;
use uuid::Uuid;

use super::*;
use crate::invites;

fn state(db: PgPool) -> AppState {
    AppState::new(
        db,
        Config::for_database("postgres://unused"),
        SecretKey::from_bytes([7; 32]),
    )
}

fn identity(provider: Provider, subject: &str, email: &str) -> ProviderIdentity {
    ProviderIdentity {
        provider,
        subject: subject.to_owned(),
        email: Some(email.to_owned()),
        email_verified: true,
        name: Some("Ana Example".to_owned()),
        picture_url: None,
        refresh_token: None,
    }
}

async fn invite(db: &PgPool, email: &str) {
    invites::add(db, email, None, Utc::now()).await.unwrap();
}

async fn count(db: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(db).await.unwrap()
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_invited_person_signing_in_first_gets_an_account(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let state = state(db.clone());
    let mut who = identity(Provider::Campbooks, "cb-1", "Ana@Example.com");
    who.refresh_token = Some(SecretString::from("refresh-secret".to_owned()));

    let SignIn::Account(account) = sign_in(&state, who).await.unwrap() else {
        panic!("an invited person signs in");
    };

    let (email, verified): (String, bool) =
        sqlx::query_as("SELECT email, email_verified FROM identity.accounts WHERE id = $1")
            .bind(account)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!((email.as_str(), verified), ("ana@example.com", true));
    let (way_in, sealed): (Uuid, Vec<u8>) =
        sqlx::query_as("SELECT id, refresh_token FROM identity.ways_in WHERE account_id = $1")
            .bind(account)
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(
        !String::from_utf8_lossy(&sealed).contains("refresh-secret"),
        "stored sealed, never in the clear"
    );
    assert_eq!(
        token_cipher(&state)
            .open(&sealed, &token_context(way_in))
            .unwrap(),
        "refresh-secret"
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM kernel.events WHERE name = 'identity.account_created'"
        )
        .await,
        1
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn someone_not_on_the_list_gets_nothing(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let state = state(db.clone());

    let outcome = sign_in(
        &state,
        identity(Provider::Linkedin, "li-1", "bo@example.com"),
    )
    .await
    .unwrap();

    assert_eq!(outcome, SignIn::NotInvited);
    assert_eq!(
        count(&db, "SELECT count(*) FROM identity.accounts").await,
        0
    );
    assert_eq!(count(&db, "SELECT count(*) FROM identity.ways_in").await, 0);
    assert_eq!(count(&db, "SELECT count(*) FROM kernel.events").await, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn an_unverified_address_is_not_enough(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let state = state(db.clone());
    let mut who = identity(Provider::Linkedin, "li-1", "ana@example.com");
    who.email_verified = false;

    assert_eq!(sign_in(&state, who).await.unwrap(), SignIn::NotInvited);
    assert_eq!(
        count(&db, "SELECT count(*) FROM identity.accounts").await,
        0
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn both_ways_in_reach_one_account(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let state = state(db.clone());

    let first = sign_in(
        &state,
        identity(Provider::Campbooks, "cb-1", "ana@example.com"),
    )
    .await
    .unwrap();
    let second = sign_in(
        &state,
        identity(Provider::Linkedin, "li-9", "ANA@example.com"),
    )
    .await
    .unwrap();

    assert!(matches!(first, SignIn::Account(_)));
    assert_eq!(first, second);
    assert_eq!(
        count(&db, "SELECT count(*) FROM identity.accounts").await,
        1
    );
    assert_eq!(count(&db, "SELECT count(*) FROM identity.ways_in").await, 2);
}

#[sqlx::test(migrations = "../../../migrations")]
async fn a_linked_way_in_stops_signing_in_once_uninvited(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let state = state(db.clone());
    let SignIn::Account(account) = sign_in(
        &state,
        identity(Provider::Campbooks, "cb-1", "ana@example.com"),
    )
    .await
    .unwrap() else {
        panic!("an invited person signs in");
    };
    assert!(
        invites::remove(&db, &state.events, "ana@example.com")
            .await
            .unwrap()
    );

    // The linked way in no longer signs in, and neither does a new one.
    for who in [
        identity(Provider::Campbooks, "cb-1", "ana@example.com"),
        identity(Provider::Linkedin, "li-1", "ana@example.com"),
    ] {
        assert_eq!(sign_in(&state, who).await.unwrap(), SignIn::NotInvited);
    }

    // Nothing was deleted: the account and its way in are kept.
    assert_eq!(
        count(&db, "SELECT count(*) FROM identity.accounts").await,
        1
    );
    assert_eq!(count(&db, "SELECT count(*) FROM identity.ways_in").await, 1);

    // Inviting the address again restores access to the same account.
    invite(&db, "ana@example.com").await;
    assert_eq!(
        sign_in(
            &state,
            identity(Provider::Campbooks, "cb-1", "ana@example.com")
        )
        .await
        .unwrap(),
        SignIn::Account(account)
    );
}

#[sqlx::test(migrations = "../../../migrations")]
async fn only_campbooks_refresh_tokens_are_kept(db: PgPool) {
    invite(&db, "ana@example.com").await;
    let state = state(db.clone());
    let mut who = identity(Provider::Linkedin, "li-1", "ana@example.com");
    who.refresh_token = Some(SecretString::from("linkedin-token".to_owned()));

    sign_in(&state, who).await.unwrap();

    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM identity.ways_in WHERE refresh_token IS NOT NULL"
        )
        .await,
        0
    );
}

#[test]
fn debug_shows_no_address_or_token() {
    let mut who = identity(Provider::Campbooks, "cb-1", "ana@example.com");
    who.refresh_token = Some(SecretString::from("refresh-secret".to_owned()));
    let debug = format!("{who:?}");
    assert!(!debug.contains("ana@") && !debug.contains("refresh-secret") && !debug.contains("Ana"));
}
