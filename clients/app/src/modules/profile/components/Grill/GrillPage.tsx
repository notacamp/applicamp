import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { problemCode } from "~/lib/api";
import {
  Banner,
  Button,
  ChoiceNav,
  Page,
  Placeholder,
  Tagged,
  Text,
  Workbench,
  type ChoiceNavItem,
} from "~/lib/ui";

import { describeRole, useGrillRole, useGrillRoles, type GrillRoleItem } from "../../api/grill";
import { CoverageCard } from "./CoverageCard";
import { GrillChat } from "./GrillChat";

export interface GrillPageProps {
  /** The role open (`/profile/grill/$position`); none shows the list alone. */
  position: number | undefined;
  /** Open a role. */
  onPosition: (position: number) => void;
  /** Back to Profile & goals. */
  onBack: () => void;
  /** The month's AI spend line. */
  spend?: ReactNode;
}

/** `/profile/grill`: the roles, and the interview of the one open. */
export function GrillPage({ position, onPosition, onBack, spend }: GrillPageProps) {
  const { t } = useTranslation("profile");
  const roles = useGrillRoles();
  const room = useGrillRole(position);

  const years = (from: string, to: string | undefined) =>
    to === undefined ? t("grill.yearsNow", { from }) : t("grill.years", { from, to });
  const metaOf = (item: GrillRoleItem) =>
    [
      describeRole(item.role, years).meta,
      t(`grill.status.${item.status}`),
      item.status === "not_started" ? "" : t("grill.coverage.count", { n: item.covered }),
    ]
      .filter((part) => part !== "")
      .join(" · ");

  const body = (): ReactNode => {
    if (roles.isPending) {
      return (
        <Tagged testId="grill-loading">
          <Text tone="secondary">{t("grill.loading")}</Text>
        </Tagged>
      );
    }
    if (roles.isError) {
      return (
        <Tagged testId="grill-load-error">
          <Banner
            tone="danger"
            icon="warning"
            title={t("grill.loadFailed")}
            action={
              <Button
                variant="secondary"
                onClick={() => {
                  void roles.refetch();
                }}
              >
                {t("retry")}
              </Button>
            }
          />
        </Tagged>
      );
    }
    if (roles.data.roles.length === 0) {
      return (
        <Placeholder
          testId="grill-no-roles"
          icon="tasks"
          title={t("grill.noRolesTitle")}
          action={
            <Button variant="primary" data-testid="grill-back" onClick={onBack}>
              {t("grill.back")}
            </Button>
          }
        >
          {t("grill.noRolesLead")}
        </Placeholder>
      );
    }
    const items: ChoiceNavItem[] = roles.data.roles.map((item) => ({
      id: String(item.position),
      title: describeRole(item.role, years).title,
      meta: metaOf(item),
      current: item.position === position,
      testId: `grill-role-${String(item.position)}`,
    }));
    const view = room.data;
    const title = view === undefined ? "" : describeRole(view.role, years).title;
    const company = view?.role.company ?? undefined;
    const roleName = [title, company]
      .filter((part): part is string => part !== undefined && part !== "")
      .join(" · ");

    let main: ReactNode;
    if (position === undefined) {
      main = (
        <Placeholder testId="grill-pick" icon="tasks" title={t("grill.pickTitle")}>
          {t("grill.pickLead")}
        </Placeholder>
      );
    } else if (room.isError) {
      main = (
        <Tagged testId="grill-role-error">
          <Banner
            tone="danger"
            icon="warning"
            title={t(
              problemCode(room.error) === "not_found"
                ? "grill.errors.not_found"
                : "grill.roleLoadFailed",
            )}
          />
        </Tagged>
      );
    } else if (view === undefined) {
      main = (
        <Tagged testId="grill-role-loading">
          <Text tone="secondary">{t("grill.loading")}</Text>
        </Tagged>
      );
    } else {
      main = <GrillChat key={view.position} view={view} role={roleName} spend={spend} />;
    }

    return (
      <Workbench
        testId="grill-workbench"
        nav={
          <ChoiceNav
            label={t("grill.rolesLabel")}
            items={items}
            testId="grill-roles"
            onSelect={(id) => {
              onPosition(Number(id));
            }}
          />
        }
        aside={view === undefined ? undefined : <CoverageCard view={view} title={roleName} />}
      >
        {main}
      </Workbench>
    );
  };

  return (
    <Page testId="grill" title={t("grill.title")} lead={t("grill.lead")} wide>
      {body()}
      {position === undefined ? spend : null}
    </Page>
  );
}
