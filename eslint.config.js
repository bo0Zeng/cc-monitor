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
      // C01：ts-rs 生成物，没人该手动去修它（Phase D 审计 S5）
      "src/generated/**",
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
  {
    // E83（2026-08-01）：`e2e/` 下那些 `.mjs`（wdio 配置、restart-shims、spec）**此前从没被 lint 过**。
    //
    // 病灶不是「它们脏」，是**作用面与配置意图对不上**：本文件的 `ignores` 明明是**仓级**的
    // （逐条列出 dist / node_modules / src-tauri / remote-daemon-proto / coverage），
    // 而 `npm run lint` 只跑 `eslint src` ⇒ 那份仓级意图从来没被兑现，
    // `npx eslint .` 是 46 个告警、比 `eslint src` 的 7 个多出 39 个，
    // **全在这几个文件里、全是 `no-undef: process/console/it`（纯缺一段 globals）**。
    // 也就是说：39 个告警永远不会被任何人看到，而它们一个真问题都不是。
    //
    // ⇒ 补上 globals（node + wdio 的 mocha 风格全局），并把 `npm run lint` 放开到 `eslint .`。
    // 补完实测：全仓 7 个，与 `eslint src` 的基线**一致** —— 基线数字不变，覆盖面变大。
    files: ["e2e/**/*.mjs"],
    languageOptions: {
      globals: {
        ...globals.node,
        ...globals.mocha, // wdio 的 describe/it/before（`e2e/tier2/**`）
        // `browser.execute(() => …)` 的**函数体在页面里跑**，所以 DOM 全局在这里是真实存在的
        // （不是漏声明）。wdio 自己注入的 `browser`/`$`/`$$` 同理。
        ...globals.browser,
        browser: "readonly",
        $: "readonly",
        $$: "readonly",
      },
    },
    rules: {
      // 与 `src/**` 同一条 `_`-前缀「有意不用」约定 —— 那条规则原本只挂在 `src/**/*.ts` 上，
      // 于是 shim 里刻意留的 `(_body, _opts)` 占位在这边会被判违规。约定该跟着仓走，不跟着目录走。
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
);
