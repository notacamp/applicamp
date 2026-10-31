import { useState, type SubmitEvent } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  ButtonLink,
  Cluster,
  FactBlock,
  FactRow,
  Stack,
  Tagged,
  Text,
  TextField,
} from "~/lib/ui";

import {
  prepareErrorCode,
  shownUrl,
  useReadCompany,
  useRemoveFact,
  useRestoreFact,
  useSetSite,
  type Brief,
} from "../../api/brief";
import { formatCost, formatShortDate } from "../../api/labels";

export interface CompanyFactsProps {
  id: string;
  brief: Brief;
}

/** The longest website the server takes. */
const MAX_SITE_CHARS = 2000;

/**
 * What the company's own pages say: the site (typed once when nothing gives
 * it), Read, and the facts each linked to the page it was read on, with
 * "Wrong, remove" and Undo. Reading is one model call, reused for 30 days.
 */
export function CompanyFacts({ id, brief }: CompanyFactsProps) {
  const { t, i18n } = useTranslation("pipeline");
  const language = i18n.language;
  const read = useReadCompany(id, language);
  const setSite = useSetSite(id);
  const remove = useRemoveFact(id);
  const restore = useRestoreFact(id);
  const [editing, setEditing] = useState(false);
  const [typed, setTyped] = useState("");
  const company = brief.company;
  const showForm = brief.site_needed || editing;
  const cost = formatCost(
    brief.estimates.company_read_eur + brief.estimates.analysis_eur,
    language,
  );
  const againCost = formatCost(brief.estimates.company_read_eur, language);

  const submit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const url = typed.trim();
    if (url === "" || url.length > MAX_SITE_CHARS) {
      return;
    }
    setSite.mutate(url, {
      onSuccess: () => {
        setEditing(false);
        setTyped("");
      },
    });
  };

  const failure = [read, remove, restore].find((action) => action.isError)?.error;
  const siteHost = brief.site === undefined || brief.site === null ? "" : shownUrl(brief.site);
  const facts = brief.facts;
  const kept = facts.filter((fact) => !fact.removed).length;

  return (
    <Stack>
      {showForm ? (
        <form onSubmit={submit} noValidate>
          <Stack gap="sm">
            <TextField
              label={t("prepare.site.label", { company })}
              hint={t("prepare.site.hint")}
              type="url"
              inputMode="url"
              autoComplete="off"
              placeholder="https://"
              value={typed}
              error={
                setSite.isError ? t(`prepare.errors.${prepareErrorCode(setSite.error)}`) : undefined
              }
              onChange={(event) => {
                setTyped(event.target.value);
              }}
              data-testid="prepare-site-input"
            />
            <Cluster>
              <Button
                variant="primary"
                type="submit"
                disabled={setSite.isPending || typed.trim() === ""}
                data-testid="prepare-site-save"
              >
                {t("prepare.site.save")}
              </Button>
              {brief.site_needed ? null : (
                <Button
                  variant="ghost"
                  type="button"
                  data-testid="prepare-site-cancel"
                  onClick={() => {
                    setEditing(false);
                    setTyped("");
                    setSite.reset();
                  }}
                >
                  {t("prepare.site.cancel")}
                </Button>
              )}
            </Cluster>
          </Stack>
        </form>
      ) : (
        <Cluster>
          <Tagged testId="prepare-site">
            <Text>{t("prepare.site.is", { company, site: siteHost })}</Text>
          </Tagged>
          <Button
            variant="ghost"
            data-testid="prepare-site-change"
            onClick={() => {
              setTyped(brief.site ?? "");
              setEditing(true);
            }}
          >
            {t("prepare.site.change")}
          </Button>
        </Cluster>
      )}

      {brief.read === undefined || brief.read === null ? (
        <Stack gap="sm">
          <Cluster>
            <Button
              variant="primary"
              icon="arrow-right"
              disabled={brief.site_needed || read.isPending}
              data-testid="prepare-read"
              onClick={() => {
                read.reset();
                read.mutate(false);
              }}
            >
              {read.isPending ? t("prepare.reading") : t("prepare.read", { company, cost })}
            </Button>
          </Cluster>
          {read.isPending ? (
            <Tagged testId="prepare-reading">
              <Banner role="status" title={t("prepare.readingLead", { company })} />
            </Tagged>
          ) : (
            <Text tone="secondary" size="sm">
              {t("prepare.readLead")}
            </Text>
          )}
        </Stack>
      ) : (
        <Stack gap="sm">
          {facts.length === 0 ? (
            <Tagged testId="prepare-no-facts">
              <Text tone="secondary">{t("prepare.noFacts", { company })}</Text>
            </Tagged>
          ) : (
            <FactBlock
              title={t("prepare.facts.title", { company })}
              testId="prepare-facts"
              footer={
                <Text tone="secondary" size="sm">
                  {t("prepare.facts.count", { count: kept })}
                </Text>
              }
            >
              {facts.map((fact) => (
                <FactRow
                  key={fact.id}
                  text={fact.text}
                  quoteLabel={t("prepare.facts.pageSays")}
                  quote={fact.quote}
                  muted={fact.removed}
                  testId={`prepare-fact-${fact.id}`}
                  actions={
                    fact.removed ? (
                      <Cluster>
                        <Text tone="secondary" size="sm">
                          {t("prepare.facts.removed")}
                        </Text>
                        <Button
                          variant="secondary"
                          disabled={restore.isPending}
                          data-testid={`prepare-fact-undo-${fact.id}`}
                          onClick={() => {
                            restore.reset();
                            restore.mutate(fact.id);
                          }}
                        >
                          {t("prepare.facts.undo")}
                        </Button>
                      </Cluster>
                    ) : (
                      <Button
                        variant="ghost"
                        icon="close"
                        disabled={remove.isPending}
                        aria-label={t("prepare.facts.removeLabel", { text: fact.text })}
                        data-testid={`prepare-fact-remove-${fact.id}`}
                        onClick={() => {
                          remove.reset();
                          remove.mutate(fact.id);
                        }}
                      >
                        {t("prepare.facts.remove")}
                      </Button>
                    )
                  }
                >
                  <Cluster>
                    <ButtonLink
                      variant="ghost"
                      icon="forward"
                      href={fact.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      data-testid={`prepare-fact-source-${fact.id}`}
                    >
                      {shownUrl(fact.url)}
                    </ButtonLink>
                    {fact.fetched_at === undefined || fact.fetched_at === null ? null : (
                      <Text tone="secondary" size="sm">
                        {t("prepare.facts.fetched", {
                          date: formatShortDate(fact.fetched_at, language),
                        })}
                      </Text>
                    )}
                  </Cluster>
                </FactRow>
              ))}
            </FactBlock>
          )}

          <Tagged testId="prepare-pages">
            <Text tone="secondary" size="sm">
              {t("prepare.pages.line", {
                count: brief.read.pages.length,
                date: formatShortDate(brief.read.read_at, language),
                until: formatShortDate(brief.read.fresh_until, language),
                pages: brief.read.pages.map((page) => shownUrl(page.url)).join(", "),
              })}
            </Text>
          </Tagged>
          {brief.read.pages_blocked > 0 ? (
            <Tagged testId="prepare-blocked">
              <Text tone="secondary" size="sm">
                {t("prepare.pages.blocked", { count: brief.read.pages_blocked })}
              </Text>
            </Tagged>
          ) : null}
          {brief.read.facts_dropped > 0 ? (
            <Tagged testId="prepare-dropped">
              <Text tone="secondary" size="sm">
                {t("prepare.pages.dropped", { count: brief.read.facts_dropped })}
              </Text>
            </Tagged>
          ) : null}
          <Cluster>
            <Button
              variant="secondary"
              disabled={read.isPending}
              data-testid="prepare-read-again"
              onClick={() => {
                read.reset();
                read.mutate(true);
              }}
            >
              {read.isPending ? t("prepare.reading") : t("prepare.readAgain", { cost: againCost })}
            </Button>
          </Cluster>
        </Stack>
      )}

      {failure === undefined ? null : (
        <Tagged testId="prepare-error">
          <Banner
            tone="danger"
            icon="warning"
            role="alert"
            title={t(`prepare.errors.${prepareErrorCode(failure)}`)}
          />
        </Tagged>
      )}
    </Stack>
  );
}
