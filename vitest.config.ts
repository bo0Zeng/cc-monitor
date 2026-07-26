/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config";

// DOM 单元测试（jsdom 环境）。**只挑 *.vitest.ts**，与既有手写 node 测试（*.test.ts，
// 由 `node src/X.test.ts` 跑纯函数）分流，互不干扰。新增需 DOM/模块 mock 的测试写成
// `<name>.vitest.ts` 即自动纳入。
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.vitest.ts"],
    // F08b：覆盖率**只出报告 + 记基线**，不设 thresholds。原因：覆盖只统计本 vitest(jsdom)
    // 套件，`*.test.ts`（tsx node 纯函数测）不计入 → 85% 全局不现实（会误红）。地板棘轮/
    // per-file 目标留后续按核心 DOM 模块收紧。`npm run coverage` 本地看，CI advisory。
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
      // （如新增大块无测代码）。当前 vitest(jsdom) 套件：S41.85 / B36.48 / F38.07 / L42.98。
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
