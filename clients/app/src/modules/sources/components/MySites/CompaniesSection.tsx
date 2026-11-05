import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Cluster, Label, RowItem, RowList, Section, Tagged, Text } from "~/lib/ui";

import {
  companyErrorOf,
  reasonText,
  useCompanies,
  useCompanyAction,
  type CompanyList,
  type WatchedCompany,
} from "../../api/companies";

/** Companies proposed that show before "Show". */
const PROPOSED_SHOWN = 5;

/** What the person did to a company that left the server's lists. */
type Mark = "dropped" | "stopped";

/** A company the section has shown, kept in its place whatever happens to it. */
interface Known {
  id: string;
  name: string;
  /** Why it was proposed, from the first time it was seen. */
  why: string | null;
}

function watchedLine(company: WatchedCompany, t: TFunction<"sources">): string {
  if (company.finding) {
    return t("companies.watching.finding");
  }
  const open = company.open_jobs ?? null;
  return open === null
    ? t("sites.companies.checkedDaily")
    : `${t("sites.companies.checkedDaily")} · ${t("companies.watching.open", { count: open })}`;
}

/** `known` with the companies of `list` it did not have, after those it had. */
function remember(
  known: ReadonlyMap<string, Known>,
  list: CompanyList,
  t: TFunction<"sources">,
): ReadonlyMap<string, Known> {
  const next = new Map(known);
  for (const company of list.watching) {
    if (!next.has(company.id)) {
      next.set(company.id, { id: company.id, name: company.name, why: null });
    }
  }
  for (const company of list.suggested) {
    const first = company.reasons[0];
    next.set(company.id, {
      id: company.id,
      name: company.name,
      why: first === undefined ? null : reasonText(first, t),
    });
  }
  return next;
}

/**
 * The Companies below the sites: the ones whose jobs keep fitting the person. Keep follows a
 * company (the row then reads "Following" with Stop), Drop sets it aside; every action leaves the
 * row where it is with Undo, and no row vanishes.
 */
export function CompaniesSection() {
  const { t } = useTranslation("sources");
  const companies = useCompanies();
  const action = useCompanyAction();
  const [seen, setSeen] = useState<{
    from: CompanyList | null;
    known: ReadonlyMap<string, Known>;
  }>({ from: null, known: new Map() });
  const [marks, setMarks] = useState<ReadonlyMap<string, Mark>>(new Map());
  const [all, setAll] = useState(false);
  const list = companies.data;
  if (list !== undefined && seen.from !== list) {
    setSeen({ from: list, known: remember(seen.known, list, t) });
  }
  const mark = (id: string, value: Mark | null) => {
    setMarks((current) => {
      const next = new Map(current);
      if (value === null) {
        next.delete(id);
      } else {
        next.set(id, value);
      }
      return next;
    });
  };
  const run = (
    id: string,
    kind: "watch" | "dismiss" | "undismiss" | "stop",
    after: Mark | null,
  ) => {
    action.mutate(
      { kind, id },
      {
        onSuccess: () => {
          mark(id, after);
        },
      },
    );
  };
  const title = t("sites.companies.title");
  if (companies.isPending) {
    return (
      <Section title={title} testId="sites-companies">
        <Tagged testId="sites-companies-loading">
          <Text tone="secondary">{t("companies.loading")}</Text>
        </Tagged>
      </Section>
    );
  }
  if (list === undefined) {
    return (
      <Section title={title} testId="sites-companies">
        <Tagged testId="sites-companies-error">
          <Banner
            tone="danger"
            icon="warning"
            title={t("companies.loadFailed")}
            action={
              <Button
                variant="secondary"
                onClick={() => {
                  void companies.refetch();
                }}
              >
                {t("companies.retry")}
              </Button>
            }
          />
        </Tagged>
      </Section>
    );
  }
  const following = new Map(list.watching.map((company) => [company.id, company]));
  const proposed = new Set(list.suggested.map((company) => company.id));
  let proposedShown = 0;
  let hidden = 0;
  const rows = [...seen.known.values()].flatMap((company) => {
    const watched = following.get(company.id);
    const isProposed = proposed.has(company.id);
    const mine = marks.get(company.id);
    if (watched === undefined && !isProposed && mine === undefined) {
      return [];
    }
    if (watched === undefined && isProposed) {
      if (proposedShown >= PROPOSED_SHOWN && !all) {
        hidden += 1;
        return [];
      }
      proposedShown += 1;
    }
    const busy = action.isPending;
    const testId = `site-company-${company.id}`;
    if (watched !== undefined) {
      return [
        <RowItem
          key={company.id}
          title={company.name}
          subtitle={watchedLine(watched, t)}
          actions={
            <Cluster>
              <Label hue="green">{t("sites.companies.following")}</Label>
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => {
                  run(company.id, "stop", "stopped");
                }}
                data-testid={`site-company-stop-${company.id}`}
              >
                {t("sites.companies.stop")}
              </Button>
            </Cluster>
          }
          testId={testId}
        />,
      ];
    }
    if (isProposed) {
      return [
        <RowItem
          key={company.id}
          title={company.name}
          subtitle={company.why ?? undefined}
          actions={
            <Cluster>
              <Button
                variant="primary"
                disabled={busy}
                onClick={() => {
                  run(company.id, "watch", null);
                }}
                data-testid={`site-company-keep-${company.id}`}
              >
                {t("sites.companies.keep")}
              </Button>
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => {
                  run(company.id, "dismiss", "dropped");
                }}
                data-testid={`site-company-drop-${company.id}`}
              >
                {t("sites.companies.drop")}
              </Button>
            </Cluster>
          }
          testId={testId}
        />,
      ];
    }
    const dropped = mine === "dropped";
    return [
      <RowItem
        key={company.id}
        title={company.name}
        subtitle={company.why ?? undefined}
        actions={
          <Cluster>
            <Text tone="secondary">
              {dropped ? t("sites.companies.dropped") : t("sites.companies.stopped")}
            </Text>
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => {
                run(company.id, dropped ? "undismiss" : "watch", null);
              }}
              data-testid={`site-company-undo-${company.id}`}
            >
              {t("sites.companies.undo")}
            </Button>
          </Cluster>
        }
        testId={testId}
      />,
    ];
  });
  const failed = action.isError ? companyErrorOf(action.error) : null;
  return (
    <Section
      title={title}
      description={t("sites.companies.description", {
        followed: String(list.watching.length),
        proposed: String(list.suggested.length),
      })}
      testId="sites-companies"
    >
      {rows.length === 0 ? (
        <Tagged testId="sites-companies-empty">
          <Text tone="secondary">{t("sites.companies.empty")}</Text>
        </Tagged>
      ) : (
        <RowList label={title} testId="sites-companies-list">
          {rows}
          {hidden === 0 ? null : (
            <RowItem
              title={t("sites.companies.more", { count: hidden })}
              actions={
                <Button
                  variant="secondary"
                  onClick={() => {
                    setAll(true);
                  }}
                  data-testid="sites-companies-show"
                >
                  {t("sites.companies.show")}
                </Button>
              }
              testId="sites-companies-more"
            />
          )}
        </RowList>
      )}
      {failed === null ? null : (
        <Tagged testId="sites-companies-failed">
          <Text tone="danger" size="sm">
            {t(`companies.errors.${failed}`, { max: String(list.limit) })}
          </Text>
        </Tagged>
      )}
    </Section>
  );
}
