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
  {
    // V7-3（2026-08-09 `/full-audit`）：`scripts/` 下的 `.mjs` **掉进了 E83 修过的同一个洞**。
    //
    // ⚠ 时间线是这条的全部要害（`git log` 实测，不是推的）：
    // E83 在 **07-31**（`a02f340`）把 `npm run lint` 从 `eslint src` 放开到 `eslint .`，
    // 当场实测「全仓 7 个，与 `eslint src` 的基线一致」；而 `scripts/assert-coverage-floors.mjs`
    // 是 **08-06**（`cab8a75`）才新建的 —— **晚 6 天**。它一进来就带 7 条 `no-undef`
    // （`console`/`process`，纯缺一段 globals），基线**从 7 静默变成 14**，
    // 而 `eslint.config.js` 与 `ci.yml` 里那两句「全仓实测仍是 7 项」**没人回来改**。
    //
    // ⇒ 这不是「E83 修漏了」，是**修法本身的形状问题**：`ignores` 是仓级的（全集），
    // 而 globals 是**按目录枚举**的（`src/**`、`e2e/**`）—— 于是每新增一个带脚本的目录，
    // 洞就复发一次，且**没有任何判据数着那个基线**（对比 shellcheck 的文件数被
    // `shell_lint_registry` 钉成等号、e2e 套数被 `e2e_gate_registry` 四份副本对拍）。
    // 补这一块只是止血；钉住「下次再有新目录」那半在 `eslint-baseline.vitest.ts`。
    //
    // ★ 最该记住的一点：那 7 条**长在覆盖率门禁自己的执行体上** ——
    // 替我们数别人的那个脚本，自己没被数。
    files: ["scripts/**/*.mjs"],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
);
