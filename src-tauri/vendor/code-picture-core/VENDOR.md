# vendored: code-picture-core

Batch 15(code-picture 融合)决策 D1 = **vendor 源码进仓**。这里是 sibling 仓
`code-picture` 的 `crates/code-picture-core` 的**源码副本**(只 `src/` + `Cargo.toml`,
不含 `tests/` —— 测试留上游),作为 cc-monitor `src-tauri` 的 path 依赖。

**为什么 vendor 而非 submodule**:code-picture 是独立 Cargo workspace、仓路径含中文,
submodule 会让 Windows CI/release 的 checkout + 跨 workspace 解析出坑;vendor 后源码即在
cc-monitor 仓内,CI 无需改 checkout、构建自洽。上游已显式收官(接口冻结),re-vendor 低频。

## 来源
- 上游仓:`/home/zbl/文档/project/self项目/code-picture/code-picture`
- vendored commit:`e6b9d64`(F18 coverage-signal,全 loop 收官)
- vendored 时间:2026-07-10

## 如何 re-vendor(上游有更新时)
```
UP=<code-picture 仓>/crates/code-picture-core
VD=src-tauri/vendor/code-picture-core
rm -rf "$VD" && cp -r "$UP" "$VD" && rm -rf "$VD/tests" "$VD/target"
# 更新本文件的 commit/时间;cargo build 验证;跑门槛
```

## 注意
- code-picture-core 的 `Cargo.toml` **无 workspace 继承**(version/edition/deps 全字面量),
  故复制即 standalone,无需内联 workspace 键。
- 它的 `Engine::open(repo)` 会往**被索引的仓**写 `.codepicture/`(派生索引 db),不是往本
  vendor 目录写。cc-monitor `.gitignore` 已加 `.codepicture/`。
- 内核零 Tauri/OS 依赖;传递依赖 = tree-sitter × 9 grammar + rusqlite(bundled,自带 SQLite)
  + serde。**bundled SQLite + 9 门 grammar 的 C 编译是构建变慢的来源**(P0 实测项)。
