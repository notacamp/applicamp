import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { Page } from "~/lib/ui";
import { invalidatePipeline, PipelinePage } from "~/modules/pipeline";
import { SearchNowButton, SearchProgress } from "~/modules/sources";

import { NoSkillsLine } from "../NoSkillsLine";

/** What the Pipeline's address asks for. */
export interface PipelineSearchParams {
  /** Only the new matches. */
  new?: boolean | undefined;
  /** Start a search, or open New, once (⌘K from another page). */
  run?: "search" | "add" | undefined;
}

export interface PipelineProps {
  newOnly: boolean;
  run: "search" | "add" | undefined;
  /** Change the address's search (replacing the entry). */
  onSearch: (next: PipelineSearchParams) => void;
}

/**
 * `/pipeline`: every opportunity, from first match to offer, with Search now
 * and its progress (the sources module) beside them.
 */
export function Pipeline({ newOnly, run, onSearch }: PipelineProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const keep = newOnly ? { new: true } : {};
  return (
    <Page testId="pipeline" title={t("pipeline.title")} lead={t("pipeline.lead")} wide>
      <PipelinePage
        newOnly={newOnly}
        onNewOnly={(only) => {
          onSearch(only ? { new: true } : {});
        }}
        adding={run === "add"}
        onAdding={(open) => {
          onSearch(open ? { ...keep, run: "add" } : keep);
        }}
        search={
          <SearchNowButton
            testId="pipeline-search-now"
            autoStart={run === "search"}
            onAutoStarted={() => {
              onSearch(keep);
            }}
          />
        }
        progress={
          <>
            <SearchProgress
              onFinished={() => {
                void invalidatePipeline(queryClient);
              }}
            />
            <NoSkillsLine testId="pipeline-no-skills" />
          </>
        }
      />
    </Page>
  );
}
