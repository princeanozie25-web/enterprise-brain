import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    include: ["tests/**/*.test.ts", "tests/**/*.test.tsx"],
    setupFiles: ["tests/setup.ts"],
    css: {
      // CSS-module class names resolve to their source names so the iris
      // classes are assertable (U-7).
      modules: { classNameStrategy: "non-scoped" },
    },
  },
  resolve: {
    alias: { "@": path.resolve(__dirname, "src") },
  },
});
