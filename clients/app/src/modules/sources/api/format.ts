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

/** A day for a sentence ("4 November"); English reads day first. */
export function formatDay(iso: string, locale: string): string {
  return new Intl.DateTimeFormat(locale === "en" ? "en-GB" : locale, {
    day: "numeric",
    month: "long",
  }).format(new Date(iso));
}

/** A tag as the person reads it: the fixed ones translated, the rest capitalised. */
export function tagText(
  tag: string,
  t: (key: string) => string,
  known: (key: string) => boolean,
): string {
  for (const key of [`board.tags.kind.${tag}`, `board.tags.remote.${tag}`]) {
    if (known(key)) {
      return t(key);
    }
  }
  const words = tag.replace(/-/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}
