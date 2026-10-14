//! 判据用的私有 tmux server（仅测试构建）：`-L` 私有 socket ＋ 私有 `TMUX_TMPDIR` ＋ 不读配置（`-f /dev/null`），
//! 摘掉继承来的 `TMUX` / `TMUX_PANE`；server 的 `HOME` 指这个目录、窗格的 shell 是 `/bin/sh`（不跑用户的 rc）。
//! `-L` 压得过 `$TMUX`，`TMUX_TMPDIR` 再兜一层 ⇒ 用户的默认 server 一发都碰不到。掉了就 `kill-server`、删目录。
//! 两样口：[`SandboxTmux::run`]（判据自己摆场面）· [`SandboxTmux::child`]（交给生产的执行器，它在后面接自己的子命令）。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub(crate) struct SandboxTmux {
    dir: PathBuf,
    sock: String,
}

impl SandboxTmux {
    /// 起一台空 server（带一个占位会话 `keep`，免得最后一个会话关掉时 server 跟着退）。
    pub(crate) fn new(tag: &str) -> SandboxTmux {
        let dir = std::env::temp_dir().join(format!("ccm-sbx-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tmux")).expect("建沙箱目录");
        let sb = SandboxTmux {
            sock: format!("ccm-sbx-{tag}-{}", std::process::id()),
            dir,
        };
        sb.ok(&["new-session", "-d", "-s", "keep", "sleep 600"]);
        sb.ok(&["set-option", "-g", "default-shell", "/bin/sh"]);
        sb
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    fn env_of(&self) -> [(&'static str, PathBuf); 3] {
        [
            ("TMUX_TMPDIR", self.dir.join("tmux")),
            ("HOME", self.dir.clone()),
            ("SHELL", PathBuf::from("/bin/sh")),
        ]
    }

    /// 判据自己敲一条。
    pub(crate) fn run(&self, args: &[&str]) -> Output {
        Command::new("tmux")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env_remove("ENV")
            .envs(self.env_of())
            .args(["-L", &self.sock, "-f", "/dev/null"])
            .args(args)
            .output()
            .expect("tmux 不可执行 —— 本判据要求环境有 tmux（刻意不静默跳过）")
    }

    /// 敲一条、要它成。
    pub(crate) fn ok(&self, args: &[&str]) {
        let o = self.run(args);
        assert!(
            o.status.success(),
            "沙箱 tmux {args:?} 没成：{}",
            String::from_utf8_lossy(&o.stderr)
        );
    }

    /// 敲一条、取它的 stdout（去首尾空白）。
    pub(crate) fn out(&self, args: &[&str]) -> String {
        String::from_utf8_lossy(&self.run(args).stdout)
            .trim()
            .to_string()
    }

    /// 交给生产执行器的那一个：`tmux -L <sock> -f /dev/null`，后面由执行器接子命令。
    pub(crate) fn child(&self) -> crate::platform::child::Child {
        let mut c = crate::platform::child::Child::new("tmux")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .args(["-L", &self.sock, "-f", "/dev/null"]);
        for (k, v) in self.env_of() {
            c = c.env(k, v);
        }
        c
    }

    /// 等到 `check` 成立（每 50 ms 看一次，至多 `ms`）；回最后一次看到的。
    pub(crate) fn wait_for(&self, ms: u64, check: impl Fn(&SandboxTmux) -> bool) -> bool {
        let step = std::time::Duration::from_millis(50);
        for _ in 0..ms.div_ceil(50) {
            if check(self) {
                return true;
            }
            std::thread::sleep(step);
        }
        check(self)
    }
}

impl Drop for SandboxTmux {
    fn drop(&mut self) {
        let _ = self.run(&["kill-server"]);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
