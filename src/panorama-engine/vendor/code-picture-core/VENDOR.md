# vendored: code-picture-core

Batch 15(code-picture 融合)决策 D1 = **vendor 源码进仓**。这里是 sibling 仓
`code-picture` 的 `crates/code-picture-core` 的**源码副本**(只 `src/` + `Cargo.toml`,
不含 `tests/` —— 测试留上游),作为 cc-monitor `src-tauri` 的 path 依赖。

**为什么 vendor 而非 submodule**:code-picture 是独立 Cargo workspace、仓路径含中文,
submodule 会让 Windows CI/release 的 checkout + 跨 workspace 解析出坑;vendor 后源码即在
cc-monitor 仓内,CI 无需改 checkout、构建自洽。

**副本是上游的镜子,不是分身**(SS-10 铁律):**只照上游改,绝不在副本里改出自己的版本**。
要加字段/改行为先改上游再 re-vendor。`build.rs::check_vendor_freshness` 会在上游领先副本时
发 `cargo:warning`(过期看得见)。

## 来源
- 上游仓:`/home/user/文档/project/self项目/code-picture/code-picture`
- vendored commit:`43c5b73`(RM1d:批注 / 文档关联的写拆成「算」与「写盘」两层)
- vendored 时间:2026-09-24
- 取自上游分支 **`cc-monitor/rm1d`**(worktree `/home/user/cc-wt/up-rm1d`,只本地提交、不推 —— V79);
  并进上游 `main` 由主会话做。`build.rs::check_vendor_freshness` 比的是 `pin..上游 HEAD`,这笔并进 main 之前
  上游 HEAD(`276b531`)是 pin 的祖先 ⇒ 不报过期(看不见「副本领先上游 main」这一形 —— 记下,不改尺子)。
- 沿革:`e6b9d64`(F18,07-10)→ `179a5b2`(F68 signature+DB迁移)→ `d8f1fe7`(F68 审计修)→
  `d558e47`(F72 批注分家回仓)→ `276b531`(PN1b,09-24;中间 34 笔,见下)→ `43c5b73`(RM1d,09-24,1 笔)

## RM1d re-vendor 带进的变化(09-24,`276b531..43c5b73`,1 笔;用户 V110「引擎只算、文件管理来写」)

- **新模块 `edits`**:批注 / 文档关联的写拆成三层 —— 纯的 `next_*`(给定现状 → 新内容)· 只读的 `plan_*(repo, …)`
  (读现状 + 纯)· 写盘 `annotations::apply` / `docs::apply`。`plan_*` 回 `Planned { value, edit: Option<FileEdit> }`,
  `FileEdit { rel, before, after, parents }`(`before` 当 CAS 期望、`after = None` 即删;有改动时 `before != after`)。
- **消费方怎么用**:全景小程序(`src/panorama-engine`)只调 `plan_*`,把 `FileEdit` 原样交回;落盘由那台机器
  后端的文件管理(`files-put` 带 `expect` / `files-delete`)做。`Engine` 的六个写方法签名行为不变(上游 MCP / 测试照旧用),
  消费方生产段**零调用**(小程序的判据钉着)。
- `Engine::refresh_doc_links`(公开):外面写了 `.md` 的 `covers:` 之后让索引跟上(只写索引)。
- `annotation_id` 搬进 `edits` 公开;`annotations::{ANNOTATIONS_REL, rel_path, render, parse}`、
  `docs::{guard_doc_rel, frontmatter_covers}` 公开 —— 存储格式仍只在上游定义一次。
- 依赖差零。

## PN1b re-vendor 带进的变化(09-24,`d558e47..276b531`,34 笔)

- **依赖差零**:`cargo tree -p code-picture-core -e normal` 两边同一份 40 个包;release rlib 2.0 MB → 5.0 MB。
  精确层(语言服务器)住上游的 `code-picture-lsp`,**不在这份副本里**;core 只留接缝(`precise.rs`),不起进程。
- **画图**(本件的正题):`diagram::registry`(图种注册表,唯一真相源)· `Engine::draw(kind, &req)`
  统一出口 · `Diagram { kind, honesty, body }` 按形状分变体 · 公共诚实信号 `Honesty` · 全部 `Serialize`。
  消费方 `panorama.rs` 新增 `panorama_diagram_kinds` / `panorama_diagram` 两条命令原样透出。
- **批注来源**:`Annotation.origin`(`Human` / `Agent` / `Unrecorded`),批准只改 `status`。
- **模型扩档**(消费方的手写 TS 类型要跟):`Confidence` 加 `Dispatch`(动态派发)· `EdgeKind` 加
  `AmbiguousCall` · `Edge` 加 `candidates` / `arg_flow` · `Overview` 加 `ambiguous_calls` /
  `unresolved_imports` / `db_errors` · `Subsystem` 加 `member_hash` / `anchors` / `internal_edges` /
  `external_edges` · `Symbol` 加 `return_type` / `return_flow` / `param_flow`。
- 调用图不再「猜了当真」(歧义边单列)、社区检测换加权 Louvain、控制流/数据流一族(`cfg` / `pdg` /
  `graph::*`)进来了 —— 全景命令面没用到它们。
- 索引库 `SCHEMA_VERSION` 变了的话旧 `index.db` 打开会自动重建派生表(与 F68 同一机制)。

## F72 re-vendor 带进的变化(消费方须知)
- **批注与索引分家**:`store_dir` 原本把整个 `.codepicture`(索引 index.db + 批注 annotations/)
  一起搬;现在 **`store_dir` 只再控索引**,**批注恒落被分析仓 `<repo>/.codepicture/annotations/`**
  (可提交、随仓走、别人 clone 可见、抗仓移动)。`Engine` 加 `annotations_dir` 字段(恒仓内)、去掉
  只服务批注的 `dot` 字段。**消费方 cc-monitor `panorama.rs` 无需改**——`panorama_store_dir()`
  传 `store_dir=Some(数据目录/panorama)` 照旧,索引仍落数据目录、批注自动落用户仓。
  - **D20 保住**:`open` 不 eager 建仓内批注目录(批注首写才 lazy 建),开面板/查状态仍不污染用户仓。
    cc-monitor `panorama.rs` 的 D20 回归测试(只 index、不写批注)仍成立。
  - **`.gitignore`**:`ensure_gitignore` 仍由 `open` 在 index 侧写(默认模式下与 annotations/ 同目录、
    忽略 `/index.db`、批注可提交;store_dir 模式下写在仓外无关紧要)。批注目录本身不需 gitignore。
  - **dogfood 注意**:cc-monitor 若给自己仓建批注,仓根 `.gitignore` 若忽略 `.codepicture/` 会吞批注——
    那是「被索引仓自己的事」,按需在自己仓放行 `.codepicture/annotations/`。

## F68 沿革变化(仍适用)
- **`Symbol` 加 `signature: Option<String>`**:函数签名文本,全景图详情面板展示。9 门提取。
- **`EngineOpts { store_dir: Option<PathBuf> }`**:消费方 `panorama.rs` 传 `Some(数据目录)`(F69,D20)。
- **`index.rs` DB schema 迁移**(SCHEMA_VERSION=2):旧 `index.db` 打开自动 drop 派生表重建。
- `scan.rs` 跳过 `.claude/` + 链接 worktree;`Lang` 加 `Hash`。

## 如何 re-vendor(上游有更新时)
```
UP=<code-picture 仓>/crates/code-picture-core
VD=src/panorama-engine/vendor/code-picture-core
# 注意:rm 会删本 VENDOR.md(副本特有、非上游文件),re-vendor 后重写它、更新 pin
rm -rf "$VD" && mkdir -p "$VD" && cp -r "$UP/src" "$UP/Cargo.toml" "$VD/"
# 重建本 VENDOR.md(更新 commit/时间/变化);cargo build 验证;跑门槛
```

## 注意
- code-picture-core 的 `Cargo.toml` **无 workspace 继承**(version/edition/deps 全字面量),
  故复制即 standalone,无需内联 workspace 键。
- `Engine::open(repo, opts)` 往被索引仓写 `.codepicture/`(索引 db 随 store_dir、**批注恒仓内**);
  cc-monitor `.gitignore` 已加 `.codepicture/` + `src-tauri/vendor/**/Cargo.lock`。
- 内核零 Tauri/OS 依赖;传递依赖 = tree-sitter × 9 grammar + rusqlite(bundled,自带 SQLite)
  + serde。**bundled SQLite + 9 门 grammar 的 C 编译是构建变慢的来源**。
