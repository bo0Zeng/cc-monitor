//! 进程起来那一下（`stream/entry.rs::run`）：拨号层自己读这一端的机器表（`dial::machines_now`，起来时不用谁先装一份读法）；
//! 常驻起来那一段也钉在这里。

const ENTRY: &str = include_str!("../../src/backend/stream/entry.rs");

/// 家里放一份机器表、什么都不装 ⇒ 拨号层照它读到那一台（各台的常驻流 · 问那台 · 传输 · 部署都按名字查它）。
#[test]
fn the_machine_table_is_read_without_anything_installed() {
    let home = crate::platform::paths::home_dir().expect("测试档的家（沙箱）");
    let file = home.join(relay_route_core::MACHINES_REL);
    std::fs::create_dir_all(file.parent().expect("机器表在一个目录里")).unwrap();
    std::fs::write(
        &file,
        r#"{"version": 1, "hosts": [{"label": "startup-probe", "host": "probe.lan", "user": "pi"}]}"#,
    )
    .unwrap();
    let names: Vec<String> = crate::dial::machines_now()
        .iter()
        .map(|h| h.name().to_string())
        .collect();
    let _ = std::fs::remove_file(&file);
    assert_eq!(names, vec!["startup-probe".to_string()]);
}

/// 常驻那一位起来那一段（各台常驻流起来之前那几步）：不调换形（落盘文件的旧版经声明上挂的升级步读，`common/own_state.rs`）。
#[test]
fn the_resident_startup_runs_no_reshape() {
    let feeds = ENTRY
        .find("control::feeds::start(")
        .expect("找不到各台常驻流起来那一步 —— 本条要跟着改");
    let at = ENTRY[..feeds]
        .rfind("if resident {")
        .expect("找不到常驻那一段 —— 本条要跟着改");
    let calls: Vec<&str> = ENTRY[at..feeds]
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect();
    assert!(calls.is_empty(), "常驻起来那一段多调了别的：{calls:?}");
}
