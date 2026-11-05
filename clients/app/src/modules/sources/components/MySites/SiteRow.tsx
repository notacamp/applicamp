import { useTranslation } from "react-i18next";

import { Button, RowItem, Text } from "~/lib/ui";

import { salaryText, siteLine, type MySite } from "../../api/sites";

export interface SiteRowProps {
  site: MySite;
  /** The person manages the job boards: they can add a site's key. */
  canManage: boolean;
  /** The site is being saved. */
  saving: boolean;
  onToggle: (site: MySite) => void;
  onKey: (site: MySite) => void;
}

/**
 * One site: its name, one plain line on what it offers, its typical salary and a single On or Off
 * button. A site that needs a key has "Add the key" for the owner, and a sentence for anyone else.
 */
export function SiteRow({ site, canManage, saving, onToggle, onKey }: SiteRowProps) {
  const { t, i18n } = useTranslation("sources");
  const salary = salaryText(site, i18n.language, t);
  const state = site.on ? t("sites.on") : t("sites.off");
  let action;
  if (site.needs_key) {
    action = canManage ? (
      <Button
        variant="secondary"
        onClick={() => {
          onKey(site);
        }}
        data-testid="site-key"
      >
        {t("sites.addKey")}
      </Button>
    ) : (
      <Text tone="secondary" size="sm">
        {t("sites.adminAddsKey")}
      </Text>
    );
  } else {
    action = (
      <Button
        variant={site.on ? "primary" : "secondary"}
        aria-pressed={site.on}
        aria-label={t("sites.toggle", { state, name: site.name })}
        disabled={saving}
        onClick={() => {
          onToggle(site);
        }}
        data-testid="site-toggle"
      >
        {state}
      </Button>
    );
  }
  return (
    <RowItem
      title={site.name}
      subtitle={siteLine(site, i18n.language, t)}
      meta={
        salary === null ? undefined : (
          <Text tone="secondary" size="sm">
            {salary}
          </Text>
        )
      }
      actions={action}
      testId="site-row"
    />
  );
}
