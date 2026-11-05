// Invented job sites for the client's tests: the person's list, grouped by what the sites offer.
import type { Schema } from "~/lib/api";

type MySites = Schema<"MySites">;
type MySite = Schema<"MySite">;

export const HIMALAYAS_SITE = "0190a1b2-0000-7000-8000-0000000000e1";
export const RUBY_SITE = "0190a1b2-0000-7000-8000-0000000000e2";
export const KEYED_SITE = "0190a1b2-0000-7000-8000-0000000000e3";
export const EUROPE_SITE = "0190a1b2-0000-7000-8000-0000000000e4";

/** A remote-first tech site, On. */
export function site(overrides: Partial<MySite> = {}): MySite {
  return {
    id: HIMALAYAS_SITE,
    name: "Himalayas",
    group: "remote_worldwide",
    kinds: ["tech"],
    place: "worldwide",
    remote: "mostly",
    specialities: [],
    salary_eur_year: 96_000,
    salary_basis: "listed",
    on: true,
    set_by: "app",
    needs_key: false,
    ...overrides,
  };
}

/** Four sites in four groups, three On or Off by the app and one that needs a key. */
export function sites(overrides: Partial<MySites> = {}): MySites {
  return {
    available: 4,
    on: 2,
    country: "PT",
    groups: [
      { group: "remote_worldwide", sites: [site()] },
      {
        group: "europe",
        sites: [
          site({
            id: EUROPE_SITE,
            name: "Euro Jobs",
            group: "europe",
            place: "europe",
            remote: "some",
            salary_eur_year: 72_000,
          }),
        ],
      },
      {
        group: "country",
        sites: [
          site({
            id: KEYED_SITE,
            name: "Jobs Portugal",
            group: "country",
            place: "PT",
            remote: "some",
            salary_eur_year: 36_000,
            salary_basis: "country",
            on: false,
            needs_key: true,
          }),
        ],
      },
      {
        group: "trade",
        sites: [
          site({
            id: RUBY_SITE,
            name: "Ruby on Remote",
            group: "trade",
            specialities: ["ruby", "rails"],
            salary_eur_year: null,
            salary_basis: null,
            on: false,
          }),
        ],
      },
    ],
    blocked: { count: 2, names: ["Dice", "Wellfound"] },
    company_pages: 642,
    ...overrides,
  };
}
