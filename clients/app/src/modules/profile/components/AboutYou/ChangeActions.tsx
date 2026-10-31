import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Cluster, Stack, Tagged, Text, TextArea } from "~/lib/ui";

import { useRemoveChange, useSettleChange, type AboutFlag, type ReviewRow } from "../../api/about";
import { errorCode, firstError } from "./labels";

export interface ChangeActionsProps {
  /** The change (a review row of something they told). */
  id: string;
  value: string;
  flags: readonly AboutFlag[];
  resolution: ReviewRow["resolution"];
}

/**
 * What the person can do with a pending change: when a claim in it is not in
 * their words (shown, never dropped) *It's true*, *Fix* or *Use my own
 * words*; always Edit (checked again against their words) and Remove.
 */
export function ChangeActions({ id, value, flags, resolution }: ChangeActionsProps) {
  const { t } = useTranslation("profile");
  const settle = useSettleChange();
  const remove = useRemoveChange();
  const [editing, setEditing] = useState<string | null>(null);
  const open = flags.length > 0 && (resolution === null || resolution === undefined);
  const busy = settle.isPending || remove.isPending;
  const failure = firstError(settle.error, remove.error);

  return (
    <Stack gap="sm" testId={`about-change-${id}`}>
      {flags.length > 0 ? (
        <Tagged testId={`about-flags-${id}`}>
          <Stack gap="sm">
            {flags.map((flag) => (
              <Text key={`${flag.kind}-${flag.claim}`} tone="danger" size="sm">
                {flag.kind === "quote"
                  ? t("about.flag.quote")
                  : t("about.flag.claim", { claim: flag.claim })}
              </Text>
            ))}
          </Stack>
        </Tagged>
      ) : null}
      {resolution === null || resolution === undefined ? null : (
        <Text tone="secondary" size="sm" as="p">
          {t(`about.resolution.${resolution}`)}
        </Text>
      )}
      {editing === null ? (
        <Cluster>
          {open ? (
            <>
              <Button
                variant="secondary"
                disabled={busy}
                data-testid={`about-true-${id}`}
                onClick={() => {
                  settle.mutate({ id, action: "true" });
                }}
              >
                {t("about.change.true")}
              </Button>
              <Button
                variant="secondary"
                disabled={busy}
                data-testid={`about-own-words-${id}`}
                onClick={() => {
                  settle.mutate({ id, action: "own_words" });
                }}
              >
                {t("about.change.ownWords")}
              </Button>
            </>
          ) : null}
          <Button
            variant="ghost"
            disabled={busy}
            data-testid={`about-edit-${id}`}
            onClick={() => {
              setEditing(value);
            }}
          >
            {t(open ? "about.change.fix" : "about.change.edit")}
          </Button>
          <Button
            variant="ghost"
            disabled={busy}
            data-testid={`about-remove-${id}`}
            onClick={() => {
              remove.mutate(id);
            }}
          >
            {t("about.change.remove")}
          </Button>
        </Cluster>
      ) : (
        <Stack gap="sm">
          <TextArea
            label={t("about.change.editLabel")}
            value={editing}
            rows={3}
            data-testid={`about-edit-text-${id}`}
            onChange={(event) => {
              setEditing(event.target.value);
            }}
          />
          <Cluster>
            <Button
              variant="primary"
              disabled={busy || editing.trim() === ""}
              data-testid={`about-edit-save-${id}`}
              onClick={() => {
                settle.mutate(
                  { id, action: "edit", value: editing },
                  {
                    onSuccess: () => {
                      setEditing(null);
                    },
                  },
                );
              }}
            >
              {t("about.change.recheck")}
            </Button>
            <Button
              variant="ghost"
              onClick={() => {
                setEditing(null);
              }}
            >
              {t("about.change.cancel")}
            </Button>
          </Cluster>
        </Stack>
      )}
      {failure === undefined ? null : (
        <Banner
          tone="danger"
          icon="warning"
          role="alert"
          title={t(`about.errors.${errorCode(failure)}`)}
        />
      )}
    </Stack>
  );
}
