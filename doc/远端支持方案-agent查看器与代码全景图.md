# 远端支持方案：Agent 查看器 + 代码全景图（设计草案，待用户定）

> 用户 2026-07-20 问「为什么代码全景图和查看 agent 不支持远端、加上支持」→ 摸清后出方案供决策。**未写码。**
> 结论先行：**两者阻塞性质不同**——Agent 查看器是「读缺口」（有界可做）；代码全景图是「索引需代码本体」根本性阻塞（大工程 + 架构决策）。

---

## 0. 背景：远端数据现有机制
- 远端会话有 `origin`（主机 label）。远端会话**正文**读走 daemon：monitor `stream_read_remote_session` → SSH exec `<daemon> --read-session <jsonl_path>` → daemon 透传文件字节 → monitor 解析（chunk 口径与本地一致，SessionViewer 零改复用）。
- daemon 一次性查询命令派发在 `history_query.rs`：`--read-session` / `--read-session-tail` / `--read-session-from-offset` / `--usage`（Codex 已加）/ `--resolve` / `--search`。**加新命令照此派发。**
- daemon 是 musl 交叉编译、内嵌进 monitor（`embedded-daemons/`）、连远端时自动部署。改 daemon = 重编内嵌 + 走一次发版才能让远端用上。

---

## 功能 1：Agent 查看器（展开 subagent）远端支持 —— **有界、可做**

### 为什么现在不支持
- subagent 记录在 `<encoded-cwd>/<parent-session-id>/subagents/agent-<hash>.jsonl`（+ `*.meta.json`）。
- 本地 `load_subagent(parent_jsonl_path, description, timestamp)` 直接读**本地**文件：`derive_subagent_dir`（`<dir>/<stem>/subagents`）→ `list_meta_matches`（按 description 精确匹配 `*.meta.json`）→ 读匹配的 subagent jsonl。
- 远端会话的 subagent 文件**在远端机器**，本地 `load_subagent` 够不着 → `subagent.ts:79` 现直接显示「远端会话暂不支持展开 subagent」。**代码注释已写明方案：daemon `--read-subagent` 协议扩容**（照 `--read-session` 模式）。

### 方案（照 `--read-session` 镜像）
1. **daemon**（`history_query.rs` + 复刻 `subagent.rs` 的查找逻辑）：加 `--load-subagent <parent_jsonl_path> <description>`（可选 `<timestamp>` 消歧）。做：`derive_subagent_dir` → `list_meta_matches`（description 精确匹配）→ 读匹配 subagent jsonl。**输出**：首行 meta（subagent_type 等）+ 后续 subagent jsonl 原始字节（monitor 解析）。安全：路径 canonicalize + `projects/` 前缀 + symlink 逃逸校验（同 `--read-session`）、只读。
2. **monitor**（`remote_history.rs`，镜像 `stream_read_remote_session`）：加 `load_subagent_remote(parent_path, description, timestamp, origin)` IPC → SSH exec daemon `--load-subagent` → 组装 `SubagentLoadResult`（与本地 `load_subagent` 同结构，前端零差异）。
3. **前端**（`subagent.ts:79`）：`if (ctx.origin)` 分支从「暂不支持」note 改为调 `load_subagent_remote` → 拿到 `SubagentLoadResult` → 走现有 `renderChild` 渲染（嵌套渲染时 `ctx.parentPath` 切远端 subagent 路径、origin 保留，与本地路径逻辑一致）。

### 改动面 / 工作量 / 风险
- daemon：+1 命令 + subagent 查找逻辑（~简单文件逻辑、无新重依赖）+ 单测。**BUILD_ID bump**（行为变、需 redeploy）。
- monitor：+1 IPC（镜像现成的 `stream_read_remote_session`）+ 单测。
- 前端：改 1 处 origin 分支 + 复用现有渲染。
- **工作量：中**（1 daemon 命令 + 1 IPC + 1 前端分支 + 测 + Phase D 审计）。**风险：低-中**（照现成远端读模式，隔离性好；Claude 零回归——本地 `load_subagent` 不动）。
- 需重编内嵌 daemon + 发版才能让远端生效。

---

## 功能 2：代码全景图远端支持 —— **大工程 + 架构决策**

### 为什么现在不支持（根本性）
- 全景图靠 `panorama_index` 用 **tree-sitter 解析整个仓源码 → SQLite 索引**（`.codepicture/index.db`）。引擎 = vendored 本地 crate `code-picture-core`（`Engine`，tree-sitter 各语言 grammar + bundled rusqlite）。**索引需要代码文件本体。**
- 远端会话代码在**远端主机**、本地索引器够不着 → `panorama.ts:164` 现直接拒：「代码不在本机，无法建立本地 code-picture 索引」。
- **daemon 无任何索引能力**（只是 ~3MB 会话监视器）。
- 全景图有一大族查询命令（`panorama_index/reindex/status/overview/node/subgraph/callers/callees/impact/search`）——都基于本地 SQLite 索引。

### 两条路（都重）

**方案 A：daemon 内嵌 `code-picture-core` 引擎，在远端建索引 + 服务查询**
- daemon 引 `code-picture-core` crate → 远端 `panorama_index`（在远端建 `.codepicture/index.db`）→ daemon 加 `--panorama-overview/node/...` 一族命令服务查询 → monitor 各 panorama api 按 origin 路由到远端 IPC。
- **代价**：① daemon 二进制**暴涨几十 MB**（tree-sitter 多语言 grammar + bundled SQLite + 引擎）——从「轻会话监视器」变「重索引器」，与 daemon 定位冲突；② **musl 交叉编译复杂度**（tree-sitter C + rusqlite bundled 在 musl 上、两 arch）；③ 全族 panorama 命令都要在 daemon 复刻 + 各配远端 IPC + 前端 origin 路由；④ 远端建索引耗 CPU/存储（`.codepicture/` 落远端仓——注意「不侵入用户仓」的既定纪律，需落 daemon 数据目录）。
- **工作量：大**（内嵌引擎 + 交叉编译 + 复刻整族查询命令 + 各 IPC + 前端全路由 + 测）。

**方案 B：SFTP 拉远端整仓代码到本地缓存，用现有本地索引器**
- monitor SFTP 把远端会话 cwd 的整个仓拉到本地缓存目录 → 用现有 `panorama_index` 索引本地缓存 → 全景查询走**现有本地路径**（引擎零改）。
- **代价**：① **传输整个仓**（大仓巨量传输 + 本地存储；需 gitignore/排除 node_modules/target 等否则爆炸）；② **新鲜度**（远端代码变→需重拉；无 watch）；③ 远端 cwd 路径↔本地缓存路径映射；④ SFTP 拉大仓慢。
- **工作量：中-大**（仓传输 + 缓存 + 排除规则 + 路径映射 + 新鲜度）。引擎不动是优点。

### 评估
- 两方案都不是「顺手加」。**A** 把 daemon 变重、交叉编译风险高、但索引在代码所在处最高效；**B** 复用引擎、但传输/新鲜度是硬伤。
- **中间没有轻方案**——全景图的价值就是 tree-sitter 符号图，绕不开「解析代码本体」。仅列文件/读单文件给不出全景。
- 建议：**除非远端全景是高频刚需，否则性价比低**；若要做，倾向 **B**（复用引擎、隔离性好、不动 daemon 定位），先限定小仓/手动触发/带排除规则，验证价值再考虑 A。

---

## 决策点（请用户定）
1. **功能 1（Agent 查看器远端）**：做 / 不做？（有界、值得，建议做。）
2. **功能 2（代码全景图远端）**：做 / 不做 / 缓一缓？若做，选 **A（daemon 内嵌引擎）** 还是 **B（SFTP 拉代码）**？
3. 节奏：走 planned-build（plan→实现→Phase D 审计→发版）；两者都需重编内嵌 daemon + 发版。
