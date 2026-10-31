import { useTranslation } from "react-i18next";

import { Page } from "~/lib/ui";
import { RunsPage } from "~/modules/runs";

export interface AgentsProps {
  /** The run asked for (`?run=`). */
  run: string | undefined;
  /** Show a run; `null` goes back to the list. */
  onRun: (id: string | null) => void;
}

/** `/agents`: the runs, each in its own sandbox, watched live (`?run=` one of them). */
export function Agents({ run, onRun }: AgentsProps) {
  const { t } = useTranslation();
  return (
    <Page testId="agents" title={t("agents.title")} lead={t("agents.lead")} wide>
      <RunsPage selected={run} onSelect={onRun} />
    </Page>
  );
}
