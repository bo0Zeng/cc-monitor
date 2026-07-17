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
- 上游仓:`/home/zbl/文档/project/self项目/code-picture/code-picture`
- vendored commit:**`d558e47`**(F72 批注与索引分家)
- vendored 时间:2026-07-16
- 沿革:`e6b9d64`(F18,07-10)→ `179a5b2`(F68 signature+DB迁移)→ `d8f1fe7`(F68 审计修)→
  `d558e47`(F72 批注分家回仓)

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
VD=src-tauri/vendor/code-picture-core
# 注意:rm 会删本 VENDOR.md(副本特有、非上游文件),re-vendor 后重写它、更新 pin
rm -rf "$VD" && cp -r "$UP" "$VD" && rm -rf "$VD/tests" "$VD/target" "$VD/Cargo.lock"
# 重建本 VENDOR.md(更新 commit/时间/变化);cargo build 验证;跑门槛
```

## 注意
- code-picture-core 的 `Cargo.toml` **无 workspace 继承**(version/edition/deps 全字面量),
  故复制即 standalone,无需内联 workspace 键。
- `Engine::open(repo, opts)` 往被索引仓写 `.codepicture/`(索引 db 随 store_dir、**批注恒仓内**);
  cc-monitor `.gitignore` 已加 `.codepicture/` + `src-tauri/vendor/**/Cargo.lock`。
- 内核零 Tauri/OS 依赖;传递依赖 = tree-sitter × 9 grammar + rusqlite(bundled,自带 SQLite)
  + serde。**bundled SQLite + 9 门 grammar 的 C 编译是构建变慢的来源**。
