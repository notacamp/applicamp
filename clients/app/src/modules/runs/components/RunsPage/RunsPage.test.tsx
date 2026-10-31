import { screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";
import {
  FakeEventSource,
  OLDER_RUN_ID,
  PROBE_EVENTS,
  RUN_ID,
  run,
  runList,
  stubEventSource,
} from "../../../../../test/runs";

import { RunsPage } from "./RunsPage";

const LIVE = run({
  status: "running",
  outcome: null,
  finished_at: null,
  duration_ms: 1500,
  deadline: "2026-10-30T09:02:01Z",
});
const OLDER = run({
  id: OLDER_RUN_ID,
  created_at: "2026-10-29T08:00:00Z",
  started_at: "2026-10-29T08:00:01Z",
  outcome: "failed",
  error_kind: "agent_exited",
});

function render(
  routes: Record<string, Stub>,
  { selected }: { selected?: string | undefined } = {},
) {
  const requests = stubApi(routes);
  const onSelect = vi.fn();
  renderWithProviders(<RunsPage selected={selected} onSelect={onSelect} />);
  return { requests, onSelect };
}

describe("The Agents page", () => {
  beforeEach(() => {
    stubEventSource();
  });

  it("says what will appear when there are no runs, and offers a check", async () => {
    render({ "GET /api/v1/runs": { body: runList([]) } });
    const empty = await screen.findByTestId("agents-empty");
    expect(empty).toHaveTextContent("No runs yet");
    expect(empty).toHaveTextContent("Run a check to see one at work.");
    expect(screen.getByTestId("run-check")).toHaveTextContent("Run a check");
    expect(screen.queryByTestId("runs-unavailable")).toBeNull();
    expect(FakeEventSource.opened).toHaveLength(0);
  });

  it("says when agents can't run on this instance, and offers no check", async () => {
    render({
      "GET /api/v1/runs": { body: runList([], { available: false, startable: [] }) },
    });
    expect(await screen.findByTestId("runs-unavailable")).toHaveTextContent(
      "Agents can’t run on this instance",
    );
    expect(screen.getByTestId("agents-empty")).not.toHaveTextContent("Run a check");
    expect(screen.queryByTestId("run-check")).toBeNull();
  });

  it("lists the runs and follows the newest live: its steps, its sandbox log with a blocked request, its facts", async () => {
    const user = userEvent.setup();
    render({
      "GET /api/v1/runs": { body: runList([LIVE, OLDER]) },
      [`GET /api/v1/runs/${RUN_ID}`]: { body: LIVE },
    });
    const rows = await screen.findAllByTestId("runs-row");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveAttribute("aria-current", "true");
    expect(rows[0]).toHaveTextContent("Check");
    expect(rows[0]).toHaveTextContent("Running");
    expect(rows[1]).toHaveTextContent("Failed");
    expect(screen.getByTestId("runs")).toHaveAttribute("data-showing", "list");

    expect(await screen.findByTestId("run-status")).toHaveTextContent("Running");
    expect(screen.getByTestId("run-log-empty")).toHaveTextContent("Connecting to the run…");
    const source = FakeEventSource.latest();
    expect(source.url).toBe(`/api/v1/runs/${RUN_ID}/events/stream`);
    source.emit("open");
    expect(screen.getByTestId("run-log-empty")).toHaveTextContent("Nothing yet.");
    for (const event of PROBE_EVENTS) {
      source.emit("run_event", event);
    }
    // A line seen twice (a reconnect) is shown once.
    source.emit("run_event", PROBE_EVENTS[0]);

    const agent = screen.getByTestId("run-log-agent");
    expect(agent).toHaveAttribute("role", "log");
    const steps = within(agent).getAllByTestId("run-event");
    expect(steps).toHaveLength(4);
    expect(steps[0]).toHaveTextContent("Read its context.");
    expect(steps[1]).toHaveTextContent("ToolCalled probe_echo; it answered.");
    expect(steps[2]).toHaveTextContent("Blocked");

    await user.click(screen.getByTestId("run-tab-sandbox"));
    const lines = within(screen.getByTestId("run-log-sandbox")).getAllByTestId("run-event");
    expect(lines).toHaveLength(2);
    expect(lines[0]).toHaveTextContent("Allowed");
    expect(lines[1]).toHaveAttribute("data-level", "deny");
    expect(lines[1]).toHaveTextContent("Blocked");
    expect(lines[1]).toHaveTextContent("example.com:443");

    expect(screen.getByTestId("run-fact-sandbox")).toHaveTextContent("run-0190a1b2c1");
    expect(screen.getByTestId("run-fact-image")).toHaveTextContent(
      "ghcr.io/notacamp/applicamp-agent:main",
    );
    expect(screen.getByTestId("run-fact-policy")).toHaveTextContent("policies/probe.yaml");
    expect(screen.getByTestId("run-fact-model")).toHaveTextContent("Not yet");
    expect(screen.getByTestId("run-fact-tokens")).toHaveTextContent("0 in · 0 out");
    expect(screen.getByTestId("run-fact-cost")).toHaveTextContent("€0.00");
    expect(screen.getByTestId("run-fact-duration")).toHaveTextContent("2 sec");
    expect(screen.getByTestId("run-fact-end")).toHaveTextContent("Running");

    // It ends: the stream closes and the run says how.
    source.emit("end", { ...LIVE, status: "finished", outcome: "succeeded", duration_ms: 4200 });
    expect(source.readyState).toBe(FakeEventSource.CLOSED);
    await waitFor(() => {
      expect(screen.getByTestId("run-status")).toHaveTextContent("Succeeded");
    });
    expect(screen.getByTestId("run-status")).toHaveAttribute("data-outcome", "succeeded");
    expect(screen.getByTestId("run-fact-model")).toHaveTextContent("None: it asked no model");
    expect(screen.getByTestId("run-fact-duration")).toHaveTextContent("4 sec");
    expect(screen.queryByTestId("run-stop")).toBeNull();
  });

  it("shows another run when chosen, and why a failed one failed", async () => {
    const user = userEvent.setup();
    const { onSelect } = render({
      "GET /api/v1/runs": { body: runList([run(), OLDER]) },
      [`GET /api/v1/runs/${RUN_ID}`]: { body: run() },
    });
    const rows = await screen.findAllByTestId("runs-row");
    if (rows[1] !== undefined) {
      await user.click(rows[1]);
    }
    expect(onSelect).toHaveBeenCalledWith(OLDER_RUN_ID);
  });

  it("shows the chosen run alone on a phone, with the way back", async () => {
    const user = userEvent.setup();
    const { onSelect } = render(
      {
        "GET /api/v1/runs": { body: runList([run(), OLDER]) },
        [`GET /api/v1/runs/${OLDER_RUN_ID}`]: { body: OLDER },
      },
      { selected: OLDER_RUN_ID },
    );
    expect(await screen.findByTestId("runs")).toHaveAttribute("data-showing", "detail");
    expect(await screen.findByTestId("run-failure")).toHaveTextContent(
      "Its agent stopped before it finished.",
    );
    const rows = screen.getAllByTestId("runs-row");
    expect(rows[1]).toHaveAttribute("aria-current", "true");
    FakeEventSource.latest().emit("end", OLDER);
    expect(screen.getByTestId("run-log-empty")).toHaveTextContent("It wrote nothing.");
    await user.click(screen.getByTestId("runs-back"));
    expect(onSelect).toHaveBeenCalledWith(null);
  });

  it("runs a check and shows it", async () => {
    const user = userEvent.setup();
    const queued = run({ status: "queued", outcome: null, started_at: null, duration_ms: null });
    const { requests, onSelect } = render({
      "GET /api/v1/runs": { body: runList([]) },
      "POST /api/v1/runs": async (request) => {
        expect(await request.json()).toEqual({ kind: "runs.probe" });
        return { status: 201, body: queued };
      },
    });
    await user.click(await screen.findByTestId("run-check"));
    await waitFor(() => {
      expect(onSelect).toHaveBeenCalledWith(RUN_ID);
    });
    const post = requests.find(({ route }) => route === "POST /api/v1/runs");
    expect(post?.headers.get("x-applicamp-csrf")).toBe("1");
  });

  it("says why a check couldn't start", async () => {
    const user = userEvent.setup();
    render({
      "GET /api/v1/runs": { body: runList([]) },
      "POST /api/v1/runs": { status: 429, body: { code: "too_many_runs" } },
    });
    await user.click(await screen.findByTestId("run-check"));
    expect(await screen.findByTestId("run-check-error")).toHaveTextContent(
      "Too many runs are waiting.",
    );
  });

  it("stops a live run", async () => {
    const user = userEvent.setup();
    const { requests } = render({
      "GET /api/v1/runs": { body: runList([LIVE]) },
      [`GET /api/v1/runs/${RUN_ID}`]: { body: LIVE },
      [`POST /api/v1/runs/${RUN_ID}/cancel`]: {
        status: 202,
        body: { ...LIVE, stopping: true },
      },
    });
    await user.click(await screen.findByTestId("run-stop"));
    await waitFor(() => {
      expect(requests.map(({ route }) => route)).toContain(`POST /api/v1/runs/${RUN_ID}/cancel`);
    });
    const post = requests.find(({ route }) => route.endsWith("/cancel"));
    expect(post?.headers.get("x-applicamp-csrf")).toBe("1");
  });

  it("says when a run couldn't be stopped", async () => {
    const user = userEvent.setup();
    render({
      "GET /api/v1/runs": { body: runList([LIVE]) },
      [`GET /api/v1/runs/${RUN_ID}`]: { body: LIVE },
      [`POST /api/v1/runs/${RUN_ID}/cancel`]: { status: 500, body: { code: "internal_error" } },
    });
    await user.click(await screen.findByTestId("run-stop"));
    expect(await screen.findByTestId("run-stop-error")).toHaveTextContent(
      "It couldn’t be stopped.",
    );
  });

  it("says Stopping once Stop was pressed", async () => {
    const stopping = { ...LIVE, stopping: true };
    render({
      "GET /api/v1/runs": { body: runList([stopping]) },
      [`GET /api/v1/runs/${RUN_ID}`]: { body: stopping },
    });
    expect(await screen.findByTestId("run-status")).toHaveTextContent("Stopping…");
    expect(screen.queryByTestId("run-stop")).toBeNull();
  });

  it("reconnects a lost live view after the last line it saw", async () => {
    const user = userEvent.setup();
    render({
      "GET /api/v1/runs": { body: runList([LIVE]) },
      [`GET /api/v1/runs/${RUN_ID}`]: { body: LIVE },
    });
    await screen.findByTestId("run-detail");
    const first = FakeEventSource.latest();
    first.emit("open");
    first.emit("run_event", PROBE_EVENTS[0]);
    first.emit("run_event", PROBE_EVENTS[1]);
    first.fail();
    expect(await screen.findByTestId("run-live-lost")).toHaveTextContent("The live view stopped.");
    await user.click(screen.getByTestId("run-live-retry"));
    await waitFor(() => {
      expect(FakeEventSource.opened).toHaveLength(2);
    });
    expect(FakeEventSource.latest().url).toBe(`/api/v1/runs/${RUN_ID}/events/stream?after=2`);
    expect(first.readyState).toBe(FakeEventSource.CLOSED);
    expect(screen.queryByTestId("run-live-lost")).toBeNull();
  });

  it("says when the runs couldn't be loaded, and loads them again", async () => {
    const user = userEvent.setup();
    let calls = 0;
    render({
      "GET /api/v1/runs": () => {
        calls += 1;
        return calls === 1
          ? { status: 500, body: { code: "internal_error" } }
          : { body: runList([]) };
      },
    });
    const error = await screen.findByTestId("runs-load-error");
    expect(error).toHaveTextContent("Your runs couldn’t be loaded.");
    await user.click(error.querySelector("button") ?? error);
    expect(await screen.findByTestId("agents-empty")).toBeInTheDocument();
  });

  it("speaks the person's language", async () => {
    stubApi({
      "GET /api/v1/runs": { body: runList([run()]) },
      [`GET /api/v1/runs/${RUN_ID}`]: { body: run() },
    });
    renderWithProviders(<RunsPage selected={undefined} onSelect={vi.fn()} />, { locale: "fr" });
    expect(await screen.findByTestId("run-status")).toHaveTextContent("Réussie");
    expect(screen.getByTestId("run-tab-sandbox")).toHaveTextContent("Journal du bac à sable");
  });
});
