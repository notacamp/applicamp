import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";

import { writePref } from "~/lib/storage/prefs";

import { estimate, todayAnswer } from "../../test/pipeline";
import { DEMO_ME, renderApp, SIGNED_OUT, stubApi, type StubAnswer } from "../../test/render";

/** The element a `Tagged` test handle wraps (a nav item, a page). */
function tagged(testId: string): HTMLElement {
  const child = screen.getByTestId(testId).firstElementChild;
  if (!(child instanceof HTMLElement)) {
    throw new TypeError(`${testId} wraps nothing`);
  }
  return child;
}

function signedIn(extra: Record<string, StubAnswer | (() => StubAnswer)> = {}) {
  return stubApi({
    "GET /api/v1/me": { body: DEMO_ME },
    "GET /api/v1/today": { body: todayAnswer("set_up_profile") },
    "GET /api/v1/opportunities": { body: { opportunities: [] } },
    "GET /api/v1/searches/latest": { body: { search: null } },
    "GET /api/v1/searches/estimate": { body: estimate() },
    "GET /api/v1/runs": { body: { runs: [], available: true, startable: ["runs.probe"] } },
    ...extra,
  });
}

describe("the app", () => {
  afterEach(() => {
    writePref("theme", "system");
    writePref("locale", "en");
  });

  it("sends a signed-out browser to the sign-in page", async () => {
    stubApi({
      "GET /api/v1/me": SIGNED_OUT,
      "GET /bff/providers": { body: { providers: ["campbooks", "linkedin"] } },
    });
    const { router } = renderApp("/pipeline");

    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/sign-in");
    });
    const campbooks = await screen.findByTestId("sign-in-campbooks");
    expect(campbooks).toHaveAttribute("href", "/bff/sign-in/campbooks");
    expect(campbooks).toHaveTextContent("Continue with Campbooks");
    expect(screen.getByTestId("sign-in-linkedin")).toHaveAttribute("href", "/bff/sign-in/linkedin");
  });

  it("lands a first visit on Today, which offers to set up the profile", async () => {
    signedIn({
      "GET /api/v1/profile": {
        body: {
          profile: {
            basics: { profiles: [] },
            work: [],
            education: [],
            skills: [],
            languages: [],
            certificates: [],
            projects: [],
          },
          source: null,
          updated_at: null,
          import_kinds: ["linkedin_zip"],
        },
      },
    });
    const user = userEvent.setup();
    const { router } = renderApp("/");

    const today = await screen.findByTestId("today");
    expect(today).toHaveTextContent("Today");
    expect(await screen.findByTestId("today-setup")).toHaveTextContent(
      "First, tell the agents who you are",
    );
    expect(tagged("nav-today")).toHaveAttribute("aria-current", "page");
    expect(tagged("nav-pipeline")).not.toHaveAttribute("aria-current");

    expect(screen.getByTestId("set-up-profile")).toHaveAttribute("href", "/profile?import=new");
    await user.click(screen.getByTestId("set-up-profile"));
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/profile");
    });
    expect(router.state.location.search).toEqual({ import: "new" });
    const account = await screen.findByTestId("profile-account");
    expect(account).toHaveTextContent("demo@example.com");
    expect(account).toHaveTextContent("Campbooks");
    expect(await screen.findByTestId("import-panel")).toHaveTextContent("Import your profile");
  });

  it("moves between the places from the sidebar and the keyboard", async () => {
    signedIn();
    const user = userEvent.setup();
    const { router } = renderApp("/");
    await screen.findByTestId("today");

    await user.click(tagged("nav-agents"));
    expect(await screen.findByTestId("agents-empty")).toHaveTextContent("No runs yet");
    expect(router.state.location.pathname).toBe("/agents");
    expect(tagged("nav-agents")).toHaveAttribute("aria-current", "page");

    await user.keyboard("gp");
    expect(await screen.findByTestId("pipeline-empty")).toHaveTextContent("No opportunities yet");
    await user.keyboard("gy");
    expect(await screen.findByTestId("profile")).toHaveTextContent("Profile & goals");
    await user.keyboard("gt");
    expect(await screen.findByTestId("today")).toHaveTextContent("Today");
  });

  it("goes to a place from the command menu", async () => {
    signedIn();
    const user = userEvent.setup();
    const { router } = renderApp("/");
    await screen.findByTestId("today");

    await user.keyboard("{Control>}k{/Control}");
    await waitFor(() => {
      expect(document.activeElement).toHaveAttribute(
        "placeholder",
        "Go to a page or run a command",
      );
    });
    await user.keyboard("Pipeline{Enter}");
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/pipeline");
    });

    await user.click(tagged("jump"));
    await user.keyboard("Sign out");
    expect(document.body).toHaveTextContent("Account");
  });

  it("shows every shortcut on ?", async () => {
    signedIn();
    const user = userEvent.setup();
    renderApp("/");
    await screen.findByTestId("today");

    await user.click(tagged("shortcut-sheet-button"));
    await waitFor(() => {
      expect(document.body).toHaveTextContent("Keyboard shortcuts");
    });
    expect(document.body).toHaveTextContent("Profile & goals");
    await user.keyboard("{Escape}");
    await user.keyboard("?");
    expect(document.body).toHaveTextContent("Command menu");
  });

  it("signs out with the CSRF header and lands on the sign-in page", async () => {
    let signedOut = false;
    const requests = stubApi({
      "GET /api/v1/me": () => (signedOut ? SIGNED_OUT : { body: DEMO_ME }),
      "POST /bff/sign-out": () => {
        signedOut = true;
        return { status: 204 };
      },
      "GET /bff/providers": { body: { providers: ["campbooks"] } },
    });
    const user = userEvent.setup();
    const { router, queryClient } = renderApp("/");
    await screen.findByTestId("today");

    await user.click(screen.getByTestId("account-menu"));
    expect(document.body).toHaveTextContent("demo@example.com");
    await user.click(screen.getByTestId("sign-out"));

    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/sign-in");
    });
    expect(await screen.findByTestId("sign-in-campbooks")).toBeInTheDocument();
    const signOut = requests.find(({ route }) => route === "POST /bff/sign-out");
    expect(signOut?.headers.get("x-applicamp-csrf")).toBe("1");
    expect(queryClient.getQueryData(["identity", "me"])).toBeUndefined();
  });

  it("reaches the command menu and sign-out from the phone header", async () => {
    let signedOut = false;
    stubApi({
      "GET /api/v1/me": () => (signedOut ? SIGNED_OUT : { body: DEMO_ME }),
      "POST /bff/sign-out": () => {
        signedOut = true;
        return { status: 204 };
      },
      "GET /bff/providers": { body: { providers: ["campbooks"] } },
    });
    const user = userEvent.setup();
    const { router } = renderApp("/");
    await screen.findByTestId("today");
    expect(screen.getByTestId("tab-bar")).toHaveTextContent("Pipeline");

    await user.click(screen.getByTestId("jump-phone"));
    await waitFor(() => {
      expect(document.activeElement).toHaveAttribute("role", "combobox");
    });
    await user.keyboard("Agents{Enter}");
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/agents");
    });

    await user.click(screen.getByTestId("account-menu-phone"));
    await user.click(screen.getByTestId("sign-out"));
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/sign-in");
    });
  });

  it("says so when signing out fails", async () => {
    signedIn({ "POST /bff/sign-out": { status: 403, body: { code: "csrf_check_failed" } } });
    const user = userEvent.setup();
    const { router } = renderApp("/");
    await screen.findByTestId("today");

    await user.click(screen.getByTestId("account-menu"));
    await user.click(screen.getByTestId("sign-out"));
    expect(await screen.findByTestId("sign-out-failed")).toHaveTextContent("Couldn’t sign out");
    expect(router.state.location.pathname).toBe("/");
  });

  it("changes the theme and the language from the account menu", async () => {
    signedIn();
    const user = userEvent.setup();
    renderApp("/");
    await screen.findByTestId("today");

    await user.click(screen.getByTestId("account-menu-phone"));
    // System, Light, Dark.
    const dark = screen.getByTestId("theme-choice").querySelectorAll("button")[2];
    expect(dark).toBeDefined();
    if (dark !== undefined) {
      await user.click(dark);
    }
    expect(document.documentElement.dataset["theme"]).toBe("dark");

    await user.selectOptions(screen.getByTestId("language-choice"), "pt-PT");
    expect(await screen.findByTestId("today")).toHaveTextContent("Hoje");
  });

  it("offers a way back when the server cannot be reached", async () => {
    let up = false;
    stubApi({
      "GET /api/v1/me": () => (up ? { body: DEMO_ME } : { status: 503, body: { code: "x" } }),
    });
    const user = userEvent.setup();
    renderApp("/");

    expect(await screen.findByTestId("gate-unreachable")).toHaveTextContent(
      "Applicamp can’t be reached",
    );
    up = true;
    await user.click(screen.getByTestId("gate-retry"));
    expect(await screen.findByTestId("today")).toBeInTheDocument();
  });

  it("says why the last sign-in failed", async () => {
    stubApi({ "GET /bff/providers": { body: { providers: ["linkedin"] } } });
    renderApp("/sign-in?error=provider_unavailable");
    expect(await screen.findByTestId("sign-in-error")).toHaveTextContent(
      "That sign-in service isn’t answering",
    );
  });

  it("ignores an error it does not know", async () => {
    stubApi({ "GET /bff/providers": { body: { providers: ["linkedin"] } } });
    renderApp("/sign-in?error=elsewhere");
    await screen.findByTestId("sign-in-linkedin");
    expect(screen.queryByTestId("sign-in-error")).toBeNull();
  });

  it("tells someone not on the list, with a way back", async () => {
    stubApi({});
    renderApp("/not-invited");
    expect(await screen.findByTestId("not-invited")).toHaveTextContent(
      "You’re not on the list yet",
    );
    expect(screen.getByTestId("back-to-sign-in")).toHaveAttribute("href", "/sign-in");
  });

  it("knows when a page does not exist", async () => {
    stubApi({});
    renderApp("/nowhere");
    expect(await screen.findByTestId("not-found")).toHaveTextContent("This page doesn’t exist");
    expect(screen.getByTestId("not-found-home")).toHaveAttribute("href", "/");
  });

  it("speaks the person's language", async () => {
    signedIn();
    renderApp("/agents", { locale: "fr" });
    expect(await screen.findByTestId("agents-empty")).toHaveTextContent(
      "Aucune exécution pour l’instant",
    );
  });
  it("leads the retired Improve, Skills and Grill me addresses to Tell me about you", async () => {
    signedIn({
      "GET /api/v1/profile/about": {
        body: {
          has_profile: false,
          profile_version: 0,
          built: false,
          sentences: [],
          employers: [],
          accuracy: null,
          extra: null,
          changes: [],
          questions: {
            current: null,
            answered: 0,
            skipped: 0,
            total: 0,
            left: 0,
            done: false,
            strength: 0,
            updated: [],
          },
          calls_today: 0,
          daily_cap: 60,
          extra_max: 4000,
        },
      },
    });
    for (const old of [
      "/profile/grill",
      "/profile/grill/0",
      "/profile/improve",
      "/profile/improve?q=question:q1",
      "/profile/skills",
    ]) {
      const { router, unmount } = renderApp(old);
      await waitFor(() => {
        expect(router.state.location.pathname).toBe("/profile/about");
      });
      expect(await screen.findByTestId("about")).toBeVisible();
      unmount();
    }
  });
});
