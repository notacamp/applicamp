import { screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { discoveryRun, line, run } from "../../../../../test/boards";
import { estimate } from "../../../../../test/pipeline";
import { renderWithProviders, stubApi } from "../../../../../test/render";
import type { ScanRun } from "../../api/boards";

import { ScannedView } from "./ScannedView";

/** Last night's search: 1,112 read, 6 good matches from four sites, the rest left out. */
function lastNight(): ScanRun {
  return run({
    at: new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString(),
    cost_eur: 0.09,
    lines: [
      line({ matched: 1 }),
      line({
        board_id: "0190a1b2-0000-7000-8000-0000000000e1",
        board_slug: "wayfarer-co-greenhouse",
        board_name: "Wayfarer Co",
        company_page: true,
        matched: 2,
        jobs_read: 4,
      }),
      line({
        board_id: "0190a1b2-0000-7000-8000-0000000000e2",
        board_slug: "remoteok",
        board_name: "Remote OK",
        matched: 3,
        note: "Read 2 pages. Page 3 did not answer.",
      }),
      line({
        board_id: "0190a1b2-0000-7000-8000-0000000000e3",
        board_slug: "jobicy",
        board_name: "Jobicy",
        outcome: "error",
        jobs_read: 0,
        quick_looked: 0,
        scored_by_ai: 0,
        matched: 0,
        ai_cost_eur: 0,
        error_kind: "http_503",
      }),
    ],
  });
}

/** A search from before the funnel was counted. */
function older(): ScanRun {
  return run({
    id: "0190a1b2-0000-7000-8000-0000000000c3",
    at: "2026-10-20T21:00:00Z",
    cost_eur: 0.02,
    funnel: {
      read: 788,
      quick_looked: 0,
      read_in_full: 40,
      good_matches: 1,
      kept_with_note: 0,
      left_out: null,
      stopped: "ai_budget_reached",
    },
  });
}

function renderSearches(runs: ScanRun[]) {
  const requests = stubApi({
    "GET /api/v1/boards/scans": { body: { runs, next_cursor: null } },
    "GET /api/v1/searches/estimate": { body: estimate({ cost_eur: 0.1 }) },
    "GET /api/v1/searches/latest": { body: { search: null } },
  });
  renderWithProviders(<ScannedView />);
  return requests;
}

describe("The Searches tab", () => {
  it("shows the latest search's four numbers, where its matches came from and why the rest were left out", async () => {
    renderSearches([lastNight(), discoveryRun(), older()]);
    const latest = await screen.findByTestId("scanned-run");
    expect(latest).toHaveTextContent(/^Last night · /);
    // Each number with what it counts (read out as "jobs read: 1,112").
    expect(within(latest).getByTestId("scanned-read")).toHaveTextContent("jobs read1,112");
    expect(within(latest).getByTestId("scanned-read-in-full")).toHaveTextContent("read in full131");
    expect(within(latest).getByTestId("scanned-good-matches")).toHaveTextContent("good matches6");
    expect(within(latest).getByTestId("scanned-cost")).toHaveTextContent("cost€0.09");

    // Where the jobs came from: the store's sites and company pages.
    expect(within(latest).getByTestId("scanned-from-store")).toHaveTextContent(
      "Read from 4 sites and 212 company pages.",
    );

    // Each match under one source, a company page by name, the most first.
    const from = within(latest).getByTestId("scanned-from");
    expect([...from.querySelectorAll("li")].map((li) => li.textContent)).toEqual([
      "Remote OK · 3",
      "Wayfarer Co’s hiring page · 2",
      "Himalayas · 1",
    ]);

    // Every reason with a count, in plain words; with the matches they add up to the jobs read.
    const left = within(latest).getByTestId("scanned-left-out");
    const reasons = [...left.querySelectorAll("li")].map((li) => li.textContent);
    expect(reasons).toEqual([
      "Seen before · 431",
      "Not remote, or not where you can work · 245",
      "Not a fit, from the title and summary · 205",
      "Not among the closest to your roles today · 100",
      "In a language you don’t work in · 12",
      "Hire only in other countries · 4",
      "Pay less than you asked · 2",
      "Read in full, not a good match · 107",
    ]);
    const counted = reasons
      .map((reason) => Number(reason.split(" · ", 2)[1]?.replace(",", "")))
      .reduce((sum, count) => sum + count, 0);
    expect(counted + 6).toBe(1112);
    expect(within(latest).getByTestId("scanned-kept-with-note")).toHaveTextContent(
      "12 of the 131 read in full don’t say where they hire. They are kept, with a note.",
    );
    expect(latest).not.toHaveTextContent(/\(|\)/);

    // Each site, behind Show by site.
    expect(screen.queryByTestId("scanned-sites")).toBeNull();
    const user = userEvent.setup();
    await user.click(within(latest).getByTestId("scanned-by-site"));
    const sites = within(latest).getAllByTestId("scanned-line");
    expect(sites[0]).toHaveTextContent(
      "Read: 412 · Quick look: 30 · Read in full: 12 · Good matches: 1 · Cost: €0.004",
    );
    expect(sites[2]).toHaveTextContent("Read 2 pages. Page 3 did not answer.");
    expect(sites[3]).toHaveTextContent("Could not be read: the site did not answer.");
    await user.click(within(latest).getByTestId("scanned-by-site"));
    expect(screen.queryByTestId("scanned-sites")).toBeNull();

    // Earlier runs, one row each; Details opens one.
    const earlier = screen.getAllByTestId("scanned-earlier-run");
    expect(earlier).toHaveLength(2);
    expect(earlier[0]).toHaveTextContent("Looked for new job sites");
    expect(earlier[0]).toHaveTextContent("1 site checked");
    expect(earlier[1]).toHaveTextContent("jobs read788");
    await user.click(within(earlier[1] ?? document.body).getByTestId("scanned-details"));
    const open = screen.getByTestId("scanned-run");
    expect(within(open).getByTestId("scanned-read")).toHaveTextContent("jobs read788");
    // From before the reasons were counted: the numbers, and why it stopped.
    expect(within(open).queryByTestId("scanned-left-out")).toBeNull();
    expect(within(open).getByTestId("scanned-stopped")).toHaveTextContent(
      "Stopped at the AI spending limit.",
    );
    expect(screen.getAllByTestId("scanned-earlier-run")).toHaveLength(2);
  });

  it("offers Search now with its cost on the line under it, and no shortcut", async () => {
    renderSearches([lastNight()]);
    const line = await screen.findByTestId("scanned-search-now");
    expect(line).toHaveTextContent("The app searches every night at 21:00.");
    const button = within(line).getByTestId("scanned-search");
    expect(button).toHaveTextContent(/^Search now$/);
    expect(button).not.toHaveAttribute("aria-keyshortcuts");
    await waitFor(() => {
      expect(screen.getByTestId("scanned-search-cost")).toHaveTextContent("About €0.10.");
    });
  });

  it("says a cost of less than a tenth of a cent as under one", async () => {
    stubApi({
      "GET /api/v1/boards/scans": {
        body: {
          runs: [run({ cost_eur: 0.0004, lines: [line({ ai_cost_eur: 0.0002 })] })],
          next_cursor: null,
        },
      },
      "GET /api/v1/searches/estimate": { body: estimate({ cost_eur: 0.004 }) },
      "GET /api/v1/searches/latest": { body: { search: null } },
    });
    renderWithProviders(<ScannedView />);
    expect(await screen.findByTestId("scanned-cost")).toHaveTextContent("under €0.001");
    await waitFor(() => {
      expect(screen.getByTestId("scanned-search-cost")).toHaveTextContent("Under €0.01.");
    });
  });

  it("opens the newest search, not a later check for new job sites", async () => {
    const check = { ...discoveryRun(), at: new Date(Date.now() - 60 * 60 * 1000).toISOString() };
    renderSearches([check, lastNight()]);
    const open = await screen.findByTestId("scanned-run");
    expect(open).toHaveTextContent(/^Last night · /);
    expect(screen.getAllByTestId("scanned-earlier-run")[0]).toHaveTextContent(
      "Looked for new job sites",
    );
  });

  it("says how many matches carry the note even when nothing was left out", async () => {
    renderSearches([
      run({
        funnel: {
          read: 3,
          quick_looked: 0,
          read_in_full: 3,
          good_matches: 3,
          kept_with_note: 1,
          left_out: {},
          stopped: null,
        },
      }),
    ]);
    expect(await screen.findByTestId("scanned-kept-with-note")).toHaveTextContent(
      "1 of the 3 read in full doesn’t say where it hires. It is kept, with a note.",
    );
    expect(screen.queryByTestId("scanned-left-out")).toBeNull();
  });

  it("adds up to the good matches, with the many company pages that gave one or two folded into one line", async () => {
    // Himalayas gave 4, eight company pages gave 2, 2, 1, 1, 1, 1, 1, 1: 14 good matches.
    const pages = [2, 2, 1, 1, 1, 1, 1, 1].map((matched, i) =>
      line({
        board_id: `0190a1b2-0000-7000-8000-0000000001${String(i).padStart(2, "0")}`,
        board_slug: `page-${String(i)}`,
        board_name: `Company ${String(i + 1)}`,
        company_page: true,
        matched,
      }),
    );
    const search = run({
      lines: [line({ matched: 4 }), ...pages],
      funnel: { ...(lastNight().funnel as NonNullable<ScanRun["funnel"]>), good_matches: 14 },
    });
    renderSearches([search]);
    const from = await screen.findByTestId("scanned-from");
    const items = [...from.querySelectorAll("li")].map((li) => li.textContent);
    expect(items).toEqual([
      "Himalayas · 4",
      "Company 1’s hiring page · 2",
      "Company 2’s hiring page · 2",
      "Company 3’s hiring page · 1",
      "Company 4’s hiring page · 1",
      "4 company hiring pages · 4",
    ]);
    // The counts add up to the good matches.
    const total = items.reduce((sum, item) => sum + Number(item?.split(" · ")[1]), 0);
    expect(total).toBe(14);
  });

  it("says when there is no search yet", async () => {
    renderSearches([]);
    expect(await screen.findByTestId("scanned-empty")).toHaveTextContent("No searches yet");
  });

  it("is in the person's language", async () => {
    stubApi({
      "GET /api/v1/boards/scans": { body: { runs: [lastNight()], next_cursor: null } },
      "GET /api/v1/searches/estimate": { body: estimate({ cost_eur: 0.1 }) },
      "GET /api/v1/searches/latest": { body: { search: null } },
    });
    renderWithProviders(<ScannedView />, { locale: "pt-PT" });
    const latest = await screen.findByTestId("scanned-run");
    expect(latest).toHaveTextContent(/^Ontem à noite · /);
    expect(within(latest).getByTestId("scanned-left-out")).toHaveTextContent(
      "Não servem, pelo título e pelo resumo · 205",
    );
  });
});
