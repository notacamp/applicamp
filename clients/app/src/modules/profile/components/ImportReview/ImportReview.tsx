import { useTranslation } from "react-i18next";

import { errorStatus } from "~/lib/api";
import {
  Banner,
  Button,
  Cluster,
  List,
  ListRow,
  Property,
  PropertyList,
  Section,
  Stack,
  Tag,
  Tagged,
  Text,
} from "~/lib/ui";

import { useApplyImport, useDiscardImport, useImport, type ApplyMode } from "../../api/imports";
import { isEmptyProfile, type Profile } from "../../api/profile";
import { BASICS, compareBasics, compareList, LISTS, type Change } from "../../api/sections";
import { useDescribe } from "../EntryList/EntryList";

export interface ImportReviewProps {
  /** The draft. */
  id: string;
  /** The profile as it is now. */
  current: Profile;
  /** Applied or discarded: back to the profile. */
  onDone: () => void;
  /** The draft is gone: import again. */
  onRestart: () => void;
}

function ChangeTag({ change }: { change: Change }) {
  const { t } = useTranslation("profile");
  return <Tag icon={change === "same" ? "check" : "plus"}>{t(`review.${change}`)}</Tag>;
}

/**
 * The draft beside the profile: every value marked new, changed or the
 * same, what the source check left out, then Replace, Merge or Discard.
 * Nothing is saved before a choice.
 */
export function ImportReview({ id, current, onDone, onRestart }: ImportReviewProps) {
  const { t } = useTranslation("profile");
  const draft = useImport(id);
  const apply = useApplyImport();
  const discard = useDiscardImport();
  const describe = useDescribe();
  const busy = apply.isPending || discard.isPending;

  if (draft.isPending) {
    return (
      <Tagged testId="review-loading">
        <Text tone="secondary">{t("review.loading")}</Text>
      </Tagged>
    );
  }
  if (draft.isError) {
    return (
      <Tagged testId="review-gone">
        <Banner
          tone="danger"
          icon="warning"
          title={errorStatus(draft.error) === 404 ? t("review.gone") : t("review.loadFailed")}
          action={
            <Button variant="secondary" onClick={onRestart}>
              {t("importAgain")}
            </Button>
          }
        />
      </Tagged>
    );
  }

  const { draft: next, dropped } = draft.data;
  const choose = (mode: ApplyMode) => () => {
    apply.mutate({ id, mode }, { onSuccess: onDone });
  };
  const basics = BASICS.flatMap((key) => {
    const change = compareBasics(key, current.basics, next.basics);
    const value = next.basics[key];
    return change === undefined || value === undefined || value === null
      ? []
      : [{ key, change, value, now: current.basics[key] }];
  });

  return (
    <Section title={t("review.title")} description={t("review.lead")} testId="import-review">
      <Stack gap="lg">
        {dropped > 0 ? (
          <Tagged testId="review-dropped">
            <Banner icon="warning" title={t("review.dropped", { dropped })}>
              {t("review.droppedBody")}
            </Banner>
          </Tagged>
        ) : null}
        {basics.length > 0 ? (
          <PropertyList testId="review-basics">
            {basics.map(({ key, change, value, now }) => (
              <Property key={key} label={t(`basics.${key}`)}>
                <Text>{value}</Text>
                <ChangeTag change={change} />
                {change === "changed" && now !== undefined && now !== null ? (
                  <Text tone="tertiary" size="sm">
                    {t("review.now", { value: now })}
                  </Text>
                ) : null}
              </Property>
            ))}
          </PropertyList>
        ) : null}
        {LISTS.map((list) => {
          const rows = compareList(list, current, next);
          if (rows.length === 0) {
            return null;
          }
          return (
            <Tagged key={list} testId={`review-${list}`}>
              <List label={t(`sections.${list}`)}>
                {rows.map(({ entry, change }, index) => {
                  const { title, meta } = describe(list, entry);
                  return (
                    <ListRow
                      key={index}
                      title={title}
                      meta={
                        meta === "" ? t(`sections.${list}`) : `${t(`sections.${list}`)} · ${meta}`
                      }
                      badge={<ChangeTag change={change} />}
                    />
                  );
                })}
              </List>
            </Tagged>
          );
        })}
        {apply.isError || discard.isError ? (
          <Tagged testId="review-error">
            <Banner tone="danger" icon="warning" role="alert" title={t("review.failed")} />
          </Tagged>
        ) : null}
        <Cluster>
          <Button
            variant="primary"
            icon="check"
            disabled={busy}
            data-testid="review-replace"
            onClick={choose("replace")}
          >
            {t("review.replace")}
          </Button>
          {isEmptyProfile(current) ? null : (
            <Button
              variant="secondary"
              icon="plus"
              disabled={busy}
              data-testid="review-merge"
              onClick={choose("merge")}
            >
              {t("review.merge")}
            </Button>
          )}
          <Button
            variant="ghost"
            disabled={busy}
            data-testid="review-discard"
            onClick={() => {
              discard.mutate(id, { onSuccess: onDone });
            }}
          >
            {t("review.discard")}
          </Button>
        </Cluster>
      </Stack>
    </Section>
  );
}
