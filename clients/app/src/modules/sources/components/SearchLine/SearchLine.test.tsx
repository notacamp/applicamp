import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { estimate, search } from "../../../../../test/pipeline";
import { renderWithProviders, stubApi } from "../../../../../test/render";
import { formatWhen, withinADay } from "../../api/format";
import { formatEuros, formatLimit, searchErrorOf, type Search } from "../../api/searches";
import { SpendLine } from "../SpendLine/SpendLine";

import { SearchLine } from "./SearchLine";
import { SearchProgress } from "./SearchProgress";

describe("a search's line", () => {
  it("says what a nightly search read and matched", () => {
    const finished = new Date(Date.now() - 60 * 60 * 1000).toISOString();
    renderWithProviders(
      <SearchLine search={search({ trigger: "nightly", finished_at: finished })} />,
    );
    expect(screen.getByTestId("search-line")).toHaveTextContent(
      "Last night: 412 jobs read, 9 matched · AI €0.01",
    );
  });

  it("says how many jobs it dropped for the person's requirements", () => {
    renderWithProviders(<SearchLine search={search({ dropped: 14 })} testId="dropped" />);
    expect(screen.getByTestId("dropped")).toHaveTextContent(
      "412 jobs read, 9 matched, 14 dropped for your requirements",
    );
  });

  it("says when a search ran, and why it stopped", () => {
    renderWithProviders(
      <SearchLine
        search={{
          status: "finished",
          trigger: "manual",
          started_at: "2026-10-20T09:00:00Z",
          finished_at: "2026-10-20T09:01:00Z",
          fetched: 412,
          added: 9,
          stopped: "ai_budget_reached",
        }}
        testId="line"
      />,
    );
    expect(screen.getByTestId("line")).toHaveTextContent("412 jobs read, 9 matched");
    expect(screen.getByTestId("search-stopped")).toHaveTextContent("AI spending limit");
  });

  it("says a search is running, or failed", () => {
    renderWithProviders(
      <>
        <SearchLine search={search({ status: "running" })} testId="running" />
        <SearchLine search={search({ status: "failed" })} testId="failed" />
      </>,
    );
    expect(screen.getByTestId("running")).toHaveTextContent("Searching the job boards now");
    expect(screen.getByTestId("failed")).toHaveTextContent("The last search failed");
  });

  it("follows a running search to its end", async () => {
    let status: Search["status"] = "running";
    stubApi({
      "GET /api/v1/searches/latest": () => ({
        body: {
          search: search({
            status,
            finished_at: status === "running" ? null : new Date().toISOString(),
          }),
        },
      }),
    });
    const onFinished = vi.fn();
    renderWithProviders(<SearchProgress onFinished={onFinished} />);
    expect(await screen.findByTestId("search-progress")).toHaveTextContent(
      "412 jobs read · 40 scored · 9 added",
    );
    status = "finished";
    await waitFor(
      () => {
        expect(screen.getByTestId("search-line")).toHaveTextContent("Today at");
      },
      { timeout: 4000 },
    );
    expect(onFinished).toHaveBeenCalled();
  });

  it("shows nothing before the first search, and a queued one as starting", async () => {
    stubApi({ "GET /api/v1/searches/latest": { body: { search: search({ status: "queued" }) } } });
    renderWithProviders(<SearchProgress />);
    expect(await screen.findByTestId("search-progress")).toHaveTextContent("Starting the search");
  });
});

describe("what the AI cost", () => {
  it("says this month's spend against its limit, in the person's language", async () => {
    stubApi({ "GET /api/v1/searches/estimate": { body: estimate() } });
    renderWithProviders(<SpendLine />, { locale: "pt-PT" });
    expect(await screen.findByTestId("spend-line")).toHaveTextContent(
      /IA este mês: 0,42\s€ de 3\s€/,
    );
  });

  it("says the spend alone when there is no limit", async () => {
    stubApi({
      "GET /api/v1/searches/estimate": {
        body: estimate({
          spend: { today_eur: 0, day_limit_eur: null, month_eur: 0.005, month_limit_eur: null },
        }),
      },
    });
    renderWithProviders(<SpendLine />);
    expect(await screen.findByTestId("spend-line")).toHaveTextContent("AI this month: €0.005");
  });

  it("formats sums and limits", () => {
    expect(formatEuros(0.42, "en")).toBe("€0.42");
    expect(formatEuros(0, "en")).toBe("€0.00");
    expect(formatLimit(3, "en")).toBe("€3");
    expect(formatLimit(0.3, "en")).toBe("€0.30");
    expect(searchErrorOf(new Error("network"))).toBe("generic");
    const now = new Date("2026-10-26T12:00:00");
    expect(formatWhen("2026-10-26T09:30:00", "en", now).today).toBe(true);
    expect(formatWhen("2026-10-20T09:30:00", "en", now).today).toBe(false);
    expect(withinADay("2026-10-26T01:00:00", now)).toBe(true);
    expect(withinADay("2026-10-24T01:00:00", now)).toBe(false);
  });
});
