//! 〔RM1a · 第四波〕`apikey_remote.rs` 的判据 —— 上游选择那份凭据文件按机器读写的 monitor 半。
//!
//! # 买到的
//!
//! - 🔴 本机那一臂**只进 `creds_store` / `history`**，一臂都不发帧（「每台机器上的写者恰好一个」
//!   在本机那台上的形状）；远端那一臂**只走发送口**、不碰本机那份文件。三个分派函数逐个量。
//! - 远端那一臂在「没有通道」「说不出账号 id」两种失败上**说得出话、话里不带明文**（真走生产函数）。
//! - 应答解析跟着后端**声明的字段**走：后端 `REGISTRY` 里 `apikey-read` 那一行的 `fields`
//!   （从后端源码现抠，不是本文件手抄）喂进解析器 ⇒ 解析得出；少任何一个必需字段 ⇒ 报错，
//!   **不退化成「没配」**。
//! - 命令名跨半边对拍：本侧两个常量 ⇔ 后端 `REGISTRY` 里的 `name:` 字面量。
//!
//! # 买不到的
//!
//! - 真远端那一趟（真 SSH、真后端进程写真文件）：本仓测试不起远端；后端那一半的行为判据住
//!   `tests/backend/accounts/upstream/file_face_tests.rs`，两半之间的线上字节没有金标准。

use super::*;

/// 夹具 key：结构样本，不是任何真 key。
const PLAIN: &str = "sk-FIXTURE-REMOTE-ARM-NOT-A-REAL-KEY";

fn own_production() -> String {
    guard_core::production_code(include_str!("../../src/bridge/src/apikey_remote.rs"))
}

/// 切出一个分派函数里 `Route::Local =>` 与 `Route::Remote(host) =>` 两臂各自那一行。
fn arms_of(src: &str, anchor: &str) -> (String, String) {
    let at = guard_core::find_pinned(src, anchor)
        .unwrap_or_else(|e| panic!("切不出 `{anchor}`（{e}）—— 按红处理"));
    let rest = &src[at..];
    let end = rest.find("\n}\n").expect("函数没有收尾");
    let body = &rest[..end];
    let line_of = |needle: &str| -> String {
        let hits: Vec<&str> = body.lines().filter(|l| l.contains(needle)).collect();
        assert_eq!(
            hits.len(),
            1,
            "`{anchor}` 里 `{needle}` 不是恰好一行：{hits:?}"
        );
        hits[0].to_string()
    };
    (
        line_of("Route::Local =>"),
        line_of("Route::Remote(host) =>"),
    )
}

#[test]
fn the_local_arm_never_sends_the_key_to_a_backend() {
    let src = own_production();
    let cases = [
        (
            "pub(crate) async fn write_key_on(",
            "crate::creds_store::write_key(",
        ),
        (
            "pub(crate) async fn status_on(",
            "crate::creds_store::read_status()",
        ),
        (
            "pub(crate) async fn rows_on(",
            "crate::history::inject_facts().rows",
        ),
    ];
    for (anchor, local_call) in cases {
        let (local, remote) = arms_of(&src, anchor);
        assert!(
            local.contains(local_call),
            "`{anchor}` 的本机那一臂不是 `{local_call}`：{local}"
        );
        for sends in ["call(", "send_key(", "inbound_client"] {
            assert!(
                !local.contains(sends),
                "`{anchor}` 的本机那一臂发了帧（`{sends}`）—— 本机那份只有 monitor 写：{local}"
            );
        }
        assert!(
            remote.contains("call(") || remote.contains("send_key("),
            "`{anchor}` 的远端那一臂没走发送口：{remote}"
        );
        assert!(
            !remote.contains("creds_store::") && !remote.contains("inject_facts()"),
            "`{anchor}` 的远端那一臂碰了本机那份文件：{remote}"
        );
    }
}

#[test]
fn the_remote_arm_says_what_went_wrong_without_the_plaintext() {
    // 一个一定没有通道的机器名（生产登记表里不会有它）。
    let host = Origin("rm1a-no-such-host-for-tests".to_string());
    let err = tauri::async_runtime::block_on(write_key_on(
        &host,
        "/home/u/.claude-accts/work",
        PLAIN.to_string(),
        Some("https://up.example.invalid".to_string()),
    ))
    .expect_err("没有通道还说写成了");
    assert!(
        err.contains("rm1a-no-such-host-for-tests"),
        "报错里说不出是哪台机器：{err}"
    );
    assert!(!err.contains(PLAIN), "报错里带着明文：{err}");

    // 说不出账号 id ⇒ 连通道都不问就拒，话里同样不带明文。
    let err = tauri::async_runtime::block_on(write_key_on(&host, "   ", PLAIN.to_string(), None))
        .expect_err("说不出账号 id 还说写成了");
    assert!(err.contains("说不出这是哪个账号"), "实得：{err}");
    assert!(!err.contains(PLAIN), "报错里带着明文：{err}");

    // 读口同样说得出话（不是静默当成「没配」）。
    let err = tauri::async_runtime::block_on(status_on(&host)).expect_err("没有通道还读到了状态");
    assert!(err.contains("rm1a-no-such-host-for-tests"), "实得：{err}");
    let err = tauri::async_runtime::block_on(rows_on(&host)).expect_err("没有通道还读到了行");
    assert!(err.contains("rm1a-no-such-host-for-tests"), "实得：{err}");
}

/// 从后端源码里现抠 `REGISTRY` 那一条的 `fields`（不是本文件手抄一份）。
fn backend_fields_of(cmd: &str) -> Vec<String> {
    let src = include_str!("../../src/backend/inbound.rs");
    let at = src
        .find(&format!("name: \"{cmd}\","))
        .unwrap_or_else(|| panic!("后端 REGISTRY 里找不到 `{cmd}`"));
    let rest = &src[at..];
    let f = rest.find("fields: &[").expect("那一条没有 fields") + "fields: &[".len();
    let end = rest[f..].find(']').expect("fields 没收尾") + f;
    rest[f..end]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[test]
fn the_command_names_are_the_ones_the_backend_registers() {
    let src = include_str!("../../src/backend/inbound.rs");
    for name in [CMD_KEY_SET, CMD_READ] {
        assert_eq!(
            src.matches(&format!("name: \"{name}\",")).count(),
            1,
            "后端 REGISTRY 里 `{name}` 不是恰好一条"
        );
    }
}

#[test]
fn the_read_answer_is_parsed_from_the_fields_the_backend_declares() {
    let fields = backend_fields_of(CMD_READ);
    assert!(
        fields.len() >= 5,
        "抠出来的字段太少，抽取器坏了：{fields:?}"
    );
    // 按后端声明的每个字段造一份样本（值的类型按那一格的语义给）。
    let sample_value = |k: &str| -> Value {
        match k {
            "configured" => json!(true),
            "rows" => json!(["work", "other"]),
            "notice" | "problem" => Value::Null,
            _ => json!(format!("v-{k}")),
        }
    };
    let mut full = serde_json::Map::new();
    for k in &fields {
        full.insert(k.clone(), sample_value(k));
    }
    let full = Value::Object(full);
    let st = status_from_wire("h", &full).expect("按后端声明的字段造的应答解析不出来");
    assert!(st.configured);
    assert_eq!(st.masked, "v-masked");
    assert_eq!(st.path, "v-path");
    assert_eq!(rows_from_wire("h", &full).unwrap(), vec!["work", "other"]);

    // 少任何一个必需字段 ⇒ 报错，**不退化成「没配」**。
    for must in ["configured", "masked", "path"] {
        let mut v = full.clone();
        v.as_object_mut().unwrap().remove(must);
        assert!(
            status_from_wire("h", &v).is_err(),
            "少了 `{must}` 还解析成功了 —— 那会被读成「没配」"
        );
    }
    let mut v = full.clone();
    v.as_object_mut().unwrap().remove("rows");
    assert!(rows_from_wire("h", &v).is_err(), "少了 `rows` 还解析成功了");
    // 类型不对同样报错。
    let mut v = full.clone();
    v["problem"] = json!(3);
    assert!(
        status_from_wire("h", &v).is_err(),
        "`problem` 是数字还解析成功了"
    );
}
