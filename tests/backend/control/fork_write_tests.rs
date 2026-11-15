use super::*;
use crate::platform::fs::NewFile;
use std::path::PathBuf;

/// 按 sid 找到的那一条的替身：哪一家 · 哪份文件（家与根这几条判据用不到）。
fn at_of(kind: &'static str, file: PathBuf) -> crate::agents::SessionAt {
    crate::agents::SessionAt {
        kind,
        home: PathBuf::new(),
        root: PathBuf::new(),
        file,
    }
}

/// 帧面那一格 `launch` 的替身（推断本身由 `fork_launch_tests.rs` 钉）。
fn no_launch(_: &crate::agents::SessionAt, _: &str) -> crate::control::fork_launch::Launch {
    use crate::control::fork_launch::{Launch, Slot, Why};
    Launch {
        cwd: Slot::Unknown { why: Why::NoCwd },
        account: Slot::Unknown { why: Why::Exited },
        terminal: Slot::Unknown { why: Why::Exited },
    }
}

/// 帧面那一跳：入参照 typed 那一处解（形状不对 ⇒ `bad_args`），再交本体；失败投成 `(码, 那一句)`。
fn ask(
    root: &Path,
    args: serde_json::Value,
    launch: &dyn Fn(&crate::agents::SessionAt, &str) -> crate::control::fork_launch::Launch,
) -> Result<serde_json::Value, crate::common::said::Said> {
    use crate::wire::typed::Codes;
    let req: ForkReq = crate::wire::typed::take(&args)?;
    fork_at(&NewFile::issue(), root, &req, launch)
        .map(|r| serde_json::to_value(&r).unwrap())
        .map_err(Codes::into_said)
}

fn tmp(tag: &str) -> PathBuf {
    let p = test_home::scratch(tag);
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

    let res = run_inner(&NewFile::issue(), &root, "srcsid", "u2").unwrap();

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
    let err = write_new_file(&NewFile::issue(), &p, &[serde_json::json!({"x":1})]).unwrap_err();
    assert!(matches!(err, NewFileFail::Create(_)), "落在新建那一步");
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "PREEXISTING\n",
        "已存在的文件被覆盖了"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 找那一份走的是注册表那一格（本体住适配层 `agents/claudecode/branch.rs`）—— 这里钉的是**这条路真的经过它**。
fn find_here(root: &Path, sid: &str) -> Result<PathBuf, String> {
    crate::agents::find_session_in(root, sid).map(|at| at.file)
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

/// ★★ backend 这一侧：**给一个查不到的 sid，处置是报错，
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
    let names = test_home::list(&root.join("projects").join("proj"));
    let err = run_inner(&NewFile::issue(), &root, "ccc", "u2").unwrap_err();
    assert_eq!(
        err.said, "未分叉 · 未找到会话记录 · ccc",
        "上屏只出文案表那一句"
    );
    assert!(
        err.raw.as_deref().is_some_and(|r| r.contains("not found")),
        "下层说的进原话：{err:?}"
    );
    // 起新会话那一条带 fork 找同一份：码照 fork_failed，那一句同上。
    let err = fork_for_launch_among(
        crate::agents::REGISTRY,
        &NewFile::issue(),
        &root,
        "ccc",
        "u2",
    )
    .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("fork_failed"));
    assert_eq!(err.said, "未分叉 · 未找到会话记录 · ccc");
    assert!(err.raw.is_some(), "下层说的进原话");
    assert_eq!(
        test_home::list(&root.join("projects").join("proj")),
        names,
        "一份都不写"
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
    let err = run_inner(&NewFile::issue(), &root, "sc", "s1").unwrap_err();
    assert_eq!(
        err.said, "未分叉 · 会话记录无法改写成分支 · sc",
        "上屏只出文案表那一句"
    );
    assert!(
        err.raw.as_deref().is_some_and(|r| r.contains("sidechain")),
        "那一家说的进原话：{err:?}"
    );
    // 起新会话那一条带 fork 走同一份改写：码照 fork_failed，那一句同上。
    let err = fork_for_launch_among(
        crate::agents::REGISTRY,
        &NewFile::issue(),
        &root,
        "sc",
        "s1",
    )
    .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("fork_failed"));
    assert_eq!(err.said, "未分叉 · 会话记录无法改写成分支 · sc");
    std::fs::remove_dir_all(&root).ok();
}

/// ★ 认家只照按 sid 找到的那一家（`SessionAt::kind`），不照路径另认（替身家：两家都认这个文件名、照路径认不出；
/// 会话住在自己叉的那一家的根下）⇒ 照那一家自己叉，回它交回的 sid，通用层一份都不写。
#[test]
fn a_launch_fork_goes_by_the_family_that_found_it_not_by_the_path() {
    let (reg, at) = crate::agents::fake::found_in_the_other("fork-by-kind", |l| l);
    std::fs::write(&at.file, "{\"node\":\"n1\"}\n").unwrap();
    let dir = at.file.parent().unwrap().to_path_buf();
    let names = test_home::list(&dir);
    let homes = [crate::agents::AgentHome {
        kind: at.kind,
        home: at.home.clone(),
    }];
    let sid = "00000000-0000-4000-8000-0000000000c7";
    crate::agents::fake::own_fork_calls();
    let got = fork_for_launch_among(&reg, &NewFile::issue(), &homes, sid, "n1");
    assert_eq!(got.ok().as_deref(), Some("new-1"), "照那一家自己叉");
    assert_eq!(
        crate::agents::fake::own_fork_calls(),
        vec![(sid.to_string(), "n1".to_string())],
        "那一家收到源 sid 与那条消息"
    );
    assert_eq!(test_home::list(&dir), names, "通用层一份都不写");
    std::fs::remove_dir_all(&at.home).ok();
}

/// ★ 源会话起在哪个目录也只照找到它的那一家问（同上夹具：照路径认不出）⇒ 那一家记录面说的那一个。
#[test]
fn the_source_cwd_goes_by_the_family_that_found_it_not_by_the_path() {
    fn here(_: &Path) -> Option<String> {
        Some("/w/proj".to_string())
    }
    let (mut reg, at) = crate::agents::fake::found_in_the_other("fork-cwd", |l| l);
    for row in reg.iter_mut().filter(|r| r.kind == at.kind) {
        if let Some(r) = row.records.as_mut() {
            r.project_dir = Some(here);
        }
    }
    assert_eq!(
        source_cwd_among(&reg, &at).as_deref(),
        Some("/w/proj"),
        "照那一家记录面给出"
    );
    std::fs::remove_dir_all(&at.home).ok();
}

/// ★ 改写那一形的每种拒法上屏只出文案表那一句（下层说的进原话）：源记录读不出 · 那一家没有会话文件命名 · 新记录建不成。
#[test]
fn each_rewrite_refusal_says_the_copy_table_sentence() {
    // 读不出：认得是哪一家的（文件名），盘上没有。
    let root = tmp("fork-read");
    let gone = root.join("projects").join("proj").join("gone.jsonl");
    let kind =
        crate::agents::record_kind_among(crate::agents::REGISTRY, &gone).expect("认得是哪一家");
    let err = rewrite(
        crate::agents::REGISTRY,
        &NewFile::issue(),
        at_of(kind, gone),
        "gone",
        "u2",
    )
    .unwrap_err();
    assert_eq!(err.said, "未分叉 · 会话记录无法读取 · gone");
    assert!(err.raw.is_some(), "读不出的原因进原话");

    // 没有会话文件命名：替身家的合成历史那一家（会话不住记录树），改写面换成原样交回。
    fn chats_home() -> Option<PathBuf> {
        Some(std::env::temp_dir().join(format!("ccm-fork-chats-{}", std::process::id())))
    }
    fn as_is(
        lines: &[serde_json::Value],
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Vec<serde_json::Value>, String> {
        Ok(lines.to_vec())
    }
    let mut row = crate::agents::fake::runs::history::row(chats_home);
    if let Some(r) = row.records.as_mut() {
        r.branch = Some(crate::agents::BranchFace::Rewrite(as_is));
    }
    let chats = crate::agents::fake::runs::history::records_root(&chats_home().unwrap());
    std::fs::create_dir_all(&chats).unwrap();
    let src = chats.join(crate::agents::fake::runs::history::file_name("c1"));
    std::fs::write(&src, "{\"chat\":\"c1\"}\n").unwrap();
    let chats_kind = row.kind;
    let err = rewrite(&[row], &NewFile::issue(), at_of(chats_kind, src), "c1", "u").unwrap_err();
    assert_eq!(err.said, "未分叉 · 该家的会话不支持分叉 · c1");
    std::fs::remove_dir_all(chats_home().unwrap()).ok();

    // 新记录建不成：源记录所在目录只读（以 root 跑时只读挡不住，这一段不判）。
    #[cfg(unix)]
    {
        seed(&root, "ro");
        let proj = root.join("projects").join("proj");
        test_home::set_dir_mode(&proj, 0o555);
        let probe = proj.join("probe");
        let read_only = std::fs::write(&probe, "").is_err();
        let res = rewrite(
            crate::agents::REGISTRY,
            &NewFile::issue(),
            at_of(kind, proj.join("ro.jsonl")),
            "ro",
            "u2",
        );
        test_home::set_dir_mode(&proj, 0o755);
        if read_only {
            let err = res.err().expect("只读目录里建不成");
            assert_eq!(err.said, "未分叉 · 分支记录无法新建 · ro");
            assert!(err.raw.is_some(), "建不成的原因进原话");
        }
    }
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

// ═════════════════════════════════════════════════════════════════════════════
// 帧面 `session-fork`：与本体 `run_inner` 同一份
// ═════════════════════════════════════════════════════════════════════════════
//
// 要求：「本机那几问从『exec 一次性本机后端』改走 `<local>` 长连接」—— 分叉上了帧面之后，
// 帧面与本体必须是**同一份**实现（「业务解释只有一个家」）。

/// ★ J3：同一份夹具会话、同一个 uuid，帧面 `fork_at` 与本体 `run_inner` 落盘的**内容**逐字相同（只差新 sid），
/// 帧面回包的 id 与落点就是本体那一份；金样 `tests/__fixtures__/session-fork.golden.json`
/// 的 `product` 键集 == 真产出的键集（界面那一侧的生成类型同一形）。
#[test]
fn the_frame_face_and_the_core_are_one_fork() {
    let root = tmp("wire");
    seed(&root, "srcsid");
    let via_cli = run_inner(&NewFile::issue(), &root, "srcsid", "u2").unwrap();
    // 帧面那一格 `launch` 由入口交：它拿到的是源会话那份记录与源 sid。
    let asked = std::cell::RefCell::new(Vec::new());
    let via_wire = ask(
        &root,
        serde_json::json!({"sid": "srcsid", "uuid": "u2"}),
        &|source, sid| {
            asked
                .borrow_mut()
                .push((source.file.clone(), sid.to_string()));
            no_launch(source, sid)
        },
    )
    .unwrap();
    assert_eq!(
        asked.into_inner(),
        vec![(
            root.join("projects").join("proj").join("srcsid.jsonl"),
            "srcsid".to_string()
        )]
    );
    assert_eq!(
        via_wire["launch"],
        serde_json::to_value(no_launch(&at_of("", PathBuf::new()), "")).unwrap()
    );
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    let read_rows = |p: &str, sid: &str| -> String {
        std::fs::read_to_string(p).unwrap().replace(sid, "<SID>")
    };
    assert_eq!(
        read_rows(
            via_wire["jsonlPath"].as_str().unwrap(),
            via_wire["sessionId"].as_str().unwrap()
        ),
        read_rows(&via_cli.jsonl_path, &via_cli.session_id),
        "帧面与本体落盘的内容不是同一份变换"
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

/// 入参缺 / 空 / 不是串 / 多一格 ⇒ `bad_args`，一个字节都不写；找不到 ⇒ `fork_failed`（原因原样带回）。
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
        serde_json::json!({"sid": "srcsid", "uuid": "u2", "extra": 1}),
    ] {
        let e = ask(&root, bad.clone(), &no_launch).expect_err("该拒");
        assert_eq!(e.code.as_deref(), Some("bad_args"), "{bad}");
    }
    let e = ask(
        &root,
        serde_json::json!({"sid": "nosuch", "uuid": "u2"}),
        &no_launch,
    )
    .unwrap_err();
    assert_eq!(e.code.as_deref(), Some("fork_failed"));
    assert!(e.said.contains("nosuch"), "{}", e.said);
    assert_eq!(std::fs::read(&src).unwrap(), before, "拒了却动了源文件");
    std::fs::remove_dir_all(&root).ok();
}

/// 〔要求住址 `INVARIANTS §47` ①「分叉的 sid / 消息 uuid」〕界面经通道直说、不再判 ⇒ 帧面入口先过放行判定
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
            let e = ask(&root, args.clone(), &no_launch).expect_err("该拒");
            let msg = e.said;
            assert_eq!(e.code.as_deref(), Some("bad_args"), "{args} ⇒ {msg}");
            assert!(
                copy_core::copy_matches("rsRemoteBranch.forkId.badChar", &msg)
                    || copy_core::copy_matches("rsRemoteBranch.forkId.badLength", &msg),
                "{args} ⇒ {msg}"
            );
        }
    }
    assert_eq!(std::fs::read(&src).unwrap(), before, "拒了却动了源文件");
    let ok = ask(
        &root,
        serde_json::json!({"sid": "srcsid", "uuid": "u2"}),
        &no_launch,
    )
    .expect("真实形状的 id 放得过");
    assert!(ok["sessionId"].as_str().is_some_and(|s| s.len() <= 64));
    assert!(
        ask(
            &root,
            serde_json::json!({"sid": "srcsid", "uuid": "a".repeat(64)}),
            &no_launch
        )
        .err()
        .and_then(|e| e.code)
        .as_deref()
            != Some("bad_args"),
        "64 位是上界本身，不该被形状判定拒"
    );
    std::fs::remove_dir_all(&root).ok();
}
