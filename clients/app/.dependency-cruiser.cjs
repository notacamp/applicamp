// Architecture rules for the client (AGENTS.md "Architecture rules").
// Run with `pnpm deps:check`.
/** @type {import("dependency-cruiser").IConfiguration} */
module.exports = {
  forbidden: [
    {
      name: "module-public-surface",
      comment:
        "A module may import another module only through its index.ts (modules mirror the server's crates and stay independently refactorable).",
      severity: "error",
      from: { path: "^src/modules/([^/]+)/" },
      to: {
        path: "^src/modules/[^/]+/",
        pathNot: ["^src/modules/$1/", String.raw`^src/modules/[^/]+/index\.ts$`],
      },
    },
    {
      name: "app-uses-module-public-surface",
      comment:
        "The shell wires modules in through their index.ts. Locale JSON is the one exception, read for i18n.",
      severity: "error",
      from: { path: "^src/app/" },
      to: {
        path: "^src/modules/",
        pathNot: [
          String.raw`^src/modules/[^/]+/index\.ts$`,
          String.raw`^src/modules/[^/]+/i18n/[^/]+\.json$`,
        ],
      },
    },
    {
      name: "lib-is-business-agnostic",
      comment: "lib/ is the business-agnostic toolbox: it never imports modules/ or the app shell.",
      severity: "error",
      from: { path: "^src/lib/" },
      to: { path: "^src/(modules|app)/" },
    },
    {
      name: "ui-public-surface",
      comment:
        "Modules and the app shell use the design system only through src/lib/ui/index.ts, so components stay refactorable and nothing reaches past the tokens.",
      severity: "error",
      from: { path: "^src/(modules|app)/" },
      to: { path: "^src/lib/ui/", pathNot: String.raw`^src/lib/ui/index\.ts$` },
    },
    {
      name: "design-system-through-lib-ui",
      comment:
        "Modules and the app shell use @notacamp/ui through src/lib/ui/index.ts, which adds Applicamp's token set, preferences, strings and compositions to it.",
      severity: "error",
      from: { path: "^src/(modules|app)/" },
      to: { path: "@notacamp/ui" },
    },
    {
      name: "modules-have-no-stylesheets",
      comment:
        "Visual styling lives in src/lib (the token set, lib/ui compositions); modules and the shell compose components.",
      severity: "error",
      from: { path: "^src/(modules|app)/" },
      to: { path: String.raw`\.css$` },
    },
    {
      name: "schema-is-private-to-lib-api",
      comment:
        "The generated OpenAPI types are an implementation detail of lib/api; use its client and Schema<> instead.",
      severity: "error",
      from: { pathNot: "^src/lib/api/" },
      to: { path: String.raw`^src/lib/api/schema\.gen\.ts$` },
    },
    {
      name: "no-circular",
      severity: "error",
      from: {},
      to: { circular: true },
    },
    {
      name: "not-to-unresolvable",
      severity: "error",
      from: {},
      to: { couldNotResolve: true },
    },
    {
      name: "not-to-dev-dep",
      comment:
        "Shipped code must not depend on devDependencies (they are absent from the production install).",
      severity: "error",
      from: { path: "^src/", pathNot: String.raw`\.test\.tsx?$` },
      to: { dependencyTypes: ["npm-dev"], dependencyTypesNot: ["type-only"] },
    },
    {
      name: "no-orphans",
      severity: "warn",
      from: {
        orphan: true,
        pathNot: [String.raw`\.d\.ts$`, String.raw`\.test\.tsx?$`, String.raw`^src/app/main\.tsx$`],
      },
      to: {},
    },
  ],
  options: {
    doNotFollow: { path: "node_modules" },
    // Count `import type` too: a type dependency is still a dependency.
    tsPreCompilationDeps: true,
    tsConfig: { fileName: "tsconfig.app.json" },
    enhancedResolveOptions: {
      exportsFields: ["exports"],
      conditionNames: ["import", "require", "node", "default", "types"],
      mainFields: ["module", "main", "types", "typings"],
    },
  },
};
