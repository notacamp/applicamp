import { screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders } from "../../../test/render";

import { InlineEdit, ListField, withItem } from ".";

function Roles({ max = 3, onItems }: { max?: number; onItems?: (items: string[]) => void }) {
  const [items, setItems] = useState<string[]>(["Designer"]);
  const [draft, setDraft] = useState("");
  return (
    <ListField
      label="Role"
      hint="Up to 3."
      addHint="adds another"
      items={items}
      onItems={(next) => {
        setItems(next);
        onItems?.(next);
      }}
      draft={draft}
      onDraft={setDraft}
      max={max}
      maxLength={20}
      addLabel="Add"
      removeLabel={(item) => `Remove ${item}`}
      testId="roles"
    />
  );
}

function Editable({ onSave }: { onSave: () => void }) {
  const [editing, setEditing] = useState(false);
  return (
    <InlineEdit
      label="Roles"
      summary={editing ? undefined : "Designer"}
      placeholder="Add roles"
      editing={editing}
      onEdit={() => {
        setEditing(true);
      }}
      onSave={() => {
        onSave();
        setEditing(false);
      }}
      onCancel={() => {
        setEditing(false);
      }}
      saveLabel="Save"
      cancelLabel="Cancel"
      testId="roles-edit"
    >
      <input data-testid="editor-input" aria-label="Editor" />
    </InlineEdit>
  );
}

describe("the list and in-place editing compositions", () => {
  it("adds an item to a list: trimmed, once, and never past the limit", () => {
    expect(withItem(["a"], "  b   c ", 3)).toEqual(["a", "b c"]);
    expect(withItem(["a"], "A", 3)).toEqual(["a"]);
    expect(withItem(["a"], "   ", 3)).toEqual(["a"]);
    expect(withItem(["a", "b"], "c", 2)).toEqual(["a", "b"]);
  });

  it("builds a list one item at a time: Add or Shift+Enter, a tag with a remove button", async () => {
    const user = userEvent.setup();
    const onItems = vi.fn<(items: string[]) => void>();
    renderWithProviders(<Roles onItems={onItems} />);

    expect(screen.getByTestId("roles-item-0")).toHaveTextContent("Designer");
    expect(screen.getByTestId("roles-add")).toBeDisabled();
    await user.type(screen.getByTestId("roles-input"), "Writer");
    await user.click(screen.getByTestId("roles-add"));
    expect(onItems).toHaveBeenLastCalledWith(["Designer", "Writer"]);

    await user.type(screen.getByTestId("roles-input"), "Editor");
    await user.keyboard("{Shift>}{Enter}{/Shift}");
    expect(onItems).toHaveBeenLastCalledWith(["Designer", "Writer", "Editor"]);
    expect(screen.getByTestId("roles-input")).toBeDisabled();
    expect(screen.getByTestId("roles-remove-1")).toHaveAccessibleName("Remove Writer");

    await user.click(screen.getByTestId("roles-remove-1"));
    expect(onItems).toHaveBeenLastCalledWith(["Designer", "Editor"]);
    expect(screen.getByTestId("roles-input")).toBeEnabled();
  });

  it("reads as text, opens its editor on a click, keeps with Enter and leaves with Esc", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    renderWithProviders(<Editable onSave={onSave} />);

    expect(screen.getByTestId("roles-edit")).toHaveAccessibleName("Roles: Designer");
    await user.click(screen.getByTestId("roles-edit"));
    await user.type(screen.getByTestId("editor-input"), "x{Enter}");
    expect(onSave).toHaveBeenCalledTimes(1);

    await user.click(screen.getByTestId("roles-edit"));
    await user.keyboard("{Escape}");
    expect(screen.getByTestId("roles-edit")).toHaveTextContent("Designer");
    expect(onSave).toHaveBeenCalledTimes(1);

    await user.click(screen.getByTestId("roles-edit"));
    await user.click(screen.getByTestId("roles-edit-cancel"));
    expect(screen.getByTestId("roles-edit")).toBeInTheDocument();
    await user.click(screen.getByTestId("roles-edit"));
    await user.click(screen.getByTestId("roles-edit-save"));
    expect(onSave).toHaveBeenCalledTimes(2);
  });

  it("shows the placeholder, muted, when there is no value", () => {
    renderWithProviders(
      <InlineEdit
        label="Roles"
        summary={undefined}
        placeholder="Add roles"
        editing={false}
        onEdit={vi.fn()}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        saveLabel="Save"
        cancelLabel="Cancel"
        testId="empty-edit"
      >
        {null}
      </InlineEdit>,
    );
    expect(screen.getByTestId("empty-edit")).toHaveTextContent("Add roles");
    expect(screen.getByTestId("empty-edit")).toHaveAttribute("data-empty", "true");
  });
});
