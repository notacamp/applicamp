import { useSyncExternalStore } from "react";

/** Below this the app frame becomes the phone's: header on top, tab bar below. */
export const NARROW_QUERY = "(width < 48rem)";

function subscribe(onChange: () => void): () => void {
  const list = globalThis.matchMedia(NARROW_QUERY);
  list.addEventListener("change", onChange);
  return () => {
    list.removeEventListener("change", onChange);
  };
}

const narrowNow = () => globalThis.matchMedia(NARROW_QUERY).matches;

/**
 * Whether the screen is a phone's (under 48rem), following rotation and
 * resizing: for the few views that change form there, not only layout (a
 * table becomes a list of cards).
 */
export function useNarrowScreen(): boolean {
  return useSyncExternalStore(subscribe, narrowNow, () => false);
}
