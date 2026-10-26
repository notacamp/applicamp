import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { AppBrand, AppFrame, AppMark, Page, Placeholder, Tagged } from ".";

describe("the compositions", () => {
  it("draws the mark and the brand", () => {
    render(
      <Tagged testId="brand">
        <AppBrand name="Applicamp" detail="demo@example.com" place="sidebar" />
        <AppMark size="lg" />
      </Tagged>,
    );
    const brand = screen.getByTestId("brand");
    expect(brand).toHaveTextContent("Applicamp");
    expect(brand).toHaveTextContent("demo@example.com");
    expect(brand.querySelector('[data-size="lg"]')).toHaveAttribute("aria-hidden", "true");
  });

  it("lays a page out with its title, eyebrow and lead", () => {
    render(
      <AppFrame>
        <Page testId="page" eyebrow="Sunday" title="Today" lead="What waits on you.">
          <Placeholder
            testId="empty"
            icon="check"
            title="Nothing yet"
            action={<button type="button">Go</button>}
          >
            Soon.
          </Placeholder>
        </Page>
      </AppFrame>,
    );
    const page = screen.getByTestId("page");
    expect(page.tagName).toBe("MAIN");
    expect(page).toHaveTextContent("Sunday");
    expect(page).toHaveTextContent("What waits on you.");
    expect(page.querySelector("h1")).toHaveTextContent("Today");
    const empty = screen.getByTestId("empty");
    expect(empty.querySelector("h2")).toHaveTextContent("Nothing yet");
    expect(empty).toHaveTextContent("Soon.");
  });

  it("leaves out what it is not given", () => {
    render(
      <Page testId="bare" title="Pipeline">
        <Placeholder testId="bare-empty" icon="tasks" title="No opportunities yet" />
      </Page>,
    );
    expect(screen.getByTestId("bare")).toHaveTextContent(/^PipelineNo opportunities yet$/);
    render(<AppBrand name="Applicamp" />);
  });
});
