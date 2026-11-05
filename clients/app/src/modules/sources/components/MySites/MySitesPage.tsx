import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  RowList,
  Section,
  Select,
  SkeletonList,
  Stack,
  Tagged,
  Text,
  TextField,
} from "~/lib/ui";

import { useDebounced } from "../../api/debounce";
import {
  listText,
  placeText,
  SITE_SHOWS,
  useMySites,
  useSetSite,
  type MySites,
  type SiteGroup,
  type SiteShow,
} from "../../api/sites";
import { BoardPeek } from "../BoardPeek/BoardPeek";
import { CompaniesSection } from "./CompaniesSection";
import { SiteRow } from "./SiteRow";

/** Names of sites that do not allow apps said in the sentence, before "and N more". */
const NAMED = 3;

export interface MySitesPageProps {
  /** The person manages the job boards: "Add the key" takes them to the key. */
  canManage: boolean;
}

function groupTitle(
  group: SiteGroup,
  country: string | null,
  locale: string,
  t: TFunction<"sources">,
) {
  if (group === "country" && country !== null) {
    return placeText(country, locale, t);
  }
  return t(`sites.groups.${group}`);
}

/** The one sentence about sites that do not allow apps, or null when there are none. */
function blockedSentence(
  blocked: MySites["blocked"],
  locale: string,
  t: TFunction<"sources">,
): string | null {
  if (blocked.count === 0) {
    return null;
  }
  const names = blocked.names.slice(0, NAMED);
  const rest = blocked.count - names.length;
  return rest > 0
    ? t("sites.blocked.more", { names: names.join(", "), rest })
    : t("sites.blocked.sentence", { names: listText(names, locale), count: blocked.count });
}

/**
 * The Job sites a person has On: one list grouped by what the sites offer, each site On or Off.
 * The app has turned on the ones that fit their goals; their own choice is kept. Above the list a
 * "Find a site" box and one "Show" select; under it the sites that do not allow apps in one
 * sentence, how many company pages are also read, and the Companies to follow.
 */
export function MySitesPage({ canManage }: MySitesPageProps) {
  const { t, i18n } = useTranslation("sources");
  const [search, setSearch] = useState("");
  const [show, setShow] = useState<SiteShow>("all");
  const [keyFor, setKeyFor] = useState<string | null>(null);
  const sites = useMySites(useDebounced(search), show);
  const set = useSetSite();
  if (sites.isPending) {
    return (
      <Tagged testId="sites-loading">
        <SkeletonList rows={6} />
      </Tagged>
    );
  }
  if (sites.isError) {
    return (
      <Tagged testId="sites-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("sites.loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void sites.refetch();
              }}
            >
              {t("sites.retry")}
            </Button>
          }
        />
      </Tagged>
    );
  }
  const list = sites.data;
  const country = list.country ?? null;
  const options = SITE_SHOWS.filter((value) => value !== "country" || country !== null).map(
    (value) => ({
      value,
      label:
        value === "country" && country !== null
          ? placeText(country, i18n.language, t)
          : t(`sites.show.${value}`),
    }),
  );
  const blocked = blockedSentence(list.blocked, i18n.language, t);
  return (
    <Stack gap="lg">
      <Stack gap="sm">
        <Tagged testId="sites-count">
          <Text tone="secondary">
            {t("sites.count", { available: list.available, on: list.on })}
          </Text>
        </Tagged>
        <TextField
          label={t("sites.find")}
          type="search"
          autoComplete="off"
          value={search}
          onChange={(event) => {
            setSearch(event.target.value);
          }}
          data-testid="sites-find"
        />
        <Select
          label={t("sites.show.label")}
          options={options}
          value={show}
          onChange={setShow}
          data-testid="sites-show"
        />
      </Stack>
      {list.groups.length === 0 ? (
        <Tagged testId="sites-none">
          <Text tone="secondary">
            {list.available === 0 ? t("sites.noneYet") : t("sites.noMatch")}
          </Text>
        </Tagged>
      ) : (
        list.groups.map((group) => {
          const title = groupTitle(group.group, country, i18n.language, t);
          return (
            <Section key={group.group} title={title} testId={`sites-group-${group.group}`}>
              <RowList label={title}>
                {group.sites.map((site) => (
                  <SiteRow
                    key={site.id}
                    site={site}
                    canManage={canManage}
                    saving={set.isPending && set.variables.id === site.id}
                    onToggle={(chosen) => {
                      set.mutate({ id: chosen.id, on: !chosen.on });
                    }}
                    onKey={(chosen) => {
                      setKeyFor(chosen.id);
                    }}
                  />
                ))}
              </RowList>
            </Section>
          );
        })
      )}
      {set.isError ? (
        <Tagged testId="sites-save-failed">
          <Text tone="danger" size="sm">
            {t("sites.saveFailed")}
          </Text>
        </Tagged>
      ) : null}
      {blocked === null ? null : (
        <Tagged testId="sites-blocked">
          <Text tone="secondary">{blocked}</Text>
        </Tagged>
      )}
      <CompaniesSection />
      {list.company_pages === 0 ? null : (
        <Tagged testId="sites-company-pages">
          <Text tone="secondary">
            {t("sites.companyPages", {
              count: list.company_pages,
              n: new Intl.NumberFormat(i18n.language).format(list.company_pages),
            })}
          </Text>
        </Tagged>
      )}
      <BoardPeek
        id={keyFor}
        onClose={() => {
          setKeyFor(null);
        }}
      />
    </Stack>
  );
}
