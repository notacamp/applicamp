import { screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders } from "../../../test/render";

import { EventLine, EventLog, PickList, PickRow, SplitView } from ".";

describe("the list-and-detail compositions", () => {
  it("shows the list or the chosen item on a phone, with the way back above the item", () => {
    const { rerender } = renderWithProviders(
      <SplitView
        listLabel="Runs"
        detailLabel="Run"
        list={<p data-testid="the-list">Runs</p>}
        detail={<p data-testid="the-detail">A run</p>}
        showing="list"
        back={<button data-testid="the-back">Back</button>}
        testId="split"
      />,
    );
    const split = screen.getByTestId("split");
    expect(split).toHaveAttribute("data-showing", "list");
    expect(screen.getByTestId("the-list")).toBeInTheDocument();
    expect(screen.getByTestId("the-back")).toBeInTheDocument();

    rerender(
      <SplitView
        listLabel="Runs"
        detailLabel="Run"
        list={<p data-testid="the-list">Runs</p>}
        detail={<p data-testid="the-detail">A run</p>}
        showing="detail"
        testId="split"
      />,
    );
    expect(screen.getByTestId("split")).toHaveAttribute("data-showing", "detail");
    expect(screen.getByTestId("the-detail")).toHaveTextContent("A run");
    expect(screen.queryByTestId("the-back")).toBeNull();
  });

  it("chooses one row of a pick list, marking the chosen one current", async () => {
    const onSelect = vi.fn();
    const user = userEvent.setup();
    renderWithProviders(
      <PickList label="Runs" testId="picks">
        <PickRow
          title="Check"
          meta="Succeeded"
          leading={<span>•</span>}
          selected
          onSelect={vi.fn()}
          testId="pick"
          data={{ "data-id": "a" }}
        />
        <PickRow title="Search" onSelect={onSelect} testId="pick" data={{ "data-id": "b" }} />
      </PickList>,
    );
    const [first, second] = screen.getAllByTestId("pick");
    expect(first).toHaveAttribute("aria-current", "true");
    expect(first).toHaveTextContent("CheckSucceeded");
    expect(second).not.toHaveAttribute("aria-current");
    expect(second).toHaveAttribute("data-id", "b");
    if (second !== undefined) {
      await user.click(second);
    }
    expect(onSelect).toHaveBeenCalledOnce();
  });

  it("writes a log's lines with their time and kind, machine lines in monospace", () => {
    renderWithProviders(
      <EventLog label="Sandbox log" testId="log">
        <EventLine at="2026-10-31T09:00:00Z" atText="09:00:00" testId="line">
          Read its context.
        </EventLine>
        <EventLine
          at="2026-10-31T09:00:01Z"
          atText="09:00:01"
          badge={<span>Blocked</span>}
          mono
          testId="line"
          data={{ "data-level": "deny" }}
        >
          NET:OPEN [MED] DENIED applicamp-agent -&gt; example.com:443
        </EventLine>
      </EventLog>,
    );
    expect(screen.getByTestId("log")).toHaveAttribute("role", "log");
    const [plain, blocked] = screen.getAllByTestId("line");
    expect(plain).toHaveTextContent("09:00:00Read its context.");
    expect(plain).not.toHaveAttribute("data-mono");
    expect(blocked).toHaveTextContent("Blocked");
    expect(blocked).toHaveAttribute("data-mono");
    expect(blocked).toHaveAttribute("data-level", "deny");
  });
});
