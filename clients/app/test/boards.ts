// Invented job boards, checks and scans for the client's tests. No real
// person; the boards' names are the first boards Applicamp reads, and the
// jobs are made up.
import type { Schema } from "~/lib/api";

type BoardSummary = Schema<"BoardSummary">;
type BoardDetail = Schema<"BoardDetail">;
type BoardRanking = Schema<"BoardRanking">;
type ScanRun = Schema<"ScanRun">;
type ScanLine = Schema<"ScanLine">;

export const HIMALAYAS = "0190a1b2-0000-7000-8000-0000000000b1";
export const WAITING = "0190a1b2-0000-7000-8000-0000000000b2";
export const DECLINED = "0190a1b2-0000-7000-8000-0000000000b3";

const NO_TOTALS = { read: 0, matched: 0, cost_eur: 0 };

/** A ranking on the board's own salaries. */
export function ranking(overrides: Partial<BoardRanking> = {}): BoardRanking {
  return {
    avg_salary_eur_year: 118_000,
    salaried_jobs: 64,
    relevant_jobs: 210,
    salary_share: 0.3,
    basis: "own",
    key: 118_000,
    ...overrides,
  };
}

/** An approved board, read lately. */
export function board(overrides: Partial<BoardSummary> = {}): BoardSummary {
  return {
    id: HIMALAYAS,
    slug: "himalayas",
    name: "Himalayas",
    homepage: "https://himalayas.app",
    kind: "json_api",
    priority: "high",
    status: "approved",
    origin: "seed",
    ranking: ranking(),
    attribution: "Himalayas",
    last_fetch_at: "2026-10-26T21:00:00Z",
    last_fetch_jobs: 100,
    flags: [],
    totals_7d: { read: 412, matched: 9, cost_eur: 0.012 },
    totals_30d: { read: 1200, matched: 20, cost_eur: 0.04 },
    ...overrides,
  };
}

/** A candidate found by discovery, its check done. */
export function waiting(overrides: Partial<BoardSummary> = {}): BoardSummary {
  return board({
    id: WAITING,
    slug: "example-remote",
    name: "Example Remote",
    homepage: "https://remote.example",
    kind: "rss",
    status: "candidate",
    origin: "discovery",
    ranking: ranking({
      basis: "few",
      salaried_jobs: 3,
      relevant_jobs: 20,
      key: 90_000,
      avg_salary_eur_year: 90_000,
      salary_share: 0.15,
    }),
    attribution: "Example Remote",
    last_fetch_at: null,
    last_fetch_jobs: null,
    flags: ["its terms mention scraping or automated access: read them"],
    note: "High priority. Found on the list's Remote section.",
    totals_7d: NO_TOTALS,
    totals_30d: NO_TOTALS,
    ...overrides,
  });
}

/** A declined board. */
export function declined(): BoardSummary {
  return board({
    id: DECLINED,
    slug: "landing-jobs",
    name: "Landing.jobs",
    homepage: "https://landing.jobs",
    priority: "low",
    status: "declined",
    ranking: ranking({
      basis: "none",
      salaried_jobs: 0,
      relevant_jobs: 0,
      avg_salary_eur_year: null,
      key: null,
      salary_share: 0,
    }),
    decline_reason: "Its robots.txt disallows /api/ and its terms forbid scraping.",
    last_fetch_at: null,
    totals_7d: NO_TOTALS,
    totals_30d: NO_TOTALS,
  });
}

/** A board, whole. */
export function detail(summary: BoardSummary, overrides: Partial<BoardDetail> = {}): BoardDetail {
  return {
    board: summary,
    config: {
      endpoint: "https://remote.example/feed.xml",
      fields: { title: "title", url: "link" },
    },
    terms: {
      attribution: summary.attribution,
      link_back: true,
      follow: true,
      apply_unchanged: true,
      no_redistribution: true,
      drop_expired: false,
      no_logo: false,
      max_fetches_per_day: 4,
      notes: "Credit it with a link back.",
      sources: [],
    },
    report: {
      checked_at: "2026-10-26T04:00:00Z",
      robots: {
        host: "remote.example",
        found: true,
        rules: [{ allow: false, pattern: "/private" }],
        endpoint_allowed: true,
        fingerprint: "x",
      },
      terms_url: "https://remote.example/terms",
      terms_sentences: ["You may not scrape this site with automated tools."],
      terms_fingerprint: "y",
      found_kind: "rss",
      found_url: "https://remote.example/feed.xml",
      jsonld: false,
      sample: [
        {
          title: "Senior Backend Engineer",
          company: "Example Co",
          location: "Anywhere",
          url: "https://remote.example/jobs/1",
          regions: ["Anywhere"],
        },
      ],
      remote_share: 1,
      worldwide_share: 0.5,
      language: "en",
      attribution: summary.attribution,
      flags: summary.flags,
      notes: summary.note ?? "",
    },
    endpoint: "https://remote.example/feed.xml",
    approvable: summary.status === "candidate",
    can_manage: true,
    ...overrides,
  };
}

/** One board's line of a run. */
export function line(overrides: Partial<ScanLine> = {}): ScanLine {
  return {
    board_id: HIMALAYAS,
    board_slug: "himalayas",
    board_name: "Himalayas",
    outcome: "read",
    jobs_read: 412,
    passed_requirements: 30,
    scored_by_ai: 30,
    matched: 9,
    ai_cost_eur: 0.004,
    duration_ms: 1200,
    ...overrides,
  };
}

/** A nightly search and what it scanned. */
export function run(overrides: Partial<ScanRun> = {}): ScanRun {
  return {
    kind: "search",
    id: "0190a1b2-0000-7000-8000-0000000000c1",
    at: "2026-10-25T21:00:00Z",
    trigger: "nightly",
    cost_eur: 0.012,
    lines: [
      line(),
      line({
        board_id: WAITING,
        board_slug: "remotive",
        board_name: "Remotive",
        outcome: "skipped_poll_limit",
        jobs_read: 1,
        passed_requirements: 1,
        scored_by_ai: 1,
        matched: 0,
        ai_cost_eur: 0,
      }),
      line({
        board_id: DECLINED,
        board_slug: "jobicy",
        board_name: "Jobicy",
        outcome: "error",
        jobs_read: 0,
        passed_requirements: 0,
        scored_by_ai: 0,
        matched: 0,
        ai_cost_eur: 0,
        error_kind: "rate_limited",
      }),
    ],
    ...overrides,
  };
}

/** A discovery run that checked one board. */
export function discoveryRun(): ScanRun {
  return {
    kind: "discovery",
    id: "0190a1b2-0000-7000-8000-0000000000c2",
    at: "2026-10-26T04:00:00Z",
    trigger: null,
    cost_eur: 0,
    lines: [
      line({
        board_id: WAITING,
        board_slug: "example-remote",
        board_name: "Example Remote",
        outcome: "checked",
        jobs_read: 3,
        passed_requirements: 0,
        scored_by_ai: 0,
        matched: 0,
        ai_cost_eur: 0,
        note: "Feed at /feed.xml · 3 sample jobs · robots allow it",
      }),
    ],
  };
}
