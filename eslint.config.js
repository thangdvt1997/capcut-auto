// Flat ESLint config (ESLint 9). Minimal on purpose for Phase 2: catch real
// correctness issues (no-unused-vars, no-explicit-any per master prompt §66
// "avoid `any`") without importing a house style. Tighten as the codebase
// grows past the app-shell scaffold.
import js from "@eslint/js";
import svelte from "eslint-plugin-svelte";
import tseslint from "typescript-eslint";
import globals from "globals";

export default tseslint.config(
  {
    // `vendor/**`: the read-only, gitignored upstream clones (`docs/upstream.md`)
    // used only for audit/porting reference (`autocut`/`capcut-mate`, each with
    // its own lint setup and its own Node/Electron globals) — never part of
    // this app's build, so linting it against this config's browser-only
    // globals/rules is both wrong and not this repo's problem to fix.
    ignores: ["dist/**", "src-tauri/**", "node_modules/**", "src/types/bindings.ts", "vendor/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs["flat/recommended"],
  {
    // This is a Tauri webview frontend (browser globals), not Node.
    languageOptions: {
      globals: globals.browser,
    },
  },
  {
    files: ["**/*.svelte"],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
    },
  },
  {
    rules: {
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-unused-vars": ["warn", { argsIgnorePattern: "^_" }],
    },
  },
);
