import { Navigate } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { errorStatus } from "~/lib/api";
import { AppBrand, Button, FocusPage, Tagged } from "~/lib/ui";

import { useMe } from "../../api/me";

/**
 * Renders the signed-in app only for a signed-in browser: it asks who is
 * signed in once, and a 401 sends the browser to the sign-in page. A server
 * that cannot be reached says so, with a way to try again.
 */
export function SessionGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation("identity");
  const { t: tShell } = useTranslation();
  const me = useMe();

  if (me.isPending) {
    return (
      <Tagged testId="gate-opening">
        <FocusPage top={<AppBrand name={tShell("appName")} />} title={t("gate.opening")} />
      </Tagged>
    );
  }
  if (me.isError) {
    if (errorStatus(me.error) === 401) {
      return <Navigate to="/sign-in" replace />;
    }
    return (
      <Tagged testId="gate-unreachable">
        <FocusPage
          top={<AppBrand name={tShell("appName")} />}
          title={t("gate.unreachableTitle")}
          lead={t("gate.unreachableLead")}
          actions={
            <Button
              variant="primary"
              size="lg"
              data-testid="gate-retry"
              onClick={() => {
                void me.refetch();
              }}
            >
              {t("gate.retry")}
            </Button>
          }
        />
      </Tagged>
    );
  }
  return children;
}
