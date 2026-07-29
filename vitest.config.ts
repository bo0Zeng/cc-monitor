/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config";

// DOM 单元测试（jsdom 环境）。**只挑 *.vitest.ts**，与既有手写 node 测试（*.test.ts，
// 由 `node src/X.test.ts` 跑纯函数）分流，互不干扰。新增需 DOM/模块 mock 的测试写成
// `<name>.vitest.ts` 即自动纳入。
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.vitest.ts"],
    // F08b：覆盖率**设地板阈值（下方 thresholds）**——`npm run coverage` 与 CI 的 `coverage floor`
    // 步骤（ci.yml，**无 `|| true`=真·阻断门禁**）都吃它，低于地板即红。**不是** advisory、不是只报告。
    // 只设「地板」不追「85% 全局」：覆盖只统计本 vitest(jsdom) 套件，`*.test.ts`(tsx node) 不计入，
    // 全局高目标会误红——故用「当前值下方 ~2-3% 的地板」只挡明显回归。收紧留后续按核心 DOM 模块 per-file。
    coverage: {
      provider: "v8",
      reporter: ["text-summary", "json-summary"],
      include: ["src/**/*.ts"],
      exclude: [
        "src/**/*.test.ts",
        "src/**/*.vitest.ts",
        "src/**/*.d.ts",
        "src/**/types.ts",
      ],
      // 地板棘轮（非追高目标）：设在当前值下方 ~2-3% 吸收环境/v8 版本差，只挡**明显回归**
      // （如新增大块无测代码）。当前 vitest(jsdom) 套件：S48.98 / B41.15 / F44.60 / L50.41。
      // 注：只统计 `*.vitest.ts`；`*.test.ts`(tsx node) 不计入 → 故意不设 85% 全局。
      thresholds: {
        statements: 40,
        branches: 34,
        functions: 36,
        lines: 41,
      },
    },
  },
});
