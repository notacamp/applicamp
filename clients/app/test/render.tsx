import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createMemoryHistory, RouterProvider } from "@tanstack/react-router";
import { render, type RenderResult } from "@testing-library/react";
import type { ReactNode } from "react";
import { I18nextProvider } from "react-i18next";
import { vi } from "vitest";

import { createI18n, type Locale } from "~/lib/i18n";
import { DEFAULT_SHORTCUTS, ShortcutProvider, ShortcutRegistry } from "~/lib/shortcuts";

import { NAMESPACES } from "../src/app/messages";
import { createAppRouter } from "../src/app/router";

/** A fresh query cache that never retries (a failure shows at once). */
export function testQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
}

/**
 * Renders `ui` like the app does: real i18n (every namespace), the app's
 * shortcuts on the non-Apple platform (Ctrl, not ⌘), a fresh query cache.
 */
export function renderWithProviders(
  ui: ReactNode,
  {
    locale = "en",
    queryClient = testQueryClient(),
  }: { locale?: Locale; queryClient?: QueryClient } = {},
): RenderResult & { queryClient: QueryClient } {
  const i18n = createI18n({ locale, namespaces: NAMESPACES });
  const registry = new ShortcutRegistry(DEFAULT_SHORTCUTS);
  function Providers({ children }: { children: ReactNode }) {
    return (
      <I18nextProvider i18n={i18n}>
        <ShortcutProvider registry={registry} platform="other">
          <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
        </ShortcutProvider>
      </I18nextProvider>
    );
  }
  const result = render(ui, { wrapper: Providers });
  return { ...result, queryClient };
}

/** The whole app at `path`, with a memory history. */
export function renderApp(path: string, options: { locale?: Locale } = {}) {
  const queryClient = testQueryClient();
  const router = createAppRouter(queryClient, createMemoryHistory({ initialEntries: [path] }));
  const view = renderWithProviders(<RouterProvider router={router} />, { ...options, queryClient });
  return { ...view, router };
}

/** What a stubbed endpoint answers. */
export interface StubAnswer {
  status?: number;
  body?: unknown;
}

/** Every request the stubbed API received, as `METHOD /path` with its headers. */
export interface StubbedRequest {
  route: string;
  headers: Headers;
}

/**
 * Stubs `fetch` with answers keyed by `METHOD /path`. An answer can be a
 * function, called per request. Anything else is a network error.
 */
export function stubApi(routes: Record<string, StubAnswer | (() => StubAnswer)>): StubbedRequest[] {
  const requests: StubbedRequest[] = [];
  const fetch = (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const request = input instanceof Request ? input : new Request(input, init);
    const route = `${request.method} ${new URL(request.url).pathname}`;
    requests.push({ route, headers: request.headers });
    const stub = routes[route];
    if (stub === undefined) {
      return Promise.reject(new TypeError(`Network error: no stub for ${route}`));
    }
    const { status = 200, body } = typeof stub === "function" ? stub() : stub;
    if (body === undefined) {
      return Promise.resolve(new Response(null, { status }));
    }
    const contentType = status >= 400 ? "application/problem+json" : "application/json";
    return Promise.resolve(
      Response.json(body, { status, headers: { "content-type": contentType } }),
    );
  };
  vi.stubGlobal("fetch", fetch);
  return requests;
}

/** A signed-in person (invented). */
export const DEMO_ME = {
  id: "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b",
  email: "demo@example.com",
  name: "Alex Demo",
  picture_url: null,
  ways_in: ["campbooks"],
};

/** The API's answer to a signed-out browser. */
export const SIGNED_OUT = {
  status: 401,
  body: {
    type: "about:blank",
    title: "Unauthorized",
    status: 401,
    detail: "Sign in first.",
    code: "session_required",
  },
};
