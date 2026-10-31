import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Chip,
  Cluster,
  List,
  ListRow,
  PortraitPanel,
  Section,
  Stack,
  Tagged,
  Text,
  TextArea,
  TextField,
} from "~/lib/ui";

import {
  MAX_EXTRA,
  useAccuracy,
  useChoose,
  useEmployerSite,
  useMakePortrait,
  useSubmitExtra,
  type AboutAccuracyValue,
  type AboutChoice,
  type AboutView,
} from "../../api/about";
import { errorCode, firstError } from "./labels";
import { AboutLegend, PortraitText } from "./PortraitText";

const ACCURACY: readonly AboutAccuracyValue[] = ["yes", "some", "no"];
const CHOICES: readonly AboutChoice[] = ["right", "wrong", "later"];

export interface PortraitStepProps {
  view: AboutView;
  /** On to what was added from the text (or straight to the review when nothing was written). */
  onContinue: (added: boolean) => void;
}

/**
 * Screen 1: the portrait with its assumptions to settle, "Is this
 * accurate?" and "Anything else I should know about you?". Nothing is saved
 * here.
 */
export function PortraitStep({ view, onContinue }: PortraitStepProps) {
  const { t, i18n } = useTranslation("profile");
  const [open, setOpen] = useState<string | undefined>();
  const [extra, setExtra] = useState(view.extra ?? "");
  const choose = useChoose();
  const accuracy = useAccuracy();
  const submit = useSubmitExtra(i18n.language);
  const failure = firstError(choose.error, accuracy.error, submit.error);
  const busy = submit.isPending;

  const sentence = view.sentences.find((candidate) => candidate.id === open);
  const trimmed = extra.trim();
  const fresh = trimmed === (view.extra ?? "").trim() && view.changes.length > 0;

  const settle = (choice: AboutChoice) => {
    if (sentence === undefined) {
      return;
    }
    choose.mutate({ id: sentence.id, choice: sentence.choice === choice ? null : choice });
  };
  const next = () => {
    if (trimmed === "" || fresh) {
      onContinue(trimmed !== "");
      return;
    }
    submit.mutate(trimmed, {
      onSuccess: () => {
        onContinue(true);
      },
    });
  };

  return (
    <Stack gap="lg" testId="about-portrait-step">
      <Text tone="secondary">{t("about.lead")}</Text>
      <AboutLegend />
      <PortraitText
        sentences={view.sentences}
        open={open}
        onOpen={(id) => {
          setOpen((current) => (current === id ? undefined : id));
        }}
        testId="about-portrait"
      />
      {sentence === undefined ? null : (
        <PortraitPanel label={t("about.panel.label")} testId="about-panel">
          <Text>
            <b>{t("about.panel.assumed")}</b> {sentence.text}
          </Text>
          <Text tone="secondary" size="sm">
            {t("about.panel.why")}{" "}
            {sentence.downgraded
              ? t("about.panel.downgraded")
              : (sentence.why ?? t("about.panel.guess"))}
            {sentence.urls.length > 0 ? ` ${sentence.urls[0] ?? ""}` : ""}
          </Text>
          <Cluster>
            {CHOICES.map((choice) => (
              <Button
                key={choice}
                variant={sentence.choice === choice ? "primary" : "secondary"}
                aria-pressed={sentence.choice === choice}
                disabled={choose.isPending}
                data-testid={`about-choice-${choice}`}
                onClick={() => {
                  settle(choice);
                }}
              >
                {t(`about.panel.${choice}`)}
              </Button>
            ))}
          </Cluster>
        </PortraitPanel>
      )}
      <Employers view={view} />
      <Section title={t("about.accuracy.title")}>
        <Cluster>
          {ACCURACY.map((value) => (
            <Chip
              key={value}
              pressed={view.accuracy === value}
              data-testid={`about-accuracy-${value}`}
              onClick={() => {
                accuracy.mutate(value);
              }}
            >
              {t(`about.accuracy.${value}`)}
            </Chip>
          ))}
        </Cluster>
      </Section>
      <Section title={t("about.extra.title")} description={t("about.extra.lead")}>
        <Stack gap="sm">
          <TextArea
            label={t("about.extra.label")}
            value={extra}
            rows={5}
            maxLength={MAX_EXTRA}
            hint={t("about.extra.hint", { left: MAX_EXTRA - extra.length })}
            data-testid="about-extra"
            onChange={(event) => {
              setExtra(event.target.value);
            }}
          />
        </Stack>
      </Section>
      {failure === undefined ? null : (
        <Tagged testId="about-error">
          <Banner
            tone="danger"
            icon="warning"
            role="alert"
            title={t(`about.errors.${errorCode(failure)}`)}
          />
        </Tagged>
      )}
      {busy ? (
        <Tagged testId="about-working">
          <Text tone="secondary">{t("about.extra.reading")}</Text>
        </Tagged>
      ) : null}
      <Cluster>
        <Button
          variant="primary"
          icon="arrow-right"
          disabled={busy}
          data-testid="about-continue"
          onClick={next}
        >
          {t("about.continue")}
        </Button>
        <Text tone="tertiary" size="sm" as="span">
          {t("about.cost")}
        </Text>
      </Cluster>
    </Stack>
  );
}

/** The latest employers: what was read on their own sites, and where to add a site. */
function Employers({ view }: { view: AboutView }) {
  const { t, i18n } = useTranslation("profile");
  const site = useEmployerSite();
  const again = useMakePortrait(i18n.language);
  const [typed, setTyped] = useState<Record<number, string>>({});
  if (view.employers.length === 0) {
    return null;
  }
  const waiting = view.employers.some((employer) => employer.state === "unread");
  const failure = firstError(site.error, again.error);
  return (
    <Section
      title={t("about.employers.title")}
      description={t("about.employers.lead")}
      testId="about-employers"
    >
      <Stack gap="sm">
        <List label={t("about.employers.title")}>
          {view.employers.map((employer) => (
            <ListRow
              key={employer.position}
              title={employer.company}
              meta={t(`about.employers.state.${employer.state}`, {
                count: employer.facts,
                site: employer.site ?? "",
                error: employer.error ?? "",
              })}
              trailing={
                employer.site === null || employer.site === undefined ? (
                  <Cluster>
                    <TextField
                      label={t("about.employers.siteLabel", { company: employer.company })}
                      hideLabel
                      placeholder={t("about.employers.sitePlaceholder")}
                      value={typed[employer.position] ?? ""}
                      data-testid={`about-site-${String(employer.position)}`}
                      onChange={(event) => {
                        setTyped({ ...typed, [employer.position]: event.target.value });
                      }}
                    />
                    <Button
                      variant="secondary"
                      disabled={site.isPending || (typed[employer.position] ?? "").trim() === ""}
                      data-testid={`about-site-add-${String(employer.position)}`}
                      onClick={() => {
                        site.mutate({
                          position: employer.position,
                          url: (typed[employer.position] ?? "").trim(),
                        });
                      }}
                    >
                      {t("about.employers.add")}
                    </Button>
                  </Cluster>
                ) : null
              }
            />
          ))}
        </List>
        {waiting ? (
          <Cluster>
            <Button
              variant="secondary"
              disabled={again.isPending}
              data-testid="about-update"
              onClick={() => {
                again.mutate(true);
              }}
            >
              {t("about.employers.update")}
            </Button>
            <Text tone="tertiary" size="sm" as="span">
              {t("about.employers.updateCost")}
            </Text>
          </Cluster>
        ) : null}
        {failure === undefined ? null : (
          <Banner
            tone="danger"
            icon="warning"
            role="alert"
            title={t(`about.errors.${errorCode(failure)}`)}
          />
        )}
      </Stack>
    </Section>
  );
}
