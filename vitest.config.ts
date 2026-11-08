/// <reference types="vitest/config" />
import { configDefaults, defineConfig } from "vitest/config";
import { nodeModulesAllow } from "./vite.config";
import { WALL_CEILING_MS } from "./tests/test-support/wall-ceiling";

// 前端测试只有这一套：`tests/**/*.vitest.ts` 即自动纳入，分两组跑（下面 `projects`）。
// ── 两组：node · jsdom ─────────────────────────────────────────────────────────
// jsdom 环境每份文件要另花半秒上下建一份 DOM（10-31 量：空用例 50 份，jsdom 0.56 s/份、node 0.14 s/份）。
// 不碰 DOM 的文件放进 node 组省掉这一截。node 组收 `NODE_ENV_FILES` 这份明列的名单：逐份在 node 环境里跑过、
// 通过与跳过的条数和 jsdom 下一样才进。jsdom 组收全部、减掉这份名单 ⇒ 新文件默认在 jsdom（慢些，但不会因为没有
// `window` 红）；要进 node 组就把路径加进名单，测试文件里不写 `@vitest-environment`（判据扫着）。
// 判据：`tests/frontend/ui/vitest-env-split.vitest.ts`（两组合起来恰好是全部、不重叠、名单每条都在盘上、测试文件里没有环境指令）。
const NODE_ENV_FILES = [
  "tests/comms/inward/chan.vitest.ts",
  "tests/copy/copy-ledgers.vitest.ts",
  "tests/copy/copy-rules.vitest.ts",
  "tests/copy/copy-table.vitest.ts",
  "tests/copy/copy-terms.vitest.ts",
  "tests/copy/copy-test-literals.vitest.ts",
  "tests/copy/recognize-by-text.vitest.ts",
  "tests/copy/rotation-words.vitest.ts",
  "tests/copy/short-duration.vitest.ts",
  "tests/copy/tmux-session-prefix.vitest.ts",
  "tests/frontend/ui/account-availability-guard.vitest.ts",
  "tests/frontend/ui/account-reads.vitest.ts",
  "tests/frontend/ui/accounts.vitest.ts",
  "tests/frontend/ui/acct-center-changed.vitest.ts",
  "tests/frontend/ui/agent-profile-parity.vitest.ts",
  "tests/frontend/ui/alias-reads.vitest.ts",
  "tests/frontend/ui/answer-reads.vitest.ts",
  "tests/frontend/ui/apikey-reads.vitest.ts",
  "tests/frontend/ui/app-grid-claims.vitest.ts",
  "tests/frontend/ui/attach-draft.vitest.ts",
  "tests/frontend/ui/backend-deploy.vitest.ts",
  "tests/frontend/ui/backend-policy.vitest.ts",
  "tests/frontend/ui/bg-flat.vitest.ts",
  "tests/frontend/ui/cards/api-error.vitest.ts",
  "tests/frontend/ui/cards/first-line.vitest.ts",
  "tests/frontend/ui/cc-bus-control.vitest.ts",
  "tests/frontend/ui/cc-bus-hooks-reads.vitest.ts",
  "tests/frontend/ui/cc-bus-read.vitest.ts",
  "tests/frontend/ui/changed-stream.vitest.ts",
  "tests/frontend/ui/config-fields.vitest.ts",
  "tests/frontend/ui/config-persist-failure.vitest.ts",
  "tests/frontend/ui/config-unknown-keys.vitest.ts",
  "tests/frontend/ui/css-conventions.vitest.ts",
  "tests/frontend/ui/css-modules.vitest.ts",
  "tests/frontend/ui/design-tokens.vitest.ts",
  "tests/frontend/ui/end-confirm.vitest.ts",
  "tests/frontend/ui/entry-graphs.vitest.ts",
  "tests/frontend/ui/eslint-baseline.vitest.ts",
  "tests/frontend/ui/events-follow.vitest.ts",
  "tests/frontend/ui/events-tap-machines.vitest.ts",
  "tests/frontend/ui/exit-judgment-scan.vitest.ts",
  "tests/frontend/ui/exit-said-acts.vitest.ts",
  "tests/frontend/ui/export-readers-guard.vitest.ts",
  "tests/frontend/ui/ext-reads.vitest.ts",
  "tests/frontend/ui/front-exit.vitest.ts",
  "tests/frontend/ui/front-result.vitest.ts",
  "tests/frontend/ui/gate-vitest-workers.vitest.ts",
  "tests/frontend/ui/generated-boundary-guard.vitest.ts",
  "tests/frontend/ui/group-book.vitest.ts",
  "tests/frontend/ui/history-list-reads.vitest.ts",
  "tests/frontend/ui/history-reads.vitest.ts",
  "tests/frontend/ui/icons-one-source.vitest.ts",
  "tests/frontend/ui/import-graph-lint.vitest.ts",
  "tests/frontend/ui/ipc/decode.vitest.ts",
  "tests/frontend/ui/ipc/peer-version.vitest.ts",
  "tests/frontend/ui/ipc/typed-calls.vitest.ts",
  "tests/frontend/ui/ipc/write.vitest.ts",
  "tests/frontend/ui/judgment-single-home.vitest.ts",
  "tests/frontend/ui/keybindings/actions.vitest.ts",
  "tests/frontend/ui/kit/single-home.vitest.ts",
  "tests/frontend/ui/last-seen.vitest.ts",
  "tests/frontend/ui/launcher-diagnostics.vitest.ts",
  "tests/frontend/ui/live-one-cell.vitest.ts",
  "tests/frontend/ui/live-window.vitest.ts",
  "tests/frontend/ui/machine-commands.vitest.ts",
  "tests/frontend/ui/machine-context.vitest.ts",
  "tests/frontend/ui/machine-feed.vitest.ts",
  "tests/frontend/ui/morph-only.vitest.ts",
  "tests/frontend/ui/needs-book.vitest.ts",
  "tests/frontend/ui/needs-clock.vitest.ts",
  "tests/frontend/ui/node-suite-registry-guard.vitest.ts",
  "tests/frontend/ui/offline-cargo-cache.vitest.ts",
  "tests/frontend/ui/online-bypass-ledger.vitest.ts",
  "tests/frontend/ui/overlay-router.vitest.ts",
  "tests/frontend/ui/palette-names.vitest.ts",
  "tests/frontend/ui/paste-block-guard.vitest.ts",
  "tests/frontend/ui/pe-crt-imports.vitest.ts",
  "tests/frontend/ui/plan-needs-order.vitest.ts",
  "tests/frontend/ui/plan-needs-poke.vitest.ts",
  "tests/frontend/ui/plan-reads.vitest.ts",
  "tests/frontend/ui/port-forward-reads.vitest.ts",
  "tests/frontend/ui/profiles-reads.vitest.ts",
  "tests/frontend/ui/pubkey-push.vitest.ts",
  "tests/frontend/ui/quit-now.vitest.ts",
  "tests/frontend/ui/quota-read-golden.vitest.ts",
  "tests/frontend/ui/record-file-notice.vitest.ts",
  "tests/frontend/ui/record-reads.vitest.ts",
  "tests/frontend/ui/reduced-motion.vitest.ts",
  "tests/frontend/ui/release-notes-args.vitest.ts",
  "tests/frontend/ui/remote-config.vitest.ts",
  "tests/frontend/ui/remote-probe.vitest.ts",
  "tests/frontend/ui/resume-presets.vitest.ts",
  "tests/frontend/ui/resync.vitest.ts",
  "tests/frontend/ui/rot-drag.vitest.ts",
  "tests/frontend/ui/route-parity.vitest.ts",
  "tests/frontend/ui/said-object-registry.vitest.ts",
  "tests/frontend/ui/scanning-guard-registry.vitest.ts",
  "tests/frontend/ui/session-account-reads.vitest.ts",
  "tests/frontend/ui/session-open.vitest.ts",
  "tests/frontend/ui/session-reads.vitest.ts",
  "tests/frontend/ui/session-writes.vitest.ts",
  "tests/frontend/ui/settings/claude-dir-check.vitest.ts",
  "tests/frontend/ui/settings/interrupts.vitest.ts",
  "tests/frontend/ui/settings/machine-aliases-no-ccm-grammar.vitest.ts",
  "tests/frontend/ui/settings/settings-source-markdown.vitest.ts",
  "tests/frontend/ui/single-home-lint.vitest.ts",
  "tests/frontend/ui/single-line-css.vitest.ts",
  "tests/frontend/ui/skeleton-ledger-huge.vitest.ts",
  "tests/frontend/ui/skeleton-ledger.vitest.ts",
  "tests/frontend/ui/ssh-config-reads.vitest.ts",
  "tests/frontend/ui/startup-active.vitest.ts",
  "tests/frontend/ui/tab-bar-state.vitest.ts",
  "tests/frontend/ui/tab-batch-copy.vitest.ts",
  "tests/frontend/ui/tab-batch-run.vitest.ts",
  "tests/frontend/ui/tab-drop-default-name.vitest.ts",
  "tests/frontend/ui/tab-group-drop.vitest.ts",
  "tests/frontend/ui/tab-menu-auto-answer.vitest.ts",
  "tests/frontend/ui/tab-render-context.vitest.ts",
  "tests/frontend/ui/tab-switch-cost.vitest.ts",
  "tests/frontend/ui/tabs-copy-terms.vitest.ts",
  "tests/frontend/ui/tabs-split-graph.vitest.ts",
  "tests/frontend/ui/terminal-follow.vitest.ts",
  "tests/frontend/ui/terminal-open.vitest.ts",
  "tests/frontend/ui/terminal-reads.vitest.ts",
  "tests/frontend/ui/topbar-icons.vitest.ts",
  "tests/frontend/ui/ui-writes-no-channel-said.vitest.ts",
  "tests/frontend/ui/viewer-window-text.vitest.ts",
  "tests/frontend/ui/views/history-search.vitest.ts",
  "tests/frontend/ui/views/plan-rings.vitest.ts",
  "tests/frontend/ui/vitest-env-split.vitest.ts",
  "tests/frontend/ui/wire-tmux-words.vitest.ts",
  "tests/scripts/dep-advisories.vitest.ts",
  "tests/shots/browser-env.vitest.ts",
  "tests/shots/cdp.vitest.ts",
  "tests/shots/fake-no-core.vitest.ts",
  "tests/shots/host-tools.vitest.ts",
  "tests/shots/page-env.vitest.ts",
  "tests/shots/pb-stand-in.vitest.ts",
  "tests/shots/perf/world-split.vitest.ts",
  "tests/shots/proc-start.vitest.ts",
  "tests/shots/real-keys.vitest.ts",
  "tests/shots/tmux-stand-in.vitest.ts",
  "tests/shots/world-boxes.vitest.ts",
  "tests/test-support/component-sources.vitest.ts",
  "tests/test-support/copy-pattern.vitest.ts",
  "tests/test-support/non-empty.vitest.ts",
  "tests/test-support/strip-comments.vitest.ts",
];
// ── 🔴 显示时区与 locale 钉死在这里 ────────────────────────────────────────────
// 秤的语料（`tests/frontend/ui/scale2-height-corpus.ts`）每张卡都带时间戳，渲染走 `src/frontend/ui/format.ts` 的
// `toLocaleTimeString([], …)` —— `[]` 就是「跟这台机器走」：
//   · `TZ=America/Los_Angeles` / `zh-CN` 渲染出 `05:34`（12:34 UTC 换成 PDT）
//   · `TZ=UTC` / `en-US` 渲染出 `12:34 PM`
// **字面不同、字数也不同** ⇒ 照语料打的金样（骨架外框 `tests/evidence/skel-golden.json`）与各处字面断言只在钉住的这一对值下判得了；
// CI 的 runner 是 UTC/en-US，不钉就在云端红。`tests/evidence/skel-run.ts` 打金样时钉同一个 TZ。
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

// 门禁分核（`tests/scripts/gate.sh` 的「vitest 分核」那一段）：门禁里 `npm` 与 `coverage` 两格同时各起一整套 vitest，
// 各按默认起 worker 合起来近两倍核数 ⇒ 门禁给每格设 `GATE_VITEST_WORKERS`（核数 ÷ 格数），这里当 `maxWorkers`。
// CI 与平时不设 ⇒ 照 vitest 默认。设了却不是正整数 ⇒ 当场报错（静默落回默认等于这一手没生效）。
const GATE_WORKERS = process.env.GATE_VITEST_WORKERS;
if (GATE_WORKERS !== undefined && !/^[1-9]\d*$/.test(GATE_WORKERS)) {
  throw new Error(`GATE_VITEST_WORKERS=${JSON.stringify(GATE_WORKERS)} 不是正整数`);
}

export default defineConfig({
  server: { fs: { allow: nodeModulesAllow() } },
  test: {
    ...(GATE_WORKERS === undefined ? {} : { maxWorkers: Number(GATE_WORKERS) }),
    // 两组各自只写组名 · 环境 · 收哪些文件；余下的设定（期限 · css · setupFiles）写在这一层，`extends: true` 两组都吃。
    projects: [
      { extends: true, test: { name: "node", environment: "node", include: NODE_ENV_FILES } },
      {
        extends: true,
        test: {
          name: "jsdom",
          environment: "jsdom",
          include: ["tests/**/*.vitest.ts"],
          exclude: [...configDefaults.exclude, ...NODE_ENV_FILES],
        },
      },
    ],
    // 每条用例 / 钩子的上限是那一个兜底的数（`tests/test-support/wall-ceiling.ts`）：vitest 默认的 5 秒 / 10 秒在忙机上会撞
    // （10-11：`scale6-memory-ledger` 负载 95 下超时、单跑就绿）。用例里不再各写各的期限，判据 `wall_ceiling` 扫着。
    testTimeout: WALL_CEILING_MS,
    hookTimeout: WALL_CEILING_MS,
    // 模块 CSS 照产品那样处理：不处理时 `composes`（如 `composes: one-line from global`）不生效，jsdom 里的类名比产品少一截，
    // DOM 与产品对不上（秤 2 的 DOM 指纹就是这样逮到的）。
    css: { include: [/\.module\.css$/] },
    setupFiles: ["tests/test-support/pin-locale.ts", "tests/test-support/host-facts-default.ts"], // 默认 locale 钉在上面那行 `LANG` 上（Windows 那一半，见顶上）
    // F08b：覆盖率**设地板阈值（下方 thresholds）**——`npm run coverage` 与 CI 的 `coverage floor`
    // 步骤（ci.yml，**无 `|| true`=真·阻断门禁**）都吃它，低于地板即红。**不是** advisory、不是只报告。
    // 只设「地板」不追「85% 全局」：覆盖只统计 `src/**` 被 vitest 套件跑到的部分，
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
        // 而 `include` 只圈 `src/**` ⇒ 原先那几条（`*.vitest.ts` /
        // `test-support/**`）**已无对象可排**，删掉而不是留成死规则。
        // 留下的这两条仍有对象：`.d.ts` 与 `types.ts` 都住在 `src/` 里。
        "src/**/*.d.ts",
        "src/**/types.ts",
      ],
      // 地板棘轮（非追高目标）：设在当前值下方 ~2-3% 吸收环境/v8 版本差，只挡**明显回归**
      // （如新增大块无测代码）→ 故意不设 85% 全局。
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
