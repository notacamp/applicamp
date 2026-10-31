import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";

import { JobAnswerFacts } from "./JobAnswerFacts";

const ROLE = {
  position: "Senior Engineer",
  company: "Example Org",
  start_date: "2021-03",
  end_date: null,
};

interface Proposal {
  id: string;
  text: string;
  quote: string;
  area: string;
  turn_seq: number;
  status: string;
  expires_at: string;
}

const FACT: Proposal = {
  id: "f1",
  text: "Took 7 months",
  quote: "It took 7 months",
  area: "results",
  turn_seq: 2,
  status: "ready",
  expires_at: "2026-10-31T10:00:00Z",
};

const view = (proposals: Proposal[], kept = 0) => ({
  position: 0,
  role: ROLE,
  status: "in_progress",
  coverage: [],
  covered: 0,
  turns: [],
  question: null,
  proposals,
  kept,
  calls_today: 0,
  daily_cap: 150,
});

const routes = (extra: Record<string, Stub>) => stubApi(extra);

function renderFacts(ids: string[]) {
  const props = { position: 0, ids, role: "Senior Engineer · Example Org", testKey: "2" };
  return renderWithProviders(<JobAnswerFacts {...props} />);
}

describe("Facts taken from an answer about a job", () => {
  it("shows the facts with the person's words and keeps one", async () => {
    let kept: unknown;
    const taken = view([FACT, { ...FACT, id: "f2", text: "2 helped", quote: "2 helped" }]);
    const second = taken.proposals.slice(1);
    routes({
      "GET /api/v1/profile/grill/0": { body: taken },
      "POST /api/v1/profile/grill/proposals/f1/keep": async (request) => {
        kept = await request.json();
        return { body: view([{ ...FACT, status: "kept" }, ...second], 1) };
      },
    });
    const user = userEvent.setup();
    renderFacts(["f1", "f2"]);
    const block = await screen.findByTestId("grill-facts-2");
    expect(block).toHaveTextContent("Facts taken from your answer");
    expect(block).toHaveTextContent("Your words: It took 7 months");
    expect(block).toHaveTextContent("Saved to Senior Engineer · Example Org in your profile");
    await user.click(screen.getByTestId("grill-keep-f1"));
    await waitFor(() => {
      expect(kept).toEqual({});
    });
    expect(await screen.findByTestId("grill-kept-f1")).toHaveTextContent("Kept");
  });

  it("fixes a fact, and says when the fix says more than the person's words", async () => {
    const texts: unknown[] = [];
    const taken = view([FACT]);
    routes({
      "GET /api/v1/profile/grill/0": { body: taken },
      "POST /api/v1/profile/grill/proposals/f1/keep": async (request) => {
        const sent = (await request.json()) as { text: string };
        texts.push(sent.text);
        return sent.text.includes("9")
          ? {
              status: 400,
              body: { title: "Bad Request", status: 400, code: "fact_not_supported", detail: "x" },
            }
          : { body: view([{ ...FACT, text: sent.text, status: "kept" }], 1) };
      },
    });
    const user = userEvent.setup();
    renderFacts(["f1"]);
    await user.click(await screen.findByTestId("grill-fix-f1"));
    const field = screen.getByTestId("grill-fix-text-f1");
    expect(field).toHaveValue("Took 7 months");
    await user.clear(field);
    await user.type(field, "Took 9 months");
    await user.click(screen.getByTestId("grill-fix-save-f1"));
    await waitFor(() => {
      expect(field).toHaveAccessibleDescription(/Your words and your profile don’t say that/);
    });
    await user.clear(field);
    await user.type(field, "Took seven months");
    await user.click(screen.getByTestId("grill-fix-save-f1"));
    expect(await screen.findByTestId("grill-kept-f1")).toBeVisible();
    expect(texts).toEqual(["Took 9 months", "Took seven months"]);
    expect(screen.queryByTestId("grill-fix-text-f1")).toBeNull();
  });

  it("drops a fact, and keeps all that wait", async () => {
    const three = view([
      FACT,
      { ...FACT, id: "f2", text: "2 helped", quote: "2 helped" },
      { ...FACT, id: "f3", text: "Used 3 tools", quote: "3 tools" },
    ]);
    const rest = three.proposals.slice(1);
    const requests = routes({
      "GET /api/v1/profile/grill/0": { body: three },
      "POST /api/v1/profile/grill/proposals/f1/drop": {
        body: view([{ ...FACT, status: "dropped" }, ...rest]),
      },
      "POST /api/v1/profile/grill/0/keep-all": {
        body: view([
          { ...FACT, status: "dropped" },
          ...rest.map((fact) => ({ ...fact, status: "kept" })),
        ]),
      },
    });
    const user = userEvent.setup();
    renderFacts(["f1", "f2", "f3"]);
    await user.click(await screen.findByTestId("grill-drop-f1"));
    expect(await screen.findByTestId("grill-dropped-f1")).toHaveTextContent("Dropped");
    await user.click(await screen.findByTestId("grill-keep-all-2"));
    expect(await screen.findByTestId("grill-kept-f2")).toBeVisible();
    expect(screen.getByTestId("grill-kept-f3")).toBeVisible();
    expect(screen.queryByTestId("grill-keep-all-2")).toBeNull();
    expect(requests.map(({ route }) => route)).toContain("POST /api/v1/profile/grill/0/keep-all");
  });
});
