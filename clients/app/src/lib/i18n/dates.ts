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

/**
 * A date as a CV writes it, in the UI's language: `2021-03` is "Mar 2021",
 * `2021` stays "2021", `2021-03-15` is "15 Mar 2021". Anything else is
 * shown as it is.
 */
export function formatPartialDate(locale: string, value: string): string {
  const match = /^(\d{4})(?:-(\d{2}))?(?:-(\d{2}))?$/.exec(value);
  if (match === null) {
    return value;
  }
  const [, year, month, day] = match;
  if (month === undefined) {
    return year ?? value;
  }
  const date = new Date(
    Date.UTC(Number(year), Number(month) - 1, day === undefined ? 1 : Number(day)),
  );
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "short",
    ...(day === undefined ? {} : { day: "numeric" }),
    timeZone: "UTC",
  }).format(date);
}
