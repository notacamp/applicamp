//! The boards seeded from the research of 2026-11-04, read through their seed
//! config from a recorded answer of each (`fixtures/boards/`): the config in
//! `job_boards.toml` reads real shapes, and what it cannot read is stated.

use serde_json::Value;
use uuid::Uuid;

use super::{
    mapping::{BoardRef, job_of},
    rss,
};
use crate::{
    job::Job,
    job_boards::{self, SeedBoard},
};

const FOUR_DAY_WEEK: &str = include_str!("fixtures/boards/4dayweek.json");
const REMOTE_FIRST_JOBS: &[u8] = include_bytes!("fixtures/boards/remotefirstjobs.rss");
const REMOTEYEAH: &[u8] = include_bytes!("fixtures/boards/remoteyeah.rss");
const REAL_WORK_FROM_ANYWHERE: &[u8] = include_bytes!("fixtures/boards/realworkfromanywhere.rss");
const RAILS_JOBS: &[u8] = include_bytes!("fixtures/boards/railsjobs.rss");
const ELIXIR_JOBS: &[u8] = include_bytes!("fixtures/boards/elixirjobs.rss");
const GET_ON_BOARD: &str = include_str!("fixtures/boards/getonboard.json");

fn seed_board(slug: &str) -> SeedBoard {
    let boards = job_boards::parse(job_boards::SEED).expect("the seed list parses");
    boards
        .into_iter()
        .find(|b| b.slug == slug)
        .unwrap_or_else(|| panic!("{slug} is in the seed list"))
}

fn board_ref(board: &SeedBoard) -> BoardRef {
    BoardRef {
        id: Uuid::nil(),
        slug: board.slug.clone(),
        name: board.name.clone(),
        drop_expired: false,
    }
}

/// The jobs of a JSON answer, read through the board's config.
fn json_jobs(slug: &str, answer: &str) -> Vec<Job> {
    let board = seed_board(slug);
    let value: Value = serde_json::from_str(answer).expect("the fixture is JSON");
    let pointer = board
        .config
        .items
        .clone()
        .expect("the board names its items");
    let items = value
        .pointer(&pointer)
        .and_then(Value::as_array)
        .expect("the items are a list");
    let reference = board_ref(&board);
    items
        .iter()
        .filter_map(|item| job_of(item, &board.config, &reference))
        .collect()
}

/// The jobs of an RSS answer, read through the board's config.
fn feed_jobs(slug: &str, answer: &[u8]) -> Vec<Job> {
    let board = seed_board(slug);
    let reference = board_ref(&board);
    rss::items(answer)
        .iter()
        .filter_map(|item| job_of(item, &board.config, &reference))
        .collect()
}

/// At least five jobs, each with a title, a link on the web and a company or
/// an apply link.
fn assert_postings(jobs: &[Job]) {
    assert!(jobs.len() >= 5, "{} jobs read", jobs.len());
    for job in jobs {
        assert!(!job.title.trim().is_empty(), "a title");
        assert!(job.url.starts_with("https://"), "{}", job.url);
        let apply = job.board.as_ref().and_then(|b| b.apply_url.as_ref());
        assert!(
            !job.company.trim().is_empty() || apply.is_some(),
            "{} has neither a company nor an apply link",
            job.title
        );
    }
}

#[test]
fn four_day_week_is_read_with_pay_in_dollars_not_cents() {
    let jobs = json_jobs("4dayweek", FOUR_DAY_WEEK);
    assert_postings(&jobs);
    let value: Value = serde_json::from_str(FOUR_DAY_WEEK).unwrap();
    let first = &value["data"][0];
    let cents = first["salary_min"]
        .as_f64()
        .expect("the fixture states a salary");
    let pay = jobs[0]
        .board
        .as_ref()
        .unwrap()
        .pay
        .clone()
        .expect("a salary is read");
    let min = pay.min.expect("a least");
    assert!(
        (min - cents / 100.0).abs() < 0.01,
        "{min} is {cents} cents in dollars"
    );
    assert_eq!(pay.currency.as_deref(), first["salary_currency"].as_str());
    assert_eq!(pay.period.as_deref(), first["salary_period"].as_str());
    assert!(
        jobs[0].url.starts_with("https://4dayweek.io/"),
        "the link back is its own page"
    );
}

#[test]
fn remote_first_jobs_is_read_with_the_company_after_the_last_at() {
    let jobs = feed_jobs("remote-first-jobs", REMOTE_FIRST_JOBS);
    assert_postings(&jobs);
    assert_eq!(jobs[0].company, "Stitch Fix");
    assert_eq!(jobs[0].title, "Lead Engineer, Order Management");
}

#[test]
fn remoteyeah_is_read_from_its_malformed_feed() {
    let jobs = feed_jobs("remoteyeah", REMOTEYEAH);
    assert_postings(&jobs);
    assert_eq!(jobs[0].company, "Sparkland");
}

#[test]
fn real_work_from_anywhere_is_read_from_its_malformed_feed() {
    let jobs = feed_jobs("real-work-from-anywhere", REAL_WORK_FROM_ANYWHERE);
    assert_postings(&jobs);
    assert_eq!(jobs[0].company, "Sourcegraph");
}

#[test]
fn the_rails_job_board_is_read_with_the_company_after_the_last_at() {
    let jobs = feed_jobs("rails-job-board", RAILS_JOBS);
    assert_postings(&jobs);
    assert_eq!(jobs[0].company, "Exponent");
    assert_eq!(jobs[0].title, "Lead Engineer, AI Accounting");
}

#[test]
fn elixir_jobs_reads_titles_and_links_but_no_company() {
    // Its feed gives only the role: the limit is stated in the seed note.
    let jobs = feed_jobs("elixir-jobs", ELIXIR_JOBS);
    assert!(jobs.len() >= 5, "{} jobs read", jobs.len());
    for job in &jobs {
        assert!(!job.title.trim().is_empty());
        assert!(job.url.starts_with("https://"), "{}", job.url);
        assert!(
            job.company.is_empty(),
            "{} names {}",
            job.title,
            job.company
        );
        assert!(job.board.as_ref().unwrap().apply_url.is_none());
    }
}

#[test]
fn get_on_board_is_read_with_pay_in_dollars_a_month() {
    let jobs = json_jobs("get-on-board", GET_ON_BOARD);
    assert_postings(&jobs);
    let pay = jobs[0]
        .board
        .as_ref()
        .unwrap()
        .pay
        .clone()
        .expect("a salary is read");
    assert_eq!(pay.min, Some(4_500.0));
    assert_eq!(pay.max, Some(6_000.0));
    assert_eq!(pay.currency.as_deref(), Some("USD"));
    assert_eq!(pay.period.as_deref(), Some("month"));
    assert!(jobs[0].url.starts_with("https://www.getonbrd.com/jobs/"));
}

#[test]
fn the_boards_that_need_the_owner_are_seeded_with_what_they_need() {
    for slug in ["jobgether", "vuejobs", "elixir-forum"] {
        let board = seed_board(slug);
        assert!(!board.blockers.is_empty(), "{slug} has a blocker");
        assert!(
            board.blockers.iter().all(|b| b.code == "needs_permission"),
            "{slug} needs permission"
        );
        assert!(board.blockers.iter().all(|b| b.source.is_some()));
    }
    let startup_jobs = seed_board("startup-jobs");
    assert!(startup_jobs.blockers.is_empty(), "a key, not a blocker");
    let key = startup_jobs.config.key.expect("it needs a key");
    assert_eq!(key.params, ["api_key"]);
    assert!(
        startup_jobs.config.fields.url.is_some(),
        "its jobs are mapped for when a key is set"
    );
}
