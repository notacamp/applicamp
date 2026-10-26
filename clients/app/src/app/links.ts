import { useNavigate } from "@tanstack/react-router";
import type { MouseEvent } from "react";

/** The places of the signed-in app, in the sidebar's order. */
export const PLACES = [
  { id: "today", path: "/", icon: "home", shortcut: "nav.today" },
  { id: "pipeline", path: "/pipeline", icon: "tasks", shortcut: "nav.pipeline" },
  { id: "agents", path: "/agents", icon: "hourglass", shortcut: "nav.agents" },
  { id: "profile", path: "/profile", icon: "people", shortcut: "nav.profile" },
] as const;

export type Place = (typeof PLACES)[number];
export type PlacePath = Place["path"];

/** The place `pathname` is in. */
export function placeOf(pathname: string): Place["id"] {
  const place = PLACES.find(
    (candidate) =>
      candidate.path !== "/" &&
      (pathname === candidate.path || pathname.startsWith(`${candidate.path}/`)),
  );
  return place?.id ?? "today";
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

/** The import panel of Profile & goals, as an in-app link (Today's "Set up your profile"). */
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
