//! 〔RM1a · 第四波〕`remote_relay.rs` 的判据 —— 中转按机器，monitor 半。
//!
//! # 买到的
//!
//! - 🔴 本机那一臂**从不**起第二个中转：`ensure_on(本机)` 真走生产函数 ⇒ 拒，而且一帧都没发
//!   （本机后端那条连接在不在都一样 —— 拒在分派那一步）；本机的「在不在」走起会话那一侧那条缝
//!   （装替身、翻答案 ⇒ 结果跟着翻）。
//! - 远端那一臂在没有通道时说得出是哪台机器。
//! - 应答解析跟着后端**声明的字段**走（后端 `REGISTRY` 里 `relay-*` 两行的 `fields`，现抠）。
//! - 🔴 本模块一个上游选择的名字都没有（生产段零命中，带正控）。
//!
//! # 买不到的
//!
//! - 真远端那一趟（真后端起真中转）：后端那一半住 `tests/backend/relay/machine_tests.rs`（真子进程）。
//! - 触发点：本拍没有自动触发（模块头注），那一格交主会话。

use super::*;

fn own_production() -> String {
    guard_core::production_code(include_str!("../../src/bridge/src/remote_relay.rs"))
}

#[test]
fn the_local_arm_never_starts_a_second_relay() {
    let err =
        tauri::async_runtime::block_on(ensure_on(&Origin::local())).expect_err("本机也起了一个");
    assert_eq!(err, LOCAL_HAS_ITS_OWN);
    // 结构半：`ensure_on` 本机那一臂那一行里没有发送口。
    let src = own_production();
    let at =
        guard_core::find_pinned(&src, "pub(crate) async fn ensure_on(").expect("切不出 ensure_on");
    let body = &src[at..at + src[at..].find("\n}\n").expect("没收尾")];
    let local: Vec<&str> = body
        .lines()
        .filter(|l| l.contains("Route::Local =>"))
        .collect();
    assert_eq!(local.len(), 1, "本机那一臂不是恰好一行：{local:?}");
    assert!(
        !local[0].contains("call(") && local[0].contains("LOCAL_HAS_ITS_OWN"),
        "本机那一臂发了帧，或不是那句拒绝：{}",
        local[0]
    );
}

/// 〔US1 · 4D〕本机「中转在不在」**不再由 monitor 问**（本机那一个住在本机后端里，它在 `launch-endpoint` 的成品里答）：
/// `running_on` 本机那一臂与 `ensure_on` 一样当场拒、一帧都不发。期望手写（那句拒绝）。
#[test]
fn the_local_arm_of_running_on_refuses_like_ensure_on() {
    let e = tauri::async_runtime::block_on(running_on(&Origin::local())).expect_err("本机那一臂答了");
    assert_eq!(e, LOCAL_HAS_ITS_OWN);
}

#[test]
fn the_remote_arm_names_the_machine_when_there_is_no_channel() {
    let host = Origin("rm1a-no-such-host-for-relay".to_string());
    for err in [
        tauri::async_runtime::block_on(ensure_on(&host)).expect_err("没有通道还起了"),
        tauri::async_runtime::block_on(running_on(&host)).expect_err("没有通道还答了"),
    ] {
        assert!(
            err.contains("rm1a-no-such-host-for-relay"),
            "说不出是哪台机器：{err}"
        );
    }
}

/// 从后端源码里现抠 `REGISTRY` 那一条的 `fields`。
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
fn the_answers_are_parsed_from_the_fields_the_backend_declares() {
    let src = include_str!("../../src/backend/inbound.rs");
    for name in [CMD_STATUS, CMD_ENSURE] {
        assert_eq!(
            src.matches(&format!("name: \"{name}\",")).count(),
            1,
            "后端没有恰好一条 `{name}`"
        );
    }
    let sample = |cmd: &str| -> Value {
        let mut m = serde_json::Map::new();
        for k in backend_fields_of(cmd) {
            let v = match k.as_str() {
                "port" => json!(8788),
                "pid" => json!(42),
                _ => json!(true),
            };
            m.insert(k, v);
        }
        Value::Object(m)
    };
    let st = sample(CMD_STATUS);
    assert!(listening_from_wire("h", &st).unwrap());
    let en = sample(CMD_ENSURE);
    assert_eq!(
        ensured_from_wire("h", &en).unwrap(),
        RelayEnsured {
            listening: true,
            started: true
        }
    );
    // 少一格 ⇒ 报错，不猜。
    for k in ["listening", "started"] {
        let mut v = en.clone();
        v.as_object_mut().unwrap().remove(k);
        assert!(
            ensured_from_wire("h", &v).is_err(),
            "少了 `{k}` 还解析成功了"
        );
    }
    let mut v = st.clone();
    v.as_object_mut().unwrap().remove("listening");
    assert!(
        listening_from_wire("h", &v).is_err(),
        "少了 `listening` 还解析成功了"
    );
}

#[test]
fn this_module_knows_no_upstream_selection_name() {
    let prod = own_production();
    // 正控：同一把尺子在隔壁那份（上游选择那一半）上数得到。
    let apikey = guard_core::production_code(include_str!("../../src/bridge/src/apikey_remote.rs"));
    let needles = ["apikey", "creds", "account"];
    assert!(
        needles.iter().any(|n| apikey.contains(n)),
        "正控失败：尺子在账号那一半上都数不到 —— 瞎了"
    );
    // 只看代码行（剥掉 `//` 注释行：头注里要说「那是 apikey_remote.rs 的事」）。
    let code: String = prod
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for n in needles {
        assert!(
            !code.contains(n),
            "中转那一半的代码里出现了上游选择的词 `{n}`"
        );
    }
}
