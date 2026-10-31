/** Whether `at` is on the same day as `now`. */
function sameDay(at: Date, now: Date): boolean {
  return (
    at.getFullYear() === now.getFullYear() &&
    at.getMonth() === now.getMonth() &&
    at.getDate() === now.getDate()
  );
}

/** When a run started, for its row: "14:02" today, else a short date and time. */
export function formatStarted(
  iso: string,
  locale: string,
  now: Date = new Date(),
): { today: boolean; text: string } {
  const at = new Date(iso);
  const today = sameDay(at, now);
  const text = new Intl.DateTimeFormat(
    locale,
    today
      ? { hour: "2-digit", minute: "2-digit" }
      : { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" },
  ).format(at);
  return { today, text };
}

/** A log line's time, to the second ("14:02:07"). */
export function formatClock(iso: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(iso));
}

/** How long a run took: "4 sec", "2 min, 5 sec", "under a second" is "0 sec". */
export function formatDuration(ms: number, locale: string): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const unit = (value: number, name: "second" | "minute" | "hour") =>
    new Intl.NumberFormat(locale, { style: "unit", unit: name, unitDisplay: "short" }).format(
      value,
    );
  if (total < 60) {
    return unit(total, "second");
  }
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  const parts =
    hours > 0
      ? [unit(hours, "hour"), unit(minutes, "minute")]
      : [unit(minutes, "minute"), unit(seconds, "second")];
  return new Intl.ListFormat(locale, { style: "narrow", type: "unit" }).format(parts);
}

/** A count in `locale` ("1,234"). */
export function formatCount(value: number, locale: string): string {
  return new Intl.NumberFormat(locale).format(value);
}

/** What a run's model calls cost: "€0.00", "€0.004" (small sums keep their cents). */
export function formatCost(value: number, locale: string): string {
  const digits = value > 0 && value < 0.01 ? 3 : 2;
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "EUR",
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
}
