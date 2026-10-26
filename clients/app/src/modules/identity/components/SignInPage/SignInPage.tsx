import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { AppBrand, Banner, Button, ButtonLink, FocusPage, Tagged, Text } from "~/lib/ui";

import { PROVIDER_NAMES, signInUrl, useProviders } from "../../api/providers";

/** Why the last sign-in did not finish, as the server's BFF says (`?error=`). */
export const SIGN_IN_ERRORS = ["sign_in_failed", "provider_unavailable"] as const;
export type SignInError = (typeof SIGN_IN_ERRORS)[number];

export interface SignInPageProps {
  /** The last sign-in failed: say why, above the ways in. */
  error?: SignInError | undefined;
}

/**
 * `/sign-in`: one button per way in the server offers. Each starts the
 * sign-in at the server (a full navigation to `/bff/sign-in/{provider}`),
 * which comes back signed in to Today, to `/not-invited`, or here with an
 * error.
 */
export function SignInPage({ error }: SignInPageProps) {
  const { t } = useTranslation("identity");
  const { t: tShell } = useTranslation();
  const providers = useProviders();

  let ways: ReactNode;
  if (providers.isPending) {
    ways = (
      <Tagged testId="sign-in-loading">
        <Text tone="tertiary">{t("signIn.loading")}</Text>
      </Tagged>
    );
  } else if (providers.isError) {
    ways = (
      <Tagged testId="sign-in-unreachable">
        <Banner
          tone="danger"
          icon="offline"
          title={t("signIn.unreachableTitle")}
          action={
            <Button
              variant="secondary"
              data-testid="sign-in-retry"
              onClick={() => {
                void providers.refetch();
              }}
            >
              {t("signIn.retry")}
            </Button>
          }
        >
          {t("signIn.unreachableBody")}
        </Banner>
      </Tagged>
    );
  } else if (providers.data.length === 0) {
    ways = (
      <Tagged testId="sign-in-none">
        <Banner title={t("signIn.noneTitle")}>{t("signIn.noneBody")}</Banner>
      </Tagged>
    );
  } else {
    ways = providers.data.map((provider, index) => (
      <ButtonLink
        key={provider}
        variant={index === 0 ? "primary" : "secondary"}
        size="lg"
        grow
        href={signInUrl(provider)}
        data-testid={`sign-in-${provider}`}
      >
        {t("signIn.with", { provider: PROVIDER_NAMES[provider] })}
      </ButtonLink>
    ));
  }

  return (
    <Tagged testId="sign-in">
      <FocusPage
        top={<AppBrand name={tShell("appName")} />}
        title={t("signIn.title")}
        lead={t("signIn.lead")}
        actions={ways}
        footer={t("signIn.footer")}
      >
        {error === undefined ? null : (
          <Tagged testId="sign-in-error">
            <Banner
              tone="danger"
              icon="warning"
              role="alert"
              title={t(`signIn.errors.${error}.title`)}
            >
              {t(`signIn.errors.${error}.body`)}
            </Banner>
          </Tagged>
        )}
      </FocusPage>
    </Tagged>
  );
}
