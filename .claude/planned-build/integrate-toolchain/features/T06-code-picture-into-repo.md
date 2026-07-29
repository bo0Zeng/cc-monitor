# 功能计划 — T06 code-picture 搬进本仓（vendor 从「镜子」升格为「本体」）

## 0. 现状实测（不是从 VENDOR.md 抄的，是刚跑出来的）

| 事实 | 值 |
|---|---|
| 上游仓 | `/home/zbl/文档/project/self项目/code-picture/code-picture`（**存在**；不是 cc-monitor 的 sibling，在另一棵树下） |
| 上游 HEAD | `d558e47`（F72 批注与索引分家） |
| VENDOR.md 记的 vendored commit | `d558e47` — **完全一致，上游领先 0 个 commit** |
| `diff -rq` 上游 `core/src` vs vendor `core/src` | **无差异**（逐字一致，副本没漂移） |
| 上游 crates | `code-picture-core`（src 4378 / tests 2467）· `code-picture-lsp`（src 1381 / tests 551）· `code-picture-mcp`（src 549 / tests 536） |
| 本仓现有 vendor | **只有 `code-picture-core` 的 `src/` + `Cargo.toml`**，无 `tests/` |
| 依赖链 | `mcp → lsp → core`（两条 path 依赖） |

**这是「翻成本体」最好的时机窗口**：副本与上游逐字一致，没有分歧要调和。
这个前提**以后不一定还在**——一旦上游继续走，翻转就要先做一次三方合并。**计划把这条写在最前面。**

## 1. D2 的真实工作量比"搬 tests/"大得多

已决策 D2：`code-picture-mcp` 二进制**由本仓构建+部署**、进受管工具注册表 → CI 多一个构建目标。
数下来那意味着：

- vendor 从 **1 个 crate 的 src** 变成 **3 个 crate 的 src + tests**（src 6308 行 + tests 3554 行）
- 本仓 `src-tauri` 目前只 path-依赖 `core`；要构建 `mcp` 就得把 `lsp` 也搬进来（`mcp → lsp`）
- `mcp` 是个**独立二进制**，不是 `src-tauri` 的库依赖 → CI 要新增一个 `cargo build --bin` 目标，
  且它要跟着 release 一起分发（跨平台？只 Linux 远端？**这一条计划里没定，是个待决点**）

**所以 T06 拆两步**，第一步不碰 D2：

## 2. 第一步（本轮做）：`core` 升格为本体 + 搬 `tests/`

- [ ] `vendor/code-picture-core/tests/` 从上游同批搬入（2467 行）
- [ ] `build.rs::check_vendor_freshness` **删掉**——它的语义是"上游领先就 warn"，
      而升格为本体之后**没有上游可领先**。留着就是一句永远不会响的警报
      （本会话已两次因为"永远不会红的钉子"付账）
- [ ] `VENDOR.md` 改写为「本体，非镜子」：说清从此在本仓改、上游 sibling 仓退役、
      **并记下翻转时刻的 commit `d558e47` 与"当时逐字一致"这个事实**（将来若有人从上游捡东西，
      这是唯一的对齐锚点）
- [ ] `cargo test` 必须把新搬入的 2467 行 tests 真跑起来（**不是搬进来放着**——
      要确认它们在本仓的 workspace 配置下能编译、能跑、且全绿；跑不起来就说跑不起来）
- [ ] 跨仓引用：**先只改指向"上游仓路径"的那些**，B 级（README / doc / 计划文档）本轮改，
      **A 级（`claude-code.md` + daemon 协议）不动**——memory 记着它需要两仓 lockstep

**不做**：`lsp` / `mcp` 两个 crate（第二步）· CI 新增构建目标（第二步）·
`code-picture-mcp` 进注册表（第二步，且要按 T04 第一步的 `host` 逐条声明）

## 3. 第二步（登记，不在本轮）：`lsp` + `mcp` + CI 目标

**开工前必须先定的待决点**（现在没答案，别猜着做）：
1. `code-picture-mcp` 分发到哪台机器？按 T04 的 `host` 模型它是 `Remote` 还是 `Client`？
   （用户在 Windows 客户端上跑 cc-monitor，而 MCP 要给远端的 Claude Code 用 → 大概是 `Remote`，
   但那就要交叉编译 musl，和 daemon 同一套 `embedded-daemons` 机制 —— **这是第二步的主要工程量**）
2. 进注册表后它的 `destination` 是常量还是 `UserConfiguredPath`？
   （T04 审计教训：**凭印象写个假常量比不声明更坏**）
3. CI 新增目标会不会把 Windows job 拖长？现有 `windows-latest` job 已是瓶颈。

## 4. 风险（memory 里那条，必须逐条防）

- 语料已从 `code-picture/` 搬进 `allgent-picture/`（两个仓都在
  `/home/zbl/文档/project/self项目/` 下）→ **改引用时别把 `code-picture` 全局替换成 `allgent-picture`**：
  代码仓仍叫 `code-picture`，只有**语料**搬走了。
- **别误伤**：`vendor/code-picture-core` crate 本身 · INVARIANTS 的 uuid-复用段 · panorama 功能族
  · `.codepicture/` 运行期目录（本仓根下有一个 `code-picture-1ea376b6bc064b06`）
- 删 `check_vendor_freshness` 时**别顺手删** `check_acct_iso_vendor_freshness`
  ——cc-acct-iso 仍是镜子，那条还有用（`build.rs:7` 两条紧挨着）

## 5. 测试策略

- 搬入的 tests 必须**真跑**：`cargo test -p code-picture-core` 报出数字，不绿就如实说
- **结构性守卫**：`build.rs` 里不得再出现 `check_vendor_freshness`（剥注释 + 反向自检
  确认 `check_acct_iso_vendor_freshness` 仍在）
- VENDOR.md 的「本体」声明与 `build.rs` 的实际行为要对齐——加一条测试断言
  VENDOR.md 不再含"镜子"/"照上游改"这类措辞（**文档与代码漂移是 Phase G 的常客**）

## 6. 代码审计结果（Phase D）
（待填）

## 7. 工程审计结果（Phase E）
（待填）

## 8. 签收
- [ ] 通过代码审计（无阻塞项）
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含变更记录）
