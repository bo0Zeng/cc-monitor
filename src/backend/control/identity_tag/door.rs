//! [`super`] 起 tmux 的那个口 —— 生产构建那一份（探测与写都经它）。
//!
//! 测试构建里 `identity_tag.rs` 的 `#[cfg_attr(test, path = …)]` 把整个口换成
//! `tests/backend/control/identity_tag_door.rs`：只交本线程注入的假 tmux，没注入就炸（`INVARIANTS §48.3`）。

pub(super) fn tmux() -> crate::platform::child::Child {
    crate::platform::child::Child::new("tmux")
}
