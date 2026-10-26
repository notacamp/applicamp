import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { I18nextProvider } from "react-i18next";

import { DEFAULT_SHORTCUTS, ShortcutProvider, ShortcutRegistry } from "~/lib/shortcuts";

import { i18n } from "./i18n";
import { createAppRouter } from "./router";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      // One retry smooths over a blip without leaving a dead API spinning for long.
      retry: 1,
    },
  },
});

const router = createAppRouter(queryClient);

/** One registry for the app, conflict-checked once at startup. */
const shortcuts = new ShortcutRegistry(DEFAULT_SHORTCUTS);

export function App() {
  return (
    <StrictMode>
      <I18nextProvider i18n={i18n}>
        <ShortcutProvider registry={shortcuts}>
          <QueryClientProvider client={queryClient}>
            <RouterProvider router={router} />
          </QueryClientProvider>
        </ShortcutProvider>
      </I18nextProvider>
    </StrictMode>
  );
}
