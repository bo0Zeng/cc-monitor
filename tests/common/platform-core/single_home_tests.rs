//! **这几样平台原语只在本包有实现**：后端 · 壳 · 共享 crate 的生产段里，各自那一份的指纹零处，本包里在。
//!
//! 每一格是「一样东西 · 它的指纹（剥过注释与测试段后数的字面子串）· 例外（另一件事、碰巧长得像）」。
//! 指纹选的是**每一份实现都绕不开的那一笔**：互斥量名的前缀 · 阻塞的 `LOCK_EX)` · 建目录时置权限位的那个 trait ·
//! 切 `/proc/<pid>/stat` 时找最后一个右括号 · ToolHelp 快照 · `CREATE_NO_WINDOW` 的位 · `ReplaceFileW` ·
//! 原子写那一段「旁名 → 换名」的换名。
//!
//! 买不到：换个写法重写一遍（不经这些字面）这里看不见；只扫 `src/backend` · `src/frontend/shell/src` · `src/frontend/filewin/src` ·
//! `src/common/*/src` · `src/comms`。

use std::path::{Path, PathBuf};

/// (一样东西, 指纹, 例外：仓根相对路径的前缀 —— 那几份里是另一件事)。
const HOMES: &[(&str, &str, &[&str])] = &[
    ("目录锁：互斥量名", "ccm-own-state-", &[]),
    // `own-chan` 那一把是 `LOCK_EX | LOCK_NB`（抢门、不等），不在这里。
    ("目录锁：阻塞拿", "LOCK_EX)", &[]),
    ("目录锁：阻塞等互斥量", "WAIT_ABANDONED", &[]),
    ("只给本人的目录", "DirBuilderExt", &[]),
    ("/proc/<pid>/stat 怎么切", "rfind(')')", &[]),
    ("Windows 进程表", "CreateToolhelp32Snapshot", &[]),
    ("CREATE_NO_WINDOW", "0x0800_0000", &[]),
    ("原子写：Windows 保目标 ACL 的换法", "ReplaceFileW", &[]),
];

/// 自家状态的三个写口：各自只留「起旁名 · 交正文」，「建旁名 → 写满 → 落盘 → 换上去 → 失败删旁名」那一段改调本包。
/// (文件, 必须出现的调用, 不许再出现的那几笔)。
const ATOMIC_WRITERS: &[(&str, &str, &[&str])] = &[
    (
        "src/common/host-core/src/atomic.rs",
        "platform_core::atomic::write_atomic(",
        &["rename(", "remove_file(", "create_new("],
    ),
    (
        "src/frontend/shell/src/platform/own_fs.rs",
        "platform_core::atomic::write_atomic(",
        &["fn write_atomic("],
    ),
    (
        "src/backend/platform/fs/own.rs",
        "platform_core::atomic::write_atomic(",
        &["std::fs::rename(&tmp", "f.sync_all()"],
    ),
];

/// 只扫这几棵（仓根相对）。
const TREES: &[&str] = &[
    "src/backend",
    "src/frontend/shell/src",
    "src/frontend/filewin/src",
    "src/common",
    "src/comms",
];

/// 本包的源码树（仓根相对）。
const HOME: &str = "src/common/platform-core/src";

fn root() -> PathBuf {
    guard_core::addr::repo_root()
}

fn rel(p: &Path) -> String {
    p.strip_prefix(root())
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 生产段 `.rs`（剥注释与测试段）：(仓根相对路径, 正文)。
fn production() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for t in TREES {
        for (p, src) in guard_core::scan_tree!(&root().join(t), &["rs"]) {
            let r = rel(&p);
            if r.contains("/tests/") || r.starts_with("tests/") || r.contains("/target/") {
                continue;
            }
            out.push((r, guard_core::production_code(&src)));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

#[test]
fn each_primitive_has_its_only_home_in_platform_core() {
    let files = production();
    assert!(files.len() >= 300, "只扫到 {} 份 —— 遍历坏了", files.len());
    let mut stray = Vec::new();
    for (what, needle, except) in HOMES {
        let home_hits: usize = files
            .iter()
            .filter(|(r, _)| r.starts_with(HOME))
            .map(|(_, s)| s.matches(needle).count())
            .sum();
        assert!(
            home_hits > 0,
            "{what}：本包里一处 `{needle}` 都没有 —— 指纹选错了，或本包丢了这一样"
        );
        for (r, s) in &files {
            if r.starts_with(HOME) || except.iter().any(|e| r.starts_with(e)) {
                continue;
            }
            let n = s.matches(needle).count();
            if n > 0 {
                stray.push(format!("{what}：{r} 里 `{needle}` × {n}"));
            }
        }
    }
    assert!(
        stray.is_empty(),
        "这几样在 `platform-core` 之外还有一份（改调 `platform_core`，那一层只留造那一句）：\n{}",
        stray.join("\n")
    );
}

#[test]
fn the_three_own_state_writers_go_through_one_atomic_write() {
    let mut bad = Vec::new();
    for (file, must, never) in ATOMIC_WRITERS {
        let src = std::fs::read_to_string(root().join(file))
            .unwrap_or_else(|e| panic!("读 {file}: {e}"));
        let code = guard_core::production_code(&src);
        if !code.contains(must) {
            bad.push(format!("{file}：没调 `{must}`"));
        }
        for n in *never {
            if code.contains(n) {
                bad.push(format!("{file}：还有自己那一段 `{n}`"));
            }
        }
    }
    assert!(bad.is_empty(), "自家状态的原子写还有不止一份：\n{}", bad.join("\n"));
}

/// 正控：同一把尺子在合成语料里认得出多出来的一份。
#[test]
fn the_scan_sees_a_planted_copy() {
    let planted = "fn x() { let close = line.rfind(')')?; }";
    let code = guard_core::production_code(planted);
    assert_eq!(code.matches("rfind(')')").count(), 1);
}
