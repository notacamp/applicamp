import createClient from "openapi-fetch";

import type { paths } from "./schema.gen";

/** The header every unsafe request carries; the server refuses those without it. */
export const CSRF_HEADERS = { "X-Applicamp-CSRF": "1" } as const;

/**
 * A typed client of the server on `origin`: the API (`/api/v1/…`) and the BFF
 * (`/bff/…`). The browser sends the session cookie itself (same origin); no
 * token ever reaches this code. Built over the current `fetch` on every call,
 * so tests can stub it.
 */
export function createApiClient(origin: string = globalThis.location.origin) {
  return createClient<paths>({
    baseUrl: origin,
    credentials: "same-origin",
    fetch: (request) => globalThis.fetch(request),
  });
}

export type ApiClient = ReturnType<typeof createApiClient>;

/** The app's one API client, typed from `openapi/openapi.json`. */
export const api: ApiClient = createApiClient();
