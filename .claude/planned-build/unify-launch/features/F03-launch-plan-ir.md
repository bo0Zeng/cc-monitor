# 功能计划 — F03 LaunchPlan IR + 双渲染器 + 维度注册表

> 对应主计划 §1 的 F03。本文件是该功能从规划到签收的全程记录。
> **动手前先读 MASTERPLAN §0 核心思想** —— 本功能是那个思想在代码结构上的落点：
> 「IR、CLI、UI 三处是同一个模型的三种投影」，本功能建的正是那个 IR。

## 0. 本计划的来源（Phase B 方法论说明）

按用户 2026-07-27 指示「具体决策由你开 agent 讨论分析后决定」，本计划出自两个独立 Plan agent
的架构方案综合，而非单一视角：
- **方案 A（增量优先）**：`LaunchContext`（输入）/`LaunchPlan`（输出）两型分离；`TmuxTarget`
  判别式对象；`EnvOp` 窄变体（`export-config-dir` 专用而非通用 `export`）；`cliFlags()` 返回
  `null` = 该维度在当前 plan 下无法用 CLI 语法表达，强制整条 plan 降级兜底。
- **方案 B（IR 一次到位）**：`transport`/`origin` 整体移出 IR；`quoting` 拆成 `name`+`raw|quoted`
  平级字段；`EnvOp` 通用变体；`LaunchContext` 与 `LaunchPlan` 合一。

两版都独立核实了同一组约束（e2e driver 位置参数、循环 import 风险、envReset 顺序），细节分歧
不大。**本计划采纳方案 A 的三处具体设计**，理由逐条写在 §2；采纳过程中额外核对 `shared/ccm`
实际行为，发现并修复了 R11（见 MASTERPLAN 变更记录 07，已 commit `ef1310b`，独立于本功能）。

## 1. 目标与验收标准（DoD）

- **目标**：把 7 个 builder + 6 个 executor 背后散落的命令拼接逻辑，收敛成一份结构化的
  `LaunchPlan` + 一个维度注册表 + 两个渲染器（CLI 优先、裸 shell 兜底），使"加一个新启动维度"
  从"改好几个文件"变成"注册一个维度"。

- **验收标准**：
  - [ ] `src/launch-plan.ts`：`LaunchPlan`/`LaunchContext` 类型 + `buildLaunchPlan()`
  - [ ] `src/launch-dimensions.ts`：4 个维度（identity/env-reset/account/nested-env-reset）+
        顺序不变量断言（模块加载即跑，错序直接 throw）
  - [ ] `src/session-backend.ts`：`SessionBackend` 接口改判别式入参 `TmuxTarget`，消灭字符串
        形状嗅探；`TMUX_BACKEND` 四个方法逐字节验证（手工对拍今天的输出）
  - [ ] `src/launch-render-fallback.ts`：兜底渲染器，7 个 builder 改造后**逐字节**产出与今天
        相同的命令串（`remote-launch.test.ts` 零编辑仍全绿）
  - [ ] `src/launch-render-cli.ts` + `src/ccm-probe.ts` + Rust `probe_ccm_cli`：CLI 渲染器 +
        探测缓存（5 分钟 TTL）+ 降级判据
  - [ ] `--print` 平价预言机测试：新驱动脚本对拍 `renderCli` 产出的 token 与本地
        `bash shared/ccm ... --print` 的真实解析结果（复用 F02 `ccm-cli.test.sh` 的对拍模式）
  - [ ] 7 个 builder / 6 个 executor **位置参数签名逐字不变**（`e2e/resume-cmd-driver.ts`、
        `e2e/restart-cmd-driver.ts` 经 `account-restart.ts` 传递性锁死的签名，零改动）
  - [ ] 门禁：`npm test` / `cargo test` / `tsc` / `test:tmux-target` / `test:ccm-cli` /
        `test:ccm-acceptance` 全绿 + 新增的对拍/顺序不变量测试

- **明确不做什么**：
  - 不做账号名→CLI flag 的映射（`ACCOUNT_DIMENSION.cliFlags` 本次恒返回 `null`，强制账号相关
    plan 走兜底渲染器）——今天调用方只有 `configDir`（目录路径），没有账号「名字」，`ccm --account`
    要的是名字。这是 **F05 的移交点**，不在本功能内提前假装解决。
  - 不做 F04 的三道门 / `@ccm_sid_expect` 仲裁。
  - 不做本地路径（F06）。
  - 不碰 daemon（`resolve_query.rs` 契约冻结不动）。

## 2. 与主计划的对接 + 对两版方案分歧的取舍（附理由）

**触及的共享面**：`src/session-backend.ts`、`src/remote-launch.ts`、`src/remote-launch-run.ts`、
`src/agent-profile.ts`（只读，不改）、`e2e/*-cmd-driver.ts`（约束，不改）。全部账本条目已在
MASTERPLAN §3 预写最终形态，本功能严格遵循。

**三处取舍**（采纳方案 A，理由）：

1. **`TmuxTarget = {kind:"raw"|"quoted"; value}` 判别式对象**（非方案 B 的 `name`+`quoting`
   平级字段）：`SessionBackend` 接口今天是单一 `target: string` 参数，改成一个对象类型是
   **最小 diff**——调用点从"传一个字符串"变成"传一个对象"，仍是一个参数；改成两个平级字段
   会让每个调用点从一处改动变成两处，且 `session-backend.ts` 内部要多处"解包两个字段"的样板
   代码，纯增加仪式感。
2. **`EnvOp` 窄变体**（`{kind:"export-config-dir";value}` 而非方案 B 的通用
   `{op:"export";key;value}`）：这不是品味问题，是**安全问题**——早前账号隔离审计（D7）已经
   点过"`extraEnv` 的 key 无校验 = 直接注入点"这条真实风险。通用 `export` variant 允许任何
   维度往里塞任意变量名（`PATH`/`BASH_ENV`/`LD_PRELOAD`……），而窄变体从类型层面就只能导出
   `CLAUDE_CONFIG_DIR`（且渲染时复用既有的 `isValidConfigDir` 专属校验）。未来真需要导出第二个
   变量，加一个新的具名 variant——这是一次刻意的、可审查的决定，不是一条默认打开的通用通道。
3. **保留 `transport: {kind:"local"}|{kind:"ssh"}` 无 payload 标记字段**（方案 B 主张整体移出
   IR）：F06（本地路径）落地时，维度/渲染器需要一个"这是本地还是远端"的判断依据；现在留一个
   零成本的标记字段，比 F06 时再给 `LaunchPlan` 加字段（哪怕是 additive）更省一次账本变更。
   `origin`（远端标签字符串）确认不进 IR——它是调度元数据不是渲染输入，继续作 `runLaunch`/
   `renderLaunchCommand` 的独立首参，对齐今天 6 个 executor"origin 打头"的既有惯例。

**两版方案都独立发现、且已被本功能计划吸收的关键约束**：
- `e2e/restart-cmd-driver.ts` 不直接 import builder，但它驱动 `account-restart.ts::restartWithAccount`
  第⑤步以位置参数调用 `runRemoteResumeTmux(origin, sessionId, cwd, launcher, tmuxName, configDir)`
  ——约束**传递性覆盖**到 `remote-launch-run.ts` 的这个 executor 签名，不只是 e2e 直接 import 的
  5 个 builder。本计划的薄适配器设计对此已显式处理（§4）。
- `src/remote-launch.test.ts` 直接 import 15 个符号（`CLAUDE_NESTED_ENV_VARS, posixQuote,
  isValidSessionId, sanitizeRemoteLauncher, buildResumeDirectCmd, buildResumeTmuxCmd,
  buildResumeIntoExistingTmuxCmd, pickFreshTmuxName, buildOpenTerminalCmd, isValidTmuxName,
  isValidNewTmuxName, buildAttachCmd, deriveTmuxName, buildLauncherCmd, buildEnvPrefix,
  isValidConfigDir`）——虽不是 prompt 里点名的硬约束，但重写它比重写 e2e driver 便宜得多、
  收益相同，本计划保证这 15 个符号继续从 `remote-launch.ts` 原样可 import。
- `shared/ccm` 没有「就地复用已存在 idle tmux、不新建」的模式（`buildResumeIntoExistingTmuxCmd`
  对应的 #76 修复）——CLI 渲染器对这类 plan **诚实放弃**（`canRenderCli` 直接返回 `false`），
  绝不近似渲染成幂等 create-or-attach（那会让 #76 以 CLI 路径的新形式复发，且现有回归测试测
  不到——见 §5 步骤⑤的强制单测）。

## 3. 接口 / 契约设计

### 3.1 `LaunchPlan` / `LaunchContext`

```ts
// src/launch-plan.ts
export type TmuxMode = "create-or-attach" | "send-into" | "attach-only";

export type LaunchContainer =
  | { kind: "none" }
  | { kind: "tmux"; name: string; nameQuoting: "raw" | "quoted"; mode: TmuxMode };

export type LaunchAction =
  | { kind: "new" }
  | { kind: "resume"; sid: string }
  | { kind: "attach"; name: string };

export type LaunchAccount = { kind: "account"; configDir: string } | { kind: "none" };

/** 有序、渲染时逐项原样吐出（不合并/不去重）——合并会破坏「与今天逐字节相同」：
 *  今天 `unset CLAUDE_CONFIG_DIR;` 与 `unset <嵌套env>;` 是两条独立语句，
 *  e2e 探针用 `grep -q "unset CLAUDE_CONFIG_DIR;"` 断言这个精确子串。 */
export type EnvOp =
  | { kind: "export-config-dir"; value: string }
  | { kind: "unset"; keys: string[] };

/** `(inner) => string`：包裹而非片段追加。F03 恒空数组，结构留给 F04（rbind 落点）。 */
export interface WrapSpec { id: string; order: number; wrap: (inner: string) => string }

export interface LaunchPlan {
  transport: { kind: "local" } | { kind: "ssh" };
  action: LaunchAction;
  container: LaunchContainer;
  cwd: string | null;
  env: EnvOp[];
  launcher: string;   // 用户可配原串，未 sanitize
  args: string[];
  identity?: { ccmSid: string };
  wrap: WrapSpec[];
}

/** buildLaunchPlan 的输入——调用方已解析好的具体意图，维度据此派生 env/args/identity。 */
export interface LaunchContext {
  transport: { kind: "local" } | { kind: "ssh" };
  action: LaunchAction;
  container: LaunchContainer;
  cwd: string | null;
  account: LaunchAccount;
  launcherOverride: string | undefined;
  ccmSid: string | undefined;
}

export interface LaunchDimension {
  id: string;
  order: number;
  applies(ctx: LaunchContext): boolean;
  apply(plan: LaunchPlan, ctx: LaunchContext): void;
  /** null = 该维度在当前 ctx 下无法用 CLI 语法表达 → 强制整条 plan 走兜底渲染器。 */
  cliFlags?(ctx: LaunchContext): string[] | null;
}
```

### 3.2 维度注册表（顺序即契约）

| id | order | 触发条件 | 效果 | cliFlags |
|---|---|---|---|---|
| `identity` | 5 | `ctx.ccmSid` 存在 | 校验 sid + 设 `plan.identity` | `["--ccm-sid=<sid>"]` |
| `env-reset` | 10 | 就地复用（`mode==="send-into"`）且无账号 | `unset CLAUDE_CONFIG_DIR` | `[]`（ccm 内部按 `--base` 自处理） |
| `account` | 20 | 账号存在 | `export CLAUDE_CONFIG_DIR=<dir>` | **恒 `null`**（见 §1"明确不做什么"） |
| `nested-env-reset` | 30 | `action` 是 `new`/`resume` | `unset <嵌套env>` | `[]`（ccm 内部按 agent 查表自处理） |

**顺序不变量**（模块加载即断言，见 §4 骨架代码）：`env-reset.order < account.order < nested-env-reset.order`。
这条顺序对应今天代码里"账号前缀在 unset 之前"的既有事实（`buildResumePayload` 逐字如此），
错序的后果是**静默账号被抹掉**——所以钉成开发期即崩的断言，而非留作注释纪律。

### 3.3 `SessionBackend` 判别式重构

```ts
// src/session-backend.ts（改动部分）
export type TmuxTarget = { kind: "raw" | "quoted"; value: string };

export interface SessionBackend {
  createRunAttach(args: {
    target: TmuxTarget; quotedCwd: string | null; quotedPayload: string; ccmSid?: string;
  }): string;
  attach(target: TmuxTarget): string;
  runInExistingAttach(args: { target: TmuxTarget; quotedPayload: string }): string;
}
```

`exactTarget(target: TmuxTarget)` 变成纯粹的 `switch(target.kind)`，不再对 `value` 的内容做
任何猜测。`kind` 的正确性由调用方（`launch-render-fallback.ts` 里从 `LaunchContainer.nameQuoting`
透传）保证——`nameQuoting` 本身由校验域决定：`planResumeTmux`/`planResumeIntoExisting` 用
`raw`（名字域收紧到 `[A-Za-z0-9_-]`，裸拼恒安全）；`planLauncher`/`planAttach` 用 `quoted`
（名字域允许空格等自由字符）。

### 3.4 双渲染器分工

```
                              ┌── renderCli(plan, ctx)   → "ccm resume <sid> --tmux=<名> ..."
LaunchPlan + LaunchContext ───┤    （每个维度重新问一次"这在 CLI 词汇里怎么说"）
                              └── renderFallback(plan)   → 今天逐字节相同的裸 shell 串
                                   （维度效果已摊平进 plan.env/args，直接编译）
```

`renderCli` 需要 `ctx` 而 `renderFallback` 只需要 `plan`——这个不对称是真实的：兜底渲染器把
维度效果**编译进文本**，CLI 渲染器把维度效果**翻译成 flag**，两种目标不同，理应各自向维度
问一次。`canRenderCli`：任一已触发维度的 `cliFlags(ctx)` 返回 `null`，或
`container.mode !== "create-or-attach"`（即 `send-into`/`attach-only` 之外的 idle 复用形态），
整条 plan 强制走兜底——**诚实放弃，不近似**（§2 已说明理由）。

**CLI 探测**（`src/ccm-probe.ts`）：惰性、按 origin、5 分钟 TTL、失败一律 fail-open 到"未装"
（探测是可用性判断不是安全边界，与 `isValidConfigDir` 这类必须 fail-closed 的校验不同）。
`behavior.ts` 加 `forceLegacyLaunchRenderer: boolean`（默认 `false`）手动兜底开关。

Rust 侧新增 `probe_ccm_cli`（`src-tauri/src/ccm_probe.rs`），命令串
`command -v ccm >/dev/null 2>&1 && ccm --ccm-probe || printf 'NO_CCM\n'`，照
`capture_remote_pane` 的一次性 headless SSH exec 范式，不涉及 daemon。

## 4. 实现步骤（严格顺序执行）

- [x] **步骤 1**：新建 `src/shell-quote.ts`（零依赖叶子模块）：从 `remote-launch.ts` 搬
      `posixQuote`/`isValidSessionId`/`isValidConfigDir`/`sanitizeRemoteLauncher`/
      `buildEnvPrefix`。`remote-launch.ts` 改为 `export {...} from "./shell-quote"`（15 个符号
      的 import 面零改动）。
      — 验证：`npm test`（含 `remote-launch.test.ts`）+ `tsc` 全绿；diff 应只有函数体搬家。
- [x] **步骤 2**：改 `src/session-backend.ts` 为 `TmuxTarget` 判别式（§3.3），同步改
      `src/session-backend.test.ts` 的调用字面量（**只改入参形状，断言字符串不变**）。
      — 验证：手工核对两条关键路径的字节等价（裸名 `cc-p1` / 已引号 `'my session'`）；
      `npm test` + `npm run test:tmux-target`（26 项，本步骤唯一触及真实 tmux 语义的地方）全绿。
- [x] **步骤 3**：新建 `src/launch-plan.ts`（类型 + `buildLaunchPlan`）+
      `src/launch-dimensions.ts`（4 个维度 + 顺序不变量断言，故意造错序验证它真的 throw）。
      纯新增，不接触 `remote-launch.ts`。
      — 验证：新增单测覆盖每个维度的 `applies`/`apply`/`cliFlags` 独立行为 + 顺序断言。
- [x] **步骤 4**：新建 `src/launch-render-fallback.ts`（`renderFallback`）。**在改
      `remote-launch.ts` 之前**，写一个 scratchpad 对拍脚本：覆盖 `remote-launch.test.ts` 里
      每条边界（空 cwd、含空格/单引号的 cwd、自定义 launcher、launcher 注入、有/无 configDir、
      显式/默认 tmux 名）+ 额外几组（configDir 含反斜杠、cwd 全空白），对同一输入分别喂给
      "今天仍在的旧 builder"和"`planXxx → renderFallback`新管线"，逐字符 diff 清零。
      **清零之前不进入下一步**。
- [x] **步骤 5**：新建 `src/launch-requests.ts`（`planResumeDirect`/`planResumeTmux`/
      `planResumeIntoExisting`/`planLauncher`/`planAttach`，每个返回 `{ctx, plan}`）；改写
      `remote-launch.ts` 的 7 个导出为薄适配器（`buildOpenTerminalCmd`/`pickFreshTmuxName`/
      `deriveTmuxName` 原样保留，不进 IR——见 §1"明确不做什么"之外的边界：这三者无账号/tmux
      维度可言，硬套 IR 零收益）。
      — 验证：`npm test`（`remote-launch.test.ts` 零编辑仍全绿）+ `npm run test:tmux-target`。
- [x] **步骤 6**：新建 `src/launch-render-cli.ts`（`renderCli`/`canRenderCli`）+
      `src/ccm-probe.ts` + `src-tauri/src/ccm_probe.rs`（含 Rust 单测：真实 probe 输出解析、
      `NO_CCM`/无关同名程序判定为未装）。**强制单测**：任何 `container.mode !== "create-or-attach"`
      的 plan，`canRenderCli` 必须返回 `false`（防 #76 以 CLI 形式复发，先写这条断言再写实现）。
      新增 `--print` 平价预言机测试（复用 F02 `ccm-cli.test.sh` 的跨语言对拍模式）。
      — 验证：`cargo test`；本地跑 `bash shared/ccm --ccm-probe` 核对 TS 解析吃得下真实输出。
- [x] **步骤 7**：改 `src/remote-launch-run.ts`：6 个 executor 收敛调用 `renderLaunchCommand`
      （`forceLegacyLaunchRenderer` 时短路兜底）；`account-restart.ts`/`tabs.ts`/`history.ts`
      **零改动**（executor 对外签名不变，`git status` 核对确认零 diff）。
      `behavior.ts` 加 `forceLegacyLaunchRenderer: boolean`（默认 `false`）+ `settings/panel.ts`
      的 `onBehaviorToggle` 补一个私有字段原样带回该值（面板无 UI 暴露，但每次保存都要交一份
      完整 `BehaviorConfig`——发现 tsc 抓到这个漏洞：面板原先手搓字面量，加字段后编译报错，
      顺手核实了"不带回就会被任一勾选框改动悄悄重置成默认值"这条真实风险，已修）。
      — 验证：`bash e2e/resume-suite.sh`（17/17）+ `bash e2e/restart-suite.sh`（24/24）绿（两者
      的 shim 对未知 `invoke`——即新的 `probe_ccm_cli`——一律走 `default` 分支且返回 `undefined`，
      `probeCcm` 内部访问 `raw.installed` 抛错被其自身 try/catch 捕获 fail-open 到"未装"，天然
      只覆盖兜底渲染器路径，已核对不影响任何 `grep -qx` 断言）。三种降级路径（探测未装/账号存在/
      idle-tmux 复用）已在步骤 6 的 `canRenderCli` 单测直接覆盖，此处不重复造 mock-invoke 单测
      （`renderLaunchCommand` 本体是 3 行胶水，逻辑已在别处测过，e2e 提供端到端证据即可）；额外
      修了 `remote-launch-run.vitest.ts` 一个真实回归：该文件用单一 `invoke` mock 队列
      （`mockResolvedValueOnce`/`mockRejectedValueOnce`）配 `launch_remote_terminal` 的行为，
      新增的 `probe_ccm_cli` 调用抢先消耗了队列，导致两个用例假绿/假红——改按 cmd 路由
      （`probe_ccm_cli` 恒答未装，`launch_remote_terminal` 才落到用例配的行为）。
      全量门禁：`tsc`0 / `npm test`598 / `cargo test`374 / `test:tmux-target`26 / `test:ccm-cli`36 /
      `test:ccm-acceptance`12 / `test:ccm-print-parity`9，全绿（结果落盘 Read 核实）。
- [x] **步骤 8**：`doc/INVARIANTS.md` 新增 §33（LaunchPlan 双渲染器诚实边界的不变量）；
      MASTERPLAN §1 F03 状态改「实现完成，待双 agent 审」，§3 账本新增/更新 5 行（`session-backend.ts`
      /`launch-plan.ts`+`launch-dimensions.ts`/双渲染器+ccm-probe/`remote-launch.ts`/
      `remote-launch-run.ts`/`behavior.ts`）反映落地形态；两个独立 agent（后端架构 + UX）双审
      已跑完，prompt 自包含带 §0 核心思想全文——结论见下方 §6/§7。
- [x] **步骤 9**：全量门禁（`tsc`0/`npm test`606/`cargo test`374/`test:tmux-target`26/
      `test:ccm-cli`36/`test:ccm-acceptance`12/`test:ccm-print-parity`9/`e2e/resume-suite.sh`17/
      `e2e/restart-suite.sh`24），结果重定向落盘后 Read 核实，`git status` 核对
      `account-restart.ts`/`tabs.ts`/`src/views/history.ts` 零 diff。

## 5. 测试策略

- **黄金串对拍**：步骤 4 的一次性 scratchpad 脚本是"逐字节相同"这条硬约束的直接验证，做完
  即弃（不进仓库）；`remote-launch.test.ts` 本身零编辑，跑绿即是长期回归证据。
- **顺序不变量单测**：`env-reset < account < nested-env-reset`，故意造错序验证真的 throw。
- **`canRenderCli` 的 #76 防线**：任何 `mode!=="create-or-attach"` 必须强制走兜底——先写失败
  测试再写实现（回归纪律）。
- **`--print` 平价预言机**：新驱动脚本，`renderCli` 产出的 flag 值必须全部出现在
  `bash shared/ccm ... --print` 的真实展开结果里——这是唯一能在没有真远端机器的场景下验证
  "CLI 渲染器真的会让 ccm 干对事"的手段。
- **回归**：`e2e/resume-suite.sh`/`restart-suite.sh`（覆盖兜底路径）+
  `test:tmux-target`/`test:ccm-cli`/`test:ccm-acceptance`（F01/F02 遗留，必须持续全绿）。

## 6. 代码审计结果（Phase D）

两个独立 agent 并行审（prompt 自包含，各带 MASTERPLAN §0 核心思想全文），均**无阻塞项**：

**后端架构 + 正确性 agent**：本地重跑全部门禁与 F03 计划记录的数字完全一致；
`remote-launch.test.ts`/`account-restart.ts`/`tabs.ts`/`src/views/history.ts` 经 `git diff --stat`
确认零改动。发现两条重要项：
1. **`canRenderCli` 的 #76 闸门误伤 `attach-only`**——`mode!=="create-or-attach"` 同时挡住了
   `send-into`（真正的风险场景）和 `attach-only`（`ccm attach <名>` 经核对 `shared/ccm` 源码就是
   `exec tmux attach -t "=$名:"`，与兜底渲染器逐字同构，无 #76 歧义），导致 `renderCli` 的 attach
   分支与 `CLI_REQUIRED_CAPS` 的 `"attach"` 能力项在生产路径上永不可达，"单一渲染目标"这条推论
   在 attach 动作上事实上没有兑现。**已修**：闸门收窄为只挡 `mode==="send-into"`；
   `launch-render-cli.ts`/`launch-render-cli.test.ts`/`doc/INVARIANTS.md §33`/
   `remote-launch-run.ts` 的 `runRemoteAttach` 文档注释同步更新；补 2 条测试
   （`attach-only + FULL_CAPS → true`、`attach-only + NOT_INSTALLED → false`），验证过
   `shared/ccm` 真实实现后确认此修复安全，全量门禁重跑仍绿。
2. 计划文件步骤⑧⑨当时仍显示 `[ ]` 未勾选但内容已完成——记录滞后，已补勾（见上）。
   另两条「建议」级别（tmux 名校验正则长度上限不一致——F03 之前已如此、非本次回归；
   `ACCOUNT_DIMENSION.cliFlags` 说明在四处重复但一致）判定为不影响签收，不处理。

架构核心思想符合度结论（agent 原话精神）：维度注册表与两个渲染器之间没有硬编码的维度 ID
switch，加第 5 个维度确实可以做到"注册一条 + 渲染器零改"；顺序不变量是模块加载期断言、真正
load-bearing；三条降级路径全部诚实放弃而非近似渲染。骨架设计可信、正交，值得在此基础上继续
F04/F05。

**UX agent**：Job A（零回归）与 Job B（F09 前瞻兼容性）均无阻塞。两条重要项：
1. **container/agent 轴不经维度注册表**——只有 environment 轴（account/env-reset/
   nested-env-reset/identity）享受"加维度=+1行注册"的收敛红利，与 MASTERPLAN §2.6 把
   `container=tmux|none` 与 `account=X` 列为同级 flyout 修饰的 UI 设想之间有一个此前只存在于
   源码注释、未写进风险表的不对称。**已处理**：登记为 §6 风险表 **R12**，明确留给 F09 Phase B
   自己决定"是否把 container/agent 也收进维度注册表"，不在 F03/F04/F05 预判。
2. **toast 文案单测覆盖不对称**——只有 `runRemoteResume` 有直接的 toast 文案单测断言，其余
   4 个 executor 靠间接覆盖（e2e/mock 整个模块）。**已处理**：`remote-launch-run.vitest.ts`
   新增 8 条 smoke test（4 个 executor 各 1 条成功 + 1 条失败），全部通过。

## 7. 工程审计结果（Phase E）

主线程对账（读 MASTERPLAN §3/§6 + 本功能计划）：F03 落地后主计划仍自洽——账本 5 行已更新反映
最终形态（`session-backend.ts`/`launch-plan.ts`+`launch-dimensions.ts`/双渲染器族/
`remote-launch.ts`/`remote-launch-run.ts`/`behavior.ts`）。唯一需要现在就做的"优雅统一重构"
（账本预见的重叠）是 `remote-launch-run.ts` 的剪贴板回退——已在本轮实现阶段顺手做掉
（`invokeLaunchOrCopyFallback` 单一实现，6 处调用点只传文案），不留给 F05/F06 打补丁。
唯一未做到账本原定最终形态的一点：`remote-launch-run.ts` 保留 6 个具名 executor（未合并成单一
`runLaunch(plan)`）、返回值仍 void/boolean 混合——这与"executor 位置参数签名逐字不变"硬约束
互斥（`account-restart.ts` 按名调用、经 `restart-cmd-driver.ts` 传递性锁死），已在账本该行显式
记录为有意保留，若 F05/F06/F09 真要做"单一入口"，须先解决四个下游文件的联动改动，不建议顺手做。
无新增技术债留给后续功能背负；R12（container/agent 轴不对称）是唯一转发给 F09 的开放决策，
且已有明确处理路径（F09 Phase B 自行决定），不是模糊的"以后再说"。

## 8. 签收（Sign-off）

- [x] 通过代码审计（无阻塞项；2 条重要项已修复：#76 闸门误伤 attach-only、计划记录滞后）
- [x] 通过双 agent 架构/UX 审（无阻塞项；2 条重要项已处理：R12 风险登记、toast 测试补齐）
- [x] 通过工程审计（主计划仍自洽；剪贴板回退已提前统一；唯一遗留决策 R12 已转发 F09 且有明确路径）
- [x] 主计划已据此更新（§1 状态、§3 账本 5 行、§6 新增 R12、§7 变更记录见下）
