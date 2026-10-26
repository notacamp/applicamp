//! `applicamp ai usage`: what AI cost the whole instance this month, against
//! its monthly limit (`APPLICAMP_AI__BUDGET__INSTANCE_MONTHLY_EUR`), and the
//! per-account limits. Totals and counts only, never who.

use std::{io::Write as _, process::ExitCode};

use applicamp_ai::{AiConfig, Gateway, InstanceSpend};
use applicamp_kernel::{Config, db};
use clap::Subcommand;

use crate::{fail, runtime};

/// The AI commands.
#[derive(Debug, Subcommand)]
pub enum AiCommand {
    /// Print this month's AI spending against the limits.
    Usage,
}

/// Run `command`.
pub fn run(command: AiCommand) -> ExitCode {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(e) => return fail(&e.to_string()),
    };
    let ai = match AiConfig::from_env() {
        Ok(ai) => ai,
        Err(e) => return fail(&e.to_string()),
    };
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(e) => return fail(&e),
    };
    match runtime.block_on(execute(&config, ai, command)) {
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

async fn execute(config: &Config, ai: AiConfig, command: AiCommand) -> Result<Vec<String>, String> {
    let AiCommand::Usage = command;
    let db = db::connect(config)
        .await
        .map_err(|e| format!("could not reach the database: {e}"))?;
    if config.run_migrations {
        applicamp_server::migrate(&db)
            .await
            .map_err(|e| format!("could not apply migrations: {e}"))?;
    }
    let budget = ai.budget;
    let gateway = Gateway::new(ai).map_err(|e| e.to_string())?;
    let spend = gateway
        .instance_spend(&db, chrono::Utc::now())
        .await
        .map_err(|e| e.to_string())?;
    Ok(lines(
        &spend,
        budget.account_daily_eur(),
        budget.account_monthly_eur(),
    ))
}

fn euros(amount: f64) -> String {
    format!("€{amount:.2}")
}

fn limit(amount: Option<f64>) -> String {
    amount.map_or_else(|| "no limit".to_owned(), euros)
}

/// The report.
fn lines(
    spend: &InstanceSpend,
    account_day: Option<f64>,
    account_month: Option<f64>,
) -> Vec<String> {
    let share = spend
        .limit_eur
        .filter(|l| *l > 0.0)
        .map(|l| format!(" ({:.0}%)", spend.month_eur / l * 100.0))
        .unwrap_or_default();
    vec![
        format!(
            "this month: {} of {}{share}",
            euros(spend.month_eur),
            limit(spend.limit_eur)
        ),
        format!("today: {}", euros(spend.today_eur)),
        format!(
            "calls this month: {} by {} accounts",
            spend.calls, spend.accounts
        ),
        format!(
            "per account: {} a day, {} a month",
            limit(account_day),
            limit(account_month)
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_says_spend_against_the_limits() {
        let spend = InstanceSpend {
            month_eur: 12.0,
            today_eur: 0.4,
            limit_eur: Some(15.0),
            calls: 300,
            accounts: 4,
        };
        assert_eq!(
            lines(&spend, Some(0.3), None),
            [
                "this month: €12.00 of €15.00 (80%)",
                "today: €0.40",
                "calls this month: 300 by 4 accounts",
                "per account: €0.30 a day, no limit a month",
            ]
        );
    }
}
