import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";

import { api, CSRF_HEADERS, problemCode, unwrap, type Schema } from "~/lib/api";

/** A run of an agent in its own sandbox, with its facts. */
export type Run = Schema<"Run">;
export type RunStatus = Schema<"RunStatus">;
export type Outcome = Schema<"Outcome">;
/** The person's runs, and whether runs can start here. */
export type RunList = Schema<"RunList">;

export const RUNS_QUERY_KEY = ["runs"] as const;
const LIST_KEY = [...RUNS_QUERY_KEY, "list"] as const;
export const runKey = (id: string) => [...RUNS_QUERY_KEY, "run", id] as const;

/** The check anyone can start ("Run a check"). */
export const PROBE = "runs.probe";

/** How often a live run (and the list holding one) is asked how it is doing. */
export const LIVE_POLL_MS = 2000;

/** Whether `run` has not ended yet. */
export function isLive(run: Pick<Run, "status">): boolean {
  return run.status !== "finished";
}

/** The runs, newest first (`GET /api/v1/runs`), polled while one is live. */
export const runsQuery = queryOptions({
  queryKey: LIST_KEY,
  queryFn: async ({ signal }): Promise<RunList> =>
    unwrap(await api.GET("/api/v1/runs", { signal })),
  refetchInterval: (query) =>
    query.state.data?.runs.some((run) => isLive(run)) === true ? LIVE_POLL_MS : false,
});

export function useRuns() {
  return useQuery(runsQuery);
}

/** One run (`GET /api/v1/runs/{id}`), polled while it is live. */
export function runQuery(id: string) {
  return queryOptions({
    queryKey: runKey(id),
    queryFn: async ({ signal }): Promise<Run> =>
      unwrap(await api.GET("/api/v1/runs/{id}", { params: { path: { id } }, signal })),
    refetchInterval: (query) =>
      query.state.data === undefined || isLive(query.state.data) ? LIVE_POLL_MS : false,
  });
}

/** The run `id`, starting from what the list already says about it. */
export function useRun(id: string, initial: Run | undefined) {
  return useQuery({ ...runQuery(id), placeholderData: initial });
}

/** Starts a check (`POST /api/v1/runs` with `runs.probe`); the list then holds it. */
export function useStartCheck() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (): Promise<Run> =>
      unwrap(await api.POST("/api/v1/runs", { body: { kind: PROBE }, headers: CSRF_HEADERS })),
    onSuccess: (run) => {
      queryClient.setQueryData(runKey(run.id), run);
      queryClient.setQueryData<RunList>(LIST_KEY, (list) =>
        list === undefined ? list : { ...list, runs: [run, ...list.runs] },
      );
      void queryClient.invalidateQueries({ queryKey: LIST_KEY });
    },
  });
}

/** Stops a live run (`POST /api/v1/runs/{id}/cancel`). */
export function useStopRun() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string): Promise<Run> =>
      unwrap(
        await api.POST("/api/v1/runs/{id}/cancel", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: (run) => {
      queryClient.setQueryData(runKey(run.id), run);
    },
    // Stopping or not (it may have just ended), the run says where it is.
    onSettled: (_run, _error, id) => {
      void queryClient.invalidateQueries({ queryKey: runKey(id) });
      void queryClient.invalidateQueries({ queryKey: LIST_KEY });
    },
  });
}

/** A run has ended: what the live view ended with is the run as it is now. */
export function settleRun(queryClient: QueryClient, run: Run) {
  queryClient.setQueryData(runKey(run.id), run);
  void queryClient.invalidateQueries({ queryKey: LIST_KEY });
}

/** The problems starting a check can answer with, each with its own message. */
export const START_ERRORS = ["too_many_runs", "sandboxes_unavailable", "unknown_kind"] as const;
export type StartError = (typeof START_ERRORS)[number] | "generic";

function isStartError(code: string | undefined): code is (typeof START_ERRORS)[number] {
  return code !== undefined && (START_ERRORS as readonly string[]).includes(code);
}

/** What to say about a check that could not start. */
export function startErrorOf(error: unknown): StartError {
  const code = problemCode(error);
  return isStartError(code) ? code : "generic";
}
