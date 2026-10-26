/**
 * A day as a page's eyebrow says it, in the UI's language: "Sunday,
 * October 25". `now` defaults to the moment it is asked.
 */
export function formatDay(locale: string, now: Date = new Date()): string {
  return new Intl.DateTimeFormat(locale, {
    weekday: "long",
    month: "long",
    day: "numeric",
  }).format(now);
}
