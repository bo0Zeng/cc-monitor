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
// 「复制为」：根 ＝ 当前目录，源 ＝ 那一行的名字，目标 ＝ 框里的名字原样（那台判）
// ════════════════════════════════════════════════════════════════════════

/// 线上那一条逐字（期望手写）：名字去两头空白、别的一个字不改（带 `/` 的也原样交出去），带 `single: true`。
#[test]
fn the_copy_box_sends_the_typed_name_as_is_with_single() {
    assert_eq!(CopyPrompt::suggest("big.bin"), "big.bin.copy");
    let mut p = CopyPrompt::for_row("/srv/data", &row("big.bin", false, false));
    assert_eq!(p.from, "/srv/data/big.bin");
    assert_eq!(p.new_name, "big.bin.copy");
    let wire = |p: &CopyPrompt| serde_json::to_value(p.to_req()).unwrap();
    assert_eq!(
        wire(&p),
        serde_json::json!({ "root": "/srv/data", "items": [{ "from": "big.bin", "to": "big.bin.copy" }], "single": true })
    );
    p.new_name = "  ../x  ".into();
    assert_eq!(
        wire(&p),
        serde_json::json!({ "root": "/srv/data", "items": [{ "from": "big.bin", "to": "../x" }], "single": true })
    );
    // 有损名：根与源按字节发。
    p.dir_raw = Some(b"/srv/d\xe9".to_vec());
    p.from_raw = Some(b"/srv/d\xe9/b\xe9".to_vec());
    p.new_name = "c".into();
    assert_eq!(
        wire(&p),
        serde_json::json!({ "root": { "b16": "2f7372762f64e9" }, "items": [{ "from": { "b16": "62e9" }, "to": "c" }], "single": true })
    );
}

/// 能复制的是「名字寻址得到」的那一档 —— 文件与**目录**都行（目录经后端 `recursive: true` 复制整棵，
/// 要求：「复制目录」· `§7 #6`）。**相等断言，逐档。**
#[test]
fn files_and_directories_with_addressable_names_can_be_copied() {
    assert!(is_copyable(&row("big.bin", false, false)));
    assert!(
        is_copyable(&row("adir", true, false)),
        "目录复制不了 —— 后端 `files-copy` 已经收 `recursive: true`"
    );
    assert!(
        !is_copyable(&row("\u{FFFD}odd", false, true)),
        "有损名能复制 —— 那个名字寻址不到真字节，写操作一律灰置"
    );
}
