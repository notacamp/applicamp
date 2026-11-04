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

/** A site's host without `www.` (`https://www.example.com/a` is `example.com`); none for a bad link. */
export function hostOf(url: string | null | undefined): string | null {
  if (url === null || url === undefined) {
    return null;
  }
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return null;
  }
}

/** "28 Oct": a day and a short month, in the person's language. */
export function shortDate(iso: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, { day: "numeric", month: "short" }).format(new Date(iso));
}
