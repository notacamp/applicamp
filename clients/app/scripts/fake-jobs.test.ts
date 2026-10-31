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
});
