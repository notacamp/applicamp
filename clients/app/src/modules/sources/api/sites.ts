import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryKey,
} from "@tanstack/react-query";
import type { TFunction } from "i18next";

import { api, CSRF_HEADERS, unwrap, type Schema } from "~/lib/api";

/** The person's job sites: the groups, the counts, the sites that do not allow apps. */
export type MySites = Schema<"MySites">;
/** One site on the list: what it offers, its typical salary, On or Off. */
export type MySite = Schema<"MySite">;
export type MySiteGroup = Schema<"MySiteGroup">;
export type SiteGroup = Schema<"SiteGroup">;

/** What the list can show (the Show select). */
export const SITE_SHOWS = ["all", "on", "remote", "europe", "country", "trade"] as const;
export type SiteShow = (typeof SITE_SHOWS)[number];

export const MY_SITES_KEY = ["my-sites"] as const;

/** The list (`GET /api/v1/my-sites`), searched by name and shown by `show`. */
export function mySitesQuery(q: string, show: SiteShow) {
  const search = q.trim();
  return queryOptions({
    queryKey: [...MY_SITES_KEY, search, show] as const,
    queryFn: async ({ signal }): Promise<MySites> =>
      unwrap(
        await api.GET("/api/v1/my-sites", {
          params: {
            query: { ...(search === "" ? {} : { q: search }), ...(show === "all" ? {} : { show }) },
          },
          signal,
        }),
      ),
    // The list moves under the mouse when the previous answer disappears while typing.
    placeholderData: (previous) => previous,
  });
}

export function useMySites(q: string, show: SiteShow) {
  return useQuery(mySitesQuery(q, show));
}

/** `list` with one site set On or Off, and the count of sites that are On kept right. */
export function withSite(list: MySites, id: string, on: boolean): MySites {
  let delta = 0;
  const groups = list.groups.map((group) => ({
    ...group,
    sites: group.sites.map((site) => {
      if (site.id !== id) {
        return site;
      }
      delta = (on ? 1 : 0) - (site.on ? 1 : 0);
      return { ...site, on, set_by: "person" as const };
    }),
  }));
  return { ...list, groups, on: Math.max(0, list.on + delta) };
}

/**
 * Turn a site On or Off (`PUT /api/v1/my-sites/{board_id}`). The button changes at once and goes
 * back if the server refuses.
 */
export function useSetSite() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, on }: { id: string; on: boolean }) =>
      unwrap(
        await api.PUT("/api/v1/my-sites/{board_id}", {
          params: { path: { board_id: id } },
          headers: CSRF_HEADERS,
          body: { on },
        }),
      ),
    onMutate: async ({ id, on }) => {
      await queryClient.cancelQueries({ queryKey: MY_SITES_KEY });
      const before = queryClient.getQueriesData<MySites>({ queryKey: MY_SITES_KEY });
      queryClient.setQueriesData<MySites>({ queryKey: MY_SITES_KEY }, (list) =>
        list === undefined ? list : withSite(list, id, on),
      );
      return { before };
    },
    onError: (_error, _vars, context) => {
      for (const [key, list] of context?.before ?? ([] as [QueryKey, MySites | undefined][])) {
        queryClient.setQueryData(key, list);
      }
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: MY_SITES_KEY }),
  });
}

/** A site's typical salary in the person's currency format ("€96k"). */
export function salaryText(site: MySite, locale: string, t: TFunction<"sources">): string | null {
  if (site.salary_eur_year === undefined || site.salary_eur_year === null) {
    return null;
  }
  const pay = new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "EUR",
    notation: "compact",
    maximumFractionDigits: 0,
  }).format(site.salary_eur_year);
  return site.salary_basis === "country"
    ? t("sites.salary.country", { pay })
    : t("sites.salary.listed", { pay });
}

const REGION_CODE = /^[A-Z]{2}$/;

/** The kinds of jobs a site can be tagged with (the server's fixed list). */
const KINDS = [
  "tech",
  "data",
  "design",
  "product",
  "marketing",
  "sales",
  "finance",
  "legal",
  "people",
  "operations",
  "support",
  "health-care",
  "education",
  "hospitality-travel",
  "retail",
  "logistics",
  "trades",
  "climate",
  "nonprofit",
  "public-sector",
  "media",
  "science",
] as const;
type Kind = (typeof KINDS)[number];

function isKind(value: string): value is Kind {
  return (KINDS as readonly string[]).includes(value);
}

/** The places a site can be tagged with besides one country. */
const PLACES = [
  "worldwide",
  "europe",
  "north-america",
  "latin-america",
  "asia",
  "africa",
  "oceania",
] as const;
type Place = (typeof PLACES)[number];

function isPlace(value: string): value is Place {
  return (PLACES as readonly string[]).includes(value);
}

/** Where a site's jobs are, in words: Europe, North America, Portugal. */
export function placeText(place: string, locale: string, t: TFunction<"sources">): string {
  if (REGION_CODE.test(place)) {
    return new Intl.DisplayNames([locale], { type: "region" }).of(place) ?? place;
  }
  return isPlace(place) ? t(`sites.places.${place}`) : place;
}

/** A list of words in the person's language: "tech, data and design". */
export function listText(items: readonly string[], locale: string): string {
  return new Intl.ListFormat(locale, { style: "long", type: "conjunction" }).format(items);
}

/** One plain line on what a site offers, from what the server says it offers. */
export function siteLine(site: MySite, locale: string, t: TFunction<"sources">): string {
  const kinds = site.kinds.map((kind) => (isKind(kind) ? t(`sites.kinds.${kind}`) : kind));
  const parts: string[] = [];
  if (site.specialities.length > 0) {
    parts.push(t("sites.line.trade", { what: listText(site.specialities, locale) }));
  } else {
    const what =
      kinds.length === 0
        ? t("sites.line.allKinds")
        : t("sites.line.mostly", { kinds: listText(kinds, locale) });
    const place = site.place ?? null;
    if (site.remote === "mostly" && (place === null || place === "worldwide")) {
      parts.push(
        kinds.length === 0
          ? t("sites.line.remoteAll")
          : t("sites.line.remote", { kinds: listText(kinds, locale) }),
      );
    } else if (place === null) {
      parts.push(what);
    } else {
      parts.push(t("sites.line.place", { place: placeText(place, locale, t), what }));
    }
    if (site.remote === "some") {
      parts.push(t("sites.line.someRemote"));
    }
  }
  if (site.needs_key) {
    parts.push(t("sites.line.needsKey"));
  }
  return parts.join(" ");
}
