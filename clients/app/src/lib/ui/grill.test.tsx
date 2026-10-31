import { render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { Chat, ChatBubble, ChoiceNav, FactBlock, FactRow, StatusMark, Workbench } from ".";

describe("the Grill me compositions", () => {
  it("lays a list, the work and a card out, and the work alone when there is no card", () => {
    const { rerender } = render(
      <Workbench testId="bench" nav={<p>roles</p>} aside={<p>card</p>}>
        <p>work</p>
      </Workbench>,
    );
    const bench = screen.getByTestId("bench");
    expect(bench).toHaveAttribute("data-aside", "some");
    expect(bench).toHaveTextContent("roles");
    expect(bench).toHaveTextContent("work");
    expect(bench.querySelector("aside")).toHaveTextContent("card");
    rerender(
      <Workbench testId="bench" nav={<p>roles</p>}>
        <p>work</p>
      </Workbench>,
    );
    expect(screen.getByTestId("bench")).toHaveAttribute("data-aside", "none");
    expect(screen.getByTestId("bench").querySelector("aside")).toBeNull();
  });

  it("chooses between things, one open at a time", async () => {
    const onSelect = vi.fn();
    render(
      <ChoiceNav
        label="Roles"
        onSelect={onSelect}
        items={[
          { id: "a", title: "Senior", meta: "Example · now", current: true, testId: "a" },
          { id: "b", title: "Junior", testId: "b" },
        ]}
      />,
    );
    expect(screen.getByTestId("a")).toHaveAttribute("aria-current", "true");
    expect(screen.getByTestId("a")).toHaveTextContent("SeniorExample · now");
    expect(screen.getByTestId("b")).not.toHaveAttribute("aria-current");
    await userEvent.setup().click(screen.getByTestId("b"));
    expect(onSelect).toHaveBeenCalledWith("b");
  });

  it("tells questions, push-backs and answers apart", () => {
    render(
      <Chat label="Interview" testId="chat">
        <ChatBubble kind="question" label="Question" testId="q">
          What did you do?
        </ChatBubble>
        <ChatBubble kind="push" label="Pushing back" testId="p">
          How many?
        </ChatBubble>
        <ChatBubble kind="answer" testId="a">
          Three.
        </ChatBubble>
      </Chat>,
    );
    expect(screen.getByTestId("chat")).toHaveAttribute("role", "log");
    expect(screen.getByTestId("q")).toHaveAttribute("data-kind", "question");
    expect(screen.getByTestId("p")).toHaveTextContent("Pushing back");
    expect(screen.getByTestId("a")).toHaveTextContent("Three.");
    expect(screen.getByTestId("a")).not.toHaveTextContent("Question");
  });

  it("marks how far something is without relying on colour", () => {
    render(
      <>
        <StatusMark state="ok" label="Covered" testId="ok" />
        <StatusMark state="part" label="Partly covered" testId="part" />
        <StatusMark state="none" label="Not yet" testId="none" />
      </>,
    );
    expect(screen.getByTestId("ok")).toHaveTextContent("✓");
    expect(screen.getByTestId("part")).toHaveTextContent("½");
    expect(screen.getByTestId("none")).toHaveTextContent("!");
    expect(screen.getByTestId("part")).toHaveAccessibleName("Partly covered");
  });

  it("shows a fact with the person's words, what can be done and what is under it", () => {
    render(
      <FactBlock title="Facts" footer={<span>Saved when kept</span>} testId="block">
        <FactRow
          text="Took 7 months"
          quoteLabel="Your words:"
          quote="It took 7 months"
          actions={<button type="button">Keep</button>}
          testId="row"
        >
          <p>editor</p>
        </FactRow>
        <FactRow text="Dropped one" quoteLabel="Your words:" quote="x" muted testId="muted" />
      </FactBlock>,
    );
    expect(screen.getByTestId("block")).toHaveAccessibleName("Facts");
    expect(screen.getByTestId("block")).toHaveTextContent("Saved when kept");
    expect(screen.getByTestId("row")).toHaveTextContent("Your words: It took 7 months");
    expect(screen.getByTestId("row")).toHaveTextContent("editor");
    expect(screen.getByTestId("muted")).toHaveAttribute("data-muted");
    expect(screen.getByTestId("row")).not.toHaveAttribute("data-muted");
  });
});
