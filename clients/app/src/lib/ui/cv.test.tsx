import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { renderWithProviders } from "../../../test/render";

import { CvLine, CvLines, Marked, markedParts, Paper, PaperRole, ReviewLayout } from ".";

describe("markedParts", () => {
  it("marks the words that are claims, in any case, and keeps the rest as it was", () => {
    expect(markedParts("Cut runs to 40 minutes with rust", ["40", "Rust"])).toEqual([
      { text: "Cut runs to ", hit: false },
      { text: "40", hit: true },
      { text: " minutes with ", hit: false },
      { text: "rust", hit: true },
    ]);
  });

  it("marks nothing without words, and never part of a longer word", () => {
    expect(markedParts("Go to Google", [])).toEqual([{ text: "Go to Google", hit: false }]);
    expect(markedParts("Go to Google", ["Go"])).toEqual([
      { text: "Go", hit: true },
      { text: " to Google", hit: false },
    ]);
  });
});

describe("the CV compositions", () => {
  it("lays out the aside and the CV, a page with its roles and lines, a flagged line picked out", () => {
    renderWithProviders(
      <ReviewLayout testId="layout" aside={<p>Aside</p>}>
        <Paper label="Your CV" testId="paper">
          <PaperRole title="Engineer · Sample Labs" meta="2017 – 2021" testId="role">
            <CvLines>
              <CvLine testId="plain" mark="As in profile">
                Built ingestion for <Marked text="3 partner feeds" words={["3"]} />
              </CvLine>
              <CvLine testId="bad" flagged detail={<p>Why</p>}>
                Cut runs
              </CvLine>
            </CvLines>
          </PaperRole>
        </Paper>
      </ReviewLayout>,
    );
    expect(screen.getByTestId("layout")).toHaveTextContent("Aside");
    expect(screen.getByTestId("paper")).toHaveAccessibleName("Your CV");
    expect(screen.getByTestId("role")).toHaveTextContent("Engineer · Sample Labs");
    expect(screen.getByTestId("plain")).not.toHaveAttribute("data-flagged");
    expect(screen.getByTestId("bad")).toHaveAttribute("data-flagged");
    expect(screen.getByTestId("bad")).toHaveTextContent("Why");
  });
});
