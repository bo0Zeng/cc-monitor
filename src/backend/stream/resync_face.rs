//! **`resync`（手动对齐）的帧面宿主** —— 薄壳，本体在 `observe/watcher.rs::resync`；入参与回包住 `cmd_wire::resync`（壳的 Rust 客户端照它读）。
//!
//! 住命令表这一层（`stream/`）：回包里 `unavailable` 那一栏从命令表派生（功能不许引 `stream`）；不住 `stream/inbound/`：那一目录不许出现 `observe::`，控制面那个包也不依赖观测面。
//! 用户按钮触发、不是节拍：零定时器铁律不变。

use crate::wire::typed::{codes, Command};
use cmd_wire::resync::{ResyncReq, Resynced};

codes! {
    /// `resync` 自己没有别的码。
    pub(crate) enum ResyncErr {}
}

/// `resync`：手动对齐（整机，或 `sid` 只对一个会话），回差异。用户按钮触发、不是节拍。
pub(crate) struct ResyncCmd;

impl Command for ResyncCmd {
    crate::wire::typed::by_call!(ResyncReq);
    type Err = ResyncErr;
}

impl crate::wire::typed::Handle for ResyncCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: ResyncReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<Resynced, ResyncErr> {
        if req.sid.as_deref() == Some("") {
            return Err(ResyncErr::BadArgs(crate::common::contract::malformed(
                "`sid` must be a non-empty string when given",
            )));
        }
        let (d, watchers) = crate::observe::watcher::resync(req.sid.as_deref());
        Ok(Resynced {
            added: d.added,
            removed: d.removed,
            said: said(d.added, d.removed, d.retagged, d.caught_up, watchers),
            unavailable: crate::stream::inbound::unavailable_here(),
        })
    }
}

/// 对齐的结果 ⇒ 一句话：补读了几条（0 ⇒「无遗漏」）接差异（全 0 ⇒「与实际一致」）；没有一份 watcher 在看 ⇒ 只那一句。
pub(crate) fn said(
    added: usize,
    removed: usize,
    retagged: usize,
    caught_up: usize,
    watchers: usize,
) -> String {
    use copy_core::copy_text;
    if watchers == 0 {
        return copy_text("resync.done.nobody", &[]);
    }
    let diff = if added + removed + retagged == 0 {
        copy_text("resync.done.same", &[])
    } else {
        copy_text(
            "resync.done.diff",
            &[
                ("added", &added.to_string()),
                ("removed", &removed.to_string()),
                ("retagged", &retagged.to_string()),
            ],
        )
    };
    let caught = if caught_up == 0 {
        copy_text("resync.done.caughtNone", &[])
    } else {
        copy_text("resync.done.caughtUp", &[("n", &caught_up.to_string())])
    };
    format!("{caught}{diff}")
}
