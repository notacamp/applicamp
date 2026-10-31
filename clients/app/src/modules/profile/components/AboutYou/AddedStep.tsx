import { useTranslation } from "react-i18next";

import { Button, Cluster, FactBlock, FactRow, Stack, Text } from "~/lib/ui";

import type { AboutView } from "../../api/about";
import { ChangeActions } from "./ChangeActions";
import { AboutLegend, PortraitText } from "./PortraitText";

export interface AddedStepProps {
  view: AboutView;
  onBack: () => void;
  onQuestions: () => void;
}

/**
 * Screen 2: what the model took from what the person wrote ("Added from what
 * you wrote", each change with their own words and its flags), and the
 * portrait with the new sentences highlighted. Nothing is saved yet.
 */
export function AddedStep({ view, onBack, onQuestions }: AddedStepProps) {
  const { t } = useTranslation("profile");
  const open = view.changes.filter(
    (change) => change.flags.length > 0 && (change.resolution ?? null) === null,
  ).length;
  return (
    <Stack gap="lg" testId="about-added-step">
      {view.changes.length === 0 ? (
        <Text tone="secondary" as="p">
          {t("about.added.none")}
        </Text>
      ) : (
        <FactBlock
          title={t("about.added.title")}
          testId="about-added"
          footer={<Text size="sm">{t("about.added.savedLater")}</Text>}
        >
          {view.changes.map((change) => (
            <FactRow
              key={change.id}
              text={`${t(`about.fields.${change.field}`)}: ${change.value}`}
              quoteLabel={t("about.yourWords")}
              quote={change.quote}
              testId={`about-added-${change.id}`}
            >
              {change.flags.length > 0 || (change.resolution ?? null) !== null ? (
                <ChangeActions
                  id={change.id}
                  value={change.value}
                  flags={change.flags}
                  resolution={change.resolution}
                />
              ) : null}
            </FactRow>
          ))}
        </FactBlock>
      )}
      <AboutLegend />
      <PortraitText sentences={view.sentences} testId="about-portrait-updated" />
      <Text tone="secondary" as="p" size="sm">
        {t("about.added.questionsNext")}
      </Text>
      {open > 0 ? (
        <Text tone="danger" size="sm" as="p">
          {t("about.added.open", { count: open })}
        </Text>
      ) : null}
      <Cluster>
        <Button
          variant="primary"
          icon="arrow-right"
          data-testid="about-to-questions"
          onClick={onQuestions}
        >
          {t("about.added.questions")}
        </Button>
        <Button variant="ghost" data-testid="about-back-to-portrait" onClick={onBack}>
          {t("about.added.back")}
        </Button>
      </Cluster>
    </Stack>
  );
}
