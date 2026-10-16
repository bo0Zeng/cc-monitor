//! 判据用的住址，全仓一份：仓根在哪、各包的源码树与测试树在哪、几组按住址取的文件。
//!
//! 仓根从本 crate 的 manifest 目录（`src/common/guard-core`）上三级定；别处一律从仓根往下拼，搬树只改这里。
//! 只给测试档用（本 crate 只进各包的 `[dev-dependencies]`）。

use std::path::{Path, PathBuf};

/// 仓根。
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("src/common/guard-core 的上三级 = 仓根")
        .to_path_buf()
}

/// 仓里的 `src/`。
pub fn repo_src_root() -> PathBuf {
    repo_root().join("src")
}

/// 仓里的测试树 `tests/`。
pub fn tests_root() -> PathBuf {
    repo_root().join("tests")
}

/// 后端包（`src/backend`，`lib.rs` 与 manifest 同层）。
pub fn backend_src_root() -> PathBuf {
    repo_src_root().join("backend")
}

/// 后端的测试树。
pub fn backend_tests_root() -> PathBuf {
    tests_root().join("backend")
}

/// 中转 crate（`comms-outward`）的根：它是后端生产闭包的一部分（后端 manifest 的 `[package.metadata.guard]`）。
pub fn relay_root() -> PathBuf {
    repo_src_root().join("comms/outward")
}

/// 中转的单测树。
pub fn comms_tests_root() -> PathBuf {
    tests_root().join("comms/outward")
}

/// 后端里中转的宿主（绑口 · 钥匙 · 起中转）。
pub fn relay_host_root() -> PathBuf {
    backend_src_root().join("relay")
}

/// **全体后端代码**的几棵树：生产 ＋ 后端测试 ＋ 中转的测试，互不包含。
/// 只用 [`backend_src_root`] 会安静地少一块人群。
pub fn backend_code_roots() -> [PathBuf; 3] {
    [backend_src_root(), backend_tests_root(), comms_tests_root()]
}

/// monitor 包（`src/frontend/shell`）。
pub fn shell_root() -> PathBuf {
    repo_src_root().join("frontend/shell")
}

/// monitor 自己的源码树。与 [`repo_src_root`] 不是一件事。
/// 拿它当根交给本 crate 扫的时候，人群还含 monitor manifest 明写的兄弟源码树（[`crate::population_trees`]）。
pub fn shell_src_root() -> PathBuf {
    shell_root().join("src")
}

/// monitor 源码人群的全部根：自己的 `src/` ＋ manifest 明写的兄弟包。
/// 自己走目录的判据按它逐棵走 —— 与经遍历器的那几条同一群，声明只有 manifest 那一处。
pub fn shell_population_roots() -> Vec<PathBuf> {
    std::iter::once(shell_src_root())
        .chain(
            crate::population_trees(&shell_src_root())
                .into_iter()
                .map(|(_, src)| src),
        )
        .collect()
}

/// 文件窗口包（`src/frontend/filewin`）。
pub fn filewin_root() -> PathBuf {
    repo_src_root().join("frontend/filewin")
}

/// 文件窗口包的源码树。
pub fn filewin_src_root() -> PathBuf {
    filewin_root().join("src")
}

/// 一份文件相对仓根的路径（正斜杠）。
pub fn rel_of(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 一份文件的生产段 —— 按它住在哪棵树分流：住测试树（[`backend_tests_root`] · [`comms_tests_root`]，
/// 整份是测试段、外面没有 `#[cfg(test)]` 包着）⇒ 空串；别处 ⇒ [`crate::production_code`]。
/// 测试树里的文件仍要收进人群（贡献 0 字节）：按文件数对账的判据要它们。判定按词法规范化后的路径前缀比。
pub fn production_side_of(path: &Path, src: &str) -> String {
    let at = crate::lexically_normalized(path);
    if [backend_tests_root(), comms_tests_root()]
        .iter()
        .any(|t| at.starts_with(t))
    {
        return String::new();
    }
    crate::production_code(src)
}

/// git 跟踪着的、住在 `dir`（仓根相对）下且后缀是 `ext` 的文件：仓根相对、正斜杠、排好序；工作树里已删的不算。
/// 与走文件系统的遍历不同源，「扫到的 ⊇ 这里列的」是一条两侧不同源的对拍。
pub fn tracked_under(dir: &str, ext: &str) -> Vec<String> {
    let root = repo_root();
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z", "--", dir])
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("起不来 `git ls-files`（{e}）—— 采集面对拍的另一侧拿不到"));
    assert!(
        out.status.success(),
        "`git ls-files -- {dir}` 非零退出：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|rel| {
            !rel.is_empty()
                && Path::new(rel).extension().is_some_and(|e| e == ext)
                && root.join(rel).is_file()
        })
        .map(str::to_string)
        .collect();
    v.sort();
    v
}

/// 采集面对拍：`dirs` 下 git 跟踪着的每一份 `ext` 文件都得在 `seen`（仓根相对、正斜杠）里，`skip` 里的除外。
/// 漏了 ⇒ panic，点名漏的是哪几份（路径拼错 · 后缀过滤掉 · 静默跳过子目录都在这里红）。
pub fn assert_scanned_every_tracked(
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

/// 一棵目录下的每一份 `.rs`：`(仓根相对路径, 生产段)`，按路径排；一份都没有 ⇒ 红。
fn production_files(dir: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = crate::scan_tree_excluding(dir, &["rs"], &[])
        .into_iter()
        .map(|(path, src)| (rel_of(&path), crate::production_code(&src)))
        .collect();
    out.sort();
    assert!(
        !out.is_empty(),
        "{} 下一份 `.rs` 都没扫到 —— 住址坏了，用它的判据会在空人群上恒绿",
        dir.display()
    );
    out
}

/// 后端入方向目录（`stream/inbound/`，含命令表各族）下的每一份：`(仓根相对路径, 生产段)`。只看其中一份会安静地少一块人群。
pub fn backend_inbound_sources() -> Vec<(String, String)> {
    production_files(&backend_src_root().join("stream/inbound"))
}

/// 后端命令表各族（`stream/inbound/registry/*.rs`），一族一份。
///
/// 按 `CommandSpec {` 切块的判据要**逐份切**，别把几份拼起来再切：拼起来的话，
/// 一份的文件头（`use` · 常量）会粘到上一份的最后一块上，被算成那条命令的一部分。
pub fn backend_registry_sources() -> Vec<(String, String)> {
    production_files(&backend_src_root().join("stream/inbound/registry"))
}

/// 后端的根源码面：`lib.rs`（模块声明、身份、`SUBCOMMANDS` 这一族）＋ `main.rs`（分派），两份拼起来的全集。
/// 要「只看分派」的判据别用它。
pub fn backend_root_source() -> String {
    ["lib.rs", "main.rs"]
        .iter()
        .map(|f| {
            let p = backend_src_root().join(f);
            std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// monitor 会话流来源目录（`stream_source/`）下的每一份：`(仓根相对路径, 原文)`，按路径排。
/// 那份东西从一份文件拆成了一个目录：只看其中一份会安静地少一块人群。
pub fn stream_source_files() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> =
        crate::scan_tree_excluding(&shell_src_root().join("stream_source"), &["rs"], &[])
            .into_iter()
            .map(|(path, src)| (rel_of(&path), src))
            .collect();
    out.sort();
    assert!(
        out.iter()
            .any(|(rel, _)| rel == "src/frontend/shell/src/stream_source/mod.rs"),
        "会话流来源目录里没扫到 `mod.rs` —— 住址坏了，用它的判据会在空人群上恒绿"
    );
    out
}

/// [`stream_source_files`] 各份的**生产段**按路径顺序接成一段。要逐份报住址的用 [`stream_source_files`]。
pub fn stream_source_production() -> String {
    stream_source_files()
        .iter()
        .map(|(_, src)| crate::production_code(src))
        .collect::<Vec<_>>()
        .join("\n")
}

/// [`stream_source_files`] 各份的**原文**（不剥测试段）按路径顺序接成一段。
pub fn stream_source_raw() -> String {
    stream_source_files()
        .into_iter()
        .map(|(_, src)| src)
        .collect::<Vec<_>>()
        .join("\n")
}

/// 会话流来源目录里**一份**文件的原文（`name` 是目录内文件名，如 `run.rs`）。
pub fn stream_source_file(name: &str) -> String {
    let rel = format!("src/frontend/shell/src/stream_source/{name}");
    stream_source_files()
        .into_iter()
        .find(|(r, _)| *r == rel)
        .unwrap_or_else(|| panic!("会话流来源目录里没有 `{name}` —— 住址坏了"))
        .1
}

/// 第 `i` 行（0 起）所在的那一项的名字：往上找最近一处包着它的 `fn` / `struct` / `enum` / `const` / `static` / `trait`
///（Rust 源码；按缩进与收口的 `}` 认外层，粗，但归错会红在名字对不上上）。找不到 ⇒ `"<顶层>"`。
/// 逐处登记按「文件 × 外层函数」认，不按处数认。
pub fn enclosing_fn(lines: &[&str], i: usize) -> String {
    let indent = |l: &str| l.len() - l.trim_start().len();
    let Some(here) = lines.get(i) else {
        return "<顶层>".to_string();
    };
    let fn_name = |l: &str| -> Option<String> {
        let t = crate::strip_visibility(l.trim_start());
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

#[cfg(test)]
#[path = "../../../../tests/common/guard-core/addr_tests.rs"]
mod tests;
