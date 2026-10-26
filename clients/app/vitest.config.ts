import { defineConfig, mergeConfig } from "vitest/config";

import viteConfig from "./vite.config.ts";

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: "jsdom",
      setupFiles: ["./test/setup.ts"],
      include: ["src/**/*.test.{ts,tsx}", "test/**/*.test.{ts,tsx}", "scripts/**/*.test.ts"],
      restoreMocks: true,
      unstubGlobals: true,
      testTimeout: 15_000,
      // Process CSS so the design system's class names are real.
      css: true,
      coverage: {
        provider: "v8",
        include: ["src/lib/**", "src/modules/**"],
        exclude: ["**/*.gen.ts", "**/*.test.{ts,tsx}", "**/*.json", "**/*.css", "**/*.d.ts"],
        reporter: ["text", "html", "lcov"],
        thresholds: { lines: 80, functions: 80, branches: 80, statements: 80 },
      },
    },
  }),
);
