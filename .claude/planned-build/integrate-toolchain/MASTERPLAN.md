# 主计划 / MASTERPLAN — 工具链集成（integrate-toolchain）

> 所有功能宏观设计的**单一事实来源**。跨功能的任何决策以此为准。
> 每次修订都在末尾「§8 变更记录」追加一行。
>
> **前身**：`unify-launch/`（F01-F11 已交付，F10 未 commit）。本轮是它的教训**上推一层**：
> unify-launch 治的是「起会话」被写死成 N 套实现；本轮治的是「装/管一个工具」被写死成 N 套实现。

---

## 0. 核心思想（先读这一节）

> **把「一个工具怎么被装、被探测、被升级、被卸载、碰了用户哪些文件」
> 从每个工具各写一遍，变成「一份声明 + 一套统一机制」；
> 并让 cc-monitor 成为这套机制的唯一实现者与唯一审计入口。**

四条推论（任何设计决策与它们冲突，就是设计错了）：

1. **声明，不是实现。** 今天 `ccm` / `cc-acct-iso` / daemon / MCP / PowerShell 集成五套各写各的
   部署+探测+卸载。加第 N+1 个工具应该是 **+1 条声明**，不是 +1 套机制。
2. **边界即产品。** cc-monitor **写自己的地盘；别人的地盘只读、只诊断、只生成待贴文本**。
   这不是保守，是并发写者问题的唯一正确答案——Claude Code 自己也在写那些文件。
3. **能力自报，不是版本比对。** daemon 的 `hello` 帧、`ccm --ccm-probe` 已经证明了这条路；
   新工具一律走能力自报，禁止「版本号 >= X 就假定有某能力」。
4. **卸载与安装同等一等。** 今天只有 `strip_profile_block` 一条半卸载路径。
   任何装得上的东西必须卸得掉，且卸载路径与安装路径共用同一份「配置面申报」。

### 0.1 边界规矩（推论②的可执行版本）

| 地盘 | 归属 | cc-monitor 可以做什么 |
|---|---|---|
| `~/.local/bin/<tool>`（软链/可执行） | 自己 | 读写 |
| `~/.claude/skills/<tool>/`（vendored 源的落点） | 自己 | 读写 |
| `~/.claude-accts/` | 自己 | 读写 |
| 项目级 `<dir>/.mcp.json` | 自己 | 读写（沿用 SS-14 既有边界） |
| 远端部署产物（`~/.local/bin/ccm`、daemon） | 自己 | 读写 |
| `~/.bashrc` | **用户** | 只读 + 诊断 + **生成待贴文本** |
| `~/.claude/settings.json` | **用户** | 只读 + 诊断 + **生成待贴文本** |
| user/local scope 的 `~/.claude.json` MCP 段 | **用户** | 只读（**明确不做**写入） |
| `~/.codex/config.toml` / `~/.codex/hooks.json` | **用户** | 只读 + 诊断 + 生成 |

**唯一例外（须显式记档，否则规矩会被当成假的）**：`ccm` 在**运行期**会写
`~/.claude.json` / `~/.codex/config.toml` 的预信任字段（免得 detached 会话卡在信任确认框）。
它与本规矩不冲突，因为：① 是**运行期**行为不是安装期配置；② **窄字段**（每个项目目录一个 key）；
③ 先备份；④ 有 `CCM_NO_PRETRUST` 逃生口。
→ 规矩的准确表述：**安装期不写别人的配置；运行期只做窄字段写入且必须可 opt-out。**

### 0.2 目标与范围

- **总体目标**：cc-monitor 从「会话监视窗口」升格为「Claude Code 工具链的控制面」——
  一个地方装/管/审计全部周边工具，且**它自己就是这些工具的上游仓库**。
- **范围内**：
  - 受管工具注册表（声明 + 统一生命周期）。
  - 把 `cc-bus`、`code-picture` 两个项目**搬进本仓**成为上游。
  - 现有 5 套一次性机制（`ccm` / `cc-acct-iso` / daemon / 项目 MCP / PowerShell 集成）收编进注册表，**行为等价**。
  - 配置面审计视图 + 统一的「生成待贴文本」组件。
  - 设置面板信息架构重构（按受管工具组织）。
- **范围外**：
  - 写 `~/.bashrc`、写 `~/.claude/settings.json`、写 user/local scope MCP（用户 2026-07-28 明确定调）。
  - `allgent-picture` 语料迁移（用户 2026-07-28 明确：不管它，只要代码分析工具）。
  - 发版 / bump / push。新增轮询。daemon 协议改动。

### 0.3 成功标准

1. 加一个新受管工具 = 写一条 `ToolSpec` 声明 + 提供源，**零改**注册表内核 / UI 骨架 / 审计视图。
2. 任意时刻能回答「cc-monitor 动过我哪些文件」，且每一条都有对应的撤销动作或撤销指引。
3. `cc-bus` 与 `code-picture` 的源在本仓，改它们不需要碰仓外任何目录；
   本仓构建产物能完整重建两者在机器上的安装态。
4. 五套既有机制收编后**行为等价**——现有真机 e2e 套件逐条仍绿，不 re-baseline。

---

## 1. 功能清单（Feature Inventory）

| ID | 功能 | 一句话目标 | 状态 | 依赖 | 优先级 |
|----|------|-----------|------|------|--------|
| G01 | 信号修复：fmt + CI + e2e 进 CI | `cargo fmt` 28 处、分支从未跑过 CI、7 套真机 e2e(126 断言)在 CI 外 | 待做 | — | P0 |
| G02 | 伪测试扫荡 | 已确认 3 条；对每个功能的核心防线断言做变异检查 | 待做 | G01 | P0 |
| G03 | unify-launch 收尾 | F10 收口 commit；INVENTORY 重写（成功标准1 首次可验收）；账本 2 行未达成项定性 | 待做 | — | P0 |
| T01 | 受管工具注册表内核 | `ToolSpec` 声明 + 统一生命周期（源/落点/探测/装升卸/配置面申报） | 待做 | G01 | P1 |
| T02 | 配置面审计视图 | 「动过你哪些文件」+ 撤销动作/指引 | 待做 | T01 | P1 |
| T03 | 「生成待贴文本」统一组件 | bashrc 别名 / settings.json hooks / codex hooks 共用一套生成+复制+回读校验 | 待做 | T01 | P1 |
| T04 | 五套既有机制收编 | ccm / cc-acct-iso / daemon / 项目 MCP / PowerShell 集成 → 声明化，**行为等价** | 待做 | T01 | P1 |
| T05 | cc-bus 搬进本仓 | 1118 行 bash 成为仓内源；部署到 skills + symlink；钩子走「读+诊断+生成」 | 待做 | T01,T03 | P2 |
| T06 | code-picture 搬进本仓 | vendor 从「上游镜子」升格为**上游本体**（含 `tests/`）；旧 sibling 仓退役 | 待做 | T01 | P2 |
| T07 | 设置面板信息架构重构 | 从「按功能分节」改成「按受管工具 + 全局配置面审计页」 | 待做 | T02,T04 | P2 |
| T08 | skill 管理泛化 | cc-acct-iso / cc-bus 已是 skill；泛化成装/卸/看版本 | 待做 | T04,T05 | P3 |
| T09 | 多 agent 驾驶舱 | cc-monitor 看见/管理 bus 上的 agent、派活、读 inbox、`cc-spawn` 图形化 | 待做（**D6 已定：同批做**） | T05 | P2 |

---

## 2. 现状盘点：五套各写各的机制

横着看这张表——**源 / 落点 / 探测 / 装升卸 / 配置面**五个正交关注点，被拆成五套实现，
每套只实现了其中三四个。这就是本轮要治的病。

| 工具 | 源 | 落点 | 探测 | 卸载 | 碰的用户文件 |
|---|---|---|---|---|---|
| `ccm` | 仓内 `shared/ccm`（542 行 bash） | SFTP 写远端 `~/.local/bin/ccm` | `--ccm-probe capabilities=`（11 项） | `strip_profile_block`（只认自己围栏） | `~/.bashrc`、运行期 `~/.claude.json`/`~/.codex/config.toml` |
| `cc-acct-iso` | `src-tauri/vendor/cc-acct-iso` + `.vendor_id` 指纹 | `~/.claude/skills/` + symlink | 存在性检测 | 无 | `~/.claude-accts/` |
| remote daemon | `embedded-daemons/`（交叉编译内嵌） | 远端部署 | `hello` 帧自报 capabilities | 无 | 远端 |
| MCP（项目 scope） | 用户配置 | `<dir>/.mcp.json` | `read_mcp_servers` 宽容读三 scope | `remove_project_mcp_server` | `<dir>/.mcp.json` |
| PowerShell 集成 | 生成 snippet | `$PROFILE` | 扫 profile | `strip_profile_block` | `$PROFILE` |
| **cc-bus**（未集成） | 仓外 `~/.claude/skills/cc-bus/` | 已手工软链 | 无 | 无 | `~/.claude/settings.json`（钩子） |
| **code-picture**（半集成） | 仓外 upstream + 仓内**镜子** | 仓内 path 依赖 + `~/.local/bin/code-picture-mcp` | `build.rs::check_vendor_freshness` | 无 | 无 |

**可复用的最佳范式**（收编时以它们为最终形态，不要新发明）：
- 备份→写→**读回精确比对**→不符回滚：`profile_installer.rs` / `sftp.rs`
- vendored + 指纹 + 过期检查：`cc-acct-iso` 的 `.vendor_id` + `build.rs`
- 能力自报：daemon `hello` 帧 / `ccm --ccm-probe`
- 结构性扫描 > 固定 needle：`sftp.rs::ccm_cli_has_required_elements`（且已实证固定 needle 是空转的）
- 生成待贴文本：F08 的别名生成器 + 越层启动器诊断

---

## 3. ★共享面账本（Shared Surface Ledger）

| 共享面 | 涉及功能 | 最终形态 | 当前状态 | 备注 |
|---|---|---|---|---|
| `src-tauri/src/profile_installer.rs` | T01,T04 | 「备份→写→读回比对→回滚」提炼成注册表可复用的**通用写入器**，PowerShell profile 变成它的一个调用方 | 只服务 PowerShell profile | 不得为新工具另写一套写入器 |
| `src-tauri/src/sftp.rs` | T01,T04 | 远端写入走同一个通用写入器；`ccm_cli_has_required_elements` 的**结构性扫描**范式推广给所有部署产物 | 远端 profile 专用 | 结构性扫描是本仓质量最高的一处防线，推广不得降级为固定 needle |
| `src-tauri/vendor/` + `build.rs` | T01,T04,T05,T06 | 统一 vendor 布局：每个 vendored 工具一个子目录 + `VENDOR.md` + 指纹；`build.rs` 统一过期检查 | cc-acct-iso 有（6 文件指纹）、code-picture-core 有（freshness warning）、两套写法不同 | **T06 会反转 code-picture 的 vendor 语义**（镜子→本体），届时 `check_vendor_freshness` 应删除而非保留 |
| `src/settings/panel.ts` + 各 section | T02,T04,T07 | 从「按功能手写分节」改成「遍历注册表渲染 + 一个全局审计页」 | 826 行 panel + 5 个手写 section | T07 是本轮 UI 主战场；`mcp-section.ts` 的 SS-14 读写分界**必须原样保住** |
| `src/settings/mcp-section.ts` | T04,T07 | 并入注册表的 UI 骨架，但**读写边界不变**（只写项目 scope） | 643 行，边界清晰、注释完整 | 用户 2026-07-28 定调：全局 MCP 不做。这条边界是**加强**不是放松 |
| `~/.claude/settings.json` 的 cc-bus 钩子 | T03,T05 | cc-monitor **只读+诊断+生成**：装了 cc-bus 但钩子没挂 → 报「不会自动收信」；钩子指向失效路径 → 报出来；给出可复制的 JSON 片段 | 用户手工挂着（SessionStart→cc-register / Stop→cc-bus-stop-hook） | cc-bus 自己的安装脚本第 3 行就写着「不改全局 settings.json」——本仓沿用该边界 |
| `shared/ccm` | T04 | 成为注册表里的一条声明；本体不改 | 542 行，unify-launch 刚稳定 | **本轮不改 ccm 本体**（12 条 print-parity + 39 条 ccm-cli 是外部预言机，动它风险最高、可观测性最低） |
| `.github/workflows/ci.yml` | G01,T01 | 新增 ubuntu job 跑 7 套真机 e2e（隔离 `-L` socket，装 tmux 即可）；每个新受管工具的部署 e2e 挂进同一 job | 只有 shellcheck + py_compile 冒烟 | 126 条真机断言现在既不在 CI 也不在 `npm test` |

---

## 4. 依赖图与实现顺序

```
G01 ─┬─► G02 ─┐
     │        ├─► T01 ─┬─► T02 ─┐
G03 ─┘        │        ├─► T03 ─┴─► T04 ─┬─► T05 ─► T08
              │        │                  ├─► T06
              └────────┴─► (T07 需 T02+T04) ──► T07
                                             └─► T09（待定）
```

1. **G01/G02/G03 先做**：在信号不可信的基座上开集成工程 = 蒙眼开刀。三项合计工作量小，
   且新工程立刻受益（每个新受管工具都需要真机 e2e，而 e2e 现在不在 CI 里）。
2. **T01 是地基**，T02/T03 是它的两个必备面。
3. **T04 收编先于 T05/T06 搬迁**——先用 5 个**已知行为**的工具验证抽象，再拿它吃新工具。
   反过来做等于用一个未验证的抽象去接一个未集成的工具，两个变量一起动。
4. **T07 排在 T04 之后**：UI 重构要等注册表定型，否则重构两遍。

---

## 5. 横切关注点与约定

### 5.1 硬约束

1. **行为等价**：T04 收编五套既有机制后，现有真机 e2e 逐条仍绿，**不 re-baseline**。
2. **边界规矩**（§0.1）：安装期不写别人的配置。违反即阻塞。
3. **不改 `shared/ccm` 本体**（本轮）。
4. daemon 零改；`TMUX_LS_FMT` 双写点逐字节一致；不新增轮询；不用 emoji。
5. **卸载路径与安装路径同批交付**——不允许「先做装，卸以后再说」。

### 5.2 门禁纪律

- 所有门禁命令 `set -o pipefail`，输出重定向到文件后 Read/grep 核实，绝不信内联回显。
- `tsc` / `npm test` / `cargo test --all` / `cargo test -p code-picture-core` / `cargo fmt --check` 全绿。
  **`cargo fmt --check` 是 CI 唯一阻断性 Rust 门**（clippy/eslint/stylelint 都是顾问式）。
- **G01 之后**：7 套真机 e2e 进 CI，此后凡改部署/命令构造的功能，DoD 必须含真机行为验收表。
- **伪测试纪律**（G02 立的规矩，此后长期适用）：任何声称验证核心防线的测试，
  必须做一次**变异检查**（临时改坏被测实现，确认它真的变红），并把结论写进 feature 文件。
- 修 bug 走回归纪律：先写复现的失败测试再修。

### 5.3 审计纪律（用户 2026-07-28 指定）

每个功能的 Phase D 必须包含**一个独立的对抗性 agent**——不是复核实现，而是**论证这个功能
不该这么做 / 不该做**。prompt 自包含、带 §0 核心思想全文、明确要求「不要为了对抗而对抗，
核实后若认同就说认同并给证据」。UX 视角与实现视角都要覆盖。

---

## 6. 风险与开放问题

| # | 风险 | 缓解 |
|---|---|---|
| K1 | **code-picture vendor 语义反转**：今天是「上游的镜子」（SS-10 铁律：只照上游改），反转后 cc-monitor 成为上游。当初 vendor **刻意没搬 `tests/`**（"测试留上游"）——不一起搬就等于接手一个 4378 行的无测试内核 | T06 必须同批搬 `tests/`，并删掉 `build.rs::check_vendor_freshness`（语义已反转，保留会误导）+ 改写 `VENDOR.md` 为「本体，非镜子」 |
| K2 | **cc-bus 是运行中的活基础设施**：本会话身份就靠它认领，Stop 钩子挂着；且服务的不只 cc-monitor | T05 先固化一个基线（盘上有 3 个 `scripts.bak-*` + `cc-spawn`/`cc-whoami` 各一份 `.bak`，说明一直在手改）；部署走「备份→写→读回比对→回滚」；保留用户手工回退路径 |
| K3 | **并发写者**：Claude Code 自己也在写 `~/.claude.json` / `settings.json` | §0.1 边界规矩已把这些划为只读。若未来要放开，必须先解决并发写（JSON 无法用 BEGIN/END 围栏隔离） |
| K4 | **UI 重构撞在审计最密集的代码上**：`tabs.ts` flyout 有 F09 Phase D 的 1 阻塞 + 5 重要修复 | T07 只碰 `settings/`，**不碰 `tabs.ts`**；若必须碰，先把那 6 处修复各补一条回归测试再动 |
| K5 | **抽象过早**：注册表可能被设计成只服务已知 5 个工具的形状 | T04 的验收标准就是「五套收编后行为等价」；T05/T06 是对抽象的真实检验——若接第 6/7 个工具时要改内核，说明 T01 设计错了，回炉 |
| K6 | 搬进来后 cc-monitor 成为两个项目的上游，构建链变长；链坏了 = 实例间通信/代码分析一起坏 | 部署产物与 cc-monitor 运行时解耦（工具装在 `~/.local/bin`，不依赖 cc-monitor 进程存活）；保留「从仓内源手工部署」的脚本路径 |
| K7 | **T09 驾驶舱与「不新增轮询」红线冲突**：实时显示 bus 上 agent 状态天然想轮询，而 `~/.cc-bus/{agents.tsv,inbox/,spawned.tsv}` 是本机文件、cc-monitor 在 Windows 侧只能经 SSH 看 | T09 Phase B 必须先定读取模型：① 按需刷新（同 F10 用量探针的懒加载，用户点才查）②　复用 daemon 既有的 inotify watcher（daemon 零改红线下能否加订阅需单独论证）③ 明确放弃实时性。**默认取①**，除非 Phase B 论证出②可行且不破红线 |
| K8 | T09 的 `cc-spawn` 图形化与 unify-launch 未达成的账本行重叠 | `cc-spawn` 本体收编（建会话/送环境/送任务改经 `ccm`）正是 unify-launch 账本里未达成的一行。T09 应**顺带完成它**，而不是在 cc-monitor 侧再造第四套起会话实现——否则本轮亲手制造 unify-launch 刚消灭的病 |

---

## 7. 待用户决策（阻塞 Phase A 定稿）

见 STATUS.md「待决策」。

---

## 8. 变更记录

1. 2026-07-28 建档。基于用户三条定调（bashrc 不写 / 全局 MCP 不做 / cc-bus 与 code-picture 搬进本仓）+ 对现状的实测盘点。
