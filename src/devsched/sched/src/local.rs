//! 本机那一支：在原树原目录、一个进程组里起，不替作业设变量；重活照 `heavy_prefix` 原样加在前面。
//! 跑着每 2 秒量本机可用内存 · 内存压力 · 盘剩余，连着两次撞主机表本机那一行的停线（判定同节点，wire 的 `stop` 件）⇒ 整组停掉、回队里。

use crate::config::Host;
use crate::dispatch::{trip_limit, LocalOutcome, LocalRun};
use crate::gate::{local_facts, stop_lines};
use crate::plan::Plan;
use devsched_wire::{Breach, Facts, StopLines, StopWatch};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::Command;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

/// 本机那一支要起的命令数组：重活 ⇒ `heavy_prefix` 按空白拆开 ＋ 原命令。
pub fn local_argv(plan: &Plan) -> Vec<String> {
    let mut v = Vec::new();
    if plan.heavy {
        if let Some(p) = &plan.project {
            v.extend(p.list("heavy_prefix"));
        }
    }
    v.extend(plan.argv.iter().cloned());
    v
}

/// 撞的那一条说成一句（停线跟作业帧带去远端的是同一份 `gate::stop_lines`，判定是 wire 的 `stop` 件）。
fn breach_words(s: &StopLines, b: Breach, f: &Facts) -> String {
    let n = |v: Option<u64>| v.map(|x| x.to_string()).unwrap_or_default();
    match b {
        Breach::Mem => format!("可用内存 {}MiB < {}MiB", n(f.mem), n(s.mem)),
        Breach::Psi => format!(
            "内存压力 avg10 {}% ≥ {}%",
            f.psi.unwrap_or(0.0),
            s.psi.unwrap_or(0.0)
        ),
        Breach::Disk => format!("盘剩余 {}MiB < {}MiB", n(f.disk), n(s.disk)),
    }
}

/// 正在跑的那一组（收到 INT · TERM · HUP 时转给它）。
static GROUP: AtomicI32 = AtomicI32::new(0);

extern "C" fn forward(sig: libc::c_int) {
    let g = GROUP.load(Ordering::SeqCst);
    if g > 0 {
        // SAFETY: kill 是异步信号安全的
        unsafe {
            libc::kill(-g, sig);
        }
    }
}

const FORWARDED: [libc::c_int; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

/// 装上转发，返回原来的处理（跑完照原样装回去）。
fn install_forwarding() -> [libc::sighandler_t; 3] {
    // SAFETY: 处理函数只调 kill
    FORWARDED.map(|s| unsafe { libc::signal(s, forward as *const () as libc::sighandler_t) })
}

fn restore(old: [libc::sighandler_t; 3]) {
    for (s, h) in FORWARDED.iter().zip(old) {
        // SAFETY: 装回原来的处理
        unsafe {
            libc::signal(*s, h);
        }
    }
}

/// 起不来：没有这个命令 · 不能执行是命令自己的错，照 shell 的写法退 127 · 126（说一句途中说明）；别的是本机准备不成。
fn spawn_error(prog: &str, e: std::io::Error) -> LocalOutcome {
    match e.kind() {
        std::io::ErrorKind::NotFound => {
            eprintln!("{} 没有这个命令：{prog}", crate::dispatch::NOTE);
            LocalOutcome::Exit(127)
        }
        std::io::ErrorKind::PermissionDenied => {
            eprintln!("{} 不能执行：{prog}", crate::dispatch::NOTE);
            LocalOutcome::Exit(126)
        }
        _ => LocalOutcome::CannotStart(format!("起不了 {prog}：{e}")),
    }
}

/// 真的本机那一支。
pub struct RealLocal {
    pub check_every: Duration,
}

impl Default for RealLocal {
    fn default() -> Self {
        RealLocal {
            check_every: Duration::from_secs(2),
        }
    }
}

impl LocalRun for RealLocal {
    fn run(&self, host: &Host, plan: &Plan, _slot: u32) -> LocalOutcome {
        let argv = local_argv(plan);
        let Some((prog, args)) = argv.split_first() else {
            return LocalOutcome::CannotStart("命令是空的".into());
        };
        if !plan.cwd.is_dir() {
            return LocalOutcome::CannotStart(format!("起命令的目录没了：{}", plan.cwd.display()));
        }
        // 先装转发再起（起之后、装上之前来的信号，devsched 会按默认死掉、把子进程留成孤儿）
        let old = install_forwarding();
        let forced = plan
            .project
            .as_ref()
            .map(crate::env::forced)
            .unwrap_or_default();
        let mut child = match Command::new(prog)
            .args(args)
            .envs(&forced)
            .current_dir(&plan.cwd)
            .process_group(0)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                restore(old);
                return spawn_error(prog, e);
            }
        };
        let pg = child.id() as i32;
        GROUP.store(pg, Ordering::SeqCst);
        let lines = stop_lines(host);
        let mut watch = StopWatch::new(lines.clone());
        let mut last = Instant::now();
        let started = Instant::now();
        let limit = trip_limit(host, plan);
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break Ok(s),
                Ok(None) => {}
                Err(e) => break Err(e),
            }
            if started.elapsed() >= limit {
                // 整趟时限到：整组停掉（同远端）
                // SAFETY: 停掉我们起的那一组
                unsafe {
                    libc::kill(-pg, libc::SIGKILL);
                }
                let _ = child.wait();
                GROUP.store(0, Ordering::SeqCst);
                restore(old);
                return LocalOutcome::Timeout(limit.as_secs());
            }
            if last.elapsed() >= self.check_every {
                last = Instant::now();
                let f = local_facts(&plan.root);
                if let Some(b) = watch.observe(&f) {
                    // SAFETY: 停掉我们起的那一组
                    unsafe {
                        libc::kill(-pg, libc::SIGKILL);
                    }
                    let _ = child.wait();
                    GROUP.store(0, Ordering::SeqCst);
                    restore(old);
                    return LocalOutcome::Breach(breach_words(&lines, b, &f));
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        GROUP.store(0, Ordering::SeqCst);
        restore(old);
        match status {
            Ok(s) => LocalOutcome::Exit(s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0))),
            Err(e) => LocalOutcome::CannotStart(format!("等不了作业：{e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{fixtures, Config, HostTable};
    use crate::testkit::{plan_in, TempDir};

    #[test]
    fn heavy_argv_is_prefix_split_plus_command() {
        let c = Config::load(&fixtures()).unwrap();
        let t = TempDir::new("local");
        let mut p = plan_in(t.path(), &["gradle", "assembleDebug"]);
        p.project = Some(c.projects[0].clone());
        assert_eq!(local_argv(&p), ["gradle", "assembleDebug"]);
        p.heavy = true;
        assert_eq!(
            local_argv(&p),
            [
                "nice",
                "-n",
                "19",
                "ionice",
                "-c3",
                "env",
                "GRADLE_OPTS=-Dorg.gradle.workers.max=8",
                "gradle",
                "assembleDebug"
            ]
        );
    }

    #[test]
    fn runs_in_tree_with_callers_env_and_own_code() {
        let t = TempDir::new("local2");
        let _g = crate::testkit::REAL_LOCAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let host = HostTable::parse("l 1 local\n").hosts.remove(0);
        // 调度不替作业设变量：作业看到的环境就是本进程的
        let p = plan_in(
            t.path(),
            &["sh", "-c", "pwd > out; env | sort > env; exit 7"],
        );
        assert_eq!(
            RealLocal::default().run(&host, &p, 1),
            LocalOutcome::Exit(7)
        );
        let pwd = std::fs::read_to_string(t.path().join("out")).unwrap();
        assert_eq!(
            std::fs::canonicalize(pwd.trim()).unwrap(),
            std::fs::canonicalize(t.path()).unwrap()
        );
        let env = std::fs::read_to_string(t.path().join("env")).unwrap();
        for (k, _) in std::env::vars() {
            assert!(env.contains(&format!("{k}=")), "{k} 没传下去");
        }
        // 调度没加自己的变量：`DEVSCHED_` 起头的只有本进程本来就带着的（测试自己跑在节点的作业里时有 DEVSCHED_SKIPPED）
        for line in env.lines().filter(|l| l.starts_with("DEVSCHED_")) {
            let (k, v) = line.split_once('=').unwrap();
            assert_eq!(
                std::env::var(k).ok().as_deref(),
                Some(v),
                "调度加了自己的变量 {k}"
            );
        }
    }

    #[test]
    fn sccache_off_project_blanks_rustc_wrapper_locally() {
        let t = TempDir::new("local-sccache");
        let _g = crate::testkit::REAL_LOCAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let host = HostTable::parse("l 1 local\n").hosts.remove(0);
        let mut p = plan_in(
            t.path(),
            &["sh", "-c", "printf '[%s]' \"${RUSTC_WRAPPER-unset}\" > out"],
        );
        p.project
            .as_mut()
            .unwrap()
            .keys
            .insert("sccache".into(), "0".into());
        assert_eq!(
            RealLocal::default().run(&host, &p, 1),
            LocalOutcome::Exit(0)
        );
        assert_eq!(
            std::fs::read_to_string(t.path().join("out")).unwrap(),
            "[]",
            "设成空（盖过 cargo 配置里的 rustc-wrapper），不是没设"
        );
    }

    #[test]
    fn missing_command_is_the_commands_own_127() {
        let t = TempDir::new("local3");
        let _g = crate::testkit::REAL_LOCAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let host = HostTable::parse("l 1 local\n").hosts.remove(0);
        let p = plan_in(t.path(), &["/nonexistent/devsched-xyz"]);
        assert_eq!(
            RealLocal::default().run(&host, &p, 1),
            LocalOutcome::Exit(127)
        );
    }

    #[test]
    fn vanished_cwd_is_not_a_missing_command() {
        let t = TempDir::new("local6");
        let _g = crate::testkit::REAL_LOCAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let host = HostTable::parse("l 1 local\n").hosts.remove(0);
        let p = plan_in(&t.path().join("gone"), &["true"]);
        assert!(matches!(
            RealLocal::default().run(&host, &p, 1),
            LocalOutcome::CannotStart(_)
        ));
    }

    #[test]
    fn signal_handlers_are_put_back_after_a_run() {
        let t = TempDir::new("local5");
        let _g = crate::testkit::REAL_LOCAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let host = HostTable::parse("l 1 local\n").hosts.remove(0);
        let p = plan_in(t.path(), &["true"]);
        // SAFETY: 先定成忽略，跑完读回来看是不是还是忽略，再装回默认
        unsafe { libc::signal(libc::SIGHUP, libc::SIG_IGN) };
        assert_eq!(
            RealLocal::default().run(&host, &p, 1),
            LocalOutcome::Exit(0)
        );
        let after = unsafe { libc::signal(libc::SIGHUP, libc::SIG_DFL) };
        assert_eq!(after, libc::SIG_IGN, "跑完照原样装回去");
    }

    #[test]
    fn past_the_trip_limit_the_group_is_stopped() {
        let t = TempDir::new("local5");
        let _g = crate::testkit::REAL_LOCAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let host = HostTable::parse("l 1 local timeout_s=1\n").hosts.remove(0);
        let p = plan_in(t.path(), &["sh", "-c", "sleep 30"]);
        let r = RealLocal {
            check_every: Duration::from_millis(50),
        }
        .run(&host, &p, 1);
        assert_eq!(r, LocalOutcome::Timeout(1));
    }

    #[test]
    fn breach_twice_stops_the_group() {
        let t = TempDir::new("local4");
        let _g = crate::testkit::REAL_LOCAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // 停线定成不可能不撞（psi ≥ 0）
        let host = HostTable::parse("l 1 local psi=0/0\n").hosts.remove(0);
        if local_facts(t.path()).psi.is_none() {
            return; // 这台量不到内存压力：判不了
        }
        let p = plan_in(t.path(), &["sh", "-c", "sleep 30"]);
        let r = RealLocal {
            check_every: Duration::from_millis(50),
        }
        .run(&host, &p, 1);
        assert!(matches!(r, LocalOutcome::Breach(_)), "{r:?}");
    }

    #[test]
    fn local_stop_lines_read_the_stop_half() {
        let host = HostTable::parse("l 1 local mem=24/2 psi=20/60\n")
            .hosts
            .remove(0);
        let ok = Facts {
            mem: Some(4096),
            psi: Some(10.0),
            ..Default::default()
        };
        assert_eq!(stop_lines(&host).first_hit(&ok), None);
        assert!(stop_lines(&host)
            .first_hit(&Facts {
                mem: Some(1000),
                ..ok.clone()
            })
            .is_some());
        assert!(stop_lines(&host)
            .first_hit(&Facts {
                psi: Some(60.0),
                ..ok
            })
            .is_some());
    }

    #[test]
    fn local_stop_lines_count_disk_too() {
        // 盘：放行 8G、停线 1G（MiB 计）
        let host = HostTable::parse("l 1 local disk=8/1\n").hosts.remove(0);
        let low = Facts {
            disk: Some(500),
            ..Default::default()
        };
        assert!(
            stop_lines(&host).first_hit(&low).is_some(),
            "盘剩余低于停线也算撞"
        );
        let ok = Facts {
            disk: Some(4096),
            ..Default::default()
        };
        assert!(stop_lines(&host).first_hit(&ok).is_none());
    }
}
