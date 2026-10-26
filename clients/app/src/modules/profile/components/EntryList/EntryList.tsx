import { useState } from "react";
import { useTranslation } from "react-i18next";

import { formatPartialDate } from "~/lib/i18n";
import {
  Button,
  Cluster,
  IconButton,
  List,
  ListRow,
  Section,
  Stack,
  Tag,
  Tagged,
  TextArea,
  TextField,
} from "~/lib/ui";

import {
  entryOf,
  FIELDS,
  problemsOf,
  textOf,
  valuesOf,
  type Entry,
  type FieldSpec,
  type ListKey,
  type Values,
} from "../../api/sections";

/** The lists shown as tags (short names), not as rows. */
const COMPACT: ReadonlySet<ListKey> = new Set(["skills", "languages"]);

/** How an entry reads in its row: a title and a line under it. */
export function useDescribe() {
  const { t, i18n } = useTranslation("profile");
  const date = (value: string | undefined) =>
    value === undefined ? undefined : formatPartialDate(i18n.language, value);
  const range = (entry: Entry) => {
    const start = date(textOf(entry, "startDate"));
    const end = date(textOf(entry, "endDate"));
    if (start === undefined) {
      return end;
    }
    return t("dates.range", { start, end: end ?? t("dates.now") });
  };
  return (list: ListKey, entry: Entry): { title: string; meta: string } => {
    const parts = (...values: (string | undefined)[]) =>
      values.filter((value): value is string => value !== undefined && value !== "").join(" · ");
    switch (list) {
      case "work": {
        const position = textOf(entry, "position");
        const company = textOf(entry, "name");
        return {
          title: position ?? company ?? "",
          meta: parts(
            position === undefined ? undefined : company,
            textOf(entry, "location"),
            range(entry),
          ),
        };
      }
      case "education": {
        return {
          title: textOf(entry, "institution") ?? "",
          meta: parts(textOf(entry, "studyType"), textOf(entry, "area"), range(entry)),
        };
      }
      case "languages": {
        return { title: textOf(entry, "language") ?? "", meta: parts(textOf(entry, "fluency")) };
      }
      case "certificates": {
        return {
          title: textOf(entry, "name") ?? "",
          meta: parts(textOf(entry, "issuer"), date(textOf(entry, "date"))),
        };
      }
      case "projects": {
        return {
          title: textOf(entry, "name") ?? "",
          meta: parts(range(entry), textOf(entry, "url")),
        };
      }
      default: {
        return { title: textOf(entry, "name") ?? "", meta: parts(textOf(entry, "level")) };
      }
    }
  };
}

interface EntryFormProps {
  list: ListKey;
  initial?: Entry | undefined;
  busy: boolean;
  testId: string;
  onSave: (entry: Entry) => void;
  onCancel: () => void;
  onRemove?: (() => void) | undefined;
}

/** One entry's fields, to add or edit it. */
function EntryForm({ list, initial, busy, testId, onSave, onCancel, onRemove }: EntryFormProps) {
  const { t } = useTranslation("profile");
  const [values, setValues] = useState<Values>(() => valuesOf(list, initial));
  const [checked, setChecked] = useState(false);
  const problems = checked ? problemsOf(list, values) : {};

  const set = (spec: FieldSpec) => (value: string) => {
    setValues((current) => ({ ...current, [spec.key]: value }));
  };
  const save = () => {
    setChecked(true);
    if (Object.keys(problemsOf(list, values)).length === 0) {
      onSave(entryOf(list, values));
    }
  };
  const problemText = (spec: FieldSpec) => {
    const problem = problems[spec.key];
    if (problem === "date") {
      return t("form.dateInvalid");
    }
    return problem === "required" ? t("form.required") : undefined;
  };


  return (
    <Tagged testId={testId}>
      <Stack gap="sm">
        {FIELDS[list].map((spec) => {
          const label = t(`fields.${spec.label}`);
          const value = values[spec.key] ?? "";
          return spec.kind === "long" || spec.kind === "lines" ? (
            <TextArea
              key={spec.key}
              label={label}
              value={value}
              rows={spec.kind === "lines" ? 3 : 4}
              hint={spec.kind === "lines" ? t("form.onePerLine") : undefined}
              disabled={busy}
              data-testid={`${testId}-${spec.key}`}
              onChange={(event) => {
                set(spec)(event.target.value);
              }}
            />
          ) : (
            <TextField
              key={spec.key}
              label={label}
              value={value}
              placeholder={spec.kind === "date" ? t("form.datePlaceholder") : undefined}
              hint={spec.kind === "date" ? t("form.dateHint") : undefined}
              error={problemText(spec)}
              disabled={busy}
              data-testid={`${testId}-${spec.key}`}
              onChange={(event) => {
                set(spec)(event.target.value);
              }}
            />
          );
        })}
        <Cluster>
          <Button variant="primary" disabled={busy} data-testid={`${testId}-save`} onClick={save}>
            {t("form.save")}
          </Button>
          <Button
            variant="ghost"
            disabled={busy}
            data-testid={`${testId}-cancel`}
            onClick={onCancel}
          >
            {t("form.cancel")}
          </Button>
          {onRemove === undefined ? null : (
            <Button
              variant="danger-outline"
              icon="trash"
              disabled={busy}
              data-testid={`${testId}-remove`}
              onClick={onRemove}
            >
              {t("form.remove")}
            </Button>
          )}
        </Cluster>
      </Stack>
    </Tagged>
  );
}

export interface EntryListProps {
  list: ListKey;
  entries: readonly Entry[];
  /** Save the list as it is now. */
  onChange: (entries: Entry[]) => void;
  /** While a save runs. */
  busy: boolean;
}

/**
 * One of the profile's lists, editable: its entries as rows (skills and
 * languages as tags until edited), each edited in place, and Add.
 */
export function EntryList({ list, entries, onChange, busy }: EntryListProps) {
  const { t } = useTranslation("profile");
  const describe = useDescribe();
  // The entry being edited (its index), `entries.length` for a new one.
  const [editing, setEditing] = useState<number | null>(null);
  const [expanded, setExpanded] = useState(false);
  const compact = COMPACT.has(list) && !expanded && editing === null;
  const testId = `section-${list}`;

  const close = () => {
    setEditing(null);
  };
  const saveAt = (index: number) => (entry: Entry) => {
    const next = [...entries];
    next.splice(index, 1, entry);
    onChange(next);
    close();
  };
  const removeAt = (index: number) => () => {
    onChange(entries.filter((_, at) => at !== index));
    close();
  };

  const add = (
    <Button
      variant="ghost"
      icon="plus"
      disabled={busy || editing !== null}
      data-testid={`${testId}-add`}
      onClick={() => {
        setEditing(entries.length);
      }}
    >
      {t(`add.${list}`)}
    </Button>
  );

  return (
    <Section title={t(`sections.${list}`)} action={add} testId={testId}>
      {entries.length === 0 && editing === null ? (
        <Tagged testId={`${testId}-empty`}>
          <Button
            variant="link"
            onClick={() => {
              setEditing(0);
            }}
          >
            {t(`empty.${list}`)}
          </Button>
        </Tagged>
      ) : null}
      {compact && entries.length > 0 ? (
        <Cluster>
          {entries.map((entry, index) => (
            <Tag key={index}>{describe(list, entry).title}</Tag>
          ))}
          <Button
            variant="ghost"
            icon="edit"
            data-testid={`${testId}-edit`}
            onClick={() => {
              setExpanded(true);
            }}
          >
            {t("form.editAll")}
          </Button>
        </Cluster>
      ) : null}
      {!compact && entries.length > 0 ? (
        <List label={t(`sections.${list}`)}>
          {entries.map((entry, index) => {
            const { title, meta } = describe(list, entry);
            return editing === index ? (
              <ListRow key={index} title={title}>
                <EntryForm
                  list={list}
                  initial={entry}
                  busy={busy}
                  testId={`${testId}-form`}
                  onSave={saveAt(index)}
                  onCancel={close}
                  onRemove={removeAt(index)}
                />
              </ListRow>
            ) : (
              <ListRow
                key={index}
                title={<Tagged testId={`${testId}-row`}>{title}</Tagged>}
                meta={meta === "" ? undefined : meta}
                trailing={
                  <IconButton
                    icon="edit"
                    label={t("form.edit", { what: title })}
                    disabled={busy || editing !== null}
                    data-testid={`${testId}-edit-${String(index)}`}
                    onClick={() => {
                      setEditing(index);
                    }}
                  />
                }
              />
            );
          })}
        </List>
      ) : null}
      {editing === entries.length ? (
        <EntryForm
          list={list}
          busy={busy}
          testId={`${testId}-form`}
          onSave={saveAt(entries.length)}
          onCancel={close}
        />
      ) : null}
      {COMPACT.has(list) && expanded && editing === null ? (
        <Cluster>
          <Button
            variant="ghost"
            data-testid={`${testId}-done`}
            onClick={() => {
              setExpanded(false);
            }}
          >
            {t("form.done")}
          </Button>
        </Cluster>
      ) : null}
    </Section>
  );
}
