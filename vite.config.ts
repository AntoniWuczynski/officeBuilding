import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri expects a fixed dev port and a relative asset base so the built
// bundle loads from the app's `dist/` inside the native shell. `pnpm build`
// (cross-platform, no Rust) runs this config's `vite build` only.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "./",
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    port: 5183,
    strictPort: true,
    // Agent worktrees and cargo output churn thousands of files the UI never imports.
    watch: { ignored: ["**/.claude/**", "**/src-tauri/target/**"] },
  },
  build: {
    target: "es2022",
    outDir: "dist",
    sourcemap: true,
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
    // Agent worktrees live under .claude/worktrees; their copies are not ours to run.
    exclude: ["**/node_modules/**", "**/.claude/**"],
    coverage: {
      provider: "v8",
      // Phase 1 gates the pure logic layers (state selectors, animation
      // mapping, the mock discovery API). Components are the scaffold and are
      // smoke-tested but excluded from the coverage floor.
      include: ["src/state/**", "src/lib/**", "src/api/**"],
      thresholds: {
        statements: 90,
        branches: 90,
        functions: 90,
        lines: 90,
      },
    },
  },
});
