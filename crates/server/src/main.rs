//! The `applicamp` binary: the server and its command line.

#![forbid(unsafe_code)]

mod invite_cli;

use std::{
    io::{Read as _, Write as _},
    net::{SocketAddr, TcpStream},
    process::ExitCode,
    sync::Arc,
    time::Duration,
};

use applicamp_kernel::{
    AppState, Config, EventBus, crypto::SecretKey, db, jobs::Worker, telemetry,
};
use applicamp_server::{bff, web::Web};
use clap::{Parser, Subcommand};
use tokio::sync::watch;

/// Port used when `APPLICAMP_BIND_ADDR` is unset or unparsable.
const DEFAULT_PORT: u16 = 8080;
/// How long `healthcheck` waits for the server.
const HEALTHCHECK_TIMEOUT: Duration = Duration::from_secs(5);
/// How often ended sessions are deleted.
const PRUNE_EVERY: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Parser)]
#[command(
    name = "applicamp",
    version,
    about = "The Applicamp server: HTTP API, job worker and tools."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the HTTP API and the job worker (the default, and what the image
    /// runs).
    Serve,
    /// Manage the invite list: who may create an account. The only way to
    /// change it.
    Invite {
        #[command(subcommand)]
        command: invite_cli::InviteCommand,
    },
    /// Print the OpenAPI document as JSON.
    Openapi,
    /// Exit 0 if the local server reports healthy, 1 otherwise. For container
    /// health checks (the runtime image has no shell or curl).
    Healthcheck {
        /// Port to probe on 127.0.0.1. Defaults to the port of
        /// `APPLICAMP_BIND_ADDR`, else 8080.
        #[arg(long)]
        port: Option<u16>,
    },
}

fn main() -> ExitCode {
    telemetry::install_panic_hook();
    match Cli::parse().command.unwrap_or(Command::Serve) {
        Command::Serve => serve(),
        Command::Invite { command } => invite_cli::run(command),
        Command::Openapi => print_json(&applicamp_server::openapi()),
        Command::Healthcheck { port } => healthcheck(port.unwrap_or_else(port_from_env)),
    }
}

fn print_json(value: &impl serde::Serialize) -> ExitCode {
    let result = serde_json::to_string_pretty(value)
        .map_err(std::io::Error::other)
        .and_then(|json| writeln!(std::io::stdout(), "{json}"));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&format!("could not print the OpenAPI document: {e}")),
    }
}

/// A multi-threaded runtime.
pub(crate) fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("could not start the async runtime: {e}"))
}

fn serve() -> ExitCode {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(e) => return fail(&e.to_string()),
    };
    telemetry::init(config.log_format);
    let secret = match SecretKey::from_config(&config) {
        Ok(secret) => secret,
        Err(e) => return fail(&e.to_string()),
    };
    let settings = match bff::Settings::from_config(&config) {
        Ok(settings) => settings,
        Err(e) => return fail(&e.to_string()),
    };
    let web = match config.web_dir.as_deref().map(Web::new).transpose() {
        Ok(web) => web,
        Err(e) => return fail(&e.to_string()),
    };
    let composition = match applicamp_server::Composition::from_env() {
        Ok(composition) => composition,
        Err(e) => return fail(&e),
    };
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(e) => return fail(&e),
    };
    match runtime.block_on(run(config, secret, &settings, web, &composition)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "server failed");
            ExitCode::FAILURE
        }
    }
}

async fn run(
    config: Config,
    secret: SecretKey,
    settings: &bff::Settings,
    web: Option<Web>,
    composition: &applicamp_server::Composition,
) -> Result<(), Box<dyn std::error::Error>> {
    let registry = Arc::new(
        applicamp_server::registry()
            .map_err(|problems| format!("background work: {}", problems.join("; ")))?,
    );
    let db = db::connect(&config).await?;
    if config.run_migrations {
        applicamp_server::migrate(&db).await?;
        tracing::info!("migrations applied");
    }
    let events = EventBus::new(Arc::clone(&registry));
    let addr: SocketAddr = config
        .bind_addr
        .parse()
        .map_err(|e| format!("APPLICAMP_BIND_ADDR={}: {e}", config.bind_addr))?;
    let state = AppState::new(db.clone(), config, secret).with_events(events.clone());
    let bff = bff::Bff::new(&state, settings)?;
    if !settings.secure {
        tracing::warn!(
            "APPLICAMP_PUBLIC_URL is plain http: cookies are not Secure (development only)"
        );
    }
    let providers: Vec<&str> = bff.providers().iter().map(|p| p.as_str()).collect();
    if providers.is_empty() {
        tracing::warn!("no sign-in provider is configured: nobody can sign in");
    }
    if web.is_none() {
        tracing::info!("APPLICAMP_WEB_DIR is unset: serving the API without the web client");
    }
    match composition.complete_model() {
        Some(model) => tracing::info!(model, "AI role `complete` configured"),
        None => tracing::info!(
            "AI role `complete` is not configured: profile imports take only the LinkedIn \
             export (set APPLICAMP_AI__COMPLETE__BASE_URL and __MODEL)"
        ),
    }

    let (stop, stopped) = watch::channel(false);
    let worker = tokio::spawn(Worker::new(db.clone(), registry, events).run(stopped.clone()));
    let pruner = tokio::spawn(prune_sessions(state.clone(), stopped));

    let app = applicamp_server::app_with(state, &bff, web, composition);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, providers = ?providers, version = applicamp_server::VERSION, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    let _ = stop.send(true);
    let _ = worker.await;
    let _ = pruner.await;
    tracing::info!("stopped");
    Ok(())
}

/// Delete ended sessions every [`PRUNE_EVERY`] until told to stop.
async fn prune_sessions(state: AppState, mut stop: watch::Receiver<bool>) {
    let mut every = tokio::time::interval(PRUNE_EVERY);
    loop {
        tokio::select! {
            _ = every.tick() => {}
            _ = stop.changed() => return,
        }
        match bff::store::prune(&state.db, state.clock.now()).await {
            Ok(0) => {}
            Ok(pruned) => tracing::info!(pruned, "deleted ended sessions"),
            Err(e) => tracing::warn!(error = %e, "could not delete ended sessions"),
        }
    }
}

/// Resolves on Ctrl-C or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("shutting down");
}

fn port_from_env() -> u16 {
    std::env::var("APPLICAMP_BIND_ADDR")
        .ok()
        .and_then(|addr| addr.parse::<SocketAddr>().ok())
        .map_or(DEFAULT_PORT, |addr| addr.port())
}

/// A dependency-free HTTP/1.1 probe of `/healthz`.
fn healthcheck(port: u16) -> ExitCode {
    match probe(port) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => fail("unhealthy"),
        Err(e) => fail(&format!("healthcheck failed: {e}")),
    }
}

fn probe(port: u16) -> std::io::Result<bool> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, HEALTHCHECK_TIMEOUT)?;
    stream.set_read_timeout(Some(HEALTHCHECK_TIMEOUT))?;
    stream.set_write_timeout(Some(HEALTHCHECK_TIMEOUT))?;
    write!(
        stream,
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n",
        applicamp_server::HEALTH_PATH
    )?;
    let mut head = [0_u8; 32];
    let read = stream.read(&mut head)?;
    let status_line = String::from_utf8_lossy(&head[..read]);
    Ok(status_line.starts_with("HTTP/1.1 200"))
}

/// Report `message` on stderr; a failed exit.
pub(crate) fn fail(message: &str) -> ExitCode {
    // Best effort: if stderr is gone there is nowhere left to report to.
    let _ = writeln!(std::io::stderr(), "applicamp: {message}");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_parses() {
        let cli =
            Cli::try_parse_from(["applicamp", "invite", "add", "someone@example.com"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Invite {
                command: invite_cli::InviteCommand::Add { ref email, note: None }
            }) if email == "someone@example.com"
        ));
        let cli = Cli::try_parse_from(["applicamp"]).unwrap();
        assert!(cli.command.is_none(), "serve is the default");
        assert!(Cli::try_parse_from(["applicamp", "invite", "add"]).is_err());
    }
}
