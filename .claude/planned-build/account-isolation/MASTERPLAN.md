# MASTERPLAN — 多账号「隔离又同步」通用管线 + cc-monitor 按会话切账号

> 新功能族(bug sweep 已收官)。设计先行,过用户审批门禁后再动手。归并/推进 issue #68(多账号集成)/#69(resume 指定账号)。

## 目标(Goals)
- **G1 并发隔离 + 同步**:各账号独立 `CLAUDE_CONFIG_DIR`(各自 `.credentials.json`,两号可**同时跑不互踢**、token 各刷各的);同时 skills/memory/history/settings/plugins **实时共享**(symlink 到同一共享库)。
- **G2 通用模块**:参数化(共享库路径 / 账号目录 / 隔离集 / 共享集 / launcher / 账号名任意 / N 个),**不硬编码** `~/.claude`、`z/b`、本用户。
- **G3 用户自跑的安全管线**:内建 dry-run / 备份 / 逐步 / 自检验证 / 一键回退;**用户空闲时自己跑**动凭据那步,我**不擅自改用户 `~/.claude`**。
- **G4 cc-monitor 按会话切账号**:resume/新会话选账号 → 用该账号 `CLAUDE_CONFIG_DIR` 起(**远端优先**);显示当前账号、默认跟随 live。

## 非目标(防蔓延)
- **不做本地 Windows 账号隔离**(远端优先;Windows 无 bash 模块,%USERPROFILE% 隔离另议 → future)。
- **不做「运行中的会话中途换号」**(切号=下次启动生效;单会话运行期不换——凭据启动时读)。
- **不自动迁移用户 `~/.claude`**(工具建好,迁移由用户跑管线;我只在沙盒测)。
- **不改 `cc-<sid8>` 会话名契约**(§3,与 aterm 锁定)。
- **不读/传/显示凭据 token 内容**(只搬/链文件、只列账号名/邮箱/路径)。

## ★load-bearing 假设(必须最先验证——Phase 0)
整个设计压在一条上:**设 `CLAUDE_CONFIG_DIR=<dir>` 后,Claude Code 的 `.credentials.json` 与 `.claude.json` 都落在 `<dir>/` 下**(而非恒 `$HOME`)。
- 证据(间接):cc-monitor `mcp.rs::claude_json_candidates` 先查 `$CLAUDE_CONFIG_DIR/.claude.json`;credentials 惯例在 config dir 内。
- **风险**:CC 版本差异可能让 `.claude.json` 仍在 `~/.claude.json`、或 credentials 路径不同 → 隔离失效。
- **Phase 0 gate**:动工具前,在**真 CC**上实测 `CLAUDE_CONFIG_DIR=/tmp/xxx claude` 是否在 `/tmp/xxx` 下建 `.credentials.json`(+ `.claude.json`)。不成立则整设计需改(退回「swap 单文件顺序切换」= 现状,无并发)。**此 gate 由用户在真机确认**(或我经 daemon 在远端跑一次只读探测,不登录)。

## 架构 / 模块边界
```
┌─ M1 通用 shell 模块 cc-acct-iso ─────────────┐        ┌─ M3 cc-monitor 按会话切号 ─┐
│ 参数化 config(共享库/账号目录/隔离集/共享集/  │  写      │ daemon --list-accounts     │
│ launcher)。命令:init/add/rm/list/which/     │ ──────▶ │  (读远端 manifest、不返凭据) │
│ verify/sync/rollback + 隔离启动器。          │ M2      │ remote-launch 注入          │
│ dry-run + 备份 + 不读凭据。                   │ manifest │  CLAUDE_CONFIG_DIR=<dir>    │
└──────────────────────────────────────────────┘ 契约 ◀── │ UI 选账号/显示当前/默认跟随 │
        (账号库 + 工具在**远端 Linux**)                    └────────────────────────────┘
```
- **M1**:通用账号隔离工具(bash,远端跑)。归属决策见「开放决策①」。
- **M2 manifest**:`$ACCTS_DIR/accounts.json`,通用 schema —— shell 工具**写**、cc-monitor**读**。**核心共享面/契约。**
- **M3**:cc-monitor 消费——daemon 只读命令读远端 manifest + 前端按账号注入 `CLAUDE_CONFIG_DIR`。

## manifest 契约(M2,通用 schema —— 账本最终形态,**A1 已落地**)
```json
{
  "version": 1,
  "updatedAt": "2026-07-23T18:00:00Z",
  "sharedStore": "/home/<user>/.claude",
  "acctsDir": "/home/<user>/.claude-accts",
  "accounts": [
    { "name": "z", "email": "z@x.edu", "configDir": "/home/<user>/.claude-accts/z", "isDefault": true,  "mode": "isolated" },
    { "name": "b", "email": "b@y.com", "configDir": "/home/<user>/.claude-accts/b", "isDefault": false, "mode": "isolated" }
  ]
}
```
- 路径全绝对、由工具按机器填(通用)。email 由工具从各账号 `.claude.json oauthAccount.emailAddress` 读(**不含 token**)。cc-monitor 只读此文件拿「有哪些账号 + 各自 configDir」。
- `updatedAt`(消费端判新鲜度)/ `mode`(`isolated` | `in-place`)是 Phase D 工程审计后加的 —— A2/A3 未开工,趁早定型省得以后 bump schema。
- **`mode: "in-place"` 的账号 cc-monitor 应直接拒绝**:那是逃生口模式,`.claude.json` 会分裂(裸起与带 env 起读不同文件)。
- **消费方规则(A2/A3 必须遵守)**:
  1. `configDir` 是**不可信字符串**。工具已在源头拒绝 `' " \ ` $ ; | & < > * ? ( ) !` 与控制字符,但仍可能含空格/非 ASCII → 注入环境变量**优先用不经 shell 的方式**(`Command::env`);必须拼 shell 就自己转义 + 白名单校验,校验失败拒绝该账号。
  2. manifest 写入是**原子**的(临时文件 + `mv`),读者不会看到半截 JSON;**写者**之间用 `flock $ACCTS_DIR/.lock` 互斥,**读者免锁**。
  3. `$ACCTS_DIR` 是 700 → 消费方进程必须与账号库**同 uid**。
  4. **不要自己写 `accounts.json`**;要增删账号就 shell out 到 `cc-acct-iso add/rm --apply`。
  5. 要「实时邮箱 + 是否已登录」用 `cc-acct-iso list --json`(进程级稳定接口,含 `exists`/`loggedIn`),别把 on-disk 格式当 API、也别自己重新定义"什么算已登录"。

## 隔离/共享策略(基于真机 `~/.claude` ls,可 config 覆盖)
- **隔离(账号态,各账号真实文件)**:`.credentials.json`(必)、`.claude.json`(荐,含 oauthAccount + 避免并发写 114KB 竞争)、(可选)`policy-limits.json`/`stats-cache.json`(账号级限速/用量)。
- **共享(symlink 到共享库)**:`skills/`、`projects/`(**历史 + memory 都在此**)、`settings.json`、`plugins/`、`CLAUDE.md`、`sessions/`(共享后 cc-monitor 同屏见两号)、`claudecode-frontend/`、`history.jsonl`、其余 cache/telemetry 类。
- **实现形态**:每账号 config-dir = 对共享库每个条目建 symlink,**除隔离项**是真实文件。做成幂等脚本(CC 升级新增顶层文件 → `sync` 补链)。

## ★共享面账本(最终形态)
- **manifest schema(M1 写 / M3 读)**:上面 §契约定死。改字段=两端同步。
- **`src/remote-launch.ts` 载荷构建**:#69(多命令)+ 本功能(注入 `CLAUDE_CONFIG_DIR`)都改它。**最终形态**:载荷 = `<envPrefix> unset <nested>; [cd '<cwd>' &&] <launcher> [--resume <sid>]`,`envPrefix` = 可选 `export CLAUDE_CONFIG_DIR='<configDir>'; `(configDir 过 posixQuote/校验)。与已提交 #72 的 `@ccm_sid` set-option **协调**:CLAUDE_CONFIG_DIR 是 payload 内 export(send-keys 进会话),@ccm_sid 是 create 序列的 tmux set-option;两者正交、都在 buildResumeTmuxCmd 一带 → 账本记明,勿互踩。
- **daemon 一次性命令派发(`history_query.rs`)**:#64(panorama/agent-viewer)+ 本功能 `--list-accounts` 都加命令 → 照 `--read-session` 范式,账本记明。BUILD_ID bump。
- **cc-monitor 账号模型(前端)**:#68 的「发现/显示/切换」+ #69 的「resume 指定账号」+ 本功能「按会话 CLAUDE_CONFIG_DIR」= 同一账号模型,一处实现(`src/accounts.ts` + 状态栏 chip + 设置面板 + 右键菜单/历史动作表)。
- **daemon 只读铁律(`doc/INVARIANTS.md §1`)**:daemon 绝不写 `~/.claude/`。故账号功能里**只读部分走 daemon**(list/探测/trust 预检),**动凭据的 `--apply` 一律弹终端窗口让用户亲手跑**。daemon 内若真要跑 `cc-acct-iso`,只允许硬编码只读子命令,**绝不做子命令透传**。
- **`sessions/` 必须共享**:daemon 靠 `<claude_dir>/sessions/<PID>.json` 判活并拿 pid。A1 已把 `sessions/` 放进共享集 ⇒ 各账号的 pidfile 都落共享库,daemon 一处全看见。**改 A1 的 ISOLATE_SET 时不得把 `sessions/` 挪进去**,否则 cc-monitor 会瞎掉。
- **`tmux_send_keys` 签名(`src-tauri/src/tmux.rs`,A5 建 / A5+ 扩)**:`(origin,target,keys,enter?)`——`enter` 可选、**缺省 true**(旧调用不传即附回车,字节等价)。`enter=false` 省尾回车(优雅退出发 `Escape`)。`target` 恒经 `is_ccm_tmux_name` 白名单(cc-* 会话名,跨语言契约见 INVARIANTS §37)。改前端 tmux 名前缀/字符集必须同步 Rust 白名单。
- **A6 部署面 = 纯前端 + 既有命令(账本最终形态)**:app 内部署/维护**不新增任何 daemon/Rust 命令**——只读状态用既有 `list_remote_accounts`;动凭据/dry-run/verify/login 全经既有 `launch_remote_terminal` 弹终端。命令由纯 `src/settings/acct-deploy.ts` 构建(校验 + POSIX 单引号,与 launch.rs 双引号/控制字符拒收双层防线)。**daemon 只读铁律零妥协 ⇒ A6 不触发发版**(仅随 A2–A5 的 monitor 重编)。

## Features(拆分 + 顺序 + 理由)
- **A0(Phase 0 gate)**:验证 `CLAUDE_CONFIG_DIR` 真隔离 credentials+.claude.json(真机/远端只读探测)。**不过不动工具。**
- **A1 ✅ 已完成(2026-07-23)** 通用隔离 shell 模块 + manifest(M1+M2)。被依赖、先做。init/add/rm/list/verify/sync/rollback + 隔离启动器 + 写 manifest;全程 dry-run+备份;**测试在 mktemp 沙盒**(造假 config、模拟并发、verify、rollback),**绝不碰真 ~/.claude**。归属见开放决策①。
- **A2 daemon 账号能力**(M3 后端,只读三命令)。`--list-accounts`(读 manifest)/ `--session-accounts`(`/proc/<pid>/environ` 探测运行中会话属于哪个号)/ `--account-trust`(换号前的目录信任预检,只回布尔)。additive + BUILD_ID bump + 需重编内嵌+发版。
- **A3 账号模型 + 全局切换 UI**(前端,非破坏性)。`src/accounts.ts` store、状态栏 chip(常驻显示当前默认号 + 一键切)、设置面板「账号」组(占用既有空占位)、Ctrl+K 只读命令、每条会话的账号徽章。
- **A4 按账号启动 / resume**。`remote-launch.ts` 三处 payload 注入 `export CLAUDE_CONFIG_DIR='<dir>'; `(可选末位形参 ⇒ 不带账号时逐字节不变)、新会话对话框/历史行动作表选账号、`history-metadata.json` 记 `lastAccount`。
- **A5 换号重启编排 + compact**。预检 → 确认 → **在旧号上 compact** → 优雅退出(超时降级 kill)→ 用新号 resume → 后处理;含进度 UI 与逐步失败语义。
- **A6 ✅ app 内部署向导**（2026-07-24 完成）。**纯前端零新增 daemon 面**:只读状态用既有 `list_remote_accounts`;dry-run/verify/`--apply`/sync/`/login` **全弹终端窗口让用户亲眼看着跑**(不让 daemon 代跑,守只读铁律 §6 裁定)。命令由纯 `acct-deploy.ts` 构建(校验+单引号)。
- **A7(future,非本轮)**:本地 Windows 账号隔离(注意 `lib.rs:106` 不清 `CLAUDE_CONFIG_DIR`,本地会继承 monitor 自己的值)。

依赖:A0 → A1(已完成)→ A2 → A3 → A4 → A5;A6 依赖 A3。**交互设计基线统一在 `DESIGN-account-switching.md`**(已验证的 15 条底层事实 + 三种"切账号"语义 + 完整编排 + 降级矩阵),各 feature 只写怎么做怎么验。
A2 需要发版才能真用 → **建议 A2–A5 做完一起发一版**。

## 安全 / 验证
- **A1 全程 dry-run**(打印将建的 symlink/文件、不落盘);真动前 `cp -a` 备份共享库关键项 + 存回退步骤;`verify` 自检(两号 CLAUDE_CONFIG_DIR 起 → `cc-whoami`/`.claude.json` 邮箱不同 = 隔离;在一号 `touch` 共享 skills 文件、另一号可见 = 共享);`rollback` 还原。
- **沙盒测**:A1 单测/集成测在 `mktemp -d` 造的假 config 上跑全流程(CI-able、零真机风险)。**用户自跑管线** = 在真 ~/.claude 上 apply(工具引导:先 dry-run → 备份 → apply → verify → 可 rollback)。
- **cc-monitor**:`--list-accounts` 不返凭据;CLAUDE_CONFIG_DIR 注入前校验路径字符集 + posixQuote(防注入,复用 remote-launch 现有防护)。

## 开放决策(呈用户,审批时定)
1. **M1 shell 模块归属**:(a)独立 skill `cc-acct-iso`(像 cc-bus,可 install+自带 doc,**推荐**——账号隔离是 shell 层事、与 cc-monitor 解耦)/(b)cc-monitor 仓 `tools/` 脚本 /(c)放 ~/文档/电脑配置 的独立脚本。
2. **`.claude.json`**:隔离(推荐,oauthAccount 正确+防并发写竞争)/ 共享。
3. **cc-monitor 集成深度(A3)**:先只「resume/新会话下拉选账号 + 注入」(小)/ 还是连「设置里管账号、显示当前、默认跟随、钉死提示」全套(#68 全量,大)。
4. **A0 由谁验**:你真机 / 我经 daemon 远端只读探测(起一个 `CLAUDE_CONFIG_DIR=/tmp/probe claude --version` 级、不登录、看是否建目录)。
5. **迁移策略**:全迁(把现有单 `~/.claude` 变共享库 + 建 z/b 隔离 config)/ 并存(保留现有顺序切换 `cc-acct`,新增隔离 `<name>i` 启动器,渐进)。

## 变更记录
- 2026-07-23 建 masterplan(Phase A)。待用户审批 5 个开放决策 + 整体架构后进 A0/A1。
- 2026-07-23 用户批准架构 + 4 决策(独立 skill / `.claude.json` 隔离 / A3 全套 #68 / 全迁 / A0=本机只读探测)。进 A0。
- 2026-07-23 **A0 gate PASS**:实测 CLAUDE_CONFIG_DIR 重定位 config dir(建 `/tmp/ccprobe/.claude.json`,不读真 `~/.claude.json`)+ 文档证 `.credentials.json` 明文重定位。load-bearing 假设成立。caveat:symlink 共享未文档化,但 `projects/` 并发写风险=单账号多会话现状,不新增。见 STATUS「A0 结果」。写 `features/00-a0-gate.md`。
- 2026-07-23 写 A1 feature 计划(`features/01-a1-shell-module.md`),呈用户审批门禁。
- 2026-07-23 **真机只读核实** + **用户拍板迁移模型 = V2 纯分离**。关键发现:① `.claude.json` 无 env 时住 `$HOME/.claude.json`(不在 config dir 内),设 env 后住 `<cfg>/.claude.json` 且**不回退** ⇒ 旧 V1(默认号 configDir=`~/.claude`)会让默认号出现**两份 `.claude.json`**(裸起 vs 带 env 起),A3 必须加特例分支 → 弃 V1 为逃生口。② 现默认号 = `b`(live 凭据 md5 == `~/.claude/accounts/b.json`,`.last=b`);`z` 凭据**仅存在于快照** ⇒ `add --from-credentials` 必需。③ `~/.claude.json` 的 `oauthAccount` 与 live 凭据不同步(旧 swap 不动它)⇒ 佐证 `.claude.json` 隔离决策。A1 计划已按 V2 改写(配置模型加 `SHARE_EXCLUDE`/`LEGACY_HOME_ITEMS`,命令加 `shellinit`)。进 Phase C。
- 2026-07-23 **A1 完成**(`features/01-a1-shell-module.md` 签收):独立 skill `cc-acct-iso` 落在 `~/.claude/skills/cc-acct-iso/`,11 个子命令,166 条沙盒断言全绿,shellcheck 干净,真机 `~/.claude` 零改动。Phase D 三视角并行审计报出 **6 阻塞 / 11 重要**(rollback 可诱导 `rm -rf` 任意目录、manifest 非原子写、configDir 可注入、rollback 一条失败挡住全部、undo 在操作前登记、断链导致 sync 永不收敛)全部修掉 + 50 条回归断言。**账本变更**:manifest 加 `updatedAt`/`mode` 两字段 + 四条消费方规则(见 §契约);新增 `list --json` 作为 A2 的进程级接口。下一步 A2(daemon `--list-accounts`)需用户批计划。
- 2026-07-23 **A1 之后重新拆分 cc-monitor 侧**:原 A2/A3 两分 → A2(daemon 只读三命令)/A3(账号模型+全局 UI)/A4(注入+按账号 resume)/A5(换号重启+compact)/A6(app 内部署向导)/A7(future 本地)。新增设计基线 `DESIGN-account-switching.md`(15 条已验证底层事实 + 三种"切账号"语义 + 完整编排 + 降级矩阵 + 安全边界)。**账本新增两条**:daemon 只读铁律下的部署切法(只读走 daemon、`--apply` 走终端窗口);`sessions/` 必须留在共享集(daemon 靠它判活拿 pid)。关键实测:`--resume` 不换 sid;`/proc/<pid>/environ` 可探测运行中会话的账号;**会话 jsonl 里没有任何账号字段**(⇒ 历史会话只能靠 cc-monitor 自己记,记不到就显示未知)。写 `features/02-a2-daemon-accounts.md`,呈用户审批。
- 2026-07-24 **A2 完成**:daemon 只读三命令(`--list-accounts`/`--session-accounts`/`--account-trust`)+ BUILD_ID `p1p→p1q-accounts`。3 视角审计修 R1(PID 复用误归属,procStart 对拍)/重要-B(FIFO/设备文件 OOM,read_regular_capped)/R2(export 前缀)+ Unicode 两端对齐。monitor 侧 `src-tauri/src/accounts.rs`(available:false 区分 daemonless/旧)。
- 2026-07-24 **A6 完成 + D/E/F 签收**:app 内部署向导（`features/06`）。**纯前端、零新增 daemon 命令**——只读状态用既有 `list_remote_accounts`，dry-run/verify/--apply/sync/login 全经既有 `launch_remote_terminal` 弹终端（用户看着跑）。纯构建器 `src/settings/acct-deploy.ts`（`validateAcctName` + `buildAcctIsoCmd` 7 步 + POSIX 单引号 + 拒双引号/控制字符/前导-，与 launch.rs 双层防线）；`accounts-section.ts` not-enabled 内联启用向导（名输入+实时校验+命令预览+复制+四步按钮）+ ready 维护区（加账号/verify/sync）+ 每行登录终端按钮。D 两视角**零阻塞**，1 重要 I-1（login=`cc-acct-iso run <名>` 偏离 DoD 初稿→裁定为改进[工具唯一登录入口+注入面更小]，回写 DoD）；hardening S2 复制按钮 + S3 拒前导-。**§6 裁定回写 DESIGN**（dry-run/verify 走终端不走 daemon，守只读铁律零妥协 ⇒ A6 不触发发版）。tsc0/vitest453/build 绿。
- 2026-07-24 **A5+ 优雅退出完成 + D/E/F 签收**:换号重启 ④ 从直接 kill 升级为 DESIGN §5④ 完整形态（`features/07`）。序列 = `Escape`（打断，不带尾回车）→ `/exit`+Enter（文档化干净退出）→ 有界等 M 秒（默认 10s，`awaitExitFor` 轮询 `claudeExited`）→ `kill_remote_tmux`（清场+兜底 SIGKILL，失败仍中止防双进程）。Rust 提纯 `build_send_keys_remote_cmd` + `tmux_send_keys` 加 `enter?`（默认 true 向后兼容，补 R1 命令构造测）。D 两视角**零阻塞零重要**（亲验无 resume-while-alive 窗口）；hardening S1 清挂起轮询 timer。**账本**:`tmux_send_keys` 签名加 `enter?`；INVARIANTS §37 补注；DESIGN §1 V3 标已解。tsc0/vitest453/cargo352/build 绿。
- 2026-07-24 **A3 完成**:account store(accounts.ts)+ 状态栏 chip + 设置账号组 + Ctrl+K 只读命令 + tab 徽章。
- 2026-07-24 **⚠️ 污染事故 + 修复**:发现上一段 session 的"进度日志被终端 `vitest --watch` 污染、伪造 test/cargo 成功输出",导致 STATUS 记了**没落盘**的改动(buildEnvPrefix 调用无定义、SESSION_ACCOUNTS_TTL_MS 未定义 → 整树 tsc 4 error;history.rs/run.ts/sessionBadge/env 测全缺;panel-groups 2 红)。用户「查看进度」时重跑权威 tsc/grep/git 戳穿。**已修复到真绿基线**并建纪律(每步 Read 回盘 + 真跑测试重定向文件核实,绝不信内联绿、绝不 watch)。见 memory `terminal-pollution-fabricated-progress` + STATUS 顶部恢复记录。
- 2026-07-24 **Phase G 整体验收通过 · A0–A5 全族收官 · 交回用户**：1 个聚焦集成审计 agent 独立复核（自跑构建测试 + grep 核实"声称做了"的项，不信文档自述）——**零阻塞零重要**，历史「日志谎报」事故未复现（features/05 声称项逐条为真）。五维度全过：account store 六消费方一致无漂移 / 共享面账本落最终形态（全族仅 1 条计划内 TODO）/ 文档-代码一致 + §7 四分支齐 / daemon 只读边界全族守住 / 无回归。实测 tsc0 / npm test 433 / cargo 351 / daemon 124 / build ✓ / 真机零改动。**loop 停，交回用户**。遗留（均需用户单独决策）：发版(daemon 重编+嵌入+tag)、真机迁移(用户自跑管线)、A6 部署向导 / A7 本地(各需批)、A2-A5 未 commit(待拍板)、若干计划内裁剪/建议(优雅退出 V3/resolveLiveTmux DRY/建议-4 reason code)。
- 2026-07-24 **A5 完成 + D/E/F 签收**:换号破坏性重启 + compact。新 Rust `tmux_send_keys`(headless,`is_ccm_tmux_name` 白名单只发 cc-*)；编排器 `restartWithAccount`(account-restart.ts,①预检→②confirm→③[可选默认关]compact→④kill→⑤带账号 resume→⑥record,§5.2 失败语义 kill 失败中止防双进程)；compact 真检测器(`isCompactRecord` + onLine waiter + `awaitCompactFor`)替盲等；tabs 活 tab 右键两项 + **异步菜单追加收敛 A4 冷缓存债**(删死代码 peekSelectableAccounts)。D 三视角报 1 阻塞(restartTabWithAccount 缺 `live.sid===sid` 守卫→降级远端可 kill 错会话/双进程)已修 + 3 锁定测。架构裁定 **withAccount 与 restartWithAccount 应分离**(语义不兼容)。**账本**:新增 `tmux_send_keys`(tmux.rs);INVARIANTS 补 tmux 名跨语言契约。tsc0/vitest433/cargo351/build 绿。**A2–A5 预批全完 → Phase G**。
- 2026-07-24 **A4 完成 + D/E/F 签收**:`buildEnvPrefix` 注入(与 daemon `is_safe_config_dir` 逐码点对齐,含 C1 控制符)+ 四 runner/三 builder 透传 + Rust `last_account`/`list_last_accounts` + sessionBadge 三源(§3)+ `shouldShowAccountBadge`(§7)+ 选账号 UI 四落点。**E 收敛重构**:抽 `withAccount(origin,name,run,opts)` 统一三站点 resolve+降级+记账(消漂移)——**A5 是其超集**。**账本新增复用点**:accounts.ts 的 `accountConfigDir`/`peekSelectableAccounts`/`recordLastAccount`/`withAccount`/`shouldShowAccountBadge`(均纯/单测)。D 三视角零阻塞;补建 `features/04-a4-inject-and-select.md`。tsc 0 / npm test 417 / cargo 350 / build ✓。**遗留低优先技术债**:tabs 同步 peek vs history 异步 fetch 冷缓存分裂 → A5 顺带收敛。进 A5。
