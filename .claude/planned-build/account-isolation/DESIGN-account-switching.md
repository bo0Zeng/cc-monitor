# DESIGN — cc-monitor 按会话切账号:底层细节 + 交互规格

> A2–A6 的共同设计基线。所有「为什么这么设计」都在这;各 feature 计划只写「怎么做 + 怎么验」。
> 事实来源:2026-07-23 三份代码勘察(启动链路 / daemon 协议 / UI 落点)+ 真机实测。

---

## 0. 一句话

**账号 = 一个 `CLAUDE_CONFIG_DIR`。切账号 = 换启动时的环境变量。所以「切」只能发生在启动那一刻 —— 要么起新会话,要么重启已有会话。**

---

## 1. 已验证的底层事实(全部实测/读码确认,不是推测)

| # | 事实 | 证据 | 对设计的影响 |
|---|---|---|---|
| F1 | `claude --resume <sid>` **不换 sid**,续写同一条 jsonl | 真机:当前进程 `claude --resume 9d66c46d…` 一直写 `9d66c46d….jsonl`;代码注释 `tabs.ts:1701`、`remote-launch.ts:126` | 换号重启后会话**还是同一条 tab**,徽章原地更新。不用处理"重启后变成新会话" |
| F2 | sid 变化只来自 `/branch` fork,与 resume 无关 | `doc/INVARIANTS.md:506`「后端身份 ≠ 会话身份」;`@ccm_sid` 由 `shared/ccm-wrapper.sh:24` 每秒刷 | 换号编排**不得**假设 tmux 名↔sid 恒定,一律现查 `@ccm_sid` |
| F3 | `/proc/<pid>/environ` 同 uid 可读 | 真机 3 个 claude 进程实读成功(当前全是"未设"=裸起) | **运行中会话的账号可可靠探测**,连用户手动起的也认得出 |
| F4 | 会话 jsonl **没有任何账号字段** | 翻遍字段(`sessionId/cwd/gitBranch/version/userType/…`),全文搜 `account|email|organization|oauth` 零命中 | **历史会话无法从磁盘归属账号**。只能靠 cc-monitor 自己记;记不到就诚实显示"未知",严禁猜 |
| F5 | daemon 非常驻,每次 SSH exec 拉一个;环境是非登录 shell 最小集 | `main.rs:226`;russh 不发 env 请求;sshd `$SHELL -c` 不读 rc | daemon 的 `CLAUDE_CONFIG_DIR` 必空 → `resolve_claude_dir()` = `$HOME/.claude` = **共享库**。迁移后它仍能看见所有账号的会话(因 `projects/`+`sessions/` 都软链回共享库) |
| F6 | daemon 与用户 claude 进程、与 `~/.claude-accts`(700) **同 uid** | SSH 登录用户即 `cfg.user` | daemon 读 manifest / stat 凭据存在性 / 读 `/proc/*/environ` 全部可行 |
| F7 | daemon 只有一处 `sh -c`(`tmux ls`),命令是**纯定值零拼接**;daemon 内**没有**任何转义/白名单基础设施 | `watcher.rs:78` | 要跑外部工具得新建白名单;能不跑就不跑(见 §6 决策) |
| F8 | 远端起会话走 **SSH + 弹终端窗口**(`launch_remote_terminal`),不经 daemon;Rust 侧对远端命令**含双引号即拒绝** | `launch.rs:82/91/215` | env 前缀必须 `posixQuote` 且不含 `"` —— A1 的 `path_shell_safe` 已在源头挡掉 `"` 和 `'`,两端对上 |
| F9 | `CLAUDE_CONFIG_DIR` 被**刻意排除**在 `CLAUDE_NESTED_ENV_VARS` 的 unset 名单外 | `remote-launch.ts:12/32` 注释「必须保留」 | 注入前缀与既有 unset 不冲突 |
| F10 | 本地(Windows)启动**不清** `CLAUDE_CONFIG_DIR`,会继承 cc-monitor 自己进程的值 | `lib.rs:106` + `doc/DEVELOPMENT.md:207` | 本地切号(A7)必须显式覆盖,否则静默串号 |
| F11 | 命令栏(Ctrl+K)**刻意只放只读命令**,写动作被排除 | `command-bar.ts` 头注释 + `main.ts:428` | 账号写动作进命令栏需正面处理 danger + 二次确认 |
| F12 | `TabMenuItem` **不支持子菜单**;项目**无自定义确认弹窗**(一律 `window.confirm`);**无通用进度组件** | `tabs.ts:2217/1818` | 账号多时右键菜单会膨胀;多步编排的进度反馈要自己写 |
| F13 | 设置面板有一个**空的「远端」占位组** | `settings/panel.ts:403-415` | 账号面板直接占它,不用新造结构 |
| F14 | 历史行动作是**纯数据表** `HISTORY_ACTION_DEFS` | `views/history-actions.ts:49` | 加「用某账号 resume」= 加一条数据,不是加代码路径 |
| F15 | 旧 daemon 遇到未知命令 → `unknown argument` + exit 2 | `history_query.rs:53` | 前端必须按「该功能不可用」优雅降级,不能当致命错误 |

**待实现时验证(不确定项,已标记)**
- V1 daemon 对**软链目录**(`<acct>/projects` → 共享库)的 inotify 行为;以及迁移后 daemon 是否仍解析到共享库(F5 推论)。
- V2 ✅ 已解(A5)：`/compact` 完成靠 `isCompactRecord`（`src/cards/`）从活 tab 的 daemon 流里检测。
- V3 ✅ 已解(A5+)：优雅退出序列 = **`Escape`（打断当前回合）→ `/exit`+Enter（文档化的干净退出）**，
  等 M 秒（默认 10s）；「已退出」信号 = 轮询 tmux 该会话前台不再是 claude（`claudeExited`/`awaitExitFor`）。
  依据：claude-code-guide 查官方文档（`/exit` 是文档化退出；Esc 打断运行中回合；会话 jsonl 持续落盘故退出快）。

---

## 2. 概念模型:三种"切账号",UI 必须让用户分清

| | 语义 | 生效时机 | 破坏性 |
|---|---|---|---|
| **① 默认账号**(全局) | 以后**新起**的会话用谁 | 立即,但只影响未来 | 无 |
| **② 会话的账号**(per-session) | 这条会话**下次启动**用谁 | 下次 resume 时 | 无 |
| **③ 换号重启**(per-session) | 让**正在跑**的会话改用别的号 | 立刻,但要杀进程重起 | **有**(中断当前回合) |

**凭据在进程启动时读入,运行期不再读** ⇒ ③ 无法"热切"。UI 文案一律用「用 X **重启**」而不是「切换到 X」,避免用户以为点一下就换了。

---

## 3. 账号身份的三个真相源(优先级明确)

某条会话"属于哪个账号"要分场景取:

1. **正在跑** → daemon 读 `sessions/<PID>.json` 拿 pid → `/proc/<pid>/environ` 抠 `CLAUDE_CONFIG_DIR` → 映射回 manifest。**这是唯一的硬真相**(F3),连不是 cc-monitor 起的会话也认得出。environ 是 exec 快照,正好符合"运行期不换号"。
2. **已归档、但 cc-monitor 起过** → `history-metadata.json` 的 `EntryMetadata.lastAccount`(cc-monitor 自己写的记忆)。
3. **其余** → **未知**,UI 显示 `—`。**不猜、不用默认账号顶替**(F4:磁盘上真的没有这个信息)。

裸起(无 `CLAUDE_CONFIG_DIR`)的会话:迁移后等价于"没有凭据",实际不会发生;若探测到,显示「裸起(未指定账号)」并提示可能是 rc 没配好。

---

## 4. 交互规格

### 4.1 常驻可见:状态栏 chip
位置:`#status-bar` 最右,紧邻 ⌨命令 chip(照抄 `main.ts:469-505` 的 `.status-cmdk` 范式)。

- 常态显示 `👤 b`(当前**默认**账号名);未启用多账号时显示 `👤 未启用`。
- 点击 → 浮层选单(复用 `main.ts:718` 的 Esc/外部 pointerdown 关闭范式):
  ```
  ┌─────────────────────────────────┐
  │ ● b   b@example.com      已登录 │  ← 当前默认,打勾
  │ ○ z   z@northeastern.edu 已登录 │
  │ ○ w   (未登录)           ⚠      │  ← 灰掉,tooltip:先在终端 /login
  ├─────────────────────────────────┤
  │ 管理账号…                        │  → 打开设置的账号面板
  │ 刷新                             │
  └─────────────────────────────────┘
  ```
- 选中某项 = **只改默认账号**(非破坏性)。toast:「新会话将使用 z;已有会话不受影响,需要换号请在会话上选『用 z 重启』」。

### 4.2 每条会话的账号徽章
- tab 行(`createTabButton`,`tabs.ts:1986`)加一个小徽章,内容 = 账号名首字(或 2 字符),hover 显示 `账号 z · z@northeastern.edu · 来源:实时探测`。
- 来源按 §3 优先级;未知显示 `—` 且 hover 说明"该会话不是本工具启动的,无法判定账号"。
- **本地会话(`origin === null`)不显示徽章**(A7 之前不支持)。
- `GridSessionSnapshot`(`session-status.ts:44`)同步加 `account` 字段,让命令栏/监控板也能看见。

### 4.3 per-session 动作(右键菜单 + 历史行)
挂在 `tabs.ts:2069` 的右键菜单与 `HISTORY_ACTION_DEFS`(F14):

- **归档会话** → 「用账号 X resume」(每个可用账号一条;账号 >3 个时收进二级菜单 —— 需先扩 `TabMenuItem` 支持子菜单,见 F12)
- **活跃会话** → 「用账号 X 重启…」(`danger: true`,走 §5 编排)
- 两者都在标签里标出当前账号:`用账号 z resume(当前:b)`

### 4.4 命令式交互(Ctrl+K)
- `账号:切默认为 X` —— 只读性质,直接可进命令栏。
- `账号:用 X 重启当前会话` —— **写动作**,按 F11 的约束必须 `danger` + 走同一个 `window.confirm`。
- `账号:管理…` → 打开设置账号面板。
- 快捷键(可选):给 `account.switch-default` 注册一个动作(`keybindings/actions.ts`),默认键留空由用户自配。

### 4.5 设置面板「账号」组(占用 F13 的空占位)
- 账号表:名 / 邮箱 / mode / 登录态 / configDir / 是否默认。
- 操作:设为默认、刷新、**打开该账号的终端**(用于 `/login`)、复制 configDir。
- 顶部状态:「已启用多账号(manifest: …/accounts.json,3 个账号)」或「未启用 → 部署引导」。
- 部署引导(A6)入口在这里。

---

## 5. 换号重启的完整编排(A5 核心)

```
用户点「用账号 X 重启此会话」
 │
 ├─① 预检(任一失败 → 明确可操作的错误,不继续)
 │    · X 存在 / mode=isolated / loggedIn=true
 │    · 该 origin 非 daemonless、daemon BUILD_ID 够新(否则"功能不可用")
 │    · 会话是远端(本地 → 不支持)
 │    · 目标账号对该 cwd 已 trust?(daemon --account-trust)→ 否则只**警告**,不阻断
 │    · 现查 tmux(@ccm_sid,不读缓存)→ 命中=重启路径;未命中=直接 resume 路径
 │
 ├─② 确认(window.confirm,文案写清后果与耗时)
 │
 ├─③ [可选,**默认关**(用户 2026-07-23 拍板);勾上才做] 在**旧账号**上 /compact
 │    · send-keys "/compact" Enter
 │    · 等完成:轮询该 sid 的 jsonl 是否出现 compact 记录(V2 待验;复用 src/cards/ 的 compact 判定)
 │    · 超时(默认 5 min)→ 放弃 compact,继续后续步骤并 toast 说明
 │
 ├─④ 结束旧进程【A5+ 优雅退出已落地】
 │    · 优雅:send-keys `Escape`(打断当前回合,不带尾回车)→ `/exit`+Enter(文档化的干净退出),
 │            有界等 M 秒(默认 10s;awaitExit 轮询该 tmux 前台不再是 claude 即提前结束)
 │    · 兜底:kill_remote_tmux(既有能力)。**kill 必跑**:会话是交互 shell,CC 退出后 shell 仍占会话名,
 │            否则 ⑤ 的 new-session 会短路;超时未退出则 kill 即 SIGKILL 兜底。kill 失败=中止(§5.2 ④)
 │
 ├─⑤ 用新账号起
 │    · buildResumeTmuxCmd(sid, cwd, launcher, name, configDir=X.configDir)
 │    · → launch_remote_terminal(会弹一个终端窗口 —— 这是既有行为)
 │
 └─⑥ 后处理
      · 写 lastAccount=X 进 history-metadata
      · 刷新徽章(等 daemon 下一轮探测覆盖)
      · toast:「已用 X 重启;若 CC 询问是否信任该目录,请在弹出的终端里确认」
```

**归档会话(未在跑)**:跳过 ③④,直接 ⑤。若上下文很大,可选「resume 后立即 compact」——但要**如实告知**这是较贵的顺序(见 §5.1)。

> **默认关的实现形态**(A5 定):项目无自定义弹窗、`window.confirm` 放不下勾选框 —— 二选一:
> (a) 菜单出两条(「用 X 重启」/「用 X 重启(先压缩上下文)」);
> (b) A5 本来就要为多步编排写进度 UI,顺带做成小 modal 承载勾选框 + 进度。
> 倾向 (b)(一个组件解决两个缺口),但若 A5 实现时发现 modal 成本过高就退回 (a)。

### 5.1 为什么 compact 必须在**旧账号**上做(勾上时)
prompt cache 按账号/组织绑定,**换号必然全部失效**。compact 本身是一次 LLM 调用:

- **先 compact 再换号**(推荐,默认):压缩这次调用跑在旧号上 → 命中旧号缓存,便宜;换过去之后新号只需读**压缩后的短上下文**。
- **先换号再 compact**:新号第一件事就是全量读一遍**未压缩**的长历史(零缓存)→ 最贵。

这个顺序要写进按钮 tooltip,别让用户按错。

### 5.2 每一步的失败语义
| 步 | 失败 | 处理 |
|---|---|---|
| ① 预检 | 任一不满足 | 不动手,给可操作提示(如"先在终端用 z 登录") |
| ③ compact | 超时/报错 | **不阻断**,继续 ④(compact 是优化不是必需) |
| ④ 优雅退出 | 超时 | 降级 kill,toast 说明"当前回合已中断" |
| ④ kill | 失败 | **中止**,不继续 ⑤(否则两个进程抢同一会话) |
| ⑤ 起会话 | 失败 | 既有回退:命令复制到剪贴板 + toast(`remote-launch-run.ts` 已有) |

---

## 6. 「在 cc-monitor 内做部署」的边界(A6)

**硬约束**:`doc/INVARIANTS.md §1` 只读铁律 —— daemon 绝不写 `~/.claude/`;唯一写例外是 cc-monitor 自己的 bin 目录。而 `cc-acct-iso --apply` 要搬凭据。

**切法（A6 已落地——裁定见下）**:
- **只读状态走 daemon（且仅此）**:`--list-accounts`（前端 `list_remote_accounts`/`fetchAccounts`）。A6 只用它探测「有没有 manifest / 哪些账号」,不新增任何 daemon 命令。
- **其余一切（dry-run 预览 / verify / --apply / sync / /login）都走终端窗口**:cc-monitor 拼好命令 → `launch_remote_terminal` 弹真实终端 → 用户**亲眼看着计划、亲手确认**;`/login` 也走这里(必须 TTY)。dry-run(`cc-acct-iso init <名>` 不带 `--apply`)已被 A1 测试断言为零落盘,放终端里跑最自然——输出正应展示在用户随后确认 `--apply` 的**同一 TTY**。
- **daemon 绝不跑 `cc-acct-iso` 的任何子命令**(F7:daemon 无白名单/转义基础设施,`绝不做子命令透传`)。A2 的 `--list-accounts` 直接读 manifest 文件 + stat 凭据存在性,不 shell out。

> **§6 裁定（A6 Phase B，Invariant 4 记账）**:本节初稿曾把「dry-run 计划预览 / verify --no-probe 健康检查」也划给 daemon,但那与本节第三条「daemon 绝不子命令透传」冲突(F7)。**裁定:dry-run 与 verify 都在弹出的终端里跑,daemon 只保留 `--list-accounts` 一条只读面。** 理由:① 守只读铁律零妥协、daemon 零新增面(A6 无需 daemon 重编/发版);② dry-run 输出应与用户随后确认 --apply 在同一 TTY;③ 避免为省一次 shell out 而给 daemon 造白名单基础设施。

---

## 7. 降级矩阵

| 情况 | 表现 |
|---|---|
| 没迁移(无 manifest) | chip 显示「未启用」,账号动作全部隐藏;设置面板显示部署引导 |
| daemon 太旧(F15) | 账号功能整体不可用 + 「去设置更新 daemon」引导;**不弹错误** |
| 主机 `daemonless: true` | 该主机的会话不显示账号徽章;切号动作禁用 + tooltip |
| 账号未登录 | 选单里灰掉 + ⚠;点了给「打开该账号终端去 /login」 |
| 账号 `mode: "in-place"` | 拒绝使用(契约已定),列表里标注原因 |
| 本地会话 | 无徽章、无切号动作(A7 之前) |
| 目标 cwd 未被目标账号 trust | 仅警告,提示"终端里可能要确认信任" |

---

## 8. 数据流与存储

```
远端 ~/.claude-accts/accounts.json (A1 写,原子)
        │  daemon --list-accounts (只读,免锁)
        ▼
  Rust list_remote_accounts (fan-out,过滤 daemonless,30s 超时)
        │
        ▼
  src/accounts.ts (前端 store:TTL 缓存 + 手动刷新)
        ├──▶ 状态栏 chip / 设置面板 / 命令栏
        └──▶ 注入:remote-launch.ts buildEnvPrefix(configDir)

远端 sessions/<PID>.json + /proc/<pid>/environ
        │  daemon --session-accounts (只读)
        ▼
  每条会话的 live 账号 → tab 徽章 / GridSessionSnapshot.account

本地 history-metadata.json EntryMetadata.lastAccount
        └──▶ 归档会话的账号记忆(cc-monitor 起过的才有)

config.json accounts.defaultName  ← 全局默认账号(照 remote.hosts[] 范式)
```

**存储归属**:账号列表本身**不落 config.json 做真相**(远端 manifest 才是真相),只缓存;`defaultName` 落 config.json;per-sid 的 `lastAccount` 落 `history-metadata.json`(那里本来就是按 sid 的 truth 层)。

---

## 9. 安全边界(继承 A1 契约)

- manifest 里的 `configDir` 是**不可信字符串**:注入前过 `posixQuote` + 字符白名单,含 `"` 一律拒(F8)。
- daemon **绝不输出凭据**;`--account-trust` 只回一个布尔,**不得回传 `.claude.json` 内容**(那里面有 `mcpServers` 的 API key)。
- 所有写动作(重启/kill/部署 apply)必须二次确认;命令栏里的写命令按 F11 约束标 danger。
- 部署的 `--apply` 只在用户可见的终端里跑,cc-monitor 不代跑。

---

## 10. Feature 拆分(取代旧的 A2/A3 两分)

| ID | 名称 | 依赖 | 交付价值 |
|---|---|---|---|
| **A2** | daemon 账号能力(`--list-accounts` / `--session-accounts` / `--account-trust`),只读 + BUILD_ID bump | A1 | 后端能看见账号 |
| **A3** | 账号模型 + 全局切换 UI(`accounts.ts`、状态栏 chip、设置面板、命令栏只读命令、徽章) | A2 | 看得见 + 能切默认 |
| **A4** | 按账号启动/resume(`buildEnvPrefix` 注入 + 新会话/历史行选账号 + lastAccount 记忆) | A3 | 真能用指定账号起会话 |
| **A5** | 换号重启编排 + compact(§5 全流程 + 进度 UI) | A4 | 一键换号重启 |
| **A6** | app 内部署向导(只读探测 + 生成命令 + 弹终端跑 apply) | A3 | 不用记命令 |
| A7 | (future)本地 Windows 切号 | — | — |

A2 需要**发版**(daemon 重编+内嵌)才能真用 → 建议 A2–A5 做完一起发一版,避免连发两次。
