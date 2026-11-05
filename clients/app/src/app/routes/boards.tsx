import { useTranslation } from "react-i18next";

import { ButtonLink, Page, SkeletonList, Stack, Tagged, ViewSwitch } from "~/lib/ui";
import {
  BoardsAdmin,
  BoardsPage,
  MySitesPage,
  ScannedView,
  useBoards,
  type BoardsTab,
} from "~/modules/sources";

import { useInAppLink } from "../links";

/** `mine`: everyone's sites; `all`: the owners' page; `scanned` and `list`: searches and boards by status. */
export type BoardsRouteTab = "mine" | "all" | "scanned" | "list";

export interface BoardsProps {
  /** None: the owners' page for the owners, the person's sites for everyone else. */
  tab: BoardsRouteTab | undefined;
  onTab: (tab: BoardsRouteTab) => void;
}

/**
 * `/boards`: the Job sites. For everyone, the sites the app reads for them, each On or Off, and
 * the companies to follow. The people who manage the job boards also see "All sites", every
 * board's status, what waits for them and the permission emails to copy, and open on it. What
 * each search scanned is under "Searches"; the owners' list of boards by status stays at
 * `?tab=list`.
 */
export function Boards({ tab, onTab }: BoardsProps) {
  const { t } = useTranslation();
  const { t: sources } = useTranslation("sources");
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
  const owner = boards.data?.can_manage === true;
  if (tab === "list") {
    return (
      <Page testId="boards" title={t("boards.title")} wide>
        <BoardsPage
          tab="boards"
          onTab={(next: BoardsTab) => {
            onTab(next === "scanned" ? "scanned" : "list");
          }}
        />
      </Page>
    );
  }
  const view =
    tab === undefined ? (owner ? "all" : "mine") : tab === "all" && !owner ? "mine" : tab;
  const back = (
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
  );
  const views = (
    <ViewSwitch
      label={sources("sites.views.label")}
      options={[
        { value: "mine" as const, label: sources("sites.views.mine"), testId: "boards-view-mine" },
        ...(owner
          ? [
              {
                value: "all" as const,
                label: sources("sites.views.all"),
                testId: "boards-view-all",
              },
            ]
          : []),
        {
          value: "scanned" as const,
          label: sources("sites.views.scanned"),
          testId: "boards-view-scanned",
        },
      ]}
      value={view}
      onChange={onTab}
    />
  );
  const head = (
    <Stack gap="sm">
      {back}
      {views}
    </Stack>
  );
  return (
    <Page
      testId="boards"
      title={view === "all" ? t("boards.adminTitle") : t("boards.title")}
      eyebrow={t("boards.eyebrow")}
      wide
    >
      {view === "all" ? (
        <BoardsAdmin back={head} />
      ) : (
        <Stack gap="lg">
          {head}
          {view === "scanned" ? <ScannedView /> : <MySitesPage canManage={owner} />}
        </Stack>
      )}
    </Page>
  );
}
