/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config";

// DOM 单元测试（jsdom 环境）。**只挑 *.vitest.ts**，与既有手写 node 测试（*.test.ts，
// 由 `node src/X.test.ts` 跑纯函数）分流，互不干扰。新增需 DOM/模块 mock 的测试写成
// `<name>.vitest.ts` 即自动纳入。
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.vitest.ts"],
  },
});
