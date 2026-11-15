//! [`crate::platform::child`] 起子进程前那一道的**测试构建那一份**（体住 `tests/`，生产树里只留空桩）。
//!
//! 裸名 `tmux`（生产那几处不带 `-L` / `-S`、靠环境找 server 的调用）落到本进程自己的空 socket 目录：
//! 缺省 server 是开发机上用户正在用的那台，单测不许连上去（只读也不许；会话快照那一发会连）。
//! 那个目录建不出 ⇒ panic（不在的 `TMUX_TMPDIR` 让 tmux 退回 `/tmp`）。显式 `-S` 的不受影响；调用方自己交了 `TMUX_TMPDIR` 的照它的。`$TMUX` 压过 `TMUX_TMPDIR`，一起摘。

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

pub(super) fn apply(program: &OsStr, envs: &[(OsString, Option<OsString>)], c: &mut Command) {
    apply_under(&std::env::temp_dir(), program, envs, c);
}

/// [`apply`] 的体：socket 目录建在 `base` 底下（判据交一个建不出的根，不动进程环境）。
pub(super) fn apply_under(
    base: &Path,
    program: &OsStr,
    envs: &[(OsString, Option<OsString>)],
    c: &mut Command,
) {
    if program != "tmux" || envs.iter().any(|(k, _)| k == "TMUX_TMPDIR") {
        return;
    }
    let dir = base.join(format!("ccm-unit-tmux-{}", std::process::id()));
    // 目录要先在：`TMUX_TMPDIR` 指着不在的目录时 tmux 退回 `/tmp`，又落回缺省那台 ⇒ 建不出就停，不放行。
    if let Err(e) = std::fs::create_dir_all(&dir) {
        panic!(
            "子进程闸：建不出裸 tmux 的 socket 目录 {}：{e}",
            dir.display()
        );
    }
    c.env("TMUX_TMPDIR", dir)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE");
}
