import { Fragment } from "react";
import { useTranslation } from "react-i18next";

import {
  Cluster,
  CvLine,
  CvLines,
  Disclosure,
  Label,
  Marked,
  Paper,
  PaperHead,
  Stack,
  Tagged,
  Text,
  type LabelHue,
} from "~/lib/ui";

import type { LetterDraft, LetterParagraph, LetterTag } from "../../api/letter";
import { SentenceFix, type SentenceActions } from "./SentenceFix";

const TAG_HUES: Record<LetterTag, LabelHue> = {
  company_fact: "green",
  from_profile: "gray",
  no_claims: "blue",
};

function shownPage(url: string): string {
  try {
    const parsed = new URL(url);
    return `${parsed.host.replace(/^www\./, "")}${parsed.pathname === "/" ? "" : parsed.pathname}`;
  } catch {
    return url;
  }
}

/** "Where this came from": the sources a paragraph cites, with the words the check looks for picked out. */
function WhereFrom({ paragraph }: { paragraph: LetterParagraph }) {
  const { t } = useTranslation("pipeline");
  const words = paragraph.sentences.flatMap((sentence) => sentence.claims);
  if (paragraph.sources.length === 0) {
    return (
      <Text tone="secondary" size="sm">
        {t("letter.from.none")}
      </Text>
    );
  }
  return (
    <Stack gap="sm">
      {paragraph.sources.map((source) => (
        <Text key={source.id} tone="secondary" size="sm" as="div">
          <Marked text={source.text} words={words} />
          {source.kind === "company" && source.url !== undefined && source.url !== null ? (
            <> {t("letter.from.companyPage", { page: shownPage(source.url) })}</>
          ) : source.role === undefined || source.role === null ? null : (
            <> · {source.role}</>
          )}
        </Text>
      ))}
    </Stack>
  );
}

/**
 * One paragraph of the letter: its sentences, tagged by what they rest on
 * (a company fact, your profile, no claims). A sentence that can't be traced
 * is picked out below it with its reason and what to do; the rest open
 * "Where this came from".
 */
function ParagraphRow({
  paragraph,
  actions,
}: {
  paragraph: LetterParagraph;
  actions: SentenceActions;
}) {
  const { t } = useTranslation("pipeline");
  const flagged = paragraph.sentences.filter((sentence) => sentence.flags.length > 0);
  const edited = paragraph.sentences.some((sentence) => sentence.edited);
  return (
    <CvLine
      id={`letter-paragraph-${paragraph.id}`}
      flagged={flagged.length > 0}
      testId={`letter-paragraph-${paragraph.id}`}
      mark={
        <Tagged testId={`letter-tags-${paragraph.id}`}>
          <Cluster>
            {flagged.length > 0 ? <Label hue="red">{t("letter.flag.label")}</Label> : null}
            {paragraph.tags.map((tag) => (
              <Label key={tag} hue={TAG_HUES[tag]}>
                {t(`letter.tags.${tag}`)}
              </Label>
            ))}
            {edited ? <Label hue="yellow">{t("letter.tags.edited")}</Label> : null}
          </Cluster>
        </Tagged>
      }
      detail={
        <Stack gap="sm">
          {flagged.map((sentence) => (
            <SentenceFix
              key={sentence.id}
              sentence={sentence}
              actions={actions}
              where={t("letter.actions.whereProfile")}
            />
          ))}
          <Tagged testId={`letter-from-toggle-${paragraph.id}`}>
            <Disclosure summary={t("letter.from.show")}>
              <WhereFrom paragraph={paragraph} />
            </Disclosure>
          </Tagged>
        </Stack>
      }
    >
      {paragraph.sentences.map((sentence, index) => (
        <Fragment key={sentence.id}>
          {index === 0 ? null : " "}
          <Tagged testId={`letter-sentence-${sentence.id}`}>
            <span>{sentence.text}</span>
          </Tagged>
        </Fragment>
      ))}
    </CvLine>
  );
}

export interface LetterSheetProps {
  draft: LetterDraft;
  actions: SentenceActions;
}

/**
 * The letter as a page: the name and contact the server filled in, its
 * greeting, the paragraphs with their tags and sources, and the sign-off.
 */
export function LetterSheet({ draft, actions }: LetterSheetProps) {
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
  return (
    <Paper label={t("letter.sheet.label")} testId="letter-paper">
      <PaperHead
        name={
          contact.name === undefined || contact.name === null ? undefined : (
            <Tagged testId="letter-name">
              <span>{contact.name}</span>
            </Tagged>
          )
        }
        contact={
          contactLine === "" ? undefined : (
            <Tagged testId="letter-contact">
              <span>{contactLine}</span>
            </Tagged>
          )
        }
      />
      <Stack gap="sm">
        <Tagged testId="letter-greeting">
          <Text>{draft.greeting}</Text>
        </Tagged>
        <CvLines>
          {draft.paragraphs.map((paragraph) => (
            <ParagraphRow key={paragraph.id} paragraph={paragraph} actions={actions} />
          ))}
        </CvLines>
        <Text>{draft.closing}</Text>
        {contact.name === undefined || contact.name === null ? null : <Text>{contact.name}</Text>}
      </Stack>
    </Paper>
  );
}
