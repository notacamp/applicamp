import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Cluster,
  FactBlock,
  FactRow,
  Stack,
  Tag,
  Tagged,
  Text,
  TextArea,
} from "~/lib/ui";

import {
  useDropFact,
  useKeepAllFacts,
  useKeepFact,
  type FactProposal,
  type GrillView,
} from "../../api/grill";
import { errorCode } from "./labels";

export interface FactsTakenProps {
  view: GrillView;
  /** The facts taken from one answer. */
  facts: readonly FactProposal[];
  /** That answer's turn (or another key), for the test ids. */
  turn: number | string;
  /** The role's name, for "Saved to …". */
  role: string;
}

/**
 * The facts one answer yielded, each with the person's own words and Keep,
 * Fix or Drop; Keep all while more than one waits. Nothing is saved until a
 * click, and a fixed fact must still say only what the words and the profile
 * say.
 */
export function FactsTaken({ view, facts, turn, role }: FactsTakenProps) {
  const { t } = useTranslation("profile");
  const keep = useKeepFact();
  const drop = useDropFact();
  const keepAll = useKeepAllFacts();
  const [fixing, setFixing] = useState<{ id: string; text: string } | null>(null);
  const waiting = facts.filter((fact) => fact.status === "ready");
  const busy = keep.isPending || drop.isPending || keepAll.isPending;
  const failure = [keep, drop, keepAll].find((action) => action.isError)?.error;

  const reset = () => {
    keep.reset();
    drop.reset();
    keepAll.reset();
  };

  return (
    <FactBlock
      title={t("grill.facts.title")}
      testId={`grill-facts-${String(turn)}`}
      footer={
        waiting.length === 0 ? undefined : (
          <>
            {waiting.length > 1 ? (
              <Button
                variant="secondary"
                disabled={busy}
                data-testid={`grill-keep-all-${String(turn)}`}
                onClick={() => {
                  reset();
                  keepAll.mutate(view.position);
                }}
              >
                {t("grill.facts.keepAll")}
              </Button>
            ) : null}
            <Text tone="secondary" size="sm">
              {t("grill.facts.savedTo", { role })}
            </Text>
          </>
        )
      }
    >
      {facts.map((fact) => {
        const editing = fixing !== null && fixing.id === fact.id ? fixing : null;
        return (
          <FactRow
            key={fact.id}
            text={fact.text}
            quoteLabel={t("grill.facts.yourWords")}
            quote={fact.quote}
            muted={fact.status === "dropped"}
            testId={`grill-fact-${fact.id}`}
            actions={
              fact.status === "ready" ? (
                editing === null ? (
                  <Cluster>
                    <Button
                      variant="primary"
                      disabled={busy}
                      aria-label={t("grill.facts.keepLabel", { text: fact.text })}
                      data-testid={`grill-keep-${fact.id}`}
                      onClick={() => {
                        reset();
                        keep.mutate({ id: fact.id });
                      }}
                    >
                      {t("grill.facts.keep")}
                    </Button>
                    <Button
                      variant="secondary"
                      disabled={busy}
                      aria-label={t("grill.facts.fixLabel", { text: fact.text })}
                      data-testid={`grill-fix-${fact.id}`}
                      onClick={() => {
                        reset();
                        setFixing({ id: fact.id, text: fact.text });
                      }}
                    >
                      {t("grill.facts.fix")}
                    </Button>
                    <Button
                      variant="ghost"
                      disabled={busy}
                      aria-label={t("grill.facts.dropLabel", { text: fact.text })}
                      data-testid={`grill-drop-${fact.id}`}
                      onClick={() => {
                        reset();
                        drop.mutate(fact.id);
                      }}
                    >
                      {t("grill.facts.drop")}
                    </Button>
                  </Cluster>
                ) : undefined
              ) : fact.status === "kept" ? (
                <Tagged testId={`grill-kept-${fact.id}`}>
                  <Tag icon="check">{t("grill.facts.kept")}</Tag>
                </Tagged>
              ) : (
                <Tagged testId={`grill-dropped-${fact.id}`}>
                  <Text tone="secondary" size="sm">
                    {t("grill.facts.dropped")}
                  </Text>
                </Tagged>
              )
            }
          >
            {editing === null ? null : (
              <Stack gap="sm">
                <TextArea
                  label={t("grill.facts.fixField")}
                  hint={t("grill.facts.fixHint")}
                  rows={2}
                  maxLength={300}
                  value={editing.text}
                  error={keep.isError ? t(`grill.errors.${errorCode(keep.error)}`) : undefined}
                  data-testid={`grill-fix-text-${fact.id}`}
                  onChange={(event) => {
                    setFixing({ id: fact.id, text: event.target.value });
                  }}
                />
                <Cluster>
                  <Button
                    variant="primary"
                    disabled={busy || editing.text.trim() === ""}
                    data-testid={`grill-fix-save-${fact.id}`}
                    onClick={() => {
                      keep.mutate(
                        { id: fact.id, text: editing.text.trim() },
                        {
                          onSuccess: () => {
                            setFixing(null);
                          },
                        },
                      );
                    }}
                  >
                    {t("grill.facts.fixSave")}
                  </Button>
                  <Button
                    variant="ghost"
                    disabled={busy}
                    data-testid={`grill-fix-cancel-${fact.id}`}
                    onClick={() => {
                      reset();
                      setFixing(null);
                    }}
                  >
                    {t("grill.facts.fixCancel")}
                  </Button>
                </Cluster>
              </Stack>
            )}
          </FactRow>
        );
      })}
      {failure !== undefined && fixing === null ? (
        <Tagged testId={`grill-facts-error-${String(turn)}`}>
          <Banner
            tone="danger"
            icon="warning"
            role="alert"
            title={t(`grill.errors.${errorCode(failure)}`)}
          />
        </Tagged>
      ) : null}
    </FactBlock>
  );
}
