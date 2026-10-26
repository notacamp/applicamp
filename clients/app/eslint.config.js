// @ts-check
import eslintComments from "@eslint-community/eslint-plugin-eslint-comments";
import js from "@eslint/js";
import vitest from "@vitest/eslint-plugin";
import jsxA11y from "eslint-plugin-jsx-a11y";
import react from "eslint-plugin-react";
import reactHooks from "eslint-plugin-react-hooks";
import unicorn from "eslint-plugin-unicorn";
import globals from "globals";
import tseslint from "typescript-eslint";

// AGENTS.md → "UI comes from @notacamp/ui": screens are built from the
// package's components; class names and inline styles live in src/lib only
// (the token set and the compositions in src/lib/ui).
const NO_CLASS_NAME =
  "Screens use @notacamp/ui components through ~/lib/ui; class names live only in src/lib. Compose a new pattern in src/lib/ui.";
const NO_STYLE =
  "Inline styles bypass the design tokens. Use a ~/lib/ui component (compose one in src/lib/ui if none fits).";

// AGENTS.md: tests locate elements by `data-testid` only, so a copy edit or a
// translation never breaks one. Testing Library's other queries and
// Playwright's role/text/CSS locators are refused.
const LOCATOR_MESSAGE =
  "Locate elements by data-testid only (getByTestId); assert on text with toHaveTextContent.";
const NON_TESTID_QUERY = String.raw`/^(get|query|find)(All)?By(?!TestId$)/`;
const TESTID_ONLY_RULES = [
  {
    selector: `CallExpression[callee.property.name=${NON_TESTID_QUERY}]`,
    message: LOCATOR_MESSAGE,
  },
  { selector: `CallExpression[callee.name=${NON_TESTID_QUERY}]`, message: LOCATOR_MESSAGE },
  {
    selector: "CallExpression[callee.property.name=/^(locator|\\$|\\$\\$)$/]",
    message: LOCATOR_MESSAGE,
  },
];

const WEB_STORAGE_MESSAGE =
  "Web Storage is readable by any script on the origin. Use ~/lib/storage for UI preferences only; the session never reaches browser JS.";

export default tseslint.config(
  {
    ignores: [
      "dist/",
      "coverage/",
      "test-results/",
      "playwright-report/",
      // Generated from openapi/openapi.json (`pnpm api:generate`); never hand-edited.
      "src/lib/api/schema.gen.ts",
    ],
  },
  {
    linterOptions: {
      // Inline `eslint-disable` comments are not honoured at all, and any that
      // exist are reported. Fix the code or change the rule here, in review.
      noInlineConfig: true,
      reportUnusedDisableDirectives: "error",
    },
  },
  js.configs.recommended,
  tseslint.configs.strictTypeChecked,
  tseslint.configs.stylisticTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: { allowDefaultProject: ["*.js", "*.cjs", "public/*.js"] },
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
  unicorn.configs.unopinionated,
  {
    plugins: { "@eslint-community/eslint-comments": eslintComments },
    rules: {
      "@eslint-community/eslint-comments/no-use": ["error", { allow: [] }],
      "unicorn/filename-case": [
        "error",
        // kebab-case by default; PascalCase is for component files/folders.
        { cases: { kebabCase: true, pascalCase: true } },
      ],
      "@typescript-eslint/consistent-type-imports": ["error", { fixStyle: "inline-type-imports" }],
      "@typescript-eslint/no-import-type-side-effects": "error",
      "@typescript-eslint/restrict-template-expressions": ["error", { allowNumber: false }],
      "no-restricted-globals": [
        "error",
        { name: "localStorage", message: WEB_STORAGE_MESSAGE },
        { name: "sessionStorage", message: WEB_STORAGE_MESSAGE },
      ],
      "no-restricted-properties": [
        "error",
        ...["window", "globalThis", "self"].flatMap((object) => [
          { object, property: "localStorage", message: WEB_STORAGE_MESSAGE },
          { object, property: "sessionStorage", message: WEB_STORAGE_MESSAGE },
        ]),
        {
          object: "document",
          property: "cookie",
          message:
            "The session cookie is httpOnly and owned by the server's BFF; JS never touches cookies.",
        },
      ],
    },
  },
  {
    files: ["src/lib/storage/**"],
    rules: { "no-restricted-globals": "off", "no-restricted-properties": "off" },
  },
  {
    files: ["**/*.{ts,tsx}"],
    ...react.configs.flat.recommended,
    ...react.configs.flat["jsx-runtime"],
    languageOptions: {
      ...react.configs.flat.recommended?.languageOptions,
      globals: globals.browser,
    },
    settings: { react: { version: "detect" } },
  },
  {
    files: ["**/*.{ts,tsx}"],
    rules: {
      ...react.configs.flat.recommended?.rules,
      ...react.configs.flat["jsx-runtime"]?.rules,
      "react/no-danger": "error",
      "react/jsx-no-target-blank": "error",
      "react/no-unstable-nested-components": "error",
    },
  },
  {
    // Everything outside src/lib: no class names, no inline styles.
    files: ["src/app/**/*.{ts,tsx}", "src/modules/**/*.{ts,tsx}"],
    rules: {
      "react/forbid-dom-props": [
        "error",
        {
          forbid: [
            { propName: "className", message: NO_CLASS_NAME },
            { propName: "style", message: NO_STYLE },
          ],
        },
      ],
      "react/forbid-component-props": [
        "error",
        {
          forbid: [
            { propName: "className", message: NO_CLASS_NAME },
            { propName: "style", message: NO_STYLE },
          ],
        },
      ],
    },
  },
  reactHooks.configs.flat["recommended-latest"],
  jsxA11y.flatConfigs.strict,
  {
    files: ["**/*.test.{ts,tsx}"],
    ...vitest.configs.recommended,
    rules: {
      ...vitest.configs.recommended.rules,
      "vitest/consistent-test-it": ["error", { fn: "it" }],
      "vitest/no-focused-tests": "error",
      "vitest/no-disabled-tests": "error",
      "vitest/require-top-level-describe": "error",
    },
  },
  {
    files: ["**/*.test.{ts,tsx}", "e2e/**/*.ts", "test/**/*.{ts,tsx}"],
    rules: { "no-restricted-syntax": ["error", ...TESTID_ONLY_RULES] },
  },
  {
    files: ["*.config.{js,ts}", "*.cjs", "scripts/**/*.ts", "e2e/**/*.ts"],
    languageOptions: { globals: globals.node },
  },
  {
    files: ["**/*.{js,cjs}"],
    ...tseslint.configs.disableTypeChecked,
  },
  {
    // Runs before any module: it reads the saved theme straight from Web
    // Storage, with the same key prefix as src/lib/storage.
    files: ["public/theme-boot.js"],
    languageOptions: { sourceType: "script", globals: globals.browser },
    rules: { "no-restricted-globals": "off", "no-restricted-properties": "off" },
  },
  {
    files: ["**/*.cjs"],
    languageOptions: { sourceType: "commonjs", globals: globals.node },
    rules: { "@typescript-eslint/no-require-imports": "off" },
  },
);
