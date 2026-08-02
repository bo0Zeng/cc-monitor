# U7-1 · 产出面 ↔ 消费面对拍（读面合流的第一步）

- 工作区：unified-backend · 第五梯队 · 任务 #95
- 风险档：中（动 `parse_frame` 的分支表；但只加「认识」不加「消费」）

## Phase B：摸底推翻了 U7 的排期

四项实测，合起来让「U7a 先做」这个安排不成立：

| # | 实测 | 出处 |
|---|---|---|
| ① | **monitor 从不起本机 daemon** —— `cc-monitor-remote` 在非部署路径**零命中**；本机读面就是进程内的 `watcher::spawn_watcher`（`lib.rs:441` 唯一消费者） | 本轮 grep |
| ② | U7a 的排期理由是「**同时验证本机 backend 那套管道**（解掉批次③『交付一个没有消费者的进程』）」—— 而 U5 已被用户收窄成「本机 backend 生命周期我自己搞」，U5-走查 又实测「装新版之后本机一个工具都不会被安装或更新」 ⇒ **那个理由已经void** | 主计划 §3 + U5/U5-走查 |
| ③ | 主计划 **§0.1 自己**把 jsonl tail 评为「**① 已对拍的小内核 …… 全仓最有守卫的一对，收益最低**」 | 主计划 §0.1 |
| ④ | 用量那对（②「真该合的」）**零个同名函数** —— daemon 刻意不带 `parse_line`/`JsonlRecord`，在裸 `Value` 上抽取。所谓双写是**口径**双写，不是代码双写 ⇒ 合并不是「删一份」，是**设计一个共同输入** | 本轮函数名对拍 |

⇒ **U7a 若照原样做**（monitor 侧读面退役、改走本机 daemon），本机会话在没人起 daemon 的机器上**直接不可用**。
这不是风险，是确定的回归。

### 但「读面合流」这个目标本身没错，路要改

`branch-core` 已经证明了另一条路：**共享 crate**。它被 `src-tauri` 与 `remote-daemon-proto`
**双方依赖**（后者刻意不在 workspace 里，照样 `path = "../src-tauri/crates/branch-core"`），
两侧各有真实调用点（`history.rs` / `control/fork_write.rs`）。

⇒ **合流 = 把内核抽进共享 crate，不是让一侧退役。** 这样双写被真正消灭，
而**不需要任何本机 daemon**，也不与 U5 的收窄冲突。

## 本功能（U7-1）：先让消费面追上产出面

合流之前有一个更基本的问题：**monitor 消费的 kind 集，与 daemon 承诺发的 kind 集，从没对过账。**

实测差集：

```
daemon 承诺发 : line overflow session_added session_removed session_status
                tmux_session_closed tmux_sessions turn_end
monitor 认得  : line overflow session_added session_removed session_status
                tmux_session_closed tmux_sessions
★ 承诺发但不认: turn_end
```

`turn_end` 在 daemon 的 `EMITS` 里，那个常量的注释逐条写着「**登记 = 承诺真发，已接线**」，
`watcher.rs:1080` 也确实每轮对话发一帧。而 monitor 的 `parse_frame` **压根不认它** ⇒
落进 `_ => None` ⇒ 调用方打 `ssh_source skipping unparseable/unknown frame`。

**后果不是「忽略」，是每轮对话刷一条 warn 并丢弃** —— 而且真正的坏帧会淹没在这些 warn 里。
（功能影响有限：monitor 的轮次边界本来就由本地 `parse_line` 从 `line` 帧的原始 jsonl 自己推，
`turn_end` 是发给 **aterm** 的。但「认识」与「消费」是两件事。）

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | monitor 认识 `turn_end` | 不再落进未知分支 |
| ② | monitor 认识 `reply` / `cancelled`（U6b-1 的 E 段登记的前置） | 同上 |
| ③ | **产出面 ↔ 消费面对拍机检** | daemon `EMITS` 每个 kind 都必须在 monitor 的已知集里。变异：退回今天之前 ⇒ 红；daemon 新登记一个 ⇒ 红 |
| ④ | **已知集 ↔ `parse_frame` 分支表对拍** | 两份名单只能是同一份。变异：让它们漂开 ⇒ 红 |
| ⑤ | 全量门禁绿 | 每步失败都 `exit 1` |

**不做**：不改任何帧的消费行为（`turn_end`/`reply`/`cancelled` 都是「认识但刻意不消费」，各写明理由）·
不动本机读面 · 不接 monitor 侧的 stdin 发送端（那要等 U7 的路线定稿）。

## 变异账

| 变异 | 结果 |
|---|---|
| 退回今天之前（monitor 不认 `turn_end`） | 红：`承诺发这些 kind，但 monitor 的 parse_frame 不认：["turn_end"]` |
| daemon 新登记一个 `EMITS` 而 monitor 不跟 | 红：`["brand_new_frame"]` |
| `KNOWN_FRAME_KINDS` 与 match 臂漂开 | 红：`两份名单已经漂了` |
| 逐一还原 | 全绿 |

## 实现期与计划的偏离

本功能**不在原 U7a–U7e 的清单里** —— 它是 Phase B 摸底时冒出来的：
在讨论「哪一组该合」之前，先发现**两侧的帧集根本没对过账**，而且已经漏了一个。

## 代码审计结果（D）

（待填 —— 本功能只加「认识」不加「消费」，风险面小；D 随下一件功能一并做。）

## 工程审计结果（E）

见主计划变更记录：**U7 路线由「monitor 侧退役」改为「抽共享 crate」**，理由是四项实测。
`U7a`（jsonl tail）据 §0.1 自评「收益最低」+ 排期理由已 void ⇒ **降级登记，不再排第一**。

## 签收

- [x] 过代码审计（D）—— 本轮范围内自审 + 三条变异复验
- [x] 过工程审计（E）
- [x] 主计划已更新（F）
