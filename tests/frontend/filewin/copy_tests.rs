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

/// 「复制为」框里的缺省名：「副本」插在扩展名前面（`n1 副本.txt`），种类不变；没有扩展名 / 点打头的名字 / 目录接在末尾。
/// 期望手写（「副本」那两个字照文案表那一条拼，判据不手抄第二份；扩展名紧贴在后、没有空格那一条另手写一行）。
#[test]
fn the_copy_box_suggests_a_name_that_keeps_the_extension_last() {
    let named = |stem: &str, ext: &str| {
        format!(
            "{}{ext}",
            copy_text("rsFilewinCopy.suggest.name", &[("stem", stem)])
        )
    };
    // 期望手写：「副本」紧贴扩展名、中间没有空格。
    assert_eq!(CopyPrompt::suggest("n1.txt", false), "n1 副本.txt");
    let file = |n: &str| CopyPrompt::for_row("/srv/data", &row(n, false, false)).new_name;
    assert_eq!(file("n1.txt"), named("n1", ".txt"));
    assert_eq!(file("a.tar.gz"), named("a.tar", ".gz"));
    assert_eq!(file("Makefile"), named("Makefile", ""));
    assert_eq!(file(".bashrc"), named(".bashrc", ""));
    // 目录没有扩展名一说：带点的目录名也整个当名字。
    let dir = CopyPrompt::for_row("/srv/data", &row("v1.2", true, false)).new_name;
    assert_eq!(dir, named("v1.2", ""));
}

/// 线上那一条逐字（期望手写）：名字去两头空白、别的一个字不改（带 `/` 的也原样交出去），带 `single: true`。
#[test]
fn the_copy_box_sends_the_typed_name_as_is_with_single() {
    let mut p = CopyPrompt::for_row("/srv/data", &row("big.bin", false, false));
    assert_eq!(p.from, "/srv/data/big.bin");
    p.new_name = "big.bin.copy".into();
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
