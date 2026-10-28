import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";
import { EMPTY_PROFILE } from "../../api/profile";

import { SkillsPanel } from "./SkillsPanel";

const view = (skills: string[]) => ({
  profile: { ...EMPTY_PROFILE, skills: skills.map((name) => ({ name })) },
  source: "manual",
  updated_at: "2026-10-28T10:00:00Z",
  import_kinds: ["linkedin_zip"],
});

const found = (skills: { name: string; roles: number }[], extra: Record<string, unknown> = {}) => ({
  status: "ready",
  reason: null,
  skills,
  ...extra,
});

function routes(extra: Record<string, Stub> = {}, skills = ["PostgreSQL", "Go", "SQL"]) {
  return stubApi({
    "GET /api/v1/profile": { body: view(skills) },
    "GET /api/v1/profile/skills/suggested": {
      body: found([
        { name: "Rust", roles: 4 },
        { name: "Kafka", roles: 1 },
      ]),
    },
    ...extra,
  });
}

describe("Skills", () => {
  it("lists the skills in order, the first three marked as the ones searches use", async () => {
    routes({}, ["PostgreSQL", "Go", "SQL", "Linux"]);
    renderWithProviders(<SkillsPanel onBack={vi.fn()} />);
    expect(await screen.findByTestId("skill-rank-PostgreSQL")).toHaveTextContent("1");
    expect(screen.getByTestId("skills-yours")).toHaveTextContent(
      "The first three drive the job board searches",
    );
    for (const name of ["PostgreSQL", "Go", "SQL"]) {
      expect(screen.getByTestId(`skill-top-${name}`)).toHaveTextContent("Used for searches");
    }
    expect(screen.queryByTestId("skill-top-Linux")).toBeNull();
    expect(screen.getByTestId("skill-up-PostgreSQL")).toBeDisabled();
    expect(screen.getByTestId("skill-down-Linux")).toBeDisabled();
  });

  it("shows the skills found in the roles with how many roles, and adds one on a click", async () => {
    let left = [
      { name: "Rust", roles: 4 },
      { name: "Kafka", roles: 1 },
    ];
    const requests = routes({
      "GET /api/v1/profile/skills/suggested": () => ({ body: found(left) }),
      "POST /api/v1/profile/skills/suggested/Rust/add": () => {
        left = left.filter((skill) => skill.name !== "Rust");
        return { body: found(left) };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SkillsPanel onBack={vi.fn()} />);
    expect(await screen.findByTestId("skill-roles-Rust")).toHaveTextContent("in 4 roles");
    expect(screen.getByTestId("skill-roles-Kafka")).toHaveTextContent("in 1 role");
    await user.click(screen.getByTestId("skill-add-Rust"));
    await waitFor(() => {
      expect(screen.queryByTestId("skill-add-Rust")).toBeNull();
    });
    expect(requests.map(({ route }) => route)).toContain(
      "POST /api/v1/profile/skills/suggested/Rust/add",
    );
    expect(screen.getByTestId("skill-add-Kafka")).toBeVisible();
  });

  it("sets a skill aside with Not mine, and says when none are left", async () => {
    let left = [{ name: "Kafka", roles: 1 }];
    routes({
      "GET /api/v1/profile/skills/suggested": () => ({ body: found(left) }),
      "POST /api/v1/profile/skills/suggested/Kafka/dismiss": () => {
        left = [];
        return { body: found(left) };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SkillsPanel onBack={vi.fn()} />);
    await user.click(await screen.findByTestId("skill-dismiss-Kafka"));
    expect(await screen.findByTestId("skills-none-found")).toHaveTextContent("Nothing left");
  });

  it("moves a skill up or down with the keyboard-reachable buttons", async () => {
    let order: unknown;
    routes({
      "PUT /api/v1/profile/skills/order": async (request) => {
        order = await request.json();
        return { body: view(["Go", "PostgreSQL", "SQL"]) };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SkillsPanel onBack={vi.fn()} />);
    const down = await screen.findByTestId("skill-up-Go");
    down.focus();
    await user.keyboard("{Enter}");
    await waitFor(() => {
      expect(order).toEqual({ names: ["Go", "PostgreSQL", "SQL"] });
    });
    await user.click(await screen.findByTestId("skill-down-PostgreSQL"));
    await waitFor(() => {
      expect(order).toEqual({ names: ["Go", "SQL", "PostgreSQL"] });
    });
  });

  it("waits while the review runs", async () => {
    routes({
      "GET /api/v1/profile/skills/suggested": {
        status: 202,
        body: found([], { status: "running" }),
      },
    });
    renderWithProviders(<SkillsPanel onBack={vi.fn()} />);
    expect(await screen.findByTestId("skills-reading")).toBeVisible();
  });

  it("says without an AI model that skills can still be added by hand", async () => {
    routes({
      "GET /api/v1/profile/skills/suggested": {
        body: found([], { status: "unavailable", reason: "ai_unavailable" }),
      },
    });
    renderWithProviders(<SkillsPanel onBack={vi.fn()} />);
    expect(await screen.findByTestId("skills-unavailable")).toHaveTextContent("add skills by hand");
  });

  it("explains a spending limit and tries again when asked", async () => {
    const requests = routes({
      "GET /api/v1/profile/skills/suggested": {
        body: found([], { status: "failed", reason: "ai_budget_reached" }),
      },
      "POST /api/v1/profile/questions/review": {
        status: 202,
        body: found([], { status: "running" }),
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SkillsPanel onBack={vi.fn()} />);
    expect(await screen.findByTestId("skills-failed")).toHaveTextContent("AI spending limit");
    await user.click(screen.getByTestId("skills-review-again"));
    await waitFor(() => {
      expect(requests.map(({ route }) => route)).toContain("POST /api/v1/profile/questions/review");
    });
  });

  it("goes back to the profile", async () => {
    routes();
    const onBack = vi.fn();
    const user = userEvent.setup();
    renderWithProviders(<SkillsPanel onBack={onBack} />);
    await user.click(await screen.findByTestId("skills-back"));
    expect(onBack).toHaveBeenCalled();
  });
});
