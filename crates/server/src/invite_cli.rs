//! `applicamp invite add|remove|list`: the only way to change who may
//! create an account. Reads `APPLICAMP_DATABASE_URL`, applies pending
//! migrations unless `APPLICAMP_RUN_MIGRATIONS=false`, and prints plain
//! lines.

use std::{io::Write as _, process::ExitCode};

use applicamp_identity::invites::{self, Added};
use applicamp_kernel::{Config, db, events::EventBus};
use clap::Subcommand;

use crate::{fail, runtime};

/// The invite list's commands.
#[derive(Debug, Subcommand)]
pub enum InviteCommand {
    /// Put an address on the list: a person signing in with it (verified by
    /// Campbooks or LinkedIn) gets an account.
    Add {
        /// The address.
        email: String,
        /// Why, for you.
        #[arg(long)]
        note: Option<String>,
    },
    /// Take an address off the list: its account can no longer sign in and
    /// its sessions end. The account and its data are kept; `invite add`
    /// restores access.
    Remove {
        /// The address.
        email: String,
    },
    /// Print the list, oldest first.
    List,
}

/// Run `command`.
pub fn run(command: InviteCommand) -> ExitCode {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(e) => return fail(&e.to_string()),
    };
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(e) => return fail(&e),
    };
    match runtime.block_on(execute(&config, command)) {
        Ok(lines) => {
            let mut out = std::io::stdout().lock();
            for line in lines {
                let _ = writeln!(out, "{line}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

async fn execute(config: &Config, command: InviteCommand) -> Result<Vec<String>, String> {
    let db = db::connect(config)
        .await
        .map_err(|e| format!("could not reach the database: {e}"))?;
    if config.run_migrations {
        applicamp_server::migrate(&db)
            .await
            .map_err(|e| format!("could not apply migrations: {e}"))?;
    }
    let registry = applicamp_server::registry()
        .map_err(|problems| format!("background work: {}", problems.join("; ")))?;
    let events = EventBus::new(std::sync::Arc::new(registry));
    let now = chrono::Utc::now();
    let problem = |e: applicamp_kernel::Error| e.to_problem().detail;
    Ok(match command {
        InviteCommand::Add { email, note } => {
            match invites::add(&db, &email, note.as_deref(), now)
                .await
                .map_err(problem)?
            {
                Added::New => vec![format!("invited {}", email.trim().to_lowercase())],
                Added::AlreadyInvited => {
                    vec![format!(
                        "{} was already invited",
                        email.trim().to_lowercase()
                    )]
                }
            }
        }
        InviteCommand::Remove { email } => {
            if invites::remove(&db, &events, &email)
                .await
                .map_err(problem)?
            {
                vec![format!(
                    "removed {} (its account, if any, is signed out and kept; `invite add` restores it)",
                    email.trim().to_lowercase()
                )]
            } else {
                vec![format!("{} was not invited", email.trim().to_lowercase())]
            }
        }
        InviteCommand::List => {
            let list = invites::list(&db).await.map_err(problem)?;
            if list.is_empty() {
                vec!["nobody is invited yet".to_owned()]
            } else {
                list.into_iter()
                    .map(|invite| match invite.note {
                        Some(note) => format!(
                            "{}\t{}\t{note}",
                            invite.email,
                            invite.invited_at.format("%Y-%m-%d")
                        ),
                        None => {
                            format!("{}\t{}", invite.email, invite.invited_at.format("%Y-%m-%d"))
                        }
                    })
                    .collect()
            }
        }
    })
}
