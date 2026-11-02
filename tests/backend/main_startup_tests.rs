//! `main.rs` 起来那一下：拨号层要的「这一端的机器表怎么读」装上了（不装 ⇒ 生产上各台的常驻流 · 问那台 · 传输 · 部署按名字查不到机器）。
//! 装的那一处在 `main` 打头、分流之前（`ccm` 那一趟 · 一次性子命令 · 常驻都走得到）；hello 那一帧带的几样也钉在这里。

/// `main` 的函数体：从 `async fn main() {` 到分流那一步（`control::ccm::route`）之前。
fn main_head() -> &'static str {
    let src = include_str!("../../src/backend/main.rs");
    let at = src
        .find("async fn main() {")
        .expect("找不到 `main` —— 本条要跟着改");
    let body = &src[at..];
    let end = body
        .find("control::ccm::route")
        .expect("找不到分流那一步 —— 本条要跟着改");
    &body[..end]
}

#[test]
fn the_machine_table_is_installed_before_anything_is_dispatched() {
    let head = main_head();
    let calls: Vec<&str> = head
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//"))
        .filter(|l| l.contains("install_machines()"))
        .collect();
    assert_eq!(
        calls,
        vec!["faces::install_machines();"],
        "`main` 分流之前恰好装一次机器表（`faces::install_machines`）"
    );
}

/// 常驻那一位起来那一段（各台常驻流起来之前那几步）：不调换形（落盘文件的旧版经声明上挂的升级步读，`common/own_state.rs`）。
#[test]
fn the_resident_startup_runs_no_reshape() {
    let src = include_str!("../../src/backend/main.rs");
    let feeds = src
        .find("control::feeds::start(")
        .expect("找不到各台常驻流起来那一步 —— 本条要跟着改");
    let at = src[..feeds]
        .rfind("if resident {")
        .expect("找不到常驻那一段 —— 本条要跟着改");
    let calls: Vec<&str> = src[at..feeds]
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect();
    assert!(calls.is_empty(), "常驻起来那一段多调了别的：{calls:?}");
}
