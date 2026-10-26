// Translations at parity: every `i18n/` folder under src holds en.json,
// pt-PT.json, es-ES.json and fr.json with the same keys, no empty string, and
// the same {{placeholders}} as English. Run with `pnpm i18n:check`.
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const LOCALES = ["en", "pt-PT", "es-ES", "fr"] as const;
const root = fileURLToPath(new URL("../src", import.meta.url));

/** Every `i18n` folder of messages (with an en.json) under `dir`. */
function folders(dir: string): string[] {
  const found: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      found.push(
        ...(name === "i18n" && existsSync(join(path, "en.json")) ? [path] : folders(path)),
      );
    }
  }
  return found;
}

/** `{ a: { b: "x" } }` → `{ "a.b": "x" }`. */
function flatten(value: unknown, prefix = ""): Map<string, unknown> {
  const flat = new Map<string, unknown>();
  if (typeof value === "object" && value !== null && !Array.isArray(value)) {
    for (const [key, child] of Object.entries(value)) {
      for (const [path, leaf] of flatten(child, prefix === "" ? key : `${prefix}.${key}`)) {
        flat.set(path, leaf);
      }
    }
  } else {
    flat.set(prefix, value);
  }
  return flat;
}

const placeholders = (text: unknown): string =>
  typeof text === "string"
    ? [...text.matchAll(/\{\{\s*(\w+)\s*\}\}/g)]
        .map((match) => match[1])
        .sort()
        .join(",")
    : "";

const problems: string[] = [];
for (const folder of folders(root)) {
  const where = relative(root, folder);
  const read = (locale: string) =>
    flatten(JSON.parse(readFileSync(join(folder, `${locale}.json`), "utf8")) as unknown);
  const english = read("en");
  for (const locale of LOCALES) {
    let messages: Map<string, unknown>;
    try {
      messages = read(locale);
    } catch (error) {
      problems.push(`${where}/${locale}.json: ${String(error)}`);
      continue;
    }
    for (const [key, text] of messages) {
      if (typeof text !== "string" || text.trim() === "") {
        problems.push(`${where}/${locale}.json: "${key}" is empty or not a string`);
      }
      if (!english.has(key)) {
        problems.push(`${where}/${locale}.json: "${key}" is not in en.json`);
      }
    }
    for (const [key, text] of english) {
      if (!messages.has(key)) {
        problems.push(`${where}/${locale}.json: "${key}" is missing`);
      } else if (placeholders(messages.get(key)) !== placeholders(text)) {
        problems.push(`${where}/${locale}.json: "${key}" has other {{placeholders}} than en.json`);
      }
    }
  }
}

if (problems.length > 0) {
  console.error(problems.join("\n"));
  process.exitCode = 1;
} else {
  console.log("Translations at parity (en, pt-PT, es-ES, fr).");
}
