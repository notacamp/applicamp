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
  /** Open "Tell me about you", where more is asked about the roles. */
  onAbout: () => void;
}

/**
 * What the person told Applicamp about each role (in Tell me about you, or
 * to a tailored CV's questions), under that role, each fact with their own
 * words, Remove (with Undo) and a way into Tell me about you.
 */
export function RoleFacts({ profile, onAbout }: RoleFactsProps) {
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
                  data-testid={`role-about-${String(position)}`}
                  onClick={onAbout}
                >
                  {t("grill.roleFacts.tellMore")}
                </Button>
              }
            >
              {facts.length === 0 ? null : (
                <Stack gap="sm" testId={`role-facts-list-${String(position)}`}>
                  {facts.map((fact) => (
                    <FactRow
                      key={fact.id}
                      text={fact.text}
                      quoteLabel={
                        fact.status === "confirmed_assumption"
                          ? t("grill.roleFacts.confirmed")
                          : t("grill.roleFacts.yourWords")
                      }
                      quote={
                        fact.status === "confirmed_assumption" && fact.basis
                          ? fact.basis
                          : fact.quote
                      }
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
