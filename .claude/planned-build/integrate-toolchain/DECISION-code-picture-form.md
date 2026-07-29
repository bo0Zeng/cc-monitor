# 决策记录 — code-picture 的产品形式（2026-07-29）

> **状态：方向已定，未开工。** 开工需要**另外授权**——code-picture 是另一个仓
> （`/home/zbl/文档/project/self项目/code-picture/code-picture`），本会话的授权只覆盖 cc-monitor。

## 决定

1. **CLI 为本体。** `core` + `lsp` 的能力从命令行出。
2. **MCP 那 549 行冻结，一行不改。** 不做中间态迁移。
3. **T06（搬进 cc-monitor 仓）保持搁置。**

## 为什么（按分量排，不是按听起来顺）

1. **CLI 的输出是可引用证据。** 本会话 38 个 commit 里，code-picture 的 MCP 工具全程可用、
   项目自己的 CLAUDE.md 还写着 brownfield 摸底要走它，而我**一次都没用**——全程 grep + Read；
   我派出的 5 个审计 agent 也一样。原因不是工具不好：**grep 的输出本身就是我要贴进
   commit message 的东西，而 MCP 回来的是一份我还得再验一遍的摘要。**
   **注意这个理由比第一版讲的窄**：不是"MCP 贵"。本会话 code-picture 的工具是 deferred 的
   （只加载名字、没加载 schema），Tool Search 已经把 context 成本解掉了。成本不是原因。
2. **CLI 同时服务 Codex。** cc-monitor 自己就支持 `--agent codex`；只能被一个 agent 消费的接口，
   在多 agent 场景里是自我限制。
3. **协议问题消失。** 见下。

**反方证据，如实记**：本会话我犯了**四次**「以为在本机、其实在远端」，那四次问的都是
「到底谁在读这个路径」——`impact` / `callers` 正是干这个的。**这是保留某种结构化查询的最硬理由**，
比"省 token 的全局观"硬得多。所以 CLI 要把 `impact`/`callers` 做成一等命令，别只做 `overview`。

## MCP 协议现状（2026-07-29 实查，官方 changelog）

最新修订 **`2026-07-28`**——**前一天才定稿**，官方称「launch 以来最大的一次修订」。
我们声明 `2024-11-05`，中间隔 4 个修订版。

对那 549 行的精确影响：

| 变更 | 现状 |
|---|---|
| **`initialize`/`notifications/initialized` 握手整个删除**，版本+capabilities 移进每请求 `_meta` | 我们那段 `"initialize" => ok(...2024-11-05...)` 成死代码 |
| **`server/discover` 变 MUST**，官方明确它**可当 STDIO 向后兼容探针** | 未实现——这是唯一迁移锚点 |
| 所有 result 必带 `resultType`；**但客户端 MUST 把老服务器省略该字段的结果当 `complete`** | 未实现——**这条是官方给老服务器的明文兼容承诺** |
| `tools/list` 必带 `ttlMs` + `cacheScope` | 未实现 |
| `ping`/`logging/setLevel` 删除；Roots/Sampling/Logging 弃用（≥12 月窗口） | 都没用，不受影响 |
| `outputSchema`/`structuredContent` 这版是**放宽**（更早就有） | 都没实现 |

**stdio 没被弃用**（被弃用的是 HTTP+SSE）。所以移除 `Mcp-Session-Id`、SSE 续传、OAuth 收紧
这些听着吓人的变化**一条都不碰我们**——它们全是 HTTP 传输层的事。

**Claude Code 侧官方未表态**：只说「rolling out across Claude products **soon**」，
无日期、未说是否继续支持 `2024-11-05`。MCP 官方博客那句更有用：
**「upgrading is opt-in, nothing changes until both you and your clients act」**。

**没有找到官方说「CLI/skill 优先于 MCP」**——搜到的对比全是第三方博客。
官方唯一相关的是 Tool Search（按需加载工具定义，砍掉约 85% context 成本）。

## 所以为什么现在**不动** MCP

从 `2024-11-05` 到 `2026-07-28` 要改的是：删 `initialize` + `_meta` 版本 + `server/discover` MUST
+ `resultType` + `ttlMs`/`cacheScope`——对 549 行来说是**重写，不是打补丁**。
而 Claude Code 的切换**没有时间表**。**现在做迁移是给一个未公布日期的目标做适配。**

这次修订还定了 feature lifecycle + 最短 12 个月弃用窗口 + conformance suite
——**以后不会再有这种一次性大破坏，反过来说明现在等一等的代价很低。**

## 重启时的动作（不是现在）

1. CLI 骨架：`code-picture <verb> --repo X`，`impact`/`callers` 与 `overview` 同为一等命令
2. skill 改指 CLI（`~/.claude/skills/code-picture/`）
3. MCP：等 Claude Code 真切了再定重写还是废弃；**重写就照 `server/discover` + `_meta`**，不做中间态
4. cc-monitor 侧**不受影响**——它直接链 `code_picture_core::Engine`（`panorama.rs`），从来不经 MCP
