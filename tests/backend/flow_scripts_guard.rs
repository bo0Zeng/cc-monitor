//! 流程工具住在仓里（10-10 flow3）：门禁入口 `gate-clean.sh` · 按改动挑格 `gate-affected.sh`（＋ `gate-affected/plan.py` · `gate-affected/map`）·
//! 重活隔离 `heavy.sh` · 推候选分支 `push-candidate.sh`，连同早先进仓的 `quick-check.sh` · `bump-build-id.sh`。
//!
//! 从前它们在仓外、只认一台机器、没有版本：门禁入口的 tmux 隔离改坏了几天没人发现，挑格的表与仓里的格对不上也只有跑起来才知道。
//! 这里钉四件事：
//! - 每一份都在，且不写家目录的绝对路径（锁 · 槽位 · 日志走 `GATE_STATE_DIR`，仓根走 `git rev-parse`）；
//! - 带 `--help` 的几份能跑、退 0、印用法；
//! - 挑格那一半拿仓里这张表对仓里这份 `gate.sh` 出得了计划（表过期它退 2，这条就红）；
//! - `heavy.sh` 在没有 `systemd-run --user` 的机器上（CI · Windows · 没有 user session）退成直接跑，并说一声。

#[cfg(all(test, unix))]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    /// 流程工具全集（相对仓根）。
    const FLOW_FILES: &[&str] = &[
        "tests/scripts/gate-clean.sh",
        "tests/scripts/gate-affected.sh",
        "tests/scripts/gate-affected/plan.py",
        "tests/scripts/gate-affected/map",
        "tests/scripts/heavy.sh",
        "tests/scripts/push-candidate.sh",
        "tests/scripts/quick-check.sh",
        "tests/scripts/bump-build-id.sh",
    ];

    /// 带 `--help` 的入口。
    const HELPFUL: &[&str] = &[
        "tests/scripts/gate-clean.sh",
        "tests/scripts/gate-affected.sh",
        "tests/scripts/heavy.sh",
        "tests/scripts/push-candidate.sh",
    ];

    fn root() -> PathBuf {
        crate::guard_support::repo_root()
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("flow-scripts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn bash(script: &Path, args: &[&str], dir: &Path) -> Output {
        Command::new("bash")
            .arg(script)
            .args(args)
            .current_dir(dir)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .output()
            .unwrap()
    }

    #[test]
    fn the_flow_scripts_live_in_the_repo_and_name_no_home_directory() {
        for rel in FLOW_FILES {
            let p = root().join(rel);
            assert!(p.is_file(), "流程工具不在仓里：{rel}");
            let text = std::fs::read_to_string(&p).unwrap();
            for bad in ["/home/", "/Users/", "~/", "C:\\Users", "/root/"] {
                assert!(
                    !text.contains(bad),
                    "{rel} 里写了家目录的绝对路径（`{bad}`）—— 仓根用 git rev-parse，锁与日志用 GATE_STATE_DIR"
                );
            }
        }
    }

    #[test]
    fn each_flow_entry_answers_help() {
        let d = scratch("help");
        for rel in HELPFUL {
            let out = bash(&root().join(rel), &["--help"], &d);
            let said = String::from_utf8_lossy(&out.stdout);
            assert!(
                out.status.success() && said.contains("用法"),
                "{rel} --help 没印用法或没退 0（{:?}）：{said}{}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 挑格那一半拿仓里的表对仓里的 `gate.sh`：表里每个格名都得是 gate.sh 盘上的格，不然它退 2。
    #[test]
    fn the_plan_reads_this_map_against_this_gate() {
        let plan = |changes: &str, trust: bool| -> (i32, String) {
            let mut c = Command::new("python3");
            c.arg(root().join("tests/scripts/gate-affected/plan.py"))
                .arg(root())
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .env_remove("GATE_AFFECTED_MAP");
            if trust {
                c.arg("--trust-ci");
            }
            let mut ch = c.spawn().expect("python3 起不来");
            use std::io::Write;
            ch.stdin.take().unwrap().write_all(changes.as_bytes()).unwrap();
            let out = ch.wait_with_output().unwrap();
            (
                out.status.code().unwrap_or(-1),
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ),
            )
        };
        let line = |said: &str, key: &str| -> String {
            said.lines()
                .find_map(|l| l.strip_prefix(key))
                .unwrap_or_else(|| panic!("计划里没有 `{key}` 那一行：{said}"))
                .trim()
                .to_string()
        };
        // 只动仓根 README ⇒ 只跑恒跑那组（含 tsc），不退全量。
        let (rc, said) = plan("M\tREADME.md\n", true);
        assert_eq!(rc, 0, "挑格那一步停了（表与 gate.sh 对不上？）：{said}");
        assert_eq!(line(&said, "PLAN-MODE:"), "affected", "{said}");
        let cells = line(&said, "PLAN-CELLS:");
        assert!(cells.split_whitespace().any(|c| c == "tsc"), "恒跑那组没挑上：{said}");
        assert!(
            !cells.split_whitespace().any(|c| c == "mobile"),
            "只动 README 却挑上了手机那格：{said}"
        );
        // 动了门禁自己 ⇒ 全量。
        let (rc, said) = plan("M\ttests/scripts/gate.sh\n", false);
        assert_eq!(rc, 0, "{said}");
        assert_eq!(line(&said, "PLAN-MODE:"), "full", "{said}");
        // 表不认识的路径 ⇒ 全量（宁可多跑）。
        let (rc, said) = plan("A\tnowhere/never.txt\n", false);
        assert_eq!(rc, 0, "{said}");
        assert_eq!(line(&said, "PLAN-MODE:"), "full", "{said}");
    }

    /// PATH 里没有 `systemd-run` ⇒ 直接跑，并在 stderr 说一声；命令的退出码照传。
    #[test]
    fn heavy_runs_directly_where_there_is_no_systemd_run() {
        let d = scratch("heavy");
        let bin = d.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        // 只放脚本要的几样，故意不放 systemd-run / systemctl。
        for tool in ["bash", "sh", "flock", "seq", "sleep", "mkdir", "echo", "cat", "true", "false"] {
            let found = Command::new("bash")
                .args(["-c", &format!("type -P {tool}")])
                .output()
                .unwrap();
            let at = String::from_utf8_lossy(&found.stdout).trim().to_string();
            if at.starts_with('/') {
                std::os::unix::fs::symlink(&at, bin.join(tool)).unwrap();
            }
        }
        let run = |args: &[&str]| -> Output {
            Command::new(bin.join("bash"))
                .arg(root().join("tests/scripts/heavy.sh"))
                .args(args)
                .current_dir(&d)
                .env("PATH", &bin)
                .env("GATE_STATE_DIR", d.join("state"))
                .env_remove("TMUX")
                .env_remove("TMUX_PANE")
                .output()
                .unwrap()
        };
        let out = run(&["echo", "heavy-said-this"]);
        let said = String::from_utf8_lossy(&out.stdout);
        let note = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "没有 systemd-run 时没跑成：{said}{note}");
        assert_eq!(said.trim(), "heavy-said-this", "命令的输出没原样出来：{said}");
        assert!(
            note.contains("systemd-run"),
            "退成直接跑却没说一声：{note}"
        );
        assert!(
            d.join("state/heavy-slots").is_dir(),
            "槽位没放进 GATE_STATE_DIR"
        );
        let out = run(&["false"]);
        assert_eq!(out.status.code(), Some(1), "命令的退出码没照传");
        let _ = std::fs::remove_dir_all(&d);
    }
}
