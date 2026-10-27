// A tiny fake of the outside world a job search reads, for the e2e stack and
// a laptop demo: a Greenhouse job board (the public boards API's shapes) and
// an OpenAI-compatible model that scores every job it is shown. Never for
// production: its jobs and scores are invented.
//
//   node scripts/fake-jobs.ts
//
// Settings (environment):
//   FAKE_JOBS_PORT   port to listen on (9020)
//   FAKE_JOBS_HOST   address to bind (127.0.0.1; 0.0.0.0 in a container)
//
// Point the server at it with
//   APPLICAMP_SOURCES__ENABLED=greenhouse
//   APPLICAMP_SOURCES__GREENHOUSE_URL=http://localhost:9020
//   APPLICAMP_SOURCES__BOARDS_FILE=<a boards.toml naming one greenhouse board>
//   APPLICAMP_AI__COMPLETE__BASE_URL=http://localhost:9020/v1
//   APPLICAMP_AI__COMPLETE__MODEL=fake/model
//
// Every list of a board answers with four new jobs (new ids each time), so
// every search finds something new. The fourth is remote only from Germany or
// Austria, in the words of the posting that made #19: the model reads that
// fact and the search drops it ("… 1 dropped for your requirements") for
// someone who works remote only from Portugal.
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

/** Scores every posting of the last user message, with its facts, as the model would. */
async function complete(request: IncomingMessage, response: ServerResponse) {
  let postings: Posting[] = [];
  try {
    const parsed = JSON.parse(await body(request)) as {
      messages?: { role?: string; content?: string }[];
    };
    const user = (parsed.messages ?? []).findLast((message) => message.role === "user");
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
  json(response, 200, {
    id: `fake-${String(Date.now())}`,
    object: "chat.completion",
    model: "fake/model",
    choices: [
      {
        index: 0,
        message: { role: "assistant", content: JSON.stringify({ jobs }) },
        finish_reason: "stop",
      },
    ],
    usage: { prompt_tokens: 1200, completion_tokens: 300, total_tokens: 1500, cost: 0.0004 },
  });
}

const server = createServer((request, response) => {
  const url = new URL(request.url ?? "/", `http://${host}:${String(port)}`);
  const method = request.method ?? "GET";
  const board = /^\/v1\/boards\/([\w-]+)\/jobs\/?$/.exec(url.pathname);
  const posting = /^\/v1\/boards\/([\w-]+)\/jobs\/(\d+)$/.exec(url.pathname);
  if (method === "GET" && board !== null) {
    lists += 1;
    const base = Date.now() * 10 + lists;
    console.log(`fake-jobs: listed board ${board[1] ?? ""}`);
    json(response, 200, {
      jobs: ROLES.map((title, index) => job(base * 10 + index, title)),
      meta: { total: ROLES.length },
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
