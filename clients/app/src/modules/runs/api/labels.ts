import type { TFunction } from "i18next";

import type { LabelHue, StatusTone } from "~/lib/ui";

import { formatStarted } from "./format";
import { PROBE, type Outcome, type Run } from "./runs";
import type { RunEvent } from "./stream";

/** A run's kind in words ("Check"); a kind this client doesn't know, by its name. */
export function kindLabel(kind: string, t: TFunction<"runs">): string {
  return kind === PROBE ? t("kinds.probe") : t("kinds.other", { kind });
}

/** How each end state reads: fine, needs someone, or went wrong. */
const OUTCOME_TONES: Record<Outcome, StatusTone> = {
  succeeded: "ok",
  nothing_new: "ok",
  needs_you: "waiting",
  ended_max_steps: "problem",
  ended_budget: "problem",
  ended_timeout: "problem",
  ended_loop: "problem",
  failed: "problem",
  cancelled: "waiting",
};

/** Where a run is, in words, with the tone of its dot. */
export function statusOf(
  run: Pick<Run, "status" | "outcome" | "stopping">,
  t: TFunction<"runs">,
): { tone: StatusTone; text: string } {
  if (run.status !== "finished") {
    return {
      tone: "waiting",
      text: run.stopping ? t("status.stopping") : t(`status.${run.status}`),
    };
  }
  const outcome = run.outcome ?? "failed";
  return { tone: OUTCOME_TONES[outcome], text: t(`outcome.${outcome}`) };
}

/** The failures with words of their own. */
const FAILURES = [
  "sandbox_create_failed",
  "agent_exited",
  "server_restarted",
  "runtime_failed",
  "start_failed",
] as const;

function isKnownFailure(kind: string): kind is (typeof FAILURES)[number] {
  return (FAILURES as readonly string[]).includes(kind);
}

/** Why a failed run failed, in words (`null` when it did not fail). */
export function failureOf(
  run: Pick<Run, "outcome" | "error_kind">,
  t: TFunction<"runs">,
): string | null {
  if (run.outcome !== "failed") {
    return null;
  }
  const kind = run.error_kind ?? "";
  return isKnownFailure(kind) ? t(`failure.${kind}`) : t("failure.generic");
}

/** When a run started (or was asked for, while it waits): "Today 14:02", "30 Oct, 09:00". */
export function startedText(
  run: Pick<Run, "started_at" | "created_at">,
  locale: string,
  t: TFunction<"runs">,
  now: Date = new Date(),
): string {
  const when = formatStarted(run.started_at ?? run.created_at, locale, now);
  return when.today ? t("when.today", { time: when.text }) : when.text;
}

/** A sandbox line the proxy let through. */
function isAllowed(event: Pick<RunEvent, "layer" | "message">): boolean {
  return event.layer === "sandbox" && event.message.split(/\s+/).includes("ALLOWED");
}

/** What kind of line an event is, in words with a colour; none for a plain step. */
export function badgeOf(
  event: Pick<RunEvent, "layer" | "level" | "message">,
  t: TFunction<"runs">,
): { hue: LabelHue; text: string } | null {
  switch (event.level) {
    case "deny": {
      return { hue: "red", text: t("level.deny") };
    }
    case "error": {
      return { hue: "red", text: t("level.error") };
    }
    case "tool": {
      return { hue: "blue", text: t("level.tool") };
    }
    case "model": {
      return { hue: "purple", text: t("level.model") };
    }
    case "ask": {
      return { hue: "yellow", text: t("level.ask") };
    }
    case "info": {
      return isAllowed(event) ? { hue: "green", text: t("level.allowed") } : null;
    }
  }
}
