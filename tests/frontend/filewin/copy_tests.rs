use super::*;

fn row(name: &str, is_dir: bool, lossy: bool) -> Row {
    Row {
        name: name.to_string(),
        path: format!("/srv/data/{name}"),
        is_dir,
        size: 7,
        lossy_name: lossy,
    }
}

// ════════════════════════════════════════════════════════════════════════
// 名字：**只在同一个目录里改名**
// ════════════════════════════════════════════════════════════════════════

#[test]
fn a_copy_job_is_a_rename_inside_the_same_directory() {
    let got = CopyJob::beside("/srv/data/big.bin", "/srv/data", "big.bin.copy").unwrap();
    assert_eq!(got.to, "/srv/data/big.bin.copy");
    assert_eq!(got.name, "big.bin.copy");
    assert_eq!(got.from, "/srv/data/big.bin");
    // 目录结尾已经有斜杠也不许拼出 `//`。
    assert_eq!(
        CopyJob::beside("/srv/data/big.bin", "/srv/data/", "x")
            .unwrap()
            .to,
        "/srv/data/x"
    );
    // 根目录。
    assert_eq!(CopyJob::beside("/big.bin", "/", "x").unwrap().to, "/x");
    // 前后空白剪掉（框里很容易多敲一个空格）。
    assert_eq!(
        CopyJob::beside("/srv/a", "/srv", "  b  ").unwrap().name,
        "b"
    );

    // 三档 `None`，**一个都不许兜底**：
    assert!(
        CopyJob::beside("/srv/a", "/srv", "   ").is_none(),
        "空名字过了"
    );
    assert!(
        CopyJob::beside("/srv/a", "/srv", "../../etc/x").is_none(),
        "名字里带 `/` 过了 —— 那会把文件放到他没在看的目录里"
    );
    assert!(
        CopyJob::beside("/srv/a", "/srv", "a").is_none(),
        "复制成自己过了 —— 覆盖那一形是拿一份复制品顶掉目标，这里就是顶掉源自己"
    );
}

/// 缺省名与旧面板同一套（`panel.ts::copyFile` 的 `${e.name}.copy`）。
#[test]
fn the_suggested_name_matches_the_old_panel() {
    assert_eq!(CopyPrompt::suggest("big.bin"), "big.bin.copy");
    let p = CopyPrompt::for_row("/srv/data", &row("big.bin", false, false));
    assert_eq!(p.from, "/srv/data/big.bin");
    assert_eq!(p.dir, "/srv/data");
    assert_eq!(p.new_name, "big.bin.copy");
    assert_eq!(p.to_job().unwrap().to, "/srv/data/big.bin.copy");
}
