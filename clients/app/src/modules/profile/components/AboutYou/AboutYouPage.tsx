import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Cluster, Page, Placeholder, Stack, Tagged, Text } from "~/lib/ui";

import { useAbout, useMakePortrait } from "../../api/about";
import { AddedStep } from "./AddedStep";
import { errorCode } from "./labels";
import { PortraitStep } from "./PortraitStep";
import { ReviewStep } from "./ReviewStep";

export interface AboutYouPageProps {
  /** Back to Profile & goals. */
  onBack: () => void;
  /** The month's AI spend line. */
  spend?: ReactNode;
}

type Step = "portrait" | "added" | "review";

/**
 * `/profile/about`, "Tell me about you": what Applicamp knows and assumes
 * about you, as a portrait to confirm; anything else you want to add; and a
 * last look before it all is saved in one go. Opening it makes the portrait
 * (one model call per profile version); nothing is saved until Save.
 */
export function AboutYouPage({ onBack, spend }: AboutYouPageProps) {
  const { t, i18n } = useTranslation("profile");
  const about = useAbout();
  const portrait = useMakePortrait(i18n.language);
  const [step, setStep] = useState<Step>("portrait");
  /** The profile's strength after Save; the wizard is over. */
  const [saved, setSaved] = useState<number | null>(null);
  const started = useRef(false);
  const view = about.data;
  const needs = view !== undefined && view.has_profile && !view.built;

  // The first visit for a profile version makes the portrait, once.
  const { mutate } = portrait;
  useEffect(() => {
    if (needs && !started.current) {
      started.current = true;
      mutate(false);
    }
  }, [needs, mutate]);

  let title = t("about.title");
  if (saved !== null) {
    title = t("about.saved.title");
  } else if (step === "review") {
    title = t("about.reviewTitle");
  } else if (step === "added") {
    title = t("about.addedTitle");
  }

  const body = () => {
    if (saved !== null) {
      return (
        <Stack gap="md" testId="about-saved">
          <Banner tone="neutral" icon="check" role="status" title={t("about.saved.banner")}>
            {t("about.saved.lead", { score: saved })}
          </Banner>
          <Cluster>
            <Button variant="primary" data-testid="about-saved-done" onClick={onBack}>
              {t("about.saved.done")}
            </Button>
          </Cluster>
        </Stack>
      );
    }
    if (about.isPending) {
      return (
        <Tagged testId="about-loading">
          <Text tone="secondary">{t("about.loading")}</Text>
        </Tagged>
      );
    }
    if (about.isError || view === undefined) {
      return (
        <Tagged testId="about-load-error">
          <Banner
            tone="danger"
            icon="warning"
            title={t("about.loadFailed")}
            action={
              <Button
                variant="secondary"
                onClick={() => {
                  void about.refetch();
                }}
              >
                {t("retry")}
              </Button>
            }
          />
        </Tagged>
      );
    }
    if (!view.has_profile) {
      return (
        <Placeholder
          testId="about-no-profile"
          icon="tasks"
          title={t("about.noProfileTitle")}
          action={
            <Button variant="primary" data-testid="about-back" onClick={onBack}>
              {t("about.back")}
            </Button>
          }
        >
          {t("about.noProfileLead")}
        </Placeholder>
      );
    }
    if (!view.built) {
      if (portrait.isError) {
        return (
          <Tagged testId="about-error">
            <Banner
              tone="danger"
              icon="warning"
              role="alert"
              title={t(`about.errors.${errorCode(portrait.error)}`)}
              action={
                <Button
                  variant="secondary"
                  data-testid="about-retry"
                  onClick={() => {
                    portrait.mutate(false);
                  }}
                >
                  {t("retry")}
                </Button>
              }
            />
          </Tagged>
        );
      }
      return (
        <Tagged testId="about-making">
          <Text tone="secondary">{t("about.making")}</Text>
        </Tagged>
      );
    }
    if (step === "review") {
      return (
        <ReviewStep
          onBack={() => {
            setStep(view.changes.length > 0 ? "added" : "portrait");
          }}
          onSaved={setSaved}
        />
      );
    }
    if (step === "added") {
      return (
        <AddedStep
          view={view}
          onBack={() => {
            setStep("portrait");
          }}
          onReview={() => {
            setStep("review");
          }}
        />
      );
    }
    return (
      <PortraitStep
        view={view}
        onContinue={(added) => {
          setStep(added ? "added" : "review");
        }}
      />
    );
  };

  return (
    <Page testId="about" eyebrow={t("about.eyebrow")} title={title}>
      {body()}
      {step === "portrait" ? spend : null}
    </Page>
  );
}
