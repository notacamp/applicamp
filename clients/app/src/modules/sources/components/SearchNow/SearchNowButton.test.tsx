import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { estimate, search } from "../../../../../test/pipeline";
import { renderWithProviders, stubApi } from "../../../../../test/render";

import { SearchNowButton } from "./SearchNowButton";

describe("Search now", () => {
  it("starts a search at once when it costs a few cents", async () => {
    const requests = stubApi({
      "GET /api/v1/searches/estimate": { body: estimate() },
      "POST /api/v1/searches": { status: 202, body: search({ status: "queued" }) },
    });
    const user = userEvent.setup();
    renderWithProviders(<SearchNowButton />);
    await user.click(screen.getByTestId("search-now"));
    await waitFor(() => {
      expect(requests.map(({ route }) => route)).toContain("POST /api/v1/searches");
    });
    const post = requests.find(({ route }) => route === "POST /api/v1/searches");
    expect(post?.headers.get("x-applicamp-csrf")).toBe("1");
    expect(screen.queryByTestId("search-confirm")).toBeNull();
  });

  it("asks first when the search would cost more, and does nothing on Not now", async () => {
    const requests = stubApi({
      "GET /api/v1/searches/estimate": { body: estimate({ cost_eur: 0.07, confirm: true }) },
      "POST /api/v1/searches": { status: 202, body: search({ status: "queued" }) },
    });
    const user = userEvent.setup();
    renderWithProviders(<SearchNowButton />);

    await user.click(screen.getByTestId("search-now"));
    expect(await screen.findByTestId("search-confirm")).toBeInTheDocument();
    expect(document.body).toHaveTextContent("about €0.07 of AI to score about 40 jobs");
    expect(screen.getByTestId("search-confirm-spend")).toHaveTextContent(
      "AI this month: €0.42 of €3",
    );
    await user.click(screen.getByTestId("search-confirm-cancel"));
    await waitFor(() => {
      expect(screen.queryByTestId("search-confirm")).toBeNull();
    });
    expect(requests.map(({ route }) => route)).not.toContain("POST /api/v1/searches");

    await user.click(screen.getByTestId("search-now"));
    await user.click(await screen.findByTestId("search-confirm"));
    await waitFor(() => {
      expect(requests.map(({ route }) => route)).toContain("POST /api/v1/searches");
    });
  });

  it("says why a search cannot start", async () => {
    stubApi({
      "GET /api/v1/searches/estimate": { body: estimate() },
      "POST /api/v1/searches": { status: 429, body: { code: "search_limit_reached" } },
    });
    const user = userEvent.setup();
    renderWithProviders(<SearchNowButton testId="go" />);
    await user.click(screen.getByTestId("go"));
    expect(await screen.findByTestId("search-error")).toHaveTextContent(
      "3 searches a day, one every 15 minutes",
    );
  });

  it("starts anyway when the estimate cannot be read, and says an unknown failure plainly", async () => {
    stubApi({
      "GET /api/v1/searches/estimate": { status: 500, body: { code: "internal_error" } },
      "POST /api/v1/searches": { status: 500, body: { code: "internal_error" } },
    });
    const user = userEvent.setup();
    renderWithProviders(<SearchNowButton />);
    await user.click(screen.getByTestId("search-now"));
    expect(await screen.findByTestId("search-error")).toHaveTextContent(
      "The search couldn’t start.",
    );
  });

  it("starts on arrival when asked to (⌘K from another page)", async () => {
    const requests = stubApi({
      "GET /api/v1/searches/estimate": { body: estimate() },
      "POST /api/v1/searches": { status: 202, body: search({ status: "queued" }) },
    });
    const onAutoStarted = vi.fn();
    renderWithProviders(<SearchNowButton autoStart onAutoStarted={onAutoStarted} />);
    await waitFor(() => {
      expect(onAutoStarted).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(requests.filter(({ route }) => route === "POST /api/v1/searches")).toHaveLength(1);
    });
  });
});
