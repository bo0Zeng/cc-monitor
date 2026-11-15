//! 终端那三样各自一格（`can.front` 拉前 · `can.attach` 在终端里打开 · `can.view` 看它的终端）的判据：桌面 tab 行尾 ↗ · 右键 · 会话头 ·
//! 快捷键 · 手机顶栏各读各的那一格，由会话住的那台判；看的这一端的壳拉不拉得了窗不写进脸（那一端自己判）。

/// 照一生 ＋ 宿主：
/// - 拉前：活着 · 读记录中 · 程序退了 tmux 会话还在 ⇒ 能，宿主是哪种都一样（托管终端 · 不在宿主里的也能：拉显示它的那个窗口）；
/// - 在终端里打开 · 看它的终端：tmux · 托管 · 可重连 ⇒ 能（看它的终端 ＝ 接进它住的那个终端）；不在任何宿主里 ⇒ 不能、写无终端可接；
///   住哪说不清 ⇒ 不能、写无法确定所在终端（接回认的是名单里挂着它的那一行，说不清就是名单里没有它）；
/// - 已结束 ⇒ 拉前 · 在终端里打开不能，看它的终端「先恢复再开」；记录已不在 · 说不清 · 看不见 ⇒ 三样都不能、写那一态那一句；
/// - pidfile 说接进去没有意义（`attachable: false`）⇒ 在终端里打开 · 看它的终端不能，拉前照旧；
/// - 宣告时判出的终端句柄跟着「看它的终端」走（手机照它附着，不另问那台在哪）。
#[test]
fn the_three_terminal_cells_each_say_whether_they_can() {
    use crate::wire::{
        Housed, Life, SessionContainer, SessionFace, SessionFaceExt as _, TerminalHost,
    };
    let cell = |f: &SessionFace, k: &str| serde_json::to_value(f).unwrap()["can"][k].clone();
    let three = |f: &SessionFace| (cell(f, "front"), cell(f, "attach"), cell(f, "view"));
    let t = |k: &str| copy_core::copy_text(k, &[]);
    let yes = serde_json::json!({"can": "yes"});
    let no = |why: String| serde_json::json!({"can": "no", "why": why});
    let view_attach = serde_json::json!({"can": "yes", "said": t("beTerminal.go.here"), "open": {"kind": "attach"}});
    let view_resume = serde_json::json!({"can": "yes", "said": t("beTerminal.go.resume"), "open": {"kind": "resume"}});
    let live = |host| {
        SessionFace::of(Life::Live {
            activity: None,
            host,
        })
    };

    for (what, f) in [
        ("住在 tmux 里", live(Some(Housed::In(TerminalHost::Tmux)))),
        ("托管", live(Some(Housed::In(TerminalHost::Managed)))),
        (
            "托管 · 记录还没读完",
            SessionFace::of(Life::Reading {
                host: Some(Housed::In(TerminalHost::Managed)),
            }),
        ),
        ("可重连", SessionFace::of(Life::Reconnectable)),
    ] {
        assert_eq!(
            three(&f),
            (yes.clone(), yes.clone(), view_attach.clone()),
            "{what}"
        );
    }
    let bare = live(Some(Housed::Bare));
    let no_host = no(t("beTerminal.why.noHost"));
    assert_eq!(
        three(&bare),
        (yes.clone(), no_host.clone(), no_host.clone()),
        "不在宿主里：只拉得了它的窗"
    );
    // 住哪说不清（没打上标那几形 · 没有 tmux 的这台上没认出托管的）：只拉得了它的窗，另两格照实说说不清，不说「不在」。
    let unclear_host = no(t("beTerminal.why.unclear"));
    for reading in [false, true] {
        assert_eq!(
            three(&super::live_face(None, None, reading)),
            (yes.clone(), unclear_host.clone(), unclear_host.clone()),
            "住哪说不清（reading={reading}）：只拉得了它的窗"
        );
    }

    let ended = SessionFace::of(Life::Ended);
    let (front, attach, view) = three(&ended);
    assert_eq!(front["can"], "no", "已结束：没有窗口可拉");
    assert_eq!(attach["can"], "no", "已结束：没有终端可接");
    assert_eq!(view, view_resume, "已结束：看它的终端先恢复再开");
    let gone = no(t("sessionState.gone.tooltip"));
    let g = SessionFace::of(Life::Gone);
    assert_eq!(three(&g).2, gone, "记录没了那一态");
    assert_eq!(three(&g).0["can"], "no");
    let unclear = no(t("runs.state.unknown"));
    let u = SessionFace::of(Life::Unclear);
    assert_eq!(
        three(&u),
        (unclear.clone(), unclear.clone(), unclear),
        "说不清"
    );
    let off = face_core::book::unseen(crate::common::cells::Words("gpu-01 离线".into()));
    let offline = no("gpu-01 离线".into());
    assert_eq!(
        three(&off),
        (offline.clone(), offline.clone(), offline),
        "看不见"
    );

    // pidfile 说接进去没有意义 ⇒ 在终端里打开 · 看它的终端不能，拉前照旧；没说 · 说能 ⇒ 照原样。
    let unattachable = no(t("sessionState.unattachable.why"));
    for f in [
        SessionFace::of(Life::Reconnectable),
        live(Some(Housed::In(TerminalHost::Managed))),
    ] {
        assert_eq!(
            three(&f.attachable(Some(false))),
            (yes.clone(), unattachable.clone(), unattachable.clone())
        );
    }
    for a in [None, Some(true)] {
        let r = SessionFace::of(Life::Reconnectable).attachable(a);
        assert_eq!(
            three(&r),
            (yes.clone(), yes.clone(), view_attach.clone()),
            "{a:?}"
        );
    }

    // 句柄：宣告时判出的那一个跟着「看它的终端」走；判不出 ⇒ 缺；不是接进去那一形 ⇒ 不加。
    let c = SessionContainer::Tmux {
        terminal: Some("tmux-3-7".into()),
    };
    let f = SessionFace::of(Life::live(None, Some(&c))).housed_in(Some(&c));
    assert_eq!(
        cell(&f, "view")["open"],
        serde_json::json!({"kind": "attach", "terminal": "tmux-3-7"})
    );
    let c = SessionContainer::Managed {
        terminal: Some("h1".into()),
    };
    let f = SessionFace::of(Life::live(None, Some(&c))).housed_in(Some(&c));
    assert_eq!(
        cell(&f, "view")["open"]["terminal"],
        serde_json::json!("h1")
    );
    let e = SessionFace::of(Life::Ended).housed_in(Some(&c));
    assert_eq!(
        cell(&e, "view"),
        view_resume,
        "已结束：没有终端可接，不带句柄"
    );
}

/// 清单那条路（历史清单 · 搜索 · 分组 · 历史脸 · 一批会话）造的活会话脸照它真住的地方写宿主，与标签页那张脸同一个判法：
/// 托管终端里的活会话接得进去、句柄跟着「看它的终端」走；名单里不挂它 ⇒ 住哪说不清、接不进去（照实说说不清）；
/// 已结束那几段不看宿主。
#[test]
fn a_listed_live_face_tells_where_it_really_lives() {
    use crate::wire::{Life, SessionContainer, SessionFace, SessionFaceExt as _, WireLife};
    let cell = |f: &SessionFace, k: &str| serde_json::to_value(f).unwrap()["can"][k].clone();
    let t = |k: &str| copy_core::copy_text(k, &[]);
    let managed = SessionContainer::Managed {
        terminal: Some("h1".into()),
    };
    let tmux = SessionContainer::Tmux {
        terminal: Some("tmux-3-7".into()),
    };
    let live = WireLife::Live { activity: None };

    let f = SessionFace::listed(&live, Some(&managed));
    assert_eq!(
        cell(&f, "attach"),
        serde_json::json!({"can": "yes"}),
        "托管的活会话接得进去"
    );
    assert_eq!(
        cell(&f, "view")["open"],
        serde_json::json!({"kind": "attach", "terminal": "h1"}),
        "句柄跟着「看它的终端」走"
    );
    let unknown = SessionFace::listed(&live, None);
    let unclear = serde_json::json!({"can": "no", "why": t("beTerminal.why.unclear")});
    assert_eq!(
        (cell(&unknown, "attach"), cell(&unknown, "view")),
        (unclear.clone(), unclear),
        "名单里不挂它：无法确定所在终端，接不进去"
    );

    // 与标签页那张脸同一个判法：同一个一生 ＋ 容器 ⇒ 同一张脸。
    for c in [
        None,
        Some(&managed),
        Some(&tmux),
        Some(&SessionContainer::None),
    ] {
        assert_eq!(
            SessionFace::listed(&live, c),
            SessionFace::of(Life::live(None, c)).housed_in(c),
            "{c:?}"
        );
    }
    // 已结束 · 记录已不在 · 说不清：没有终端可接，不看宿主。
    for l in [
        WireLife::Ended { by: None },
        WireLife::Gone,
        WireLife::Unclear,
    ] {
        assert_eq!(
            SessionFace::listed(&l, Some(&managed)),
            SessionFace::listed(&l, None),
            "{l:?}"
        );
    }
}
