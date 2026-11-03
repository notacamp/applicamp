import { useState, type ReactNode, type SubmitEvent } from "react";
import { useTranslation } from "react-i18next";

import {
  Button,
  Cluster,
  Label,
  Placeholder,
  RowItem,
  RowList,
  Section,
  Stack,
  Tagged,
  Text,
  TextField,
} from "~/lib/ui";

import {
  companyErrorOf,
  failureText,
  useAddCompany,
  useCompanyAction,
  type CompanyList,
  type WatchedCompany,
} from "../../api/companies";
import { formatWhen, hostOf } from "../../api/format";

/** The companies the person watches, and "Watch a company yourself". */
export function WatchingList({ list }: { list: CompanyList }) {
  const { t } = useTranslation("sources");
  const full = list.watching.length >= list.limit;
  return (
    <Stack gap="lg">
      {list.watching.length === 0 ? (
        <Placeholder
          testId="companies-watching-empty"
          icon="stay"
          title={t("companies.watching.emptyTitle")}
        >
          {t("companies.watching.emptyBody")}
        </Placeholder>
      ) : (
        <RowList label={t("companies.tabs.watching")} testId="companies-watching">
          {list.watching.map((company) => (
            <WatchingRow key={company.id} company={company} />
          ))}
        </RowList>
      )}
      <WatchYourself max={list.limit} full={full} />
    </Stack>
  );
}

/** One watched company: its board, its open jobs, its last check, and Stop watching. */
function WatchingRow({ company }: { company: WatchedCompany }) {
  const { t, i18n } = useTranslation("sources");
  const stop = useCompanyAction();
  const id = company.id;
  const meta: ReactNode[] = [];
  if (company.finding) {
    meta.push(
      <Tagged key="finding" testId={`watching-finding-${id}`}>
        <Label hue="blue">{t("companies.watching.finding")}</Label>
      </Tagged>,
    );
  } else if (company.needs_board) {
    meta.push(
      <Tagged key="nofound" testId={`watching-nofound-${id}`}>
        <Label hue="orange">{t("companies.watching.notFound")}</Label>
      </Tagged>,
    );
  } else {
    meta.push(
      <Tagged key="board" testId={`watching-board-${id}`}>
        <Label hue="green">{t(`companies.board.${company.board}`)}</Label>
      </Tagged>,
    );
    if (company.open_jobs !== undefined && company.open_jobs !== null) {
      meta.push(
        <Tagged key="open" testId={`watching-open-${id}`}>
          <Label hue="gray">{t("companies.watching.open", { count: company.open_jobs })}</Label>
        </Tagged>,
      );
    }
    if (company.last_error !== undefined && company.last_error !== null) {
      meta.push(
        <Tagged key="failed" testId={`watching-failed-${id}`}>
          <Label hue="red">
            {t("companies.watching.failed", { reason: failureText(company.last_error, t) })}
          </Label>
        </Tagged>,
      );
    }
    const checked = company.last_checked_at ?? null;
    let checkedText = t("companies.watching.notYet");
    if (checked !== null) {
      const when = formatWhen(checked, i18n.language);
      checkedText = when.today
        ? t("companies.watching.checkedToday", { time: when.text })
        : t("companies.watching.checked", { date: when.text });
    }
    meta.push(
      <Tagged key="checked" testId={`watching-checked-${id}`}>
        <Text tone="secondary" size="sm">
          {checkedText}
        </Text>
      </Tagged>,
    );
  }
  return (
    <RowItem
      testId={`watching-row-${id}`}
      title={company.name}
      subtitle={
        company.needs_board ? (
          <Stack gap="sm">
            {hostOf(company.website) === null ? null : <span>{hostOf(company.website)}</span>}
            <Text size="sm">{t("companies.watching.pasteHint")}</Text>
            <CareersBox id={id} />
          </Stack>
        ) : (
          hostOf(company.website)
        )
      }
      meta={<Cluster>{meta}</Cluster>}
      actions={
        <Button
          variant="link"
          disabled={stop.isPending}
          onClick={() => {
            stop.mutate({ kind: "stop", id });
          }}
          data-testid={`watching-stop-${id}`}
        >
          {t("companies.watching.stop")}
        </Button>
      }
    />
  );
}

/** A watch with no board: paste the careers page, and it is looked at the same way. */
function CareersBox({ id }: { id: string }) {
  const { t } = useTranslation("sources");
  const action = useCompanyAction();
  const [url, setUrl] = useState("");
  const submit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = url.trim();
    if (value === "") {
      return;
    }
    action.mutate(
      { kind: "careers", id, url: value },
      {
        onSuccess: () => {
          setUrl("");
        },
      },
    );
  };
  const error = action.isError
    ? t(`companies.errors.${companyErrorOf(action.error)}`, { max: "20" })
    : undefined;
  return (
    <form onSubmit={submit} noValidate>
      <Cluster>
        <TextField
          label={t("companies.add.label")}
          hideLabel
          type="url"
          inputMode="url"
          autoComplete="off"
          placeholder={t("companies.add.placeholder")}
          value={url}
          error={error}
          onChange={(event) => {
            setUrl(event.target.value);
          }}
          data-testid={`watching-careers-${id}`}
        />
        <Button
          type="submit"
          variant="secondary"
          disabled={action.isPending}
          data-testid={`watching-careers-go-${id}`}
        >
          {t("companies.watching.lookThere")}
        </Button>
      </Cluster>
    </form>
  );
}

/** "Watch a company yourself": paste a careers page, and the same detection runs. */
function WatchYourself({ max, full }: { max: number; full: boolean }) {
  const { t } = useTranslation("sources");
  const add = useAddCompany();
  const [url, setUrl] = useState("");
  const submit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = url.trim();
    if (value === "") {
      return;
    }
    add.mutate(value, {
      onSuccess: () => {
        setUrl("");
      },
    });
  };
  const error = add.isError
    ? t(`companies.errors.${companyErrorOf(add.error)}`, { max: String(max) })
    : undefined;
  return (
    <Section
      title={t("companies.add.title")}
      description={t("companies.add.body")}
      testId="companies-add"
    >
      <form onSubmit={submit} noValidate>
        <Stack gap="sm">
          <Cluster>
            <TextField
              label={t("companies.add.label")}
              type="url"
              inputMode="url"
              autoComplete="off"
              placeholder={t("companies.add.placeholder")}
              value={url}
              error={error}
              onChange={(event) => {
                setUrl(event.target.value);
              }}
              data-testid="companies-add-url"
            />
            <Button
              type="submit"
              variant="secondary"
              icon="plus"
              disabled={add.isPending}
              data-testid="companies-add-go"
            >
              {t("companies.add.go")}
            </Button>
          </Cluster>
          <Text tone="secondary" size="sm">
            {full
              ? t("companies.add.full", { max: String(max) })
              : t("companies.add.limit", { max: String(max) })}
          </Text>
        </Stack>
      </form>
    </Section>
  );
}
