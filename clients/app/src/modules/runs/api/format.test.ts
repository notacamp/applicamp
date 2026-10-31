import { describe, expect, it } from "vitest";

import { createI18n } from "~/lib/i18n";

import { NAMESPACES } from "../../../app/messages";
import { run } from "../../../../test/runs";

import { formatCost, formatCount, formatDuration, formatStarted } from "./format";
import { badgeOf, failureOf, kindLabel, startedText, statusOf } from "./labels";
import { startErrorOf } from "./runs";
import { streamUrl } from "./stream";

const t = createI18n({ locale: "en", namespaces: NAMESPACES }).getFixedT("en", "runs");

describe("a run's facts in words", () => {
  it("says how long it took, in seconds, minutes or hours", () => {
    expect(formatDuration(400, "en")).toBe("0 sec");
    expect(formatDuration(4200, "en")).toBe("4 sec");
    expect(formatDuration(125_000, "en")).toBe("2 min 5 sec");
    expect(formatDuration(3_725_000, "en")).toBe("1 hr 2 min");
    expect(formatDuration(-5, "en")).toBe("0 sec");
  });

  it("counts and costs in the person's language", () => {
    expect(formatCount(12_345, "en")).toBe("12,345");
    expect(formatCost(0, "en")).toBe("€0.00");
    expect(formatCost(0.004, "en")).toBe("€0.004");
    expect(formatCost(1.5, "en")).toBe("€1.50");
  });

  it("says when it started: a time today, else a date", () => {
    const now = new Date(2026, 9, 31, 15, 0);
    expect(formatStarted(new Date(2026, 9, 31, 14, 2).toISOString(), "en", now).today).toBe(true);
    expect(formatStarted(new Date(2026, 9, 30, 9, 0).toISOString(), "en", now).today).toBe(false);
    const asked = run({ started_at: null, created_at: new Date(2026, 9, 30, 9, 0).toISOString() });
    const time = formatStarted(asked.created_at, "en", new Date(2026, 9, 30, 12)).text;
    expect(startedText(asked, "en", t, new Date(2026, 9, 30, 12))).toBe(`Today ${time}`);
    expect(startedText(asked, "en", t, now)).toBe(formatStarted(asked.created_at, "en", now).text);
  });

  it("names the kinds it knows, and the others by their name", () => {
    expect(kindLabel("runs.probe", t)).toBe("Check");
    expect(kindLabel("sources.search", t)).toBe("Run of sources.search");
  });

  it("says where a run is, and how it ended, in words", () => {
    expect(statusOf(run({ status: "queued", outcome: null }), t)).toEqual({
      tone: "waiting",
      text: "Waiting to start",
    });
    expect(statusOf(run({ status: "running", outcome: null, stopping: true }), t).text).toBe(
      "Stopping…",
    );
    expect(statusOf(run(), t)).toEqual({ tone: "ok", text: "Succeeded" });
    expect(statusOf(run({ outcome: "ended_timeout" }), t)).toEqual({
      tone: "problem",
      text: "Stopped: it ran out of time",
    });
    expect(statusOf(run({ outcome: "cancelled" }), t).text).toBe("Stopped by you");
    expect(statusOf(run({ outcome: null }), t).text).toBe("Failed");
  });

  it("says why a failed run failed", () => {
    expect(failureOf(run(), t)).toBeNull();
    expect(failureOf(run({ outcome: "failed", error_kind: "sandbox_create_failed" }), t)).toBe(
      "Its sandbox couldn’t be made.",
    );
    expect(failureOf(run({ outcome: "failed", error_kind: "something_new" }), t)).toBe(
      "Something went wrong while it ran.",
    );
    expect(failureOf(run({ outcome: "failed", error_kind: null }), t)).toBe(
      "Something went wrong while it ran.",
    );
  });

  it("marks each kind of line, a blocked request in red", () => {
    const line = (level: "info" | "tool" | "model" | "deny" | "ask" | "error", message = "x") =>
      badgeOf({ layer: "sandbox", level, message }, t);
    expect(line("deny")).toEqual({ hue: "red", text: "Blocked" });
    expect(line("error")).toEqual({ hue: "red", text: "Problem" });
    expect(line("tool")).toEqual({ hue: "blue", text: "Tool" });
    expect(line("model")).toEqual({ hue: "purple", text: "Model" });
    expect(line("ask")).toEqual({ hue: "yellow", text: "Needs you" });
    expect(line("info", "NET:OPEN [INFO] ALLOWED a -> b:443")).toEqual({
      hue: "green",
      text: "Allowed",
    });
    expect(line("info")).toBeNull();
    expect(badgeOf({ layer: "agent", level: "info", message: "ALLOWED" }, t)).toBeNull();
  });

  it("knows the problems starting a check answers with", () => {
    expect(startErrorOf(new Error("network"))).toBe("generic");
  });

  it("resumes the live stream after a line", () => {
    expect(streamUrl("abc", 0)).toBe("/api/v1/runs/abc/events/stream");
    expect(streamUrl("abc", 7)).toBe("/api/v1/runs/abc/events/stream?after=7");
  });
});
