import { useTranslation } from "react-i18next";

import {
  Button,
  ChipList,
  Cluster,
  CvLine,
  CvLines,
  Disclosure,
  Label,
  Paper,
  PaperHead,
  PaperRole,
  PaperSection,
  Stack,
  Tagged,
  Text,
} from "~/lib/ui";

import type { CvDraft } from "../../api/cv";
import { LineRow, ReasonText, type LineActions } from "./LineRow";

/** "Senior Engineer · Example Org". */
function roleLabel(title: string | null | undefined, company: string | null | undefined): string {
  return [title, company]
    .filter((part) => part !== undefined && part !== null && part !== "")
    .join(" · ");
}

/** "2021-03 – now". */
function span(
  start: string | null | undefined,
  end: string | null | undefined,
  now: string,
): string | null {
  if ((start === undefined || start === null) && (end === undefined || end === null)) {
    return null;
  }
  return `${start ?? ""} – ${end ?? now}`;
}

export interface CvSheetProps {
  draft: CvDraft;
  actions: LineActions;
  /** Put back a position, or a line, as the profile has it. */
  onPutBack: (what: { position: number } | { source: string }) => void;
}

/**
 * The CV as a page: the name and contact the server filled in, the headline
 * and summary, the positions with their lines, the skills, education and
 * languages, and what was left out. Every line can show where it came from.
 */
export function CvSheet({ draft, actions, onPutBack }: CvSheetProps) {
  const { t } = useTranslation("pipeline");
  const { contact } = draft;
  const contactLine = [
    contact.email,
    contact.location,
    contact.url,
    ...contact.links.map((link) => link.url),
  ]
    .filter((part): part is string => part !== undefined && part !== null && part !== "")
    .join(" · ");
  const lead = draft.experience[0];
  const latestWhere = lead === undefined ? undefined : roleLabel(lead.title, lead.company);
  const flaggedSkills = draft.skills.filter((skill) => skill.flags.length > 0);
  const leftOutCount = draft.left_out.roles.length + draft.left_out.lines.length;
  return (
    <Paper label={t("cv.sheet.label")} testId="cv-paper">
      <PaperHead
        name={
          contact.name === undefined || contact.name === null ? undefined : (
            <Tagged testId="cv-name">
              <span>{contact.name}</span>
            </Tagged>
          )
        }
        headline={
          draft.headline === undefined || draft.headline === null ? undefined : (
            <CvLines>
              <LineRow line={draft.headline} actions={actions} where={latestWhere} />
            </CvLines>
          )
        }
        contact={
          contactLine === "" ? undefined : (
            <Tagged testId="cv-contact">
              <span>{contactLine}</span>
            </Tagged>
          )
        }
      />
      {draft.summary === undefined || draft.summary === null ? null : (
        <PaperSection title={t("cv.sheet.summary")} testId="cv-summary">
          <CvLines>
            <LineRow line={draft.summary} actions={actions} where={latestWhere} />
          </CvLines>
        </PaperSection>
      )}
      <PaperSection title={t("cv.sheet.experience")} testId="cv-experience">
        {draft.experience.map((role) => (
          <PaperRole
            key={role.position}
            title={roleLabel(role.title, role.company)}
            meta={span(role.start_date, role.end_date, t("cv.sheet.now"))}
            testId={`cv-role-${String(role.position)}`}
          >
            <CvLines>
              {role.bullets.map((line) => (
                <LineRow
                  key={line.id}
                  line={line}
                  actions={actions}
                  where={roleLabel(role.title, role.company)}
                />
              ))}
            </CvLines>
          </PaperRole>
        ))}
      </PaperSection>
      {draft.skills.length === 0 ? null : (
        <PaperSection title={t("cv.sheet.skills")} testId="cv-skills">
          <ChipList>
            {draft.skills.map((skill) => (
              <Tagged key={skill.id} testId={`cv-skill-${skill.id}`}>
                <Label hue={skill.flags.length > 0 ? "red" : "gray"}>{skill.name}</Label>
              </Tagged>
            ))}
          </ChipList>
          {flaggedSkills.length === 0 ? null : (
            <CvLines>
              {flaggedSkills.map((skill) => (
                <CvLine
                  key={skill.id}
                  id={`cv-line-${skill.id}`}
                  flagged
                  testId={`cv-skill-flag-${skill.id}`}
                  detail={
                    <Stack gap="sm">
                      {skill.flags.map((flag) => (
                        <Text key={flag.kind} weight="medium" size="sm">
                          <ReasonText reason={flag} />
                        </Text>
                      ))}
                      {actions.editable ? (
                        <Cluster>
                          <Button
                            variant="secondary"
                            disabled={actions.busy}
                            data-testid={`cv-skill-remove-${skill.id}`}
                            onClick={() => {
                              actions.remove(skill.id);
                            }}
                          >
                            {t("cv.actions.removeSkill")}
                          </Button>
                          <Button
                            variant="ghost"
                            disabled={actions.busy}
                            data-testid={`cv-skill-confirm-${skill.id}`}
                            onClick={() => {
                              actions.confirm(skill.id);
                            }}
                          >
                            {t("cv.actions.addSkill")}
                          </Button>
                        </Cluster>
                      ) : null}
                    </Stack>
                  }
                >
                  {skill.name}
                </CvLine>
              ))}
            </CvLines>
          )}
        </PaperSection>
      )}
      {draft.education.length === 0 ? null : (
        <PaperSection title={t("cv.sheet.education")} testId="cv-education">
          <Stack gap="sm">
            {draft.education.map((school) => (
              <Text key={`${school.institution}-${school.start_date ?? ""}`} size="sm">
                {[
                  [school.study_type, school.area]
                    .filter((part) => part !== undefined && part !== null && part !== "")
                    .join(", "),
                  school.institution,
                  span(school.start_date, school.end_date, t("cv.sheet.now")),
                ]
                  .filter((part) => part !== null && part !== "")
                  .join(" · ")}
              </Text>
            ))}
          </Stack>
        </PaperSection>
      )}
      {draft.languages.length === 0 ? null : (
        <PaperSection title={t("cv.sheet.languages")} testId="cv-languages">
          <Text size="sm">
            {draft.languages
              .map((language) =>
                language.fluency === undefined || language.fluency === null
                  ? language.language
                  : `${language.language} (${language.fluency})`,
              )
              .join(" · ")}
          </Text>
        </PaperSection>
      )}
      {leftOutCount === 0 ? null : (
        <Tagged testId="cv-left-out">
          <Disclosure summary={t("cv.leftOut.title", { count: leftOutCount })}>
            <Stack gap="sm">
              <Text tone="secondary" size="sm">
                {t("cv.leftOut.lead")}
              </Text>
              {draft.left_out.roles.map((role) => (
                <Cluster key={role.position}>
                  <Text size="sm">
                    {t("cv.leftOut.role", {
                      role: roleLabel(role.title, role.company),
                      count: role.lines,
                    })}
                  </Text>
                  {actions.editable ? (
                    <Button
                      variant="link"
                      disabled={actions.busy}
                      data-testid={`cv-put-back-role-${String(role.position)}`}
                      onClick={() => {
                        onPutBack({ position: role.position });
                      }}
                    >
                      {t("cv.leftOut.putBack")}
                    </Button>
                  ) : null}
                </Cluster>
              ))}
              {draft.left_out.lines.map((line) => (
                <Cluster key={line.source}>
                  <Text size="sm">
                    {t("cv.leftOut.line", {
                      text: line.text,
                      role: roleLabel(line.title, line.company),
                    })}
                  </Text>
                  {actions.editable ? (
                    <Button
                      variant="link"
                      disabled={actions.busy}
                      data-testid={`cv-put-back-line-${line.source}`}
                      onClick={() => {
                        onPutBack({ source: line.source });
                      }}
                    >
                      {t("cv.leftOut.putBack")}
                    </Button>
                  ) : null}
                </Cluster>
              ))}
            </Stack>
          </Disclosure>
        </Tagged>
      )}
    </Paper>
  );
}
