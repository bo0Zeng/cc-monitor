//! 进程起来那一下（`stream/entry.rs::run`）：拨号层要的「这一端的机器表怎么读」装上了（不装 ⇒ 生产上各台的常驻流 · 问那台 · 传输 · 部署按名字查不到机器）。
//! 装的那一处在 `run` 打头、分流之前（`ccm` 那一趟 · 一次性子命令 · 常驻都走得到）；常驻起来那一段也钉在这里。

const ENTRY: &str = include_str!("../../src/backend/stream/entry.rs");

/// `run` 的函数体：从 `pub async fn run() {` 到分流那一步（`control::ccm::route`）之前。
fn run_head() -> &'static str {
    let at = ENTRY
        .find("pub async fn run() {")
        .expect("找不到 `run` —— 本条要跟着改");
    let body = &ENTRY[at..];
    let end = body
        .find("control::ccm::route")
        .expect("找不到分流那一步 —— 本条要跟着改");
    &body[..end]
}

#[test]
fn the_machine_table_is_installed_before_anything_is_dispatched() {
    let head = run_head();
    let calls: Vec<&str> = head
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//"))
        .filter(|l| l.contains("install_machines()"))
        .collect();
    assert_eq!(
        calls,
        vec!["faces::install_machines();"],
        "`run` 分流之前恰好装一次机器表（`faces::install_machines`）"
    );
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
