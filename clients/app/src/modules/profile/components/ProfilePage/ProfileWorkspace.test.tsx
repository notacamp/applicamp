import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";
import { EMPTY_GOALS } from "../../api/goals";
import { EMPTY_PROFILE, type Profile, type ProfileView } from "../../api/profile";
import { ImportReview } from "../ImportReview/ImportReview";

import { ProfileWorkspace } from "./ProfileWorkspace";

const IMPORT_ID = "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5c";
const EMPTY_VIEW: ProfileView = {
  profile: EMPTY_PROFILE,
  source: null,
  updated_at: null,
  import_kinds: ["linkedin_zip"],
};

/** An invented profile. */
const FILLED: Profile = {
  ...EMPTY_PROFILE,
  basics: { profiles: [], name: "Alex Demo", label: "Backend engineer", location: "Lisbon" },
  work: [
    {
      name: "Example Org",
      position: "Senior Engineer",
      startDate: "2021-03",
      highlights: [],
    },
    {
      name: "Sample Labs",
      position: "Engineer",
      startDate: "2017-06",
      endDate: "2021-02",
      highlights: [],
    },
  ],
  education: [{ institution: "Sample University", studyType: "Master's degree" }],
  skills: [{ name: "Rust" }, { name: "PostgreSQL" }],
  languages: [{ language: "Portuguese", fluency: "Native" }],
};

const FILLED_VIEW: ProfileView = {
  profile: FILLED,
  source: "linkedin_zip",
  updated_at: "2026-10-24T10:00:00Z",
  import_kinds: ["linkedin_zip", "pdf", "text"],
};

function draftOf(draft: Profile, dropped = 0) {
  return {
    id: IMPORT_ID,
    kind: "linkedin_zip",
    status: "ready",
    draft,
    dropped,
    created_at: "2026-10-26T10:00:00Z",
    expires_at: "2026-10-27T10:00:00Z",
  };
}

/** The workspace as the route drives it, with `?import` as state. */
function Workspace({ startImporting = false }: { startImporting?: boolean }) {
  const [importing, setImporting] = useState(startImporting);
  return (
    <ProfileWorkspace importing={importing} onImporting={setImporting} onStartGoals={vi.fn()} />
  );
}

/** Nothing answered about goals yet, unless a test says otherwise. */
const NO_GOALS = { version: 0, answers: EMPTY_GOALS, updated_at: null };

const stub = (routes: Record<string, Stub>) =>
  stubApi({ "GET /api/v1/goals": { body: NO_GOALS }, ...routes });

const zip = () =>
  new File(["PK\u0003\u0004"], "Basic_LinkedInDataExport.zip", { type: "application/zip" });

describe("the profile workspace", () => {
  it("imports a LinkedIn export into an empty profile: review, then Replace", async () => {
    const requests = stub({
      "GET /api/v1/profile": { body: EMPTY_VIEW },
      "POST /api/v1/profile/imports": { status: 201, body: draftOf(FILLED) },
      [`POST /api/v1/profile/imports/${IMPORT_ID}/apply`]: async (request) => {
        expect(await request.json()).toEqual({ mode: "replace" });
        return { body: FILLED_VIEW };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<Workspace />);

    // Without a model, only the export is offered, and there is no profile to go back to.
    expect(await screen.findByTestId("import-zip-only")).toHaveTextContent(
      "Import your LinkedIn data export",
    );
    expect(screen.queryByTestId("import-kinds")).toBeNull();
    expect(screen.queryByTestId("import-cancel")).toBeNull();

    await user.upload(screen.getByTestId("import-file"), zip());
    await user.click(screen.getByTestId("import-submit"));

    const review = await screen.findByTestId("import-review");
    expect(review).toHaveTextContent("Review what was found");
    expect(screen.getByTestId("review-work")).toHaveTextContent("Senior Engineer");
    expect(screen.getByTestId("review-work")).toHaveTextContent("New");
    expect(screen.getByTestId("review-basics")).toHaveTextContent("Alex Demo");
    expect(screen.queryByTestId("review-dropped")).toBeNull();
    expect(screen.queryByTestId("review-merge")).toBeNull();

    await user.click(screen.getByTestId("review-replace"));
    expect(await screen.findByTestId("profile-view")).toHaveTextContent("Example Org");
    const upload = requests.find(({ route }) => route === "POST /api/v1/profile/imports");
    expect(upload?.headers.get("x-applicamp-csrf")).toBe("1");
    expect(screen.getByTestId("profile-source")).toHaveTextContent("LinkedIn data export");
    // The profile is saved: the Goals section offers its questions, without forcing them.
    expect(await screen.findByTestId("goals-offer")).toHaveTextContent("Answer a few questions");
  });

  it("says why an upload was refused, before and after sending it", async () => {
    stub({
      "GET /api/v1/profile": { body: EMPTY_VIEW },
      "POST /api/v1/profile/imports": {
        status: 400,
        body: { status: 400, code: "archive_unsafe_path", title: "Bad Request", detail: "x" },
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<Workspace />);

    await user.click(await screen.findByTestId("import-submit"));
    expect(screen.getByTestId("import-error")).toHaveTextContent("Choose a file");

    const big = new File(["x"], "huge.zip");
    Object.defineProperty(big, "size", { value: 30 * 1024 * 1024 });
    await user.upload(screen.getByTestId("import-file"), big);
    await user.click(screen.getByTestId("import-submit"));
    expect(screen.getByTestId("import-error")).toHaveTextContent("too large");

    await user.upload(screen.getByTestId("import-file"), zip());
    await user.click(screen.getByTestId("import-submit"));
    expect(await screen.findByTestId("import-error")).toHaveTextContent(
      "stored outside it, so it wasn’t opened",
    );
  });

  it("falls back to a general message for a problem it does not know", async () => {
    stub({
      "GET /api/v1/profile": { body: EMPTY_VIEW },
      "POST /api/v1/profile/imports": { status: 500, body: { code: "internal_error" } },
    });
    const user = userEvent.setup();
    renderWithProviders(<Workspace />);
    await user.upload(await screen.findByTestId("import-file"), zip());
    await user.click(screen.getByTestId("import-submit"));
    expect(await screen.findByTestId("import-error")).toHaveTextContent("That didn’t work");
  });

  it("reads pasted text with a model, and says what it left out", async () => {
    stub({
      "GET /api/v1/profile": {
        body: { ...EMPTY_VIEW, import_kinds: ["linkedin_zip", "pdf", "text"] },
      },
      "POST /api/v1/profile/imports": { status: 201, body: draftOf(FILLED, 2) },
    });
    const user = userEvent.setup();
    renderWithProviders(<Workspace />);

    const kinds = await screen.findByTestId("import-kinds");
    const radios = kinds.querySelectorAll("input");
    expect(radios).toHaveLength(3);
    expect(kinds).toHaveTextContent("A PDF");
    const paste = radios.item(2);
    await user.click(paste);
    await user.keyboard("ALEX DEMO, Senior Engineer at Example Org");
    await user.click(screen.getByTestId("import-submit"));

    expect(await screen.findByTestId("review-dropped")).toHaveTextContent("Details left out: 2");
    expect(screen.getByTestId("review-dropped")).toHaveTextContent("Nothing is ever made up.");
  });

  it("merges a draft into a profile, marking what is new, changed or the same", async () => {
    const draft: Profile = {
      ...FILLED,
      basics: { profiles: [], name: "Alex Demo", label: "Staff engineer" },
      work: [
        FILLED.work[0] ?? { highlights: [] },
        { name: "Demo Studio", position: "Intern", startDate: "2016", highlights: [] },
      ],
    };
    const requests = stub({
      "GET /api/v1/profile": { body: FILLED_VIEW },
      "POST /api/v1/profile/imports": { status: 201, body: draftOf(draft) },
      [`POST /api/v1/profile/imports/${IMPORT_ID}/apply`]: async (request) => {
        expect(await request.json()).toEqual({ mode: "merge" });
        return { body: FILLED_VIEW };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<Workspace />);

    await user.click(await screen.findByTestId("import-again"));
    expect(screen.getByTestId("import-panel")).toBeInTheDocument();
    await user.upload(screen.getByTestId("import-file"), zip());
    await user.click(screen.getByTestId("import-submit"));

    const basics = await screen.findByTestId("review-basics");
    expect(basics).toHaveTextContent("Staff engineer");
    expect(basics).toHaveTextContent("Changed");
    expect(basics).toHaveTextContent("Now: Backend engineer");
    const work = screen.getByTestId("review-work");
    expect(work).toHaveTextContent("Same");
    expect(work).toHaveTextContent("Demo Studio");
    expect(work).toHaveTextContent("New");

    await user.click(screen.getByTestId("review-merge"));
    expect(await screen.findByTestId("profile-view")).toBeInTheDocument();
    expect(requests.map(({ route }) => route)).toContain(
      `POST /api/v1/profile/imports/${IMPORT_ID}/apply`,
    );
  });

  it("discards a draft, or goes back without importing", async () => {
    stub({
      "GET /api/v1/profile": { body: FILLED_VIEW },
      "POST /api/v1/profile/imports": { status: 201, body: draftOf(FILLED) },
      [`POST /api/v1/profile/imports/${IMPORT_ID}/discard`]: { status: 204 },
    });
    const user = userEvent.setup();
    renderWithProviders(<Workspace startImporting />);

    await user.click(await screen.findByTestId("import-cancel"));
    expect(await screen.findByTestId("profile-view")).toBeInTheDocument();

    await user.click(screen.getByTestId("import-again"));
    await user.upload(screen.getByTestId("import-file"), zip());
    await user.click(screen.getByTestId("import-submit"));
    await user.click(await screen.findByTestId("review-discard"));
    expect(await screen.findByTestId("profile-view")).toBeInTheDocument();
  });

  it("edits the profile in place, a value or an entry at a time", async () => {
    const saved: Profile[] = [];
    stub({
      "GET /api/v1/profile": { body: FILLED_VIEW },
      "PUT /api/v1/profile": async (request) => {
        const profile = (await request.json()) as Profile;
        saved.push(profile);
        return { body: { ...FILLED_VIEW, profile } };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<Workspace />);

    // A headline.
    await user.click(await screen.findByTestId("basics-label"));
    const headline = screen.getByTestId("basics-label-input");
    await user.clear(headline);
    await user.type(headline, "Staff engineer{Enter}");
    await waitFor(() => {
      expect(saved).toHaveLength(1);
    });
    expect(saved[0]?.basics.label).toBe("Staff engineer");
    await waitFor(() => {
      expect(screen.getByTestId("basics-label")).toHaveTextContent("Staff engineer");
    });

    // Cleared, it goes.
    await user.click(screen.getByTestId("basics-location"));
    await user.clear(screen.getByTestId("basics-location-input"));
    await user.click(screen.getByTestId("basics-location-save"));
    await waitFor(() => {
      expect(saved).toHaveLength(2);
    });
    expect(saved[1]?.basics.location).toBeNull();
    await waitFor(() => {
      expect(screen.getByTestId("basics-location")).toHaveTextContent("Add where you are");
    });

    // A position's title.
    expect(screen.getByTestId("section-work")).toHaveTextContent("Example Org · Mar 2021 – now");
    await user.click(screen.getByTestId("section-work-edit-0"));
    const title = screen.getByTestId("section-work-form-position");
    await user.clear(title);
    await user.type(title, "Principal Engineer");
    await user.click(screen.getByTestId("section-work-form-save"));
    await waitFor(() => {
      expect(saved).toHaveLength(3);
    });
    expect(saved[2]?.work[0]?.position).toBe("Principal Engineer");
    expect(saved[2]?.work).toHaveLength(2);
    await waitFor(() => {
      expect(screen.getByTestId("section-work")).toHaveTextContent("Principal Engineer");
    });

    // A new position with a bad date is checked before anything is sent.
    await user.click(screen.getByTestId("section-work-add"));
    await user.type(screen.getByTestId("section-work-form-name"), "Demo Studio");
    await user.type(screen.getByTestId("section-work-form-startDate"), "last spring");
    await user.click(screen.getByTestId("section-work-form-save"));
    expect(screen.getByTestId("section-work-form")).toHaveTextContent("Write a year (2021)");
    expect(saved).toHaveLength(3);
    await user.clear(screen.getByTestId("section-work-form-startDate"));
    await user.type(screen.getByTestId("section-work-form-startDate"), "2016");
    await user.click(screen.getByTestId("section-work-form-save"));
    await waitFor(() => {
      expect(saved).toHaveLength(4);
    });
    expect(saved[3]?.work[2]).toEqual({ name: "Demo Studio", startDate: "2016", highlights: [] });
    await waitFor(() => {
      expect(screen.getByTestId("section-work")).toHaveTextContent("Demo Studio");
    });

    // Removing a school.
    await user.click(screen.getByTestId("section-education-edit-0"));
    await user.click(screen.getByTestId("section-education-form-remove"));
    await waitFor(() => {
      expect(saved).toHaveLength(5);
    });
    expect(saved[4]?.education).toEqual([]);
    expect(await screen.findByTestId("section-education-empty")).toHaveTextContent(
      "No schools yet",
    );

    // Skills read as tags until edited.
    const skills = screen.getByTestId("section-skills");
    expect(skills).toHaveTextContent("Rust");
    await user.click(screen.getByTestId("section-skills-edit"));
    await user.click(screen.getByTestId("section-skills-edit-1"));
    await user.click(screen.getByTestId("section-skills-form-cancel"));
    await user.click(screen.getByTestId("section-skills-done"));
    expect(screen.getByTestId("section-skills-edit")).toBeInTheDocument();

    // A skill, added from an empty list's own link.
    await user.click(
      screen.getByTestId("section-certificates-empty").querySelector("button") ?? document.body,
    );
    await user.type(screen.getByTestId("section-certificates-form-name"), "Cloud Basics");
    await user.click(screen.getByTestId("section-certificates-form-save"));
    await waitFor(() => {
      expect(saved).toHaveLength(6);
    });
    expect(saved[5]?.certificates).toEqual([{ name: "Cloud Basics" }]);
  });

  it("says when a change could not be saved", async () => {
    stub({
      "GET /api/v1/profile": { body: FILLED_VIEW },
      "PUT /api/v1/profile": { status: 400, body: { code: "invalid_profile" } },
    });
    const user = userEvent.setup();
    renderWithProviders(<Workspace />);
    await user.click(await screen.findByTestId("basics-name"));
    await user.type(screen.getByTestId("basics-name-input"), "!{Enter}");
    expect(await screen.findByTestId("profile-save-error")).toHaveTextContent("couldn’t be saved");
  });

  it("offers to try again when the profile cannot be loaded", async () => {
    let up = false;
    stub({
      "GET /api/v1/profile": () =>
        up ? { body: EMPTY_VIEW } : { status: 503, body: { code: "x" } },
    });
    const user = userEvent.setup();
    renderWithProviders(<Workspace />);
    expect(screen.getByTestId("profile-loading")).toHaveTextContent("Loading your profile");
    const failed = await screen.findByTestId("profile-load-error");
    expect(failed).toHaveTextContent("couldn’t be loaded");
    up = true;
    await user.click(failed.querySelector("button") ?? document.body);
    expect(await screen.findByTestId("import-panel")).toBeInTheDocument();
  });

  it("speaks Portuguese", async () => {
    stub({ "GET /api/v1/profile": { body: EMPTY_VIEW } });
    renderWithProviders(<Workspace />, { locale: "pt-PT" });
    expect(await screen.findByTestId("import-panel")).toHaveTextContent("Importe o seu perfil");
  });
});

describe("a draft's review", () => {
  it("says when the draft is gone, and offers to import again", async () => {
    stub({
      [`GET /api/v1/profile/imports/${IMPORT_ID}`]: {
        status: 404,
        body: { status: 404, code: "not_found" },
      },
    });
    const user = userEvent.setup();
    const onRestart = vi.fn<() => void>();
    renderWithProviders(
      <ImportReview
        id={IMPORT_ID}
        current={EMPTY_PROFILE}
        onDone={vi.fn()}
        onRestart={onRestart}
      />,
    );
    expect(screen.getByTestId("review-loading")).toHaveTextContent("Loading the draft");
    const gone = await screen.findByTestId("review-gone");
    expect(gone).toHaveTextContent("drafts last 24 hours");
    await user.click(gone.querySelector("button") ?? document.body);
    expect(onRestart).toHaveBeenCalledOnce();
  });

  it("says when applying fails", async () => {
    stub({
      [`GET /api/v1/profile/imports/${IMPORT_ID}`]: { body: draftOf(FILLED) },
      [`POST /api/v1/profile/imports/${IMPORT_ID}/apply`]: {
        status: 409,
        body: { status: 409, code: "import_not_ready" },
      },
    });
    const user = userEvent.setup();
    renderWithProviders(
      <ImportReview id={IMPORT_ID} current={FILLED} onDone={vi.fn()} onRestart={vi.fn()} />,
    );
    expect(await screen.findByTestId("review-education")).toHaveTextContent("Same");
    await user.click(screen.getByTestId("review-replace"));
    expect(await screen.findByTestId("review-error")).toHaveTextContent("That didn’t work");
  });
});
