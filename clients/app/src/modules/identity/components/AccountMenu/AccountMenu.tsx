import { useTranslation } from "react-i18next";

import { LANGUAGE_NAMES, SUPPORTED_LOCALES, isLocale } from "~/lib/i18n";
import { writePref } from "~/lib/storage/prefs";
import {
  AccountButton,
  Button,
  Popover,
  PopoverSection,
  Segmented,
  Select,
  Tagged,
  Text,
  THEME_MODES,
  useTheme,
} from "~/lib/ui";

import { useMe } from "../../api/me";
import { useSignOut } from "../../api/sign-out";

export interface AccountMenuProps {
  /** Just the avatar, hanging from the phone header. */
  compact?: boolean | undefined;
  /** For tests: `data-testid` of the button that opens the menu. */
  testId: string;
}

/** Who is signed in, their theme and language, and Sign out. */
export function AccountMenu({ compact = false, testId }: AccountMenuProps) {
  const { t, i18n } = useTranslation("identity");
  const me = useMe();
  const theme = useTheme();
  const signOut = useSignOut();
  const email = me.data?.email;
  const name = me.data?.name ?? email ?? t("account.label");
  const language = isLocale(i18n.language) ? i18n.language : "en";

  return (
    <Popover
      label={t("account.label")}
      placement={compact ? "bottom-end" : "top-start"}
      renderTrigger={(props) => (
        <AccountButton
          {...props}
          compact={compact}
          label={t("account.label")}
          name={name}
          email={email}
          data-testid={testId}
        />
      )}
    >
      <PopoverSection>
        <Text weight="semibold" truncate>
          {name}
        </Text>
        {email === undefined || email === name ? null : (
          <Text tone="tertiary" size="sm" truncate>
            {email}
          </Text>
        )}
      </PopoverSection>
      <PopoverSection title={t("account.theme")}>
        <Tagged testId="theme-choice">
          <Segmented
            label={t("account.theme")}
            value={theme.theme}
            onChange={theme.setTheme}
            options={THEME_MODES.map((mode) => ({
              value: mode,
              label: t(`account.themes.${mode}`),
            }))}
          />
        </Tagged>
      </PopoverSection>
      <PopoverSection>
        <Select
          label={t("account.language")}
          value={language}
          data-testid="language-choice"
          options={SUPPORTED_LOCALES.map((locale) => ({
            value: locale,
            label: LANGUAGE_NAMES[locale],
          }))}
          onChange={(locale) => {
            writePref("locale", locale);
            void i18n.changeLanguage(locale);
          }}
        />
      </PopoverSection>
      <PopoverSection>
        {signOut.isError ? (
          <Tagged testId="sign-out-failed">
            <Text tone="danger" size="sm">
              {t("account.signOutFailed")}
            </Text>
          </Tagged>
        ) : null}
        <Button
          variant="ghost"
          icon="sign-out"
          disabled={signOut.isPending}
          data-testid="sign-out"
          onClick={() => {
            signOut.mutate();
          }}
        >
          {signOut.isPending ? t("account.signingOut") : t("account.signOut")}
        </Button>
      </PopoverSection>
    </Popover>
  );
}
