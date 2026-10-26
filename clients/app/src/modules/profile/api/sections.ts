// The profile's lists as the page edits and compares them: one spec per
// list, naming its fields, and the same "is this the same entry" rule the
// server merges with (crates/modules/profile/src/schema.rs).
import { present, type Basics, type Profile } from "./profile";

/** The profile's lists. */
export const LISTS = [
  "work",
  "education",
  "skills",
  "languages",
  "certificates",
  "projects",
] as const;
export type ListKey = (typeof LISTS)[number];
/** One entry of any list. */
export type Entry = Profile[ListKey][number];

/** A field's kind: its control and its check. */
export type FieldKind = "short" | "long" | "date" | "lines";

/** Every field an entry can have, by its JSON name. */
export type FieldKey =
  | "position"
  | "name"
  | "location"
  | "startDate"
  | "endDate"
  | "summary"
  | "highlights"
  | "institution"
  | "studyType"
  | "area"
  | "level"
  | "language"
  | "fluency"
  | "issuer"
  | "date"
  | "description"
  | "url";

/** A field's label, under `fields.` in the module's messages. */
export type FieldLabel =
  | "title"
  | "company"
  | "location"
  | "start"
  | "end"
  | "summary"
  | "highlights"
  | "school"
  | "degree"
  | "field"
  | "skill"
  | "level"
  | "language"
  | "fluency"
  | "certification"
  | "issuer"
  | "date"
  | "project"
  | "description"
  | "link";

export interface FieldSpec {
  key: FieldKey;
  label: FieldLabel;
  kind: FieldKind;
  /** The entry's key value: it cannot be empty. */
  required?: boolean;
}

/** The fields of each list, in the order the form shows them. */
export const FIELDS: Record<ListKey, readonly FieldSpec[]> = {
  work: [
    { key: "position", label: "title", kind: "short" },
    { key: "name", label: "company", kind: "short" },
    { key: "location", label: "location", kind: "short" },
    { key: "startDate", label: "start", kind: "date" },
    { key: "endDate", label: "end", kind: "date" },
    { key: "summary", label: "summary", kind: "long" },
    { key: "highlights", label: "highlights", kind: "lines" },
  ],
  education: [
    { key: "institution", label: "school", kind: "short", required: true },
    { key: "studyType", label: "degree", kind: "short" },
    { key: "area", label: "field", kind: "short" },
    { key: "startDate", label: "start", kind: "date" },
    { key: "endDate", label: "end", kind: "date" },
  ],
  skills: [
    { key: "name", label: "skill", kind: "short", required: true },
    { key: "level", label: "level", kind: "short" },
  ],
  languages: [
    { key: "language", label: "language", kind: "short", required: true },
    { key: "fluency", label: "fluency", kind: "short" },
  ],
  certificates: [
    { key: "name", label: "certification", kind: "short", required: true },
    { key: "issuer", label: "issuer", kind: "short" },
    { key: "date", label: "date", kind: "date" },
    { key: "url", label: "link", kind: "short" },
  ],
  projects: [
    { key: "name", label: "project", kind: "short", required: true },
    { key: "description", label: "description", kind: "long" },
    { key: "startDate", label: "start", kind: "date" },
    { key: "endDate", label: "end", kind: "date" },
    { key: "url", label: "link", kind: "short" },
  ],
};

/** A form's values: every field as text. */
export type Values = Partial<Record<FieldKey, string>>;

/** `entry`'s field `key`, whatever its type. */
function field(entry: Entry, key: FieldKey): unknown {
  return (entry as Partial<Record<FieldKey, unknown>>)[key];
}

/** `entry`'s field `key` as text (`highlights`: one per line). */
export function textOf(entry: Entry, key: FieldKey): string | undefined {
  const value = field(entry, key);
  if (Array.isArray(value)) {
    const lines = value.filter((line): line is string => typeof line === "string");
    return lines.length > 0 ? lines.join("\n") : undefined;
  }
  return typeof value === "string" && value !== "" ? value : undefined;
}

/** The form values of `entry` in `list`. */
export function valuesOf(list: ListKey, entry: Entry | undefined): Values {
  const values: Values = {};
  for (const spec of FIELDS[list]) {
    values[spec.key] = entry === undefined ? "" : (textOf(entry, spec.key) ?? "");
  }
  return values;
}

/** The entry the form `values` describe (empty values left out). */
export function entryOf(list: ListKey, values: Values): Entry {
  const entry: Record<string, unknown> = {};
  for (const spec of FIELDS[list]) {
    const text = (values[spec.key] ?? "").trim();
    if (spec.kind === "lines") {
      entry[spec.key] = text
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => line !== "");
    } else if (text !== "" || spec.required === true) {
      entry[spec.key] = text;
    }
  }
  if (list === "work" && !("highlights" in entry)) {
    entry["highlights"] = [];
  }
  return entry as unknown as Entry;
}

/** `YYYY`, `YYYY-MM` or `YYYY-MM-DD`, as the server takes dates. */
const DATE = /^\d{4}(?:-(?:0[1-9]|1[0-2])(?:-(?:0[1-9]|[12]\d|3[01]))?)?$/;

/** What is wrong with `values`, by field: an empty key value, a bad date. */
export function problemsOf(
  list: ListKey,
  values: Values,
): Partial<Record<FieldKey, "required" | "date">> {
  const problems: Partial<Record<FieldKey, "required" | "date">> = {};
  for (const spec of FIELDS[list]) {
    const text = (values[spec.key] ?? "").trim();
    if (spec.required === true && text === "") {
      problems[spec.key] = "required";
    } else if (spec.kind === "date" && text !== "" && !DATE.test(text)) {
      problems[spec.key] = "date";
    }
  }
  if (list === "work" && !present(values.position) && !present(values.name)) {
    problems.position = "required";
  }
  return problems;
}

/** `value` as entries are compared: case and spacing do not matter. */
function same(value: string | undefined): string {
  return (value ?? "").trim().replaceAll(/\s+/g, " ").toLowerCase();
}

/**
 * Whether `a` and `b` are the same entry of `list` (the server's merge
 * rule): a position by its company and its start or title, a school by its
 * name and degree, anything else by its name.
 */
export function isSameEntry(list: ListKey, a: Entry, b: Entry): boolean {
  const eq = (key: FieldKey) => same(textOf(a, key)) === same(textOf(b, key));
  switch (list) {
    case "work": {
      return eq("name") && (eq("startDate") || eq("position"));
    }
    case "education": {
      return eq("institution") && eq("studyType");
    }
    case "languages": {
      return eq("language");
    }
    default: {
      return eq("name");
    }
  }
}

/** How a draft's value compares with the profile's. */
export type Change = "new" | "changed" | "same";

/** Every entry of `draft`'s `list`, with how it compares with `current`'s. */
export function compareList(
  list: ListKey,
  current: Profile,
  draft: Profile,
): { entry: Entry; change: Change }[] {
  const mine: readonly Entry[] = current[list];
  const theirs: readonly Entry[] = draft[list];
  return theirs.map((entry): { entry: Entry; change: Change } => {
    const match = mine.find((known) => isSameEntry(list, known, entry));
    if (match === undefined) {
      return { entry, change: "new" };
    }
    const differs = FIELDS[list].some(
      (spec) => same(textOf(match, spec.key)) !== same(textOf(entry, spec.key)),
    );
    return { entry, change: differs ? "changed" : "same" };
  });
}

/** The basics fields, in the order the page shows them. */
export const BASICS = ["name", "label", "location", "email", "url", "summary"] as const;
export type BasicsKey = (typeof BASICS)[number];

/** How the draft's `key` compares with the profile's (none when the draft has none). */
export function compareBasics(key: BasicsKey, current: Basics, draft: Basics): Change | undefined {
  const theirs = draft[key];
  if (!present(theirs)) {
    return undefined;
  }
  const mine = current[key];
  if (!present(mine)) {
    return "new";
  }
  return same(mine) === same(theirs) ? "same" : "changed";
}

/** The profile with `list` set to `entries`. */
export function withList<K extends ListKey>(
  profile: Profile,
  list: K,
  entries: Profile[K],
): Profile {
  const next: Profile = { ...profile };
  next[list] = entries;
  return next;
}
