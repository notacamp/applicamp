import { useTranslation } from "react-i18next";

import { Heading, Property, PropertyList, Section, Stack, Tagged, Text } from "~/lib/ui";

/** The steps and what each costs, in the person's words. */
const STEPS = ["suggest", "find", "daily", "match"] as const;
const COSTS = ["suggest", "find", "none", "new"] as const;

/**
 * "How it works & cost": how companies are suggested and watched, which steps
 * use the AI and what they cost (nothing, except scoring the new jobs that
 * pass the person's requirements, with the nightly search).
 */
export function HowItWorks({ max }: { max: number }) {
  const { t } = useTranslation("sources");
  return (
    <Tagged testId="companies-how">
      <Stack gap="lg">
        <Stack gap="sm">
          <Text tone="secondary" size="sm" caps>
            {t("companies.how.eyebrow")}
          </Text>
          <Heading level={2} size="title">
            {t("companies.how.title")}
          </Heading>
        </Stack>
        <Stack gap="sm">
          {STEPS.map((step) => (
            <Text key={step}>
              <Text as="span" weight="semibold">
                {t(`companies.how.steps.${step}.title`)}
              </Text>{" "}
              {t(`companies.how.steps.${step}.body`)}
            </Text>
          ))}
        </Stack>
        <Section title={t("companies.how.costTitle")} testId="companies-how-cost">
          <PropertyList>
            {COSTS.map((cost) => (
              <Property key={cost} label={t(`companies.how.cost.${cost}.step`)}>
                {t(`companies.how.cost.${cost}.price`)}
              </Property>
            ))}
          </PropertyList>
          <Text tone="secondary" size="sm">
            {t("companies.how.limits", { max: String(max) })}
          </Text>
        </Section>
      </Stack>
    </Tagged>
  );
}
