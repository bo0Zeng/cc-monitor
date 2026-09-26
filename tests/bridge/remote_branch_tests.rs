//! 〔LOC1a · 第四波 4D〕要求住址：`设计/05 §14.6`「本机那几问从『exec 一次性本机后端』改走 `<local>` 长连接」·
//! `INVARIANTS §40`「本地 ＝ 不走 ssh 的远端」—— 分叉两支同一条帧命令 `session-fork`、同一份结果解码。
//! 此前这里的十几条判的是 exec 那一趟的 stdout / stderr / 退出码怎么解释（旧后端掉进流模式 · banner 噪声 ·
//! 没有退出码不许当 0）；那一趟没了（长连接的应答是结构化的 `ok` / `error`，「老后端」由 `accepts` 当场说），
//! 那组性质随传输一起退役。

use super::*;

#[path = "support/scripted_backend.rs"]
mod scripted;

/// 〔IV1 · V121〕要求住址：`INVARIANTS §47`（外部值拼进 shell / 交给对端之前本侧先过放行判定）；①形。
#[test]
fn fork_ids_are_whitelisted() {
    assert!(validate_fork_id("sid", "0473c3a0-1111-2222-3333-444455556666").is_ok());
    for bad in ["", "../etc/passwd", "a b", "a;rm -rf /", "a'b", "a/b"] {
        assert!(validate_fork_id("sid", bad).is_err(), "该拒: {bad:?}");
    }
    assert!(
        validate_fork_id("sid", &"a".repeat(65)).is_err(),
        "上限该与后端的 64 对齐"
    );
    assert!(validate_fork_id("sid", &"a".repeat(64)).is_ok());
}

/// 结果严格收：缺一格 / 类型不对 ⇒ 当场说「两端契约对不上」，**不返回空壳**。
#[test]
fn the_fork_product_is_read_strictly() {
    let r = decode_fork(
        "本机",
        serde_json::json!({"sessionId": "new", "jsonlPath": "/p/new.jsonl"}),
    )
    .expect("成功那一形");
    assert_eq!(
        (r.session_id.as_str(), r.jsonl_path.as_str()),
        ("new", "/p/new.jsonl")
    );
    for bad in [
        serde_json::json!({"sessionId": "new"}),
        serde_json::json!({"jsonlPath": "/p"}),
        serde_json::json!({"sessionId": 1, "jsonlPath": "/p"}),
        serde_json::json!(null),
    ] {
        let e = decode_fork("本机", bad.clone()).expect_err("该拒");
        assert!(e.contains("两端契约对不上"), "{bad} ⇒ {e}");
    }
}

/// 〔LOC1a · J3 的 monitor 那一半〕后端帧面真产出的那一份（`fork_write::answer_wire_at` 对夹具会话现算）
/// 与 monitor 的解码器对得上 —— 异源：键名由后端那份 `ForkResult` 的 serde 决定，monitor 这边是 `BranchResult` 的 serde。
/// 夹具的**真产出**由后端测试写进金样（`tests/__fixtures__/session-fork.golden.json`），这里读同一份。
#[test]
fn the_backend_product_decodes_on_this_side() {
    let raw = std::fs::read_to_string(
        crate::guard_support::repo_root().join("tests/__fixtures__/session-fork.golden.json"),
    )
    .expect("读金样");
    let g: serde_json::Value = serde_json::from_str(&raw).expect("金样是 JSON");
    let r = decode_fork("本机", g["product"].clone()).expect("后端产出的那一份这边读得懂");
    assert!(!r.session_id.is_empty() && r.jsonl_path.ends_with(".jsonl"));
    assert_eq!(
        g["request"],
        serde_json::json!({"sid": g["request"]["sid"], "uuid": g["request"]["uuid"]}),
        "请求体只许 sid / uuid 两格"
    );
}

/// ★ 本机那一支真走 `<local>` 长连接、问的是 `session-fork`、带的是 sid / uuid（异源：数的是假后端那一侧收到的）；
/// 坏 id 一个字节都不发。
#[test]
fn the_local_fork_goes_over_the_long_connection() {
    let _guard = crate::backend::control::inbound_client::local_origin_test_lock();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let _enter = rt.enter();
    let rig = scripted::rig(
        crate::backend::control::inbound_client::LOCAL_ORIGIN,
        &["session-fork"],
        vec![(
            "session-fork",
            Ok(serde_json::json!({"sessionId": "new-sid", "jsonlPath": "/p/new-sid.jsonl"})),
        )],
    );
    let e = rt
        .block_on(create_local_branch_session("../etc", "u1"))
        .expect_err("坏 id 该就地拒");
    assert!(e.contains("非法"), "{e}");
    assert!(rig.cmds().is_empty(), "坏 id 照样发出去了");
    let r = rt
        .block_on(create_local_branch_session("src-sid", "msg-uuid"))
        .expect("脚本给了结果");
    assert_eq!(r.session_id, "new-sid");
    let seen = rig.seen.lock().expect("lock").clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "session-fork");
    assert_eq!(
        seen[0].1,
        serde_json::json!({"sid": "src-sid", "uuid": "msg-uuid"})
    );
}

/// 长连接在、却不认 `session-fork`（老后端）⇒ 当场说、不发；对端说不行 ⇒ 把那句原因带出来。
#[test]
fn an_old_backend_or_a_refusal_is_said_not_swallowed() {
    let _guard = crate::backend::control::inbound_client::local_origin_test_lock();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let _enter = rt.enter();
    {
        let rig = scripted::rig(
            crate::backend::control::inbound_client::LOCAL_ORIGIN,
            &["ping"],
            vec![],
        );
        let e = rt
            .block_on(create_local_branch_session("s", "u"))
            .expect_err("不认");
        assert!(e.contains("还不认") && e.starts_with("本机"), "{e}");
        assert!(rig.cmds().is_empty());
    }
    let _rig = scripted::rig(
        crate::backend::control::inbound_client::LOCAL_ORIGIN,
        &["session-fork"],
        vec![(
            "session-fork",
            Err(("fork_failed", "refuse branch: session ccc not found")),
        )],
    );
    let e = rt
        .block_on(create_local_branch_session("ccc", "u"))
        .expect_err("对端拒");
    assert!(e.contains("not found") && e.contains("fork_failed"), "{e}");
}
