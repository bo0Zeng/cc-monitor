//! 金样判据只比不写：现打的与仓里那份逐字相等才绿；不等 ⇒ 现打的那份落到本树 `.build/golden/<仓相对路径>`、红。
//! 重打金样只有一条命令：人对过现打的那份后跑 `bash tests/scripts/bless-golden.sh`，它把 `.build/golden/` 下的逐份拷回仓里。

use std::path::Path;

/// 现打的金样落在仓根下的这个目录（`bless-golden.sh` 从这里拷回）。
pub(crate) const FRESH_DIR: &str = ".build/golden";

/// `rel`：金样的仓相对路径；`what`：红时说的那句（哪个读者在读这份、改了要连带改什么）。
#[track_caller]
pub(crate) fn assert_golden(rel: &str, got: &str, what: &str) {
    if let Some(e) = mismatch(rel, got) {
        panic!("{what}\n{e}");
    }
}

/// 一次比好几份时用：每份都比、都落现打的那份，再一起红。
pub(crate) fn mismatch(rel: &str, got: &str) -> Option<String> {
    check(&guard_core::addr::repo_root(), rel, got).err()
}

fn check(root: &Path, rel: &str, got: &str) -> Result<(), String> {
    let want = std::fs::read_to_string(root.join(rel)).unwrap_or_default();
    let fresh = root.join(FRESH_DIR).join(rel);
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
            "{rel} 与现打的不一致（第 {line} 行起）。现打的在 {FRESH_DIR}/{rel}；对过后跑 bash tests/scripts/bless-golden.sh 拷回"
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
    let fresh = root.join(FRESH_DIR).join("g/a.json");

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
