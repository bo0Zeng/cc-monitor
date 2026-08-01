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
        // 守卫专用的辅助模块：进不了 bundle（没有生产文件 import 它，
        // 由 `generated-boundary-guard.vitest.ts` 机检），所以也不该计入生产覆盖率地板。
        "src/test-support/**",
      ],
      // 地板棘轮（非追高目标）：设在当前值下方 ~2-3% 吸收环境/v8 版本差，只挡**明显回归**
      // （如新增大块无测代码）。注：只统计 `*.vitest.ts`；`*.test.ts`(tsx node) 不计入
      // → 故意不设 85% 全局。
      //
      // **棘紧记录（E81）——每次棘都在这里追一行，别只改数字：**
      // - 建立时：实测 S48.98 / B41.15 / F44.60 / L50.41 ⇒ 地板 40 / 34 / 36 / 41。
      // - **2026-08-01**：实测 **S54.44 / B45.25 / F48.85 / L55.96** ⇒ 地板棘到 52 / 43 / 46 / 53。
      //   棘之前裕度已经是 11-15 个点（**约 2000 条语句可以变成无覆盖而门禁不响**），
      //   注释里记的「当前值」也早已过期 —— 这是 Phase G 审计点名的那类病：
      //   **数字写下之后没人回来棘紧，等于把灵敏度慢慢交出去**。
      //   ⇒ 以后棘的时候连**实测值 + 日期**一起写，让「过期没过期」一眼可见。
      thresholds: {
        statements: 52,
        branches: 43,
        functions: 46,
        lines: 53,
      },
    },
  },
});
