import { useNavigate } from "@tanstack/react-router";
import type { MouseEvent } from "react";

/**
 * The three places of the signed-in app, in the sidebar's and the tab bar's
 * order: Today, Jobs and You. Every other page is reached from one of them,
 * or from Search, and keeps that place marked as the current one.
 */
export const PLACES = [
  { id: "today", path: "/", icon: "home" },
  { id: "pipeline", path: "/pipeline", icon: "tasks" },
  { id: "profile", path: "/profile", icon: "people" },
] as const;

export type Place = (typeof PLACES)[number];
export type PlacePath = Place["path"];

/**
 * The pages that are not places, each under the place it is reached from:
 * Job sites and Companies under You, Agents under Today, CVs under Jobs.
 */
const HOMES: readonly (readonly [path: string, place: Place["id"]])[] = [
  ["/boards", "profile"],
  ["/companies", "profile"],
  ["/agents", "today"],
  ["/cvs", "pipeline"],
];

/** The place `pathname` is in, or is reached from. */
export function placeOf(pathname: string): Place["id"] {
  const within = (path: string) => pathname === path || pathname.startsWith(`${path}/`);
  const place = PLACES.find((candidate) => candidate.path !== "/" && within(candidate.path));
  if (place !== undefined) {
    return place.id;
  }
  const home = HOMES.find(([path]) => within(path));
  return home === undefined ? "today" : home[1];
}

/**
 * An in-app link: a real `href` (it opens in a new tab with ⌘ or a middle
 * click), followed without a reload otherwise.
 */
export function useInAppLink() {
  const navigate = useNavigate();
  return (to: PlacePath) => ({
    href: to,
    onClick: (event: MouseEvent<HTMLElement>) => {
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) {
        return;
      }
      event.preventDefault();
      void navigate({ to });
    },
  });
}

/** The import panel of You, as an in-app link (Today's "Set up your profile"). */
export function useImportLink() {
  const navigate = useNavigate();
  return {
    href: "/profile?import=new",
    onClick: (event: MouseEvent<HTMLElement>) => {
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) {
        return;
      }
      event.preventDefault();
      void navigate({ to: "/profile", search: { import: "new" } });
    },
  };
}
