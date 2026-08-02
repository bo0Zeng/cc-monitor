# U6b-2 · argv 三分重建 + 能力协商扩到入方向

- 工作区：unified-backend · 任务 #104 后继（U6b 第二件）
- 风险档：**高**（动的是「进流模式还是进查询模式」这个总开关，判错一个 token = 一条子命令静默失效）
- 前置：U6b-1 的入方向信封已定并提交（`8a13ba9`）

## Phase B 摸底：实测

### ① 今天是二分，不是三分

`main.rs::split_stream_flags` 剥掉 `--with-bg` / `--tail-only`，**剩下非空即一次性查询**。
第三类「子命令自己的选项」（`--accts-dir` / `--scope` / `--limit` / `--after-ms` / `--include-tools`）
从没进过任何表 —— 它们只是被透传给子命令自己解析。

### ② §26 的危险形状**实测复现**

```
$ cc-monitor-remote --some-future-flag
cc-monitor-remote query error: unknown argument: --some-future-flag
rc=2
```

**未知 flag 在流位置 ⇒ exit 2、一个字节都不输出、没有 hello。**
monitor 那头看到的和「daemon 崩了」无法区分 ⇒ 重连 ⇒ 发同一个 flag ⇒ **死循环**。
这正是 2026-07-09 事故的形状。

现有的 `every_capability_token_is_strippable` 挡不住它：那条只覆盖**与已声明能力绑定**的 flag，
而「monitor 因为别的原因发了一个新 flag」不在它的判据里。

### ③ 判据一旦改成「按 token 分类」，就多出一个**更危险**的失效

如果改成「args[0] ∈ 子命令表 ⇒ 查询模式，否则 ⇒ 流模式并忽略未知 flag」，
那么**漏登记一个子命令**的后果不再是 exit 2（吵但看得见），而是
**那条子命令静默变成「起了个流」** —— 调用方拿到一堆 jsonl 行而不是查询结果。

这就是 v3.4.0 `--account-trust-zero` 漏登记那次事故的**加强版**。
⇒ **分类表必须有完备性机检**，且这条机检是本功能的核心交付，不是附属品。

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | **三分表**：每个 token 恰好属于「流 flag / 子命令 / 子命令选项」之一 | 一张表 + 分类函数 |
| ② | **完备性机检**（核心）：4 个分派文件里出现的每个 `--token` 都必须在表里，且只在一类里 | 变异：从表里删一个 ⇒ 红；把一个 token 同时放进两类 ⇒ 红 |
| ③ | **未知 flag 不再踢出流模式** | `--some-future-flag` ⇒ 照常发 hello 进流，stderr 一行 warn。变异：改回旧行为 ⇒ 红 |
| ④ | 已有行为逐条不变 | 14 个子命令的 stdout/stderr/退出码与 HEAD 逐字相同；无参数流模式字节相同 |
| ⑤ | **能力协商扩到入方向**：hello 声明**接受哪些命令** | `commands: Vec<String>`，`skip_serializing_if = Vec::is_empty` ⇒ 旧 monitor 字节不变 |
| ⑥ | §26 护栏同步扩 | `every_capability_token_is_strippable` 仍绿；新增「每个流 flag 都必须被剥离」 |
| ⑦ | 文档 + 全量门禁 | U6a 的护栏会自动逼 `commands` 进文档 |

**不做**：不改任何子命令的行为 · 不搬 `--resolve`（U6b-3）· 不动入方向信封。

## 逐条实现步骤

1. `main.rs` 建三分表（`STREAM_FLAGS` / `SUBCOMMANDS` / `SUBCOMMAND_OPTIONS`）。
2. 分类函数 + 模式判定改成「args[0] ∈ SUBCOMMANDS ⇒ 查询」。
3. 完备性机检：复用 U6a `protocol_doc_guard::dispatched_subcommands()` 的抽取
   （它已覆盖 4 个分派文件），断言抽出的 token 集 ⊆ 三分表的并集，且三类**两两不交**。
4. 未知 flag → warn + 忽略；加测试。
5. Hello 加 `commands`；`inbound.rs` 的命令表变成**单一真相源**，Hello 从它取值。
   *验证*：变异 —— 给 `inbound` 加一条命令但不更新 Hello ⇒ 红（**必须是同一份数据，不是两份**）。
6. 14 子命令逐字对拍 + 全量门禁。

## 测试策略

- 对拍脚本里「非空 / 条数 ≥ 下限」必须是**会让整条命令失败**的硬前置（本会话三次栽在 `&&` 链上的 echo）。
- 变异看退出码**且看失败信息**；`cp -a` 还原后 `touch`。

## 实现期与计划的偏离

### 计划把「三分」当成给现有分类加一档，实际是**重建判据**

原来是「非空即查询」，现在是「`args[0]` ∈ 子命令表才查询」。这不是同一个判断多了一个分支，
是**把判断的主语换了** —— 从「还剩没剩参数」换成「第一个参数是什么」。

### 因此多出一个计划里没有的失效模式，且比旧行为更糟

旧判据下，漏登记一条子命令 ⇒ 它落进 `unknown argument` + exit 2（吵，但看得见）。
新判据下 ⇒ `args[0]` 认不出来 ⇒ **当成流模式** ⇒ 那条子命令**静默变成「起了个流」**，
调用方拿到一堆 jsonl 行而不是查询结果。这是 v3.4.0 `--account-trust-zero` 漏登记那次事故的加强版。

⇒ 完备性机检从「附属品」升格成**这条判据能成立的唯一理由**，写进了代码注释。

### 判据比计划再细一档：未知 flag 与未知裸参数分开

计划只写「未知 flag 忽略」。实现时发现不能一刀切：
- 未知 `--flag`：可能是**新版 monitor 发给旧版 daemon** 的 ⇒ 忽略 + warn，照常发 hello，让对面能降级。
- 未知**裸参数**：任何未来协议都不会把裸参数放 `args[0]` ⇒ 那是明确的调用错误，**仍旧 exit 2**。

后来 clippy 逼出第三种：`args[0]` 是个**子命令选项**（如 `--scope`，脱离了它的子命令）——
也归调用错误。这条本来是为了消掉 `SUBCOMMAND_OPTIONS` 的 dead_code 警告，
但**把常量用起来比 `#[allow(dead_code)]` 更对**：它本来就该参与判据。

### 与 U6b-1 合并提交

U6b-1 的 D 审计与 U6b-2 的实现在同一轮完成，两者都动 `main.rs` 的 argv 段与 `protocol_doc_guard`，
拆成两个 commit 会让中间态编不过。合并进 `217835c`，本文件记录 U6b-2 那一半。

## 代码审计结果（D）

与 U6b-1 同一轮、同一份审计报告（见 `U6b1-inbound-skeleton.md` 的 D 段）。
与本功能直接相关的两条：

- **字符集 `[a-z-]` 静默丢弃**含数字/下划线/大写的 token —— 三分表的完备性机检复用
  `dispatched_subcommands()` 的抽取，抽取瞎了完备性就是空的。已放宽到 `[A-Za-z0-9_-]`。
- **分派文件探测按语法找**，裸 const 藏在未登记文件里一击即破。已放宽到「生产段出现 `"--` 就得登记」。

## 工程审计结果（E）

账本 **S7（daemon argv 面）交付完毕**。账本原文写「三类；`split_stream_flags` +
`every_capability_token_is_strippable` 同步扩」—— 实现证明那两条**不够**：
它们只覆盖与已声明能力绑定的 flag，而危险来自**未声明的**。

对后续控制面功能（U8–U11）是净利好：每加一条子命令都会被
`every_dispatched_token_is_classified` 强制登记进三分表，**纪律自动生效**。

## 签收

- [x] 过代码审计（D）
- [x] 过工程审计（E）
- [x] 主计划已更新（F）
