# G0 — 把「我们的落盘格式 == 官方 `/branch`」钉成测试

**不改任何落盘行为。** 主计划 §0.2 已用真机语料证明格式一致；本功能是把那个结论
从**散文**变成**机检**，防的是下一次有人（包括我）拿 SDK 当规范改回去。

## §0 现有测试盖到哪、漏了什么

`history.rs` 已有 9 条分支测试，盖住：祖先链顺序、`sessionId` 改写、`forkedFrom` 形状、
schema 外字段（`gitBranch`）不丢、根 `parentUuid` 置 null、未知 uuid 报错、路径守卫、
源文件零改动。

**漏的恰好是「与 SDK 分歧」的那几条**：

| 要钉的性质 | 现状 | 「改成 SDK 那套」的变异能不能被逮住 |
|---|---|---|
| 祖先回溯 ≠ 线性切片 | ⚠ **测不出来** | **逮不住**（见 §1） |
| uuid 原样保留 | 间接（断言了 uuid 列表） | 能 |
| timestamp 不改 | ✗ 没测 | **逮不住** |
| `slug` / `sourceToolAssistantUUID` 保留 | ✗ 夹具里根本没这些字段 | **逮不住** |
| 无 uuid 的旁挂记录不带过来 | ✗ 夹具里没有这类记录 | **逮不住** |
| `logicalParentUuid` 指链外时不报错 | ✗ 没测 | 逮不住 |

## §1 关键：**现有夹具区分不了两种算法**

`sample_session()` 里那条被 ESC 回退的兄弟 `u6` 排在**文件末尾**（索引 5），
而分叉点 `u4` 在索引 3。⇒ 从 `u4` 分叉时，**线性切片 `[0..=3]` 与祖先回溯给出同一个答案**。

真机里不是这样：原生 fork 的复制段在源文件里**跨 1964 行只取了 1402 条**，
中间跳过 562 条旁支 —— 旁支是**夹在链记录中间**的。

⇒ 新夹具必须把**旁支放在两条链记录之间**，两种算法才会分道扬镳。

## §2 新夹具（合成，**不含任何真实对话内容**）

按真机观察到的形状造：

| 行 | type | uuid | parent | 作用 |
|---|---|---|---|---|
| 0 | `user` | `u1` | null | 根；带 `slug` + `gitBranch` |
| 1 | `assistant` | `u2` | `u1` | 分叉点 |
| 2 | `mode` | — | — | **无 uuid 旁挂** |
| 3 | `user` | `u6` | `u2` | ★ **被 ESC 回退的旁支，夹在链中间** |
| 4 | `assistant` | `u7` | `u6` | ★ 旁支延续 |
| 5 | `ai-title` | — | — | **无 uuid 旁挂** |
| 6 | `system` | `u3` | `u2` | 主链继续 |
| 7 | `file-history-snapshot` | — | — | **无 uuid 旁挂** |
| 8 | `user` | `u4` | `u3` | 带 `sourceToolAssistantUUID` + `logicalParentUuid`（指链外） |
| 9 | `assistant` | `u5` | `u4` | 叶 |

从 `u4` 分叉：

- **祖先回溯** → `[u1, u2, u3, u4]`（4 条）
- **线性切片 `[0..=8]`** → 9 条（含 `mode` / `u6` / `u7` / `ai-title` / `file-history-snapshot`）

## §3 子 agent 的防御性 reject

`create_branch_session` 是 Tauri 命令，**可以被传任意 uuid**。F77 只是在前端
「子 agent 查看器」里不挂按钮 —— 后端没有任何拦截。若传进一条 `isSidechain: true`
的记录，会把一段 subagent 转录当成一个会话产出来。

⇒ 加一条 reject。**判据是 truthy 而不是「字段存在」**：真机里 `isSidechain` 这个键
在复制段的 244/244 条上都**存在**，值是 `false`。按「存在即拒」会把正常会话全拒掉。

## §4 变异（退出码判定，且改完先 grep 计数确认落地）

「改成 SDK 那套」四条 + reject 一条：

| # | 变异 | 该打红的 |
|---|---|---|
| M1 | 线性切片取代祖先回溯 | 记录条数与类型 |
| M2 | remap 全部 uuid | uuid 原样保留 |
| M3 | 清掉 `slug` / `sourceToolAssistantUUID` | 泄漏字段保留 |
| M4 | 把末条 timestamp 改成 now | timestamp 逐字相同 |
| M5 | 去掉 sidechain reject | 子 agent 拒绝 |

## §5 实做与验证

**实做三件，落盘行为一字未改**：

1. `native_shape_session()` 合成夹具 + `branch_matches_native_fork_shape` 六条断言
   （祖先回溯 / uuid 原样 / timestamp 不改 / 泄漏字段保留 / `logicalParentUuid` 指链外不报错 /
   根 null + 只有四类记录类型）。
2. `build_branch_records` 加 sidechain reject（**truthy 判据**）+ `branch_rejects_sidechain_record`
   带反向自检（`isSidechain:false` 必须照常可分叉）。
3. 函数头注补一段实证证据 + 「别拿 SDK 当规范」的警示，并指名新测试为该判据的机检版本。

**变异 5 条，逐条退出码见红，且每条都先 grep 计数确认变异真的落地**：

| # | 变异 | 结果 |
|---|---|---|
| M1 | 线性切片取代祖先回溯 | 红 |
| M2 | remap 全部 uuid | 红 |
| M3 | 清掉 `slug` / `sourceToolAssistantUUID` | 红 |
| M4 | 末条 timestamp 改成 now | 红 |
| M5 | 去掉 sidechain reject | 红 |

**额外做了一步「红得对不对」的核查**：`rc=101` 既可能是测试失败也可能是**编译失败**
（后者是假红）。逐条抓输出确认全部是**真·测试失败**并带预期的断言消息。

**§1 那个判断被实测印证**：M1 之下，旧测试 `branch_copies_ancestor_prefix_in_native_format`
**照样绿**，只有新夹具那条红 —— 旧夹具确实区分不了两种算法。

**门禁**：`cargo fmt --check` 干净 · `cargo test --all` **646**（+2）· clippy 62 警告（未变基线）·
vitest 1048（前端未动，确认无连带）。

## §6 签收

- [x] 过代码审计（自审 + 5 条变异，退出码判定 + 真假红核查）
- [x] 过工程审计
- [x] 主计划已更新
