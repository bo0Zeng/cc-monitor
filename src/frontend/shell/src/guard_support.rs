//! 测试用的**住址唯一源** —— 「仓根在哪 / 本 crate 的源码在哪 / 测试树在哪」只答一次。
//!
//! 🔴 **它为什么存在**：2026-09-18 装齐 Tauri 栈、本 crate 在本机第一次编译并跑测试，
//! 一次就红了 **160 条**。根因只有一个：仓库重组把本 crate 从 `<repo>/src-tauri` 搬到
//! `<repo>/src/bridge`，于是 `CARGO_MANIFEST_DIR` 到仓根**从一级变成两级**（收尾重排再下沉一级到
//! `<repo>/src/frontend/shell`，三级；这一回只改了 [`repo_root`] 一处），
//! 而「仓根在哪」这件事在本 crate 里有 **24 份各自独立的答案**，一份都没跟着改。
//!
//! 其中 `shared_crate_registry` 那份的 `.expect()` 消息已经被改成
//! 「`src/bridge` 的上级 = 仓根」——**注释改对了，代码没改**。这是最能说明问题的一处：
//! 散落的副本会各自漂，而没有任何判据数着它们。
//!
//! ⇒ 纪律（一条形状出现 N 次，就抽一个住址）
//! ＋ `§5.2`（扫描型测试拿不到人群会扫空集 ⇒ 恒绿 ⇒ 必须配反空真自检）。
//! 后端那一半早已这么做（`src/backend/guard_support.rs`），本文件是它的对侧。

#![cfg(test)]

use std::path::{Path, PathBuf};

/// 本 crate 的包根 —— `<repo>/src/frontend/shell`。
pub(crate) fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 本 crate 自己的源码树 —— `<repo>/src/frontend/shell/src`。
/// ⚠ 与 `repo_src_root()` 不是一件事，别混。
/// 拿它当根交给 `guard_core` 扫的时候，人群还含本包 manifest 明写的兄弟源码树（`[package.metadata.guard] population`：
/// 通道 · 前端宿主原语 · 开窗契约 · 文件窗口，`guard_core::population_trees`）—— monitor 的代码搬进去了，人群不变；住址 `<包名>/…`。
pub(crate) fn crate_src_root() -> PathBuf {
    crate_root().join("src")
}

/// 本 crate 源码人群的全部根：自己的 `src/` ＋ manifest 明写的兄弟包的 `src/`（`guard_core::population_trees`）。
/// 自己走目录（不经 `guard_core` 遍历器）的那几条判据按它逐棵走 —— 人群与经遍历器的那几条同一群，声明只有 manifest 那一处。
pub(crate) fn crate_population_roots() -> Vec<PathBuf> {
    std::iter::once(crate_src_root())
        .chain(
            guard_core::population_trees(&crate_src_root())
                .into_iter()
                .map(|(_, src)| src),
        )
        .collect()
}

/// 仓根 —— `<repo>`。**`src/frontend/shell` 的上三级**（包根从 `src/bridge` 下沉一级）。
pub(crate) fn repo_root() -> PathBuf {
    crate_root()
        .ancestors()
        .nth(3)
        .expect("src/frontend/shell 的上三级 = 仓根")
        .to_path_buf()
}

// 问 git 索引的那一份住测试树（它起一个 `git` 进程；生产树里起进程要进出口的登记）。
#[path = "../../../../tests/frontend/shell/guard_support_tracked.rs"]
mod tracked;
pub(crate) use tracked::tracked_under;

/// 采集面对拍：`dirs` 下 git 跟踪着的每一份 `ext` 文件都得在 `seen`（仓根相对、正斜杠）里，`skip` 里的除外。
/// 漏了 ⇒ panic，点名漏的是哪几份（路径拼错 · 后缀过滤掉 · 静默跳过子目录都在这里红）。
pub(crate) fn assert_scanned_every_tracked(
    label: &str,
    seen: &[String],
    dirs: &[&str],
    ext: &str,
    skip: &[&str],
) {
    let missing: Vec<String> = dirs
        .iter()
        .flat_map(|d| tracked_under(d, ext))
        .filter(|rel| !skip.contains(&rel.as_str()) && !seen.iter().any(|s| s == rel))
        .collect();
    assert!(
        missing.is_empty(),
        "{label}：这几份 git 跟踪着的 `.{ext}` 没被扫到 —— 采集面塌了，零命中在空转：\n  {}",
        missing.join("\n  ")
    );
}

/// 第 `i` 行（0 起）所在的那一项的名字：往上找最近一处包着它的 `fn` / `struct` / `enum` / `const` / `static` / `trait`
///（Rust 源码；按缩进与收口的 `}` 认外层，粗，但归错会红在名字对不上上）。找不到 ⇒ `"<顶层>"`。
///
/// 逐处登记按「文件 × 外层函数」认，不按处数认：同一个函数里多写一处不改登记，新函数里写一处要登记。
pub(crate) fn enclosing_fn(lines: &[&str], i: usize) -> String {
    let indent = |l: &str| l.len() - l.trim_start().len();
    let Some(here) = lines.get(i) else {
        return "<顶层>".to_string();
    };
    let fn_name = |l: &str| -> Option<String> {
        let t = guard_core::strip_visibility(l.trim_start());
        let t = t.strip_prefix("async ").unwrap_or(t);
        let rest = ["fn ", "struct ", "enum ", "const ", "static ", "trait "]
            .iter()
            .find_map(|k| t.strip_prefix(k))
            .or_else(|| l.split(" fn ").nth(1))?;
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        (!name.is_empty()).then_some(name)
    };
    // 命中落在签名那一行上：就是这个函数。
    if let Some(name) = fn_name(here) {
        return name;
    }
    let here_indent = indent(here);
    // 往上走时见过的最浅的 `}` 行：比它深（或一样深）的 `fn` 已经在那儿收口了，不是外层。
    let mut closed = usize::MAX;
    for prev in lines[..i].iter().rev() {
        let t = prev.trim_start();
        let d = indent(prev);
        if t.starts_with('}') {
            closed = closed.min(d);
            continue;
        }
        if d >= here_indent || d >= closed {
            continue;
        }
        if let Some(name) = fn_name(prev) {
            return name;
        }
    }
    "<顶层>".to_string()
}

/// 一份文件相对仓根的路径（正斜杠）。
pub(crate) fn rel_of(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 仓里的 `src/`（前端 TS ＋ 两个 Rust 半都住这儿）—— `<repo>/src`。
pub(crate) fn repo_src_root() -> PathBuf {
    repo_root().join("src")
}

/// 仓里的测试树 —— `<repo>/tests`。
pub(crate) fn tests_root() -> PathBuf {
    repo_root().join("tests")
}

/// 后端那一半的源码树 —— `<repo>/src/backend`。
pub(crate) fn backend_src_root() -> PathBuf {
    repo_src_root().join("backend")
}

/// 后端命令表各族（`src/backend/stream/inbound/registry/*.rs`）：`(仓内相对路径, 生产段)`，按路径排，一族一份。
///
/// 按 `CommandSpec {` 切块的判据要**逐份切**，别把几份拼起来再切：拼起来的话，
/// 一份的文件头（`use` · 常量）会粘到上一份的最后一块上，被算成那条命令的一部分。
pub(crate) fn backend_registry_sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut out: Vec<(String, String)> = guard_core::scan_tree_excluding(
        &backend_src_root().join("stream/inbound/registry"),
        &["rs"],
        &[],
    )
    .into_iter()
    .map(|(path, src)| {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        (rel, guard_core::production_code(&src))
    })
    .collect();
    out.sort();
    assert!(
        out.len() >= 2,
        "只扫到 {} 份后端命令表族文件 —— 住址坏了，用它的判据会在空人群上恒绿",
        out.len()
    );
    out
}

/// 会话流来源那个目录（`src/frontend/shell/src/stream_source/`）下的**每一份**源文件：`(仓内相对路径, 原文)`，按路径排。
///
/// 那份东西从一份文件拆成一个目录之后，人群曾经是「那一份文件」的判据，人群现在是这一整个目录 ——
/// 只看其中一份会安静地少一块人群（不红，只是扫不全）。
pub(crate) fn stream_source_files() -> Vec<(String, String)> {
    let root = repo_root();
    let mut out: Vec<(String, String)> =
        guard_core::scan_tree_excluding(&crate_src_root().join("stream_source"), &["rs"], &[])
            .into_iter()
            .map(|(path, src)| {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, src)
            })
            .collect();
    out.sort();
    assert!(
        out.len() >= 2
            && out
                .iter()
                .any(|(rel, _)| rel == "src/frontend/shell/src/stream_source/mod.rs"),
        "会话流来源目录只扫到 {} 份 —— 住址坏了，用它的判据会在空人群上恒绿",
        out.len()
    );
    out
}

/// [`stream_source_files`] 各份的**生产段**（逐份剥测试段之后）按路径顺序接成一段。
/// 判据要的是「整个会话流来源里有没有 / 有几处」时用它；要逐份报住址的用 [`stream_source_files`]。
pub(crate) fn stream_source_production() -> String {
    stream_source_files()
        .iter()
        .map(|(_, src)| guard_core::production_code(src))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 会话流来源目录里**一份**文件的原文（`name` 是目录内文件名，如 `run.rs`）。
pub(crate) fn stream_source_file(name: &str) -> String {
    let rel = format!("src/frontend/shell/src/stream_source/{name}");
    stream_source_files()
        .into_iter()
        .find(|(r, _)| *r == rel)
        .unwrap_or_else(|| panic!("会话流来源目录里没有 `{name}` —— 住址坏了"))
        .1
}

/// `parse_frame` 认得的全部 `kind`：从它的生产段源码里摘（`"xxx" =>` 那几条臂）。
/// 认得哪些种类不另抄一张表：判据拿它与跨语言金样的种类两向比。
pub(crate) fn parse_frame_kinds() -> std::collections::BTreeSet<String> {
    let prod = stream_source_production();
    let lines: Vec<&str> = prod.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.starts_with("pub fn parse_frame("))
        .expect("stream_source 生产段里找不到 `pub fn parse_frame(` —— 抽取面画错了");
    let end = (start..lines.len())
        .find(|&k| lines[k] == "}")
        .expect("`parse_frame` 没有收尾");
    let mut kinds = std::collections::BTreeSet::new();
    for l in &lines[start..=end] {
        if let Some(rest) = l.trim_start().strip_prefix('"') {
            if let Some((k, tail)) = rest.split_once('"') {
                if tail.trim_start().starts_with("=>") {
                    kinds.insert(k.to_string());
                }
            }
        }
    }
    assert!(
        kinds.len() >= 10,
        "只从 parse_frame 里摘到 {} 种 —— 抽取坏了（用它的判据此刻是空转的）",
        kinds.len()
    );
    kinds
}

/// [`stream_source_files`] 各份的**原文**（不剥测试段）按路径顺序接成一段。
pub(crate) fn stream_source_raw() -> String {
    stream_source_files()
        .into_iter()
        .map(|(_, src)| src)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/guard_support_tests.rs"]
mod tests;
