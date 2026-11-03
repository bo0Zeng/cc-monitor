//! 「在终端里打开」点不点得了（`can.attach`）的判据：会话头 · 报错卡 · 右键菜单都只读这一格，由核心判。

/// 活着住在托管终端里 ⇒ 能、哪台看都给（`everywhere`）；活着住在 tmux 里 · 住哪判不了 · 程序退了 tmux 会话还在 ⇒ 能、
/// 只在别的机器上看才给（`elsewhere`：本机那个 tmux 会话有 ccm 开的窗口，走 ↗）；不在任何宿主里 · 已结束 · 记录已不在 · 说不清 · 看不见 ⇒ 不能、
/// 写那一态那一句；pidfile 说接进去没有意义（`attachable: false`）⇒ 不能。
#[test]
fn session_frames_say_whether_the_session_can_be_attached() {
    use crate::wire::{Housed, Life, SessionFace, SessionFaceExt as _, TerminalHost};
    let attach = |f: SessionFace| serde_json::to_value(f).unwrap()["can"]["attach"].clone();
    let yes = |from: &str| serde_json::json!({"can": "yes", "from": from});
    let no = |why: String| serde_json::json!({"can": "no", "why": why});
    let live = |host| {
        SessionFace::of(Life::Live {
            activity: None,
            host,
        })
    };
    assert_eq!(
        attach(live(Some(Housed::In(TerminalHost::Managed)))),
        yes("everywhere")
    );
    assert_eq!(
        attach(SessionFace::of(Life::Reading {
            host: Some(Housed::In(TerminalHost::Managed))
        })),
        yes("everywhere")
    );
    assert_eq!(
        attach(live(Some(Housed::In(TerminalHost::Tmux)))),
        yes("elsewhere")
    );
    assert_eq!(attach(live(None)), yes("elsewhere"));
    assert_eq!(
        attach(SessionFace::of(Life::Reconnectable)),
        yes("elsewhere")
    );
    assert_eq!(
        attach(live(Some(Housed::Bare))),
        no(copy_core::copy_text("terminal.empty.notOurs", &[]))
    );
    let ended = copy_core::copy_text("sessionState.ended.name", &[]);
    assert_eq!(
        attach(SessionFace::of(Life::Ended)),
        no(copy_core::copy_text(
            "tabBatch.why.ended",
            &[("ended", &ended)]
        ))
    );
    assert!(attach(SessionFace::of(Life::Gone))["can"] == "no");
    assert_eq!(
        attach(SessionFace::of(Life::Unclear)),
        no(copy_core::copy_text("runs.state.unknown", &[]))
    );
    assert_eq!(
        attach(face_core::book::unseen(crate::common::cells::Words(
            "gpu-01 离线".into()
        ))),
        no("gpu-01 离线".into())
    );
    // pidfile 说接进去没有意义 ⇒ 不能（活着的 · 程序退了那一张都照它）；没说 · 说能 ⇒ 照原样。
    let unattachable = no(copy_core::copy_text("sessionState.unattachable.why", &[]));
    assert_eq!(
        attach(live(Some(Housed::In(TerminalHost::Tmux))).attachable(Some(false))),
        unattachable
    );
    assert_eq!(
        attach(SessionFace::of(Life::Reconnectable).attachable(Some(false))),
        unattachable
    );
    assert_eq!(
        attach(SessionFace::of(Life::Reconnectable).attachable(None)),
        yes("elsewhere")
    );
    assert_eq!(
        attach(SessionFace::of(Life::Reconnectable).attachable(Some(true))),
        yes("elsewhere")
    );
}
