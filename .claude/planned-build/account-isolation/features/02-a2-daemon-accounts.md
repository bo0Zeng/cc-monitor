# A2 — daemon 账号能力(只读三命令 + BUILD_ID bump)

> 设计基线见 `../DESIGN-account-switching.md`。本 feature **纯只读**、additive,不碰 wire 协议、不碰 CAPABILITIES。
> 被 A3/A4/A5/A6 依赖,先做。需要发版才能真用(见 §发版)。

## DoD(可勾选)
- [x] daemon 新增 `--list-accounts`:输出 1 行 meta + N 行账号 JSON;**绝不输出凭据内容**。
- [x] daemon 新增 `--session-accounts`:遍历 `sessions/<PID>.json` → `/proc/<pid>/environ` → 每条运行中会话属于哪个账号。
- [x] daemon 新增 `--account-trust <configDir> <cwd>`:回**一个布尔**,`configDir` 必须 ∈ manifest 账号(否则拒),**不得回传 `.claude.json` 任何内容**(内含 mcpServers 的 API key)。
- [x] manifest 定位:`~/.cc-acct-iso/config` 的 `ACCTS_DIR=`(**正则解析,绝不 source**)→ 否则 `$HOME/.claude-accts`;支持 cc-monitor 侧覆盖。
- [x] `BUILD_ID` bump + `main.rs` 编年注释追加一行;**不动** `PROTO_VERSION`/`CAPABILITIES`/`EMITS`。
- [x] monitor 侧 `list_remote_accounts(origin)` / `session_accounts(origin)` / `account_trust(...)` 三个 `#[tauri::command]` + `invoke_handler` 注册;沿用 `run_list_query` 的 30s 超时、旧 daemon hello 检测、**`daemonless` 主机过滤**。
- [x] 旧 daemon(无此命令)→ 前端拿到的是"功能不可用",**不是致命错误**。
- [x] daemon `cargo fmt` + `clippy --all-targets` + `cargo test` 全绿;新增单测覆盖:manifest 缺失/畸形、非法 configDir 拒收、凭据字段零泄漏、`/proc` 不可读时降级。
- [x] 文档:`doc/IPC-PROTOCOL.md` 查询表 + `remote-daemon-proto/README.md` 命令列表 + `doc/INVARIANTS.md` 补「daemon 只读账号态」一条。
- [x] **不做**:不写任何文件、不执行 `cc-acct-iso` 的写子命令、不做前端 UI(那是 A3)、不注入 env(那是 A4)。

## 对接主计划 / 共享面
- **读** M2 manifest(§MASTERPLAN 契约 v1)。字段以 `version/updatedAt/sharedStore/acctsDir/accounts[{name,email,configDir,isDefault,mode}]` 为准;`version != 1` → 报"不支持的 schema",不猜。
- **daemon 命令派发**(账本已记):照 `--search`/`--usage` 范式在 `main.rs` 顶层 match 加臂,**不进** `history_query` 兜底桶(语义上不属于 history)。
- 不触 `remote-launch.ts`(A4)、不触 UI(A3)。

## 命令规格

### `--list-accounts`
```
第 1 行(恒有):{"kind":"accounts-meta","enabled":bool,"acctsDir":"<abs>","manifestPath":"<abs>",
              "updatedAt":"<iso|null>","sharedStore":"<abs|null>","count":N,"error":"<string|null>"}
后续 N 行  :{"name":"z","email":"z@x.edu","configDir":"<abs>","isDefault":true,
              "mode":"isolated","exists":true,"loggedIn":true}
```
- `enabled=false` 的情形:manifest 不存在 / 不可读 / schema 不支持 → **exit 0 + 零账号行**(对齐 `usage_query` 的"无 projects 也 Ok"),`error` 里写人话原因。
- `loggedIn` = `<configDir>/.credentials.json` **存在性 stat**(绝不读内容);`exists` = configDir 是目录。
- `email` 优先取 manifest 里的;**不**为了拿实时邮箱去读各号 `.claude.json`(那是 115KB 大文件 × N,且含密钥,得不偿失)。实时邮箱由用户在终端跑 `cc-acct-iso list` 看。
- **configDir 白名单校验**(照 `resolve_query.rs:154 is_shell_safe_base` 的形状):绝对路径、无 `..`、无 shell 元字符/控制字符。不合格 → 丢弃该账号 + `tracing::warn`,不整体失败。
- `mode != "isolated"` 的账号照常输出(带 mode 字段),由前端按契约拒绝使用。

### `--session-accounts`
```
每行:{"pid":12345,"sessionId":"<uuid>","cwd":"<abs>","configDir":"<abs|null>",
      "account":"<name|null>","bare":bool,"alive":bool}
```
- 数据来源:`<claude_dir>/sessions/<PID>.json`(文件名 stem = pid,内容含 sessionId/cwd —— `watcher.rs:514` 已有解析)。
- `configDir` 来自 `/proc/<pid>/environ` 里的 `CLAUDE_CONFIG_DIR`;读不到(进程已死/无权限)→ `null` + `alive:false`。
- `bare=true` = 进程存在但没设 `CLAUDE_CONFIG_DIR`(裸起)。
- `account` = configDir 反查 manifest 得到的名字;查不到 → `null`(**不猜**)。
- 实现:`fn read_proc_environ(pid) -> Option<String>` 照 `watcher.rs:925 read_cmdline` 的形状抄(`fs::read` + NUL 分割),失败一律 `None`。
- 复杂度:会话数 × 一次小文件读,可接受;加一个上限(如 500 个 pidfile)防病态目录。

### `--account-trust <configDir> <cwd>`
```
单行:{"trusted":bool,"known":bool,"error":"<string|null>"}
```
- **安全第一**:`configDir` 必须**逐字等于** manifest 里某个账号的 `configDir`,否则 exit 2 + `{"code":"unknown_config_dir"}`。这避免它变成"任意文件读"原语。
- 读 `<configDir>/.claude.json`(带 32MB 上限,照 `mcp.rs:186`),取 `.projects[<cwd>].hasTrustDialogAccepted`。
- `known=false` = 该 cwd 在这个账号的 `.claude.json` 里没有记录(⇒ 首次用,大概率会弹信任确认)。
- **只输出这三个字段**;任何情况下不回传文件内容。

## manifest 定位(通用,不硬编码本机)
1. cc-monitor 主机配置里的显式 `acctsDir` 覆盖(A3 加设置项;A2 先支持命令行参数 `--accts-dir <p>`)。
2. `$HOME/.cc-acct-iso/config` 里 `^\s*ACCTS_DIR=` 的值 —— **用正则抠 + 去引号,绝不 source**(那是 shell 文件,daemon 不跑 shell)。仅支持 `$HOME`/`~` 前缀展开;更复杂的写法 → 走 1。
3. 兜底 `$HOME/.claude-accts`。
- meta 行里回 `manifestPath`,让前端能在"未启用"时告诉用户到底找的哪。

## 安全 / 只读铁律
- 三个命令**只 `read_dir` / `read` / `stat`**,零写入(`doc/INVARIANTS.md §1`)。
- **不 shell out `cc-acct-iso`**:manifest 直接读文件即可,省掉 PATH 依赖(daemon 是非登录 shell,PATH 很瘦)与只读铁律的争议面(设计文档 §6)。
- 输出里**不含**:token、`.credentials.json` 内容、`.claude.json` 内容、任何环境变量的完整快照(只抠 `CLAUDE_CONFIG_DIR` 一个键的值)。
- 路径参数一律校验后使用;错误走既有约定(exit 2 + stderr,`--account-trust` 用 `--resolve` 那套结构化 `{code,message}`)。

## 测试策略
**daemon 单测**(crate 内 `cargo test`,CI 已有 job):
1. manifest 正常 → meta.enabled=true、账号数/字段正确、`loggedIn` 反映 stat 结果。
2. manifest 不存在 / 空文件 / 非法 JSON / `version:2` → 各自 `enabled=false` + 合适的 `error`,**exit 0**。
3. `configDir` 含 `'`、`$`、`..`、相对路径 → 该账号被丢弃,其余正常。
4. **凭据零泄漏**:构造带 token 的假 `.credentials.json` + 假 `.claude.json`(含 `mcpServers.env.API_KEY`),断言全部输出里不含这些字符串。
5. `--account-trust` 传 manifest 之外的 configDir → exit 2 + `unknown_config_dir`;传合法的 → 只出三个字段。
6. `ACCTS_DIR` 解析:带引号/不带引号/`$HOME` 前缀/注释行/不存在的 config 文件。
7. `--session-accounts`:假 `sessions/<pid>.json` + 不存在的 pid → `alive:false`;pid 存在但无 env → `bare:true`。

**monitor 侧**:`cargo test --lib` 补 `list_remote_accounts` 的解析(含 meta 行)+ 旧 daemon `unknown argument` → 优雅 Err 的断言。

**真机冒烟**(在 aya 上,只读):
- 迁移前跑 → `enabled:false`(manifest 不存在),验证不报错。
- 用 mktemp 造一个假 manifest + 假 configDir,用 `--accts-dir` 指过去 → 验证输出。
- `--session-accounts` 对现有 claude 进程 → 应全部 `bare:true`(迁移前本就没设 env)。**这条正好可以在迁移前后各跑一次做对照。**

## ★待实现时验证(设计文档的 V1)
- **daemon 迁移后是否仍解析到共享库**:F5 推论说会(非登录 shell 不读 rc → `CLAUDE_CONFIG_DIR` 空 → `$HOME/.claude`)。**实现时必须实测确认**:若某些 sshd 配了 `PermitUserEnvironment`/`AcceptEnv` 导致 daemon 拿到了 `CLAUDE_CONFIG_DIR`,它会去看某个账号目录 —— 那里的 `projects/`/`sessions/` 是软链回共享库,**大概率仍然对**,但 inotify 对软链目录的行为要实测。
- 若实测发现有问题:在 daemon 侧显式忽略 `CLAUDE_CONFIG_DIR`、恒用 `$HOME/.claude`?**不能**——那会破坏"用户本来就把 claude 装在别处"的既有用法。届时改为:meta 行回 `claudeDir`,前端发现它不是共享库时告警。

## 逐条实现步骤
1. `remote-daemon-proto/src/accounts_query.rs` 新建:manifest 定位 + 解析 + 三个子命令的 `run()`(签名/退出码照 `usage_query.rs:38`)。
2. `main.rs`:`mod accounts_query;` + 顶层 match 加三臂(或一臂 `--accounts` 带子命令?**不**——照既有风格三个独立 flag 更一致)。
3. `read_proc_environ` 放 `accounts_query.rs`(不污染 watcher);形状抄 `watcher.rs:925`。
4. `BUILD_ID` bump → `p1q-accounts`,`main.rs:57-95` 编年注释追加一行。
5. daemon 单测 7 组。
6. `src-tauri/src/remote_history.rs`:三个 `#[tauri::command]`,内部复用 `run_list_query`;`daemonless` 过滤;旧 daemon 检测免费继承。
7. `src-tauri/src/lib.rs` `invoke_handler` 注册。
8. monitor 侧单测。
9. 文档三处 + 账本/STATUS 更新。
10. 真机只读冒烟(迁移前跑一次,留作对照基线)。

## 发版(**需用户拍板**)
- daemon 改了 → 必须 re-zigbuild 两 arch + 更新 `src-tauri/embedded-daemons/*.build_id` 清单(**只 bump 源码不 re-embed = 半 bump,比不 bump 更糟**)。
- 官方路径:版本号四处对齐 → `git tag vX.Y.Z` → push 触发 `release.yml`(CI 自己 zigbuild + 对拍 build_id)。
- **用户已拍板:A2–A5 做完一起发一版**。在此之前 A3/A4/A5 的 UI 走"功能不可用"降级路径开发,本地手工部署 dev 版 daemon 做真机验证。发版本身仍需用户在那时拍板版本号与 tag。

## 审计结果(Phase D:3 视角并行)
三个 agent 各自实跑复现。**计划符合度判"达标可签收,无阻塞无重要缺口"**;正确性 + 安全各报若干,分诊后修 5 处 + 加 12 条回归断言(daemon 10→15 测,cc-acct-iso 166→171 测)。

### 修掉的(重要级)
| 来源 | 问题 | 修法 |
|---|---|---|
| 正确性 R1 | `--session-accounts` 对 PID 复用零防御 → 陈旧 pidfile 的 PID 被别的进程占用时,把已死会话**误贴成"活着+别人的账号"**(沙盒复现) | 加 `session_process_identity_ok`:pidfile 的 `procStart` 与 `/proc/<pid>` 当前 starttime **精确对拍**,不符/缺失一律判死不归属(严于 watcher——错标签比缺标签更坏)。真机验证未误伤活会话 |
| 安全 重要-B | 特殊文件(FIFO/symlink→`/dev/zero`)绕过 32MB 上限 → 远端 **OOM**(实测 6 秒涨 11GB;`metadata().len()` 对设备报 0 骗过检查) | 新增 `read_regular_capped`:先 `is_file()` 挡设备,再 `take(cap+1)` 限量读,一步消 TOCTOU。三处读(manifest / `.claude.json` / pidfile)全改用它 |
| 正确性 R2 | `parse_accts_dir_from_config` 不认 `export ACCTS_DIR=` → config 被 source 时合法,daemon 漏认 → 账号功能"静默判失效" | 匹配前剥 `export `/`declare -x ` 等前缀;顺带修 `ACCTS_DIR =`(带空格=命令非赋值)不误认 |

### 顺手做的(建议级 + 一致性)
- **manifest 逐账号解析**:单个坏账号(缺 name/configDir)被跳过而非拖垮整份(与 cc-acct-iso 写侧"丢单条"一致)。
- **Unicode 欺骗字符两端对齐**:daemon `is_deceptive_char` + cc-acct-iso `path_shell_safe` 都拒双向覆盖/零宽/异常空白/BOM(UI 同形/反向钓鱼面)。**改动了 A1**(已签收)→ 重跑其全测 171/171 绿。
- 建议里"norm_dir 不归一 //、/./"(false-negative only,符合"查不到=不猜")、"cfg_for 的 Result<Result>"(可读性)记录为可接受,未改。

### 记入信任假设(安全 重要-A,经 monitor 不可达)
`--accts-dir` 自造 manifest 可做任意路径存在性 oracle,但 agent 实测 4 种经 monitor 注入全被 `shell_quote`+绝对路径校验挡下 → **只有能直接跑 daemon argv 的本地用户可用,而那用户本就同 uid、本就能读这些文件,不是提权**。已在本文件记录该信任假设(daemon 的"谁能拼 argv"若将来 ≠"谁能读文件",需把 `--accts-dir` 关进白名单)。

**验证**:daemon `cargo fmt`+`clippy -D warnings`+`cargo test` 124 passed(15 账号测);monitor `cargo test --lib` 348 passed(5 账号测);cc-acct-iso 沙盒 171/171;真机只读冒烟(迁移前 enabled:false 不报错、活会话正确判活+全 bare、trust 四情形+越权拒绝、凭据/密钥零泄漏)。

## 工程审计(Phase E)
- **未破坏三轴正交(§26)**:只 bump `BUILD_ID`(`p1p-tmux-frame`→`p1q-accounts`),`PROTO_VERSION`/`CAPABILITIES`/`EMITS` 未动;账号三命令是查询参数、非流模式 flag,`every_capability_token_is_strippable` 仍绿。
- **只读铁律(§1)延伸已如实记入 INVARIANTS**:读面扩到 `~/.claude-accts/`、`/proc/*/environ`、`<configDir>/.claude.json` 三处,各带硬边界;`--apply` 绝不经 daemon;`sessions/` 必须留共享集(daemon 靠它判活拿 pid)。
- **`run_list_query` 提 `pub(crate)`** 是最小可见性放宽(仅 crate 内 sibling 调用,传定死子命令),三份审计一致判无新滥用面。
- 未引入对 cc-monitor 其它模块的行为改动(git diff 证 watcher.rs 未改)。给 A3/A4/A5 的接口经计划审计判"充分,无缺项,不逼后续打补丁"。

## 签收
- [x] 过代码审计(D,3 视角 + 分诊修复 + 回归) · [x] 过工程审计(E) · [x] 主计划已更新(F) · [x] 测试绿(daemon 124 / monitor 348 / cc-acct-iso 171)
