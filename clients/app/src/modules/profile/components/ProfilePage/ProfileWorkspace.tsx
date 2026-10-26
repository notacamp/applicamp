import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Tagged, Text } from "~/lib/ui";

import { isEmptyProfile, useProfile } from "../../api/profile";
import { ImportPanel } from "../ImportPanel/ImportPanel";
import { ImportReview } from "../ImportReview/ImportReview";
import { ProfileView } from "../ProfileView/ProfileView";

export interface ProfileWorkspaceProps {
  /** The import panel is asked for (`/profile?import=new`, Today's "Set up your profile"). */
  importing: boolean;
  /** Open or close the import panel. */
  onImporting: (open: boolean) => void;
}

/**
 * The profile part of Profile & goals: the import panel while there is no
 * profile (or one is asked for), the review of a draft, else the profile
 * itself, edited in place.
 */
export function ProfileWorkspace({ importing, onImporting }: ProfileWorkspaceProps) {
  const { t } = useTranslation("profile");
  const profile = useProfile();
  const [review, setReview] = useState<string | null>(null);

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
        onDone={() => {
          setReview(null);
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
    <ProfileView
      view={view}
      onImportAgain={() => {
        onImporting(true);
      }}
    />
  );
}
