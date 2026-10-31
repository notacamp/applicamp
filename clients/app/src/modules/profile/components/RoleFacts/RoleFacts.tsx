import { useTranslation } from "react-i18next";

import {
  Button,
  FactRow,
  IconButton,
  List,
  ListRow,
  Section,
  Stack,
  Tagged,
  useUndoToast,
} from "~/lib/ui";

import { describeRole } from "../../api/grill";
import { useSaveProfile, type Profile } from "../../api/profile";

export interface RoleFactsProps {
  profile: Profile;
  /** Open Grill me on a position. */
  onGrill: (position: number) => void;
}

/**
 * What the person confirmed about each role in Grill me, under that role,
 * each fact with their own words, Remove (with Undo) and a way into Grill me
 * for the role.
 */
export function RoleFacts({ profile, onGrill }: RoleFactsProps) {
  const { t } = useTranslation("profile");
  const save = useSaveProfile();
  const undo = useUndoToast();

  const years = (from: string, to: string | undefined) =>
    to === undefined ? t("grill.yearsNow", { from }) : t("grill.years", { from, to });

  const remove = (position: number, id: string) => {
    const before = profile;
    const next: Profile = {
      ...profile,
      work: profile.work.map((work, at) =>
        at === position
          ? { ...work, facts: (work.facts ?? []).filter((fact) => fact.id !== id) }
          : work,
      ),
    };
    save.mutate(next, {
      onSuccess: () => {
        undo.push({
          label: t("grill.roleFacts.removed"),
          undo: () => {
            save.mutate(before);
          },
        });
      },
    });
  };

  return (
    <Section
      title={t("grill.roleFacts.title")}
      description={t("grill.roleFacts.lead")}
      testId="role-facts"
      anchor="role-facts"
    >
      <List label={t("grill.roleFacts.title")}>
        {profile.work.map((work, position) => {
          const facts = work.facts ?? [];
          const { title, meta } = describeRole(
            {
              position: work.position ?? null,
              company: work.name ?? null,
              start_date: work.startDate ?? null,
              end_date: work.endDate ?? null,
            },
            years,
          );
          return (
            <ListRow
              key={position}
              title={<Tagged testId={`role-facts-row-${String(position)}`}>{title}</Tagged>}
              meta={[
                meta,
                facts.length === 0
                  ? t("grill.roleFacts.none")
                  : t("grill.roleFacts.count", { count: facts.length }),
              ]
                .filter((part) => part !== "")
                .join(" · ")}
              trailing={
                <Button
                  variant="secondary"
                  data-testid={`role-grill-${String(position)}`}
                  onClick={() => {
                    onGrill(position);
                  }}
                >
                  {t("grill.aboutRole")}
                </Button>
              }
            >
              {facts.length === 0 ? null : (
                <Stack gap="sm" testId={`role-facts-list-${String(position)}`}>
                  {facts.map((fact) => (
                    <FactRow
                      key={fact.id}
                      text={fact.text}
                      quoteLabel={t("grill.roleFacts.yourWords")}
                      quote={fact.quote}
                      testId={`role-fact-${fact.id}`}
                      actions={
                        <IconButton
                          icon="trash"
                          label={t("grill.roleFacts.removeLabel", { text: fact.text })}
                          disabled={save.isPending}
                          data-testid={`role-fact-remove-${fact.id}`}
                          onClick={() => {
                            remove(position, fact.id);
                          }}
                        />
                      }
                    />
                  ))}
                </Stack>
              )}
            </ListRow>
          );
        })}
      </List>
    </Section>
  );
}
