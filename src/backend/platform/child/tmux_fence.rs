//! [`super::Child`] 起子进程前的最后一道 —— 生产构建这一份什么都不做。
//!
//! 测试构建里 `child.rs` 的 `#[cfg_attr(test, path = …)]` 把它换成 `tests/backend/platform/child_tmux_fence.rs`：
//! 裸名 `tmux` 落到本进程自己的空 socket 目录，单测连不上缺省 server（开发机上那是用户正在用的那台）。

use std::ffi::{OsStr, OsString};
use std::process::Command;

pub(super) fn apply(_program: &OsStr, _envs: &[(OsString, Option<OsString>)], _c: &mut Command) {}
