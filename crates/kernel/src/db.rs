//! The database pool and its conventions.
//!
//! Each module owns a Postgres schema named after it (`identity`, …); the
//! kernel owns `kernel` (the outbox and the job queue) and the server's
//! backend-for-frontend owns `bff`. No query joins across schemas and no
//! foreign key crosses one (`cargo xtask arch` checks the migrations).
//! Queries are runtime-checked (`sqlx::query`, never the `query!` macros), so
//! building needs no database.

use std::{str::FromStr as _, time::Duration};

use secrecy::ExposeSecret as _;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

use crate::Config;

/// How long a request waits for a free connection before it gives up.
pub const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// Longest one statement may run before Postgres cancels it.
pub const STATEMENT_TIMEOUT: Duration = Duration::from_secs(30);

/// The pool `config` describes, connected.
pub async fn connect(config: &Config) -> Result<PgPool, sqlx::Error> {
    let options = PgConnectOptions::from_str(config.database_url.expose_secret())?
        .application_name("applicamp")
        .options([(
            "statement_timeout",
            format!("{}ms", STATEMENT_TIMEOUT.as_millis()),
        )]);
    PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_with(options)
        .await
}
