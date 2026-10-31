import { Fragment, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  InlineText,
  Property,
  PropertyList,
  Section,
  Stack,
  Tagged,
  Text,
} from "~/lib/ui";

import {
  useSaveProfile,
  type Basics,
  type Profile,
  type ProfileView as View,
} from "../../api/profile";
import { BASICS, LISTS, withList, type BasicsKey, type Entry } from "../../api/sections";
import { EntryList } from "../EntryList/EntryList";
import { RoleFacts } from "../RoleFacts/RoleFacts";

export interface ProfileViewProps {
  view: View;
  /** Open the import panel again. */
  onImportAgain: () => void;
  /** Goals, between the about-you properties and the lists. */
  goals?: ReactNode;
  /** Open Grill me on a position (a role's "Grill me about this role"). */
  onGrill?: ((position: number) => void) | undefined;
}

/**
 * The profile as Notion-style properties (where it came from, then about
 * you) and its lists, every value edited in place; each change saves the
 * whole profile.
 */
export function ProfileView({ view, onImportAgain, goals, onGrill }: ProfileViewProps) {
  const { t, i18n } = useTranslation("profile");
  const save = useSaveProfile();
  const { profile } = view;

  const saveBasics = (key: BasicsKey) => (value: string) => {
    const basics: Basics = { ...profile.basics };
    basics[key] = value === "" ? null : value;
    save.mutate({ ...profile, basics });
  };
  const saveList = (list: (typeof LISTS)[number]) => (entries: Entry[]) => {
    save.mutate(withList(profile, list, entries as Profile[typeof list]));
  };

  const updated =
    view.updated_at === undefined || view.updated_at === null
      ? undefined
      : new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" }).format(
          new Date(view.updated_at),
        );
  const source =
    view.source === undefined || view.source === null ? undefined : t(`source.${view.source}`);

  return (
    <Stack gap="lg" testId="profile-view">
      <Section title={t("basics.section")} anchor="basics">
        <PropertyList testId="profile-properties">
          <Property label={t("source.label")} testId="profile-source">
            <Text>
              {updated === undefined || source === undefined
                ? (source ?? "")
                : t("source.updated", { source, date: updated })}
            </Text>
            <Button
              variant="ghost"
              icon="download"
              data-testid="import-again"
              onClick={onImportAgain}
            >
              {t("importAgain")}
            </Button>
          </Property>
          {BASICS.map((key) => (
            <Property key={key} label={t(`basics.${key}`)}>
              <InlineText
                label={t(`basics.${key}`)}
                value={profile.basics[key] ?? undefined}
                placeholder={t(`placeholders.${key}`)}
                multiline={key === "summary" || key === "background"}
                counter={(left) => t("form.charactersLeft", { left })}
                saveLabel={t("form.save")}
                cancelLabel={t("form.cancel")}
                busy={save.isPending}
                testId={`basics-${key}`}
                onSave={saveBasics(key)}
              />
            </Property>
          ))}
        </PropertyList>
      </Section>
      {save.isError ? (
        <Tagged testId="profile-save-error">
          <Banner tone="danger" icon="warning" role="alert" title={t("form.saveFailed")} />
        </Tagged>
      ) : null}
      {goals}
      {LISTS.map((list) => (
        <Fragment key={list}>
          <EntryList
            list={list}
            entries={profile[list]}
            busy={save.isPending}
            onChange={saveList(list)}
          />
          {list === "work" && onGrill !== undefined && profile.work.length > 0 ? (
            <RoleFacts profile={profile} onGrill={onGrill} />
          ) : null}
        </Fragment>
      ))}
    </Stack>
  );
}
