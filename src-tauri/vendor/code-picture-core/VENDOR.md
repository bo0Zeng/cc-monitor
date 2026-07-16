# vendored: code-picture-core

Batch 15(code-picture 融合)决策 D1 = **vendor 源码进仓**。这里是 sibling 仓
`code-picture` 的 `crates/code-picture-core` 的**源码副本**(只 `src/` + `Cargo.toml`,
不含 `tests/` —— 测试留上游),作为 cc-monitor `src-tauri` 的 path 依赖。

**为什么 vendor 而非 submodule**:code-picture 是独立 Cargo workspace、仓路径含中文,
submodule 会让 Windows CI/release 的 checkout + 跨 workspace 解析出坑;vendor 后源码即在
cc-monitor 仓内,CI 无需改 checkout、构建自洽。

**副本是上游的镜子,不是分身**(SS-10 铁律):**只照上游改,绝不在副本里改出自己的版本**。
要加字段(如 F68 的 signature)先改上游再 re-vendor。`build.rs::check_vendor_freshness`
会在上游领先副本时发 `cargo:warning`(过期看得见)。

## 来源
- 上游仓:`/home/zbl/文档/project/self项目/code-picture/code-picture`
- vendored commit:**`d8f1fe7`**(F68 审计修:Kotlin 整门签名 + JS/TS 箭头 override + 补测试)
- vendored 时间:2026-07-16
- 沿革:`e6b9d64`(F18,07-10)→ `179a5b2`(F68 signature+DB迁移)→ `d8f1fe7`(F68 signature 修)

## F68 re-vendor 带进的变化(消费方须知)
- **`Symbol` 加 `signature: Option<String>`**:函数签名文本,全景图详情面板展示。
  提取覆盖:Rust/Python/C/C++/C# 干净;JS/TS 含箭头赋值(`const f=()=>{}`/类字段)已修;
  Kotlin(块体+表达式体)已修。已知降质(记债):Java 注解污染签名、接口方法无 body 缺失、
  C++/C# 构造器初始化列表并入。
- **`EngineOpts {} → EngineOpts { store_dir: Option<PathBuf> }`**(上游 F27):**消费方
  `panorama.rs` 必须传 `store_dir`**(用 `None` 保持索引落被分析仓 `<repo>/.codepicture`;
  或 `EngineOpts::default()` 等价)。
- **`scan.rs` 跳过 `.claude/` + 链接 worktree**(上游 45c6c90):Claude 用户仓里
  `.claude/worktrees/` 不再被当重复代码索引 → `panorama_status.symbols` 计数**下降**(预期修复)。
- **`Lang` 加 `Hash` derive**(上游 F22):纯增量,无害。
- **`index.rs` 加 DB schema 迁移**(SCHEMA_VERSION=2 + meta 表版本):旧 `index.db`(无
  signature 列)打开时自动 drop 派生表重建(index.db 派生 + gitignore,reindex 便宜)——
  否则旧库 SELECT signature 爆 "no such column"。回归测试在上游 `tests/index_test.rs`。

## 如何 re-vendor(上游有更新时)
```
UP=<code-picture 仓>/crates/code-picture-core
VD=src-tauri/vendor/code-picture-core
# 注意:rm 会删本 VENDOR.md(副本特有、非上游文件),re-vendor 后重写它、更新 pin
rm -rf "$VD" && cp -r "$UP" "$VD" && rm -rf "$VD/tests" "$VD/target"
# 重建本 VENDOR.md(更新 commit/时间/变化);cargo build 验证;跑门槛
```

## 注意
- code-picture-core 的 `Cargo.toml` **无 workspace 继承**(version/edition/deps 全字面量),
  故复制即 standalone,无需内联 workspace 键。
- `Engine::open(repo, opts)` 会往**被索引的仓**写 `.codepicture/`(派生索引 db),不是往本
  vendor 目录写。cc-monitor `.gitignore` 已加 `.codepicture/` + `src-tauri/vendor/**/Cargo.lock`。
- 内核零 Tauri/OS 依赖;传递依赖 = tree-sitter × 9 grammar + rusqlite(bundled,自带 SQLite)
  + serde。**bundled SQLite + 9 门 grammar 的 C 编译是构建变慢的来源**(P0 实测项)。
