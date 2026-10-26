//! Logs: one JSON object per line (or human-readable for development),
//! filtered by `RUST_LOG` (default `info`).
//!
//! Logs carry ids, kinds and counts, never CV text, email content, names,
//! addresses or tokens (AGENTS.md → Privacy).

use std::io::Write as _;

use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use crate::config::LogFormat;

/// The filter when `RUST_LOG` is unset: our crates at `info`, and the noisy
/// dependencies at `warn`.
pub const DEFAULT_LOG_FILTER: &str = "info,sqlx=warn,hyper=warn,reqwest=warn";

/// Install the global subscriber. A second call does nothing.
pub fn init(format: LogFormat) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));
    let registry = tracing_subscriber::registry().with(filter);
    let result = match format {
        LogFormat::Json => registry
            .with(
                fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_current_span(false)
                    .with_span_list(false),
            )
            .try_init(),
        LogFormat::Pretty => registry.with(fmt::layer()).try_init(),
    };
    if result.is_err() {
        // Already installed (tests): keep the first.
        return;
    }
    install_panic_hook();
}

/// What is reported about a panic: where it happened, never its message,
/// which can quote the data that caused it.
pub fn withheld_panic_report(info: &std::panic::PanicHookInfo<'_>) -> String {
    let location = info.location().map_or_else(
        || "an unknown location".to_owned(),
        |l| format!("{}:{}", l.file(), l.line()),
    );
    format!("panicked at {location} (message withheld: it may hold personal data)")
}

/// Replace the default panic hook with one that logs
/// [`withheld_panic_report`].
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let report = withheld_panic_report(info);
        if tracing::dispatcher::has_been_set() {
            tracing::error!(target: "applicamp::panic", "{report}");
        } else {
            let _ = writeln!(std::io::stderr(), "{report}");
        }
    }));
}
