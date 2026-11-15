//! 开发调度的调度端：主机表 · 项目配置 · 队 · 挑机器 · 经连接交给编译节点。
//!
//! 命令行：`devsched [--local] [--config <目录>] [--] <命令…>`，在要编的那棵树里任一处起；`devsched --prepare <台>` 照项目的工具清单准备那台（`tools`）。
//! 退出码：作业跑完 ⇒ 命令自己的；调度自己收尾 ⇒ 标准错误末尾一行「[devsched] <类>：…」，
//! 78 本机准备不成 · 79 命令退 0 而有跳过的段 · 2 用法或配置不对。

pub mod config;
pub mod dispatch;
pub mod env;
pub mod gate;
pub mod git;
pub mod local;
pub mod plan;
pub mod queue;
pub mod rewrite;
pub mod sync;
#[cfg(test)]
#[path = "../../../../tests/devsched/sched/testkit.rs"]
pub(crate) mod testkit;
pub mod tools;

use std::io::Write;
use std::path::{Path, PathBuf};

/// 命令行解出来的。
#[derive(Debug, Clone, PartialEq)]
pub struct Cli {
    pub local: bool,
    pub config: Option<PathBuf>,
    /// `--prepare <台>`：照项目的工具清单准备那台（不带命令）
    pub prepare: Option<String>,
    pub argv: Vec<String>,
}

pub const USAGE: &str =
    "用法：devsched [--local] [--config <目录>] [--] <命令…> ｜ devsched [--config <目录>] --prepare <台>";

pub fn parse_args(args: &[String]) -> Result<Cli, String> {
    let mut cli = Cli {
        local: false,
        config: None,
        prepare: None,
        argv: Vec::new(),
    };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--local" => cli.local = true,
            "--prepare" => {
                i += 1;
                let h = args.get(i).ok_or("--prepare 后面要一台的名字")?;
                cli.prepare = Some(h.clone());
            }
            "--config" => {
                i += 1;
                let d = args.get(i).ok_or("--config 后面要一个目录")?;
                cli.config = Some(PathBuf::from(d));
            }
            "--" => {
                i += 1;
                break;
            }
            _ if a.starts_with("--config=") => {
                cli.config = Some(PathBuf::from(&a["--config=".len()..]))
            }
            _ => break,
        }
        i += 1;
    }
    cli.argv = args[i..].to_vec();
    match (&cli.prepare, cli.argv.is_empty()) {
        (Some(_), false) => Err("--prepare 不带命令".into()),
        (None, true) => Err(USAGE.into()),
        _ => Ok(cli),
    }
}

/// `--prepare <台>`：这棵树的项目配置 `tools` 指的清单，照它准备那台（见 `tools`）。
fn prepare_cli(
    cfg: &config::Config,
    host: &str,
    state_dir: &Path,
    cwd: &Path,
    err: &mut dyn Write,
) -> i32 {
    let Some((project, root)) = plan::find_tree(cwd, &cfg.projects) else {
        let _ = writeln!(
            err,
            "[devsched] 用法：这里认不出是哪个项目的树（--prepare 要在树里起）"
        );
        return 2;
    };
    let rel = project.get("tools");
    if rel.is_empty() {
        let _ = writeln!(
            err,
            "[devsched] 配置：项目 {} 没写 tools（工具清单在树里的相对路径）",
            project.name
        );
        return 2;
    }
    let text = match std::fs::read_to_string(root.join(rel)) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(err, "[devsched] 配置：读不了工具清单 {rel}：{e}");
            return 2;
        }
    };
    let tools = match tools::parse(&text) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(err, "[devsched] 配置：{rel} {e}");
            return 2;
        }
    };
    let Some(h) = cfg.hosts.get(host) else {
        let _ = writeln!(err, "[devsched] 用法：主机表里没有 {host}");
        return 2;
    };
    tools::prepare(h, &tools, state_dir, &sync::RealRunner::default(), err)
}

/// 照命令行跑一趟（真连接 · 真 rsync · 真本机那一支）。
pub fn run_cli(
    args: &[String],
    vars: &plan::Vars,
    cwd: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let cli = match parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(err, "[devsched] 用法：{e}");
            return 2;
        }
    };
    let Some(dir) = config::config_dir(cli.config.as_deref(), vars) else {
        let _ = writeln!(
            err,
            "[devsched] 配置：不知道配置目录（没有 HOME，也没给 --config / DEVSCHED_CONFIG）"
        );
        return 2;
    };
    let cfg = match config::Config::load(&dir) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(err, "[devsched] 配置：{e}");
            return 2;
        }
    };
    let Some(sdir) = queue::state_dir(vars) else {
        let _ = writeln!(
            err,
            "[devsched] 本机准备不成：不知道状态目录（没有 HOME，也没给 DEVSCHED_STATE）"
        );
        return dispatch::EXIT_LOCAL_PREP;
    };
    if let Some(h) = &cli.prepare {
        return prepare_cli(&cfg, h, &sdir, cwd, err);
    }
    let state = queue::State::new(sdir);
    let (plan, notes) = plan::make_plan(&cfg.projects, cwd, &cli.argv, vars, cli.local);
    for n in notes {
        let _ = writeln!(err, "{} {n}", dispatch::NOTE);
    }
    let runner = sync::RealRunner::default();
    let syncer = sync::RsyncSync {
        state: state.clone(),
        runner: &runner,
    };
    let local = local::RealLocal::default();
    let mut s = dispatch::Sched {
        cfg,
        state,
        connector: &dispatch::SshConnector,
        syncer: &syncer,
        local: &local,
        out,
        err,
        timing: dispatch::Timing::default(),
    };
    s.run(plan)
}

/// 命令行入口；返回进程退出码。
pub fn main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vars: plan::Vars = std::env::vars().collect();
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[devsched] 本机准备不成：读不了当前目录：{e}");
            return dispatch::EXIT_LOCAL_PREP;
        }
    };
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    run_cli(&args, &vars, &cwd, &mut out, &mut err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::tests::sv;

    #[test]
    fn args() {
        let c = parse_args(&sv(&[
            "--local", "--config", "/c", "cargo", "test", "--local",
        ]))
        .unwrap();
        assert!(c.local);
        assert_eq!(c.config, Some(PathBuf::from("/c")));
        assert_eq!(
            c.argv,
            sv(&["cargo", "test", "--local"]),
            "命令里的 --local 不吃"
        );
        let c = parse_args(&sv(&["--config=/d", "--", "--local"])).unwrap();
        assert_eq!(c.config, Some(PathBuf::from("/d")));
        assert_eq!(c.argv, sv(&["--local"]));
        assert!(parse_args(&sv(&["--local"])).is_err());
        assert!(parse_args(&sv(&["--config"])).is_err());
        let c = parse_args(&sv(&["--config", "/c", "--prepare", "hn"])).unwrap();
        assert_eq!(c.prepare.as_deref(), Some("hn"));
        assert!(c.argv.is_empty());
        assert!(parse_args(&sv(&["--prepare"])).is_err());
        assert!(
            parse_args(&sv(&["--prepare", "hn", "cargo"])).is_err(),
            "准备不带命令"
        );
    }

    #[test]
    fn prepare_without_tools_key_says_so() {
        let t = testkit::TempDir::new("cliprep");
        std::fs::create_dir_all(t.path().join("cfg/projects")).unwrap();
        std::fs::write(t.path().join("cfg/hosts"), "a 1 ssh:a base=/srv/ds\n").unwrap();
        std::fs::write(t.path().join("cfg/projects/p.conf"), "marker = m\n").unwrap();
        std::fs::create_dir_all(t.path().join("tree")).unwrap();
        std::fs::write(t.path().join("tree/m"), "").unwrap();
        let mut v = plan::Vars::new();
        v.insert(
            "DEVSCHED_STATE".into(),
            t.path().join("st").display().to_string(),
        );
        let (mut o, mut e) = (Vec::new(), Vec::new());
        let cfg = format!("--config={}", t.path().join("cfg").display());
        let code = run_cli(
            &sv(&[&cfg, "--prepare", "a"]),
            &v,
            &t.path().join("tree"),
            &mut o,
            &mut e,
        );
        assert_eq!(code, 2);
        assert!(
            String::from_utf8_lossy(&e).contains("tools"),
            "{}",
            String::from_utf8_lossy(&e)
        );
    }

    #[test]
    fn missing_config_is_said_on_the_last_line() {
        let t = testkit::TempDir::new("cli");
        let mut v = plan::Vars::new();
        v.insert(
            "DEVSCHED_CONFIG".into(),
            t.path().join("none").display().to_string(),
        );
        v.insert(
            "DEVSCHED_STATE".into(),
            t.path().join("state").display().to_string(),
        );
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(run_cli(&sv(&["true"]), &v, t.path(), &mut out, &mut err), 2);
        assert!(String::from_utf8_lossy(&err)
            .trim_end()
            .lines()
            .last()
            .unwrap()
            .starts_with("[devsched] 配置："));
    }

    #[test]
    fn unknown_tree_runs_locally_with_own_exit_code() {
        let t = testkit::TempDir::new("cli2");
        let _g = testkit::REAL_LOCAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::fs::create_dir_all(t.path().join("cfg")).unwrap();
        std::fs::write(
            t.path().join("cfg/hosts"),
            "remote 1 ssh:nowhere base=/x\nl 1 local\n",
        )
        .unwrap();
        let mut v = plan::Vars::new();
        v.insert(
            "DEVSCHED_CONFIG".into(),
            t.path().join("cfg").display().to_string(),
        );
        v.insert(
            "DEVSCHED_STATE".into(),
            t.path().join("state").display().to_string(),
        );
        let mut out = Vec::new();
        let mut err = Vec::new();
        // 认不出项目 ⇒ 只本机，不连远端
        assert_eq!(
            run_cli(
                &sv(&["sh", "-c", "exit 5"]),
                &v,
                t.path(),
                &mut out,
                &mut err
            ),
            5
        );
    }
}
