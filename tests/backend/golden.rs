//! 金样判据只比不写：现打的与仓里那份逐字相等才绿；不等 ⇒ 现打的那份落到本树 `.build/golden/<仓相对路径>`、红。
//! 重打金样只有一条命令：人对过现打的那份后跑 `bash tests/scripts/bless-golden.sh`，它把 `.build/golden/` 下的逐份拷回仓里。

use guard_core::addr::FRESH_GOLDEN_DIR;
use std::path::Path;

/// `rel`：金样的仓相对路径；`what`：红时说的那句（哪个读者在读这份、改了要连带改什么）。
#[track_caller]
pub fn assert_golden(rel: &str, got: &str, what: &str) {
    if let Some(e) = mismatch(rel, got) {
        panic!("{what}\n{e}");
    }
}

/// 一次比好几份时用：每份都比、都落现打的那份，再一起红。
pub fn mismatch(rel: &str, got: &str) -> Option<String> {
    check(&guard_core::addr::repo_root(), rel, got).err()
}

fn check(root: &Path, rel: &str, got: &str) -> Result<(), String> {
    let want = std::fs::read_to_string(root.join(rel)).unwrap_or_default();
    let fresh = guard_core::addr::fresh_golden(root, rel);
    if got == want {
        let _ = std::fs::remove_file(&fresh);
        return Ok(());
    }
    let line = want
        .lines()
        .zip(got.lines())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| want.lines().count().min(got.lines().count()))
        + 1;
    let landed = fresh
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&fresh, got));
    Err(match landed {
        Ok(()) => format!(
            "{rel} 与现打的不一致（第 {line} 行起）。现打的在 {FRESH_GOLDEN_DIR}/{rel}；对过后跑 bash tests/scripts/bless-golden.sh 拷回"
        ),
        Err(e) => format!("{rel} 与现打的不一致（第 {line} 行起）；现打的那份落不了盘（{e}）"),
    })
}

/// 不等时金样原样不动、现打的落进 `.build/golden/`；相等时绿，并清掉上一趟留下的现打那份。
#[test]
fn a_golden_is_compared_never_written() {
    let root = std::env::temp_dir().join(format!("ccm-golden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("g")).unwrap();
    std::fs::write(root.join("g/a.json"), "1\n2\n").unwrap();
    let fresh = guard_core::addr::fresh_golden(&root, "g/a.json");

    let red = check(&root, "g/a.json", "1\n3\n").expect_err("不等就红");
    assert!(red.contains("第 2 行起"), "{red}");
    assert_eq!(
        std::fs::read_to_string(root.join("g/a.json")).unwrap(),
        "1\n2\n"
    );
    assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "1\n3\n");

    assert_eq!(check(&root, "g/a.json", "1\n2\n"), Ok(()));
    assert!(!fresh.exists(), "相等时上一趟现打的那份要清掉");
    let _ = std::fs::remove_dir_all(&root);
}

/// 一段 Rust 源码里有没有以那个目录起头的字符串字面量（整个目录 · 目录再接下一层），
/// 或拆成 `.build` 与 `golden` 两段 join。说明文字里提到它（引号不紧挨着）不算。
fn writes_fresh_dir(src: &str) -> bool {
    let head = concat!("\"", ".build/", "golden");
    let split = concat!("\"", ".build\")", ".join(\"", "golden\"");
    src.contains(split)
        || src
            .match_indices(head)
            .any(|(i, _)| matches!(src[i + head.len()..].chars().next(), Some('"' | '/')))
}

/// 现打的金样落在哪只住一处（`guard_core::addr`）：仓里 git 跟踪着的每一份 Rust 文件，除了那一份，
/// 都不以字面量写那个目录（各金样判据经 `guard_core::addr::fresh_golden` 取落点）。
/// 正控：住址那一份自己写着它（不然本条在空转）；认法自己的正反例。手机 Kotlin 那一侧不在本条人群里。
#[test]
fn the_fresh_golden_dir_is_written_in_one_place() {
    for (src, want) in [
        (concat!("r.join(\"", ".build/", "golden", "\")"), true),
        (
            concat!("r.join(\"", ".build/", "golden/", "\").join(rel)"),
            true,
        ),
        (
            concat!("r.join(\"", ".build\")", ".join(\"", "golden\")"),
            true,
        ),
        (concat!("\"现打的在 ", ".build/", "golden/{rel}\""), false),
        (concat!("\"", ".build/", "golden-missing/x.jsonl\""), false),
    ] {
        assert_eq!(writes_fresh_dir(src), want, "认法：{src}");
    }
    let home = "src/common/guard-core/src/addr.rs";
    let root = guard_core::addr::repo_root();
    let mut files = guard_core::addr::tracked_under("src", "rs");
    files.extend(guard_core::addr::tracked_under("tests", "rs"));
    assert!(
        writes_fresh_dir(&std::fs::read_to_string(root.join(home)).unwrap_or_default()),
        "{home} 里没有那个目录的字面量 —— 本条在空转"
    );
    let off: Vec<&String> = files
        .iter()
        .filter(|f| f.as_str() != home)
        .filter(|f| writes_fresh_dir(&std::fs::read_to_string(root.join(f)).unwrap_or_default()))
        .collect();
    assert!(
        off.is_empty(),
        "这几份自己写着现打金样的落点（改用 guard_core::addr::fresh_golden）：{off:?}"
    );
}
