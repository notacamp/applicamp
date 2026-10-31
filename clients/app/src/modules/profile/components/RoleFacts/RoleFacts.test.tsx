import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { UndoProvider } from "~/lib/ui";

import { renderWithProviders, stubApi } from "../../../../../test/render";
import type { Profile } from "../../api/profile";

import { RoleFacts } from "./RoleFacts";

const FACT = {
  id: "f1",
  text: "Led the upgrade",
  quote: "I led it",
  area: "your_part" as const,
  kept_at: "2026-10-31T10:00:00Z",
};

const PROFILE: Profile = {
  basics: { profiles: [] },
  work: [
    {
      name: "Example Org",
      position: "Senior Engineer",
      startDate: "2021-03",
      highlights: [],
      facts: [FACT],
    },
    { name: "Sample Labs", position: "Engineer", startDate: "2017-06", highlights: [] },
  ],
  education: [],
  skills: [],
  languages: [],
  certificates: [],
  projects: [],
};

describe("What you told us about each role", () => {
  it("lists every role with the facts kept under it, in the person's words, and a way into Grill me", async () => {
    stubApi({});
    const onGrill = vi.fn();
    const user = userEvent.setup();
    renderWithProviders(
      <UndoProvider>
        <RoleFacts profile={PROFILE} onGrill={onGrill} />
      </UndoProvider>,
    );
    const list = screen.getByTestId("role-facts-list-0");
    expect(list).toHaveTextContent("Led the upgrade");
    expect(list).toHaveTextContent("Your words: I led it");
    expect(screen.queryByTestId("role-facts-list-1")).toBeNull();
    expect(screen.getByTestId("role-facts-row-1")).toHaveTextContent("Engineer");
    await user.click(screen.getByTestId("role-grill-1"));
    expect(onGrill).toHaveBeenCalledWith(1);
  });

  it("removes a fact, and puts it back on Undo", async () => {
    const saved: Profile[] = [];
    stubApi({
      "PUT /api/v1/profile": async (request) => {
        const profile = (await request.json()) as Profile;
        saved.push(profile);
        return { body: { profile, source: "answer", updated_at: null, import_kinds: [] } };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(
      <UndoProvider>
        <RoleFacts profile={PROFILE} onGrill={vi.fn()} />
      </UndoProvider>,
    );
    await user.click(screen.getByTestId("role-fact-remove-f1"));
    await waitFor(() => {
      expect(saved).toHaveLength(1);
    });
    expect(saved[0]?.work[0]?.facts).toEqual([]);
    expect(saved[0]?.work[1]?.facts).toBeUndefined();
    await user.keyboard("z");
    await waitFor(() => {
      expect(saved).toHaveLength(2);
    });
    expect(saved[1]?.work[0]?.facts).toEqual([FACT]);
  });
});
