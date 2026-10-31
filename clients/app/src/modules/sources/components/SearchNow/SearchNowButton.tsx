import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useEffectEvent, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Dialog, Tagged } from "~/lib/ui";

import {
  estimateQuery,
  formatEuros,
  formatLimit,
  searchErrorOf,
  useStartSearch,
  type SearchEstimate,
} from "../../api/searches";

export interface SearchNowButtonProps {
  /** For tests: `data-testid` of the button. */
  testId?: string | undefined;
  variant?: "primary" | "secondary" | undefined;
  /** Start at once, as if pressed (⌘K's "Search now" landed here); then `onAutoStarted`. */
  autoStart?: boolean | undefined;
  onAutoStarted?: (() => void) | undefined;
}

/**
 * Search now: asks what the search would cost first, and when that is more
 * than a few cents (the estimate's `confirm`), asks before spending it. Says
 * why when a search cannot start (limits, no model, a search running).
 */
export function SearchNowButton({
  testId = "search-now",
  variant = "primary",
  autoStart = false,
  onAutoStarted,
}: SearchNowButtonProps) {
  const { t, i18n } = useTranslation("sources");
  const queryClient = useQueryClient();
  const start = useStartSearch();
  const [confirming, setConfirming] = useState<SearchEstimate | null>(null);
  const [checking, setChecking] = useState(false);
  // One request at a time, even when an effect runs twice.
  const busy = useRef(false);

  const run = () => {
    start.mutate(undefined, {
      onSettled: () => {
        busy.current = false;
      },
    });
  };

  const request = () => {
    if (busy.current) {
      return;
    }
    busy.current = true;
    start.reset();
    setChecking(true);
    queryClient
      .query({ ...estimateQuery, staleTime: 0 })
      .then(
        (estimate) => {
          if (estimate.confirm) {
            setConfirming(estimate);
          } else {
            run();
          }
        },
        // No estimate: the server still holds every limit.
        () => {
          run();
        },
      )
      .finally(() => {
        setChecking(false);
      });
  };

  const startOnArrival = useEffectEvent(() => {
    request();
    onAutoStarted?.();
  });
  useEffect(() => {
    if (autoStart) {
      // After the render that asked for it; `busy` keeps it to one request.
      queueMicrotask(() => {
        startOnArrival();
      });
    }
  }, [autoStart]);

  const cancel = () => {
    setConfirming(null);
    busy.current = false;
  };

  const error = start.isError ? searchErrorOf(start.error) : null;
  const language = i18n.language;
  return (
    <>
      <Button
        variant={variant}
        icon="search"
        shortcut="sources.start"
        disabled={checking || start.isPending}
        data-testid={testId}
        onClick={request}
      >
        {checking || start.isPending ? t("starting") : t("searchNow")}
      </Button>
      <Dialog
        open={confirming !== null}
        onClose={cancel}
        visor={null}
        title={t("confirm.title")}
        description={
          confirming === null
            ? undefined
            : t("confirm.body", {
                cost: formatEuros(confirming.cost_eur, language),
                jobs: String(confirming.jobs),
              })
        }
        footer={
          <>
            <Button variant="secondary" data-testid="search-confirm-cancel" onClick={cancel}>
              {t("confirm.cancel")}
            </Button>
            <Button
              variant="primary"
              data-testid="search-confirm"
              onClick={() => {
                setConfirming(null);
                run();
              }}
            >
              {t("confirm.go")}
            </Button>
          </>
        }
      >
        {confirming === null ? null : (
          <Tagged testId="search-confirm-spend">
            {confirming.spend.month_limit_eur === null ||
            confirming.spend.month_limit_eur === undefined
              ? t("spend.monthNoLimit", {
                  spent: formatEuros(confirming.spend.month_eur, language),
                })
              : t("spend.month", {
                  spent: formatEuros(confirming.spend.month_eur, language),
                  limit: formatLimit(confirming.spend.month_limit_eur, language),
                })}
          </Tagged>
        )}
      </Dialog>
      {error === null ? null : (
        <Tagged testId="search-error">
          <Banner tone="danger" icon="warning" role="alert" title={t(`errors.${error}`)} />
        </Tagged>
      )}
    </>
  );
}
