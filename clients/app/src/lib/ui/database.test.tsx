import { act, render, renderHook, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ShortcutProvider } from "~/lib/shortcuts";

import {
  Board,
  BoardCard,
  BoardColumn,
  Bullets,
  CardList,
  DataTable,
  Label,
  MatchBar,
  Page,
  SidePeek,
  useNarrowScreen,
  ViewSwitch,
} from ".";

interface Row {
  id: string;
  role: string;
  company: string;
}

const ROWS: Row[] = [
  { id: "a", role: "Staff Engineer", company: "Example Co" },
  { id: "b", role: "Founding Engineer", company: "Sample Labs" },
];

describe("the database compositions", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("draws a table whose rows open, and follows the cursor with the focus", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn<(row: Row) => void>();
    const { rerender } = render(
      <DataTable
        label="All"
        columns={[
          { id: "role", header: "Role", cell: (row: Row) => row.role },
          { id: "company", header: "Company", cell: (row: Row) => row.company, nowrap: true },
        ]}
        rows={ROWS}
        rowKey={(row) => row.id}
        selected={null}
        onOpen={onOpen}
        rowTestId="row"
        openTestId="open"
        testId="table"
      />,
    );
    expect(screen.getByTestId("table")).toHaveTextContent("RoleCompany");
    expect(screen.getAllByTestId("row")).toHaveLength(2);
    const [, second] = screen.getAllByTestId("open");
    if (second === undefined) {
      throw new TypeError("two rows");
    }
    await user.click(second);
    expect(onOpen).toHaveBeenCalledWith(ROWS[1]);

    rerender(
      <DataTable
        label="All"
        columns={[{ id: "role", header: "Role", cell: (row: Row) => row.role }]}
        rows={ROWS}
        rowKey={(row) => row.id}
        selected="b"
        onOpen={onOpen}
        rowTestId="row"
        openTestId="open"
      />,
    );
    expect(screen.getAllByTestId("row")[1]).toHaveAttribute("data-selected");
    expect(document.activeElement).toBe(screen.getAllByTestId("open")[1]);
  });

  it("draws a board of columns and cards, and a list of cards", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    render(
      <>
        <Board label="By stage" testId="board">
          <BoardColumn
            title={<Label hue="blue">Shortlisted</Label>}
            count={1}
            empty="Nothing here"
            testId="col"
          >
            <BoardCard
              title="Staff Engineer"
              subtitle="Example Co"
              meta={<MatchBar value={82} label="Match" />}
              selected
              onOpen={onOpen}
              testId="card"
            />
          </BoardColumn>
          <BoardColumn title="Offer" count={0} empty="Nothing here" testId="empty-col" />
        </Board>
        <CardList label="All" testId="cards">
          <BoardCard title="Founding Engineer" onOpen={onOpen} testId="list-card" />
        </CardList>
      </>,
    );
    expect(screen.getByTestId("col")).toHaveTextContent("Shortlisted1");
    expect(screen.getByTestId("card")).toHaveTextContent("Staff EngineerExample CoMatch 82");
    expect(screen.getByTestId("card")).toHaveAttribute("data-selected");
    expect(screen.getByTestId("empty-col")).toHaveTextContent("Nothing here");
    await user.click(screen.getByTestId("card"));
    await user.click(screen.getByTestId("list-card"));
    expect(onOpen).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId("cards").querySelectorAll("li")).toHaveLength(1);
  });

  it("shows a match as a number and a bar, or a dash when there is none", () => {
    render(
      <>
        <span data-testid="scored">
          <MatchBar value={140} label="Match" />
        </span>
        <span data-testid="unscored">
          <MatchBar value={null} label="Match" empty="Not scored" />
        </span>
        <span data-testid="plain">
          <MatchBar value={undefined} label="Match" />
        </span>
      </>,
    );
    expect(screen.getByTestId("scored")).toHaveTextContent("Match 100");
    expect(screen.getByTestId("unscored")).toHaveTextContent("Not scored");
    expect(screen.getByTestId("plain")).toHaveTextContent("–");
  });

  it("opens a side peek with the focus on Close, and shuts it with Escape", async () => {
    const user = userEvent.setup();
    function Peek() {
      const [open, setOpen] = useState(true);
      return (
        <SidePeek
          open={open}
          label="Staff Engineer"
          eyebrow="Opportunity"
          closeLabel="Close"
          onClose={() => {
            setOpen(false);
          }}
          testId="peek"
          closeTestId="peek-close"
        >
          <Bullets items={["One reason", "One reason"]} testId="reasons" />
        </SidePeek>
      );
    }
    render(
      <ShortcutProvider>
        <Peek />
      </ShortcutProvider>,
    );
    const peek = screen.getByTestId("peek");
    expect(peek).toHaveAttribute("role", "dialog");
    expect(peek).toHaveTextContent("Opportunity");
    expect(screen.getByTestId("reasons").querySelectorAll("li")).toHaveLength(2);
    expect(document.activeElement).toBe(screen.getByTestId("peek-close"));
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("peek")).toBeNull();
  });

  it("switches views, one pressed at a time", async () => {
    const user = userEvent.setup();
    function Views() {
      const [view, setView] = useState<"table" | "board">("table");
      return (
        <ViewSwitch
          label="View"
          options={[
            { value: "table", label: "All", icon: "bullets", testId: "table" },
            { value: "board", label: "By stage", testId: "board" },
          ]}
          value={view}
          onChange={setView}
        />
      );
    }
    render(<Views />);
    expect(screen.getByTestId("table")).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByTestId("board"));
    expect(screen.getByTestId("board")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("table")).toHaveAttribute("aria-pressed", "false");
  });

  it("widens a database page", () => {
    render(<Page title="Pipeline" testId="wide" wide />);
    expect(screen.getByTestId("wide").firstElementChild).toHaveAttribute("data-wide");
  });

  it("knows a phone's screen and follows it", () => {
    let listener: (() => void) | undefined;
    let matches = true;
    vi.stubGlobal("matchMedia", (query: string) => ({
      get matches() {
        return matches;
      },
      media: query,
      addEventListener: (_type: string, callback: () => void) => {
        listener = callback;
      },
      removeEventListener: vi.fn(),
    }));
    const { result } = renderHook(() => useNarrowScreen());
    expect(result.current).toBe(true);
    matches = false;
    act(() => {
      listener?.();
    });
    expect(result.current).toBe(false);
  });
});
