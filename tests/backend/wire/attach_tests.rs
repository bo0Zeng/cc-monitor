//! 「终端」那一颗点了做什么（`can.terminal` 的 `near` · `far`）的判据：会话头 · 右键 · 命令面板 · 快捷键 · 手机顶栏都只读这一格，
//! 由会话住的那台判，两端只按「看的这一端在不在那台上」挑一格。

/// 照一生 ＋ 宿主：
/// - 托管终端（没有外部窗口）⇒ 两端都在这里开；
/// - tmux · 住哪判不了 · 程序退了 tmux 会话还在 ⇒ 在这台上看拉 ccm 开的那个窗口（壳拉不了 ⇒ 在这里开），别处看接进去；
/// - 不在任何宿主里 ⇒ 在这台上只拉得了它的窗口（壳拉不了 ⇒ 不能），别处不能；
/// - 已结束 ⇒ 两端都「先恢复再开」；记录已不在 · 说不清 · 看不见 ⇒ 两端不能、写那一态那一句；
/// - pidfile 说接进去没有意义（`attachable: false`）⇒ 「在这里开」不能，拉窗照旧；
/// - 宣告时判出的终端句柄跟着「在这里开」走（手机照它附着，不另问那台在哪）。
#[test]
fn the_terminal_cell_says_what_one_button_does_near_and_far() {
    use crate::wire::{
        Housed, Life, SessionContainer, SessionFace, SessionFaceExt as _, TerminalHost,
    };
    let near =
        |f: &SessionFace| serde_json::to_value(f).unwrap()["can"]["terminal"]["near"].clone();
    let far = |f: &SessionFace| serde_json::to_value(f).unwrap()["can"]["terminal"]["far"].clone();
    let t = |k: &str| copy_core::copy_text(k, &[]);
    let front = serde_json::json!({"go": "front", "said": t("beTerminal.go.front")});
    let attach = serde_json::json!({"go": "here", "said": t("beTerminal.go.here"), "open": {"kind": "attach"}});
    let resume = serde_json::json!({"go": "here", "said": t("beTerminal.go.resume"), "open": {"kind": "resume"}});
    let no = |why: String| serde_json::json!({"go": "no", "why": why});
    let can_front = cfg!(any(windows, target_os = "linux"));
    let near_front = if can_front {
        front.clone()
    } else {
        attach.clone()
    };
    let live = |host| {
        SessionFace::of(Life::Live {
            activity: None,
            host,
        })
    };

    let managed = live(Some(Housed::In(TerminalHost::Managed)));
    assert_eq!(
        (near(&managed), far(&managed)),
        (attach.clone(), attach.clone()),
        "托管"
    );
    let reading = SessionFace::of(Life::Reading {
        host: Some(Housed::In(TerminalHost::Managed)),
    });
    assert_eq!(
        (near(&reading), far(&reading)),
        (attach.clone(), attach.clone()),
        "托管 · 记录还没读完"
    );
    for (what, f) in [
        ("住在 tmux 里", live(Some(Housed::In(TerminalHost::Tmux)))),
        ("住哪判不了", live(None)),
        ("可重连", SessionFace::of(Life::Reconnectable)),
    ] {
        assert_eq!(
            (near(&f), far(&f)),
            (near_front.clone(), attach.clone()),
            "{what}"
        );
    }
    let bare = live(Some(Housed::Bare));
    let not_ours = no(t("terminal.empty.notOurs"));
    assert_eq!(far(&bare), not_ours, "不在宿主里：别处没有终端可接");
    assert_eq!(
        near(&bare),
        if can_front {
            front.clone()
        } else {
            not_ours.clone()
        },
        "不在宿主里：这台上只拉得了窗"
    );

    let ended = SessionFace::of(Life::Ended);
    assert_eq!(
        (near(&ended), far(&ended)),
        (resume.clone(), resume.clone()),
        "已结束：先恢复再开"
    );
    let gone = no(t("sessionState.gone.tooltip"));
    let g = SessionFace::of(Life::Gone);
    assert_eq!((near(&g), far(&g)), (gone.clone(), gone), "记录没了那一态");
    let unclear = no(t("runs.state.unknown"));
    let u = SessionFace::of(Life::Unclear);
    assert_eq!((near(&u), far(&u)), (unclear.clone(), unclear), "说不清");
    let off = face_core::book::unseen(crate::common::cells::Words("gpu-01 离线".into()));
    assert_eq!(
        (near(&off), far(&off)),
        (no("gpu-01 离线".into()), no("gpu-01 离线".into())),
        "看不见"
    );

    // pidfile 说接进去没有意义 ⇒「在这里开」不能，拉窗照旧；没说 · 说能 ⇒ 照原样。
    let unattachable = no(t("sessionState.unattachable.why"));
    let r = SessionFace::of(Life::Reconnectable).attachable(Some(false));
    assert_eq!(far(&r), unattachable.clone());
    assert_eq!(
        near(&r),
        if can_front {
            front.clone()
        } else {
            unattachable.clone()
        }
    );
    let m = live(Some(Housed::In(TerminalHost::Managed))).attachable(Some(false));
    assert_eq!((near(&m), far(&m)), (unattachable.clone(), unattachable));
    for a in [None, Some(true)] {
        let r = SessionFace::of(Life::Reconnectable).attachable(a);
        assert_eq!(far(&r), attach, "{a:?}");
    }

    // 句柄：宣告时判出的那一个跟着「在这里开」走；判不出 ⇒ 缺；不在「在这里开」那一形里 ⇒ 不加。
    let c = SessionContainer::Tmux {
        terminal: Some("tmux-3-7".into()),
    };
    let f = SessionFace::of(Life::live(None, Some(&c))).housed_in(Some(&c));
    assert_eq!(
        far(&f)["open"],
        serde_json::json!({"kind": "attach", "terminal": "tmux-3-7"})
    );
    let c = SessionContainer::Managed {
        terminal: Some("h1".into()),
    };
    let f = SessionFace::of(Life::live(None, Some(&c))).housed_in(Some(&c));
    assert_eq!(
        (
            near(&f)["open"]["terminal"].clone(),
            far(&f)["open"]["terminal"].clone()
        ),
        (serde_json::json!("h1"), serde_json::json!("h1"))
    );
    let e = SessionFace::of(Life::Ended).housed_in(Some(&c));
    assert_eq!(far(&e), resume, "已结束：没有终端可接，不带句柄");
}
