import { useTranslation } from "react-i18next";

import { Button, Cluster, CopyField, Section, Stack, Tag, Tagged, Text } from "~/lib/ui";

import { useDecideProposal, type LinkedInProposal } from "../../api/linkedin";

export interface TodoListProps {
  /** The kept and done proposals. */
  proposals: readonly LinkedInProposal[];
}

/**
 * "To change on LinkedIn": every kept proposal with its text to copy and
 * Done, and the done ones, each with a way back. Applicamp never writes to
 * LinkedIn: the person pastes the text themselves.
 */
export function TodoList({ proposals }: TodoListProps) {
  const { t } = useTranslation("profile");
  const decide = useDecideProposal();
  return (
    <Section
      title={t("linkedin.todo.title")}
      description={t("linkedin.todo.lead")}
      testId="linkedin-todo"
    >
      {proposals.length === 0 ? (
        <Text tone="secondary">{t("linkedin.todo.empty")}</Text>
      ) : (
        <Stack gap="md">
          {proposals.map((proposal) => {
            const field = t(`linkedin.proposal.fields.${proposal.field}`);
            return (
              <Tagged
                key={proposal.id}
                testId={`linkedin-todo-${proposal.id}`}
                data={{ "data-status": proposal.status }}
              >
                <Stack gap="sm">
                  {proposal.status === "done" ? (
                    <Cluster>
                      <Tag icon="check">{t("linkedin.proposal.done")}</Tag>
                      <Text tone="secondary" size="sm">
                        {field}: {proposal.text}
                      </Text>
                    </Cluster>
                  ) : (
                    <Tagged testId={`linkedin-copy-${proposal.id}`}>
                      <CopyField
                        label={field}
                        value={proposal.text}
                        copyLabel={t("linkedin.todo.copy")}
                        copiedLabel={t("linkedin.todo.copied")}
                        failedLabel={t("linkedin.todo.copyFailed")}
                      />
                    </Tagged>
                  )}
                  <Cluster>
                    {proposal.status === "done" ? (
                      <Button
                        variant="ghost"
                        disabled={decide.isPending}
                        data-testid={`linkedin-undone-${proposal.id}`}
                        onClick={() => {
                          decide.mutate({ id: proposal.id, decision: { status: "kept" } });
                        }}
                      >
                        {t("linkedin.todo.notDone")}
                      </Button>
                    ) : (
                      <Button
                        variant="secondary"
                        icon="check"
                        disabled={decide.isPending}
                        data-testid={`linkedin-done-${proposal.id}`}
                        onClick={() => {
                          decide.mutate({ id: proposal.id, decision: { status: "done" } });
                        }}
                      >
                        {t("linkedin.todo.done")}
                      </Button>
                    )}
                  </Cluster>
                </Stack>
              </Tagged>
            );
          })}
        </Stack>
      )}
    </Section>
  );
}
