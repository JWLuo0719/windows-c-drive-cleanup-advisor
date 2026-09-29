import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true
  },
  envPrefix: ["VITE_", "TAURI_"],
  test: {
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
    mockReset: true,
    restoreMocks: true,
    // 前端覆盖率门槛（Phase 1 Stage 5）：低于门槛即门禁失败。
    // 门槛取 2026-09-26 全量前端源码基线（语句 82.1 / 分支 79.6 / 函数 87.3 / 行 81.9）下方余量，
    // 防止覆盖倒退；新增逻辑请随用例一起补足。
    coverage: {
      provider: "v8",
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/test/**", "src/main.tsx", "src/vite-env.d.ts", "src/tauri.ts"],
      thresholds: {
        statements: 80,
        branches: 75,
        functions: 85,
        lines: 80
      }
    }
  }
});
