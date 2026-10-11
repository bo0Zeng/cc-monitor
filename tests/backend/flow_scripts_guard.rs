//! 流程工具住在仓里（10-10 flow3）：门禁入口 `gate-clean.sh` · 按改动挑格 `gate-affected.sh`（＋ `gate-affected/plan.py` · `gate-affected/map`）·
//! 重活隔离 `heavy.sh` · 推候选分支 `push-candidate.sh`，连同早先进仓的 `quick-check.sh` · `bump-build-id.sh`。
//!
//! 从前它们在仓外、只认一台机器、没有版本：门禁入口的 tmux 隔离改坏了几天没人发现，挑格的表与仓里的格对不上也只有跑起来才知道。
//! 这里钉四件事：
//! - 每一份都在，且不写家目录的绝对路径（锁 · 槽位 · 日志走 `GATE_STATE_DIR`，仓根走 `git rev-parse`）；
//! - 带 `--help` 的几份能跑、退 0、印用法；
//! - 挑格那一半拿仓里这张表对仓里这份 `gate.sh` 出得了计划（表过期它退 2，这条就红）；
//! - `heavy.sh` 在没有 `systemd-run --user` 的机器上（CI · Windows · 没有 user session）退成直接跑，并说一声；
//! - `quick-check.sh` 的手机那一段：相对 main 改到手机读的东西才跑（经 heavy、gradle 带 `--no-daemon`），没改到就跳过并说一声；
//!   「手机读的东西」那张表（`mobile_reads`）对着 `src/mobile` 里字面写到的仓内路径核，漏一处就红。

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
            ch.stdin
                .take()
                .unwrap()
                .write_all(changes.as_bytes())
                .unwrap();
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
        assert!(
            cells.split_whitespace().any(|c| c == "tsc"),
            "恒跑那组没挑上：{said}"
        );
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
        for tool in [
            "bash", "sh", "flock", "seq", "sleep", "mkdir", "echo", "cat", "true", "false",
        ] {
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
                .env_remove("HEAVY_HELD")
                .env_remove("TMUX")
                .env_remove("TMUX_PANE")
                .output()
                .unwrap()
        };
        let out = run(&["echo", "heavy-said-this"]);
        let said = String::from_utf8_lossy(&out.stdout);
        let note = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "没有 systemd-run 时没跑成：{said}{note}"
        );
        assert_eq!(
            said.trim(),
            "heavy-said-this",
            "命令的输出没原样出来：{said}"
        );
        assert!(note.contains("systemd-run"), "退成直接跑却没说一声：{note}");
        assert!(
            d.join("state/heavy-slots").is_dir(),
            "槽位没放进 GATE_STATE_DIR"
        );
        let out = run(&["false"]);
        assert_eq!(out.status.code(), Some(1), "命令的退出码没照传");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            ok.status.success(),
            "git {args:?}：{}",
            String::from_utf8_lossy(&ok.stderr)
        );
    }

    fn write(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// 一棵假仓：main 上有一份假 gradlew（记下参数、写一份三条的 test-results），工作分支上只改 `changed` 那一条。
    /// 返回 quick-check `--mobile` 的输出与假 gradlew 收到的参数（没被调就是 None）。
    fn quick_mobile(tag: &str, changed: &str) -> (String, String, Option<String>) {
        let d = scratch(tag);
        git(&d, &["init", "-q", "-b", "main"]);
        write(
            &d,
            "src/mobile/gradlew",
            "#!/bin/sh\necho \"$@\" > ../../gradlew-args\nmkdir -p ../../.build/mobile/app/test-results/t\necho '<testsuite name=\"x\" tests=\"3\" failures=\"0\">' > ../../.build/mobile/app/test-results/t/TEST-x.xml\n",
        );
        assert!(Command::new("chmod")
            .args(["+x", "src/mobile/gradlew"])
            .current_dir(&d)
            .status()
            .unwrap()
            .success());
        write(&d, ".gitignore", ".build/\ngradlew-args\n");
        git(&d, &["add", "-A"]);
        git(&d, &["commit", "-q", "-m", "base"]);
        git(&d, &["checkout", "-q", "-b", "work"]);
        write(&d, changed, "changed\n");
        git(&d, &["add", "-A"]);
        git(&d, &["commit", "-q", "-m", "work"]);
        let script = root().join("tests/scripts/quick-check.sh");
        let run = |arg: &str| {
            Command::new("bash")
                .arg(&script)
                .arg(arg)
                .current_dir(&d)
                .env("JAVA_HOME", d.join("no-jdk"))
                .env("ANDROID_HOME", d.join("no-sdk"))
                .env("GRADLE_USER_HOME", d.join("no-gradle"))
                .env("GATE_STATE_DIR", d.join("state"))
                .env_remove("HEAVY_HELD")
                .env_remove("TMUX")
                .env_remove("TMUX_PANE")
                .output()
                .unwrap()
        };
        let plan = String::from_utf8_lossy(&run("--mobile-plan").stdout).into_owned();
        let out = run("--mobile");
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let args = std::fs::read_to_string(d.join("gradlew-args")).ok();
        let _ = std::fs::remove_dir_all(&d);
        (plan, said, args)
    }

    #[test]
    fn quick_check_runs_the_mobile_suite_only_when_mobile_inputs_changed() {
        // 改到手机读的金样 ⇒ 跑：经 gradle、带 --no-daemon，ktlint · detekt · 单测都在，条数读得出来。
        for changed in [
            "tests/__fixtures__/history-list.golden.json",
            "src/mobile/app/src/main/kotlin/X.kt",
        ] {
            let (plan, said, args) = quick_mobile("mobile-run", changed);
            assert!(
                plan.contains(&format!("MOBILE: 跑 —— 改到 {changed}")),
                "{plan}"
            );
            let args = args.unwrap_or_else(|| panic!("改到 {changed} 却没调 gradle：{said}"));
            for want in [
                "--no-daemon",
                "ktlintCheck",
                "detektDebug",
                "testDebugUnitTest",
            ] {
                assert!(
                    args.split_whitespace().any(|a| a == want),
                    "gradle 少了 {want}：{args}"
                );
            }
            assert!(said.contains("3 条"), "条数没读出来：{said}");
            assert!(said.trim_end().contains("QUICK: OK"), "{said}");
        }
        // 没改到 ⇒ 跳过，并说一声；gradle 一次都没调。
        let (plan, said, args) = quick_mobile("mobile-skip", "src/frontend/ui/x.ts");
        assert!(plan.contains("MOBILE: 跳过"), "{plan}");
        assert!(args.is_none(), "没改到手机读的东西却调了 gradle：{said}");
        assert!(
            said.contains("跳过：相对 main 没改到"),
            "跳过了却没说：{said}"
        );
        assert!(said.trim_end().contains("QUICK: OK"), "{said}");
    }

    /// `mobile_reads` 漏没漏：`src/mobile` 的 Kotlin / Gradle 源里字面写到的仓内（手机树外）路径，每一条都得落在表里某一项下。
    /// 按路径段比（`src/backend` 不会被 `src/backend-x` 冒认）；Gradle 脚本里 `rootProject.file("../X")` 的 X 相对的是 `src/`。
    #[test]
    fn the_mobile_reads_cover_every_repo_path_the_mobile_tree_names() {
        let segs = |p: &str| -> Vec<String> {
            p.split('/')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        };
        let qc = std::fs::read_to_string(root().join("tests/scripts/quick-check.sh")).unwrap();
        let line = qc
            .lines()
            .find_map(|l| l.strip_prefix("mobile_reads=("))
            .and_then(|l| l.strip_suffix(')'))
            .expect("quick-check.sh 里没有 `mobile_reads=(…)` 那一行");
        let reads: Vec<Vec<String>> = line.split_whitespace().map(|r| segs(r)).collect();
        assert!(reads.contains(&segs("src/mobile")), "{reads:?}");
        let outside: Vec<Vec<String>> = [
            "tests",
            "src/backend",
            "src/shared",
            "src/frontend",
            "src/common",
            "src/comms",
            "src/doc",
            "src/vendor",
            ".github",
        ]
        .iter()
        .map(|o| segs(o))
        .collect();
        let files =
            guard_core::scan_tree_excluding(&root().join("src/mobile"), &["kt", "kts"], &[]);
        assert!(
            files.len() > 50,
            "只扫到 {} 份手机源 —— 人群坏了",
            files.len()
        );
        let mut named = Vec::new();
        for (f, text) in &files {
            let gradle = f.extension().is_some_and(|e| e == "kts");
            for lit in text.split('"').skip(1).step_by(2) {
                if lit.chars().any(char::is_whitespace) {
                    continue;
                }
                let mut path = segs(lit);
                if path.first().map(String::as_str) == Some("..") {
                    // 模块目录相对的 `../` 还在手机树里；只有 Gradle 脚本的 rootProject 相对 src/。
                    if !gradle {
                        continue;
                    }
                    path[0] = "src".to_string();
                }
                // 模板段（`$name`）起往后不算。
                if let Some(cut) = path.iter().position(|s| s.contains('$')) {
                    path.truncate(cut);
                }
                if path.len() >= 2 && outside.iter().any(|o| path.starts_with(o)) {
                    named.push((
                        f.strip_prefix(root()).unwrap_or(f).display().to_string(),
                        path.join("/"),
                    ));
                }
            }
        }
        assert!(
            named
                .iter()
                .any(|(_, p)| segs(p).starts_with(&segs("tests/__fixtures__"))),
            "一条金样路径都没认出来 —— 扫法坏了：{named:?}"
        );
        let missed: Vec<_> = named
            .iter()
            .filter(|(_, p)| !reads.iter().any(|r| segs(p).starts_with(r)))
            .collect();
        assert!(
            missed.is_empty(),
            "手机树读了这些仓内路径，quick-check 的 mobile_reads 里没有 —— 改了它们 quick-check 不会跑手机那一套：{missed:?}"
        );
    }
}
