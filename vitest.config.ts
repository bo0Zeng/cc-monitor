/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config";

// DOM 单元测试（jsdom 环境）。**只挑 *.vitest.ts**，与既有手写 node 测试（*.test.ts，
// 由 `tsx tests/X.test.ts` 跑纯函数）分流，互不干扰。新增需 DOM/模块 mock 的测试写成
// `<name>.vitest.ts` 即自动纳入。
// ── 🔴 显示时区与 locale 钉死在这里（09-19）────────────────────────────────────
// 起因：断网沙箱里 `tests/frontend/ui/scale2-height-truth.vitest.ts` 红，而宿主上它绿 —— 同一个提交、
// 同一份 node_modules。现打差异只有两条：宿主 `TZ=America/Los_Angeles` / `zh-CN`，
// 容器 `TZ=UTC` / `en-US`。
//
// 🔴 **秤 2 的语料 94 张卡全部带时间戳**，渲染走 `src/frontend/ui/format.ts` 的
// `toLocaleTimeString([], …)` —— `[]` 就是「跟这台机器走」。于是：
//   · 宿主渲染出 `05:34`（12:34 UTC 换成 PDT）
//   · 容器渲染出 `12:34 PM`
// **字面不同、字数也不同** ⇒ DOM 指纹全漂，而且**真高本身也会变**（宽度变了）。
// ⇒ 金标准把**打它那台机器的时区与 locale 一起烤了进去**，秤 2 因此只在这一台机器上判得了。
//
// ⚠ **这不是沙箱的毛病，是秤自己的毛病**：CI 的 runner 是 UTC/en-US，这条测试在云端
//   同样红（`ci.yml` 的 `unit tests` 那步跑 `npm test`，**无 `|| true`**）。
//   沙箱只是**第一个说出来的人** —— 宿主恰好就是打金标准那台机器，所以它永远绿。
//
// 🔴 **这两个值必须与金标准那一趟一致**（`tests/evidence/U-scale2-truth-golden.json`
//   的 `env` 戳）。改任何一个 ⇒ 金标准作废，要重打：`bash tests/evidence/U-scale2-run.sh`。
//   `tests/frontend/ui/scale2-height-truth.vitest.ts` 有一条判据逐字对拍这件事，改了不重打会当场红。
// ⚠ 在 `defineConfig` 之外、模块顶层设 —— worker 是**新进程**，从这里继承 env；
//   放进 `test.env` 太晚（ICU 的默认 locale 在进程启动时就定了）。
process.env.TZ = "America/Los_Angeles";
// 🔴 **写 `LANG` 的 POSIX 形，并把两个更高优先级的清掉** —— 三点理由，都是现打出来的：
//   ① `LC_ALL="zh-CN"`（BCP-47 形）Node 的 ICU 认，**但 bash / `locale` / 任何
//      `setlocale(LC_ALL,"")` 的程序不认** ⇒ 容器里当场
//      `warning: setlocale: LC_ALL: cannot change locale (zh-CN)` 并退回 C。
//      那是**往测试输出里灌噪声**，而且悄悄改了子进程的 collation/ctype。
//   ② `LC_ALL="zh_CN.UTF-8"`（POSIX 形）在**没生成这个 locale 的容器**里同样报警
//      （现打：`locale -a | grep -ci zh` == 0）。
//   ③ `LANG="zh_CN.UTF-8"` 两边都干净：Node 解析出 `zh-CN`，bash 零报警（现打）。
// ⚠ `LANG` 是**最低优先级**（`LC_ALL` > `LC_*` > `LANG`）⇒ 光设它不够，
//   一台把 `LC_ALL` 设成别的值的机器会把它盖掉 ⇒ **连带清掉那两个**，钉子才钉得住。
delete process.env.LC_ALL;
delete process.env.LC_TIME;
process.env.LANG = "zh_CN.UTF-8";
// ⚠ **Windows 的 ICU 不读 `LANG`**（取的是系统用户 locale）⇒ 光这一行，windows runner 上默认 locale 仍是 en-US。
//   JS 那一层由 `setupFiles` 里的 `tests/test-support/pin-locale.ts` 按这一行的值钉住（每个平台同一条路）；
//   `LANG` 留着管 ICU 本身与子进程。`TZ` 两边都认，不用另钉。

export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["tests/**/*.vitest.ts"],
    setupFiles: ["tests/test-support/pin-locale.ts"], // 默认 locale 钉在上面那行 `LANG` 上（Windows 那一半，见顶上）
    // F08b：覆盖率**设地板阈值（下方 thresholds）**——`npm run coverage` 与 CI 的 `coverage floor`
    // 步骤（ci.yml，**无 `|| true`=真·阻断门禁**）都吃它，低于地板即红。**不是** advisory、不是只报告。
    // 只设「地板」不追「85% 全局」：覆盖只统计本 vitest(jsdom) 套件，`*.test.ts`(tsx node) 不计入，
    // 全局高目标会误红——故用「当前值下方 ~2-3% 的地板」只挡明显回归。
    // ⚠ 「收紧留后续按核心 DOM 模块 per-file」那半**已经做了**（`tests/scripts/assert-coverage-floors.mjs`，
    //   F17 下半建语句地板、08- 2b 补上分支地板）—— 本行原先还写着「留后续」，
    //   做完之后没人回来改。留着不改就是下一处「未做自陈的腐坏」（audit-0805 F25 那一族）。
    coverage: {
      provider: "v8",
      reporter: ["text-summary", "json-summary"],
      include: ["src/**/*.ts"],
      exclude: [
        // 〔src/test 分离后〕测试与 test-support 已整体搬到 `tests/`，
        // 而 `include` 只圈 `src/**` ⇒ 原先那四条（`*.test.ts` / `*.vitest.ts` /
        // `test-support/**`）**已无对象可排**，删掉而不是留成死规则。
        // 留下的这两条仍有对象：`.d.ts` 与 `types.ts` 都住在 `src/` 里。
        "src/**/*.d.ts",
        "src/**/types.ts",
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
      // - **2026-08-06**（audit-0805 F17 下半）：实测 **S58.11 / B49.47 / F52.44 / L59.69**
      //   ⇒ 地板棘到 56 / 47 / 50 / 57。这一跳主要来自 `events.ts` 的批量调度状态机
      //   （53.33% → **84.44%**，branches 35.05% → **67.01%**）—— 那三条分支决定
      //   「整个重放期前端是 batch 还是 live」，其中 5 分钟防呆上限真机上多半从没跑过。
      //   ⚠ 棘之前裕度又攒到了 6 个点。**这条纪律要靠每轮顺手做，攒着就等于没有。**
      thresholds: {
        statements: 56,
        branches: 47,
        functions: 50,
        lines: 57,
      },
    },
  },
});
