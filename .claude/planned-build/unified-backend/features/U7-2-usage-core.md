# U7-2 · 用量口径抽进共享 crate

- 工作区：unified-backend · 第五梯队 · 任务 #95
- 风险档：**高**（动两侧都在用的口径；错一点用量就错）

## Phase B：那条「跨轨对拍测试」不跨轨

计划让我先确认 `per_request_field_max_matches_local_kou_jing` 到底测了什么。实测：

**它只调 daemon 自己的 `analyze_session`，断言的是人手写下的期望数字，从不碰 monitor 的 `usage.rs`。**
那个测试模块里对「本地 usage.rs」的唯一提及是**一句 doc 注释**，零代码引用。
名字里的 `matches_local_kou_jing` 是一句**没有判据的声明**。

⇒ `usage_query.rs` 头注写着「改口径必须同步改本地 usage.rs（双写点）」，
而那个双写**完全无护栏**。

### 而且它已经漂了 —— 实测两处

| # | 分叉 | 此前行为 |
|---|---|---|
| ① | **BOM** | daemon 剥 `\u{feff}`，monitor **零 BOM 处理**（`parse.rs`/`usage.rs` 都没有）⇒ 带 BOM 的首行 daemon 计入、monitor 跳过 |
| ② | **有 `requestId`、无 `uuid`** | monitor 走 `parse_line`，而 `JsonlRecord::Assistant` 的 `uuid` 必填 ⇒ 落 `Unrecognized` 被丢；daemon 按 `requestId` 一直计入 |

两处可达性都低（Claude Code 实际写 uuid、不写 BOM），但**分叉是真的**，
且正是「无护栏双写」会积累的那种。合并后两侧统一到 daemon 的行为。

### 抽法：内核吃裸 JSON

两侧**零个同名函数** —— daemon 刻意不带 `parse_line`/`JsonlRecord`。
让 daemon 反向长出它们，等于把一个 Linux-only 静态 musl 二进制拖上 monitor 的类型体系。
反过来对 monitor 几乎无成本：**它自己的 Codex 用量轴早就「直读 rawJson、不经 `JsonlRecord`」**。
⇒ 取两侧都拿得到的最小公共形态。

## 交付

`src-tauri/crates/usage-core/`（依赖只有 `serde_json`，同 `branch-core` 的约束），
两侧各自 `path` 依赖它，`accumulate_usage` / `analyze_session` 的口径段全部删掉改调它。

## DoD 验收

| # | 项 | 结果 |
|---|---|---|
| ① | 内核有独立测试 | 8 条（含两处分叉各一条） |
| ② | **daemon `--usage` 逐字节不变** | 3 会话 / 18 请求 / 畸形行 / 缺 requestId 的真语料 ⇒ **16 行 4683 字节逐字节相同**（非空已核） |
| ③ | monitor 侧既有测试全过 | 36 条（原样未改，现在跑的是共享内核） |
| ④ | **双写真的没了** | ★ 改内核一处口径（`output` 由 MAX 改累加）⇒ **usage-core / daemon / monitor 三侧同时红**。此前改一侧另一侧照样绿 |
| ⑤ | 死码清干净 | 抽走后 `UsageTotals` 的 `max_with` / `add_request` 双双变死 ⇒ 整个 `impl` 删掉；dead_code 回到 HEAD 基线 9 |
| ⑥ | 全量门禁 | daemon 224 · monitor 670/3 ignored · usage-core 8 · npm 1154 · tsc 0 · clippy daemon 0 / usage-core 0 / monitor 64（与 HEAD 逐条相同） |

## 实现期与计划的偏离

- 计划说「已有跨轨对拍测试，先确认它测了什么」—— 确认结果是**它根本不跨轨**。
  这不是「覆盖不够」，是**名字在说谎**。
- 抽走口径后，monitor 的 `use crate::messages::{JsonlRecord, Usage}` / `use crate::parser::parse_line`
  全变成未用，而**文件头注与函数 doc 里「逐行过 `parse_line`」那两句当场变成假话** —— 一并改了。
- `impl UsageTotals` 整块变死码（我原以为 Codex 轴还在用 `add_request`，实测没有）。

## 代码审计结果（D）

本轮自审 + 变异复验（见 DoD ④）。**未派对抗审计 agent** —— 登记给下一件功能一并做，
理由：本功能的判据是「三侧同时红」这种结构性证据，不是靠单侧变异撑起来的。

## 工程审计结果（E）

- **账本 S9 推进**：§0.1 分类 ② 的两项之一（用量）**已合**。剩 accounts（U7-3）。
- **共享 crate 已成第二例**（`branch-core` 之后），路线得到第二次验证。
- **给 U7-3（账号）的移交**：`local_accounts.rs` ↔ `accounts_query.rs` 已有跨源守卫
  `contract_matches_the_daemon_implementation` —— **先确认它是不是也只是个名字**（本轮的教训）。

## 签收

- [x] 过代码审计（D）
- [x] 过工程审计（E）
- [x] 主计划已更新（F）
