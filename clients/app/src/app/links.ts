import { useNavigate } from "@tanstack/react-router";
import type { MouseEvent } from "react";

/**
 * The places of the signed-in app, in the sidebar's order. `phone: false` keeps one off
 * the tab bar, which has room for five (it is still in the sidebar and ⌘K).
 */
export const PLACES = [
  { id: "today", path: "/", icon: "home", shortcut: "nav.today", phone: true },
  { id: "pipeline", path: "/pipeline", icon: "tasks", shortcut: "nav.pipeline", phone: true },
  { id: "boards", path: "/boards", icon: "bullets", shortcut: "nav.boards", phone: true },
  { id: "cvs", path: "/cvs", icon: "document", shortcut: "nav.cvs", phone: false },
  { id: "agents", path: "/agents", icon: "hourglass", shortcut: "nav.agents", phone: true },
  { id: "profile", path: "/profile", icon: "people", shortcut: "nav.profile", phone: true },
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
