// F08b：ESLint flat config。**基线/顾问式**（同 CI 里 clippy 不强制）——先建门禁基础设施，
// 不追一次清零；既有告警作基线、`npm run lint` 本地看，CI 步骤 advisory 不阻断（见 ci.yml）。
// 非 type-checked 预设（不需 parserOptions.project）：快、且不因文件不在 tsconfig 里而报错。
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import globals from "globals";

export default tseslint.config(
  {
    // Rust、产物、依赖、覆盖率报告、各类 config 自身不 lint。
    ignores: [
      "dist/**",
      "node_modules/**",
      "src-tauri/**",
      "remote-daemon-proto/**",
      "coverage/**",
      "*.config.js",
      "*.config.ts",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    // 前端源码：浏览器全局（DOM/window/document）。
    files: ["src/**/*.ts"],
    languageOptions: {
      globals: { ...globals.browser },
    },
    rules: {
      // 对齐既有 `_`-前缀「有意不用」约定（tsconfig 的 noUnusedParameters 本就认它）——
      // 这是配置正确性、非改代码：不 lint 掉作者刻意留的占位绑定。
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
          destructuredArrayIgnorePattern: "^_",
        },
      ],
    },
  },
  {
    // 测试文件（tsx node 的 *.test.ts / jsdom 的 *.vitest.ts）：补 node 全局。
    files: ["src/**/*.test.ts", "src/**/*.vitest.ts"],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
);
