/** When something happened, for a status line: "14:02" today, else a short date and time. */
export function formatWhen(
  iso: string,
  locale: string,
  now: Date = new Date(),
): {
  today: boolean;
  text: string;
} {
  const at = new Date(iso);
  const today =
    at.getFullYear() === now.getFullYear() &&
    at.getMonth() === now.getMonth() &&
    at.getDate() === now.getDate();
  const text = today
    ? new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" }).format(at)
    : new Intl.DateTimeFormat(locale, {
        day: "numeric",
        month: "short",
        hour: "2-digit",
        minute: "2-digit",
      }).format(at);
  return { today, text };
}

/** Whether `iso` is within the last day (a nightly search "last night"). */
export function withinADay(iso: string, now: Date = new Date()): boolean {
  return now.getTime() - new Date(iso).getTime() < 24 * 60 * 60 * 1000;
}
