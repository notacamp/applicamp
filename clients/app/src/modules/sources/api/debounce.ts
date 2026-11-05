import { useEffect, useState } from "react";

/** How long a search waits for the typing to stop. */
export const TYPING_MS = 250;

/** `value`, once it has stopped changing for `ms`. */
export function useDebounced<T>(value: T, ms: number = TYPING_MS): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => {
      setSettled(value);
    }, ms);
    return () => {
      clearTimeout(timer);
    };
  }, [value, ms]);
  return settled;
}
