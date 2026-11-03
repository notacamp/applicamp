import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";

import {
  companies,
  CONTOSO,
  FABRIKAM,
  NORTHWIND,
  suggestion,
  watching,
} from "../../../../../test/companies";
import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";
import type { CompanyList } from "../../api/companies";

import { CompaniesPage, type CompaniesTab } from "./CompaniesPage";

const PROBLEM = (status: number, code: string, detail: string) => ({
  status,
  body: { type: "about:blank", title: "Problem", status, detail, code },
});

/** A server holding the companies it is given, answering each action with the new list. */
function fakeCompanies(initial: CompanyList, extra: Record<string, Stub> = {}) {
  let current = initial;
  const posted: { route: string; body: unknown }[] = [];
  const answer = (change: (list: CompanyList) => CompanyList, status = 200): Stub => {
    return async (request) => {
      const text = await request.text();
      posted.push({
        route: `${request.method} ${new URL(request.url).pathname}`,
        body: text === "" ? null : (JSON.parse(text) as unknown),
      });
      current = change(current);
      return { status, body: current };
    };
  };
  const requests = stubApi({
    "GET /api/v1/companies": () => ({ body: current }),
    [`POST /api/v1/companies/${FABRIKAM}/watch`]: answer(
      (list) =>
        companies(
          list.suggested.filter((s) => s.id !== FABRIKAM),
          [
            ...list.watching,
            watching({
              id: FABRIKAM,
              name: "Fabrikam Pay",
              board: "none",
              finding: true,
              open_jobs: null,
              last_checked_at: null,
            }),
          ],
        ),
      202,
    ),
    [`POST /api/v1/companies/${FABRIKAM}/not-interested`]: answer((list) =>
      companies(
        list.suggested.filter((s) => s.id !== FABRIKAM),
        list.watching,
      ),
    ),
    [`POST /api/v1/companies/${NORTHWIND}/stop`]: answer((list) =>
      companies(
        list.suggested,
        list.watching.filter((w) => w.id !== NORTHWIND),
      ),
    ),
    [`POST /api/v1/companies/${CONTOSO}/careers`]: answer(
      (list) =>
        companies(
          list.suggested,
          list.watching.map((w) =>
            w.id === CONTOSO ? { ...w, finding: true, needs_board: false } : w,
          ),
        ),
      202,
    ),
    ...extra,
  });
  return { posted, requests };
}

function renderPage(initial: CompaniesTab = "suggested") {
  function Harness() {
    const [tab, setTab] = useState<CompaniesTab>(initial);
    return <CompaniesPage tab={tab} onTab={setTab} />;
  }
  return renderWithProviders(<Harness />);
}

describe("the companies page", () => {
  it("suggests a company with its reasons, and Why? shows the postings they come from", async () => {
    fakeCompanies(companies([suggestion()], []));
    const user = userEvent.setup();
    renderPage();
    const card = await screen.findByTestId(`company-card-${FABRIKAM}`);
    expect(card).toHaveTextContent("Fabrikam Pay");
    expect(card).toHaveTextContent("fabrikam.example");
    expect(screen.getByTestId("companies-tab-suggested")).toHaveTextContent("Suggested · 1");
    expect(screen.getByTestId("companies-tab-watching")).toHaveTextContent("Watching · 0");
    const reasons = screen.getByTestId(`company-reasons-${FABRIKAM}`);
    expect(reasons).toHaveTextContent("3 of their jobs fit you in the last 6 weeks");
    expect(reasons).toHaveTextContent("Ruby on Rails, PostgreSQL in their postings");
    expect(reasons).toHaveTextContent("Remote in every posting");
    expect(reasons).toHaveTextContent("Pays EUR 80000–105000 per year");
    expect(reasons).not.toHaveTextContent("Backend Engineer, Billing (fits 84)");
    await user.click(screen.getByTestId(`company-why-${FABRIKAM}`));
    expect(screen.getByTestId(`company-why-${FABRIKAM}`)).toHaveAttribute("aria-expanded", "true");
    expect(reasons).toHaveTextContent("Backend Engineer, Billing (fits 84)");
    expect(reasons).toHaveTextContent("Staff Engineer (fits 72)");
  });

  it("says a shortlisted job and a fact read for a CV in words, citing where they come from", async () => {
    fakeCompanies(
      companies(
        [
          suggestion({
            reasons: [
              {
                kind: "shortlisted",
                count: 1,
                total: 0,
                weeks: 0,
                values: [],
                sources: [
                  { kind: "posting", id: NORTHWIND, label: "Senior Rails Engineer", score: 80 },
                ],
              },
              {
                kind: "fact",
                count: 0,
                total: 0,
                weeks: 0,
                values: ["Is moving its dashboard from React to Hotwire"],
                sources: [{ kind: "fact", id: CONTOSO, label: "Fabrikam Pay", score: null }],
              },
            ],
          }),
        ],
        [],
      ),
    );
    const user = userEvent.setup();
    renderPage();
    const reasons = await screen.findByTestId(`company-reasons-${FABRIKAM}`);
    expect(reasons).toHaveTextContent("You shortlisted one of their jobs");
    expect(reasons).toHaveTextContent("Is moving its dashboard from React to Hotwire");
    await user.click(screen.getByTestId(`company-why-${FABRIKAM}`));
    expect(reasons).toHaveTextContent("Read on Fabrikam Pay’s own website, for your CV");
  });

  it("moves a company to Watching when asked to watch it, and shows its board being looked for", async () => {
    const { posted } = fakeCompanies(companies([suggestion()], []));
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByTestId(`company-watch-${FABRIKAM}`));
    expect(await screen.findByTestId(`watching-finding-${FABRIKAM}`)).toHaveTextContent(
      "Looking for their job board…",
    );
    expect(posted).toEqual([{ route: `POST /api/v1/companies/${FABRIKAM}/watch`, body: null }]);
    expect(screen.getByTestId("companies-tab-watching")).toHaveTextContent("Watching · 1");
  });

  it("hides a company for good with Not interested, and says when there is nothing to suggest", async () => {
    fakeCompanies(companies([suggestion()], []));
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByTestId(`company-dismiss-${FABRIKAM}`));
    expect(await screen.findByTestId("companies-suggested-empty")).toHaveTextContent(
      "No suggestions right now",
    );
  });

  it("lists the watched companies with their board, open jobs and last check", async () => {
    fakeCompanies(
      companies(
        [],
        [
          watching(),
          watching({
            id: CONTOSO,
            name: "Contoso Health",
            board: "jsonld",
            open_jobs: 9,
            last_error: "http_503",
          }),
        ],
      ),
    );
    renderPage("watching");
    const northwind = await screen.findByTestId(`watching-row-${NORTHWIND}`);
    expect(northwind).toHaveTextContent("Northwind Stays");
    expect(screen.getByTestId(`watching-board-${NORTHWIND}`)).toHaveTextContent("Greenhouse");
    expect(screen.getByTestId(`watching-open-${NORTHWIND}`)).toHaveTextContent("23 open jobs");
    expect(screen.getByTestId(`watching-checked-${NORTHWIND}`)).toHaveTextContent(/^Checked /);
    expect(screen.queryByTestId(`watching-failed-${NORTHWIND}`)).toBeNull();
    const contoso = screen.getByTestId(`watching-row-${CONTOSO}`);
    expect(contoso).toHaveTextContent("Their careers page (JobPosting)");
    expect(screen.getByTestId(`watching-failed-${CONTOSO}`)).toHaveTextContent(
      "Last check failed: the board answered 503",
    );
  });

  it("stops watching a company", async () => {
    const { posted } = fakeCompanies(companies([], [watching()]));
    const user = userEvent.setup();
    renderPage("watching");
    await user.click(await screen.findByTestId(`watching-stop-${NORTHWIND}`));
    expect(await screen.findByTestId("companies-watching-empty")).toBeInTheDocument();
    expect(posted).toEqual([{ route: `POST /api/v1/companies/${NORTHWIND}/stop`, body: null }]);
  });

  it("says when a job board could not be found, and looks at a careers page pasted for it", async () => {
    const { posted } = fakeCompanies(
      companies(
        [],
        [
          watching({
            id: CONTOSO,
            name: "Contoso Health",
            board: "none",
            needs_board: true,
            open_jobs: null,
            last_checked_at: null,
            last_error: "no_board",
          }),
        ],
      ),
    );
    const user = userEvent.setup();
    renderPage("watching");
    expect(await screen.findByTestId(`watching-nofound-${CONTOSO}`)).toHaveTextContent(
      "Couldn’t find their job board",
    );
    await user.type(
      screen.getByTestId(`watching-careers-${CONTOSO}`),
      "https://contoso.example/careers",
    );
    await user.click(screen.getByTestId(`watching-careers-go-${CONTOSO}`));
    expect(await screen.findByTestId(`watching-finding-${CONTOSO}`)).toBeInTheDocument();
    expect(posted).toEqual([
      {
        route: `POST /api/v1/companies/${CONTOSO}/careers`,
        body: { url: "https://contoso.example/careers" },
      },
    ]);
  });

  it("watches a company by its pasted careers page, and says why when it cannot", async () => {
    let calls = 0;
    fakeCompanies(companies([], []), {
      "POST /api/v1/companies": async (request) => {
        calls += 1;
        const body = (await request.json()) as { url: string };
        if (calls === 1) {
          return PROBLEM(409, "watch_limit_reached", "You can watch up to 20 companies.");
        }
        if (calls === 2) {
          return PROBLEM(400, "invalid_url", "Paste the company’s careers page.");
        }
        return {
          status: 202,
          body: companies([], [watching({ id: CONTOSO, name: body.url, finding: true })]),
        };
      },
    });
    const user = userEvent.setup();
    renderPage("watching");
    const input = await screen.findByTestId("companies-add-url");
    expect(screen.getByTestId("companies-add")).toHaveTextContent("Up to 20 companies");
    await user.type(input, "https://one-too-many.example/careers");
    await user.click(screen.getByTestId("companies-add-go"));
    await waitFor(() => {
      expect(screen.getByTestId("companies-add")).toHaveTextContent(
        "You can watch up to 20 companies",
      );
    });
    await user.click(screen.getByTestId("companies-add-go"));
    await waitFor(() => {
      expect(screen.getByTestId("companies-add")).toHaveTextContent("starting with https://");
    });
    await user.clear(input);
    await user.type(input, "https://northwind.example/careers");
    await user.click(screen.getByTestId("companies-add-go"));
    expect(await screen.findByTestId(`watching-finding-${CONTOSO}`)).toBeInTheDocument();
  });

  it("explains how it works and what it costs", async () => {
    fakeCompanies(companies([], []));
    renderPage("how");
    const how = await screen.findByTestId("companies-how");
    expect(how).toHaveTextContent("Suggested from your matches, checked daily without AI");
    expect(how).toHaveTextContent("at least 2 matches, or 1 you shortlisted, in the last 8 weeks");
    const cost = screen.getByTestId("companies-how-cost");
    expect(cost).toHaveTextContent("Daily check, no new postings");
    expect(cost).toHaveTextContent("about €0.0003 each");
    expect(cost).toHaveTextContent("Up to 20 watched companies");
  });

  it("says when the companies cannot be loaded, and tries again", async () => {
    let fail = true;
    stubApi({
      "GET /api/v1/companies": () =>
        fail
          ? PROBLEM(500, "internal_error", "Something went wrong on our side.")
          : { body: companies([suggestion()], []) },
    });
    const user = userEvent.setup();
    renderPage();
    expect(await screen.findByTestId("companies-load-error")).toHaveTextContent(
      "The companies couldn’t be loaded.",
    );
    fail = false;
    await user.click(screen.getByTestId("companies-retry"));
    expect(await screen.findByTestId(`company-card-${FABRIKAM}`)).toBeInTheDocument();
  });
});
