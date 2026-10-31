import { useTranslation } from "react-i18next";

import { Portrait, PortraitLegend, PortraitSentence, type PortraitKind } from "~/lib/ui";

import type { AboutSentence } from "../../api/about";

/** What a sentence looks like, given where it comes from and what the person said. */
export function kindOf(sentence: AboutSentence): PortraitKind {
  if (sentence.kind !== "assumed") {
    return sentence.kind;
  }
  if (sentence.choice === "right") {
    return "confirmed";
  }
  return sentence.choice === "wrong" ? "wrong" : "assumed";
}

export interface PortraitTextProps {
  sentences: readonly AboutSentence[];
  /** The assumption whose panel is open. */
  open?: string | undefined;
  /** Open (or close) an assumption's panel; without it assumptions are plain text. */
  onOpen?: ((id: string) => void) | undefined;
  testId: string;
}

/** The portrait as flowing text; every assumption is a button, so it is reachable by keyboard. */
export function PortraitText({ sentences, open, onOpen, testId }: PortraitTextProps) {
  const { t } = useTranslation("profile");
  return (
    <Portrait label={t("about.portraitLabel")} testId={testId}>
      {sentences.map((sentence) => {
        const select =
          onOpen !== undefined && sentence.kind === "assumed"
            ? () => {
                onOpen(sentence.id);
              }
            : undefined;
        return (
          <PortraitSentence
            key={sentence.id}
            kind={kindOf(sentence)}
            fresh={sentence.new}
            onSelect={select}
            expanded={select === undefined ? undefined : open === sentence.id}
            testId={`about-sentence-${sentence.id}`}
          >
            {sentence.text}
          </PortraitSentence>
        );
      })}
    </Portrait>
  );
}

/** The key to the marks. */
export function AboutLegend({ confirmed = false }: { confirmed?: boolean }) {
  const { t } = useTranslation("profile");
  return (
    <PortraitLegend
      label={t("about.legend.label")}
      items={[
        { kind: "known", label: t("about.legend.known") },
        { kind: "told", label: t("about.legend.told") },
        confirmed
          ? { kind: "confirmed", label: t("about.legend.confirmed") }
          : { kind: "assumed", label: t("about.legend.assumed") },
      ]}
    />
  );
}
