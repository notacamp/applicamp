import type { QueryClient } from "@tanstack/react-query";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Outlet,
  type RouterHistory,
} from "@tanstack/react-router";
import { z } from "zod";

import { ThemeProvider } from "~/lib/ui";
import { NotInvitedPage, SIGN_IN_ERRORS, SignInPage, type SignInError } from "~/modules/identity";

import { Agents } from "./routes/agents";
import { SignedInFrame } from "./routes/frame";
import { NotFound } from "./routes/not-found";
import { Pipeline } from "./routes/pipeline";
import { Profile } from "./routes/profile";
import { Today } from "./routes/today";

export interface RouterContext {
  queryClient: QueryClient;
}

const rootRoute = createRootRouteWithContext<RouterContext>()({
  component: function RootLayout() {
    return (
      <ThemeProvider>
        <Outlet />
      </ThemeProvider>
    );
  },
  notFoundComponent: NotFound,
});

/** `?error=` as the server's BFF sends it; anything else is no error. */
const signInSearch = z.object({ error: z.enum(SIGN_IN_ERRORS) });

const signInRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/sign-in",
  // Always says `error`, so an unknown one never passes through from the
  // root's raw search.
  validateSearch: (search: Record<string, unknown>): { error?: SignInError | undefined } => {
    const parsed = signInSearch.safeParse(search);
    return { error: parsed.success ? parsed.data.error : undefined };
  },
  component: function SignInRoute() {
    const { error } = signInRoute.useSearch();
    return <SignInPage error={error} />;
  },
});

const notInvitedRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/not-invited",
  component: NotInvitedPage,
});

/** Every page that needs a signed-in browser: the session gate and the app frame. */
const frameRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "signed-in",
  component: SignedInFrame,
});

const todayRoute = createRoute({ getParentRoute: () => frameRoute, path: "/", component: Today });
const pipelineRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/pipeline",
  component: Pipeline,
});
const agentsRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/agents",
  component: Agents,
});
/** `?import=new` opens the import panel (Today's "Set up your profile"). */
const profileSearch = z.object({ import: z.literal("new") });

const profileRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/profile",
  validateSearch: (search: Record<string, unknown>): { import?: "new" | undefined } => {
    const parsed = profileSearch.safeParse(search);
    return { import: parsed.success ? parsed.data.import : undefined };
  },
  component: function ProfileRoute() {
    const search = profileRoute.useSearch();
    const navigate = profileRoute.useNavigate();
    return (
      <Profile
        importing={search.import === "new"}
        onImporting={(open) => {
          void navigate({ search: open ? { import: "new" } : {} });
        }}
      />
    );
  },
});

const routeTree = rootRoute.addChildren([
  signInRoute,
  notInvitedRoute,
  frameRoute.addChildren([todayRoute, pipelineRoute, agentsRoute, profileRoute]),
]);

/** The app's one router (`history` defaults to the browser's; tests pass a memory one). */
export function createAppRouter(queryClient: QueryClient, history?: RouterHistory) {
  return createRouter({
    routeTree,
    ...(history === undefined ? {} : { history }),
    context: { queryClient },
    defaultPreload: "intent",
    scrollRestoration: true,
  });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
