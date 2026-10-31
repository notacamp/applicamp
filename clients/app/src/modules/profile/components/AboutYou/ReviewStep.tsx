import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Cluster,
  List,
  ListRow,
  Section,
  Stack,
  Tag,
  Tagged,
  Text,
} from "~/lib/ui";

import {
  useAboutReview,
  useRemoveChange,
  useChoose,
  useSaveAbout,
  type ReviewGroup,
  type ReviewRow,
} from "../../api/about";
import { ChangeActions } from "./ChangeActions";
import { errorCode, firstError } from "./labels";
import { AboutLegend, PortraitText } from "./PortraitText";

export interface ReviewStepProps {
  onBack: () => void;
  /** Everything was saved; the profile is now this strong. */
  onSaved: (strength: number) => void;
}

/**
 * The last screen: the final portrait and every line that will be saved,
 * grouped by the part of the profile it changes; each marked *You told me*
 * or *Confirmed*, with Edit and Remove. Assumptions never confirmed are left
 * out. Save writes everything in one go.
 */
export function ReviewStep({ onBack, onSaved }: ReviewStepProps) {
  const { t } = useTranslation("profile");
  const review = useAboutReview(true);
  const save = useSaveAbout();
  const unconfirm = useChoose();
  const remove = useRemoveChange();
  const failure = firstError(save.error, unconfirm.error, remove.error);

  if (review.isPending) {
    return (
      <Tagged testId="about-review-loading">
        <Text tone="secondary">{t("about.loading")}</Text>
      </Tagged>
    );
  }
  if (review.isError) {
    return (
      <Tagged testId="about-review-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("about.loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void review.refetch();
              }}
            >
              {t("retry")}
            </Button>
          }
        />
      </Tagged>
    );
  }
  const groupTitle = (group: ReviewGroup): string =>
    group.key === "position" ? (group.title ?? "") : t(`about.groups.${group.key}`);
  const { sentences, groups, open_flags: openFlags, left_out: leftOut } = review.data;
  const nothing = groups.length === 0;

  const rowActions = (row: ReviewRow) =>
    row.source === "told" ? (
      <ChangeActions id={row.id} value={row.text} flags={row.flags} resolution={row.resolution} />
    ) : (
      <Button
        variant="ghost"
        disabled={unconfirm.isPending}
        data-testid={`about-unconfirm-${row.id}`}
        onClick={() => {
          unconfirm.mutate({ id: row.id, choice: null });
        }}
      >
        {t("about.change.remove")}
      </Button>
    );

  return (
    <Stack gap="lg" testId="about-review-step">
      <Text tone="secondary">{t("about.review.lead")}</Text>
      <AboutLegend confirmed />
      <PortraitText sentences={sentences} testId="about-portrait-final" />
      {leftOut > 0 ? (
        <Tagged testId="about-left-out">
          <Text tone="secondary" size="sm">
            {t("about.review.leftOut", { count: leftOut })}
          </Text>
        </Tagged>
      ) : null}
      <Section title={t("about.review.changes")} testId="about-review">
        {nothing ? (
          <Text tone="secondary">{t("about.review.nothing")}</Text>
        ) : (
          <Stack gap="md">
            {groups.map((group) => (
              <Tagged
                key={`${group.key}-${String(group.position ?? "")}`}
                testId={`about-group-${group.key}${group.position === null || group.position === undefined ? "" : `-${String(group.position)}`}`}
              >
                <Stack gap="sm">
                  <Text weight="semibold">{groupTitle(group)}</Text>
                  <List label={groupTitle(group)}>
                    {group.rows.map((row) => (
                      <ListRow
                        key={row.id}
                        title={row.text}
                        badge={
                          <Tag>
                            {t(
                              row.source === "told"
                                ? "about.review.told"
                                : "about.review.confirmed",
                            )}
                          </Tag>
                        }
                      >
                        {rowActions(row)}
                      </ListRow>
                    ))}
                  </List>
                </Stack>
              </Tagged>
            ))}
          </Stack>
        )}
      </Section>
      {openFlags > 0 ? (
        <Tagged testId="about-open-flags">
          <Text tone="danger" size="sm">
            {t("about.added.open", { count: openFlags })}
          </Text>
        </Tagged>
      ) : null}
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
      <Cluster>
        <Button
          variant="primary"
          disabled={save.isPending || openFlags > 0 || nothing}
          data-testid="about-save"
          onClick={() => {
            save.mutate(undefined, {
              onSuccess: (saved) => {
                onSaved(saved.strength);
              },
            });
          }}
        >
          {t("about.review.save")}
        </Button>
        <Button variant="ghost" data-testid="about-review-back" onClick={onBack}>
          {t("about.review.back")}
        </Button>
      </Cluster>
    </Stack>
  );
}
