import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Button, Cluster, Disclosure, Stack, Tag, Tagged, Text, TextArea } from "~/lib/ui";

import { useDecideProposal, type LinkedInProposal, type ProposalStatus } from "../../api/linkedin";
import { errorCode } from "./labels";

export interface ProposalRowProps {
  proposal: LinkedInProposal;
}

/** The most characters a field's text may have (LinkedIn's own limits). */
const MAX: Record<LinkedInProposal["field"], number> = {
  headline: 220,
  summary: 2600,
  position_description: 2000,
  skill: 100,
};

/**
 * One proposal: the text to paste on LinkedIn, why, where it came from, and
 * its flags (a claim or a buzzword no source says; shown, never dropped).
 * Keep and Drop together; Edit saves a rewrite that is checked again against
 * the same sources. Keep is off while a flag remains. Every decision can be
 * taken back.
 */
export function ProposalRow({ proposal }: ProposalRowProps) {
  const { t } = useTranslation("profile");
  const decide = useDecideProposal();
  const [editing, setEditing] = useState<string | null>(null);
  const flagged = proposal.flags.length > 0;
  const busy = decide.isPending;
  const { id } = proposal;

  const set = (status: ProposalStatus) => {
    decide.mutate({ id, decision: { status } });
  };
  const kindLabel = (kind: string) =>
    kind === "told"
      ? t("linkedin.proposal.sourceKinds.told")
      : kind === "linkedin"
        ? t("linkedin.proposal.sourceKinds.linkedin")
        : t("linkedin.proposal.sourceKinds.profile");

  const actions = () => {
    switch (proposal.status) {
      case "ready": {
        return (
          <Stack gap="sm">
            <Cluster>
              <Button
                variant="primary"
                disabled={busy || flagged}
                aria-label={t("linkedin.proposal.keepLabel", { text: proposal.text })}
                data-testid={`linkedin-keep-${id}`}
                onClick={() => {
                  set("kept");
                }}
              >
                {t("linkedin.proposal.keep")}
              </Button>
              <Button
                variant="secondary"
                disabled={busy}
                aria-label={t("linkedin.proposal.dropLabel", { text: proposal.text })}
                data-testid={`linkedin-drop-${id}`}
                onClick={() => {
                  set("dropped");
                }}
              >
                {t("linkedin.proposal.drop")}
              </Button>
              <Button
                variant="ghost"
                disabled={busy}
                data-testid={`linkedin-edit-${id}`}
                onClick={() => {
                  decide.reset();
                  setEditing(proposal.text);
                }}
              >
                {t("linkedin.proposal.edit")}
              </Button>
            </Cluster>
            {flagged ? (
              <Text tone="secondary" size="sm">
                {t("linkedin.proposal.keepOff")}
              </Text>
            ) : null}
          </Stack>
        );
      }
      case "kept": {
        return (
          <Cluster>
            <Tag icon="check">{t("linkedin.proposal.kept")}</Tag>
            <Button
              variant="ghost"
              disabled={busy}
              data-testid={`linkedin-take-back-${id}`}
              onClick={() => {
                set("ready");
              }}
            >
              {t("linkedin.proposal.takeBack")}
            </Button>
          </Cluster>
        );
      }
      case "dropped": {
        return (
          <Cluster>
            <Text tone="secondary" size="sm">
              {t("linkedin.proposal.dropped")}
            </Text>
            <Button
              variant="ghost"
              disabled={busy}
              data-testid={`linkedin-take-back-${id}`}
              onClick={() => {
                set("ready");
              }}
            >
              {t("linkedin.proposal.takeBack")}
            </Button>
          </Cluster>
        );
      }
      case "done": {
        return (
          <Cluster>
            <Tag icon="check">{t("linkedin.proposal.done")}</Tag>
            <Button
              variant="ghost"
              disabled={busy}
              data-testid={`linkedin-take-back-${id}`}
              onClick={() => {
                set("kept");
              }}
            >
              {t("linkedin.todo.notDone")}
            </Button>
          </Cluster>
        );
      }
    }
  };

  return (
    <Tagged testId={`linkedin-proposal-${id}`} data={{ status: proposal.status }}>
      <Stack gap="sm">
        <Text weight="semibold" size="sm">
          {t(`linkedin.proposal.fields.${proposal.field}`)}
        </Text>
        <Tagged testId={`linkedin-proposal-text-${id}`}>
          <Text as="p">{proposal.text}</Text>
        </Tagged>
        {proposal.why === "" ? null : (
          <Text tone="secondary" size="sm">
            {proposal.why}
          </Text>
        )}
        {flagged ? (
          <Tagged testId={`linkedin-flags-${id}`}>
            <Stack gap="sm">
              {proposal.flags.map((flag) => (
                <Text key={`${flag.kind}-${flag.claim}`} tone="danger" size="sm">
                  {t(`linkedin.proposal.flag.${flag.kind}`, { claim: flag.claim })}
                </Text>
              ))}
            </Stack>
          </Tagged>
        ) : null}
        <Disclosure summary={t("linkedin.proposal.sources")}>
          {proposal.sources.length === 0 ? (
            <Text tone="secondary" size="sm">
              {t("linkedin.proposal.noSources")}
            </Text>
          ) : (
            <Stack gap="sm">
              {proposal.sources.map((source) => (
                <Text key={source.id} size="sm" as="p">
                  {kindLabel(source.kind)}: {source.text}
                </Text>
              ))}
            </Stack>
          )}
        </Disclosure>
        {editing === null ? (
          actions()
        ) : (
          <Stack gap="sm">
            <TextArea
              label={t("linkedin.proposal.editLabel")}
              hint={t("linkedin.proposal.editHint")}
              rows={proposal.field === "summary" ? 8 : 3}
              maxLength={MAX[proposal.field]}
              value={editing}
              data-testid={`linkedin-edit-text-${id}`}
              onChange={(event) => {
                setEditing(event.target.value);
              }}
            />
            <Cluster>
              <Button
                variant="primary"
                disabled={busy || editing.trim() === ""}
                data-testid={`linkedin-save-${id}`}
                onClick={() => {
                  decide.mutate(
                    { id, decision: { text: editing } },
                    {
                      onSuccess: () => {
                        setEditing(null);
                      },
                    },
                  );
                }}
              >
                {t("linkedin.proposal.save")}
              </Button>
              <Button
                variant="ghost"
                disabled={busy}
                data-testid={`linkedin-edit-cancel-${id}`}
                onClick={() => {
                  decide.reset();
                  setEditing(null);
                }}
              >
                {t("linkedin.proposal.cancel")}
              </Button>
            </Cluster>
          </Stack>
        )}
        {decide.isError ? (
          <Tagged testId={`linkedin-proposal-error-${id}`}>
            <Text tone="danger" size="sm">
              {t(`linkedin.errors.${errorCode(decide.error)}`)}
            </Text>
          </Tagged>
        ) : null}
      </Stack>
    </Tagged>
  );
}
