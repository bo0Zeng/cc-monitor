//! 判据：仓里每个有判据的 Rust crate 都把 `src/common/test-home` 引进它的测试二进制（dev 依赖 ＋ crate 根上那一行
//! `static TEST_HOME: fn() = test_home::linked;`）。漏了一个，那个 crate 的判据就照这台用户的真家跑（10-11 那一次就是这样改了真家的
//! `rotation.json`）。另判换不换家那一条规矩本身。

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    crate::guard_support::src_root().join("../..")
}

/// 一个 crate 的根文件（`[lib] path` 写了就照它，没写 ⇒ `src/lib.rs`）。
fn root_file(dir: &Path, toml: &str) -> PathBuf {
    let lib = toml
        .split("\n[lib]")
        .nth(1)
        .and_then(|s| s.split("\n[").next())
        .and_then(|s| s.lines().find(|l| l.trim_start().starts_with("path")))
        .and_then(|l| l.split('"').nth(1))
        .unwrap_or("src/lib.rs");
    dir.join(lib)
}

/// 这一份源码里有判据：写在本文件里（`#[test]`），或用 `#[path]` 挂在仓里 `tests/` 下（`mod tests;` · `…_tests.rs`）。
fn has_test_marks(t: &str) -> bool {
    t.contains("#[test]")
        || t.contains("::test]")
        || t.contains("_tests.rs\"")
        || t.contains("mod tests;")
}

/// 仓里的 Rust crate（`src/` 下每份 `Cargo.toml`，`src/vendor` · 手机那一套 · `test-home` 自己除外）：
/// （crate 目录, 有没有判据, 引没引）。后端的判据住 `tests/backend/`，按后端算。
fn crates() -> Vec<(PathBuf, bool, bool)> {
    let src = repo_root().join("src");
    let files = guard_core::scan_tree_excluding(&src, &["toml", "rs"], &[]);
    let skip = |p: &Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        [
            "/vendor/",
            "/mobile/",
            "/target/",
            "/node_modules/",
            "/common/test-home/",
        ]
        .iter()
        .any(|x| s.contains(x))
    };
    let dirs: Vec<(PathBuf, String)> = files
        .iter()
        .filter(|(p, _)| p.file_name().is_some_and(|n| n == "Cargo.toml") && !skip(p))
        .filter_map(|(p, t)| Some((p.parent()?.to_path_buf(), t.clone())))
        .collect();
    let owner = |rs: &Path| {
        dirs.iter()
            .filter(|(d, _)| rs.starts_with(d))
            .max_by_key(|(d, _)| d.components().count())
            .map(|(d, _)| d.clone())
    };
    let mut out: Vec<(PathBuf, bool, bool)> = dirs
        .iter()
        .map(|(d, t)| {
            let tests = d.ends_with("src/backend")
                || files.iter().any(|(p, src)| {
                    p.extension().is_some_and(|x| x == "rs")
                        && owner(p).as_deref() == Some(d.as_path())
                        && has_test_marks(src)
                });
            let dev = t
                .split("\n[dev-dependencies]")
                .nth(1)
                .is_some_and(|s| s.split("\n[").next().unwrap_or("").contains("test-home ="));
            let root = root_file(d, t);
            let linked = files.iter().any(|(p, src)| {
                *p == root && src.contains("static TEST_HOME: fn() = test_home::linked;")
            });
            (d.clone(), tests, dev && linked)
        })
        .collect();
    out.sort();
    out
}

/// ★ 每个有判据的 Rust crate 都引了 `test-home`（dev 依赖 ＋ 根上那一行）。正控：人群不空，后端 · monitor · 文件窗口都在里面。
#[test]
fn every_rust_test_binary_pulls_in_the_sandboxed_home() {
    let all = crates();
    let names: Vec<String> = all
        .iter()
        .map(|(p, _, _)| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        })
        .collect();
    for want in ["backend", "shell", "filewin", "copy-core", "outward"] {
        assert!(
            names.iter().any(|n| n == want),
            "人群里没有 {want}：{names:?}"
        );
    }
    let missing: Vec<String> = all
        .iter()
        .filter(|(_, tests, ok)| *tests && !ok)
        .map(|(p, _, _)| p.display().to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "这几个 crate 有判据却没把 `test-home` 引进测试二进制（Cargo.toml 的 [dev-dependencies] 加 `test-home = {{ path = … }}`，\
         crate 根上加 `#[cfg(test)] #[used] static TEST_HOME: fn() = test_home::linked;`）：\n  {}",
        missing.join("\n  ")
    );
}

/// 换不换家：`HOME` 是真家（或没设）⇒ 换；判据自己给的家照用；查不到真家 ⇒ 一律换。
#[test]
fn only_the_real_home_is_moved() {
    let real = Path::new("/home/someone");
    assert!(test_home::must_move(Some(real), Some(real)));
    assert!(test_home::must_move(None, Some(real)));
    assert!(!test_home::must_move(
        Some(Path::new("/tmp/x/home")),
        Some(real)
    ));
    assert!(test_home::must_move(Some(real), None));
}

/// 外面那个会话漏进来、子测试进程里要看见「都不在」的那几样（名字 ＝ 值随便；真值是什么不要紧，要紧的是进不来）。
const LEAKS: &[&str] = &[
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_CODE_PLUGIN_DIRS",
    "TMUX",
    "TMUX_PANE",
    "CCM_RELAY_PORT",
    "CCM_DATA_DIR",
    "CCM_RESIDENT",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_KEY",
];
/// 台架 / 人交给判据的输入：照留（反控：清单没把 `CCM_` 一族一刀切）。
const KEPT: &[&str] = &["CCM_E2E_WORK", "CCM_SHOTS_NOW_MS", "CCM_SANDBOX"];
const CHILD: &str = "CCM_E2E_TEST_HOME_CHILD";

/// 子进程那一半：只在下面那条判据带着一身外面会话的变量把本测试二进制再拉起来时跑。
#[test]
#[ignore = "子进程入口：只在 a_test_process_never_sees_the_outer_session 拉起时跑"]
fn child_sees_no_outer_session() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let left: Vec<&str> = LEAKS
        .iter()
        .copied()
        .filter(|k| std::env::var_os(k).is_some())
        .collect();
    assert!(left.is_empty(), "这几样从外面漏进了测试进程：{left:?}");
    for k in KEPT {
        assert!(std::env::var_os(k).is_some(), "{k} 是判据的输入，不该摘");
    }
}

/// ★ 测试进程里没有外面会话的状态：带着 `CLAUDE_CONFIG_DIR` · `TMUX` · `TMUX_PANE` · `CCM_RELAY_PORT` · `CCM_*` · `ANTHROPIC_*`
/// 把本测试二进制当一次新进程拉起（摘掉换家标记、`HOME` 给真家，像在会话里直接敲 `cargo test` 那样），子进程里一样都看不见；
/// 判据的输入照留。
#[test]
fn a_test_process_never_sees_the_outer_session() {
    let exe = std::env::current_exe().expect("拿不到测试二进制自己的路径");
    let mut cmd = std::process::Command::new(exe);
    cmd.args([
        "--exact",
        "test_home_guard::child_sees_no_outer_session",
        "--ignored",
        "--test-threads=1",
    ])
    .env_remove(test_home::SANDBOX_MARK)
    .env(CHILD, "1");
    // 真家查得到 ⇒ 给真家（子进程起来那一刻就换走）；查不到（非 unix）⇒ 不给（没设一样换）。
    match test_home::account_home() {
        Some(h) => cmd.env("HOME", h),
        None => cmd.env_remove("HOME"),
    };
    for k in LEAKS.iter().chain(KEPT) {
        cmd.env(k, "outer-session");
    }
    let out = cmd.output().expect("起子测试进程");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success() && text.contains("1 passed"),
        "子测试进程里看见了外面会话的变量：\n{text}"
    );
}

/// 清单本身：摘的与留的各几样（同一份 `leaked`）。
#[test]
fn the_scrub_list_takes_session_state_and_keeps_test_inputs() {
    for k in LEAKS {
        assert!(test_home::leaked(k), "{k} 该摘");
    }
    for k in KEPT.iter().chain(&[
        "CCM_REGEN_PROTOCOL_DOC",
        "CCM_BLESS",
        "CCM_FILEWIN_PROC_CHILD",
        "HOME",
        "PATH",
    ]) {
        assert!(!test_home::leaked(k), "{k} 不该摘");
    }
}
