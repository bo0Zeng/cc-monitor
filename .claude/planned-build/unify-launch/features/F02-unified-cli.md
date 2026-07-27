# 功能计划 — F02 统一启动 CLI `ccm` + 重构 bashrc

> 对应主计划 §1 的 F02。本文件是该功能从规划到签收的全程记录。
> **动手前先读 MASTERPLAN §0 核心思想** —— 本功能是那个思想在 shell 侧的投影。

## 1. 目标与验收标准（DoD）

- **目标**：把「起一个会话」在 shell 侧实现成**一个动作 + 若干正交修饰**的单一命令，并用它
  **整体取代** `~/.bashrc` 里 4 个 block（119-168 / 169-205 / 210-236 / 238-305，共 187 行、
  12 个命令、2 套账号模型）。从此终端敲的和 app 发的是同一条命令。

- **验收标准**：
  - [ ] `ccm` 作为**可执行文件**装在 `~/.local/bin/ccm`（不是 shell 函数；zsh/fish 同样可用）
  - [ ] 契约（§3）全部实现：`new`/`resume`/`attach` × `--tmux` / `--account|--base` / `--cwd auto|<dir>` /
        `--agent claude|codex` / `--launcher` / `--ccm-sid` / `--print` / `--ccm-probe`
  - [ ] **零维度调用逐字节等于今天**：`ccm`（无修饰）产出的最终 exec 与今天 `ccm()` 函数一致
  - [ ] **`--resume <sid>` 与 `resume <sid>` 等价** → 用户把设置里「远端 resume 命令」填 `ccm`，
        **cc-monitor 前端零改动**即修好「用账号 X resume 无效」（本功能独立可验的关键）
  - [ ] **账号注入穿得过 tmux 边界**：`ccm --tmux --account z` 起的会话里 `CLAUDE_CONFIG_DIR` 正确
        （对照今天 `cct` 的失败：export 落在 tmux 外被 `update-environment` 吃掉）
  - [ ] **身份随行**：`ccm --tmux` 起的会话带 `@ccm_sid`，cc-monitor 能 attach / 能重启
  - [ ] 会话名 = `cc-<safe-basename>`，与 `deriveTmuxName` 同规则（终端与 app 同目录造同名 → 幂等接回）
  - [ ] `--cwd auto` 与 `_cc_resolve_target` **行为逐字节相同**（$HOME→工作区 / git 仓→父目录 / 否则当前）
  - [ ] 代理注入等价于今天的 `eval "$CC_ENV"`（配置一次性迁到 `~/.config/ccm/config`）
  - [ ] bashrc 重构：4 block → 1 block，只剩别名；**落盘前出完整 diff + 备份**
  - [ ] vendored 进 cc-monitor + 一键部署到远端（照 F5 的 `cc-acct-iso` vendor 范式）+ `--ccm-probe` 自检
  - [ ] 门禁：`npm test` / `cargo test` / `tsc` / `npm run test:tmux-target` 全绿
        + **CLI 自己的 shell 级测试** + **真机行为验收表**（§5）

- **明确不做什么**：
  - 不改 cc-monitor 的渲染路径（`LaunchPlan` IR + CLI 渲染器是 **F03**）。F02 只让 CLI 存在且好用，
    前端仍走今天的 builder——靠 `--resume` 等价性拿到即时收益。
  - 不动 `is_ccm_tmux_name` 的身份语义、不做三道门（**F04**）
  - 不做 AccountResolver（**F05**）、不做本地路径（**F06**）、不做每账号模型（**F07**）
  - 不删用户 bashrc 里与 claude 无关的东西（`proxy-on/off/status`、PATH、cargo/deno env 等）

## 2. 与主计划的对接

- **触及的共享面**（对照 §3 账本）：
  - `shared/ccm-wrapper.sh` → **CLI 本体**。账本最终形态：参数化 CLI、独占 L1/L2/L5、vendored 可部署。本功能落地它。
  - **用户 `~/.bashrc` 的 4 个 block** → 整体取代为一个只含别名的 block。账本已明确「能力必须迁移」
    与「真删」的分界，严格照办。
  - `src-tauri/src/sftp.rs` 的 `install_remote_ccm_helper` + needle 守卫 —— 从「往 bashrc 塞一段函数」
    改为「部署可执行文件 + 写别名 block」。**审计 C7 警告**：现有 needle 含 `"exec claude"`，本功能会打破它，
    必须同步更新（不是"顺手"，是硬同步点）。
  - `src/settings/remote-section.ts` 的 ccm 安装/卸载 UI + `CCM_WRAPPER_SNIPPET` raw import。
- **遵循的最终形态**：CLI 是**唯一实现**；bashrc 只剩组合层别名；cc-monitor 侧的渲染改造留 F03，
  但 F02 就要把 `--print` / `--ccm-probe` 这两个 F03 依赖的接口做出来（否则 F03 得回头补）。
- **本功能的边界**：不碰前端 builder / executor 结构；不碰 Rust 控制面（tmux.rs）。

## 3. 接口 / 契约设计

```
ccm [动作] [修饰…] [-- 透传给 agent 的参数]

动作（缺省 = new）
  new                     起新会话
  resume <sid>            恢复会话
  attach <名>             接回已有会话
  # `--resume <sid>` 是 `resume <sid>` 的等价别名（向下兼容 cc-monitor 今天的拼法）

修饰（全部正交、可任意组合、可缺省；缺省即今天的行为）
  --tmux[=<名>]           容器。缺省名 = cc-<safe-basename>；同目录**幂等接回**（不避让）
  --account <名>          注入该账号的 CLAUDE_CONFIG_DIR（从 ~/.claude-accts/accounts.json 解析）
  --base                  显式不注入（issue #75 逃生口）
  --cwd auto|<dir>        默认 auto = 复刻 _cc_resolve_target
  --agent claude|codex    默认 claude
  --launcher <cmd>        覆盖该 agent 的默认启动命令
  --ccm-sid <sid>         身份打标（cc-monitor 已知 sid 时传）

自省
  --print                 打印将要执行的命令，不执行（F03 的等价断言 + 调试）
  --ccm-probe             打印 name/version/capabilities（安装自检 + cc-monitor 降级判据）
  --version / --help
```

**分层职责**（对应 MASTERPLAN §2.2 的所有权表）：

| 层 | CLI 负责 | 说明 |
|---|---|---|
| L1 容器 | 建 tmux / 送入 / attach | **独占**。用户别名只传 `--tmux`，不自己建 |
| L2 环境 | cwd、`CLAUDE_CONFIG_DIR`、代理、unset 嵌套 env | **必须在最终 exec 的那个 shell 里设**——这是全部病根 |
| L3 启动器 | 按 agent 轴取默认，`--launcher` 可覆盖 | |
| L4 参数 | resume flag 按 agent 轴，`--` 后透传 | |
| L5 身份 | `@ccm_sid` 两通道 | 已知 sid → 建后即 set；未知 → rbind poller（今天的 `__ccm_rbind` 收进来） |

**agent 轴差异**（CLI 主体零分支，全部查表）：

| | claude | codex |
|---|---|---|
| 默认启动器 | `claude` | `codex` |
| resume flag | `--resume` | （无，`resume` 动作报错提示） |
| 嵌套 env unset | `CLAUDECODE` 等 4 个 | 无 |
| 身份回填 | `~/.claude/sessions/$PID.json` 轮询 | **无 per-PID session 文件** → 不起 poller |
| `@ccm_agent` | `claude` | `codex` |

**关键结构（审计 C1/D3/D4 的沉淀，必须这样写）**：
- 最终一定是 `( <身份注册>; exec <launcher> <args> )` 的**包裹**结构——`exec` 不能省（rbind 用
  `$BASHPID` 读 session 文件，不 exec 则 PID 对不上）。
- `exec` 展开不了别名/函数 → `--launcher` 若是 shell 函数，走 `bash -ic '<fn> "$@"'` 间接层。
- 身份注册用**运行时自适应**而非编译期门控（审计 D1 已证伪"未装会话立死"）：
  `command -v <reg> >/dev/null 2>&1 && <reg>` —— 装了就用，没装逐字节退化。

## 4. 实现步骤（严格顺序执行）

- [x] **步骤 1**：写 CLI 骨架 `shared/ccm`（POSIX sh 兼容优先，bash-only 特性只在必要处）：
      参数解析 + `--help` / `--version` / `--ccm-probe` / `--print`。
      — 验证：`--print` 对零修饰调用产出与今天 `ccm()` **逐字节相同**的 exec 串（钉成黄金串）。
- [x] **步骤 2**：agent 轴查表（claude/codex）+ L3/L4（launcher / resume flag / 嵌套 env）。
      — 验证：`--print` 对 4 组组合（agent × resume）的黄金串。
- [x] **步骤 3**：L2 环境——`--cwd auto` 复刻 `_cc_resolve_target`、账号解析、代理注入。
      — 验证：`--cwd auto` 与旧函数在 5 种目录布局下**输出同一路径**（对拍测试）。
- [x] **步骤 4**：L1 容器——`--tmux` 建会话 + 送载荷 + attach；名字派生与撞名后缀。
      **所有 `-t` 用 `=名:`**（INVARIANTS §31a）。
      — 验证：接入 `npm run test:tmux-target` 的同一套真机 harness。
- [x] **步骤 5**：L5 身份——`@ccm_sid` 两通道 + `@ccm_agent`；rbind poller 收进 CLI。
      — 验证：真机起会话后 `tmux show-options -v -t '=<名>:' @ccm_sid` 读得到。
- [x] **步骤 6**：**账号穿透验收**（本功能的核心证据）——`ccm --tmux --account z` 起的会话里
      `CLAUDE_CONFIG_DIR` 必须正确；并**对照**今天 `cct` 的失败路径，证明修好了。
- [x] **步骤 7**：vendor 进 cc-monitor + 部署 IPC + `--ccm-probe` 自检；更新
      `sftp.rs` 的 needle（**审计 C7：现有 needle 含 `"exec claude"`，会被打破**）与
      `remote-section.ts` 的安装 UI 文案。
      — 验证：`cargo test` 的 needle 守卫绿（且证明非空转：把 CLI 的 `=名:` 改回裸目标当场红）；
        以**可执行文件形态**（install 到临时 PATH，不带 `CCM_SELF`）跑通 `--ccm-probe` / `--print` /
        `--help`，`$0` 自解析正确。**真机远端部署留到步骤 8 一起做**（两者都动用户系统，同一道门禁）。
- [ ] **步骤 8**：生成新 bashrc block（只含别名）+ 完整 diff + 备份。
      **先把 diff 呈给用户看过再落盘。**
      — 验证：新 shell 里 `cc`/`cct`/`zcct`/`oot` 全部可用；旧命令行为对照表逐条过。
- [ ] **步骤 9**：三门禁 + `test:tmux-target` + CLI shell 级测试，结果重定向落盘后 Read 核实。

## 5. 测试策略

- **CLI 单元（shell 级）**：新建 `e2e/ccm-cli.test.sh`——全部走 `--print`，断言命令串。
  覆盖：零修饰逐字节等价 / 各修饰单独 / 组合 / `--resume` 与 `resume` 等价 / 非法参数报错。
- **对拍测试**：`--cwd auto` vs 旧 `_cc_resolve_target`，5 种目录布局输出同一路径。
- **真机行为验收**（MASTERPLAN §5.2 常设门禁）：复用 `e2e/tmux-target-acceptance.sh` 的 harness 形状，
  新增 F02 专属场景：
  | 场景 | 期望 |
  |---|---|
  | `ccm --tmux --account z` | 会话内 `CLAUDE_CONFIG_DIR` = z 的目录 |
  | 对照：今天的 `cct` 路径 | `CLAUDE_CONFIG_DIR` **丢失**（证明这就是要修的） |
  | `ccm --tmux` | `@ccm_sid` 被写上；`@ccm_agent` = claude |
  | `ccm --tmux --agent codex` | `@ccm_agent` = codex；不起 rbind poller |
  | 同目录连开两次 `ccm --tmux` | 幂等接回同一会话，不产孤儿 |
  | `ccm --tmux` 后用 cc-monitor 的 Rust 形态 kill | 能命中（过 `is_ccm_tmux_name`） |
  **探针载荷用 `CCMPROBE` 不用真 claude**（F01 踩过：真 claude 清屏 → 假 PASS）。
- **回归**：`sftp.rs` needle 守卫、`remote-section.ts` 的 raw import 路径。
- **修 bug 纪律**：账号穿透那条先写复现失败（对照 `cct` 路径）再修。

## 6. 代码审计结果（Phase D）

两个独立 agent（后端架构 / UX 收敛），prompt 自包含并带 MASTERPLAN §0 核心思想全文。均报告**阻塞**，
两条是我自己在实现中造成的**净退化**（比要修的原 bug更坏），已全部修复并复验；另有一批重要发现，
已分诊：能在 F02 内低成本修的当场修，属于其他功能范围的记入账本/风险表推迟。

**阻塞（已修复，均有真机复验）**：

| # | 发现 | 处置 |
|---|---|---|
| B1(UX) | `--account` 打错字：`die` 在 `$(...)` 里只杀子 shell，主流程**照跑**、rc=0，落到继承来的账号上——比"换号不生效"更坏，是"生效到错账号上" | 账号解析改成"只回显值、绝不 die"，顶层判空后中止；错误信息带出可用账号列表 |
| B2(UX) | 不传 `--account`：cc-acct-iso 搬走凭据后基座常无 `.credentials.json`，`cc`/`cch`/`cct` 会掉进未登录目录 | 落 manifest 的 `isDefault`（复刻旧 `_cc_acct_last` 粘滞体验）；`--base` 仍是显式逃生口 |
| B1(架构) | `--cwd auto` 默认对 `resume`/`attach` 也生效：cc-monitor 已 `cd` 到会话目录，CLI 又解析一次会跳到 git 仓**父目录**，Claude 按 `projects/<enc(cwd)>/<sid>.jsonl` 找不到会话——直接打掉 F02 头号 DoD | `auto` 只对 `action=new` 生效；`resume`/`attach` 用调用方已定位的 `$PWD` |
| B3(架构) | needle 守卫空转：把 CLI 的 `=名:` 全改回裸目标，`cargo test` 依旧全绿 | 改结构性扫描（逐个 `-t ` 目标窗口断言 `=` 在前 `:` 在后）+ 把间接变量 `$t` 的定义逐字钉死；三种退化各试一次都变红 |

**真机测试自己额外抓出的 3 条（不在两份书面审计里，是我按用户要求真起终端时发现的）**：
- `resume`/`attach` 后跟 flag 会被当成值吃掉（`ccm resume --tmux` 把 `--tmux` 当 sid）→ 补位置参数校验
- **六个带值 flag 全部缺"下一个 token 是否是漏填"校验**（`--account --print` 会把 `--print` 当账号名）→ 统一加 `need_val` 守卫，真机复现过一次（漏到用户真实 tmux 上，已清场）
- 中文目录名全塌成 `cc-session`，配合"无避让+同名接回" → 在不同中文目录敲 `cct` 会接回错的会话 → 加同名异目录退让 + 已在 tmux 内时就地起（不建嵌套，复刻旧 `cct` 的 `$TMUX` 分支）

**重要（记入账本/风险表，推迟到对应功能）**：
- `--tmux` 幂等短路会 attach 进空壳（idle-tmux 复用，前端已有 `runInExistingAttach` 但 CLI 未实现同等能力）——**记入 F04**（idle-tmux 语义本就是它的范围）
- agent 轴不一致：`adapter/codex.rs` 明写 codex 有 `resume` 子命令，CLI 说它没有——**记入 F06**（本地 Rust 侧统一时一并核对 agent 轴）
- `--ccm-probe` 无消费者（安装自检/降级判据管线未接）——**记入 F03**（双渲染器降级正是消费它的地方）
- `--tmux` 的 inner 透传是手工枚举，新维度只在此处漏改会静默丢——**记入 F03**（IR 维度注册表落地后自然消解）
- `--help` 对新用户不够（无例子、不提别名/配置文件、无法列账号）——**记入 F08**（终端集成收尾，用户体验向）
- 加第三个 agent 要改 8 处、零漂移守卫——**记入 F07**（每账号模型是"加维度"的架构验收，届时一并把 agent 轴的可扩展性做扎实）

**Phase E 内直接处置的清理项**（不算功能债，顺手做掉）：
- 孤儿凭据快照 `~/.claude/accounts/{z,b,.last}`（含真 token，消费者已随 bashrc 重构删除）——已确认无引用后删除
- `doc/INVARIANTS.md` §30 仍提已删除的 `shared/ccm-wrapper.sh`/`__ccm_rbind`；§31a"三处同源"漏了 CLI 本体（现四处）——已同步更新

## 7. 工程审计结果（Phase E）

- **主计划是否仍自洽**：是。F02 落地账本里「CLI 本体」「bashrc 重构」两行的最终形态；未触碰 F03 的
  IR/渲染器结构、未触碰 F04 的三道门、未触碰 Rust 控制面（`tmux.rs` 除 needle 守卫改动外零改）。
- **是否引入拖累后续的债**：有，已记账（见上表六条），全部有明确归属功能，不是无主债务。
  最重的一条是 idle-tmux（记 F04）——F04 本就要做三道门 + 身份精确判定，idle 检测是同一批工作。
- **账本预见的重叠 → 现在就做的统一重构（铁律 6）**：
  - `--ccm-probe`/`--print` 接口在 F02 就位，虽然 F03 才真正消费，避免了 F03 回头改 CLI 契约。
  - 会话命名统一为 `cc-<safe-basename>`（与 `deriveTmuxName` 同规则）在 F02 就做了，不留给 F09/F04
    再解决"终端与 app 命名不一致"的问题。
  - INVARIANTS §30/§31a 的过期引用当场修正，不留到 F04 再发现文档漂移。
- **工程健康度**：三门禁全绿（tsc 0 / npm test 598 + 13 个 tsx 套件 / cargo 370）；新增专属门禁
  `test:ccm-cli`（32 项）、`test:ccm-acceptance`（12 项，真机对照实验）全绿；`test:tmux-target`（26 项，
  F01 遗留）仍绿。真机端到端验证：终端 `cct` 起真 claude、账号穿透（对照组证明旧 `cct` 路径丢账号）、
  身份两通道（建时打标 + poller 回填，2 秒内）、cc-monitor 六列齐全（名字过白名单/`@ccm_sid`
  命中/capture-pane/send-keys 全通）。文档-代码无新增漂移。
- **反馈到主计划**：账本新增 F11（cc-spawn 并入 ccm，预信任能力上提）；R10（一个 sid 匹配多个活会话，
  F04 须根治）；六条 F02 未尽事项按功能分派（见上）。

## 8. 签收（Sign-off）

- [x] 通过代码审计（阻塞项已全部修复；重要发现按功能分派，无遗漏无主债务）
- [x] 通过双 agent 架构/UX 审
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录）
