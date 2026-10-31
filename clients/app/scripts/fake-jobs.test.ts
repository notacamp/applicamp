// The fake model's tailored CV (#37): the e2e test and a recording rely on it
// flagging exactly three things, so it is pinned here. It starts the script
// on a free port and asks it the CV prompt, as the server does.
import { spawn, type ChildProcess } from "node:child_process";
import { createServer } from "node:net";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const PROMPT = `<language>en</language>
<posting>
Staff Engineer
Company: Example Co

You will own billing at Example Co.
</posting>
<wants>
</wants>
<company_facts>
[cab12cd34] Builds ledgers for small shops
</company_facts>
<profile>
[hl] Backend engineer who ships billing systems
<position id="w0" title="Senior Engineer" company="Example Org" dates="2021-03 - now">
[w0.h1] Rebuilt the billing pipeline in Rust, cutting invoice runs from 6 hours to 40 minutes
[w0.h2] Led a team of 4 engineers
[w0.fe2e0aaaa] Moved invoicing onto PostgreSQL
</position>
<position id="w1" title="Engineer" company="Sample Labs" dates="2017-06 - 2021-02">
[w1.h1] Built ingestion for 3 partner feeds
</position>
<position id="w2" title="Junior Developer" company="Tiny Co" dates="2015 - 2017">
[w2.h1] Wrote internal tools
</position>
[sk] Rust, PostgreSQL
</profile>`;

interface Line {
  text: string;
  sources: string[];
}

interface Cv {
  headline: Line;
  summary: Line;
  experience: { position: number; bullets: Line[] }[];
  skills: string[];
}

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const address = probe.address();
      const port = typeof address === "object" && address !== null ? address.port : 0;
      probe.close(() => {
        resolve(port);
      });
    });
  });
}

let base = "";

async function cv(prompt: string): Promise<Cv> {
  const response = await fetch(`${base}/v1/chat/completions`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      messages: [
        { role: "system", content: "You write a one-page CV for one person and one job." },
        { role: "user", content: prompt },
      ],
    }),
  });
  const body = (await response.json()) as { choices: { message: { content: string } }[] };
  return JSON.parse(body.choices[0]?.message.content ?? "{}") as Cv;
}

async function model<T>(system: string, user: string): Promise<T> {
  const response = await fetch(`${base}/v1/chat/completions`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      messages: [
        { role: "system", content: system },
        { role: "user", content: user },
      ],
    }),
  });
  const body = (await response.json()) as { choices: { message: { content: string } }[] };
  return JSON.parse(body.choices[0]?.message.content ?? "{}") as T;
}

const PORTRAIT_SYSTEM = "You write what a job-hunting app knows about one person, as a portrait.";
const EXTRA_SYSTEM = "You read what a person wrote about themselves, to fit it into a profile.";
const QUESTION_SYSTEM = "You word one question that a job-hunting app asks a person.";
const ANSWER_SYSTEM = "You read a person's answer to one question about their own profile.";

interface Asked {
  question: string;
  why: string;
  tag: string;
  choices: string[];
}

interface Answered {
  changes: {
    field: string;
    value: string;
    position: number | null;
    area: string;
    quote: string;
  }[];
}

/** The prompt of a question about `gap`, with the role it is about. */
const questionPrompt = (gap: Record<string, unknown>, assumption: unknown = null) =>
  `<language>en</language>\n<today>2026-10-31</today>\n<gap>\n${JSON.stringify(gap)}\n</gap>\n<role>\n${JSON.stringify({ index: 0, title: "Lead Engineer", company: "Example Co" })}\n</role>\n<positions>\nnull\n</positions>\n<assumption>\n${JSON.stringify(assumption)}\n</assumption>`;

/** The prompt of an answer about position 0 in `area`. */
const answerPrompt = (area: string, words: string) =>
  `<language>en</language>\n<profile>\n${JSON.stringify({ positions: [{ index: 0, title: "Lead Engineer", company: "Example Co" }] })}\n</profile>\n<about>\n${JSON.stringify({ index: 0, area })}\n</about>\n<question>\nQ?\n</question>\n<text>\n${words}\n</text>`;

describe("the fake CV writer", () => {
  let child: ChildProcess | undefined;

  beforeAll(async () => {
    const port = await freePort();
    base = `http://127.0.0.1:${String(port)}`;
    const started = spawn("node", ["scripts/fake-jobs.ts"], {
      env: { ...process.env, FAKE_JOBS_PORT: String(port), FAKE_JOBS_HOST: "127.0.0.1" },
      stdio: ["ignore", "pipe", "inherit"],
    });
    child = started;
    await new Promise<void>((resolve, reject) => {
      started.once("error", reject);
      started.stdout.on("data", (chunk: Buffer) => {
        if (String(chunk).includes("listening")) {
          resolve();
        }
      });
    });
  });

  afterAll(() => {
    child?.kill();
  });

  it("rewords a kept fact, copies a highlight, gets a number wrong, cites a company fact as an achievement, adds a skill the profile lacks and leaves a role out", async () => {
    const draft = await cv(PROMPT);
    const bullets = draft.experience[0]?.bullets ?? [];
    expect(draft.headline).toEqual({
      text: "Backend engineer who ships billing systems",
      sources: ["hl"],
    });
    expect(bullets[0]).toEqual({
      text: "Delivered: Moved invoicing onto PostgreSQL",
      sources: ["w0.fe2e0aaaa"],
    });
    expect(bullets[1]).toEqual({
      text: "Rebuilt the billing pipeline in Rust, cutting invoice runs from 6 hours to 40 minutes",
      sources: ["w0.h1"],
    });
    expect(bullets[2]?.text).toContain("from 23 hours to 40 minutes");
    expect(bullets[2]?.sources).toEqual(["w0.h1"]);
    expect(bullets[3]).toEqual({
      text: "Led a team of 4 engineers for customers like those of Example Co",
      sources: ["w0.h2", "cab12cd34"],
    });
    expect(draft.summary.sources).toEqual(["w0.h1", "cab12cd34"]);
    expect(draft.experience.map((role) => role.position)).toEqual([0, 1]);
    expect(draft.skills).toEqual(["Rust", "PostgreSQL", "Kubernetes"]);
  });

  it("copes with a profile that has less: no company fact, no kept fact", async () => {
    const draft = await cv(`<language>en</language>
<posting>
Engineer
Company: Example Co
</posting>
<profile>
<position id="w0" title="Engineer" company="Sample Labs" dates="">
[w0.h1] Built ingestion for 3 partner feeds
</position>
[sk] Rust
</profile>`);
    expect(draft.headline.sources).toEqual([]);
    expect(draft.summary.text).toBe("Built ingestion for 3 partner feeds.");
    expect(draft.experience).toHaveLength(1);
    expect(draft.skills).toEqual(["Rust", "Kubernetes"]);
  });

  it("writes a portrait with every kind, a known claim no source makes, and an assumption from the employer's own site", async () => {
    const portrait = await model<{
      sentences: { text: string; kind: string; position: number | null }[];
    }>(
      PORTRAIT_SYSTEM,
      `<language>en</language>
<positions>
[{"index":0,"title":"Senior Engineer","company":"Example Co"},{"index":1,"title":"Engineer","company":"Sample Labs"}]
</positions>
<sources>
[{"id":"w0","kind":"profile","text":"Senior Engineer at Example Co"},
 {"id":"w0f0","kind":"told","text":"Moved invoicing onto PostgreSQL"},
 {"id":"r0.0","kind":"research","text":"Builds ledgers for small shops","quote":"builds invented ledgers","url":"http://example-co.test:9020/"}]
</sources>`,
    );
    const kinds = new Set(portrait.sentences.map((sentence) => sentence.kind));
    expect([...kinds].toSorted()).toEqual(["assumed", "known", "told"]);
    expect(portrait.sentences.map((sentence) => sentence.text)).toContain(
      "You also run Kubernetes clusters in production.",
    );
    expect(
      portrait.sentences.some((sentence) => sentence.text.includes("according to its own website")),
    ).toBe(true);
    expect(
      portrait.sentences.filter(
        (sentence) => sentence.kind === "assumed" && sentence.position === 0,
      ).length,
    ).toBeGreaterThan(1);
  });

  it("reads what a person wrote into changes across the profile, with a summary that says one thing they did not", async () => {
    const text =
      "I ran events and a restaurant for 6 years. I want a senior Rails role, ideally for restaurants. I use Rails, Elixir and Postgres. I led 4 engineers at Example Co.";
    const read = await model<{ changes: { field: string; value: string; quote: string }[] }>(
      EXTRA_SYSTEM,
      `<language>en</language>\n<profile>\n{}\n</profile>\n<text>\n${text}\n</text>`,
    );
    expect(read.changes.map((change) => `${change.field}:${change.value}`)).toEqual([
      "summary:I ran events and a restaurant for 6 years. Now building hospitality products at Acme.",
      "headline:Senior Rails engineer",
      "skill:Rails",
      "skill:Elixir",
      "skill:Postgres",
      "background:I ran events and a restaurant for 6 years",
      "goal_role:Senior Rails engineer",
      "role_fact:Led 4 engineers",
    ]);
    for (const change of read.changes) {
      expect(text).toContain(change.quote);
    }
  });

  it("words a question for each kind of gap, always saying what was assumed or what is missing", async () => {
    const ask = (gap: Record<string, unknown>, assumption: unknown = null) =>
      model<Asked>(QUESTION_SYSTEM, questionPrompt(gap, assumption));
    const assumed = await ask(
      { kind: "assumed", area: "your_part", basis: "ask me later" },
      { text: "I assume you built bookings.", why: "Platforms like this are made of bookings." },
    );
    expect(assumed.tag).toBe("assumed");
    expect(assumed.why).toContain("I assumed this: I assume you built bookings.");
    expect(assumed.choices).toContain("None of these");
    const results = await ask({
      kind: "results",
      area: "results",
      basis: "Lead Engineer at Example Co states no results.",
    });
    expect(results.tag).toBe("unknown");
    expect(results.why).toContain("states no results");
    expect(results.choices).toEqual([]);
    const team = await ask({ kind: "team_size", area: "people", basis: "x" });
    expect(team.why).toContain("Your title was Lead Engineer");
    expect(team.choices).toHaveLength(4);
    const found = await ask({ kind: "skills_found", names: ["Kafka"], basis: "x" });
    expect(found.why).toBe("Your roles mention Kafka, but they aren’t in your skills.");
    expect(found.choices).toEqual(["Kafka", "None of these"]);
    const claim = await ask({ kind: "claim", names: ["Elixir"], basis: "x" });
    expect(claim.tag).toBe("told");
    expect(claim.why).toBe("You mentioned Elixir, but none of your roles do.");
  });

  it("reads an answer into changes quoting it, one of them naming a company the answer never wrote", async () => {
    const answer = (area: string, words: string) =>
      model<Answered>(ANSWER_SYSTEM, answerPrompt(area, words));
    const result = await answer("results", "Invoice runs went from 6 hours to 40 minutes.");
    expect(result.changes).toEqual([
      {
        field: "role_fact",
        value: "Invoice runs went from 6 hours to 40 minutes",
        position: 0,
        area: "results",
        quote: "Invoice runs went from 6 hours to 40 minutes.",
        sentence: "You told me: Invoice runs went from 6 hours to 40 minutes.",
      },
    ]);
    const team = await answer("people", "about 4 people\n3 to 6");
    expect(team.changes[0]?.value).toBe("Led about 4 people at Example Co");
    expect(team.changes[0]?.quote).toBe("about 4 people");
    const built = await answer("your_part", "Bookings\nPayments");
    expect(built.changes[0]?.value).toBe("Worked on bookings, payments");
    expect((await answer("your_part", "None of these")).changes).toEqual([]);
  });
});
