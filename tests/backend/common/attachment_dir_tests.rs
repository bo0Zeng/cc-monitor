//! 附件落点的形状：编号怎么写、怎么认，一行引用怎么认回（编号, 原名）。

use super::*;

#[test]
fn an_id_is_month_day_hour_minute_and_four_hex() {
    let id = id_of(10, 3, 9, 5, 0xa3f2);
    assert_eq!(id, "1003-0905-a3f2");
    assert_eq!(id.len(), ID_LEN);
    assert!(is_id(&id));
}

#[test]
fn only_the_id_shape_is_an_id() {
    for bad in [
        "",
        "1003-0905-A3F2",
        "1003-0905-a3f",
        "1003_0905-a3f2",
        "10030905-a3f2x",
        "1003-0905-a3g2",
        ".gitignore",
    ] {
        assert!(!is_id(bad), "{bad:?} 不该认成编号");
    }
}

#[test]
fn a_reference_reads_back_to_id_and_name_relative_or_absolute() {
    let rel = rel_of("1023-1529-7c01", "login.css");
    assert_eq!(rel, ".ccm-attach/1023-1529-7c01/login.css");
    assert_eq!(parse_rel(&rel), Some(("1023-1529-7c01", "login.css")));
    assert_eq!(
        parse_rel("/home/u/src/w/.ccm-attach/1023-1529-a3f2/login narrow.png"),
        Some(("1023-1529-a3f2", "login narrow.png"))
    );
    for bad in [
        ".ccm-attach/1023-1529-7c01",
        ".ccm-attach/1023-1529-7c01/",
        ".ccm-attach/1023-1529-7c01/a/b",
        ".ccm-attach/x/login.css",
        "src/.ccm-attachx/1023-1529-7c01/a",
        "login.css",
    ] {
        assert_eq!(parse_rel(bad), None, "{bad:?}");
    }
}

/// 按编号找回那一份的原名：在 ⇒ 名字；编号目录不在 · 编号不合形 ⇒ `None`。
#[test]
fn find_reads_back_the_one_file_in_a_numbered_dir() {
    let d = std::env::temp_dir().join(format!("ccm-attdir-find-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(d.join(".ccm-attach/1023-1529-7c01")).expect("建");
    std::fs::write(d.join(".ccm-attach/1023-1529-7c01/login.css"), b"x").expect("写");
    assert_eq!(find(&d, "1023-1529-7c01").as_deref(), Some("login.css"));
    assert_eq!(find(&d, "1023-1529-7c02"), None);
    assert_eq!(find(&d, "../1023-1529-7c01"), None);
    let _ = std::fs::remove_dir_all(&d);
}
