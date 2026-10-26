import { render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { Cluster, FileField, InlineText, Property, PropertyList, Section, Stack } from ".";

function EditableHeadline({ onSave }: { onSave: (value: string) => void }) {
  const [value, setValue] = useState<string | undefined>("Engineer");
  return (
    <InlineText
      label="Headline"
      value={value}
      placeholder="Add a headline"
      saveLabel="Save"
      cancelLabel="Cancel"
      testId="headline"
      onSave={(next) => {
        onSave(next);
        setValue(next === "" ? undefined : next);
      }}
    />
  );
}

describe("the property and input compositions", () => {
  it("lays properties out as labelled values in sections", () => {
    render(
      <Stack gap="lg" testId="stack">
        <Section
          title="Experience"
          description="Your positions."
          action={<button type="button">Add</button>}
          testId="section"
        >
          <PropertyList testId="props">
            <Property label="Headline" testId="headline-value">
              Engineer
            </Property>
          </PropertyList>
          <Cluster>
            <span>a</span>
          </Cluster>
        </Section>
        <Section title="Bare" />
      </Stack>,
    );
    const section = screen.getByTestId("section");
    expect(section.querySelector("h2")).toHaveTextContent("Experience");
    expect(section).toHaveTextContent("Your positions.");
    expect(screen.getByTestId("props").tagName).toBe("DL");
    expect(screen.getByTestId("headline-value").tagName).toBe("DD");
    expect(screen.getByTestId("stack")).toHaveAttribute("data-gap", "lg");
  });

  it("edits a value in place: Enter keeps it, Escape leaves it", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn<(value: string) => void>();
    render(<EditableHeadline onSave={onSave} />);

    await user.click(screen.getByTestId("headline"));
    const input = screen.getByTestId("headline-input");
    expect(input).toHaveFocus();
    await user.clear(input);
    await user.type(input, "  Staff engineer {Enter}");
    expect(onSave).toHaveBeenLastCalledWith("Staff engineer");
    expect(screen.getByTestId("headline")).toHaveTextContent("Staff engineer");

    await user.click(screen.getByTestId("headline"));
    await user.type(screen.getByTestId("headline-input"), " and more{Escape}");
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("headline")).toHaveTextContent("Staff engineer");

    await user.click(screen.getByTestId("headline"));
    await user.clear(screen.getByTestId("headline-input"));
    await user.click(screen.getByTestId("headline-save"));
    expect(onSave).toHaveBeenLastCalledWith("");
    expect(screen.getByTestId("headline")).toHaveTextContent("Add a headline");
    expect(screen.getByTestId("headline")).toHaveAttribute("data-empty", "true");

    await user.click(screen.getByTestId("headline"));
    await user.click(screen.getByTestId("headline-cancel"));
    expect(screen.getByTestId("headline")).toBeInTheDocument();
  });

  it("edits a paragraph with ⌘↵", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn<(value: string) => void>();
    render(
      <InlineText
        multiline
        label="Summary"
        value={undefined}
        placeholder="Add a summary"
        saveLabel="Save"
        cancelLabel="Cancel"
        counter={(left) => `${String(left)} left`}
        maxLength={50}
        testId="summary"
        onSave={onSave}
      />,
    );
    await user.click(screen.getByTestId("summary"));
    await user.keyboard("Builds calm backends.");
    await user.click(screen.getByTestId("summary-save"));
    expect(onSave).toHaveBeenCalledWith("Builds calm backends.");
  });

  it("picks a file and shows its name", async () => {
    const user = userEvent.setup();
    function Picker() {
      const [file, setFile] = useState<File | null>(null);
      return (
        <FileField
          label="Your LinkedIn export"
          hint="Up to 20 MB."
          accept=".zip"
          file={file}
          chooseLabel="Choose a file"
          testId="file"
          onChange={setFile}
        />
      );
    }
    render(<Picker />);
    const input = screen.getByTestId("file");
    expect(input.closest("label")).toHaveTextContent("Choose a file");
    await user.upload(input, new File(["PK"], "export.zip", { type: "application/zip" }));
    expect(input.closest("label")).toHaveTextContent("export.zip");
    expect(input).toHaveAccessibleDescription("Up to 20 MB.");
  });
});
