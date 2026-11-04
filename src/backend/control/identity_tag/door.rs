//! [`super`] 起 tmux 的那个口 —— 生产构建那一份（探测与写都经它）。
//!
//! 测试构建里 `identity_tag.rs` 的 `#[cfg_attr(test, path = …)]` 把整个口换成
//! `tests/backend/control/identity_tag_door.rs`：只交本线程注入的假 tmux，没注入就炸（`INVARIANTS §48.3`）。

#[expect(
    clippy::disallowed_methods,
    reason = "打会话身份 `set-option @ccm_sid`（探测复用 `control/gate.rs` 那一处）：改的是 tmux server 的运行期状态，不写文件"
)]
pub(super) fn tmux() -> crate::platform::child::Child {
    crate::host::tmux::child(None)
}
