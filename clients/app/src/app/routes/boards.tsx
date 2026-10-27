import { useTranslation } from "react-i18next";

import { Page } from "~/lib/ui";
import { BoardsPage, type BoardsTab } from "~/modules/sources";

export interface BoardsProps {
  tab: BoardsTab;
  onTab: (tab: BoardsTab) => void;
}

/**
 * `/boards`: the job boards searches read, ranked by average salary, the
 * ones waiting for the owner, and what each search scanned (`?tab=scanned`).
 */
export function Boards({ tab, onTab }: BoardsProps) {
  const { t } = useTranslation();
  return (
    <Page testId="boards" title={t("boards.title")} lead={t("boards.lead")} wide>
      <BoardsPage tab={tab} onTab={onTab} />
    </Page>
  );
}
