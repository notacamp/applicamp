// Invented pipeline, search and Today answers for the client's tests. No
// real company or person.
import type { Schema } from "~/lib/api";

type Opportunity = Schema<"Opportunity">;
type OpportunitySummary = Schema<"OpportunitySummary">;
type Search = Schema<"Search">;
type SearchEstimate = Schema<"SearchEstimate">;
type Today = Schema<"Today">;

/** One opportunity, whole, as the server sends it. */
export function opportunity(overrides: Partial<Opportunity> = {}): Opportunity {
  return {
    id: "0190a1b2-0000-7000-8000-000000000001",
    title: "Staff Engineer",
    company: "Example Co",
    location: "Remote - Europe",
    work_mode: "remote",
    url: "https://example.com/jobs/1",
    source: "greenhouse",
    match: 82,
    stage: "found",
    next_step: null,
    salary_text: "€90–110k",
    new: true,
    created_at: "2026-10-26T09:00:00Z",
    updated_at: "2026-10-26T09:00:00Z",
    description: "Example Co builds invented software for invented customers.",
    why: ["Your roles name this one", "Remote within Europe, as you asked"],
    gaps: ["On-call duty is not on your profile"],
    reasons_updated: false,
    notes: null,
    posted_at: "2026-10-25T09:00:00Z",
    dismissed: false,
    editable: false,
    sources: [],
    ...overrides,
  };
}

/** The list form of an opportunity. */
export function summary(whole: Opportunity): OpportunitySummary {
  return {
    id: whole.id,
    title: whole.title,
    company: whole.company,
    url: whole.url,
    source: whole.source,
    stage: whole.stage,
    new: whole.new,
    created_at: whole.created_at,
    updated_at: whole.updated_at,
    ...(whole.location === undefined ? {} : { location: whole.location }),
    ...(whole.work_mode === undefined ? {} : { work_mode: whole.work_mode }),
    ...(whole.match === undefined ? {} : { match: whole.match }),
    ...(whole.next_step === undefined ? {} : { next_step: whole.next_step }),
    ...(whole.salary_text === undefined ? {} : { salary_text: whole.salary_text }),
    ...(whole.hidden_reason === undefined ? {} : { hidden_reason: whole.hidden_reason }),
    ...(whole.facts?.region_note === undefined ? {} : { region_note: whole.facts.region_note }),
    ...(whole.source_name === undefined ? {} : { source_name: whole.source_name }),
  };
}

/** A search, finished by default. */
export function search(overrides: Partial<Search> = {}): Search {
  return {
    id: "0190a1b2-0000-7000-8000-0000000000aa",
    status: "finished",
    trigger: "manual",
    started_at: "2026-10-26T09:00:00Z",
    finished_at: "2026-10-26T09:01:00Z",
    fetched: 412,
    kept: 40,
    scored: 40,
    added: 9,
    dropped: 0,
    stopped: null,
    cost_eur: 0.012,
    ...overrides,
  };
}

/** What was spent on AI, and the limits. */
export const SPEND: SearchEstimate["spend"] = {
  today_eur: 0.01,
  day_limit_eur: 0.3,
  month_eur: 0.42,
  month_limit_eur: 3,
};

/** An estimate under the confirm threshold. */
export function estimate(overrides: Partial<SearchEstimate> = {}): SearchEstimate {
  return { jobs: 40, cost_eur: 0.01, confirm: false, spend: SPEND, ...overrides };
}

/** Today's answer at `nextStep`. */
export function todayAnswer(nextStep: Today["next_step"], overrides: Partial<Today> = {}): Today {
  const early = nextStep === "set_up_profile" || nextStep === "answer_goals";
  return {
    profile: { done: nextStep !== "set_up_profile", no_skills: false, strength_hint: null },
    goals: { answered: early ? 0 : 3, total: 11 },
    // Strong, so Today's profile card stays away unless a test asks for it.
    profile_strength: 100,
    next_profile_action: null,
    profile_questions: null,
    new_matches: nextStep === "review_matches" ? 9 : 0,
    latest_search: null,
    next_step: nextStep,
    ...overrides,
  };
}

type Cv = Schema<"Cv">;
type CvDraft = Schema<"CvDraftView">;
type CvLine = Schema<"LineView">;

/** A line of a tailored CV, traced to one source of the profile. */
export function cvLine(id: string, text: string, overrides: Partial<CvLine> = {}): CvLine {
  return {
    id,
    text,
    mark: "as_in_profile",
    sources: [
      {
        id: "w0.h1",
        kind: "highlight",
        text,
        position: 0,
        role: "Senior Engineer · Example Org",
      },
    ],
    flags: [],
    claims: [],
    ...overrides,
  };
}

/** A draft with a reworded line, a verbatim one, a number nobody wrote and a skill off the profile. */
export function cvDraft(overrides: Partial<CvDraft> = {}): CvDraft {
  const wrong = cvLine("l5", "Cut invoice runs to 20 minutes", {
    mark: "reworded",
    claims: ["20"],
    flags: [{ kind: "claim_not_in_sources", claim: "20", cites: ["w0.h1"] }],
  });
  return {
    n: 1,
    status: "draft",
    language: "en",
    created_at: "2026-10-31T10:00:00Z",
    expires_at: "2026-11-07T10:00:00Z",
    contact: {
      name: "Alex Demo",
      email: "alex@example.com",
      location: "Porto, Portugal",
      links: [],
    },
    headline: cvLine("l1", "Backend engineer who ships billing systems"),
    summary: cvLine("l2", "Rust engineer on billing."),
    experience: [
      {
        position: 0,
        title: "Senior Engineer",
        company: "Example Org",
        start_date: "2021-03",
        bullets: [
          cvLine("l3", "Moved invoicing over to PostgreSQL", {
            mark: "reworded",
            claims: ["PostgreSQL"],
            sources: [
              {
                id: "w0.faaaa1111",
                kind: "fact",
                text: "Moved invoicing onto PostgreSQL",
                position: 0,
                role: "Senior Engineer · Example Org",
              },
            ],
          }),
          cvLine("l4", "Led a team of 4 engineers"),
          wrong,
        ],
      },
    ],
    skills: [
      { id: "l6", name: "Rust", flags: [] },
      {
        id: "l7",
        name: "Kubernetes",
        flags: [{ kind: "skill_not_on_profile", skill: "Kubernetes" }],
      },
    ],
    education: [{ institution: "Example University", study_type: "BSc", area: "Computer science" }],
    languages: [{ language: "English", fluency: "Fluent" }],
    left_out: {
      roles: [
        {
          position: 2,
          title: "Junior Developer",
          company: "Tiny Co",
          start_date: "2015",
          end_date: "2017",
          lines: 1,
        },
      ],
      lines: [],
    },
    flagged: 2,
    first_flagged: "l5",
    can_approve: false,
    keywords: {
      items: [
        { label: "Rust", kind: "tech", state: "on_cv" },
        { label: "Kafka", kind: "want", level: "nice", state: "not_in_profile" },
      ],
      on_cv: 1,
      total: 2,
    },
    changes: {
      reworded: 2,
      as_in_profile: 3,
      moved_up: 0,
      edited: 0,
      left_out_roles: 1,
      left_out_lines: 0,
    },
    ...overrides,
  };
}

/** The tailored CV of an opportunity, with no draft unless `draft` is given. */
export function cv(overrides: Partial<Cv> = {}): Cv {
  return {
    opportunity_id: "0190a1b2-0000-7000-8000-000000000001",
    draft: null,
    versions: [],
    can_draft: { allowed: true },
    caps: {
      drafts_here: 0,
      drafts_here_cap: 10,
      drafts_today: 0,
      drafts_today_cap: 20,
      today_eur: 0.01,
      day_limit_eur: 0.3,
      month_eur: 0.42,
      month_limit_eur: 5,
    },
    draft_eur: 0.002,
    ...overrides,
  };
}
