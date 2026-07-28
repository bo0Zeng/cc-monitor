# 主计划 / MASTERPLAN — 统一会话启动架构（unify-launch）

> 所有功能宏观设计的**单一事实来源**。跨功能的任何决策以此为准。
> 每次修订都在末尾「§7 变更记录」追加一行。
>
> **前身**：`account-onboarding/`（F5 部署 9d3a7e6、F1 切号入口 7680a43 已交付并保留）。
> 其 `MASTERPLAN-v2.md` 经四视角 full-audit 判定**不可直接执行**（见 `account-onboarding/AUDIT-v2-FINDINGS.md`，
> 其编号 C1-C7 / D1-D9 / E1-E9 / P1-P3 在本文件中反复引用，仍是权威依据）。

---

## 0. 核心思想（先读这一节）

> **把「起一个会话」从一堆写死的命令，变成「一个动作 + 若干正交修饰」；
> 并让这个模型在 IR、CLI、UI 三处是同一个——
> app 的菜单项、终端的命令行参数、代码里的 IR 字段，是同一模型的三种投影。**

五条推论（任何设计决策与它们冲突，就是设计错了）：

1. **正交，不是笛卡尔积。** 今天 15 套实现、10 个 resume 菜单项，根源是把 N 个正交维度
   （直连/tmux × 账号/基座 × 本地/远端 × 新建/恢复）做成了组合展开。
   改成「动作 + 修饰」后，加第 N+1 个维度是 **+1 而不是 ×2**。
2. **单一渲染目标。** cc-monitor 不直接渲染裸 shell，而是渲染成对同一个 CLI 的调用。
   终端用户敲的和 app 发出去的是**同一条命令**——「app 行为 = 终端行为」是结构保证，不是同步纪律。
3. **自定义在组合层，不在实现层。** 用户自定义 = 给一个组合起别名（`cct = ccm --tmux`），
   而不是自己写一个新实现。便利完全保留，但不再产生新的越层实现。
4. **向下兼容 = 少传几个参数**，不是第二条代码路径。单账号 = 不传 `--account`；
   CLI 未装 = 兜底渲染器输出与今天**逐字节相同**的载荷。
5. **身份随行。** `@ccm_sid` 由 CLI 统一负责，无论从 app 起还是从终端起，cc-monitor 都能无缝识别。
   这直接消灭「本工具起的会话自己认不出」这个最致命的根因。

---

## 0.1 目标与范围

- **总体目标**：把「起一个会话」在整个软件里收敛成一条路径，并把这条路径**同时暴露给终端**。
- **范围内**：
  - 远端会话 启动 / 恢复 / attach / 换号重启 的全部路径（前端 `remote-launch*` + Rust `tmux.rs`）。
  - 本地会话 启动 / 恢复 路径（Rust `history.rs` 的 PowerShell 两套）。
  - **统一启动 CLI（`ccm`）**：终端侧与 app 侧共享的唯一实现。
  - 账号维度（`CLAUDE_CONFIG_DIR`）在上述所有路径上的一致注入。
  - 会话身份（`@ccm_sid`）的产生、回填、仲裁与消费。
  - 上述统一之后的 UI 收敛（动作 × 修饰）。
- **范围外**：
  - daemon 侧协议/实现改动（审计 P3 已逐条判定本轮零改；`TMUX_LS_FMT` 双写点不动）。
  - 代替用户修改 `~/.bashrc`（只检测 + 生成 + 引导；删用户自己写的块需显式确认）。
  - 新增任何轮询。发版 / bump / push。
- **整体成功标准**：
  1. `INVENTORY.md` 表里每个「起会话」入口都能带账号、带 tmux、带未来参数，且**行为一致**。
  2. 加一个新启动维度 = 注册一个 dimension + CLI 加一个 flag + UI 加一个修饰项，
     **零改** builder / renderer / 调用点。
  3. 单账号（不传 `--account`）载荷与今天**逐字节相同**（P0 的 tmux 目标修正是唯一有意变更）。
  4. 终端敲 `ccm --tmux --account z` 起的会话，cc-monitor **无缝识别**（有 `@ccm_sid`、可 attach、可重启）。

---

## 1. 功能清单（Feature Inventory）

| ID | 功能 | 一句话目标 | 状态 | 依赖 | 优先级 |
|----|------|-----------|------|------|--------|
| F01 | tmux 目标精确匹配 | 所有 `-t` 用 `=name:`，修今天正在杀错会话的生产 bug | **完成** | — | P0 |
| F02 | 统一启动 CLI `ccm` + 重构 bashrc | 「一个动作 + 正交修饰」在 shell 侧的实现；app 与终端的共同渲染目标；**旧 4 个 block（187 行）整体取代** | **完成** | — | P0 |
| F03 | LaunchPlan IR + 双渲染器 + 维度注册表 | 结构化启动意图；主渲染器 → CLI，兜底渲染器 → 今天的裸 shell | **完成** | F02 | P1 |
| F04 | 会话身份统一（@ccm_sid） | 本工具/CLI 起的会话必有身份；三道门取代名前缀白名单；**须根治「一个 sid 匹配多个活会话」**（见下） | **完成** | F02,F03 | P1 |
| F05 | AccountResolver | 账号解析收敛成判别联合，注入源过 `isSelectable` | **完成** | F03 | P1 |
| F06 | 本地路径并入 IR | Rust 两套 PowerShell builder 收进同一意图模型 | 完成 | F03 | P2 |
| F07 | 每账号默认模型 | 维度注册表的**架构验收**（第一个真实新维度） | 完成 | F03 | P2 |
| F08 | 终端集成收尾 | CLI 安装向导 + 别名生成 + 越层启动器诊断 + 旧 swap 退役 | 完成 | F02,F04 | P2 |
| F09 | UI 收敛：动作 × 修饰 | 10 个 resume 入口 → 1 动作 + 修饰 flyout；徽章常显；删对齐全套 | **完成**（Phase B→F 全过，R12 降级见 `features/F09-ui-convergence.md`；双 agent 审计 1 阻塞+5 重要，全部修复） | F05 | P2 |
| F10 | 剩余账号 UX | 面板砍卡片 / 加号一键化 / 用量（plan 窗口 %） | 待规划 | F09 | P3 |
| F11 | cc-bus 集成（`cc-spawn` 并入 `ccm`） | `~/.local/bin/cc-spawn` 是第三套独立 tmux 启动实现，收编；预信任能力**上提**进 `ccm` 核心 | 完成（范围收窄——只上提，不改 cc-spawn 本体，见 F11 计划 §1） | F02 | P2 |

---

## 2. 架构概览（Architecture Map）

### 2.1 病因：正交维度被做成了组合展开

「启动一个会话」实际是六层的合成，今天被**压平成一个字符串**：

```
L0 传输    本地 wt.exe/PowerShell        远端 ssh -t host "bash -lic '<payload>'"
L1 容器    none | tmux(建/接入/送入)      ← 会话名、attach 语义
L2 环境    cwd + 会话级 env + unset 嵌套env
L3 启动器  claude | cc | ccm | cct | 用户自定义
L4 参数    --resume <sid> | 未来任意
L5 身份    @ccm_sid 打标 / rbind 回填
```

**决定性证据（2026-07-27 真机实测）**：用户配 `resumeCommandRemote = cct`。cc-monitor 发出

```
export CLAUDE_CONFIG_DIR='/home/zbl/.claude-accts/z'; unset <嵌套env>; cct --resume <sid>
```

而 `cct` 内部 `tmux new-session -d -s <sess>` + `send-keys "ccm --resume <sid>"`。
send-keys 打进的是 **tmux server fork 出来的全新 shell**，环境来自 tmux server 全局环境。
实测 `update-environment` 默认列表为
`DISPLAY KRB5CCNAME MSYSTEM SSH_ASKPASS SSH_AUTH_SOCK SSH_AGENT_PID SSH_CONNECTION WINDOWID XAUTHORITY`
——**不含 `CLAUDE_CONFIG_DIR`**。那句 export 在 L1 边界上被整个吃掉。

> **账号注入的成败，只取决于 `export` 落在 tmux 进程边界的哪一侧。**
> 这解释了为什么全 app 九个「选账号」入口只有两个生效：
> 「设置→开新 Claude」（cc-monitor 自建 tmux，export 写在 send-keys 载荷**内**）
> 与「设置→账号→登录」（`cc-acct-iso run <名>` 在同一 shell 内设好 env 再 exec）。

**但根治办法不是"禁止用户越层"**，而是让 L1/L2 **本来就是参数**——见 2.2。

### 2.2 解法：统一启动 CLI（`ccm`）= 模型的 shell 侧投影

单一契约，app 与终端共用：

```
ccm [动作] [修饰…] [-- 透传给 agent 的参数]

动作   （缺省 = new）
  new                       起新会话
  resume <sid>              恢复会话（`--resume <sid>` 是等价别名，见下）
  attach <名>               接回已有会话
修饰   （全部正交、可任意组合、可缺省；缺省即今天的行为）
  --tmux[=<名>]             容器维度。缺省名 = `cc-<safe-basename>`；**同目录幂等接回**同一会话，
                            不做撞名避让（避让属于「灰会话 fresh resume」，由调用方显式传名）
  --account <名> | --base   账号维度（--base = 显式不隔离，issue #75 逃生口）
  --cwd auto|<dir>          工作目录。**auto = 复刻 `_cc_resolve_target`**（$HOME→工作区 /
                            git 仓→父目录 / 否则当前目录），默认 auto，行为逐字节保持
  --agent claude|codex      agent 轴（resume flag / 默认启动器 / 嵌套 env / 身份回填能力）
  --launcher <cmd>          覆盖该 agent 的默认启动命令
  --ccm-sid <sid>           身份打标（cc-monitor 已知 sid 时传）
  <未来维度>                 --model X / --proxy Y / …
自省   （给 cc-monitor 与调试）
  --print                   只打印将要执行的命令，不执行
  --ccm-probe               打印 name/version/capabilities（安装自检 + 降级判据）
```

**`--resume <sid>` 必须与 `resume <sid>` 等价**——这样 F02 一落地，用户把设置里的
「远端 resume 命令」填成 `ccm`，**cc-monitor 前端零改动**就已经正确（它自己会在后面拼
`--resume <sid>`）。这是 §0 推论④「向下兼容 = 少传参数」的具体兑现，也让 F02 独立可验。

用户自定义 = **给组合起别名**，不是写新实现：

```bash
cc()   { ccm "$@"; }
cct()  { ccm --tmux "$@"; }
zcct() { ccm --tmux --account z "$@"; }
oot()  { ccm --tmux --agent codex "$@"; }
```

> **命名说明**：规范命令名取 `ccm` 而非 `cc`——`cc` 在 Linux 上是 C 编译器（`/usr/bin/cc`），
> `exec cc --resume <sid>` 会起一个编译器（审计 D4 已指出）。旧的 `ccm()` bashrc 函数**被本 CLI
> 整体取代**（不共存），故无遮蔽问题（实测：shell 函数优先于 PATH，若共存则新 CLI 一次都跑不到）。
> 装成 `~/.local/bin/ccm` **可执行文件**而非 shell 函数：与 shell 无关，zsh/fish 用户同样可用
> （审计 D2 指出远端是 zsh/fish 时 `.bashrc` 根本不被 source）。

**会话命名统一**（顺带消灭 INVENTORY 记的「4 套命名规则」）：CLI 与 cc-monitor 的
`deriveTmuxName` 同规则产出 `cc-<safe-basename>`。于是终端 `cct` 与 app「开新 Claude」在同一目录
**造出同一个名字** → 幂等短路 = 接回同一个会话（这正是期望行为）。agent 类别写进 tmux option
`@ccm_agent`，不进名字——名字不承担身份职责（F04）。前缀仍是 `cc-` 以过今天的
`is_ccm_tmux_name`，使 F02 在 F04 之前就能被控制面接受。

**CLI 独占实现 L1/L2/L5**（建 tmux、设 env、打 `@ccm_sid`、exec agent），于是：

- **终端**：用户敲 `ccm --tmux --account z` → 环境在 tmux **内**设好 → 账号生效 → 身份打好 → cc-monitor 无缝识别。
- **app**：cc-monitor 把 `LaunchPlan` 渲染成同一条 `ccm …` 命令 → 与终端**逐字符同源**。
- 层所有权冲突**消失**：启动器不再"拥有"容器，而是**接受**容器作为参数。

### 2.3 双渲染器（向下兼容的落点）

```
                        ┌── CLI 渲染器（主）→  ccm resume <sid> --tmux=cc-ab12 --account z
LaunchPlan IR ──────────┤
                        └── 裸 shell 渲染器（兜底）→ 今天逐字节相同的载荷
```

- 远端探到 `ccm` 已装（且版本兼容）→ 走 CLI 渲染器。
- 未装 / 版本旧 / 探测失败 → 走兜底渲染器，行为与今天**逐字节相同**。
- 兜底渲染器**本来就必须存在**（它就是 §0.1 成功标准 3 的实现），所以"两个渲染器"不是重复实现，
  而是「新路径 + 兼容路径」，且共用同一份 IR 与同一套维度。

### 2.4 三条正交轴

沿用仓库既有范式（`session-backend.ts:6-9` 已有 agent × backend 两轴），本计划补第三条：

- **agent 轴**（哪个 AI）：`AGENT_PROFILE` / `adapter::active()` — resume flag、默认启动器、嵌套 env。
- **backend 轴**（哪个容器）：tmux / none。
- **environment 轴**（哪套环境）：**本计划新增** — account / model / 未来维度。

### 2.5 关键接口

```ts
// —— 启动意图 IR ——
interface LaunchPlan {
  transport: { kind: "local" } | { kind: "ssh"; origin: string };
  action: { kind: "new" } | { kind: "resume"; sid: string }
        | { kind: "attach"; name: string } | { kind: "restart"; sid: string };
  container:
    | { kind: "none" }
    | { kind: "tmux"; name: string; mode: "create-or-attach" | "send-into" | "attach-only" };
  cwd: string | null;
  env: Record<string, string>;   // 会话级；key 过白名单 + denylist（审计 D7）
  unsetEnv: string[];
  launcher: string;              // 用户可配；经 sanitize，wrap 必须在 sanitize 之后（审计 D3）
  args: string[];                // 未来维度往这里加
  identity?: { ccmSid: string };
  wrap: WrapSpec[];              // 有序**嵌套**（order = 嵌套深度），表达 `( … ; exec X )`
}

// —— 维度注册表（可扩展性的唯一落点）——
interface LaunchDimension {
  id: string;                    // "account" | "model" | "nested-env-reset" | "identity" | …
  order: number;
  applies(ctx: LaunchContext): boolean;
  apply(plan: LaunchPlan, ctx: LaunchContext): void;   // 就地改 IR，绝不拼字符串
  cliFlags?(ctx: LaunchContext): string[];             // CLI 渲染器用
}
```

**为什么 `wrap` 是有序嵌套而不是字符串片段**：审计 C1（三方独立指出）——`( __ccm_rbind; exec claude --resume S )`
是**包裹**不是追加，扁平的 `fragment: string` 没有闭括号槽位；且 `exec` 不能省（wrapper 用 `$BASHPID`
读 `sessions/$cpid.json`，不 exec 则 PID 对不上）。`wrap: (inner) => string` + order = 嵌套深度，
把「rbind 必须与 exec 同一子 shell」变成**结构保证**而非注释约定。
（CLI 路径下 wrap 由 CLI 内部负责，IR 的 wrap 只在兜底渲染器生效。）

### 2.6 UI 侧：动作 × 修饰

今天「Resume」这一个意图散成 10 个菜单项，因为「直连/tmux/基座/账号」这四条**正交**维度被摊平成
并列条目做排列组合，于是同一级菜单里同时出现「Resume（tmux）」和「用基座 resume（tmux，不隔离）」。收敛为：

```
Action   = resume | new | attach | restart          ← 一级菜单
Modifier = account=X | base | container=tmux|none | 未来任意   ← 二级 flyout
```

一级列动作、二级 flyout 列修饰（R4 已定：悬停 + 点击都可触发）。
**与 CLI 一一对应**：一级 = 动作参数，二级 = 修饰 flag。新维度 = 多一个修饰项，不多一条一级菜单。

---

## 3. ★共享面账本（Shared Surface Ledger）

| 共享面 | 涉及功能 | 最终形态 | 当前状态 | 备注 |
|---|---|---|---|---|
| `shared/ccm-wrapper.sh` → **新 CLI 本体** | F02,F04,F08 | 从「一个 rbind 函数 + 一个裸 ccm 壳」升格为**参数化启动 CLI**（`~/.local/bin/ccm` 可执行文件）；独占 L1/L2/L5 实现；vendored 可部署（照 F5 的 `cc-acct-iso` vendor 范式） | 硬编码 `~/.claude/sessions/`，`ccm() { ( __ccm_rbind; exec claude "$@" ); }` | **D7 已证伪**：`~/.claude-accts/*/{sessions,projects}` 均 symlink 回 `~/.claude`，同一 inode → 账号感知改造是 no-op，**从计划删除**，省一处四点 lockstep。**F04 已落地**：通道A（建时/exec 时立即声明）两处改写 `@ccm_sid_expect`；通道B（poller 确认）仍是 `@ccm_sid` 唯一写者，`sftp.rs` needle 清单结构性锁死这条分离，防未来改动悄悄写回裸 `@ccm_sid`（重蹈旧审计 D6） |
| **用户 `~/.bashrc` 的 4 个 block**（119-168 cc-block / 169-205 ccm / 210-236 oo-block / 238-305 account-block，共 187 行） | F02,F08 | **整体取代**为一个 block：只剩别名（`cc`/`cct`/`zcc`/`zcct`/`bcc`/`bcct`/`oo`/`oot`），实现全在 CLI。用户 2026-07-27 显式授权「直接重构」 | 4 套并存实现 + 凭据 swap | **能力不能丢**：`_cc_resolve_target` → `--cwd auto`（逐字节保持）、`CC_ENV` 代理 → CLI 内部注入（配置一次性迁到 `~/.config/ccm/config`）、`__ccm_rbind` → CLI 内部。**真删**：`_cc_acct`/`_cc_acct_last`/`cc-acct` 凭据 swap 全套（已被 cc-acct-iso 取代，`~/.claude/accounts/*.json` 快照已无用）。`proxy-on/off/status` 与 claude 无关，**不动**。落盘前须先出完整 diff + 备份 |
| `src/session-backend.ts` | F01,F03,F04 | tmux 动词唯一来源；所有 `-t` 走 `exactTarget()` 产出 `=name:`；**IR 渲染器经 `SessionBackend` 接口取命令**（不导出 `exactTarget` 直呼——那会破 INVARIANTS §31①「前端绝不硬编码后端命令」） | **F01+F03 已落地**：`SessionBackend` 三方法入参改判别式 `TmuxTarget={kind:"raw"\|"quoted";value}`（`exactTarget`/`targetToken` 内部纯 `switch(kind)`，无形状嗅探）；`renderFallback`（`launch-render-fallback.ts`）经此接口取命令，未直呼 | F04 若加新维度需要新的 target 形态，扩 `TmuxMode`/`TmuxTarget`，别绕过接口 |
| `src/launch-plan.ts`+`src/launch-dimensions.ts`（F03 新增） | F03,F04,F05,F06,F07,F08 | `LaunchPlan`/`LaunchContext` 两型 IR + 维度注册表（`identity`/`env-reset`/`account`/`model`/`nested-env-reset`，`order` 定序 + 加载期断言）；**新增维度的唯一落点** | **F08 关闭 R14**：`MODEL_DIMENSION.cliFlags` 从恒 `null` 改成真吐 `["--model", name]`（`ccm` 学会 `--model` 后）；`applies` 仍是条件式不变 | `MODEL_DIMENSION.applies` 是**条件式**（`!!ctx.modelOverride`），不是像 `ACCOUNT_DIMENSION` 那样恒真——判断依据见 `doc/INVARIANTS.md` §37 |
| `src/launch-render-fallback.ts`+`src/launch-render-cli.ts`+`src/ccm-probe.ts`（F03 新增） | F03,F08 | 双渲染器：兜底逐字节等于 F03 之前；CLI 渲染器翻译成 `ccm …` 调用，`canRenderCli` 对任一维度 `cliFlags` 返回 `null` 或容器 `mode!=="create-or-attach"` 一律强制走兜底（诚实放弃，防 #76 复发，见 INVARIANTS §33） | **F08 已落地**：`--print` 平价预言机测试（12 项）+ Rust `probe_ccm_cli`（5 分钟 TTL 探测缓存）；`canRenderCli` 新增针对性检查 `ctx.modelOverride && !probe.capabilities.has("model")`（**没有**塞进 `CLI_REQUIRED_CAPS`——那个列表语义是"每次调用都要求"，只对 `applies` 恒真的维度成立，`model` 是条件式，见 F08 计划 §3.2） | 未来若有新维度也是条件式 `applies`、且 CLI 渲染需要新能力探测，照 `model` 这个"针对性特判"模式，不要不假思索塞进 `CLI_REQUIRED_CAPS` |
| `src-tauri/src/tmux.rs` | F01,F04 | `exact_target()` 产出 `=name:`；三道独立门（`is_safe_tmux_target` 恒强制 / 身份 = `@ccm_sid` ∪ `cc-*` / 破坏性额外要求 `windows==1`）；verify+act 单条原子命令 | **F01+F04 已落地**：`is_safe_tmux_target` 只拒空 target（**不**收紧字符集——与既有 glob/元字符安全引号化通过的测试冲突，字符集收紧是 TS 侧职责）；`build_guarded_tmux_cmd` 用 `display-message` 折进 Gate2 远端半支+Gate3 一条原子命令；`capture_remote_pane` 补 Gate1。真机验收 `e2e/tmux-guarded-acceptance.sh`（14 项）| `is_ccm_tmux_name` 不删除，降级为身份判据**之一**。F05/F06/F07 若加新 tmux 动词，走 `build_guarded_tmux_cmd` 而非另起一套判断 |
| `src/remote-launch.ts` | F02,F03,F04,F05 | 7 个 builder → 薄适配器，**保持位置参数签名**（e2e driver 直接 import，审计 §五.4）；内部构 `LaunchPlan` 后交渲染器 | **F03 已落地**：7 个导出全改薄适配器（一行调 `planXxx(...).plan` 再 `renderFallback`），15 个符号 import 面对 `remote-launch.test.ts` 零改动、断言零编辑仍全绿；`posixQuote`/`isValidSessionId`/等移到叶子模块 `src/shell-quote.ts`，本文件 re-export | `sanitizeRemoteLauncher` 只作用于用户串，`wrap` 必须在其**后**（审计 D3）。校验谓词已两分：创建 vs attach，别再合回一个 |
| `e2e/tmux-target-*`（F01 新增） | F01,F02,F04 | **常设真机行为验收 harness**：`tmux-target-emit.mts` 从真 builder 取生产串 → `tmux-target-acceptance.sh` 在隔离 `-L` socket 上验「命令干了什么」 | F01 建成，26 项 | 凡改 tmux/shell 命令构造的功能都要过它（§5.2）。**别手搓等价命令**——必须从真 builder 取 |
| `src/remote-launch-run.ts` | F03,F05,F06 | 6 个 executor → 单一 `runLaunch(plan)`；剪贴板回退集中一处；返回值统一为 boolean | **F03 已落地「剪贴板回退集中一处」**（`invokeLaunchOrCopyFallback` 唯一实现，6 处调用点只传文案）+「挑渲染器」集中在 `renderLaunchCommand`；**保留 6 个具名 executor（未合并成单一 `runLaunch(plan)`）且返回值仍是 void/boolean 混合**——F03 的硬约束「executor 位置参数签名逐字不变」（`account-restart.ts` 按名调用 `runRemoteResumeTmux`，经 `restart-cmd-driver.ts` 传递性锁死）与「合并成单一入口 + 统一返回类型」互斥，本轮取前者、后半段暂不做 | 若 F05/F06/F09 要真做「单一 `runLaunch`」，须先解决 `account-restart.ts`/`tabs.ts`/`history.ts`/两个 e2e driver 的联动改动——那是比 F03 大得多的一次性重构，不建议顺手做 |
| `src-tauri/src/history.rs` | F06 | 两套 PowerShell builder 收进同一意图模型（F06 的 Phase B 先定「IR 前端构造下发」vs「Rust 侧同构 renderer」） | **F06 已落地**：`build_resume_ps_command`/`build_new_session_ps_command` 收拢成 `build_local_ps_command`（`LocalPsAction` 枚举驱动，同构 TS `LaunchAction`），两个 `#[tauri::command]` 降级成薄委托；前端 `src/launch-requests.ts::planLocal` 构造真 `LaunchContext`（`transport:{kind:"local"}`，F03 起就有但从未实例化过的类型分支）走一遍 `LAUNCH_DIMENSIONS`，`plan.env` 故意算出来不消费（等价保护已在 `lib.rs::scrub_env_vars` 做完，见 INVARIANTS §36） | 采用「Rust 侧同构 renderer」而非「IR 前端构造下发」——`Get-Command` 探测是 render-time 决策，只能在目标机器上做，TS 无法预先渲染。本地路径不接账号维度（`account:{kind:"base"}` 恒定，Windows 本地无 `CLAUDE_CONFIG_DIR` 隔离概念）。为 F09「同一个 Resume 按钮」的统一动作模型铺路——F09 落地时本地/远端已共享同一套 `LaunchContext`/`LaunchAction` |
| `src/behavior.ts` | F02,F03,F08 | launcher 字段仍存**裸字符串**（向下兼容）；新增「CLI 已装/版本」探测缓存 | 探测缓存在 `src/ccm-probe.ts`（按 origin、非 `behavior.ts` 字段）；**F03 加 `forceLegacyLaunchRenderer: boolean`**（默认 `false`，无 UI，手改 config.json 的逃生口，MASTERPLAN R2）——`settings/panel.ts` 的 `onBehaviorToggle` 手搓字面量构造 `BehaviorConfig`，加字段时 tsc 揪出「面板会把它悄悄重置成默认值」的真实回归，已修（面板缓存 open() 读到的值原样带回） | 绝不改 config schema；面板以后再加新的无 UI 字段，切记同一模式（缓存 + 带回），别重蹈 |
| `src/accounts.ts` | F05,F07,F09 | `AccountResolver` 返回判别联合 `{kind:"account"\|"base"\|"unavailable"}`；注入源过 `isSelectable`；保留 `useBase` | **F05 已落地**：`resolveAccount`（纯函数，判别联合）+ `withAccount`（内部改用它，`run` 回调扩成 `(configDir?, accountName?)`）。`account-restart.ts` 的独立解析路径**有意不合并**（失败语义故意不同：`withAccount` 退化基座、`account-restart.ts` 中止），只补传了已知的 `accountName`。**F07 已落地**：新增 `getModelForAccount`/`setModelForAccount`（本机 `config.json` 的 `accounts.modelByAccount` 映射，`defaultName` 单值模式的复数版），`setModelForAccount` 在写入点用 `isValidModelName` 校验（fail-closed，Phase D 审计发现并堵上——非法值若能落盘会让该账号往后每次会话拉起都在 `MODEL_DIMENSION.apply` 里统一 throw）；`withAccount` 的 `run` 回调再扩一参 `(configDir?, accountName?, modelOverride?)`，`account-restart.ts` 并列路径同样补了一次独立查询 | `accountColorsActive` **只有一个消费者**（审计 C6 纠正 v2 事实错误）；徽章门是 `shouldShowAccountBadge`，**不得**把 ≥2 门恢复上去。账号名已线通进 `LaunchAccount`（`name?: string`，可选——见 F05 §3.3 实现期修正），F09 可直接消费。模型偏好是本机独立配置，不经 daemon/manifest 同步（同 `defaultName` 既有先例，见 R14 关于跨机器/跨终端一致性的边界记录） |
| `src/tabs.ts` | F04,F09 | 菜单支持二级 flyout；徽章多账号即常显；对齐全套（⇄/⚠k/alignAll/countAccountMismatches）删除 | **F04 已落地其中的身份统一部分**：`findClaudeTmuxMatches`（不折叠成第一个）+ 三个调用点按严重度分级（resume 警告继续/restart 拒绝/菜单 kill 项禁用）+ `resumingSids` 互斥；**F09 已完成**（`features/F09-ui-convergence.md`）——归档远端 tab 收敛成 1 个 `Resume` 一级项 + 账号×容器 3 级级联 flyout（`resumeTabTmux` 补齐显式选号，账号×容器真正正交）；存活远端 tab 收敛成 1 个 `Restart` 一级项 + 账号 flyout（无容器轴）；`updateAccountBadge` 恒显示身份（R7 语义反转）；对齐全套（⇄/`alignAll`/`countAccountMismatches`/`account.align-active`）全仓删除；`container`/`agent` 不进 `LAUNCH_DIMENSIONS`（R12 见 `doc/INVARIANTS.md` §38），新增独立发现层 `src/launch-menu.ts::enumerateModifierGroups`；`restart` 不进 `LaunchAction`，继续走 `account-restart.ts::restartWithAccount` 编排。双 agent 审计发现 1 阻塞（`updateTabContextMenuItem` 替换 DOM 时丢失 flyout 展开态，命中 R4 契约）+ 5 重要（safe-triangle 悬停收起零延迟/视口边缘无碰撞检测/`restartingSids` 守卫对用户不可见/`openTimer` 未清理/`configDir` 过滤行为变化未记录），全部修复 | R8 已核实安全（e2e driver 零 import `tabs.ts`）且 F09 落地后重跑 `resume-suite.sh`/`restart-suite.sh`（17/24）确认真机行为不受影响 |
| `src/agent-profile.ts` | F02,F03 | agent 轴保持独立；IR 与 CLI 都从它取 resume flag / 默认 launcher | 已是单一来源 | 不与 environment 轴混 |
| `e2e/*-cmd-driver.ts` | F03,F09 | 位置参数签名不变；新增真机行为断言层 | 直接 import builder | **F03 硬验收条件** |
| `~/.local/bin/cc-spawn` | F11 | **第三套独立 tmux 启动实现**并入 `ccm`：`cc-spawn` 保留 cc-bus 专属部分（总线登记 `cc-register`、台账 `spawned.tsv`、复用判定），但**建会话/送环境/送任务改经 `ccm`** | **F11 已落地预信任能力上提**（`shared/ccm` 的 `--tmux` 建会话路径新增预信任 `~/.claude.json`/`~/.codex/config.toml`，逻辑与 `cc-spawn` 逐行对齐 + `pretrusted` 追踪 + screen-scrape 轮询兜底 + `CCM_NO_PRETRUST` opt-out）；**范围收窄**——`cc-spawn` 本体（建会话/送环境/送任务改经 `ccm`）未动，物理上不在这个仓库，改动需要用户另行明确授权，见 F11 计划 §1 | `cc-spawn` 本体的改造（三步改经 `ccm`，只留 cc-bus 专属部分）仍是本行"最终形态"里未完成的一半，留给用户明确要求时再做，非遗漏；`CC_BUS_ID`/`agent_needs_bus_id` 的重复也保持现状不去重（同一理由） |

---

## 4. 依赖图与实现顺序

```
F01 ──┐
F02 ──┴─► F03 ─┬─► F04 ─┬─► F08
               ├─► F05 ─┴─► F09 ─► F10
               ├─► F06
               └─► F07  (架构验收)
```

1. **F01 先做**：修**今天正在损坏数据**的 bug（`kill-session -t cc-abc12345` 会杀掉 `cc-abc12345-2` 且 rc=0 当成功回报）。
2. **F02 第二**：CLI 是「核心思想」的载体，也是 F03 唯一的主渲染目标。先有契约与实现，IR 才知道该渲染成什么。
   F02 独立于前端重构，落地即修好当下最痛的「用账号 X resume 无效」。
3. **F03 地基**：其余全部依赖。
4. **F04/F05/F06/F07 并列**：IR 之上的维度与消费者，互相正交。
   F07 刻意排在此当**架构验收**——加「每账号模型」若做不到零改 builder/CLI 主体，说明 F03/F02 没做到位，回炉。
5. **F08 需要 F02（CLI 本体）+ F04（身份）**才有意义。
6. **F09/F10 UI 最后**：依赖 F05 的解析层稳定。

---

## 5. 横切关注点与约定

### 5.1 向下兼容硬约束（每个功能的 DoD 都要复核）

1. **不传 `--account` = 今天的行为**，载荷逐字节相同。多账号是同一条路径上的参数，不是第二套代码（R7）。
2. **CLI 未装 → 兜底渲染器**，逐字节等于今天。CLI 是增强，不是硬依赖。
3. **老 tmux 会话仍可控**：无 `@ccm_sid` 但名为 `cc-*` 的会话必须继续能 kill/send-keys（审计 C3③：替换白名单会让它们变得**不可杀**）。
4. **老 config 不迁移**：`resumeCommandRemote` 存的仍是裸字符串。
5. **e2e driver 的位置参数签名不变**（`resume-cmd-driver.ts` ~20 断言 + `restart-cmd-driver.ts` ~24 断言）。
6. **`useBase` 逃生口保留**（issue #75）。
7. **不替用户改 bashrc**：越层启动器（`cct`/`oot`/`<n>cct`）**只诊断 + 引导迁移**，不自动降格、不偷改配置。

### 5.2 门禁纪律

- 所有门禁命令用 `set -o pipefail`，结果**重定向到文件后 Read/grep 核实**，绝不信内联回显。
- `npm test`（tsx 黄金串 + vitest）/ `cargo test` / `tsc` 三者全绿。
  **`vitest` 的 `include` 只收 `src/**/*.vitest.ts`，黄金串在 `*.test.ts` 由 tsx 跑——只跑 `npx vitest run` 会假绿，必须 `npm test`。**
- **新增硬门禁：真机行为验收 `npm run test:tmux-target`**（F01 建成，26 项）。凡改动 tmux/shell 命令
  构造的功能，DoD 必须含一张「在隔离 `-L` socket 上验证命令**干了什么**」的表，不能只有黄金串。
  > 实证理由：`cargo test` 369 + `npm test` 全绿 + `tsc` 0，仍放行了一个让 `send-keys` 完全失效的改动
  > ——它们只断言「我写出了打算写的字符串」，从不断言「这条命令在 tmux 上的效果」。

  两条来自 F01 的踩坑纪律：
  1. **验收输入必须从真 builder 取**（`e2e/tmux-target-emit.mts`），手搓等价命令会重蹈覆辙。
  2. **探针载荷不能用真 `claude`**——它会启动并重绘/清屏，把打进去的内容盖掉，让「兄弟会话未被污染」
     的 grep 给出**假 PASS**。用 `CCMPROBE` 这类纯字母词（过 `sanitizeRemoteLauncher`，报 command not
     found 后留在屏幕上）。
- **e2e 的 shell 探针也必须与生产同构**（`has-session` / `set-option` / `kill-session` 一律 `=名:`）。
  探针前缀匹配会说谎：只剩 `X-2` 时 `has-session -t X` 返 0（存活断言假阳）、`set-option -t $S` 会把
  `@ccm_sid` 写到错的会话上（污染 fixture）。见 INVARIANTS §31a。
- **新增：每个功能必须过两个独立 agent 审（用户 2026-07-27 指定）**——
  **后端架构 agent**（把握核心思想、审后端架构是否被破坏）+ **UX agent**（把握核心思想、审交互是否真的收敛）。
  两者 prompt 必须自包含且**带上 §0 核心思想全文**。这是 Phase D 之外的附加关卡，不可省。
- 修 bug 走回归纪律：先写复现的失败测试再修。

### 5.3 tmux 目标语法（F01 产出，全局约定）

真机实测（tmux 3.6）结论，写死为约定：

| 动词 | 目标类型 | `name` | `=name` | `=name:` |
|---|---|---|---|---|
| `send-keys` | pane | 送达但**前缀误伤** | **rc=1 全失败** | 送达且精确 |
| `capture-pane` | pane | 前缀误伤 | **rc=1 全失败** | 精确 |
| `set-option` / `show-options` | pane 解析后上溯 | 前缀误伤 | **rc=1 全失败** | 精确 |
| `kill-session` / `has-session` / `attach` | session | 前缀误伤 | 精确 | 精确 |

→ **统一用 `=name:`**（唯一通用且精确的形式）。`new-session -s <名>` 收的是**名字不是目标**，不加。
→ `show-options -v` 读未设置的 option 是 **rc=1 + stderr `invalid option`**，不是空串（F04 的原子命令必须对此 fail-closed）。
→ 第二道防线：`isValidTmuxName` 禁 glob 字符 `*`/`?`（实测 `kill-session -t 'si*'` 会 glob 命中）。

### 5.4 其他

- 不用 emoji（回复、代码 UI 文案、文档）。
- git commit 不加 `Co-Authored-By`；仅在用户要求时提交。
- 不静默改 `~/.bashrc`；删用户自己写的块（如 `_cc_acct`）需显式确认——`strip_profile_block` 只认
  cc-monitor 自己的围栏，删用户块是**全新能力**，风险等级不同（审计 E9）。
- 不新增轮询；daemon 零改。

---

## 6. 风险与开放问题

| # | 风险 | 缓解 |
|---|---|---|
| R1 | **门禁只锁字符串形状、不锁行为**——本轮已翻车一次 | §5.2 真机行为验收表列为 DoD 硬项 |
| R2 | CLI 成为新的单点：它若有 bug，所有路径一起坏 | 兜底渲染器恒在（可一键切回）；CLI 本体有独立的 shell 级测试；版本探测 + 不兼容即降级 |
| R3 | 远端 shell 是 zsh/fish 时 `.bashrc` 不被 source（审计 D2） | CLI 装成**可执行文件**（`~/.local/bin/ccm`）而非 shell 函数，与 shell 无关；别名才进 rc |
| R4 | F06 跨语言（IR 在 TS、本地路径在 Rust） | F06 的 Phase B 先定「IR 前端构造下发」vs「Rust 侧同构 renderer」，不在 F03 预设 |
| R5 | rbind 的 `( … ; exec … )` 含 `;`，会被 `sanitizeRemoteLauncher` denylist fail-closed 成裸 `claude` | 钉死顺序：sanitize 只作用于用户串，wrap 在其后（审计 D3）。CLI 路径下此风险消失（wrap 在 CLI 内部） |
| R6 | `exec <launcher>` 展开不了别名/函数，且 `cc` 在 Linux 上是 C 编译器 | CLI 用 `ccm` 名；`--launcher` 显式区分外部命令与 shell 函数，函数型走间接层 |
| R7（**已落地关闭**） | 「徽章从不一致信号 → 身份标识」是**语义反转** | `updateAccountBadge` 门（`shouldShowAccountBadge`）通过+账号已知即恒显示身份头像，不再要求 `detectAccountMismatch` 为真；一致/不一致仍靠既有 live 实心/last 幽灵区分，信息未消失只是从"仅告警"变"恒常展示"。批量一键对齐（`alignAll`）**不做等价替代**——F09 Phase D UX 审计指出初稿援引 MASTERPLAN 推论③论证这个决定站不住脚（批量对齐是薄编排,不产生组合爆炸,跟 R12 的场景不是一类问题）,已在 `features/F09-ui-convergence.md` §1 改写为诚实的范围/复杂度取舍说明,并在设置页加一行静态提示（本仓库无 changelog 机制,最低成本告知老用户能力去哪了） |
| R8（**已核实为安全，非阻塞**） | 删对齐全套会断 `e2e/restart-cmd-driver.ts` 的 import | **两个 Plan agent 独立核实**：`e2e/restart-cmd-driver.ts`/`resume-cmd-driver.ts` 只 import `src/account-restart.ts`/`src/accounts.ts` 的具名导出，**零行**import `tabs.ts`——只要 F09 不改前两者的导出签名，删 `tabs.ts` 里的对齐全套不触及 e2e driver 的 import 面。仍按教训清单第2条重跑 `resume-suite.sh`/`restart-suite.sh` 作真机行为回归（import 面不变不能替代行为验证） |
| R9 | 改动面大，e2e 会大面积变红 | 每功能 Phase B 预先列「预期会红清单」，区分「是 bug」与「须 re-baseline」 |
| R10（**已修复**） | **一个 sid 可同时活在 ≥2 个 tmux 会话里，`findClaudeTmux` 静默只挑第一个**（用户 2026-07-27 观察触发核实）：`@ccm_sid` 只写不清（wrapper 明写「不 unset」），resume 前「是否已存活」的判断只在点击瞬间查一次远端，无锁；终端手动 resume 与 app 内 resume 之间没有互斥。命中重复时另一个活会话对 app **完全不可见、也够不着**（继续跑、继续计费），直到用户自己去终端发现。核实当时：现存活会话无重复（`tmux ls` 按 sid 去重后为空），机制是**结构性风险**而非已发作的现存 bug | **F04 必须根治**：三道门 + `@ccm_sid_expect`/`@ccm_sid`（意图 vs 事实）仲裁（审计 D6）之外，须补：① resume 前的「已存活」检查与「创建」必须是**一条原子远端命令**（verify+act 合一，见 §5.2 TOCTOU 教训）而不是「查一次、再另发一条建会话命令」；② `findClaudeTmux` 命中 >1 时不得静默挑第一个——至少要让调用方能拿到全部候选（UI 层如何呈现留 F09）。**不在 F02/F03 单独打补丁**（用户 2026-07-27 拍板：留给 F04 一起做，避免先垫一个后面还要拆的半吊子）。**已修复**：`tmux.rs` 三道门（Gate1 空 target 恒拒/Gate2 `@ccm_sid`∪`cc-*` union/Gate3 仅 kill 要求 `windows==1`）+ `build_guarded_tmux_cmd` 原子 verify+act（`display-message` 一次 round-trip 判断后再执行，真机验收 `e2e/tmux-guarded-acceptance.sh` 14 项证明 TOCTOU 窗口真的消除，非仅字符串断言）；`@ccm_sid_expect`（意图，`shared/ccm` 通道A）/`@ccm_sid`（事实，poller 通道B 唯一写者）拆分，`sftp.rs` 结构性锚点防悄悄写回；`tabs.ts::findClaudeTmuxMatches` 不折叠成第一个，三个真正需要分级的调用点（resume-attach 警告继续/restart-kill 拒绝/菜单 kill 项禁用）全部升级，双 agent 审确认这条"按后果可逆性分级"合理。UI 候选选择器仍留给 F09（数据缝已留干净：`findClaudeTmuxMatches` 全量数组在三处调用点存活到用户点击那一刻，未提前坍缩成布尔） |
| R11（**已修复**） | **F02 已发货的账号选择在特定配置下会被静默覆盖**——综合两版 F03 方案时发现，不是任一方案报的，是核对 `shared/ccm` 实际行为时发现的：`resumeCommandRemote=ccm`（本轮建议的配置）时，cc-monitor 的调用形态是「外层已 `export CLAUDE_CONFIG_DIR=<选中账号>`，再 `exec ccm --resume <sid>`（不带任何账号 flag）」；而 `ccm` 在两者都不传时**无条件**落 manifest 默认账号并重新 `export`，把 cc-monitor 精心选中的**非默认**账号静默覆盖成默认账号——真机复现：外层 export 账号 b，`ccm --print` 却输出账号 z。这比"换号不生效"更隐蔽：它看起来生效了（确实换了号），只是换成了错的号 | 已修：`shared/ccm` 的默认号回退闸加一条 `&& [ -z "${CLAUDE_CONFIG_DIR:-}" ]`——已有继承值（cc-monitor 的调用场景）→ 尊重、不覆盖；真裸终端（无继承）→ 仍落默认号，两个场景用同一个条件天然区分，不需要新增参数。`--account`/`--base` 显式指定不受影响，优先级不变。回归测试补 4 条（继承 b 保留 / 裸终端仍落默认 / `--base` 不受继承影响 / 显式 `--account` 优先级最高），顺带发现并修了测试文件本身没隔离 `CLAUDE_CONFIG_DIR` 的问题（开发者本人的账号环境泄漏进了测试断言） |
| R12（**已降级为已归档设计决策，非阻塞**） | **F09 需要处理"哪些正交轴享受维度注册表的收敛红利、哪些不享受"这条不对称**——F03 只把 environment 轴（`account`/`env-reset`/`nested-env-reset`/`identity`/`model`）注册进 `LAUNCH_DIMENSIONS`；`container`（哪个容器/哪种 tmux 模式）与 agent（哪个 AI）两条轴仍是 `LaunchPlan`/`LaunchContext` 的硬编码一等字段，散落在两个渲染器各自的 `if`/`switch` 里，没有 `applies`/`cliFlags` 接口 | **F09 Phase B 已决策**：开两个独立 Plan agent 论证对立方向（扩大注册表 vs 维持三轴三机制），综合后**采纳"维持三轴三机制，只在 UI 层收敛"**——`container`（`kind` 与 `mode` 都）与 `agent` 继续硬编码，新增 `src/launch-menu.ts::enumerateModifierGroups` 作为独立于 `LaunchDimension` 的 UI 发现层。判断准则（三条 checklist）记入 `doc/INVARIANTS.md` §38。**风险状态不是"关闭"是"降级"**：三轴两种机制的不对称依然存在，但现在有据可查，不再是每次重新审视的开放问题。详见 `features/F09-ui-convergence.md` §0 |
| R13（**已接受，非阻塞**） | **F05 让每次"未选账号"的 CLI 渲染调用都携带 `--base`，而 `shared/ccm` 的 `--base` 不是无害透传，是无条件 `unset CLAUDE_CONFIG_DIR`**（F05 Phase D 双 agent 审计 UX agent 发现，读 `shared/ccm` 400-410 行核对源码得出，不是猜测）。F05 之前，CLI 渲染路径从未发送这个 flag（`ACCOUNT_DIMENSION.applies` 对 `base` 恒 false，见 R11 同型 bug）——ccm 走"两者都没传"分支，只有当 `CLAUDE_CONFIG_DIR` 本就为空时才回落 manifest 默认号；若远端 shell 环境本就 export 了该变量（如用户自己在 shell profile/wrapper 里维护），旧行为是尊重继承值、不覆盖。F05 后，每次"未选账号"的 CLI 调用都会主动清空它——对绝大多数用户（该变量本就未设）是 no-op，但对"自己在 shell profile 手动管理 `CLAUDE_CONFIG_DIR`"这类边缘配置用户，是一次此前不存在的静默覆盖 | **判定为可接受的代价，不回退**：不发 `--base` 会让 R11/R11同型bug 对绝大多数用户复发（多数场景优先于少数边缘配置）；`behavior.forceLegacyLaunchRenderer` 手动逃生口可退回兜底渲染器（该路径不受影响，继续"未选账号 = 不发任何 flag、静默继承"）。风险面窄（需要"手动维护 CLAUDE_CONFIG_DIR + ccm 已装 + 该次调用走 CLI 渲染"同时成立），不做代码改动，仅登记存档 |
| R14（**已关闭**） | **F07 每账号模型偏好带来两个隐藏机制切换，均无应用内提示**（F07 Phase D 双 agent 审计 UX agent 发现）：① `MODEL_DIMENSION.cliFlags` 恒 `null`（`ccm` 无 `--model` flag，诚实降级）——任何配了模型偏好的账号，其**此后每一次**会话拉起都从快速的 `ccm` CLI 渲染路径永久降级到兜底 shell 渲染器；② `shared/ccm` 完全不认识这个偏好——用户在设置里配好模型、点 app 里的按钮起会话会遵循它，手动在终端敲等价的 `ccm --tmux --account z` 则完全不生效，字面违反 MASTERPLAN §0 corollary②"单一渲染目标" | **F08 已关闭**：`ccm` 学会 `--model`（参数解析/tmux 内层命令/`--print`/非容器 env 导出四处落地）；`MODEL_DIMENSION.cliFlags` 从恒 `null` 改成真吐 `["--model", name]`；`canRenderCli` 用针对性特判（非塞进 `CLI_REQUIRED_CAPS`，理由见 F08 计划 §3.2"实现期修正"——`model` 的 `applies` 是条件式，机械照抄 F05 给 `account` 的先例会误伤未配模型偏好的多数会话）确保配了偏好但远端 `ccm` 太旧的会话仍被正确挡回兜底。①关闭后②随之自动关闭：终端 `ccm --model <名>` 与 app 现在是同一条命令，不需要额外工作 |

### 已由本轮证据关闭的旧风险

- **D7 / 根因2（wrapper 账号感知）**：证伪。`~/.claude-accts/{z,b}/{sessions,projects}` 均 symlink 回 `~/.claude`，同一 inode → 改造是纯 no-op，**删除**。
- **rbind 门控「未部署会话立死」**：证伪（审计 D1 实测 `( __ccm_rbind_missing; exec … )` 打一行 stderr、rc=0 继续）。改运行时自适应，去门控。

### 已拍板的决策（2026-07-27）

| 决策 | 结论 |
|---|---|
| 坏 P0 工作区 | **整体 revert 回干净基线**（已执行） |
| 越层启动器调和 | **只诊断，不自动降格**。根治靠 F02 的参数化 CLI：不再用 `cct` 这类单命令，改用 `ccm [动作] [修饰]` + 用户别名组合 |
| 本地路径 | **纳入本轮**（F06） |
| 自动度 | **全自动**（连续 B→G），但每个重要架构必须过**后端架构 agent + UX agent** 两道独立审 |

---

## 7. 变更记录

- 15 — 2026-07-27 — **F09 完成签收**（Phase B→F 全过；Phase B 开两个独立 Plan agent 论证 R12
  对立方向，综合后采纳"维持三轴三机制"，判断准则记入 `doc/INVARIANTS.md` §38，R12 状态从
  "open"降级为"accepted with documented rationale"）。实现：`src/launch-menu.ts`（新，独立于
  `LaunchDimension` 的 UI 发现层）；归档远端 tab 收敛成 `Resume` 一级项 + 账号×容器 3 级级联
  flyout（补齐 `resumeTabTmux` 此前不支持显式选号的实现缺口，账号×容器真正正交）；存活远端
  tab 收敛成 `Restart` 一级项 + 账号 flyout（无容器轴，restart 仍是编排不进 `LaunchAction`）；
  `updateAccountBadge` 从"仅不一致才显"改"账号已知即恒显"（R7 语义反转，信息未消失只是从
  告警变常显）；对齐全套（⇄/`alignAll`/`countAccountMismatches`/`account.align-active`）全仓
  删除（R8 落地前已核实 e2e driver 零 import `tabs.ts`，安全）。双 agent 审计：后端架构 0
  阻塞+2 重要（`openTimer` 未清理/`configDir` 过滤行为分歧，均已处置）；UX 审计 1 阻塞
  （`updateTabContextMenuItem` 替换 DOM 丢失 flyout 展开态，命中 R4"悬停+点击都可触发"契约，
  越熟练用户越易踩中——已修）+ 4 重要（safe-triangle 悬停收起零延迟/三级级联无视口边缘碰撞
  检测/`restartingSids` 守卫对用户不可见/6 处引用已删除 UI 的过时注释，均已修复）+ 指出 §1
  "不做什么"对批量对齐删除的原始论证误用 MASTERPLAN 推论③（已改写为诚实的范围/复杂度取舍
  说明，不是架构强制）。落地 3 条 UX 建议（Resume flyout 视觉分隔线/Restart 账号项 title 说明
  无基座选项/设置页批量对齐下线提示）。门禁全绿：tsc 0/npm test 648/cargo test 379/全部既有
  e2e 套件（含 `resume-suite.sh` 17/`restart-suite.sh` 24 真机回归）；`remote-launch.test.ts`/
  两个 e2e driver/`src-tauri/` 全程零 diff。
- 14 — 2026-07-27 — **F08 完成签收，R14 已关闭**（Phase B→F 全过，未开 Plan agent
  fanout——5 个子项均无架构分歧；后端架构审计因安全护栏两次误判被打断——讨论 shell 转义/
  注入防护的常规防御性代码审查被误判成攻击性安全测试——本席直接接手完成剩余核实，UX 审计
  由自动化 agent 完整跑完）。实现：①`invalidateCcmProbeCache` 接进安装成功分支；②`ccm`
  学会 `--model`（参数解析/tmux 内层命令/`--print`/非容器 env 导出四处 + `--ccm-probe`
  能力位）；③`MODEL_DIMENSION.cliFlags` 从恒 `null` 改成真吐 flag，**关闭 R14①**；④
  `canRenderCli` 加针对性检查（未塞进 `CLI_REQUIRED_CAPS`——实现期修正：`model` 的
  `applies` 是条件式，机械照抄 F05 给 `account` 的先例会误伤未配模型偏好的多数会话，见
  F08 计划 §3.2）；⑤别名生成器 + 越层启动器诊断；⑥旧 swap 退役确认已关闭（F02 落地，
  无需代码）；⑦`--help` 内容补齐（示例/别名指引/账号查看指路）。R14②随①自动关闭（终端
  `ccm --model` 与 app 现在同一条命令）。UX 审计发现 1 阻塞（别名名字零校验可拼出语法
  错误的 shell 代码，实测复现并修复）+ 多条重要（生成器与诊断分居两处且按主机重复渲染，
  已合并进同一设置分组、紧邻彼此、不再重复；`--base`/`--account` 从静默优先级改成控件级
  主动互斥；复制 toast 补齐 source/新终端提示；诊断正则语义订正）。门禁全绿：tsc 0/
  npm test 658/cargo test 379/全部既有 e2e 套件（含 `test:ccm-cli` 39/`test:ccm-print-parity`
  12）；真实全局配置文件复测确认干净。
- 13 — 2026-07-27 — **F11 完成签收**（Phase B→F 全过，未开 Plan agent fanout——关键决策是
  范围边界而非技术方案；双 agent 审各发现真实阻塞项，是本轮迄今审计"命中率"最高的一次功能，
  因为这是唯一一个"写用户真实全局配置文件"的功能，风险面客观更高）。实现：`shared/ccm` 的
  `--tmux` 建会话路径新增预信任逻辑（照抄 `~/.claude/skills/cc-bus/scripts/cc-spawn` 的
  `~/.claude.json`/`~/.codex/config.toml` 写入，逐行对齐不重新设计）；范围收窄为**只上提进
  `ccm`，不改 `cc-spawn` 本体**（cc-spawn 物理上不在这个仓库，是跨项目共享的 cc-bus 基础
  设施，改动需要用户另行明确授权，不该被"全部自动做"隐式覆盖——这条决策经审计独立评估
  认可）。双 agent 审计各自独立发现并实测复现真实 bug：① `--cwd <相对路径>` 场景下 `$cwd`
  非绝对路径导致预信任写出字面量野 key（如 `"proj1"`），对真实信任表零效果还污染用户真实
  配置文件、`jq` 校验照样通过不报错——修法是预信任专用 `cwd_abs="$(cd "$cwd" && pwd)"`
  规范化；② 完全丢失了 cc-spawn 真正的安全网——`pretrusted` 追踪 + 写入未成功时轮询抓 pane
  文本自动按 Enter，而 cc-monitor 前端对本地 tmux 会话零可见性（`capture-pane` 预览只对
  远端开放），没有这层兜底信任框会静默卡死无人发现——已补齐并用真实假 launcher 端到端验证
  轮询确实生效；③ 本功能让**既有**（非新增）`e2e/ccm-acceptance.sh` 从"纯净沙盒"变成真实
  污染开发机 `~/.claude.json`/`~/.codex/config.toml` 的测试（该脚本从未设隔离钩子，因为
  F11 之前 `--tmux` 无此副作用）——**已实测复现两次**、清理本机污染、给该脚本补齐隔离。另修：
  计划承诺的 stderr 诊断补齐；受众从 cc-spawn 窄众变宽后加 `CCM_NO_PRETRUST` opt-out（同时
  关闭写入与轮询两条路径）；e2e 测试从 6 场景扩到 9 场景（新增相对路径回归/opt-out/真失败/
  轮询兜底端到端验证）。门禁全绿：tsc 0/npm test 640/cargo test 379/全部既有 e2e 套件（含
  新增 `test:ccm-pretrust` 13/13）；真实全局配置文件复测确认干净。
- 12 — 2026-07-27 — **F07 完成签收（架构验收通过）**（Phase B→F 全过，未开 Plan agent
  fanout——`account-onboarding/MASTERPLAN-v2.md` 的既有先例收窄了设计空间；双 agent 架构/UX
  审无阻塞项、1+3 条重要发现全修）。实现：`launch-dimensions.ts` 新增第 5 个维度
  `MODEL_DIMENSION`（order 25，卡在 `account`(20) 与 `nested-env-reset`(30) 之间），
  `applies:!!ctx.modelOverride` 是**条件式**而非像 `ACCOUNT_DIMENSION` 那样恒真——判断依据
  记入新增 `doc/INVARIANTS.md` §37（"沉默是否等价于用户期望"，不是"是不是账号相关"，防未来
  维度作者机械照抄 F05 的"恒真"教训）；`cliFlags` 恒 `null`（`ccm` 无 `--model`，诚实降级，
  留给 F08）。`launch-plan.ts`（`EnvOp` 加 `export-model` 变体 + `LaunchContext.modelOverride`）/
  `launch-render-fallback.ts`（`renderEnvOps` 加一个分支）/`launch-requests.ts`（4 个
  `planXxx`）/`remote-launch-run.ts`（5 个 executor）各加一个末尾可选参数；`accounts.ts` 新增
  `getModelForAccount`/`setModelForAccount`（本机 `config.json`，`defaultName` 单值模式的复数
  版），`withAccount` 的 `run` 回调再扩一参；6 个调用点 + `account-restart.ts` 并列路径接线；
  `settings/accounts-section.ts` 加每账号"默认模型"输入框。架构验收核心断言逐行核对属实：
  `buildLaunchPlan`/`renderCli`/`canRenderCli`/`renderFallback` 的既有分支结构 diff 为零。
  双 agent 审计发现并修复：① **阻塞**——模型输入框保存路径不校验，非法值静默落盘后会让该
  账号往后**每一次**会话拉起在 `MODEL_DIMENSION.apply` 里统一 throw，用户难以联想到根因；
  已把 `isValidModelName` 校验移到 `setModelForAccount` 写入点（fail-closed）；② 保存无任何
  toast 反馈（同文件其余动作都有）且失败会真正无声消失（设置窗口没有主窗那个全局
  unhandledrejection 兜底）——已按 `selectDefault` 既有模式补齐；③ `cliFlags` 恒 `null` 的隐藏
  CLI 降级与终端 `ccm` 不识别此偏好这两个隐藏机制切换，处置力度未达到 F05 R13 的先例、应用内
  也无提示——已登记 **R14**（已接受，非阻塞，②留给 F08 关闭）+ 输入框 tooltip 补一句边界提示；
  ④ `canRenderCli` 的模型降级此前零端到端测试覆盖（只孤立测过 `cliFlags()` 返回值）——已补 2
  条；⑤ `modelOverride` 在 `tabs.ts` 等集成层的真值转传此前从未被验证过（同 F05 曾堵过的同一类
  缺口）——已补 1 条真实字符串（"opus"）的集成测试；⑥ `doc/INVARIANTS.md` 计划承诺的新维度
  落地样例最初漏写——已补 §37。门禁全绿：tsc 0/npm test 640/cargo test 379/全部既有 e2e 套件
  不变（本功能不碰 Rust/远端 tmux 路径）；`remote-launch.test.ts`/两个 e2e driver 全程零 diff。
- 11 — 2026-07-27 — **F06 完成签收**（Phase B→F 全过，未开 Plan agent fanout——Explore fork 用
  平台约束（`Get-Command` 探测只能在目标机器现场跑）单向决定了"Rust 侧同构 renderer"这条路，
  不是两个旗鼓相当需要比较的方案；双 agent 架构/UX 审无阻塞项、2+2 条重要发现全修）。实现：
  `history.rs` 的 `build_resume_ps_command`/`build_new_session_ps_command` 收拢成
  `build_local_ps_command`（`LocalPsAction` 枚举驱动，`Get-Command` 探测-回退分支只写一次），
  两个 `#[tauri::command]` 降级成薄委托，新增 2 条黄金串对拍测试锁死重构前后逐字节同输出；
  `src/launch-requests.ts` 新增 `planLocal`，把本地 resume/新建两条路径从"直拼
  `{sessionId,cwd,launcher}`"改成"构造真 `LaunchContext`（`transport:{kind:"local"}`，F03 起
  就有但从未被任何调用点实例化过的类型分支）→ 跑 `LAUNCH_DIMENSIONS`"，4 个调用点
  （`history.ts`×2/`tabs.ts`/`session-viewer.ts`）接入；实现期自己发现并修一个一致性缺口：
  本地路径此前唯一缺失 `isValidSessionId` 校验（同其余 4 个 `planXxx` 早有的模式），已补齐（防御
  性收紧，不改变任何合法输入下的行为）。`plan.env` 因 `NESTED_ENV_RESET_DIMENSION`（不看
  transport）对本地场景恒非空，判定**故意不消费**——本地场景的嵌套 env 污染保护已经在
  `lib.rs::scrub_env_vars`（进程启动期一次性清洗 cc-monitor.exe 自己的环境，`launch_powershell_window`
  spawn 的子进程默认继承这份已清洗过的环境）做完，补渲染期重复清洗只会引入未经真机验证的新
  PowerShell 代码，不增加安全收益（已记 INVARIANTS §36）。双 agent 审计发现并修复：①
  Rust/TS 两处 sid 字符集校验方向安全但不完全相同（措辞订正，未改代码）；②
  `launch-render-cli.test.ts` 一处过期测试标题漏改；③ 本地 sid 校验失败的错误 toast headline
  与远端同类失败不一致（已把 4 个调用点统一改成两阶段 catch，对齐远端"无法构造 resume 命令"
  措辞）；④ `planLocal`/`getBehavior()` 相对顺序在 4 个调用点不统一、一处注释因此不准确（已
  订正）。Phase D 一份初次汇报误判"计划 checkbox 未勾"为阻塞项（复核后判定这只是 Phase F 文档
  收尾尚未进行，非功能性阻塞）；另排除一条误报（`nestedEnvVars` 顺序差异被误读为字段缺失，两份
  列表实际内容相同）。门禁全绿：tsc 0/npm test 631/cargo test 379/全部 e2e 套件不变（本功能不碰
  远端/tmux 路径）；`remote-launch.test.ts`/两个 e2e driver 全程零 diff。
- 10 — 2026-07-27 — **F05 完成签收**（Phase B→F 全过，未开 Plan agent fanout——目标形态已由
  账本给定，直接规划；双 agent 架构/UX 审无阻塞项、2+1 条重要发现全修）。实现：`accounts.ts`
  新增 `AccountResolution` 判别联合 + `resolveAccount` 纯函数（`withAccount` 内部改用，行为
  逐字节保持）；`withAccount` 的 `run` 回调扩成 `(configDir?, accountName?)`；`LaunchAccount`
  类型加可选 `name` 字段；`ACCOUNT_DIMENSION.applies` 改恒真 + `cliFlags` 三分支（account 有
  名字/base/account 无名字→null），把 F03 的移交点接上，同时**顺带发现并修复一个 R11 同型
  潜在 bug**——F03 的 `applies` 只在选中账号时为真，导致最常见的"未选账号"场景从未过
  `cliFlags` 的 null 安全网检查，CLI 渲染器可能吐出既不带 `--account` 也不带 `--base` 的
  命令，R11 病灶以新形式复现在多数用户身上。审计发现并修复：① `doc/INVARIANTS.md` 计划承诺
  的新不变量最初漏写（只留源码注释）——已补 §33 更新 + 新增 §35；② 6 个 `withAccount` 调用点
  的集成层测试此前全部只覆盖 accountName 恒 undefined 的场景，接线本身从未被验证——已补 4 条
  集成测试；③ UX 审计发现 `shared/ccm` 的 `--base` 是无条件 `unset CLAUDE_CONFIG_DIR`（非无害
  透传），F05 让每次未选账号的 CLI 调用都携带它，对手动管理 `CLAUDE_CONFIG_DIR` 的边缘配置
  用户是新的静默覆盖——判定为可接受代价（不回退，登记 R13，`forceLegacyLaunchRenderer` 逃生口
  可退避）。实现期自己也踩了一次坑又自己修：`LaunchAccount.name` 最初设计成必需字段，导致
  `remote-launch.test.ts`（F03 定的"零编辑"硬约束）3 个测试炸——已改 `name` 为可选、
  `configDir` 单独触发 `account` 态（同 F03 原行为）。全量门禁：`tsc`0/`npm test`625/
  `cargo test`377/`test:tmux-target`26/`test:ccm-cli`36/`test:ccm-acceptance`15/
  `test:ccm-print-parity`10/`test:tmux-guarded`14/`resume-suite`17/`restart-suite`24，
  `e2e/resume-cmd-driver.ts`/`restart-cmd-driver.ts`/`remote-launch.ts` 全程零 diff
- 09 — 2026-07-27 — **F04 完成签收，R10 已修复**（Phase B→F 全过，两版 Plan agent 方案综合，
  双 agent 架构/UX 审无阻塞项、2+2 条重要发现全修）。实现：`tmux.rs` 三道门（Gate1 空 target
  恒拒/Gate2 `@ccm_sid`∪`cc-*` union/Gate3 仅 kill 要求 `windows==1`）+ `build_guarded_tmux_cmd`
  原子 verify+act（`display-message` 单次 round-trip，真机验收新增 `e2e/tmux-guarded-acceptance.sh`
  14 项证明 TOCTOU 真的消除）；`shared/ccm` 的 `@ccm_sid_expect`（意图）/`@ccm_sid`（事实）通道
  拆分，`sftp.rs` 结构性锚点防回归；`tabs.ts::findClaudeTmuxMatches`（不折叠成第一个）+ 三个
  真正需要分级的调用点全部升级（resume-attach 警告继续/restart-kill 拒绝/菜单 kill 项禁用）+
  `resumingSids` 互斥。审计发现并修复：① `CCM_GUARD_REJECTED` 拒绝消息曾恒带无关的 `windows=`
  字段（send-keys 不受 Gate3 约束）；② 真机验收脚本缺 cargo-失败前置检查与结束时的 tmux server
  清理；③ 6 处新增消息里有 1 处措辞漂移（"远端"vs"终端"，已统一）；④ 3 处新增 toast 时长
  （10000ms）偏离本文件既有惯例（8000ms），且方向拧了（警告类比拒绝类更长），已对齐。全量门禁：
  `tsc`0/`npm test`615/`cargo test`377/`test:tmux-target`26/`test:ccm-cli`36/
  `test:ccm-acceptance`15/`test:ccm-print-parity`9/`test:tmux-guarded`14/`resume-suite`17/
  `restart-suite`24，`account-restart.ts`/两个 e2e driver 全程零 diff
- 08 — 2026-07-27 — **F03 完成签收**（Phase C→F 全过，双 agent 架构/UX 审无阻塞项、2+2 条重要
  发现全修）。实现：`LaunchPlan`/`LaunchContext` IR + 4 维度注册表 + 双渲染器（`renderFallback`
  逐字节等于旧行为、`renderCli` 翻译成 `ccm …` 调用）+ ccm 探测缓存（TS+Rust）+
  `--print` 平价预言机测试 + 6 个 executor 收敛调用统一的渲染器挑选与剪贴板回退。审计发现并修复：
  ① `canRenderCli` 的 #76 闸门此前误伤 `attach-only`（与真正有歧义的 `send-into` 合并成一把闸门），
  导致 CLI 渲染器的 attach 分支永不可达——核对 `shared/ccm` 源码确认 `attach` 与兜底渲染器逐字
  同构后，收窄闸门只挡 `send-into`；② `settings/panel.ts` 手搓 `BehaviorConfig` 字面量，加
  `forceLegacyLaunchRenderer` 字段后 tsc 揪出"任一勾选框变动会把该字段悄悄重置成默认值"的真实
  回归，已修（缓存 open() 读到的值原样带回）。计划侧改动：③ 账本新增 R12——container/agent 两条
  正交轴不经维度注册表（只有 environment 轴享受"加维度=+1行"红利），转发给 F09 Phase B 自行决定；
  ④ 补 8 条 toast 文案 smoke test（此前只有 `runRemoteResume` 有直接断言）；⑤ `doc/INVARIANTS.md`
  新增 §33（双渲染器诚实边界不变量）。全量门禁：`tsc`0/`npm test`606/`cargo test`374/
  `test:tmux-target`26/`test:ccm-cli`36/`test:ccm-acceptance`12/`test:ccm-print-parity`9/
  `resume-suite`17/`restart-suite`24，`account-restart.ts`/`tabs.ts`/`views/history.ts` 全程零 diff
- 07 — 2026-07-27 — **R11 修复（F02 追加）**：综合 F03 两版架构方案时，核对 `shared/ccm` 实际账号解析
  行为，发现并修复一个真实存在、影响已发货功能的 bug——`resumeCommandRemote=ccm` 配置下，cc-monitor
  选中的非默认账号会被 `ccm` 自己的"两者都不传落默认号"逻辑静默覆盖。修法：默认号回退闸加
  `[ -z "${CLAUDE_CONFIG_DIR:-}" ]`，只在真无继承时才落默认号。补 4 条回归测试，顺带修了测试文件
  自身未隔离 `CLAUDE_CONFIG_DIR` 导致断言随开发者本人账号环境漂移的问题。已真机部署、门禁全绿
  （tsc 0/npm 598/cargo 370/tmux-target 26/ccm-cli 36/ccm-acceptance 12）。记入风险表 R11（已修复）
- 06 — 2026-07-27 — **F02 完成签收**（Phase C→F 全过，两阻塞项+若干真机发现全修，双 agent 架构/UX
  审通过）。计划侧改动：① 账本新增 F11「cc-bus 集成（cc-spawn 并入 ccm）」——`cc-spawn` 是第三套独立
  tmux 启动实现，且其"预信任写入"能力应上提进 `ccm` 核心（直接解决 R10 调研中发现的"卡信任确认页
  数小时、@ccm_sid 永不写入"现象）；② §6 新增 R10：一个 sid 可同时活在 ≥2 个 tmux 会话里，
  `findClaudeTmux` 静默只挑第一个，用户 2026-07-27 观察触发核实，根治指派给 F04（不单独打补丁，
  用户明确拍板）；③ F02 遗留六条按功能分派进 F03/F04/F06/F07/F08 的账本/风险表，无遗留孤儿债务；
  ④ INVARIANTS §30/§31a 同步过期引用（`ccm-wrapper.sh` 已删、"三处同源"更正为四处）
- 05 — 2026-07-27 — 用户拍板：R10（sid 匹配多个会话）留给 F04 一起根治，不在 F02/F03 单独打补丁
- 01 — 2026-07-27 — 初版：从 `account-onboarding/MASTERPLAN-v2` + `AUDIT-v2-FINDINGS` §八 + 本轮真机证据重写 — v2 被四视角审计判定不可执行；且本轮查明「层所有权混乱」是真病根
- 04 — 2026-07-27 — **F02 定形（用户三次澄清后）**：① 用户指出「旧的本来就该全部抛弃，为什么还考虑冲突」——
  `ccm` 名字冲突是个**不该存在的问题**，旧 4 个 block 整体取代、不共存，故沿用 `ccm` 名；
  实测佐证：shell 函数优先于 PATH，若共存则新 CLI 一次都跑不到（静默）→ 故装成
  `~/.local/bin/ccm` **可执行文件**而非函数（顺带解决审计 D2 的 zsh/fish 问题）。
  ② **codex 收编**：`--agent claude|codex` 成为 agent 轴的第二个值，`oo/oom/oot` 变别名，CLI 主体零分支。
  ③ 新增 `--resume <sid>` 与 `resume <sid>` 等价——使 F02 一落地、前端零改动就正确（把设置里的
  「远端 resume 命令」填 `ccm` 即可），F02 因此独立可验。④ 新增 `--print` / `--ccm-probe` 自省接口
  （黄金串等价断言 + 安装自检 + 降级判据）。⑤ 会话命名统一为 `cc-<safe-basename>`，消灭 INVENTORY 记的
  「4 套命名规则」；agent 类别进 `@ccm_agent` option 不进名字。⑥ 账本新增「用户 `~/.bashrc` 4 个 block」一行，
  明确「命令可全废、**能力必须迁移**」（`--cwd auto` / 代理 / rbind）与「真删」（凭据 swap 全套）的分界
- 03 — 2026-07-27 — **F01 完成签收**（Phase B→F 全过，双 agent 审计无阻塞、7 项重要发现全修）。计划侧改动：
  ① §3 账本 `session-backend.ts` 最终形态措辞改为「渲染器**经 `SessionBackend` 接口**取命令」——原措辞
  「由 IR 渲染器调用 `exactTarget()`」若字面执行会破 INVARIANTS §31①；② 账本给 F03 接一条「`exactTarget`
  改判别式入参，别留形状嗅探」、给 F04 接一条「`is_safe_tmux_target` 须涵盖空 target」；③ 账本新增
  `e2e/tmux-target-*` 行（真机验收 harness 由 scratchpad 一次性脚本**升为常设门禁**，理由=铁律 6，F02/F04
  马上要用）；④ §5.2 增两条踩坑纪律（验收输入取自真 builder / 探针载荷不能用真 claude 否则假 PASS）
  + e2e shell 探针同构要求；⑤ 新立 `doc/INVARIANTS.md §31a`（`=名:` 三处同源 + 尾冒号不可省的原因）。
  另：F01 计划里「`session-backend.ts` 7 处 `-t`」实为 8 处（多出的是 F03.4 甲′ 第二条 set-titles），已订正。
- 02 — 2026-07-27 — **架构重定向**：核心从「cc-monitor 独占 L1/L2、用户不得越层」改为「**L1/L2 本来就是参数**——统一启动 CLI `ccm [动作] [修饰]`，用户自定义 = 给组合起别名」 — 用户指出不该被现有 bashrc 约束，cc-monitor 的目的是**把终端行为与 app 行为统一**，让终端也拿到 app 的便利（短命令、tmux 内启动、被 cc-monitor 无缝识别、方便切号且可叠加）。新增 §0 核心思想；F02 由「LauncherProfile 降格」改为「统一启动 CLI」；F08 改为终端集成收尾；§5.2 增「后端架构 + UX 双 agent 审」硬门禁
- 16 — 2026-07-28 — **本轮重排为 R/B/P 三段；R00/R01/R02 完成签收**。四视角复核结论：
  IR 内核保留（一个先独立设计后读实现的 agent 会重新引入 R11 并缺 fail-soft 一维，
  自列 11 条"现有实现做对了而我没想到"），该重构的是它上面两层 + 验证信号本身。
  证伪账本一处记载：「e2e driver 传递性锁死 executor 签名」为假（三方独立核实），R03 代价
  远小于账本描述。**R00 信号修复**：`cargo fmt --check` 转绿（29 处，已验证全由本分支引入）；
  开 draft PR #83 让分支**首次跑 CI**；7 套真机 e2e（126 条断言）接进两个 ubuntu job
  （按需不需要 Rust 工具链拆开）。首次 CI 立刻揪出 2 个本地永远看不见的真缺陷：
  ① `ccm-print-parity` 一直在验开发者装机版 ccm 而非仓内 `shared/ccm`（PATH 解析）；
  ② `shared/ccm` 在 git 里没有可执行位（干净 checkout 上不可执行；生产不受影响，
  `sftp.rs` 显式 `0o755`）。另把 `ccm-acceptance`/`ccm-pretrust` 的固定 `sleep` 改轮询等待，
  并加 `dump_panes` 超时诊断——`send-keys` 载荷失败的信息只存在于 pane 里，没有它就只能靠猜。
  **R02 伪测试扫荡**：11 条核心防线变异检查全部 RED，无新增伪测试；同时记录变异检查自身的
  三个失效模式（变异不可达 / 变异语义无效 / 门禁太窄），报伪测试前必须先排除这三条。
- 17 — 2026-07-28 — **R03/R06/R09 完成签收**。**R03**（`4f6c313`）：尾部三元组
  `(configDir?, accountName?, modelOverride?)` 收进 `LaunchModifiers`；实现期发现**列车车头是
  `accounts.ts::withAccount` 的 run 回调**而非 planXxx（6 个调用点全是逐字转发），按铁律 4 扩围。
  **诚实结论：只达成成功标准②「零改调用点」那一半**（32 行透传编辑归零），「零改 builder」
  未达成（4 个 planXxx 仍各 2 行解构+ctx，`LaunchContext` 也要加字段）——`launch-plan.ts` 头注
  原写「只需三处」属过度声称，已订正。最值钱的收益是三个相邻 `string|undefined` 消失
  （传反 tsc 抓不到、后果是 R11/R08 那族「用了错的号」），用 `@ts-expect-error` 钉死。
  Phase D 独立对抗性 agent 0 阻塞 + 4 重要全修：① `toHaveBeenCalledWith` 对对象忽略
  undefined 值的键 → `account-restart.vitest.ts` 断言真的变松（已补 `"opus"` 用例）；
  ② `planXxx` 的 bag→ctx 解包层全仓零覆盖，变异后 tsc/测试/print-parity 三道门全瞎（已补）；
  ③ §38 引用用错（那条谈注册表 vs IR 一等字段，不管入参形状）已换真理由；
  ④「占位实参已出现」在 HEAD 生产代码里为 0，属抬价，已订正。审计另用独立预言机
  （HEAD 快照 + 81 条修饰矩阵组合 diff）确认行为**零字节差异**。
  **新登记 R15**：`LaunchContext.passThrough` 纯透传子集可闭合「零改 builder」那一半，
  但今天纯透传只有 `modelOverride` 一个元素，一个元素撑不起抽象（同 R12 教训），
  等 B02 的 `--bus-id` 进来有第二个元素再抽。
  **R06**：`INVENTORY.md` 重写为符号名 + 可复跑 grep 锚点（行号是十个功能里最先腐烂的东西；
  符号锚点腐烂时 grep 返回空、自己会报错）。11 条锚点逐条验证，首轮即抓到 1 条空锚点。
  §E 首次给出成功标准① 的逐条判定。**R09**：查证 `@ccm_sid` 全仓有**两个**写者
  （`shared/ccm` poller + 兜底渲染器 `session-backend.ts` 直写），但那是 F04 Phase B 的明确取舍
  （兜底路径无 poller，写 `_expect` 将永不被提升 → 会话不可 kill）。两侧反向断言各自已被钉住
  （变异验证过）。成功标准④ 不受影响。唯一真问题是「通道B 是唯一写者」少了作用域限定，已订正。
- 18 — 2026-07-28 — **R04 完成签收**（`fb81fe1`）：IR 内核四处结构性收紧
  （`tryRenderCli` 返回 Result / `requiredCaps` 下放维度 / `EnvOp` unset 侧收窄 /
  `WrapSpec` 闭包改纯数据），共性是「把只写在注释里的纪律变成类型或结构上做不到的事」。
  账本双渲染器那一行据此更新：`canRenderCli`/`renderCli` 已合成单一 `tryRenderCli`，
  且 **attach 分支有显式豁免**（不收集 `requiredCaps`、不受 §33 铁律#1 约束，
  理由：`ccm attach <名>` 不接受任何修饰 flag；豁免范围与撤销条件见 INVARIANTS §33）。
  Phase D 独立对抗性 agent 1 阻塞 + 5 重要全修，**推翻本席三处声称**：
  ① 新加的三条测试落在测试文件失败聚合点**之后** → 双向死区、CI 上零守护
  （**这是伪测试失效模式的第四种**，已并入 STATUS 纪律清单，连同审计自己踩到的第五种
  「变异未落在代码行上」）；② `WrapSpec.prelude` 声称"覆盖已知唯一用例"是假的——
  `applyWraps` 套整条 payload 导致 `exec` 落在 env 前缀前，实测 `rc=127` launcher 起不来，
  而新测试**断言的正是这个坏形态**（已修 call-site 为只包 `renderArgv`，wrap 零生产者故零风险）；
  ③ `unset` 收窄丢掉了编译期穷尽性（旧末支读 `op.keys` 会**逼**编译器穷尽，新末支把一切兜住，
  加第 4 个变体 tsc 静默通过并静默渲染成嵌套 env unset）——已补 `never` 守卫，
  并接受审计判定：③ 的安全收益是**纸面**的，加守卫后才值得做。
  另：`launch-dimensions.ts` 的"语义不变"表述错误、计划 §3.1 曾写"已补测试"而实际没补
  （attach 一次放宽了**三**道闸门，其中 model 那道真实可达）——已补 attach 豁免组三条测试。
  **顺带关掉 tsc 盲区**：`tsconfig.json` 的 `include` 加 `"e2e"`，
  R04 实现期漏过的那个 import 错误现在是编译期错误。
  审计独立验证推论④ 零破坏（720 场景×5 探针，`renderFallback` 差异 0 条）。
- 19 — 2026-07-28 — **R05 + R07 完成签收，R 段 9/9 全部收口**。
  **R05**（`fb20c03`）：删 `launch-menu.ts` 的 container 死代码（唯一生产调用点从不读它）、
  函数收成 `enumerateAccountModifiers` 直接返回数组、`"__base__"` 裸魔法串换判别联合。
  **③ 顺带修了一个真 bug**：`validateAcctName` 放行下划线，真实账号可以叫 `__base__`，
  改造前会被判成基座 → 静默落基座丢号 + Restart 入口凭空消失（R11/R08 同族）。
  审计 0 阻塞 + 5 重要：**我唯一实质重写的那行零测试覆盖、三个变异全存活**（已补 6 条行为测试
  并复跑转红）；`doc/INVARIANTS.md` §38 作为**规范性**章节指着已删符号讲道理（已同步，
  论证反而更强：container 值域固定为两字面量、不需要"现查"，故不需要发现层）。
  **否决账本「5 处账号菜单收敛」**（五处早共用 `fetchAccounts`+`isSelectable`，
  不同的只有渲染载体，强行收敛=新增抽象），但审计指出核实不完整——实为 **7 处**，
  且漏掉的 `views/history.ts` **缺基座逃生口**，已登记 **R16**。
  **R07**（`2c365a4`）：`planLocal` → `validateLocalLaunch`，返回 `void`，
  **并删掉内部那遍 `buildLaunchPlan`**。审计 1 阻塞 + 5 重要，**推翻了本席最核心的论证**：
  ① 否决「真接上」引的是**错误论据**——`Get-Command` 只排除"TS 全量渲染好字符串、Rust 只管 exec"，
  **不排除**"TS 构造 IR、Rust 只补 Get-Command"；真正理由是 F06 §3.2 的
  **「`plan.action`/`plan.cwd` 恒等于输入、没有信息增量可取回」**（F06 §1 那条已勾 `[x]` 的
  "从 LaunchPlan 取 action/cwd/launcher"DoD 从未实现、已在 §3.2 撤回，现已就地标注）；
  ② 「走一遍 `buildLaunchPlan` 是便宜的一致性检查」**零门禁守护**（删掉后 705 全绿；
  改 `void` 把类型层强制降级成了可随手删的裸语句）且是 **fail-closed 风险**，已删。
  **INVARIANTS §36 标题重写**为「本地路径不经 IR 产出命令」（原题的 `plan.env` 前提已不成立）。
  另修一个**坏 commit**：R05 曾把 R07 的 tabs.ts 改名卷入，导致该 commit 单独不可编译且
  **本地 tab resume 功能性损坏**；核实未 push 后 amend 修复（未改写已发布历史）。
  **纪律新增**：审计 agent 在跑时只做新文件与只读核实，绝不改下一个功能将要改的源文件。
