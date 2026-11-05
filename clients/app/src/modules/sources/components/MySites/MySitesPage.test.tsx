import { screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import {
  companies,
  FABRIKAM,
  NORTHWIND,
  suggestion,
  watching,
} from "../../../../../test/companies";
import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";
import { EUROPE_SITE, HIMALAYAS_SITE, RUBY_SITE, site, sites } from "../../../../../test/sites";
import type { CompanyList } from "../../api/companies";
import type { MySites } from "../../api/sites";
import { withSite } from "../../api/sites";

import { MySitesPage } from "./MySitesPage";

/** A server with the person's sites; a toggle is kept like the real one keeps it. */
function fakeSites(extra: Record<string, Stub> = {}, initial: MySites = sites()) {
  let current = initial;
  const queries: string[] = [];
  const put: { id: string; on: unknown }[] = [];
  const toggle =
    (id: string): Stub =>
    async (request) => {
      const body = (await request.json()) as { on: boolean };
      put.push({ id, on: body.on });
      current = withSite(current, id, body.on);
      return { body: { id, on: body.on, set_by: "person" } };
    };
  const requests = stubApi({
    "GET /api/v1/my-sites": (request) => {
      queries.push(new URL(request.url).search);
      return { body: current };
    },
    [`PUT /api/v1/my-sites/${HIMALAYAS_SITE}`]: toggle(HIMALAYAS_SITE),
    [`PUT /api/v1/my-sites/${RUBY_SITE}`]: toggle(RUBY_SITE),
    "GET /api/v1/companies": { body: companies([], []) },
    ...extra,
  });
  return { queries, put, requests };
}

/** The row of the site called `name`. */
async function rowOf(name: string): Promise<HTMLElement> {
  const rows = await screen.findAllByTestId("site-row");
  const row = rows.find((candidate) => (candidate.textContent ?? "").includes(name));
  if (row === undefined) {
    throw new Error(`no row for ${name}`);
  }
  return row;
}

describe("the job sites page", () => {
  it("says how many sites there are and how many are on, in groups, each row with its line and salary", async () => {
    fakeSites();
    renderWithProviders(<MySitesPage canManage={false} />);
    expect(await screen.findByTestId("sites-count")).toHaveTextContent("4 available, 2 on");
    const remote = screen.getByTestId("sites-group-remote_worldwide");
    expect(remote).toHaveTextContent("Remote, worldwide");
    const row = within(remote).getByTestId("site-row");
    expect(row).toHaveTextContent("Himalayas");
    expect(row).toHaveTextContent("Remote jobs, mostly tech.");
    expect(row).toHaveTextContent("Typical salary");
    expect(row).toHaveTextContent("96");
    // The group of their own country carries its name.
    expect(screen.getByTestId("sites-group-country")).toHaveTextContent("Portugal");
    expect(await rowOf("Jobs Portugal")).toHaveTextContent("Portugal. Mostly tech.");
    expect(await rowOf("Ruby on Remote")).toHaveTextContent("Only ruby and rails jobs.");
    // No filter chips: one search box and one select.
    expect(screen.getByTestId("sites-find")).toBeInTheDocument();
    expect(screen.getByTestId("sites-show")).toBeInTheDocument();
  });

  it("turns a site on and off with one button, keeping the count right", async () => {
    const { put } = fakeSites();
    const user = userEvent.setup();
    renderWithProviders(<MySitesPage canManage={false} />);
    const toggle = async (name: string) => within(await rowOf(name)).getByTestId("site-toggle");
    const ruby = await toggle("Ruby on Remote");
    expect(ruby).toHaveAttribute("aria-pressed", "false");
    expect(ruby).toHaveTextContent("Off");
    await user.click(ruby);
    await waitFor(async () => {
      expect(await toggle("Ruby on Remote")).toHaveAttribute("aria-pressed", "true");
    });
    expect(await toggle("Ruby on Remote")).toHaveTextContent("On");
    await waitFor(() => {
      expect(screen.getByTestId("sites-count")).toHaveTextContent("4 available, 3 on");
    });
    expect(put).toEqual([{ id: RUBY_SITE, on: true }]);
    await user.click(await toggle("Himalayas"));
    await waitFor(() => {
      expect(screen.getByTestId("sites-count")).toHaveTextContent("4 available, 2 on");
    });
    expect(put[1]).toEqual({ id: HIMALAYAS_SITE, on: false });
  });

  it("asks for the key for the owner and tells anyone else that the admin adds it", async () => {
    fakeSites();
    const { unmount } = renderWithProviders(<MySitesPage canManage />);
    const owner = await rowOf("Jobs Portugal");
    expect(within(owner).getByTestId("site-key")).toHaveTextContent("Add the key");
    expect(within(owner).queryByTestId("site-toggle")).toBeNull();
    unmount();
    renderWithProviders(<MySitesPage canManage={false} />);
    const row = await rowOf("Jobs Portugal");
    expect(row).toHaveTextContent("The admin adds this key");
    expect(within(row).queryByTestId("site-key")).toBeNull();
  });

  it("names the sites that do not allow apps in one sentence, and counts the company pages", async () => {
    fakeSites();
    renderWithProviders(<MySitesPage canManage={false} />);
    const sentence = await screen.findByTestId("sites-blocked");
    expect(sentence).toHaveTextContent(
      "Dice and Wellfound do not allow apps to read them, so they are not on the list.",
    );
    expect(screen.getByTestId("sites-company-pages")).toHaveTextContent(
      "Also reads 642 company hiring pages",
    );
  });

  it("finds a site by name and shows one group, asking the server", async () => {
    const { queries } = fakeSites();
    const user = userEvent.setup();
    renderWithProviders(<MySitesPage canManage={false} />);
    await screen.findByTestId("sites-count");
    await user.type(screen.getByTestId("sites-find"), "euro");
    await waitFor(() => {
      expect(queries).toContain("?q=euro");
    });
    await user.selectOptions(screen.getByTestId("sites-show"), "trade");
    await waitFor(() => {
      expect(queries).toContain("?q=euro&show=trade");
    });
  });

  it("says so when nothing matches", async () => {
    fakeSites({}, sites({ groups: [] }));
    renderWithProviders(<MySitesPage canManage={false} />);
    expect(await screen.findByTestId("sites-none")).toHaveTextContent("No site matches");
  });

  it("counts a site as on or off by its own button only", () => {
    const list = sites();
    const turned = withSite(list, EUROPE_SITE, false);
    expect(turned.on).toBe(1);
    expect(turned.groups[1]?.sites[0]).toMatchObject({ on: false, set_by: "person" });
    // The same choice twice changes nothing more.
    expect(withSite(turned, EUROPE_SITE, false).on).toBe(1);
    expect(site().on).toBe(true);
  });
});

/** A server with the companies it is given: Keep, Drop, Stop and their Undos answer the new list. */
function fakeCompanies(initial: CompanyList) {
  let current = initial;
  const posted: string[] = [];
  const answer = (change: (list: CompanyList) => CompanyList): Stub => {
    return (request) => {
      posted.push(`${request.method} ${new URL(request.url).pathname}`);
      current = change(current);
      return { body: current };
    };
  };
  const kept = watching({
    id: FABRIKAM,
    name: "Fabrikam Pay",
    board: "none",
    finding: true,
    open_jobs: null,
    last_checked_at: null,
  });
  fakeSites({
    "GET /api/v1/companies": () => ({ body: current }),
    [`POST /api/v1/companies/${FABRIKAM}/watch`]: answer((list) =>
      companies(
        list.suggested.filter((s) => s.id !== FABRIKAM),
        [...list.watching, kept],
      ),
    ),
    [`POST /api/v1/companies/${FABRIKAM}/stop`]: answer((list) =>
      companies(
        list.suggested,
        list.watching.filter((w) => w.id !== FABRIKAM),
      ),
    ),
    [`POST /api/v1/companies/${FABRIKAM}/not-interested`]: answer((list) =>
      companies(
        list.suggested.filter((s) => s.id !== FABRIKAM),
        list.watching,
      ),
    ),
    [`POST /api/v1/companies/${FABRIKAM}/undo-not-interested`]: answer((list) =>
      companies([suggestion(), ...list.suggested], list.watching),
    ),
  });
  return { posted };
}

describe("the companies below the sites", () => {
  it("keeps a company in its row: Following with Stop, then Stopped with Undo, then Following again", async () => {
    const { posted } = fakeCompanies(
      companies([suggestion()], [watching({ id: NORTHWIND, name: "Northwind Stays" })]),
    );
    const user = userEvent.setup();
    renderWithProviders(<MySitesPage canManage={false} />);
    const section = await screen.findByTestId("sites-companies");
    await waitFor(() => {
      expect(within(section).getByTestId(`site-company-${FABRIKAM}`)).toBeInTheDocument();
    });
    expect(section).toHaveTextContent("1 followed, 1 proposed");
    expect(within(section).getByTestId(`site-company-${NORTHWIND}`)).toHaveTextContent("Following");
    await user.click(screen.getByTestId(`site-company-keep-${FABRIKAM}`));
    const row = await screen.findByTestId(`site-company-stop-${FABRIKAM}`);
    expect(screen.getByTestId(`site-company-${FABRIKAM}`)).toHaveTextContent("Following");
    await user.click(row);
    const undo = await screen.findByTestId(`site-company-undo-${FABRIKAM}`);
    expect(screen.getByTestId(`site-company-${FABRIKAM}`)).toHaveTextContent("Stopped");
    await user.click(undo);
    await screen.findByTestId(`site-company-stop-${FABRIKAM}`);
    expect(posted).toEqual([
      `POST /api/v1/companies/${FABRIKAM}/watch`,
      `POST /api/v1/companies/${FABRIKAM}/stop`,
      `POST /api/v1/companies/${FABRIKAM}/watch`,
    ]);
  });

  it("drops a company without the row leaving, and Undo brings its Keep and Drop back", async () => {
    const { posted } = fakeCompanies(companies([suggestion()], []));
    const user = userEvent.setup();
    renderWithProviders(<MySitesPage canManage={false} />);
    await user.click(await screen.findByTestId(`site-company-drop-${FABRIKAM}`));
    const undo = await screen.findByTestId(`site-company-undo-${FABRIKAM}`);
    const row = screen.getByTestId(`site-company-${FABRIKAM}`);
    expect(row).toHaveTextContent("Fabrikam Pay");
    expect(row).toHaveTextContent("Dropped");
    await user.click(undo);
    await screen.findByTestId(`site-company-keep-${FABRIKAM}`);
    expect(screen.getByTestId(`site-company-drop-${FABRIKAM}`)).toBeInTheDocument();
    expect(posted).toEqual([
      `POST /api/v1/companies/${FABRIKAM}/not-interested`,
      `POST /api/v1/companies/${FABRIKAM}/undo-not-interested`,
    ]);
  });
});
