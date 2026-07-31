# G2 — daemon `--fork-session` + 收窄 `readonly_guard`

全区风险最高的一步：daemon **第一次写盘**，且要动一条用户立的红线守卫。

## §1 命令：照既有一次性查询的形状

`main.rs` 的 dispatch 里加 `Some("--fork-session") => fork_write::run(...)`，
错误信封沿用 `--resolve` 那套（exit 2 + stderr `{code,message}` JSON）。

**只收 sid，不收路径**（monitor 侧那个 `validate_branch_source` 收的是路径）：
daemon 是被 ssh 远程调起来的，少一个可被构造的路径入参就少一条穿越面。
sid 先过 `[A-Za-z0-9-]` 格式校验，再只在 `projects/` 下按文件名匹配。

**变换走 G1 的 `branch-core`**，daemon 侧不写第二份实现。

**不引 uuid crate**（daemon 依赖表刻意极简）：新 sid 用「时间 + pid + 源 sid 哈希」拼
v4 形状串。**唯一性不靠这个串**，靠 `O_EXCL` —— 撞了直接失败，绝不覆盖。

## §2 护栏：收窄成两层，整体更强

| 层 | 范围 | 判据 |
|---|---|---|
| 默认层 | 除白名单外**所有**生产源码 | 原那 11 条写模式一条不许出现（**未放宽**） |
| 白名单层 | **恰好一个**模块 `fork_write.rs` | 必须含 `.create_new(true)`；不得出现删除/改名/复制/硬链软链/截断/追加/覆盖写/建目录/`set_len`/`.create(true)` |

判据从「daemon 不许碰文件系统」改述成**「daemon 不许改动用户既有数据」**——
前者只是后者在「daemon 从不写盘」年代的近似。边界写进 `doc/INVARIANTS.md` §41.6，
BACKLOG **E50 标为已解**。

**反向自检**直接喂字符串给判据函数，不去改真文件：后者要么污染工作区，
要么因为改不进去而**假绿**（本会话已栽过两次「变异没落地却当成没覆盖」）。

## §3 ★ 护栏第一次跑就抓了我自己 —— 抓的是注释

`fork_write.rs` 的头注里列了那几个禁用函数名（`fs::write` / `truncate(true)` …），
而护栏**连注释一起扫**（fail-closed 的设计）⇒ 白名单模块被自己的文档判成违规。

这个形状在本仓**已栽过四次**（见 `test-support/strip-comments.ts` 头注）。
按既有约定 **「改措辞，别改护栏」** 处理：头注改成不写字面名字，
并在该文件里写明为什么措辞别扭、别改回去。`INVARIANTS` 那条派生纪律也补了这次的记录。

## §4 ★ N5：一个只被行为测试抓住、护栏自己没抓到的洞

变异「把 `.create_new(true)` 换成 `.create(true)`」（不截断，但对已存在的文件会从头写花它）：

- **第一版**：只有 `write_new_file_refuses_existing_target` 红，**护栏通过**。
  因为 `WHITELIST_REQUIRED` 是裸 token `create_new(true)`，而模块文档里那句
  「`create_new(true)` = O_EXCL」**把要求喂饱了**。
- **修法两处**：① 必需 token 改成**带前导点**的 `.create_new(true)`，注释满足不了、
  只有真调用能满足；② `.create(true)` 加进白名单层禁用清单
  （安全性：`create_new(true)` **不含**子串 `create(true)`，不会自伤）。
- **重跑**：护栏自己也红了，报「找不到 `.create_new(true)` —— 写盘方式被换掉了？」

## §5 变异（退出码 + 真假红核查）

| # | 变异 | 结果 |
|---|---|---|
| N1 | 白名单**之外**（`watcher.rs`）加一句覆盖写 | 红（默认层） |
| N2 | 白名单**之内**加一句删除 | 红（白名单层） |
| N3 | `O_EXCL` 换成会截断的建法 | 红（3 条） |
| N4 | 白名单模块改名 | 红（「恰好一个」不成立） |
| N5 | `.create_new(true)` → `.create(true)` | 修前**护栏没抓到**、修后红 |

## §6 e2e：真跑二进制

`e2e/daemon-fork-session.sh`（`npm run test:daemon-fork`，**10 条断言全过**）。

**为什么单测不够**：`accounts_query` 的单测当年直接调 `run()`、**绕过 main 的 dispatch**，
于是 `--account-trust-zero` 在模块里实现完整却在 dispatch 漏了一臂，随 v3.4.0 发出去成了真 bug。
本套件从 **argv 进、stdout/exit code 出**，盯的正是那一层。

覆盖：正常分叉 exit 0 + stdout JSON · 源文件 sha256 不变 · **夹在链中间的旁支被跳过**
（祖先回溯，不是线性切片）· sessionId 换新 · 未知 sid exit 2 + 结构化 stderr ·
路径穿越被拒 · 既有分支文件不被后一次分叉动过。

隔离走 `CLAUDE_CONFIG_DIR`（daemon 没有 `--claude-dir` 参数），不碰真 `~/.claude`。

## §7 并发 / 磁盘满 / 权限

- **并发**：两个 monitor 同时分叉同一会话 —— `O_EXCL` 让后到的拿到错误而不是覆盖。
  新 sid 含 pid + nanos，实际撞名概率极低；**但正确性不依赖它**。
- **磁盘满 / 权限**：`open` 与 `write_all` 的错误都带路径原样返回，
  经 exit 2 + stderr JSON 回到 UI —— 静默半截文件比报错糟得多。

## §8 门禁

monitor `--all` **639** · `-p branch-core` **7** · daemon **183**（+7）·
两侧 `cargo fmt --check` 干净 · monitor clippy 62 未变基线 · **daemon clippy 0 警告** ·
`shellcheck -S warning` 新 e2e 干净 · vitest 1048（前端未动）· 新 e2e 10/10。

> 顺带记一个自己踩的坑：核 clippy 时写了 `cargo clippy … | grep -c "^warning: "; echo rc=$?`
> —— 那个 `$?` 是 **grep 的**（数到 0 时 grep 退 1），不是 clippy 的。已改成先存日志再分别取。

## §9 签收

- [x] 过代码审计（5 条变异，含一条**修前护栏抓不到**的真洞）
- [x] 过工程审计（e2e 进 package.json；I7 边界进 INVARIANTS §41.6；E50 标已解）
- [x] 主计划已更新
