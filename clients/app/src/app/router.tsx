import type { QueryClient } from "@tanstack/react-query";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Outlet,
  type RouterHistory,
} from "@tanstack/react-router";
import { z } from "zod";

import { ThemeProvider, UndoProvider } from "~/lib/ui";
import { NotInvitedPage, SIGN_IN_ERRORS, SignInPage, type SignInError } from "~/modules/identity";
import { QUESTIONS, type Question } from "~/modules/profile";

import { Agents } from "./routes/agents";
import { Boards } from "./routes/boards";
import { Cvs } from "./routes/cvs";
import { SignedInFrame } from "./routes/frame";
import { GoalsInterview } from "./routes/goals-interview";
import { NotFound } from "./routes/not-found";
import { Pipeline } from "./routes/pipeline";
import { PipelineCv } from "./routes/pipeline-cv";
import { PipelinePrepare } from "./routes/pipeline-prepare";
import { Profile } from "./routes/profile";
import { ProfileGrill } from "./routes/profile-grill";
import { ProfileImprove } from "./routes/profile-improve";
import { ProfileSkills } from "./routes/profile-skills";
import { Today } from "./routes/today";

export interface RouterContext {
  queryClient: QueryClient;
}

const rootRoute = createRootRouteWithContext<RouterContext>()({
  component: function RootLayout() {
    return (
      <ThemeProvider>
        <UndoProvider>
          <Outlet />
        </UndoProvider>
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

/**
 * `?new=true` shows only the new matches (Today's "N new matches");
 * `?run=search` starts a search and `?run=add` opens New (⌘K, from anywhere).
 */
const pipelineSearch = z.object({
  new: z.boolean().optional(),
  run: z.enum(["search", "add"]).optional(),
});

const pipelineRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/pipeline",
  validateSearch: (
    search: Record<string, unknown>,
  ): { new?: boolean | undefined; run?: "search" | "add" | undefined } => {
    const parsed = pipelineSearch.safeParse(search);
    return parsed.success ? { new: parsed.data.new, run: parsed.data.run } : {};
  },
  component: function PipelineRoute() {
    const search = pipelineRoute.useSearch();
    const navigate = pipelineRoute.useNavigate();
    return (
      <Pipeline
        newOnly={search.new === true}
        run={search.run}
        onSearch={(next) => {
          void navigate({ search: next, replace: true });
        }}
        onPrepare={(id) => {
          void navigate({ to: "/pipeline/$id/prepare", params: { id } });
        }}
        onReview={(id) => {
          void navigate({ to: "/pipeline/$id/cv", params: { id } });
        }}
      />
    );
  },
});

/** Prepare a tailored CV: the company's and the job's real facts, in the app frame. */
const prepareRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/pipeline/$id/prepare",
  component: function PrepareRoute() {
    const { id } = prepareRoute.useParams();
    const navigate = prepareRoute.useNavigate();
    return (
      <PipelinePrepare
        id={id}
        onBack={() => {
          void navigate({ to: "/pipeline" });
        }}
        onReview={() => {
          void navigate({ to: "/pipeline/$id/cv", params: { id } });
        }}
      />
    );
  },
});

/** Review the tailored CV: every line traced to the profile, approved when none is blocked. */
const cvRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/pipeline/$id/cv",
  component: function CvRoute() {
    const { id } = cvRoute.useParams();
    const navigate = cvRoute.useNavigate();
    return (
      <PipelineCv
        id={id}
        onBack={() => {
          void navigate({ to: "/pipeline/$id/prepare", params: { id } });
        }}
        onPipeline={() => {
          void navigate({ to: "/pipeline" });
        }}
      />
    );
  },
});

/** Every CV: the PDFs made and the drafts being reviewed. */
const cvsRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/cvs",
  component: function CvsRoute() {
    const navigate = cvsRoute.useNavigate();
    return (
      <Cvs
        onReview={(id) => {
          void navigate({ to: "/pipeline/$id/cv", params: { id } });
        }}
        onOpportunity={() => {
          void navigate({ to: "/pipeline" });
        }}
        onPipeline={() => {
          void navigate({ to: "/pipeline" });
        }}
      />
    );
  },
});

/** `?tab=scanned` shows what each search scanned; anything else, the boards. */
const boardsSearch = z.object({ tab: z.literal("scanned") });

const boardsRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/boards",
  validateSearch: (search: Record<string, unknown>): { tab?: "scanned" | undefined } => {
    const parsed = boardsSearch.safeParse(search);
    return { tab: parsed.success ? parsed.data.tab : undefined };
  },
  component: function BoardsRoute() {
    const search = boardsRoute.useSearch();
    const navigate = boardsRoute.useNavigate();
    return (
      <Boards
        tab={search.tab ?? "boards"}
        onTab={(tab) => {
          void navigate({ search: tab === "scanned" ? { tab } : {}, replace: true });
        }}
      />
    );
  },
});
/** `?run=<id>` shows that run (on a phone, instead of the list). */
const agentsSearch = z.object({ run: z.uuid() });

const agentsRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/agents",
  validateSearch: (search: Record<string, unknown>): { run?: string | undefined } => {
    const parsed = agentsSearch.safeParse(search);
    return { run: parsed.success ? parsed.data.run : undefined };
  },
  component: function AgentsRoute() {
    const search = agentsRoute.useSearch();
    const navigate = agentsRoute.useNavigate();
    return (
      <Agents
        run={search.run}
        onRun={(id) => {
          void navigate({ search: id === null ? {} : { run: id } });
        }}
      />
    );
  },
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
        onStartGoals={(step) => {
          void navigate({ to: "/goals/interview", search: { step } });
        }}
        onGrill={(position) => {
          if (position === undefined) {
            void navigate({ to: "/profile/grill" });
          } else {
            void navigate({ to: "/profile/grill/$position", params: { position } });
          }
        }}
        onAction={(action) => {
          switch (action.kind) {
            case "improve": {
              void navigate({ to: "/profile/improve" });
              break;
            }
            case "question": {
              void navigate({ to: "/profile/improve", search: { q: action.id } });
              break;
            }
            case "skills": {
              void navigate({ to: "/profile/skills" });
              break;
            }
            case "import": {
              void navigate({ search: { import: "new" } });
              break;
            }
            case "goals": {
              void navigate({ to: "/goals/interview" });
              break;
            }
            case "edit": {
              void navigate({ to: "/profile", hash: action.section });
              break;
            }
          }
        }}
      />
    );
  },
});

/** The skills page, in the app frame: Profile & goals' Skills, in order. */
const profileSkillsRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/profile/skills",
  component: function ProfileSkillsRoute() {
    const navigate = profileSkillsRoute.useNavigate();
    return (
      <ProfileSkills
        onBack={() => {
          void navigate({ to: "/profile" });
        }}
      />
    );
  },
});

/** Grill me: the roles, in the app frame. */
const profileGrillRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/profile/grill",
  component: function ProfileGrillRoute() {
    const navigate = profileGrillRoute.useNavigate();
    return (
      <ProfileGrill
        position={undefined}
        onPosition={(position) => {
          void navigate({ to: "/profile/grill/$position", params: { position } });
        }}
        onBack={() => {
          void navigate({ to: "/profile" });
        }}
      />
    );
  },
});

/** Grill me about one role (its place in the profile's positions). */
const profileGrillRoleRoute = createRoute({
  getParentRoute: () => frameRoute,
  path: "/profile/grill/$position",
  params: {
    parse: (raw) => ({ position: Number(raw.position) }),
    stringify: ({ position }) => ({ position: String(position) }),
  },
  component: function ProfileGrillRoleRoute() {
    const { position } = profileGrillRoleRoute.useParams();
    const navigate = profileGrillRoleRoute.useNavigate();
    if (!Number.isInteger(position) || position < 0) {
      return <NotFound />;
    }
    return (
      <ProfileGrill
        position={position}
        onPosition={(next) => {
          void navigate({ to: "/profile/grill/$position", params: { position: next } });
        }}
        onBack={() => {
          void navigate({ to: "/profile" });
        }}
      />
    );
  },
});

/** `?q=` is the question asked for; none is the most useful open one. */
const improveSearch = z.object({ q: z.string().min(1).max(64) });

/** Improve my profile: signed in, outside the app frame, so nothing else competes with the question. */
const improveRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/profile/improve",
  validateSearch: (search: Record<string, unknown>): { q?: string | undefined } => {
    const parsed = improveSearch.safeParse(search);
    return { q: parsed.success ? parsed.data.q : undefined };
  },
  component: function ImproveRoute() {
    const search = improveRoute.useSearch();
    const navigate = improveRoute.useNavigate();
    return (
      <ProfileImprove
        question={search.q}
        onLeave={(reason) => {
          void navigate({ to: reason === "done" ? "/profile" : "/" });
        }}
      />
    );
  },
});

/** `?step=` is the question on screen; none (or an unknown one) is the first. */
const interviewSearch = z.object({ step: z.enum(QUESTIONS) });

/** The goals interview: signed in, but outside the app frame, so nothing else competes with the question. */
const interviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/goals/interview",
  validateSearch: (search: Record<string, unknown>): { step?: Question | undefined } => {
    const parsed = interviewSearch.safeParse(search);
    return { step: parsed.success ? parsed.data.step : undefined };
  },
  component: function InterviewRoute() {
    const search = interviewRoute.useSearch();
    const navigate = interviewRoute.useNavigate();
    return (
      <GoalsInterview
        question={search.step}
        onQuestion={(step) => {
          void navigate({ search: { step } });
        }}
        onLeave={(reason) => {
          void navigate({ to: reason === "done" ? "/profile" : "/" });
        }}
      />
    );
  },
});

const routeTree = rootRoute.addChildren([
  signInRoute,
  notInvitedRoute,
  interviewRoute,
  improveRoute,
  frameRoute.addChildren([
    todayRoute,
    pipelineRoute,
    prepareRoute,
    cvRoute,
    cvsRoute,
    boardsRoute,
    agentsRoute,
    profileRoute,
    profileSkillsRoute,
    profileGrillRoute,
    profileGrillRoleRoute,
  ]),
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
