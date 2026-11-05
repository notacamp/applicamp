import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { SUPPORTED_LOCALES, type Locale } from "~/lib/i18n";

import { renderWithProviders } from "../../../../../test/render";
import type { Waiting } from "../../api/waiting";

import { WhyLine } from "./WhyLine";

const CODES = [
  "needs_key",
  "list_not_allowed",
  "no_job_list",
  "request_limit",
  "empty_list",
  "jobs_on_hiring_system",
  "not_a_job_board",
  "nothing_recent",
  "undated",
  "terms_forbid",
  "terms_ask_first",
  "terms_personal_only",
  "terms_unread",
  "owner_edited",
  "config_to_use",
  "needs_approval",
] as const;

function why(waiting?: Waiting | null, locale: Locale = "en") {
  renderWithProviders(<WhyLine waiting={waiting} />, { locale });
  return screen.queryByTestId("boards-admin-why");
}

describe("why a site waits", () => {
  it("says the sentence for the code, with its detail", () => {
    expect(why({ code: "request_limit", detail: "25" })?.textContent).toBe(
      "The check used its 25 requests before it found where the jobs are.",
    );
  });

  it("names the hiring system", () => {
    expect(why({ code: "jobs_on_hiring_system", detail: "Lever" })?.textContent).toContain(
      "on Lever",
    );
  });

  it("quotes the sentence of the terms that decided it", () => {
    expect(why({ code: "terms_forbid", detail: "No apps." })?.textContent).toBe(
      "Its terms do not allow apps to read it. “No apps.”",
    );
    expect(why({ code: "terms_forbid" })?.textContent).toBe(
      "Its terms do not allow apps to read it.",
    );
  });

  it("writes a date in the person's language", () => {
    const text = why({ code: "nothing_recent", detail: "2026-08-07" }, "fr")?.textContent ?? "";
    expect(text).toContain("aoû");
    expect(text).toContain("plus de 30 jours");
  });

  it("falls back to a generic sentence for a code it does not know", () => {
    const unknown = { code: "something_new" } as unknown as Waiting;
    expect(why(unknown)?.textContent).toBe(
      "It waits for you, for a reason the app cannot put into words yet.",
    );
  });

  it("shows nothing without a reason", () => {
    expect(why(null)).toBeNull();
    expect(why()).toBeNull();
  });

  it("has a sentence in every language for every code, in plain words", () => {
    for (const locale of SUPPORTED_LOCALES) {
      for (const code of CODES) {
        const { unmount } = renderWithProviders(<WhyLine waiting={{ code, detail: "Lever" }} />, {
          locale,
        });
        const text = screen.getByTestId("boards-admin-why").textContent;
        expect(text, `${locale} ${code}`).not.toContain("admin.why");
        expect(text, `${locale} ${code}`).not.toMatch(/RSS|robots|API|[()]/);
        unmount();
      }
    }
  });
});
