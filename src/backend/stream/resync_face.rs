//! **`resync`（手动对齐）的帧面宿主** —— 薄壳，本体在 `observe/watcher.rs::resync`。
//!
//! 住命令表这一层（`stream/`）：回包里 `uncancellable` 那一栏从命令表派生（功能不许引 `stream`）；不住 `stream/inbound/`：那一目录不许出现 `observe::`，`control/` 不许引用 `observe/`（`layering_guard`）。
//! 用户按钮触发、不是节拍：零定时器铁律不变。

use crate::wire::typed::{codes, Blocking, Command, Cx};

/// `resync` 的入参。
#[derive(Debug, Default, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ResyncReq {
    /// 可缺：给了 ⇒ 只对那一个会话（关卡 2「对齐后重试」，不许空串）；不给 / `null` ⇒ 整机。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) sid: Option<String>,
}

/// `resync` 的回包：这一趟的差异 ＋ 这台**当下**的能力事实（与 hello 那两格同一个函数、同形）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Resynced {
    /// 这次补宣告的会话数。
    pub(crate) added: usize,
    /// 补移除的会话数（各份 watcher 取最大：看的是同一台机器）。
    pub(crate) removed: usize,
    /// 这次真写了几处 `@ccm_sid`（值一样的不写）。
    pub(crate) retagged: usize,
    /// 在跟的会话这次从游标补读出几行（带 `sid` 只数那一个；各份 watcher 相加）。
    pub(crate) caught_up: usize,
    /// 几份 watcher 做完了对齐（常驻后端每条连接一份 ＋ 空转那一份）。
    pub(crate) watchers: usize,
    /// 这台当下接得下但做不到的命令（同 hello 那一格）：握手之后才装上 tmux 的机器，靠「重新对齐」让客户端认出来。
    pub(crate) unavailable: Vec<crate::wire::Unavailable>,
    /// 这台当下开跑之后打不断的命令，按字母排（同 hello 那一格）。
    pub(crate) uncancellable: Vec<String>,
}

codes! {
    /// `resync` 自己没有别的码。
    pub(crate) enum ResyncErr {}
}

/// `resync`：手动对齐（整机，或 `sid` 只对一个会话），回差异。用户按钮触发、不是节拍。
pub(crate) struct ResyncCmd;

impl Command for ResyncCmd {
    const NAME: &'static str = "resync";
    type Req = ResyncReq;
    type Reply = Resynced;
    type Err = ResyncErr;
}

impl Blocking for ResyncCmd {
    fn run(req: ResyncReq, _cx: &Cx) -> Result<Resynced, ResyncErr> {
        if req.sid.as_deref() == Some("") {
            return Err(ResyncErr::BadArgs(crate::common::contract::malformed(
                "`sid` must be a non-empty string when given",
            )));
        }
        let (d, watchers) = crate::observe::watcher::resync(req.sid.as_deref());
        Ok(Resynced {
            added: d.added,
            removed: d.removed,
            retagged: d.retagged,
            caught_up: d.caught_up,
            watchers,
            unavailable: crate::unavailable_here(),
            uncancellable: crate::stream::inbound::uncancellable(),
        })
    }
}
