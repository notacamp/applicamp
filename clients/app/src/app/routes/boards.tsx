import { useTranslation } from "react-i18next";

import { ButtonLink, Page, SkeletonList, Tagged } from "~/lib/ui";
import { BoardsAdmin, BoardsPage, useBoards, type BoardsTab } from "~/modules/sources";

import { useInAppLink } from "../links";

/** `admin` is the owners' page; `boards` and `scanned` are the list and what each search scanned. */
export type BoardsRouteTab = BoardsTab | "admin";

export interface BoardsProps {
  tab: BoardsRouteTab;
  onTab: (tab: BoardsTab) => void;
}

/**
 * `/boards`: for the people who manage the job boards, one page with every board's status, what
 * waits for them and the permission emails to copy; for everyone else, the job boards searches
 * read, ranked by average salary, and what each search scanned (`?tab=scanned`). The owners' list
 * of boards by status stays at `?tab=list`.
 */
export function Boards({ tab, onTab }: BoardsProps) {
  const { t } = useTranslation();
  const boards = useBoards();
  const link = useInAppLink();
  if (boards.isPending) {
    return (
      <Page testId="boards" title={t("boards.title")} wide>
        <Tagged testId="boards-route-loading">
          <SkeletonList rows={6} />
        </Tagged>
      </Page>
    );
  }
  if (tab === "admin" && boards.data?.can_manage === true) {
    return (
      <Page testId="boards" title={t("boards.adminTitle")} wide>
        <BoardsAdmin
          back={
            <div>
              <ButtonLink
                variant="ghost"
                icon="chevron-left"
                data-testid="boards-back"
                {...link("/profile")}
              >
                {t("boards.back")}
              </ButtonLink>
            </div>
          }
        />
      </Page>
    );
  }
  return (
    <Page testId="boards" title={t("boards.title")} lead={t("boards.lead")} wide>
      <BoardsPage tab={tab === "admin" ? "boards" : tab} onTab={onTab} />
    </Page>
  );
}
