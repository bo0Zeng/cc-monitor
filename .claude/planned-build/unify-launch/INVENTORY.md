# 起会话入口全量清单（unify-launch 的**验收面**）

MASTERPLAN §0.1 **成功标准①**「`INVENTORY.md` 表里每个起会话入口都能带账号、带 tmux、
带未来参数，且**行为一致**」以本表为验收面。

> **2026-07-28 重写（R06）。** 上一版停在 F01 时点，之后 10 个功能一次没回改过，于是：
> 全部行号失效；§D 还列着 F09 已全仓删除的 ⇄ 与「对齐全部」；§C 描述的是 F02 已整体取代的
> 旧 4-block bashrc；§E/§F 描述的是改造**前**的状态。**这条验收标准因此从未真正被验收过。**
>
> **本版不再写行号，改用「符号名 + 可复跑的 grep 锚点」。** 理由就是上面这段：行号是
> 十个功能里最先腐烂的东西，而符号名腐烂时 `grep` 会返回空——**锚点自己会报错**，
> 不像行号会静默指向一段无关代码。每行末尾的 `grep` 命令可直接粘进终端复核。

## 怎么用这张表验收

对每一行问三个问题（这就是成功标准① 的可操作化）：
1. **能带账号吗**——这个入口能不能把用户选的账号送到最终 `claude` 进程？
2. **能带未来维度吗**——加一个新修饰（如 `--proxy`）时，这个入口是否**零改**就能透传？
3. **行为一致吗**——同一个修饰在这个入口和别的入口，效果是否相同？

---

## A. 远端 — 全部经 `launch_remote_terminal` → wt.exe/PowerShell → `ssh -t <host> "bash -lic '<载荷>'"`

`grep -n 'pub async fn launch_remote_terminal' src-tauri/src/launch.rs`

载荷由**双渲染器**产出（`renderCli` → `ccm …` / `renderFallback` → 裸 shell，逐字节等于改造前）：
`grep -n 'async function renderLaunchCommand' src/remote-launch-run.ts`（模块私有，非导出）

| # | 执行器（`src/remote-launch-run.ts`） | 起 tmux | **带账号/模型/未来维度** | `@ccm_sid` | UI 入口 |
|---|---|---|---|---|---|
| 1 | `runRemoteResume` | 否（除非 launcher 自建） | **经 `LaunchModifiers`** | 由 `ccm` 负责 | tab 右键 Resume（直连）· 历史右键 resume |
| 2 | `runRemoteResumeTmux` | 是（新建/幂等接回） | **经 `LaunchModifiers`** | 建时打 `@ccm_sid_expect` | tab Resume flyout（账号×容器）· 换号重启第⑤步 |
| 3 | `runRemoteResumeIntoExistingTmux` | 否（复用 idle 空壳） | **经 `LaunchModifiers`** | 沿用 | tab Resume flyout（复用 idle 分支） |
| 4 | `runRemoteLauncher` | 是（新建） | **经 `LaunchModifiers`** | 由 `ccm` 负责 | 设置→开新 Claude |
| 5 | `runNewSessionRemote` → 转发 `runRemoteLauncher` | 是 | **经 `LaunchModifiers`** | 同上 | 历史右键「在该目录起新会话」 |
| 6 | `runRemoteAttach` | 否（接回已有） | **不适用**（见下注） | — | tab Attach |
| 7 | SFTP「在此打开终端」`buildOpenTerminalCmd` | 否 | **否**（见下注） | — | SFTP 面板 |
| 8 | 设置→账号 `launchStep` / `buildAcctIsoCmd`（7 种 step） | 否 | **自带**（`cc-acct-iso run <名>` 同 shell 内设 env 再 exec） | — | 迁移预览/执行/自检/装 shell 集成/同步/加号/**登录** |

复核锚点：
```
grep -nE '^export async function run' src/remote-launch-run.ts
grep -n 'mods: LaunchModifiers' src/remote-launch-run.ts        # 期望 5 处（#1-#5）
grep -n 'buildOpenTerminalCmd' src/sftp/panel.ts src/remote-launch.ts
grep -n 'launchStep\|buildAcctIsoCmd' src/settings/accounts-section.ts
```

**#1-#5 已达成成功标准①**：五个执行器**同一个** `LaunchModifiers` 入参
（R03 落地，`grep -c 'mods: LaunchModifiers' src/remote-launch-run.ts` = 5），
且值由 `accounts.ts::withAccount` 统一解析后一次性塞进 bag——
「加一个新修饰 = `LaunchModifiers` 加一个字段」，这 5 行零改。

**#6 `runRemoteAttach` 不带账号是设计而非缺口**：attach 是接回一个**已经在跑**的进程，
它的账号在当初创建时就定了，此刻注入任何 env 都不会改变那个已存在进程的身份——
带账号在这里没有可实现的语义。故它不参与成功标准①（`canRenderCli` 允许 attach 走 CLI，
只挡 `send-into`，见 `doc/INVARIANTS.md` §33）。

**#7 / #8 是已知的两处未收编，不是遗漏**：
- **#7 SFTP 开终端**只是「在这台机的这个目录开个 shell」，不起 agent、不是起会话入口的一员；
  它出现在本表是因为它也走 `launch_remote_terminal`。要不要给它账号维度是产品问题，非本轮范围。
- **#8 账号 step** 自己就是**账号管理动作**（登录/迁移/自检），账号是它的**主语**而不是修饰，
  收进 `LaunchModifiers` 属于范畴错误。

---

## B. 本地 — 全部经 `launch_powershell_window`

`grep -n 'fn launch_powershell_window' src-tauri/src/launch.rs`

| # | Rust command | 构造器 | 带账号 | UI 入口 |
|---|---|---|---|---|
| 9 | `resume_history_session` | `build_local_ps_command`（`LocalPsAction::Resume`） | **无账号维度**（见下注） | tab resume 本地分支 · 历史 resume · 分支 resume |
| 10 | `new_local_session` | `build_local_ps_command`（`LocalPsAction::New`） | **无账号维度** | 历史右键起新会话（本地） |

复核锚点：
```
grep -n 'fn resume_history_session\|fn new_local_session\|fn build_local_ps_command\|enum LocalPsAction' src-tauri/src/history.rs
grep -n 'export function validateLocalLaunch' src/launch-requests.ts
```

**本地无账号维度是平台事实，不是未完成项**：Windows 本地没有 `CLAUDE_CONFIG_DIR` 隔离这套
账号模型（那是 `cc-acct-iso` 在 Linux 远端做的事）。
F06 已把两套逐字符重复的 PowerShell builder 收成一个 `LocalPsAction` 枚举驱动的
`build_local_ps_command`（黄金串对拍锁死重构前后逐字节同输出）。
**R07 已收尾**：原 `planLocal` 返回 `LaunchPlanBuild`、内部还跑一遍 `buildLaunchPlan`，
但**4 个**（不是 3 个）生产调用点全部丢弃返回值，真命令始终由 Rust 独立构造。
现已改名 `validateLocalLaunch`、返回 `void`、并删掉那遍构造——它是**纯前置校验**（sid 字符集）。
见 `doc/INVARIANTS.md` §36 与 `features/R07-plan-local-honest-name.md`。

---

## C. 终端侧（F02 已整体取代旧 4-block bashrc）

改造前这里有 4 个 bashrc block / 187 行 / 4 套并存实现。F02 后：**实现只有一份**
（`shared/ccm`，装成 `~/.local/bin/ccm` 可执行文件），bashrc 里只剩别名。

| # | 命令 | 实质 | 起 tmux | 账号模型 |
|---|---|---|---|---|
| 11 | `ccm [动作] [修饰…]` | **唯一实现** | `--tmux` 时 | `--account <名>` / `--base` / 继承 / manifest 默认号 |
| 12 | `cc` / `cch` / `cct` 等别名 | **组合层别名**（推论③） | 看别名带不带 `--tmux` | 同上，别名只是预置修饰 |

复核锚点：
```
grep -oE '^[a-z-]+\(\)' shared/ccm-aliases.sh          # 别名清单
grep -n 'CCM_VERSION\|--ccm-probe' shared/ccm          # 能力自省（供降级判据）
bash shared/ccm --help
```

**这一条是核心思想推论②③ 的落点**：终端敲的与 app 发出的是**同一条命令**，
用户自定义 = 给组合起别名而不是写新实现。**成功标准④**（终端起的会话 cc-monitor 无缝识别）
由 `ccm` 统一打 `@ccm_sid_expect` + poller 回填 `@ccm_sid` 保证；
两者的语义分叉由 R09 复核（见 §E）。

---

## D. 按 UI 位置（用户视角，F09 收敛后的现状）

1. **tab 右键（存活）**：Attach · **`Restart` 一级项 + 账号 flyout**（无容器轴）
2. **tab 右键（归档）**：**`Resume` 一级项 + 账号×容器 3 级级联 flyout**
3. **历史页**：行内 ↺ · 右键 resume /「在该目录起新会话」/「用账号 X resume」· 搜索卡片同套
4. **会话查看器**：在新终端 resume 此分支
5. **设置→远端机器**：开新 Claude（cwd / tmux 名 / 启动命令 / 账号下拉）
6. **设置→账号**：迁移预览/执行/自检/装 shell 集成/同步/加号/**每账号登录** + 每账号默认模型 + 用量
7. **SFTP**：在此打开终端
8. **状态栏 chip**：全局「当前账号」切换器（不起会话）+ 当前账号用量摘要

**改造前的 §D.3「tab 上的 ⇄」与 §D.9「Ctrl+K 对齐全部」已由 F09 全仓删除**，
故本版不再列。删除是**被测试钉住的**（负向断言）：
```
grep -rn 'alignAll\|countAccountMismatches\|account\.align-active' src/ --include=*.ts
# 期望只剩 2 条：account-chip.vitest.ts 与 keybindings/actions.vitest.ts 里的"不再存在"断言
```

---

## E. 成功标准① 的逐条判定（2026-07-28）

| 判据 | 状态 |
|---|---|
| 远端 5 个真正的「起会话」执行器（#1-#5）能带账号/模型/未来维度且**同一个入参型** | **达成**（R03） |
| 同一修饰在这 5 个入口效果一致（同一套 `LAUNCH_DIMENSIONS` + 同一对渲染器） | **达成**（F03/F05/F07） |
| `runRemoteAttach`（#6）不带账号 | **达成——不适用**，见 §A 注 |
| SFTP 开终端（#7）/ 账号 step（#8）不带账号 | **有意在范围外**，见 §A 注 |
| 本地两条（#9/#10）无账号维度 | **达成——平台事实**，见 §B 注 |
| 终端侧（#11/#12）与 app 同源 | **达成**（F02，`ccm-print-parity` 12 条外部预言机守着） |
| ~~遗留：`planLocal` 返回值被全部丢弃~~ → **R07 已收尾**：改名 `validateLocalLaunch`、返回 `void`、删掉那遍 `buildLaunchPlan`（纯前置校验） | **已完成** |
| **遗留**：`@ccm_sid`（事实）vs `@ccm_sid_expect`（意图）语义分叉是否伤到标准④ | **R09 复核** |

---

## F. 历史诊断（改造**前**的状态，保留作为设计依据，勿当现状读）

> 以下三节是 2026-07-27 改造前的实测诊断。它们**不是现状**，但是整套设计的论证基础
> （MASTERPLAN §2.1 那段「账号注入的成败只取决于 export 落在 tmux 进程边界的哪一侧」由此而来），
> 故原样保留。

### F.1 三个用户意图 vs 当时现状

**A「把这个会话再跑起来」——10 个入口，4 种后端行为。**
「直连 / tmux / 基座 / 账号」是四条**正交**维度，当时被摊平成并列菜单项做排列组合，
于是同一级菜单里同时出现「Resume（tmux）」和「用基座 resume（tmux，不隔离）」。
→ 已由 F09 收敛成「一级动作 + 二级修饰 flyout」。

**B「起一个新会话」——3 个入口，2 种行为。**
历史那条起 tmux 但不让选账号（跟随）；设置那条起 tmux 且可选账号。

**C「换个账号跑」——9 个入口，5 种机制，只有 2 个真的生效。**

| 入口 | 机制 | 当时现状 |
|---|---|---|
| 设置→账号→登录 | `cc-acct-iso run` 同 shell 内设 env 再 exec | **有效** |
| 设置→开新 Claude→账号下拉 | export 写进 send-keys 载荷**内** | **有效** |
| tab 右键「切到账号 X（重启）」/「先压缩再重启」/ ⇄ / Ctrl+K 对齐（单个/全部） | 需精确 `@ccm_sid` + `cc-*` 名 | 基本全废 |
| tab 右键「切到账号 X（resume）」/ 历史右键「用账号 X resume」 | export 在 `cct` **外层** | **被 tmux 边界吃掉** |
| 状态栏 chip | 只改设定 | 名字像切号、其实不切 |

九个地方让你选账号，两个管用，而且都在设置里——最不像「我要换号跑这个会话」的地方。

### F.2 失效根因索引（→ 负责修的功能）

| 根因 | 证据 | 修它的功能 |
|---|---|---|
| `export` 落在 tmux 进程边界外被吃掉（`cct`） | `update-environment` 默认列表不含 `CLAUDE_CONFIG_DIR`（实测） | **F02** |
| 「开新 Claude」起的会话永不带 `@ccm_sid` | `buildLauncherCmd` 调 `createRunAttach` 未传 `ccmSid`；载荷是裸 `claude` | **F04** |
| 终端 `cct` 会话名 `<dir>_cc` 被 Rust 白名单拒 | `is_ccm_tmux_name` 只认 `cc-` 前缀 | **F04** |
| `-t <name>` 是前缀/glob 匹配不是精确匹配 | 实测 `kill-session -t sib` 杀掉 `sib-2` 且 rc=0；`-t 'si*'` glob 命中 | **F01** |
| 通道A 的 `@ccm_sid` 焊死建时 sid，`/branch` 后漂移 | 审计 D6 | **F04** |
| 本地路径完全没有账号维度 | `history.rs` 两个 command | **F06**（判定为平台事实，见 §B） |
| 已证伪：wrapper 读死 `~/.claude/sessions/` | `~/.claude-accts/*/sessions` symlink 回 `~/.claude`，同一 inode | 无需修 |

### F.3 改造后新发现、已修的两条（不在原诊断里）

| 根因 | 证据 | 修它的 |
|---|---|---|
| 选中的非默认账号被 `ccm` 自己的默认号回退**静默覆盖** | 真机复现：外层 export 账号 b，`ccm --print` 输出账号 z | **R11**（`ef1310b`） |
| 同上症状在**容器路径**残留——继承值穿不过 tmux 边界 | 实测复现：`--tmux` 时内层载荷无 `--account`/`--base`/export | **R08**（`9dc0aad`） |
