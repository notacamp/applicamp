import { queryOptions, useQuery } from "@tanstack/react-query";

import { api, unwrap, type Schema } from "~/lib/api";

/** How strong the profile is, and what would raise it (`GET /api/v1/profile/strength`). */
export type StrengthView = Schema<"StrengthView">;
export type StrengthItem = Schema<"StrengthItem">;
export type StrengthKey = Schema<"StrengthKey">;

export const STRENGTH_QUERY_KEY = ["profile", "strength"] as const;

/** The strength (`GET /api/v1/profile/strength`). */
export const strengthQuery = queryOptions({
  queryKey: STRENGTH_QUERY_KEY,
  queryFn: async ({ signal }): Promise<StrengthView> =>
    unwrap(await api.GET("/api/v1/profile/strength", { signal })),
});

export function useStrength() {
  return useQuery(strengthQuery);
}

/** What a part's action asks for. */
export type Action =
  | { kind: "improve" }
  | { kind: "question"; id: string }
  | { kind: "skills" }
  | { kind: "import" }
  | { kind: "goals" }
  | { kind: "edit"; section: "basics" | "work" | "education" | "languages" };

/** The strength item's `action`, as the page acts on it. */
export function parseAction(action: string | null | undefined): Action | null {
  if (action === undefined || action === null) {
    return null;
  }
  if (action === "skills" || action === "import" || action === "goals") {
    return { kind: action };
  }
  if (action.startsWith("question:")) {
    return { kind: "question", id: action.slice("question:".length) };
  }
  if (action.startsWith("edit:")) {
    const section = action.slice("edit:".length);
    if (
      section === "basics" ||
      section === "work" ||
      section === "education" ||
      section === "languages"
    ) {
      return { kind: "edit", section };
    }
  }
  return null;
}
