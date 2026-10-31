import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Stack, Tagged, Text } from "~/lib/ui";

import type { Action } from "../../api/completion";
import { isEmptyProfile, useProfile } from "../../api/profile";
import type { Question } from "../../api/questions";
import { ImportPanel } from "../ImportPanel/ImportPanel";
import { ImportReview } from "../ImportReview/ImportReview";
import { GoalsSection } from "../GoalsSection/GoalsSection";
import { ProfileView } from "../ProfileView/ProfileView";
import { StrengthCard } from "../StrengthCard/StrengthCard";

export interface ProfileWorkspaceProps {
  /** The import panel is asked for (`/profile?import=new`, Today's "Set up your profile"). */
  importing: boolean;
  /** Open or close the import panel. */
  onImporting: (open: boolean) => void;
  /** Open the goals interview at a question (the Goals section's buttons). */
  onStartGoals: (question: Question) => void;
  /** The person chose what to do about a part of their profile's strength (a part's Add). */
  onAction: (action: Action) => void;
  /** Open "Tell me about you": from the strength card, and from a role's kept facts. */
  onAbout: () => void;
}

/**
 * The profile part of Profile & goals: the import panel while there is no
 * profile (or one is asked for), the review of a draft, else the profile
 * itself, edited in place.
 */
export function ProfileWorkspace({
  importing,
  onImporting,
  onStartGoals,
  onAction,
  onAbout,
}: ProfileWorkspaceProps) {
  const { t } = useTranslation("profile");
  const profile = useProfile();
  const [review, setReview] = useState<string | null>(null);
  // A profile was just imported: the Goals section offers its questions.
  const [offer, setOffer] = useState(false);

  if (profile.isPending) {
    return (
      <Tagged testId="profile-loading">
        <Text tone="secondary">{t("loading")}</Text>
      </Tagged>
    );
  }
  if (profile.isError) {
    return (
      <Tagged testId="profile-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void profile.refetch();
              }}
            >
              {t("retry")}
            </Button>
          }
        />
      </Tagged>
    );
  }

  const view = profile.data;
  const empty = isEmptyProfile(view.profile);
  const restart = () => {
    setReview(null);
    onImporting(true);
  };
  if (review !== null) {
    return (
      <ImportReview
        id={review}
        current={view.profile}
        onRestart={restart}
        onDone={(outcome) => {
          setReview(null);
          setOffer(outcome === "applied");
          onImporting(false);
        }}
      />
    );
  }
  if (importing || empty) {
    return (
      <ImportPanel
        kinds={view.import_kinds}
        onDrafted={(draft) => {
          setReview(draft.id);
        }}
        onCancel={
          empty
            ? undefined
            : () => {
                onImporting(false);
              }
        }
      />
    );
  }
  return (
    <Stack gap="lg">
      <StrengthCard onAction={onAction} onAbout={onAbout} />
      <ProfileView
        view={view}
        onAbout={onAbout}
        onImportAgain={() => {
          onImporting(true);
        }}
        goals={
          <GoalsSection
            offer={offer}
            onDismissOffer={() => {
              setOffer(false);
            }}
            onStart={onStartGoals}
          />
        }
      />
    </Stack>
  );
}
