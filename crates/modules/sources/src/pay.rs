//! What boards pay: each board's average stated salary over its relevant
//! jobs, and the order the Boards page lists boards in.
//!
//! - **A job's salary** is the midpoint of its stated range (a single value
//!   counts as itself), in euros a year with the fixed table of the salary
//!   floor ([`crate::requirements::EUR_PER_UNIT`]): per year, or per month
//!   times twelve; pay per hour, day or week, or in a currency outside the
//!   table, is left out.
//! - **Relevant jobs** are engineering roles (by their title) open to where
//!   the person lives (by the board's structured regions; a job whose
//!   regions say nothing counts), listed in the last 30 days.
//! - **A board's ranking key** is its own average when at least
//!   [`MIN_SALARIES`] relevant jobs state a salary. A country-scoped board
//!   with fewer ranks by its country's typical pay (`country_pay.toml`),
//!   marked as a benchmark; any other board with fewer ranks by its own
//!   average, marked "few salaries". A board with neither comes last.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    job_boards::ListedJob,
    regions,
    requirements::{EUR_PER_UNIT, Salary},
};

/// Salaried relevant jobs a board's own average needs to be its key.
pub const MIN_SALARIES: u32 = 10;

/// The country table built in.
pub const COUNTRY_PAY: &str = include_str!("../country_pay.toml");

/// Words of an engineering role's title.
const ENGINEERING: [&str; 16] = [
    "engineer",
    "engineering",
    "developer",
    "programmer",
    "software",
    "sre",
    "devops",
    "architect",
    "backend",
    "frontend",
    "fullstack",
    "firmware",
    "cto",
    "desarrollador",
    "programador",
    "entwickler",
];

/// Whether `title` is an engineering role.
pub fn is_engineering(title: &str) -> bool {
    let lower = title.to_lowercase();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| ENGINEERING.contains(&word))
        || lower.contains("full-stack")
        || lower.contains("back-end")
        || lower.contains("front-end")
}

/// `salary` in euros a year: its midpoint, converted; `None` when it is
/// stated per hour, day or week, in a currency outside the table, or is not
/// a plausible yearly salary.
pub fn eur_per_year(salary: &Salary) -> Option<f64> {
    let amount = match (salary.min, salary.max) {
        (Some(min), Some(max)) if max >= min => f64::midpoint(min, max),
        (Some(one), None) | (None, Some(one)) => one,
        (Some(min), Some(_)) => min,
        (None, None) => return None,
    };
    let currency = salary.currency.as_deref()?.trim().to_ascii_uppercase();
    let rate = EUR_PER_UNIT
        .iter()
        .find(|(code, _)| *code == currency)
        .map(|(_, rate)| *rate)?;
    let months = match salary
        .period
        .as_deref()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "year" | "yearly" | "annual" | "annually" | "per year" => 1.0,
        "month" | "monthly" | "per month" => 12.0,
        _ => return None,
    };
    let eur = amount * rate * months;
    (5_000.0..=2_000_000.0).contains(&eur).then_some(eur)
}

/// A country's typical pay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = CountryBenchmark)]
pub struct Benchmark {
    /// ISO 3166-1.
    pub code: String,
    pub eur_year: f64,
    /// Where the figure is from, and what it measures.
    pub source: String,
    pub url: String,
    pub year: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CountryFile {
    country: Vec<Benchmark>,
}

/// The countries of a country table.
pub fn parse_countries(toml_text: &str) -> Result<Vec<Benchmark>, String> {
    let file: CountryFile = toml::from_str(toml_text).map_err(|e| format!("country pay: {e}"))?;
    Ok(file.country)
}

/// The table built in (empty if it does not read, which a test forbids).
pub static COUNTRIES: LazyLock<Vec<Benchmark>> =
    LazyLock::new(|| parse_countries(COUNTRY_PAY).unwrap_or_default());

/// What a board's place in the order rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = RankingBasis)]
pub enum Basis {
    /// Its own average, from enough salaries.
    Own,
    /// Its own average, from fewer than [`MIN_SALARIES`]: shown greyed.
    Few,
    /// Its country's typical pay: a country-scoped board with few salaries.
    Benchmark,
    /// No salary data: listed last.
    None,
}

/// A board's average salary and its key in the order.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = BoardRanking)]
pub struct Ranking {
    /// The mean of its relevant jobs' stated salaries, in euros a year.
    pub avg_salary_eur_year: Option<f64>,
    /// Relevant jobs that state a salary.
    pub salaried_jobs: u32,
    /// Relevant jobs (engineering, open to where the person lives, 30 days).
    pub relevant_jobs: u32,
    /// `salaried_jobs / relevant_jobs` (0–1).
    pub salary_share: f64,
    pub basis: Basis,
    /// The country benchmark it ranks by, when it does.
    pub benchmark: Option<Benchmark>,
    /// What it is sorted by, in euros a year; none sorts last.
    pub key: Option<f64>,
}

/// The ranking of a board whose recent jobs are `jobs`, for someone in
/// `home` (ISO 3166-1; none known: every engineering job counts); the board
/// is scoped to `board_country`, if it is. `table` is the country table.
pub fn rank(
    jobs: &[&ListedJob],
    home: Option<&str>,
    board_country: Option<&str>,
    table: &[Benchmark],
) -> Ranking {
    let relevant: Vec<&&ListedJob> = jobs
        .iter()
        .filter(|job| job.engineering)
        .filter(|job| home.is_none_or(|c| regions::open_to(&job.regions, c) != Some(false)))
        .collect();
    let salaries: Vec<f64> = relevant
        .iter()
        .filter_map(|job| job.salary_eur_year)
        .collect();
    let relevant_jobs = u32::try_from(relevant.len()).unwrap_or(u32::MAX);
    let salaried_jobs = u32::try_from(salaries.len()).unwrap_or(u32::MAX);
    // Counts of a few thousand: exact as f64.
    let avg = (!salaries.is_empty()).then(|| salaries.iter().sum::<f64>() / salaries.len() as f64);
    let salary_share = if relevant_jobs == 0 {
        0.0
    } else {
        f64::from(salaried_jobs) / f64::from(relevant_jobs)
    };
    let benchmark = board_country
        .and_then(|code| table.iter().find(|b| b.code == code))
        .cloned();
    let (basis, key, benchmark) = if salaried_jobs >= MIN_SALARIES {
        (Basis::Own, avg, None)
    } else if let Some(benchmark) = benchmark {
        (Basis::Benchmark, Some(benchmark.eur_year), Some(benchmark))
    } else if avg.is_some() {
        (Basis::Few, avg, None)
    } else {
        (Basis::None, None, None)
    };
    Ranking {
        avg_salary_eur_year: avg,
        salaried_jobs,
        relevant_jobs,
        salary_share,
        basis,
        benchmark,
        key,
    }
}

/// The order of `a` and `b` on the Boards page: the higher key first, none
/// last.
pub fn order(a: Option<f64>, b: Option<f64>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.total_cmp(&a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paid(min: Option<f64>, max: Option<f64>, currency: &str, period: &str) -> Salary {
        Salary {
            min,
            max,
            currency: Some(currency.into()),
            period: Some(period.into()),
        }
    }

    fn job(title: &str, regions: &[&str], salary: Option<f64>) -> ListedJob {
        ListedJob {
            source_ref: title.into(),
            title: title.into(),
            regions: regions.iter().map(|r| (*r).to_owned()).collect(),
            engineering: is_engineering(title),
            salary_eur_year: salary,
            posted_at: None,
            words: vec![],
        }
    }

    #[test]
    fn salaries_are_midpoints_in_euros_a_year() {
        assert_eq!(
            eur_per_year(&paid(Some(100_000.0), Some(140_000.0), "EUR", "year")),
            Some(120_000.0)
        );
        assert_eq!(
            eur_per_year(&paid(None, Some(5_000.0), "EUR", "month")),
            Some(60_000.0)
        );
        let usd = eur_per_year(&paid(Some(90_000.0), Some(110_000.0), "usd", "annual")).unwrap();
        assert!((usd - 92_000.0).abs() < 1.0, "{usd}");
        assert_eq!(
            eur_per_year(&paid(Some(50.0), Some(70.0), "USD", "hourly")),
            None
        );
        assert_eq!(eur_per_year(&paid(Some(1.0), None, "JPY", "year")), None);
        assert_eq!(
            eur_per_year(&paid(Some(3.0), None, "EUR", "year")),
            None,
            "not a salary"
        );
    }

    #[test]
    fn engineering_roles_by_their_title() {
        assert!(is_engineering("Senior Backend Engineer"));
        assert!(is_engineering("Full-Stack Developer (m/w/d)"));
        assert!(!is_engineering("Senior Accountant"));
        assert!(!is_engineering("Account Executive"));
    }

    #[test]
    fn boards_rank_by_their_average_or_their_country() {
        let table = parse_countries(COUNTRY_PAY).unwrap();
        assert!(table.iter().any(|b| b.code == "PT"));
        assert!(
            table
                .iter()
                .all(|b| b.url.starts_with("https://") && b.year >= 2022)
        );
        // A worldwide board with plenty of salaries: its own average.
        let rich: Vec<ListedJob> = (0..12)
            .map(|i| {
                job(
                    "Staff Engineer",
                    &["Anywhere"],
                    Some(110_000.0 + f64::from(i) * 1_000.0),
                )
            })
            .chain([job("Accountant", &[], Some(1_000_000.0))])
            .chain([job("Backend Engineer", &["United States"], Some(300_000.0))])
            .collect();
        let refs: Vec<&ListedJob> = rich.iter().collect();
        let ranking = rank(&refs, Some("PT"), None, &table);
        assert_eq!(ranking.basis, Basis::Own);
        assert_eq!(ranking.relevant_jobs, 12, "no accountant, nothing US only");
        assert_eq!(ranking.salaried_jobs, 12);
        assert!((ranking.avg_salary_eur_year.unwrap() - 115_500.0).abs() < 0.1);
        // Fewer than ten: greyed, still sorted by it.
        let few = [job("Engineer", &[], Some(80_000.0))];
        let ranking = rank(&few.iter().collect::<Vec<_>>(), None, None, &table);
        assert_eq!((ranking.basis, ranking.key), (Basis::Few, Some(80_000.0)));
        // Country boards with few salaries: their country's pay, Swiss first.
        let swiss = rank(&few.iter().collect::<Vec<_>>(), None, Some("CH"), &table);
        let portuguese = rank(&[], None, Some("PT"), &table);
        assert_eq!(swiss.basis, Basis::Benchmark);
        assert_eq!(portuguese.basis, Basis::Benchmark);
        assert_eq!(order(swiss.key, portuguese.key), std::cmp::Ordering::Less);
        // Nothing at all: last.
        let none = rank(&[], None, None, &table);
        assert_eq!((none.basis, none.key), (Basis::None, None));
        assert_eq!(order(none.key, Some(1.0)), std::cmp::Ordering::Greater);
    }
}
