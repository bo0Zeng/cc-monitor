use super::*;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-fork-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(p.join("projects").join("proj")).unwrap();
    p
}

fn seed(dir: &Path, sid: &str) {
    let rows = [
        serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"timestamp":"t1","sessionId":sid}),
        serde_json::json!({"type":"assistant","uuid":"u2","parentUuid":"u1","timestamp":"t2","sessionId":sid}),
        serde_json::json!({"type":"user","uuid":"u3","parentUuid":"u2","timestamp":"t3","sessionId":sid}),
    ];
    let body: String = rows
        .iter()
        .map(|r| format!("{r}\n"))
        .collect::<Vec<_>>()
        .concat();
    std::fs::write(
        dir.join("projects")
            .join("proj")
            .join(format!("{sid}.jsonl")),
        body,
    )
    .unwrap();
}

#[test]
fn fork_writes_new_file_and_leaves_source_untouched() {
    let root = tmp("ok");
    seed(&root, "srcsid");
    let src = root.join("projects").join("proj").join("srcsid.jsonl");
    let before = std::fs::read(&src).unwrap();

    let res = run_inner(&root, &sargs(&["--fork-session", "srcsid", "u2"])).unwrap();

    assert_eq!(std::fs::read(&src).unwrap(), before, "源文件被改动了");
    let out = PathBuf::from(&res.jsonl_path);
    assert!(out.exists());
    assert_eq!(
        out.parent().unwrap(),
        src.parent().unwrap(),
        "应落在源同目录"
    );
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(&out)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // 走的是共享变换：祖先链 u1→u2，sessionId 换新
    let uuids: Vec<&str> = rows.iter().map(|r| r["uuid"].as_str().unwrap()).collect();
    assert_eq!(uuids, vec!["u1", "u2"]);
    assert_eq!(rows[0]["sessionId"].as_str().unwrap(), res.session_id);
    std::fs::remove_dir_all(&root).ok();
}

/// ★ 并发/覆盖：`O_EXCL` 必须让第二次写落到错误上，而不是盖掉第一份。
#[test]
fn write_new_file_refuses_existing_target() {
    let root = tmp("excl");
    let p = root.join("projects").join("proj").join("dup.jsonl");
    std::fs::write(&p, "PREEXISTING\n").unwrap();
    let err = write_new_file(&p, &[serde_json::json!({"x":1})]).unwrap_err();
    assert!(err.contains("create"), "got: {err}");
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "PREEXISTING\n",
        "已存在的文件被覆盖了"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 找那一份走的是注册表那一格（〔THIN〕本体住适配层 `agents/claudecode/branch.rs`）—— 这里钉的是**这条路真的经过它**。
fn find_here(root: &Path, sid: &str) -> Result<PathBuf, String> {
    crate::agents::find_session_file(&projects_root(root), sid)
}

#[test]
fn rejects_path_traversal_sid() {
    let root = tmp("trav");
    for bad in ["../../etc/passwd", "a/b", "..", "a\\b", ""] {
        assert!(find_here(&root, bad).is_err(), "sid {bad:?} 应被拒");
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn missing_session_is_an_error_not_a_panic() {
    let root = tmp("missing");
    let err = find_here(&root, "nope").unwrap_err();
    assert!(err.contains("not found"), "got: {err}");
    std::fs::remove_dir_all(&root).ok();
}

/// ★★ `KR88D2` 第三刀（backend 这一侧）：**给一个查不到的 sid，处置是报错，
/// 不是「树上有什么就拿什么」。**
///
/// 树上**真的有两份**别的会话 —— 少了这一步，下面那条断言在空树上也绿，
/// 而「静默取第一个」正是它要逮的那一形。
/// monitor 侧的同形判据是 `history·rs::an_unknown_session_id_is_refused_not_silently_substituted`，
/// 两条读的是同一份实现。
#[test]
fn an_unknown_session_id_is_refused_not_silently_substituted() {
    let root = tmp("unknown");
    seed(&root, "aaa");
    seed(&root, "bbb");
    // 反向自检：树上真有东西可被「随手挑」。
    assert!(find_here(&root, "aaa").is_ok(), "夹具没造出可被挑中的会话");
    let err = run_inner(&root, &sargs(&["--fork-session", "ccc", "u2"])).unwrap_err();
    assert!(
        err.contains("not found") && err.contains("ccc"),
        "查不到的 sid 应当报错并点名，实得：{err}"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// ★ 符号链接不算命中 —— 记录树里一条指向界外的链接，**分叉不到它**。
///
/// 这一半原先只有 monitor 那条路有（靠 canonicalize 两边比前缀）；
/// 收成一份之后两侧同时拿到。
#[cfg(unix)]
#[test]
fn a_symlink_inside_the_tree_is_not_a_hit() {
    let root = tmp("symlink");
    let outside = root.join("secret.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    let link = root.join("projects").join("proj").join("linked.jsonl");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let err = find_here(&root, "linked").unwrap_err();
    assert!(err.contains("not found"), "got: {err}");
    assert!(outside.exists(), "界外那份被动过了");
    std::fs::remove_dir_all(&root).ok();
}

/// 子 agent 记录不可分叉 —— 这条判据在共享 crate 里，这里钉住后端真的走了它。
#[test]
fn sidechain_reject_reaches_backend_path() {
    let root = tmp("side");
    seed(&root, "sc");
    let f = root.join("projects").join("proj").join("sc.jsonl");
    let mut body = std::fs::read_to_string(&f).unwrap();
    body.push_str(
        &serde_json::json!({"type":"assistant","uuid":"s1","parentUuid":"u2",
            "timestamp":"t9","sessionId":"sc","isSidechain":true})
        .to_string(),
    );
    body.push('\n');
    std::fs::write(&f, body).unwrap();
    let err = run_inner(&root, &sargs(&["--fork-session", "sc", "s1"])).unwrap_err();
    assert!(err.contains("sidechain"), "got: {err}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn new_session_id_has_uuid_shape_and_varies() {
    let a = new_session_id("x");
    assert_eq!(a.len(), 36, "{a}");
    assert_eq!(a.split('-').count(), 5, "{a}");
    // 同一输入连续两次也应不同（含 nanos）——但唯一性真正靠 O_EXCL，不靠这个。
    assert_ne!(a, new_session_id("x"));
}

fn sargs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

// ═════════════════════════════════════════════════════════════════════════════
// 〔LOC1a · 第四波 4D〕帧面 `session-fork`：与 CLI `--fork-session` 同一份本体
// ═════════════════════════════════════════════════════════════════════════════
//
// 要求住址：`设计/05 §14.6`「本机那几问从『exec 一次性本机后端』改走 `<local>` 长连接」—— 分叉上了帧面之后，
// 帧面与 CLI 两个入口必须是**同一份**实现（`设计/05 §14.3`「业务解释只有一个家」）。

/// ★ J3：同一份夹具会话、同一个 uuid，帧面 `answer_wire_at` 与 CLI `run_inner` 落盘的**内容**逐字相同（只差新 sid），
/// 帧面的 `data` 键集 == CLI 那一行的键集（两个入口，一份形状）；金样 `tests/__fixtures__/session-fork.golden.json`
/// 的 `product` 键集 == 真产出的键集（monitor 那一侧读同一份金样解码）。
#[test]
fn the_frame_face_and_the_cli_face_are_one_fork() {
    let root = tmp("wire");
    seed(&root, "srcsid");
    let via_cli = run_inner(&root, &sargs(&["--fork-session", "srcsid", "u2"])).unwrap();
    let via_wire =
        answer_wire_at(&root, &serde_json::json!({"sid": "srcsid", "uuid": "u2"})).unwrap();
    let cli_v = serde_json::to_value(&via_cli).unwrap();
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(keys(&via_wire), keys(&cli_v), "两个入口的形状不一样了");
    let read_rows = |p: &str, sid: &str| -> String {
        std::fs::read_to_string(p).unwrap().replace(sid, "<SID>")
    };
    assert_eq!(
        read_rows(
            via_wire["jsonlPath"].as_str().unwrap(),
            via_wire["sessionId"].as_str().unwrap()
        ),
        read_rows(&via_cli.jsonl_path, &via_cli.session_id),
        "帧面与 CLI 落盘的内容不是同一份变换"
    );
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/session-fork.golden.json")).unwrap();
    assert_eq!(
        keys(&golden["product"]),
        keys(&via_wire),
        "金样的成品键集与真产出对不上"
    );
    assert_eq!(
        keys(&golden["request"]),
        vec!["sid".to_string(), "uuid".to_string()]
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 入参缺 / 空 / 不是串 ⇒ `bad_args`，一个字节都不写；找不到 ⇒ `fork_failed`（原因原样带回）。
#[test]
fn the_frame_face_refuses_bad_args_before_touching_the_disk() {
    let root = tmp("wirebad");
    seed(&root, "srcsid");
    let src = root.join("projects").join("proj").join("srcsid.jsonl");
    let before = std::fs::read(&src).unwrap();
    for bad in [
        serde_json::json!({}),
        serde_json::json!({"sid": "srcsid"}),
        serde_json::json!({"sid": "", "uuid": "u2"}),
        serde_json::json!({"sid": 1, "uuid": "u2"}),
    ] {
        let (code, _) = answer_wire_at(&root, &bad).expect_err("该拒");
        assert_eq!(code, "bad_args", "{bad}");
    }
    let (code, msg) =
        answer_wire_at(&root, &serde_json::json!({"sid": "nosuch", "uuid": "u2"})).unwrap_err();
    assert_eq!(code, "fork_failed");
    assert!(msg.contains("nosuch"), "{msg}");
    assert_eq!(std::fs::read(&src).unwrap(), before, "拒了却动了源文件");
    std::fs::remove_dir_all(&root).ok();
}

/// 〔MIG-3b · 要求住址 `INVARIANTS §47` ①「分叉的 sid / 消息 uuid」〕界面经通道直说、不再判 ⇒ 帧面入口先过放行判定
/// （共享那一份 `session_id_ok`，从 monitor 分叉那一侧的白名单判据搬来）：坏值 `bad_args`、源一个字节不动；
/// 真实形状的 id 放得过（正控，防判定焊成恒拒）。
#[test]
fn the_fork_ids_are_whitelisted_at_the_frame_face() {
    let root = tmp("wireids");
    seed(&root, "srcsid");
    let src = root.join("projects").join("proj").join("srcsid.jsonl");
    let before = std::fs::read(&src).unwrap();
    let long = "a".repeat(65);
    for bad in [
        "../etc/passwd",
        "a b",
        "a;rm -rf /",
        "a'b",
        "a/b",
        "-srcsid",
        long.as_str(),
    ] {
        for args in [
            serde_json::json!({"sid": bad, "uuid": "u2"}),
            serde_json::json!({"sid": "srcsid", "uuid": bad}),
        ] {
            let (code, msg) = answer_wire_at(&root, &args).expect_err("该拒");
            assert_eq!(code, "bad_args", "{args} ⇒ {msg}");
            assert!(msg.contains("非法"), "{args} ⇒ {msg}");
        }
    }
    assert_eq!(std::fs::read(&src).unwrap(), before, "拒了却动了源文件");
    let ok = answer_wire_at(&root, &serde_json::json!({"sid": "srcsid", "uuid": "u2"}))
        .expect("真实形状的 id 放得过");
    assert!(ok["sessionId"].as_str().is_some_and(|s| s.len() <= 64));
    assert!(
        answer_wire_at(
            &root,
            &serde_json::json!({"sid": "srcsid", "uuid": "a".repeat(64)})
        )
        .map_err(|(c, _)| c)
        .err()
            != Some("bad_args"),
        "64 位是上界本身，不该被形状判定拒"
    );
    std::fs::remove_dir_all(&root).ok();
}
