import { useTranslation } from "react-i18next";

import { problemCode } from "~/lib/api";
import {
  Banner,
  Button,
  Cluster,
  DecisionCard,
  DecisionList,
  Label,
  Page,
  Placeholder,
  Progress,
  Section,
  Stack,
  Tag,
  Tagged,
  Text,
  TouchTargets,
  type LabelHue,
} from "~/lib/ui";

import {
  foundInRead,
  progressOf,
  useDecideFact,
  useFactProposals,
  useKeepAllFacts,
  useUndoKeepAll,
  type FactGroup,
  type FactProposal,
  type FactSource,
} from "../../api/facts";
import { useFoundList } from "../ImportReview/found";

export interface CompletePageProps {
  /** Back to Profile & goals. */
  onBack: () => void;
}

/** Each source's colour: the words say it, the colour only repeats it. */
const HUES: Record<FactSource, LabelHue> = {
  you_said: "green",
  you_confirmed: "yellow",
  your_website: "purple",
  your_cv: "blue",
};

/** The year of a `YYYY`, `YYYY-MM` or `YYYY-MM-DD` date. */
const yearOf = (date: string | null | undefined) =>
  date === null || date === undefined ? undefined : date.slice(0, 4);

/** A group's heading: "Example Co · 2021 – now", "Your skills". */
function useGroupTitle() {
  const { t } = useTranslation("profile");
  return (group: FactGroup): string => {
    switch (group.target) {
      case "skills": {
        return t("complete.groups.skills");
      }
      case "background": {
        return t("complete.groups.background");
      }
      case "position": {
        const name = group.company ?? group.title ?? "";
        const start = yearOf(group.start_date);
        if (start === undefined) {
          return name;
        }
        const end = yearOf(group.end_date) ?? t("complete.now");
        const years = start === end ? start : t("complete.years", { start, end });
        return name === "" ? years : `${name} · ${years}`;
      }
    }
  };
}

/** Where a fact comes from, and the words it comes from. */
function FactMeta({ fact }: { fact: FactProposal }) {
  const { t } = useTranslation("profile");
  const words = (() => {
    switch (fact.source) {
      case "you_said":
      case "your_cv": {
        return t("complete.quoted", { quote: fact.quote });
      }
      case "your_website": {
        return fact.basis ?? t("complete.website");
      }
      case "you_confirmed": {
        // "You said this was right" only under an assumption marked right.
        return fact.said_right ? t("complete.confirmed") : (fact.basis ?? null);
      }
    }
  })();
  return (
    <>
      <Label hue={HUES[fact.source]}>{t(`complete.sources.${fact.source}`)}</Label>
      {words === null ? null : <span>{words}</span>}
    </>
  );
}

/** One proposed fact: Keep and Drop; once decided, its state and Undo. */
function FactCard({
  fact,
  busy,
  onDecide,
}: {
  fact: FactProposal;
  busy: boolean;
  onDecide: (decision: "keep" | "drop" | "undo") => void;
}) {
  const { t } = useTranslation("profile");
  const { id, text } = fact;
  const actions =
    fact.state === "open" ? (
      <Cluster>
        <Button
          variant="primary"
          disabled={busy}
          aria-label={t("complete.keepLabel", { text })}
          data-testid={`complete-keep-${id}`}
          onClick={() => {
            onDecide("keep");
          }}
        >
          {t("complete.keep")}
        </Button>
        <Button
          variant="secondary"
          disabled={busy}
          aria-label={t("complete.dropLabel", { text })}
          data-testid={`complete-drop-${id}`}
          onClick={() => {
            onDecide("drop");
          }}
        >
          {t("complete.drop")}
        </Button>
      </Cluster>
    ) : (
      <Cluster>
        {fact.state === "kept" ? (
          <Tag icon="check">{t("complete.kept")}</Tag>
        ) : (
          <Text tone="secondary" size="sm">
            {t("complete.dropped")}
          </Text>
        )}
        <Button
          variant="ghost"
          disabled={busy}
          aria-label={t("complete.undoLabel", { text })}
          data-testid={`complete-undo-${id}`}
          onClick={() => {
            onDecide("undo");
          }}
        >
          {t("complete.undo")}
        </Button>
      </Cluster>
    );
  return (
    <DecisionCard
      text={text}
      meta={<FactMeta fact={fact} />}
      actions={actions}
      state={fact.state}
      testId={`complete-fact-${id}`}
    />
  );
}

/**
 * `/profile/complete`, "Complete your profile": the facts the app already
 * knows that the profile does not hold yet (what the person said and
 * confirmed in Tell me about you, their own website, a CV read again), by
 * job, each with where it comes from and Keep or Drop. Kept and dropped facts
 * stay in place with Undo for a day; Keep all keeps the rest at once.
 */
export function CompletePage({ onBack }: CompletePageProps) {
  const { t } = useTranslation("profile");
  const proposals = useFactProposals();
  const decide = useDecideFact();
  const keepAll = useKeepAllFacts();
  const undoAll = useUndoKeepAll();
  const groupTitle = useGroupTitle();
  const foundList = useFoundList();
  const view = proposals.data;
  const busy = decide.isPending || keepAll.isPending || undoAll.isPending;
  const { decided, total } = view === undefined ? { decided: 0, total: 0 } : progressOf(view);

  const body = () => {
    if (proposals.isPending) {
      return (
        <Tagged testId="complete-loading">
          <Text tone="secondary">{t("complete.loading")}</Text>
        </Tagged>
      );
    }
    if (proposals.isError || view === undefined) {
      return (
        <Tagged testId="complete-load-error">
          <Banner
            tone="danger"
            icon="warning"
            title={t("complete.loadFailed")}
            action={
              <Button
                variant="secondary"
                data-testid="complete-retry"
                onClick={() => {
                  void proposals.refetch();
                }}
              >
                {t("retry")}
              </Button>
            }
          />
        </Tagged>
      );
    }
    const read = view.cv_read ?? null;
    const batch = view.batch ?? null;
    const failed = decide.error ?? keepAll.error ?? undoAll.error;
    return (
      <Stack gap="lg">
        {read === null ? null : (
          <Tagged testId="complete-cv-read">
            <Banner icon="check" title={t("complete.cvRead")}>
              {t("review.found", { list: foundList(foundInRead(read)) })}
            </Banner>
          </Tagged>
        )}
        {total === 0 ? (
          <Placeholder
            testId="complete-empty"
            icon="people"
            title={t("complete.emptyTitle")}
            action={
              <Button variant="secondary" data-testid="complete-empty-back" onClick={onBack}>
                {t("complete.back")}
              </Button>
            }
          >
            {t("complete.emptyLead")}
          </Placeholder>
        ) : (
          <>
            <Tagged testId="complete-progress">
              <Stack gap="sm">
                <Progress
                  label={t("complete.progress", { decided, total })}
                  value={decided}
                  max={total}
                >
                  {t("complete.progress", { decided, total })}
                </Progress>
                <Cluster>
                  <Button
                    variant="secondary"
                    icon="check"
                    disabled={busy || view.open === 0}
                    data-testid="complete-keep-all"
                    onClick={() => {
                      keepAll.mutate();
                    }}
                  >
                    {t("complete.keepAll")}
                  </Button>
                </Cluster>
              </Stack>
            </Tagged>
            {failed === null ? null : (
              <Tagged testId="complete-error">
                <Banner
                  tone="danger"
                  icon="warning"
                  role="alert"
                  title={
                    problemCode(failed) === "profile_changed"
                      ? t("complete.profileChanged")
                      : t("complete.failed")
                  }
                />
              </Tagged>
            )}
            {view.groups.map((group, index) => {
              const title = groupTitle(group);
              return (
                <Section
                  key={`${group.target}-${String(group.position)}`}
                  title={title}
                  testId={`complete-group-${String(index)}`}
                >
                  <DecisionList label={title}>
                    {group.proposals.map((fact) => (
                      <FactCard
                        key={fact.id}
                        fact={fact}
                        busy={busy}
                        onDecide={(decision) => {
                          keepAll.reset();
                          undoAll.reset();
                          decide.mutate({ id: fact.id, decision });
                        }}
                      />
                    ))}
                  </DecisionList>
                </Section>
              );
            })}
            {view.open === 0 ? (
              <Tagged testId="complete-done">
                <Banner
                  icon="check"
                  role="status"
                  title={
                    view.kept === 0
                      ? t("complete.doneNone")
                      : t("complete.done", { count: view.kept })
                  }
                  action={
                    batch === null ? undefined : (
                      <Button
                        variant="secondary"
                        disabled={busy}
                        data-testid="complete-undo-all"
                        onClick={() => {
                          decide.reset();
                          keepAll.reset();
                          undoAll.mutate(batch.id);
                        }}
                      >
                        {t("complete.undo")}
                      </Button>
                    )
                  }
                >
                  {view.kept === 0 ? null : t("complete.doneUndo")}
                </Banner>
              </Tagged>
            ) : null}
          </>
        )}
        <Cluster>
          <Button variant="ghost" data-testid="complete-back" onClick={onBack}>
            {t("complete.back")}
          </Button>
        </Cluster>
      </Stack>
    );
  };

  return (
    <Page
      testId="profile-complete"
      eyebrow={t("complete.eyebrow")}
      title={t("complete.title")}
      lead={
        view === undefined || total === 0
          ? t("complete.leadNone")
          : t("complete.lead", { count: total })
      }
    >
      <TouchTargets>{body()}</TouchTargets>
    </Page>
  );
}
