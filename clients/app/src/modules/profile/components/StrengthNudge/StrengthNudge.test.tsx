import { screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { readPref, writePref } from "~/lib/storage/prefs";

import { renderWithProviders } from "../../../../../test/render";

import { StrengthNudge } from "./StrengthNudge";

const NOW = new Date("2026-10-28T10:00:00Z");
const link = { href: "/profile/improve", onClick: vi.fn() };

describe("Today's profile strength card", () => {
  afterEach(() => {
    // An empty time is no time: the card is not put away.
    writePref("profile.nudge", "");
  });

  it("says how strong the profile is and how many questions would sharpen it", () => {
    renderWithProviders(
      <StrengthNudge score={45} questions={9} action="improve" link={link} now={NOW} />,
    );
    const card = screen.getByTestId("today-profile-nudge");
    expect(card).toHaveTextContent("Your profile is 45% strong.");
    expect(card).toHaveTextContent("9 quick questions");
    expect(screen.getByTestId("nudge-continue")).toHaveAttribute("href", "/profile/improve");
  });

  it("names the other ways on", () => {
    renderWithProviders(
      <StrengthNudge score={45} questions={null} action="skills" link={link} now={NOW} />,
    );
    expect(screen.getByTestId("today-profile-nudge")).toHaveTextContent("Add the skills");
  });

  it("is put away for a week by Not now, and comes back after", async () => {
    const user = userEvent.setup();
    const first = renderWithProviders(
      <StrengthNudge score={45} questions={9} action="improve" link={link} now={NOW} />,
    );
    await user.click(screen.getByTestId("nudge-not-now"));
    expect(screen.queryByTestId("today-profile-nudge")).toBeNull();
    expect(readPref("profile.nudge")).toBe("2026-11-04T10:00:00.000Z");
    first.unmount();

    const sixDays = new Date("2026-11-03T10:00:00Z");
    const during = renderWithProviders(
      <StrengthNudge score={45} questions={9} action="improve" link={link} now={sixDays} />,
    );
    expect(screen.queryByTestId("today-profile-nudge")).toBeNull();
    during.unmount();

    renderWithProviders(
      <StrengthNudge
        score={45}
        questions={9}
        action="improve"
        link={link}
        now={new Date("2026-11-05T10:00:00Z")}
      />,
    );
    expect(screen.getByTestId("today-profile-nudge")).toBeVisible();
  });
});
