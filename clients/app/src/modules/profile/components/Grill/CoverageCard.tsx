import { useTranslation } from "react-i18next";

import { List, ListRow, Progress, Section, StatusMark, Tagged, Text } from "~/lib/ui";

import { AREAS, type GrillView } from "../../api/grill";

/**
 * How far a role is covered: seven rows (done, half, not yet) and n of 7.
 * Computed by the server from what the answers covered; it only goes up.
 */
export function CoverageCard({ view, title }: { view: GrillView; title: string }) {
  const { t } = useTranslation("profile");
  const levels = new Map(view.coverage.map((entry) => [entry.area, entry.level]));
  return (
    <Section
      title={title}
      testId="grill-coverage"
      action={
        <Tagged testId="grill-coverage-count">
          <Text weight="semibold" numeric>
            {t("grill.coverage.count", { n: view.covered })}
          </Text>
        </Tagged>
      }
    >
      <Progress label={t("grill.coverage.label", { n: view.covered })} value={view.covered} max={7}>
        {t("grill.coverage.label", { n: view.covered })}
      </Progress>
      <List label={t("grill.coverage.title")}>
        {AREAS.map((area) => {
          const level = levels.get(area) ?? "none";
          return (
            <ListRow
              key={area}
              leading={
                <StatusMark
                  state={level}
                  label={t(`grill.coverage.state.${level}`)}
                  testId={`grill-area-${area}`}
                />
              }
              title={t(`grill.coverage.areas.${area}.name`)}
              meta={t(`grill.coverage.areas.${area}.hint`)}
            />
          );
        })}
      </List>
      <Text tone="secondary" size="sm">
        {t("grill.coverage.note")}
      </Text>
    </Section>
  );
}
