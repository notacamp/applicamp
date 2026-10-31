//! `applicamp-agent`: the runner inside every sandbox (ADR 0009).
//!
//! `applicamp-agent --run <id> --api <url> (--token <t> | --token-file <path>)`
//! (`--token-file -` reads the token from standard input). It holds nothing
//! but its run id, its single-run token and the agent API's address; it
//! reads its context, does its kind's work through the agent API, writes
//! each event on standard output and through the API, finishes, and exits.
//! It depends on no other Applicamp crate.

#![forbid(unsafe_code)]

mod api;
mod probe;

use std::{
    io::{Read as _, Write as _},
    path::PathBuf,
    process::ExitCode,
};

use clap::Parser;
use url::Url;

#[derive(Debug, Parser)]
#[command(
    name = "applicamp-agent",
    version,
    about = "The Applicamp agent runner."
)]
struct Cli {
    /// The run.
    #[arg(long)]
    run: String,
    /// The agent API's address.
    #[arg(long)]
    api: Url,
    /// The run's token.
    #[arg(
        long,
        env = "APPLICAMP_AGENT_TOKEN",
        hide_env_values = true,
        conflicts_with = "token_file"
    )]
    token: Option<String>,
    /// A file holding the run's token; `-` is standard input.
    #[arg(long)]
    token_file: Option<PathBuf>,
}

/// Report `message` on stderr; a failed exit.
fn fail(message: &str) -> ExitCode {
    let _ = writeln!(std::io::stderr(), "applicamp-agent: {message}");
    ExitCode::FAILURE
}

fn token(cli: &Cli) -> Result<String, String> {
    let raw = match (&cli.token, &cli.token_file) {
        (Some(token), _) => token.clone(),
        (None, Some(path)) if path.as_os_str() == "-" => {
            let mut token = String::new();
            std::io::stdin()
                .read_to_string(&mut token)
                .map_err(|e| format!("could not read the token: {e}"))?;
            token
        }
        (None, Some(path)) => std::fs::read_to_string(path)
            .map_err(|e| format!("could not read the token file: {e}"))?,
        (None, None) => return Err("no token (--token or --token-file)".to_owned()),
    };
    let token = raw.trim().to_owned();
    if token.is_empty() {
        return Err("the token is empty".to_owned());
    }
    Ok(token)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let token = match token(&cli) {
        Ok(token) => token,
        Err(e) => return fail(&e),
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => return fail(&format!("could not start: {e}")),
    };
    runtime.block_on(async {
        let http = match api::client() {
            Ok(http) => http,
            Err(e) => return fail(&format!("could not make its HTTP client: {e}")),
        };
        let api = api::Api::new(http, cli.api.clone(), token);
        let context = match api.context().await {
            Ok(context) => context,
            Err(e) => return fail(&format!("could not read its context: {e}")),
        };
        match context["kind"].as_str().unwrap_or_default() {
            probe::KIND => {
                let outside = Url::parse(probe::OUTSIDE).unwrap_or_else(|_| cli.api.clone());
                match probe::run(&api, &context, &outside).await {
                    "succeeded" => ExitCode::SUCCESS,
                    _ => ExitCode::FAILURE,
                }
            }
            other => {
                api.event(
                    "error",
                    &format!("This runner does not do `{other}` runs."),
                    None,
                    None,
                )
                .await;
                let _ = api.finish("failed", serde_json::json!({})).await;
                fail(&format!("unknown kind `{other}` (run {})", cli.run))
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_parses() {
        let cli = Cli::try_parse_from([
            "applicamp-agent",
            "--run",
            "0190",
            "--api",
            "https://applicamp.example.com",
            "--token",
            "t",
        ])
        .unwrap();
        assert_eq!(token(&cli).unwrap(), "t");
        assert!(
            Cli::try_parse_from([
                "applicamp-agent",
                "--run",
                "x",
                "--api",
                "https://applicamp.example.com",
                "--token",
                "t",
                "--token-file",
                "-",
            ])
            .is_err()
        );
        let none = Cli::try_parse_from([
            "applicamp-agent",
            "--run",
            "x",
            "--api",
            "https://a.example.com",
        ])
        .unwrap();
        assert!(token(&none).is_err());
    }
}
