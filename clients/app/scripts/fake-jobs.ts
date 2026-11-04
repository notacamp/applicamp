// A tiny fake of the outside world a job search reads, for the e2e stack and
// a laptop demo: a remote job board shaped like Himalayas' JSON API (the job
// board the search reads), a Greenhouse company board (off in searches by
// default, kept for watchers), and an OpenAI-compatible model that scores
// every job it is shown. Never for production: its jobs and scores are
// invented.
//
//   node scripts/fake-jobs.ts
//
// Settings (environment):
//   FAKE_JOBS_PORT   port to listen on (9020)
//   FAKE_JOBS_HOST   address to bind (127.0.0.1; 0.0.0.0 in a container)
//
// Point the server at it with (deploy/fake-jobs/job_boards.toml names the
// fake board, at http://fake-jobs:9020; for a laptop, copy it with
// localhost:9020):
//   APPLICAMP_SOURCES__JOB_BOARDS_FILE=<that job_boards.toml>
//   APPLICAMP_SOURCES__ALLOW_PRIVATE_HOSTS=true
//   APPLICAMP_SOURCES__DISCOVERY=false
//   APPLICAMP_SOURCES__BOARD_OWNERS=<your address, to approve boards>
//   APPLICAMP_AI__COMPLETE__BASE_URL=http://localhost:9020/v1
//   APPLICAMP_AI__COMPLETE__MODEL=fake/model
//
// Boards waiting for the owner (#26), each at its own host name (aliases of
// this fake on the e2e network), every one with robots rules that allow it:
//   seeded-board.test     a JSON API the seed list names: checked at start,
//                         then approvable;
//   keyed-board.test      a JSON API that answers only with the key `e2e-key`;
//   clean-site.test       a JSON API with five jobs from the last week and a
//                         terms page with nothing against apps (and a
//                         Content-Signal line): the check at start approves
//                         it by itself, and the model's reading of its terms
//                         (the fake answers "silent") tags it (#75);
//   ask-first-site.test   the same, but its terms ask for written consent
//                         first: it waits, with that sentence quoted;
//   feed-site-<p>.test    a site with no config whose homepage links its RSS
//                         feed: the check proposes a config (one per
//                         Playwright project, desktop and phone).
//
// The company and job facts of a tailored CV (#34): example-co.test is a
// company's own site (an alias of this fake on the e2e network, so a pasted
// posting at http://example-co.test:9020/jobs/staff-engineer is a link to a
// site that is not a job board). Its robots.txt forbids /private, which the
// home page links to anyway and which must never be read; the model reads the
// pages it is shown and answers with facts quoting them (plus one whose quote
// is on no page, which the server drops), then with what the posting wants
// and up to three questions; an answer about a job becomes facts quoting it.
//
// The tailored CV (#37): the model answers the CV prompt ("You write a
// one-page CV…") from the sources it is shown, by fixed rules: a bullet
// reworded from a kept fact, a highlight word for word, a bullet with a number
// no source says (it is flagged), a bullet that cites a company fact outside
// the summary (flagged), a summary that cites a company fact, a skill that is
// not on the profile (flagged), and every position after the second left out.
// The fix prompt ("You fix the lines of a CV…", #73) rewrites the company-fact
// line to its own source's words and asks about any other, proposing the line
// as a first-person fact.
//
// The cover letter (#45): the model answers the letter prompt ("You write a
// cover letter…") by fixed rules: a paragraph citing a company fact and a
// highlight, one with a superlative nobody wrote (flagged), one that cites
// only a company fact yet says what the person did (flagged), a closing that
// claims nothing, an answer to each of the posting's application questions it
// can support, and an empty one for a notice period, salary, visa or move
// (the profile cannot say). The posting lists two application questions.
//
// Company pages (#87): the hiring systems, each at its own host name (aliases of
// this fake on the e2e network, and the address the server reads that system
// at, `<system>.hiring.test`), every one with robots rules that allow it. The
// jobs of clean-site.test carry apply links on the systems' public addresses,
// so the check at start registers a company page for each:
//   greenhouse.hiring.test  clean: the app approves the system, then the
//                           company page `harbour-stays` (a board with recent
//                           jobs, read through the Greenhouse API);
//   lever.hiring.test       robots.txt says `ai-train=no`: the system waits for
//                           the owner (the blocker `needs_choice`), and the
//                           company page `northlight-labs` waits with it;
//   workable.hiring.test    clean: the system, then the company page
//                           `wayfarer-co` (the Workable widget answer below).
// The other four systems answer with clean rules and no jobs.
//
// Company watchers (#20): three companies' own sites per Playwright project
// (desktop, phone) and one to show by hand (demo), each at its own host name:
//   northwind-<p>.test  a company whose /careers links its Greenhouse board
//                       `northwind-<p>`. The board's first read lists two jobs;
//                       every later one adds two: a Platform Engineer that
//                       fits, and a Senior Backend Engineer that is open only to
//                       Germany and Austria (dropped for the requirements). Two
//                       jobs of this company are what the remote board lists for
//                       the skill `Watch-<p>`, so a profile that lists it has
//                       matches from Northwind Stays, and its site is known;
//   contoso-<p>.test    a company whose /careers holds schema.org JobPosting data;
//   tailspin-<p>.test   a company with no job board anywhere.
//
// Every list answers with four new jobs (new ids each time); a board keeps to
// its polling interval, so a search within it reads the last answer. The
// fourth is remote only from Germany or Austria, in the words of the posting
// that made #19: the model reads that fact and the search drops it ("… 1
// dropped for your requirements") for someone who works remote only from
// Portugal.
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";

const env = process.env;
const port = Number(env["FAKE_JOBS_PORT"] ?? "9020");
const host = env["FAKE_JOBS_HOST"] ?? "127.0.0.1";

/** The invented roles every board lists; the last is open only to Germany and Austria. */
const ROLES = [
  "Staff Engineer",
  "Founding Engineer",
  "Platform Engineer",
  "Staff Engineer (m/w/d)",
] as const;

/** The role at `index` is the one remote only from Germany or Austria. */
const DACH = ROLES.length - 1;

let lists = 0;

interface FakeJob {
  id: number;
  title: string;
  updated_at: string;
  first_published: string;
  location: { name: string };
  absolute_url: string;
  company_name: string;
}

function job(id: number, title: string): FakeJob {
  const now = new Date().toISOString();
  return {
    id,
    title,
    updated_at: now,
    first_published: now,
    location: { name: title === ROLES[DACH] ? "Berlin" : "Remote - Europe" },
    absolute_url: `https://example.com/jobs/${String(id)}`,
    company_name: "Example Co",
  };
}

/** HTML as Greenhouse sends it: entity-escaped. */
function escaped(html: string): string {
  return html
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function json(response: ServerResponse, status: number, body: unknown) {
  response.writeHead(status, { "content-type": "application/json", "cache-control": "no-store" });
  response.end(JSON.stringify(body));
}

async function body(request: IncomingMessage): Promise<string> {
  let text = "";
  for await (const chunk of request) {
    text += String(chunk);
    if (text.length > 2 * 1024 * 1024) {
      break;
    }
  }
  return text;
}

/** A posting in the scoring request's `<jobs>` block. */
interface Posting {
  ref?: string;
  posting?: string;
}

/** The postings of the last user message's `<jobs>` block. */
function postingsOf(content: string): Posting[] {
  const block = content.split("<jobs>", 2)[1]?.split("</jobs>", 1)[0] ?? "[]";
  try {
    const parsed: unknown = JSON.parse(block);
    return Array.isArray(parsed) ? (parsed as Posting[]) : [];
  } catch {
    return [];
  }
}

/** The eligibility facts of a posting, as a model reads them from its words. */
function factsOf(posting: Posting) {
  return (posting.posting ?? "").includes("Deutschland oder Österreich")
    ? {
        remote: "yes",
        open_to_candidate: "no",
        region_note: "Germany or Austria only",
        working_languages: ["en"],
        salary: null,
      }
    : {
        remote: "yes",
        open_to_candidate: "yes",
        region_note: "Europe",
        working_languages: ["en"],
        salary: null,
      };
}

/** The text between `<tag>` and `</tag>` in `content`. */
function between(content: string, tag: string): string {
  return content.split(`<${tag}>`, 2)[1]?.split(`</${tag}>`, 1)[0]?.trim() ?? "";
}

/** `JSON.parse` of the text between `<tag>` tags, or `fallback`. */
function parsedBetween<T>(content: string, tag: string, fallback: T): T {
  try {
    return JSON.parse(between(content, tag)) as T;
  } catch {
    return fallback;
  }
}

/** What the portrait's sources look like in the prompt (`<sources>`). */
interface AboutSource {
  id: string;
  kind: string;
  text: string;
  url?: string;
}

/**
 * "Tell me about you" (#43), the portrait (system prompt "You write what a
 * job-hunting app knows about one person…"): a known sentence for each
 * position, one `known` sentence whose claim no source makes (the server must
 * downgrade it to an assumption), a told sentence for each fact the person
 * wrote, assumptions about the roles with their reasons, and an assumption
 * for each fact read on an employer's own website.
 */
function aboutPortrait(userContent: string) {
  const sources = parsedBetween<AboutSource[]>(userContent, "sources", []);
  const positions = parsedBetween<
    { index: number; title?: string | null; company?: string | null }[]
  >(userContent, "positions", []);
  const sentence = (
    text: string,
    kind: "known" | "told" | "assumed",
    cited: string[],
    why: string,
    topic: string,
    position: number | null,
    claim = "",
  ) => ({ text, kind, sources: cited, why, claim, topic, position });
  const sentences = positions.slice(0, 3).flatMap((role) => {
    const title = role.title ?? "an engineer";
    const company = role.company ?? "an employer";
    const lines = [
      sentence(
        `You were ${title} at ${company}.`,
        "known",
        [`w${String(role.index)}`],
        "",
        "context",
        null,
      ),
      sentence(
        `I assume you led a small team at ${company}, probably 3 to 6 people.`,
        "assumed",
        [],
        `${title} often means leading people. Your profile doesn’t say how many.`,
        "people",
        role.index,
        `Led a small team at ${company} (3 to 6 people)`,
      ),
    ];
    if (role.index === 0) {
      lines.push(
        sentence(
          `Companies like ${company} usually run on bookings, payments and building access, so I assume you worked on some of those.`,
          "assumed",
          [],
          "That is what platforms like this are made of. Your role only says what it says.",
          "your_part",
          role.index,
          `Worked on bookings, payments and building access at ${company}`,
        ),
      );
    }
    return lines;
  });
  const told = sources.filter((source) => source.kind === "told").slice(0, 2);
  const research = sources.filter((source) => source.kind === "research").slice(0, 3);
  return {
    sentences: [
      ...sentences.slice(0, 1),
      // No source says it: the claim check must catch it.
      sentence(
        "You also run Kubernetes clusters in production.",
        "known",
        ["w0"],
        "",
        "stack_scale",
        null,
      ),
      ...told.map((source) =>
        sentence(`You told me: ${source.text}`, "told", [source.id], "", "your_part", null),
      ),
      ...research.map((source) =>
        sentence(
          `${source.text}, according to its own website. I assume that is the company you worked for.`,
          "assumed",
          [source.id],
          "Read on the company’s own website, so I didn’t need to ask you. Tell me if your part was something else.",
          "context",
          0,
          `${source.text} (read on its own website)`,
        ),
      ),
      ...sentences.slice(1),
    ],
  };
}

const ABOUT_TECH = ["Rails", "Elixir", "Ruby", "TypeScript", "Postgres", "Terraform"] as const;

/**
 * "Tell me about you", "Anything else" (system prompt "You read what a person
 * wrote about themselves…"): changes across the summary, the headline,
 * skills, background, goals and a fact about the first position, each quoting
 * the text; the summary also says one thing the text does not (the server
 * must flag it).
 */
function aboutChanges(userContent: string) {
  const text = between(userContent, "text");
  const change = (
    field: string,
    value: string,
    quote: string,
    sentence: string,
    position: number | null = null,
    area = "context",
  ) => ({ field, value, position, area, quote, sentence });
  const found = (pattern: RegExp) => pattern.exec(text)?.[0];
  const first = text.split(/(?<=[.!?])\s+/, 1)[0]?.replace(/[.!?]+$/, "") ?? "";
  const role = found(/senior rails role/i);
  const background = text
    .split(/(?<=[.!?])\s+/)
    .map((part) => part.replace(/[.!?]+$/, ""))
    .find((part) => /before|\d+ years/i.test(part));
  const led = found(/led \d+ engineers/i);
  const changes = [
    ...(text === ""
      ? []
      : [change("summary", `${first}. Now building hospitality products at Acme.`, first, "")]),
    ...(role === undefined
      ? []
      : [change("headline", "Senior Rails engineer", role, `You describe yourself as a ${role}.`)]),
    ...ABOUT_TECH.flatMap((name) => {
      const written = found(new RegExp(String.raw`\b${name}\b`, "i"));
      return written === undefined ? [] : [change("skill", name, written, "")];
    }),
    ...(background === undefined
      ? []
      : [change("background", background, background, `${background}.`)]),
    ...(role === undefined
      ? []
      : [change("goal_role", "Senior Rails engineer", role, `You want a ${role}.`)]),
    ...(led === undefined
      ? []
      : [change("role_fact", led.replace(/^l/, "L"), led, `You ${led}.`, 0, "people")]),
  ];
  return { changes };
}

/** The outcomes the fake offers for a "what changed" question (all invented). */
const RESULT_CHOICES = ["Fewer manual steps", "Faster month-end close", "More shops onboarded"];

/** The languages the fake offers, as a `language` change writes them. */
const LANGUAGE_CHOICES = ["English (fluent)", "Portuguese (native)", "Spanish (conversational)"];

/** The choices the fake offers: what a picked one looks like in an answer. */
const QUESTION_CHOICES: ReadonlySet<string> = new Set([
  "Bookings",
  "Payments",
  "Door and building access",
  "None of these",
  "1 to 2",
  "3 to 6",
  "7 to 15",
  "More than 15",
  ...RESULT_CHOICES,
  ...LANGUAGE_CHOICES,
]);

/** The gap a question is about (`<gap>` of the prompt). */
interface AskedGap {
  kind?: string;
  area?: string | null;
  basis?: string;
  names?: string[];
}

/** One position as the question call shows it (`<role>`). */
interface AskedRole {
  index?: number;
  title?: string | null;
  company?: string | null;
}

/**
 * "Tell me about you", the questions about the gaps (#44): the model words one
 * question about the one gap it is shown (system prompt "You word one question
 * that a job-hunting app asks…"), always saying what was assumed or what is
 * not known, in its `why`, tagged, with choices for a closed question. The
 * server has already decided what to ask.
 */
function aboutQuestion(userContent: string) {
  const gap = parsedBetween<AskedGap>(userContent, "gap", {});
  const role = parsedBetween<AskedRole | null>(userContent, "role", null);
  const assumption = parsedBetween<{ text?: string; why?: string } | null>(
    userContent,
    "assumption",
    null,
  );
  const company = role?.company ?? "your last employer";
  const title = role?.title ?? "your role";
  const names = gap.names ?? [];
  const basis = gap.basis ?? "";
  const ask = (question: string, why: string, tag: string, choices: string[] = []) => ({
    question,
    why,
    tag,
    choices,
  });
  switch (gap.kind) {
    case "assumed": {
      return ask(
        `Which of these did you build at ${company}?`,
        `I assumed this: ${assumption?.text ?? basis} ${assumption?.why ?? ""}`.trim(),
        "assumed",
        ["Bookings", "Payments", "Door and building access", "None of these"],
      );
    }
    case "results": {
      // Plain words, and example outcomes that fit the company to pick from.
      return ask(
        role?.title === undefined || role.title === null
          ? `At ${company} you worked on a ledger product for small shops. What got better because of your work?`
          : `At ${company} you were ${/^[aeiou]/i.test(title) ? "an" : "a"} ${title.toLowerCase()} on a ledger product for small shops. What got better because of your work?`,
        `${basis} CVs that show a difference get more replies.`,
        "unknown",
        RESULT_CHOICES,
      );
    }
    case "team_size": {
      return ask(
        `How many people did you lead at ${company}?`,
        `Your title was ${title}, which usually means leading 3 to 6 engineers. Your profile doesn’t say, and recruiters ask.`,
        "assumed",
        ["1 to 2", "3 to 6", "7 to 15", "More than 15"],
      );
    }
    case "skills_found": {
      return ask(
        `Which of these do you use?`,
        `Your roles mention ${names.join(", ")}, but they aren’t in your skills.`,
        "unknown",
        [...names, "None of these"],
      );
    }
    case "languages": {
      return ask(
        "Which languages do you speak, and how well?",
        `${basis} Postings say which languages the work needs.`,
        "unknown",
        LANGUAGE_CHOICES,
      );
    }
    case "claim": {
      return ask(
        `Where did you use ${names[0] ?? "it"}?`,
        `You mentioned ${names[0] ?? "it"}, but none of your roles do.`,
        "told",
      );
    }
    default: {
      return ask(
        `What should I know about your work at ${company}?`,
        `${basis} I don’t know it yet.`,
        "unknown",
      );
    }
  }
}

/**
 * An answer to one of those questions (system prompt "You read a person's
 * answer to one question about their own profile…"): a fact on the position
 * asked about, quoting the answer. A number is a result; picked choices are
 * what the person built; a team size also names the company, which the
 * answer never wrote (the server must flag it).
 */
function aboutAnswer(userContent: string) {
  const words = between(userContent, "text");
  const about = parsedBetween<{ index?: number | null; area?: string | null }>(
    userContent,
    "about",
    {},
  );
  const held = parsedBetween<{ positions?: { index: number; company?: string | null }[] }>(
    userContent,
    "profile",
    {},
  );
  const company = held.positions?.find((role) => role.index === about.index)?.company ?? "";
  // The server puts what was typed first and the choices picked after it.
  const lines = words.split("\n");
  const choices = lines.filter((line) => QUESTION_CHOICES.has(line) && line !== "None of these");
  const first = lines.find((line) => !QUESTION_CHOICES.has(line)) ?? choices[0] ?? "";
  const change = (value: string, quote: string, area: string, sentence: string) => ({
    field: about.index === null || about.index === undefined ? "skill" : "role_fact",
    value,
    position: about.index ?? null,
    area,
    quote,
    sentence,
  });
  // A languages question: each picked language is a `language` change.
  const languages = choices.filter((pick) => LANGUAGE_CHOICES.includes(pick));
  if (languages.length > 0) {
    return {
      changes: languages.map((pick) => ({
        ...change(pick, pick, "context", `You speak ${pick}.`),
        field: "language",
        position: null,
      })),
    };
  }
  if (about.area === "people") {
    return {
      changes: [
        change(
          `Led ${first.replace(/[.!?]+$/, "")} at ${company}`,
          first,
          "people",
          `You led ${first.replace(/[.!?]+$/, "")}.`,
        ),
      ],
    };
  }
  if (about.area === "results" && /\d/.test(words)) {
    const value = first.replace(/[.!?]+$/, "");
    return { changes: [change(value, first, "results", `You told me: ${value}.`)] };
  }
  if (about.area === "results" && choices.length > 0) {
    // Picked outcomes are the person's own words: one fact each, no number added.
    return {
      changes: choices.map((pick) => change(pick, pick, "results", `You told me: ${pick}.`)),
    };
  }
  if (choices.length > 0 && about.index !== null && about.index !== undefined) {
    const value = `Worked on ${choices.join(", ").toLowerCase()}`;
    return {
      changes: [
        change(
          value,
          choices.join("\n"),
          about.area ?? "your_part",
          `You worked on ${choices.join(", ").toLowerCase()}.`,
        ),
      ],
    };
  }
  const tech = ABOUT_TECH.find((name) => new RegExp(String.raw`\b${name}\b`, "i").test(words));
  if (tech !== undefined && (about.index === null || about.index === undefined)) {
    return { changes: [change(tech, tech, "context", "")] };
  }
  return { changes: [] };
}

/** An OpenAI-compatible chat completion whose message is `content` (as JSON text). */
function completion(content: unknown) {
  return {
    id: `fake-${String(Date.now())}`,
    object: "chat.completion",
    model: "fake/model",
    choices: [
      {
        index: 0,
        message: { role: "assistant", content: JSON.stringify(content) },
        finish_reason: "stop",
      },
    ],
    usage: { prompt_tokens: 1200, completion_tokens: 300, total_tokens: 1500, cost: 0.0004 },
  };
}

/** What the LinkedIn check-up's prompt shows (`<linkedin>`, `<goals>`). */
interface LinkedInShown {
  headline?: string | null;
  summary?: string | null;
  positions?: { index: number; title?: string | null; company?: string | null }[];
  skills?: string[];
}

/**
 * The LinkedIn check-up (#84, system prompt "You review one person's LinkedIn
 * profile…"): a headline that keeps LinkedIn's own words and names the role
 * wanted (clean), an About text with a buzzword (flagged), a description of
 * the first position that adds a tool no source names (flagged, to edit), a
 * skill the profile has and LinkedIn lacks (clean) and one that cites nothing
 * (flagged), and education left as it is.
 */
function linkedinReview(userContent: string) {
  const sources = parsedBetween<AboutSource[]>(userContent, "sources", []);
  const goals = parsedBetween<{ roles?: string[] }>(userContent, "goals", {});
  const linkedin = parsedBetween<LinkedInShown>(userContent, "linkedin", {});
  const role = goals.roles?.[0] ?? null;
  const has = (id: string) => sources.some((source) => source.id === id);
  const text = (id: string) => sources.find((source) => source.id === id)?.text ?? "";
  const headlineNow = linkedin.headline ?? "";
  const headline = {
    field: "headline",
    text:
      role !== null && has("goals")
        ? `${headlineNow} · open to ${role} roles`.trim()
        : headlineNow || text("p.headline"),
    sources: [
      ...(headlineNow === "" ? ["p.headline"] : ["l.headline"]),
      ...(has("goals") ? ["goals"] : []),
    ],
    why: "Says the role you want next, in your own words.",
  };
  const summary = {
    field: "summary",
    text: `Passionate about this work. ${text("w0")}`.trim(),
    sources: ["w0"],
    why: "An About text that says what you do today.",
  };
  const description = {
    field: "position_description",
    text: `${text("w0")} Also runs Kubernetes.`.trim(),
    sources: ["w0"],
    why: "A description that says what the role was.",
  };
  const onLinkedin = new Set((linkedin.skills ?? []).map((skill) => skill.toLowerCase()));
  const known = text("skills")
    .replace(/^Skills:\s*/, "")
    .replace(/\.$/, "")
    .split(",")
    .map((skill) => skill.trim())
    .filter((skill) => skill !== "" && !onLinkedin.has(skill.toLowerCase()));
  const skills = [
    ...known.slice(0, 1).map((skill) => ({
      field: "skill",
      text: skill,
      sources: ["skills"],
      why: "Your profile has it; LinkedIn does not.",
    })),
    { field: "skill", text: "Kubernetes", sources: [], why: "Often asked for in such roles." },
  ];
  const first = linkedin.positions?.[0];
  return {
    overall: `Your headline says what you do today, not the ${role ?? "role"} you want next. Your About text is ${
      (linkedin.summary ?? "") === "" ? "empty" : "short"
    }.`,
    findings: [
      {
        part: "headline",
        position: null,
        state: "could_be_better",
        why: "A recruiter for that role does not see it named here.",
        proposals: [headline],
      },
      {
        part: "summary",
        position: null,
        state: (linkedin.summary ?? "") === "" ? "missing" : "could_be_better",
        why: "The About text is where you say what you want next.",
        proposals: [summary],
      },
      ...(first === undefined
        ? []
        : [
            {
              part: "position",
              position: first.index,
              state: "could_be_better",
              why: "The description says little about what you did.",
              proposals: [description],
            },
          ]),
      {
        part: "skills",
        position: null,
        state: "could_be_better",
        why: "Recruiters filter on skills; some of yours are missing here.",
        proposals: skills,
      },
      {
        part: "education",
        position: null,
        state: "good",
        why: "It says what it needs to.",
        proposals: [],
      },
    ],
  };
}

/**
 * Answers a chat completion as the model would, by what it is asked, or (any
 * other) the scoring of every posting of the last user message, with its
 * facts.
 */
async function complete(request: IncomingMessage, response: ServerResponse) {
  let postings: Posting[] = [];
  try {
    const parsed = JSON.parse(await body(request)) as {
      messages?: { role?: string; content?: string }[];
    };
    const messages = parsed.messages ?? [];
    const system = messages.find((message) => message.role === "system")?.content ?? "";
    const user = messages.findLast((message) => message.role === "user");
    if (system.startsWith("You write what a job-hunting app knows about one person")) {
      json(response, 200, completion(aboutPortrait(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You review one person's LinkedIn profile")) {
      json(response, 200, completion(linkedinReview(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You read what a person wrote about themselves")) {
      json(response, 200, completion(aboutChanges(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You word one question that a job-hunting app asks")) {
      json(response, 200, completion(aboutQuestion(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You read a person's answer to one question about their own profile")) {
      json(response, 200, completion(aboutAnswer(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You read the terms of a job board")) {
      json(response, 200, completion(termsReading(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You read pages from a company")) {
      json(response, 200, completion(companyFacts(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You compare a job posting")) {
      json(response, 200, completion(jobAnalysis(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You write a one-page CV")) {
      json(response, 200, completion(tailoredCv(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You fix the lines of a CV")) {
      json(response, 200, completion(cvFixes(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You write a cover letter")) {
      json(response, 200, completion(coverLetter(user?.content ?? "")));
      return;
    }
    if (system.startsWith("You read a person's answer to one question about a job")) {
      json(response, 200, completion(jobAnswer(user?.content ?? "")));
      return;
    }
    postings = postingsOf(user?.content ?? "").filter((posting) => (posting.ref ?? "") !== "");
  } catch {
    json(response, 400, { error: { message: "not JSON" } });
    return;
  }
  const jobs = postings.map((posting, index) => ({
    ref: posting.ref,
    match: 90 - ((index * 7) % 21),
    why: ["Your roles name this one", "Remote within Europe, as you asked"],
    gaps: ["The posting asks for on-call duty; your profile does not mention it"],
    ...factsOf(posting),
  }));
  json(response, 200, completion({ jobs }));
}

/** The company a remote board job is of; its own site, when the board gives it. */
interface Employer {
  name: string;
  slug: string;
  url?: string;
}

const EXAMPLE_CO: Employer = { name: "Example Co", slug: "example-co" };

/** A job as Himalayas' API lists it (`/jobs/api`), with the posting's words. */
function remoteJob(id: number, title: string, employer: Employer = EXAMPLE_CO) {
  const dach = title === ROLES[DACH];
  const slug = `${title.toLowerCase().replaceAll(/[^a-z0-9]+/g, "-")}-${String(id)}`;
  const seconds = Math.floor(Date.now() / 1000);
  return {
    title,
    excerpt: `${employer.name} builds invented software for invented customers.`,
    companyName: employer.name,
    companySlug: employer.slug,
    ...(employer.url === undefined ? {} : { companyUrl: employer.url }),
    employmentType: "Full Time",
    minSalary: dach ? 70_000 : 110_000,
    maxSalary: dach ? 90_000 : 140_000,
    currency: "EUR",
    salaryPeriod: "annual",
    seniority: ["Senior"],
    locationRestrictions: [],
    timezoneRestrictions: [],
    categories: ["Software-Engineer"],
    description: `<p>${employer.name} builds invented software for invented customers.</p><ul><li>Work with a small team</li>${
      dach
        ? "<li>Standort: Remote-first aus Deutschland oder Österreich</li>"
        : "<li>Remote within Europe</li>"
    }</ul>`,
    pubDate: seconds,
    expiryDate: seconds + 30 * 24 * 3600,
    applicationLink: `https://example.com/apply/${String(id)}?utm_source=api`,
    guid: `https://example.com/remote-board/jobs/${slug}`,
  };
}

/** A few invented jobs of a waiting board, as a JSON API lists them. */
function waitingJobs(site: string) {
  return {
    jobs: ["Senior Backend Engineer", "Staff Platform Engineer"].map((title, index) => ({
      id: `${site}-${String(index + 1)}`,
      title,
      company: "Example Co",
      url: `https://example.com/${site}/jobs/${String(index + 1)}`,
      location: "Remote",
    })),
  };
}

/** The sentence the sites that approve themselves have in their terms. */
const CLEAN_TERMS = "Jobs here may be linked to from other sites, with attribution to Clean Site.";
/** The sentence of the site that asks first. */
const ASK_FIRST_TERMS =
  "You may not access the site by automated means without our prior written consent.";

/**
 * Where the first three jobs of clean-site.test say to apply: a company page
 * on a hiring system's public address (never fetched: the stack reads the
 * system at its `.hiring.test` host).
 */
const APPLY_LINKS = [
  "https://boards.greenhouse.io/harbour-stays/jobs/4001",
  "https://jobs.lever.co/northlight-labs/0b6d1c2e-4f1c-4a55-8a1d-2f1c0b6d1c2e",
  "https://apply.workable.com/wayfarer-co/j/WF10A/",
] as const;

/** Five invented jobs posted in the last week, as the sites of #75 list them. */
function recentJobs(site: string) {
  const titles = [
    "Hotel Operations Lead",
    "Backend Engineer",
    "Travel Experience Designer",
    "Platform Engineer",
    "Guest Services Manager",
  ];
  return {
    jobs: titles.map((title, index) => ({
      id: `${site}-${String(index + 1)}`,
      title,
      company: [
        "Harbour Stays",
        "Northlight Labs",
        "Wayfarer Co",
        "Northlight Labs",
        "Harbour Stays",
      ][index],
      url:
        site === "clean-site.test" && APPLY_LINKS[index] !== undefined
          ? APPLY_LINKS[index]
          : `https://example.com/${site}/jobs/${String(index + 1)}`,
      location: "Remote - Europe",
      posted_at: new Date(Date.now() - (index + 1) * 24 * 3600 * 1000).toISOString(),
    })),
  };
}

/** The terms page of a site of #75. */
function termsPage(site: string): string {
  const sentence = site === "ask-first-site.test" ? ASK_FIRST_TERMS : CLEAN_TERMS;
  return `<html lang="en"><body><h1>Terms</h1><p>${sentence}</p></body></html>`;
}

/**
 * The terms reader (system prompt "You read the terms of a job board…"): the
 * sentences it is shown are a JSON array between <terms> tags. Written consent
 * first is `ask_first`, quoting that sentence; anything else is `silent`.
 * Both are tagged for a hospitality-and-tech board in Europe.
 */
function termsReading(user: string) {
  const block = /<terms>\s*([\s\S]*?)\s*<\/terms>/.exec(user)?.[1] ?? "[]";
  let sentences: string[] = [];
  try {
    sentences = (JSON.parse(block) as unknown[]).filter(
      (sentence): sentence is string => typeof sentence === "string",
    );
  } catch {
    sentences = [];
  }
  const asks = sentences.find((sentence) => /written consent|written permission/i.test(sentence));
  return {
    verdict: asks === undefined ? "silent" : "ask_first",
    quote: asks ?? "",
    kinds: ["hospitality-travel", "tech"],
    where: ["europe"],
    remote: "mostly-remote",
    specialities: [],
  };
}

/** The same jobs as an RSS feed. */
function waitingFeed(site: string): string {
  const items = waitingJobs(site)
    .jobs.map(
      (job) =>
        `<item><title>${job.title}</title><link>${job.url}</link><guid>${job.id}</guid>` +
        `<description>Invented.</description></item>`,
    )
    .join("");
  return `<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel>${items}</channel></rss>`;
}

function text(response: ServerResponse, type: string, body: string, status = 200) {
  response.writeHead(status, { "content-type": type, "cache-control": "no-store" });
  response.end(body);
}

/** A waiting board's site, by its host name; whether it answered. */
function waitingBoard(site: string, url: URL, response: ServerResponse): boolean {
  if (url.pathname === "/robots.txt") {
    const signal = site === "clean-site.test" ? "Content-Signal: search=yes, ai-train=no\n" : "";
    text(response, "text/plain", `User-agent: *\n${signal}Disallow: /private\n`);
    return true;
  }
  if (site === "clean-site.test" || site === "ask-first-site.test") {
    if (url.pathname === "/") {
      text(
        response,
        "text/html",
        `<html lang="en"><body>Invented jobs. <a href="/terms">Terms of service</a></body></html>`,
      );
      return true;
    }
    if (url.pathname === "/terms") {
      text(response, "text/html", termsPage(site));
      return true;
    }
    if (url.pathname === "/api/jobs") {
      console.log(`fake-jobs: listed ${site}`);
      json(response, 200, recentJobs(site));
      return true;
    }
    return false;
  }
  if (site.startsWith("feed-site-")) {
    if (url.pathname === "/") {
      text(
        response,
        "text/html",
        `<html lang="en"><head><link rel="alternate" type="application/rss+xml" href="/jobs.rss"></head><body>Invented jobs.</body></html>`,
      );
      return true;
    }
    if (url.pathname === "/jobs.rss") {
      text(response, "application/rss+xml", waitingFeed(site));
      return true;
    }
    return false;
  }
  if (url.pathname === "/") {
    text(response, "text/html", `<html lang="en"><body>Invented jobs.</body></html>`);
    return true;
  }
  if (url.pathname === "/api/jobs") {
    if (site === "keyed-board.test" && url.searchParams.get("api_key") !== "e2e-key") {
      json(response, 401, { error: "API key required." });
      return true;
    }
    console.log(`fake-jobs: listed the waiting board ${site}`);
    json(response, 200, waitingJobs(site));
    return true;
  }
  return false;
}

/** The hiring systems' hosts (#87), one per system the server reads. */
const HIRING_HOST =
  /^(greenhouse|lever|ashby|workable|recruitee|teamtailor|personio)\.hiring\.test$/;

/** The company a Greenhouse board of a hiring system's fake is of. */
const HIRING_COMPANIES: Record<string, string> = {
  "harbour-stays": "Harbour Stays",
  "northlight-labs": "Northlight Labs",
  "wayfarer-co": "Wayfarer Co",
};

/** Workable's widget answer for the company `token`: two jobs published today. */
function workableList(token: string) {
  const day = new Date().toISOString().slice(0, 10);
  return {
    name: HIRING_COMPANIES[token] ?? "Example Co",
    description: "Invented.",
    jobs: ["Travel Experience Designer", "Guest Services Manager"].map((title, index) => {
      const code = `WF${String(index + 1)}0A`;
      return {
        title,
        shortcode: code,
        url: `https://apply.workable.com/${token}/j/${code}/`,
        application_url: `https://apply.workable.com/${token}/j/${code}/apply/`,
        published_on: day,
        created_at: day,
        telecommuting: true,
        country: "Portugal",
        city: "Lisbon",
        locations: [{ country: "Portugal", city: "Lisbon", hidden: false }],
      };
    }),
  };
}

/**
 * A hiring system's host: robots rules (Lever's say `ai-train=no`), a home page
 * with no terms, and Workable's widget answer; the other readers' paths are
 * answered by the generic routes below. Whether it answered.
 */
function hiringSite(site: string, url: URL, response: ServerResponse): boolean {
  if (url.pathname === "/robots.txt") {
    const signal = site === "lever.hiring.test" ? "Content-Signal: search=yes, ai-train=no\n" : "";
    text(response, "text/plain", `User-agent: *\n${signal}Disallow: /private\n`);
    return true;
  }
  if (url.pathname === "/") {
    text(response, "text/html", `<html lang="en"><body>An invented hiring system.</body></html>`);
    return true;
  }
  const widget = /^\/api\/v1\/widget\/accounts\/([\w-]+)$/.exec(url.pathname);
  if (widget?.[1] !== undefined) {
    console.log(`fake-jobs: listed the Workable company ${widget[1]}`);
    json(response, 200, workableList(widget[1]));
    return true;
  }
  return false;
}

/** The watched companies' own sites (#20): one host per Playwright project, and one for a demo. */
const WATCHED_HOST = /^(northwind|contoso|tailspin)-(desktop|phone|demo)\.test$/;
/** The skill whose search on the remote board lists a project's Northwind Stays jobs. */
const WATCH_KEYWORD = /^watch-(desktop|phone|demo)$/i;

/** What differs between projects, so that two of them never share a job, a company or an id. */
const PROJECTS: Record<string, { first: number; titles: [string, string]; fit: string }> = {
  desktop: {
    first: 1000,
    titles: ["Senior Rails Engineer", "Backend Engineer"],
    fit: "Staff Platform Engineer",
  },
  phone: {
    first: 2000,
    titles: ["Senior Ruby Engineer", "Platform Engineer"],
    fit: "Principal Platform Engineer",
  },
  demo: {
    first: 3000,
    titles: ["Staff Rails Engineer", "Product Engineer"],
    fit: "Staff Product Engineer",
  },
};

/** Northwind Stays of a project: the demo's is plain, the tests' say which they are. */
function northwind(project: string): Employer {
  return {
    name: project === "demo" ? "Northwind Stays" : `Northwind Stays (${project})`,
    slug: `northwind-${project}`,
    url: `http://northwind-${project}.test:9020/`,
  };
}

/** Reads of each Northwind board: the first lists two jobs, every later one two more. */
const northwindReads = new Map<string, number>();

interface WatchedJob {
  id: number;
  title: string;
  place: string;
}

function northwindJobs(project: string, reads: number): WatchedJob[] {
  const first = PROJECTS[project]?.first ?? 0;
  const fit = PROJECTS[project]?.fit ?? "Staff Platform Engineer";
  const jobs: WatchedJob[] = [
    { id: first + 1, title: "Senior Rails Engineer", place: "Remote - Europe" },
    { id: first + 2, title: "Office Manager", place: "Porto" },
  ];
  if (reads >= 2) {
    jobs.push(
      { id: first + 3, title: fit, place: "Remote - Europe" },
      { id: first + 4, title: "Senior Backend Engineer (m/w/d)", place: "Berlin" },
    );
  }
  return jobs;
}

/** A job of a Northwind board, as Greenhouse lists it. */
function northwindJob(token: string, job: WatchedJob) {
  const now = new Date().toISOString();
  return {
    id: job.id,
    title: job.title,
    updated_at: now,
    first_published: now,
    location: { name: job.place },
    absolute_url: `https://boards.greenhouse.io/${token}/jobs/${String(job.id)}`,
    company_name: "Northwind Stays",
  };
}

/** The pages of the watched companies' sites; whether one answered. */
function watchedSite(site: string, url: URL, response: ServerResponse): boolean {
  const match = WATCHED_HOST.exec(site);
  const kind = match?.[1];
  const project = match?.[2];
  if (kind === undefined || project === undefined) {
    return false;
  }
  if (url.pathname === "/robots.txt") {
    text(response, "text/plain", "User-agent: *\nDisallow: /private\n");
    return true;
  }
  const name = kind === "northwind" ? northwind(project).name : kind;
  if (url.pathname === "/") {
    const careers = kind === "tailspin" ? "" : `<nav><a href="/careers">Careers</a></nav>`;
    text(
      response,
      "text/html",
      `<html><head><title>${name}</title></head><body><h1>${name}</h1><p>Invented software for invented customers.</p>${careers}</body></html>`,
    );
    return true;
  }
  if (kind === "northwind" && url.pathname === "/careers") {
    text(
      response,
      "text/html",
      `<html><head><title>Careers at ${name}</title></head><body><h1>Careers</h1><p>Come and build with us.</p><a href="https://boards.greenhouse.io/northwind-${project}">See our open roles</a></body></html>`,
    );
    return true;
  }
  if (kind === "contoso" && url.pathname === "/careers") {
    const posting = (slug: string, title: string) => ({
      "@type": "JobPosting",
      title,
      url: `http://${site}:9020/careers/${slug}`,
      datePosted: new Date().toISOString().slice(0, 10),
      jobLocationType: "TELECOMMUTE",
      hiringOrganization: { "@type": "Organization", name: "Contoso Health" },
      description: `<p>${title} at Contoso Health, an invented company.</p>`,
    });
    const graph = [
      posting("clinical-platform-engineer", "Clinical Platform Engineer"),
      posting("data-engineer", "Data Engineer"),
    ];
    text(
      response,
      "text/html",
      `<html><head><title>Careers at Contoso Health</title><script type="application/ld+json">${JSON.stringify({ "@context": "https://schema.org", "@graph": graph })}</script></head><body><h1>Careers</h1></body></html>`,
    );
    return true;
  }
  return false;
}

/** The company's own site (#34): invented, at its own host name. */
const COMPANY_HOST = "example-co.test";
/**
 * Invented companies whose site fails on its first visit to the home page
 * (503) and answers like Example Co's after that: a read that fails, then
 * works on Try again. One per e2e project (they share a stack), and one to
 * show by hand.
 */
const FLAKY_HOSTS: ReadonlySet<string> = new Set([
  "flaky-desktop.test",
  "flaky-phone.test",
  "flaky-demo.test",
]);
const flakyFailed = new Set<string>();

const COMPANY_PAGES: Record<string, string> = {
  "/": `<html><head><title>Example Co</title></head><body><h1>Example Co</h1>
    <p>Example Co builds invented ledgers for small shops.</p>
    <nav><a href="/about">About us</a> <a href="/customers">Customers</a>
    <a href="/blog/hotwire">Engineering blog</a> <a href="/contact">Contact</a>
    <a href="/private/roadmap">Customers roadmap</a>
    <a href="http://elsewhere.test/about">About them</a></nav></body></html>`,
  "/about": `<html><body><h1>About us</h1><p>Founded in an invented year. We are 40 people in Porto.</p></body></html>`,
  "/customers": `<html><body><h1>Customers</h1><p>Our customers are small shops and cafés in Portugal and Spain.</p></body></html>`,
  "/blog/hotwire": `<html><body><h1>Why we left React</h1><p>In March 2026 we started moving our dashboard from React to Hotwire.</p></body></html>`,
  "/contact": `<html><body><p>Write to hello at example-co dot test.</p></body></html>`,
  // Forbidden by robots.txt: the server must never ask for it.
  "/private/roadmap": `<html><body><p>SECRET ROADMAP: never read.</p></body></html>`,
};

/**
 * The posting (`/jobs/staff-engineer`): it asks for a skill in production,
 * Hotwire unless `?skill=` names another (a test that puts a skill away asks
 * for one of its own, so the next run still has its question).
 */
function posting(skill: string): string {
  return `<html><body><h1>Staff Engineer</h1>
    <p>You will own billing at Example Co.</p>
    <ul><li>5+ years of Rails required</li><li>${skill} in production</li><li>Kafka is a plus</li></ul>
    <h2>Application questions</h2>
    <ul><li>Why do you want to work at Example Co?</li><li>What's your notice period?</li></ul></body></html>`;
}

/** The company's site, by its host name; whether it answered. */
function companySite(url: URL, response: ServerResponse): boolean {
  if (url.pathname === "/jobs/staff-engineer") {
    const skill = (url.searchParams.get("skill") ?? "Hotwire").replaceAll(/[^\w-]/g, "");
    text(response, "text/html", posting(skill === "" ? "Hotwire" : skill));
    return true;
  }
  if (url.pathname === "/robots.txt") {
    text(response, "text/plain", "User-agent: *\nDisallow: /private\n");
    return true;
  }
  const page = COMPANY_PAGES[url.pathname];
  if (page === undefined) {
    return false;
  }
  if (url.pathname.startsWith("/private")) {
    console.log("fake-jobs: a page robots.txt forbids was asked for!");
  }
  text(response, "text/html", page);
  return true;
}

/** The `<page url="…">` blocks of the company prompt's user message. */
function pagesOf(content: string): { url: string; text: string }[] {
  return [...content.matchAll(/<page url="([^"]*)">\n([\s\S]*?)\n<\/page>/g)].map((match) => ({
    url: match[1] ?? "",
    text: match[2] ?? "",
  }));
}

/** What the fake company reader finds on a page, as `[fact, words on the page]`. */
const COMPANY_FACTS: readonly [string, string][] = [
  ["Builds ledgers for small shops", "builds invented ledgers for small shops"],
  ["Has about 40 people, in Porto", "We are 40 people in Porto"],
  [
    "Sells to small shops and cafés in Portugal and Spain",
    "small shops and cafés in Portugal and Spain",
  ],
  ["Is moving its dashboard from React to Hotwire", "moving our dashboard from React to Hotwire"],
];

/**
 * The company reader (system prompt "You read pages from a company's own
 * website…"): a fact for each known sentence the pages hold, quoted from the
 * page it is on, plus one whose quote is on no page, which the server drops.
 */
function companyFacts(userContent: string) {
  const pages = pagesOf(userContent);
  const facts = COMPANY_FACTS.flatMap(([fact, quote]) => {
    const page = pages.find((candidate) => candidate.text.includes(quote));
    return page === undefined ? [] : [{ text: fact, quote, url: page.url }];
  });
  const about = pages.find((page) => page.url.endsWith("/about"));
  return {
    facts:
      about === undefined
        ? facts
        : [...facts, { text: "Has 900 employees", quote: "We are 900 people", url: about.url }],
  };
}

/**
 * The job reader (system prompt "You compare a job posting…"): what the
 * posting asks for, quoting it, and up to three questions: the Hotwire one
 * (a skill, on the first position), one about owning billing (no position
 * named) and a Kafka one (no skill, no position).
 */
function jobAnalysis(userContent: string) {
  const posting = between(userContent, "posting");
  const profile = parsedBetween<{ positions?: { index: number }[] }>(userContent, "profile", {});
  const first = profile.positions?.[0]?.index ?? null;
  const skill = /([\w-]+) in production/.exec(posting)?.[1] ?? "Hotwire";
  const dismissed = parsedBetween<string[]>(userContent, "dismissed", []);
  const wants = (
    [
      ["Rails", "5+ years of Rails required", "must"],
      [`${skill} in production`, `${skill} in production`, "must"],
      ["Kafka", "Kafka is a plus", "nice"],
    ] as const
  )
    .filter(([, quote]) => posting.includes(quote))
    .map(([label, quote, level]) => ({ label, quote, level }));
  const questions = [
    {
      question: `They are moving from React to Hotwire (their blog). Have you worked with ${skill}?`,
      about: first,
      skill,
    },
    {
      question: "The posting says you will own billing. What did you own at your last job?",
      about: null,
      skill: null,
    },
    {
      question: "Kafka is a plus for this job. Have you run it in production?",
      about: null,
      skill: null,
    },
  ].filter(
    (question) =>
      (question.skill === null || !dismissed.includes(question.skill.toLowerCase())) &&
      (wants.length > 0 || question.skill === null),
  );
  return { wants, questions };
}

/**
 * An answer about a job (system prompt "You read a person's answer to one
 * question about a job opening…"): up to three facts quoted from the answer,
 * plus one whose quote the answer never says, which the server drops.
 */
function jobAnswer(userContent: string) {
  const answer = between(userContent, "answer");
  const quotes = answer
    .split(/(?<=[.!?])\s+/)
    .map((sentence) => sentence.replace(/[.!?]+$/, "").trim())
    .filter((sentence) => sentence !== "")
    .slice(0, 3);
  return {
    facts: [
      ...quotes.map((quote) => ({ text: quote, quote, area: "your_part" })),
      {
        text: "Saved the company 30% of its costs",
        quote: "we saved 30% of costs",
        area: "results",
      },
    ],
  };
}

/** A source of the CV prompt: `[w0.h1] text`. */
interface CvSource {
  id: string;
  text: string;
}

/** The `[id] text` lines of the CV prompt's user message. */
function cvSourcesOf(content: string): CvSource[] {
  return [...content.matchAll(/^\[([^\]\s]+)\] (.+)$/gm)].map((match) => ({
    id: match[1] ?? "",
    text: match[2] ?? "",
  }));
}

/** `text` with its first whole number changed to one no source has. */
function wrongNumber(text: string): string {
  const number = /\b(\d+)\b/.exec(text);
  if (number === null) {
    return `${text.replace(/[.!?]+$/, "")} by 37%`;
  }
  const wrong = String(Number(number[1]) + 17);
  return text.replace(number[0], wrong);
}

/**
 * The CV writer (system prompt "You write a one-page CV…"): the rules at the
 * head of this script, deterministic from the sources it is shown.
 */
function tailoredCv(userContent: string) {
  const sources = cvSourcesOf(userContent);
  const company = /^Company: (.+)$/m.exec(userContent)?.[1]?.trim() ?? "";
  const positions = [...userContent.matchAll(/<position id="w(\d+)"/g)].map((match) =>
    Number(match[1]),
  );
  const of = (position: number, kind: string) =>
    sources.filter((source) => source.id.startsWith(`w${String(position)}.${kind}`));
  const companyFact = sources.find((source) => /^c[0-9a-z]+$/.test(source.id));
  const headline = sources.find((source) => source.id === "hl");
  const [first, second] = positions;
  const highlights = first === undefined ? [] : of(first, "h");
  const facts = first === undefined ? [] : of(first, "f");
  const stripped = (text: string) => text.replace(/[.!?]+$/, "");

  const bullets: { text: string; sources: string[] }[] = [];
  const kept = facts[0];
  if (kept !== undefined) {
    bullets.push({ text: `Delivered: ${stripped(kept.text)}`, sources: [kept.id] });
  }
  const one = highlights[0];
  if (one !== undefined) {
    bullets.push({ text: one.text, sources: [one.id] });
  }
  const counted = highlights.find((source) => /\d/.test(source.text)) ?? one;
  if (counted !== undefined) {
    // A number no source says: the check flags it.
    bullets.push({ text: wrongNumber(counted.text), sources: [counted.id] });
  }
  const two = highlights[1];
  if (two !== undefined && companyFact !== undefined) {
    // A company fact used as the person's own achievement: the check flags it.
    bullets.push({
      text: `${stripped(two.text)} for customers like those of ${company}`,
      sources: [two.id, companyFact.id],
    });
  }
  const experience: { position: number; bullets: { text: string; sources: string[] }[] }[] = [];
  if (first !== undefined) {
    experience.push({ position: first, bullets });
  }
  const older = second === undefined ? undefined : of(second, "h")[0];
  if (second !== undefined && older !== undefined) {
    experience.push({ position: second, bullets: [{ text: older.text, sources: [older.id] }] });
  }
  const skills = (sources.find((source) => source.id === "sk")?.text ?? "")
    .split(",")
    .map((skill) => skill.trim())
    .filter((skill) => skill !== "");
  return {
    headline: {
      text: headline?.text ?? "Software engineer",
      sources: headline === undefined ? [] : ["hl"],
    },
    summary: {
      text: [
        one === undefined ? "" : `${stripped(one.text)}.`,
        companyFact === undefined ? "" : `A fit for what ${company} builds.`,
      ]
        .filter((part) => part !== "")
        .join(" "),
      sources: [...(one === undefined ? [] : [one.id]), ...(companyFact ? [companyFact.id] : [])],
    },
    experience,
    // One skill the profile does not list: the check flags it.
    skills: [...skills, "Kubernetes"],
  };
}

/** One flagged line of the CV fix prompt: its id, text, why, and its sources' words. */
interface FixLine {
  id: string;
  text: string;
  flagged: string[];
  sources: string[];
}

/** The `<line>` blocks of the CV fix prompt. */
function fixLinesOf(content: string): FixLine[] {
  return [...content.matchAll(/<line id="([^"]+)"[^>]*>\n([\s\S]*?)<\/line>/g)].map((match) => {
    const block = match[2] ?? "";
    return {
      id: match[1] ?? "",
      text: /<text>([\s\S]*?)<\/text>/.exec(block)?.[1] ?? "",
      flagged: [...block.matchAll(/<flagged>(.*?)<\/flagged>/g)].map((found) => found[1] ?? ""),
      sources: [...block.matchAll(/<source id="[^"]+">(.*?)<\/source>/g)].map(
        (found) => found[1] ?? "",
      ),
    };
  });
}

/**
 * The CV fixer (system prompt "You fix the lines of a CV…"): a line that uses
 * a company fact as an achievement is rewritten to its own source's words;
 * any other gets a question, with the line as a first-person fact.
 */
function cvFixes(userContent: string) {
  return {
    lines: fixLinesOf(userContent).map((line) => {
      const own = line.sources[0];
      if (line.flagged.some((why) => why.includes("about the company")) && own !== undefined) {
        return { id: line.id, rewrite: own, question: null, fact: null };
      }
      const lower = `${line.text.charAt(0).toLowerCase()}${line.text.slice(1)}`;
      const claim = line.flagged
        .map((why) => /^"(.+)" is not in its sources$/.exec(why)?.[1])
        .find((found) => found !== undefined);
      return {
        id: line.id,
        rewrite: null,
        question: claim === undefined ? "Is this line right?" : `Is “${claim}” right?`,
        fact: `I ${lower.replace(/[.!?]+$/, "")}.`,
      };
    }),
  };
}

/** The job's own questions of the letter prompt: `1. text` lines. */
function letterQuestionsOf(content: string): string[] {
  const block = /<questions>\n([\s\S]*?)\n?<\/questions>/.exec(content)?.[1] ?? "";
  return [...block.matchAll(/^\d+\. (.+)$/gm)].map((match) => match[1] ?? "");
}

/**
 * The letter writer (system prompt "You write a cover letter…"): the rules at
 * the head of this script, deterministic from the sources it is shown.
 */
function coverLetter(userContent: string) {
  const sources = cvSourcesOf(userContent);
  const company = /^Company: (.+)$/m.exec(userContent)?.[1]?.trim() ?? "";
  const companyFact = sources.find((source) => /^c[0-9a-z]+$/.test(source.id));
  const highlight = sources.find((source) => /^w\d+\.h\d+$/.test(source.id));
  const stripped = (text: string) => text.replace(/[.!?]+$/, "");
  const sentence = (text: string, cited: string[]) => ({ text, sources: cited });
  const paragraph = (...sentences: { text: string; sources: string[] }[]) => ({
    text: sentences.map((one) => one.text).join(" "),
    sentences,
  });

  const paragraphs: ReturnType<typeof paragraph>[] = [];
  const opening: { text: string; sources: string[] }[] = [];
  if (companyFact !== undefined) {
    opening.push(
      sentence(`What draws me to ${company}: ${stripped(companyFact.text)}.`, [companyFact.id]),
    );
  }
  if (highlight !== undefined) {
    opening.push(sentence(`${stripped(highlight.text)}.`, [highlight.id]));
  }
  if (opening.length > 0) {
    paragraphs.push(paragraph(...opening));
  }
  // Praise nobody wrote: the check flags it.
  paragraphs.push(
    paragraph(sentence("I'm the go-to person for the hardest billing problems.", [])),
  );
  if (companyFact !== undefined) {
    // What the company did, said as if the person did it: the check flags it.
    paragraphs.push(
      paragraph(sentence(`I led our migration, as ${company} did.`, [companyFact.id])),
    );
  }
  paragraphs.push(paragraph(sentence("I'd like to bring that experience to your team.", [])));

  const answers = letterQuestionsOf(userContent).map((question) => {
    const unknown = /notice|salary|visa|relocat/i.test(question);
    if (unknown || companyFact === undefined || highlight === undefined) {
      return { question, text: "", sources: [] as string[] };
    }
    return {
      question,
      text: `${stripped(companyFact.text)} matches the work I have done: ${stripped(highlight.text)}.`,
      sources: [companyFact.id, highlight.id],
    };
  });
  return { paragraphs, answers };
}

const server = createServer((request, response) => {
  const url = new URL(request.url ?? "/", `http://${host}:${String(port)}`);
  const method = request.method ?? "GET";
  const site = (request.headers.host ?? "").split(":", 1)[0] ?? "";
  if (method === "GET" && site === COMPANY_HOST && companySite(url, response)) {
    return;
  }
  if (method === "GET" && FLAKY_HOSTS.has(site)) {
    if (url.pathname === "/" && !flakyFailed.has(site)) {
      flakyFailed.add(site);
      console.log(`fake-jobs: ${site} failed its first visit`);
      text(response, "text/plain", "Try again later.", 503);
      return;
    }
    if (companySite(url, response)) {
      return;
    }
  }
  if (method === "GET" && WATCHED_HOST.test(site) && watchedSite(site, url, response)) {
    return;
  }
  if (method === "GET" && HIRING_HOST.test(site) && hiringSite(site, url, response)) {
    return;
  }
  if (
    method === "GET" &&
    site.endsWith(".test") &&
    !HIRING_HOST.test(site) &&
    waitingBoard(site, url, response)
  ) {
    return;
  }
  const board = /^\/v1\/boards\/([\w-]+)\/jobs\/?$/.exec(url.pathname);
  const posting = /^\/v1\/boards\/([\w-]+)\/jobs\/(\d+)$/.exec(url.pathname);
  const watchedBoard = /^northwind-(desktop|phone|demo)$/.exec(board?.[1] ?? posting?.[1] ?? "");
  if (method === "GET" && watchedBoard?.[1] !== undefined) {
    // A watched company's board (#20): two jobs on the first read, two more after it.
    const project = watchedBoard[1];
    const token = `northwind-${project}`;
    if (board !== null) {
      const reads = (northwindReads.get(token) ?? 0) + 1;
      northwindReads.set(token, reads);
      console.log(`fake-jobs: listed the watched board ${token} (read ${String(reads)})`);
      const jobs = northwindJobs(project, reads);
      json(response, 200, {
        jobs: jobs.map((one) => northwindJob(token, one)),
        meta: { total: jobs.length },
      });
      return;
    }
    const wanted = northwindJobs(project, 2).find((one) => one.id === Number(posting?.[2]));
    if (wanted === undefined) {
      json(response, 404, { error: "not_found" });
      return;
    }
    const where = wanted.title.includes("(m/w/d)")
      ? "<li>Standort: Remote-first aus Deutschland oder Österreich</li>"
      : "<li>Remote within Europe</li>";
    json(response, 200, {
      ...northwindJob(token, wanted),
      content: escaped(
        `<p>Northwind Stays builds invented booking software for invented guesthouses.</p><ul><li>Ruby on Rails</li>${where}</ul>`,
      ),
    });
    return;
  }
  if (method === "GET" && board !== null) {
    lists += 1;
    const base = Date.now() * 10 + lists;
    const token = board[1] ?? "";
    console.log(`fake-jobs: listed board ${token}`);
    const company = HIRING_COMPANIES[token];
    json(response, 200, {
      jobs: ROLES.map((title, index) => ({
        ...job(base * 10 + index, title),
        ...(company === undefined ? {} : { company_name: company }),
      })),
      meta: { total: ROLES.length },
    });
    return;
  }
  if (method === "GET" && url.pathname === "/himalayas/jobs/api/search") {
    // The remote board's keyword search (#28): one job naming the keyword.
    const keyword = url.searchParams.get("q") ?? "";
    console.log(`fake-jobs: searched the remote board for "${keyword}"`);
    const watched = WATCH_KEYWORD.exec(keyword)?.[1]?.toLowerCase();
    const project = watched === undefined ? undefined : PROJECTS[watched];
    if (watched !== undefined && project !== undefined) {
      // Two matches from one company, whose site the board gives (#20).
      json(response, 200, {
        offset: 0,
        limit: 20,
        totalCount: 2,
        jobs: project.titles.map((title, index) =>
          remoteJob(project.first + 90 + index, title, northwind(watched)),
        ),
      });
      return;
    }
    json(response, 200, {
      offset: 0,
      limit: 20,
      totalCount: 1,
      jobs: [remoteJob(7000 + keyword.length, `Senior ${keyword} Engineer`)],
    });
    return;
  }
  if (method === "GET" && url.pathname === "/himalayas/jobs/api") {
    lists += 1;
    const base = Date.now() * 10 + lists;
    console.log("fake-jobs: listed the remote board");
    json(response, 200, {
      offset: 0,
      limit: ROLES.length,
      totalCount: ROLES.length,
      nextCursor: null,
      jobs: ROLES.map((title, index) => remoteJob(base * 10 + index, title)),
    });
    return;
  }
  if (method === "GET" && posting !== null) {
    const id = Number(posting[2]);
    const title = ROLES[id % 10] ?? ROLES[0];
    const where =
      title === ROLES[DACH]
        ? "<li>Standort: Remote-first aus Deutschland oder Österreich</li>"
        : "<li>Remote within Europe</li>";
    json(response, 200, {
      ...job(id, title),
      content: escaped(
        `<p>Example Co builds invented software for invented customers.</p><ul><li>Work with a small team</li>${where}</ul><script>alert("not run")</script>`,
      ),
    });
    return;
  }
  if (method === "POST" && url.pathname === "/v1/chat/completions") {
    complete(request, response).catch(() => {
      json(response, 500, { error: { message: "server error" } });
    });
    return;
  }
  if (method === "GET" && url.pathname === "/healthz") {
    json(response, 200, { status: "ok" });
    return;
  }
  json(response, 404, { error: "not_found" });
});

server.listen(port, host, () => {
  console.log(`fake-jobs: listening on http://${host}:${String(port)}`);
});

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, () => {
    server.closeAllConnections();
    server.close();
  });
}
