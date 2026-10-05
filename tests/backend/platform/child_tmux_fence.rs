//! [`crate::platform::child`] 起子进程前那一道的**测试构建那一份**（体住 `tests/`，生产树里只留空桩）。
//!
//! 裸名 `tmux`（生产那几处不带 `-L` / `-S`、靠环境找 server 的调用）落到本进程自己的空 socket 目录：
//! 缺省 server 是开发机上用户正在用的那台，单测不许连上去（只读也不许；会话快照那一发会连）。
//! 显式 `-S` 的不受影响；调用方自己交了 `TMUX_TMPDIR` 的照它的。`$TMUX` 压过 `TMUX_TMPDIR`，一起摘。

use std::ffi::{OsStr, OsString};
use std::process::Command;

pub(super) fn apply(program: &OsStr, envs: &[(OsString, Option<OsString>)], c: &mut Command) {
    if program != "tmux" || envs.iter().any(|(k, _)| k == "TMUX_TMPDIR") {
        return;
    }
    let dir = std::env::temp_dir().join(format!("ccm-unit-tmux-{}", std::process::id()));
    // 目录要先在：tmux 建不出 `$TMUX_TMPDIR/tmux-<uid>` 时会退回 `/tmp`，又落回缺省那台。
    let _ = std::fs::create_dir_all(&dir);
    c.env("TMUX_TMPDIR", dir)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE");
}
