//! 流旗标收进声明之后，monitor 这一侧剩下的那几件：起参只剩 `--stream`（本机两条载体一份常量）·
//! 远端那条流按「显示后台会话」藏 bg 会话（与本机同一个口径）。

/// 后端 `lib.rs::STREAM_FLAGS` 的字面量（从后端源码摘，异源）。
fn backend_stream_flags_cf1() -> std::collections::BTreeSet<String> {
    let src = include_str!("../../../../src/backend/lib.rs");
    let at = src
        .find("pub const STREAM_FLAGS: &[&str] = &[")
        .expect("后端 lib.rs 里找不到 `STREAM_FLAGS` 的定义");
    let rest = &src[at..];
    let body = &rest[..rest.find("];").expect("STREAM_FLAGS 没有收尾")];
    let mut out: std::collections::BTreeSet<String> = body
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    // 表里引用 `STREAM_FLAG_EXPLICIT` ⇒ 从后端源码补它的字面量。
    if body.contains("STREAM_FLAG_EXPLICIT") {
        let l = src
            .lines()
            .find(|l| l.contains("pub const STREAM_FLAG_EXPLICIT: &str"))
            .expect("抠不到 STREAM_FLAG_EXPLICIT");
        out.insert(
            l.split('"')
                .nth(1)
                .expect("STREAM_FLAG_EXPLICIT 没有字面量")
                .to_string(),
        );
    }
    out
}

/// 本机两条载体的起参：`--` 打头 ＋ 后端 `STREAM_FLAGS` 里那几个（后端不剥 ⇒ 当一次性查询跑完就退，§26）；两条载体用的是这一份。
#[test]
fn both_carriers_start_the_backend_with_the_same_stream_flags_the_backend_strips() {
    use crate::local_backend::LOCAL_STREAM_ARGS;
    let backend = backend_stream_flags_cf1();
    assert!(
        backend.contains("--stream"),
        "后端 STREAM_FLAGS 只摘到 {backend:?} —— 抽取坏了"
    );
    assert_eq!(
        LOCAL_STREAM_ARGS,
        &["--", "--stream"],
        "本机起参只剩「我是流模式」"
    );
    for a in &LOCAL_STREAM_ARGS[1..] {
        assert!(
            backend.contains(*a),
            "本机后端起参 `{a}` 不在后端 `STREAM_FLAGS`（{backend:?}）里"
        );
    }
    let stdio = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/local_backend.rs"
    ));
    let host = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    assert_eq!(
        stdio.matches("LOCAL_STREAM_ARGS.iter()").count(),
        1,
        "stdio 载体的起法（`start_or_extract`）要用 LOCAL_STREAM_ARGS"
    );
    guard_core::find_pinned(&host, "cmd.args(local_backend::LOCAL_STREAM_ARGS)")
        .unwrap_or_else(|e| panic!("常驻载体没用 LOCAL_STREAM_ARGS：{e}"));
}

/// 远端那条流：没开「显示后台会话」⇒ bg 会话的宣告与它后面的行 · 状态 · 一轮结束都藏、去向那一帧之后摘；交互会话一帧不藏。
/// 开着 ⇒ 什么都不藏。
#[test]
fn the_remote_stream_hides_background_sessions_like_the_local_one() {
    let frame = |l: &str| crate::stream_source::parse_frame(l).expect("认得的帧");
    let added = |sid: &str, bg: bool| {
        frame(&format!(
            r#"{{"kind":"session_added","sid":"{sid}","background":{bg},"activity_text":"x","activity_tone":"now","pid":1}}"#
        ))
    };
    let line = |sid: &str| {
        frame(&format!(
            r#"{{"kind":"line","session_id":"{sid}","path":"/p","seq":0,"byte_offset":1}}"#
        ))
    };
    let turn = |sid: &str| {
        frame(&format!(
            r#"{{"kind":"turn_end","session_id":"{sid}","uuid":"u"}}"#
        ))
    };
    let state = |sid: &str| {
        frame(&format!(
            r#"{{"kind":"session_state","sid":"{sid}","state":"ended","state_text":"x","state_hint":"x","state_tone":"plain"}}"#
        ))
    };
    let mut off = crate::stream_source::local::BgHide::new(false);
    assert!(off.hides(&added("bg1", true)));
    assert!(off.hides(&line("bg1")));
    assert!(off.hides(&turn("bg1")));
    assert!(!off.hides(&added("i1", false)));
    assert!(!off.hides(&line("i1")));
    assert!(!off.hides(&turn("i1")));
    assert!(off.hides(&state("bg1")), "去向那一帧也藏");
    assert!(
        !off.hides(&line("bg1")),
        "去向之后摘掉（同一个 sid 再来是新的宣告）"
    );
    let mut on = crate::stream_source::local::BgHide::new(true);
    assert!(!on.hides(&added("bg1", true)));
    assert!(!on.hides(&line("bg1")));
}

/// 「bg 会话藏不藏」只有一道门（[`crate::stream_source::local::BgHide`]），本机远端两条流各过一次：
/// 读「显示后台会话」开关的生产调用方恰好是这两条读循环、各自拿它造那道门；远端读循环对每一帧过 `hides`，本机对每一件过 `admit`。
/// 谁在别处另写一份藏法（或哪条流不过这道门），这一条红。
#[test]
fn both_streams_hide_background_sessions_through_the_one_gate() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    assert!(files.len() > 100, "只扫到 {} 份 —— 取法坏了", files.len());
    let callers: std::collections::BTreeSet<String> = files
        .iter()
        .filter(|(p, raw)| {
            !p.ends_with("lib.rs")
                && guard_core::production_code(raw).contains("load_show_bg_sessions()")
        })
        .map(|(p, _)| {
            p.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(
        callers,
        ["stream_source/local.rs", "stream_source/run.rs"]
            .into_iter()
            .map(String::from)
            .collect(),
        "读「显示后台会话」的不止两条读循环 —— 别处另有一份藏法"
    );
    let remote = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/stream_source/run.rs"
    ));
    for needle in [
        "let mut bg = super::local::BgHide::new(crate::load_show_bg_sessions());",
        "let frame = frame.filter(|f| !bg.hides(f));",
    ] {
        guard_core::find_pinned(&remote, needle)
            .unwrap_or_else(|e| panic!("远端那条流没过那道门：{e}"));
    }
    let local = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/stream_source/local.rs"
    ));
    for needle in [
        "let mut bg = BgHide::new(show_bg);",
        "let Some(item) = bg.admit(item) else {",
    ] {
        guard_core::find_pinned(&local, needle)
            .unwrap_or_else(|e| panic!("本机那条流没过那道门：{e}"));
    }
}
