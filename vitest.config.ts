import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Separate from `vite.config.ts` (rather than a `test` block bolted onto it)
// so the real app build config never has to think about test-only concerns
// (`resolve.conditions`, `environment`) — Vitest's own `defineConfig` from
// `vitest/config` re-exports Vite's, so this is still a real Vite config the
// same `svelte()` plugin attaches to, just not the one `vite build`/`vite
// dev` read.
//
// `resolve.conditions: ["browser"]` matters for real: Svelte 5 ships
// different code behind the `browser`/`worker`/default export conditions
// (`svelte/internal/client` vs a server-rendering stub), and without forcing
// the `browser` condition here Vitest's default Node resolution picks the
// non-reactive server build, silently breaking every `$state`/`$derived` in
// `src/stores/*.svelte.ts`. This is the documented fix from Svelte's own
// Vitest setup guide, not a guess.
export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    conditions: ["browser"],
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.{test,spec}.ts"],
    globals: false,
    // Confirmed via direct reproduction (STUDIO_PLAN.md, Phase D16/D17
    // combined verification): running test files in separate parallel
    // workers made two independent, individually-100%-passing files
    // (`aiSettings.test.ts`, `presets.test.ts`) each hang on their very
    // first `vi.resetModules()` + dynamic `import()` when run together —
    // a real Vitest/jsdom worker-isolation interaction, not a logic bug in
    // either store (both pass cleanly alone, and both pass cleanly here).
    // This suite is small enough (7 files, ~130 tests, ~7s total) that
    // running files sequentially in one process is a trivial cost for
    // eliminating a real, reproducible flake.
    fileParallelism: false,
  },
});
