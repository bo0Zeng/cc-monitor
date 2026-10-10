//! 起子进程原语的行为：期限到点杀整组（孙进程也没了、直接子进程已收尸）· 自有环境无条件摘 · 显式交自有格只有一个口。

use super::*;
use std::path::PathBuf;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-child-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("建临时目录");
    d
}

/// `/proc/<pid>` 还在且不是僵尸 ⇒ 活着；僵尸单独报（没被收尸）。
#[cfg(target_os = "linux")]
fn proc_state(pid: &str) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(") ")
        .and_then(|(_, rest)| rest.chars().next())
}

/// 有界地等某个 pid 从进程表里消失（被杀之后内核与收尸方要一点时间）。
#[cfg(target_os = "linux")]
fn gone_within(pid: &str, tries: u32) -> Result<(), Option<char>> {
    for _ in 0..tries {
        match proc_state(pid) {
            None => return Ok(()),
            Some(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
    Err(proc_state(pid))
}

/// 名单 == 起常驻后端的那一方只交给它自己用的那几格（手写期望，异源：逐个现取各模块的常量）。
#[test]
fn own_envs_are_exactly_what_the_host_hands_the_backend_for_itself() {
    let got: std::collections::BTreeSet<&str> = OWN_ENVS.into_iter().collect();
    let want: std::collections::BTreeSet<&str> = [
        "CCM_LISTEN_PORT",
        "CCM_LISTEN_TOKEN_FILE",
        "CCM_BACKEND_STDERR_LOG",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
    // 远端 `--resident-ensure` 交给常驻载体的那几格 == 这一份 ＋ 中转口（中转口要往下传给窗格里的 ccm）。
    let handed: std::collections::BTreeSet<String> = crate::control::resident::child_env(
        1,
        std::path::Path::new("/h/.cc-monitor/listen-token"),
        std::path::Path::new("/h"),
        &[
            (crate::stream::listen::RELAY_PORT_ENV, "8788".into()),
            (crate::stderr_log::ENV, "~/x.log".into()),
        ],
    )
    .into_iter()
    .map(|(k, _)| k)
    .collect();
    let mut want_handed: std::collections::BTreeSet<String> =
        want.iter().map(|s| s.to_string()).collect();
    want_handed.insert(crate::stream::listen::RELAY_PORT_ENV.to_string());
    assert_eq!(handed, want_handed, "起常驻载体交的 ≠ 自有那一份 ＋ 中转口");
    assert!(
        !OWN_ENVS.contains(&crate::stream::listen::RELAY_PORT_ENV),
        "中转口不许清：窗格里的 ccm 靠它找同机中转，清掉就去连默认口"
    );
}

/// 超时：用时 < 期限 ＋ 余量；它起的孙进程也没了；直接子进程已被收尸（不留僵尸）。
#[cfg(target_os = "linux")]
#[test]
fn a_child_past_its_deadline_is_killed_with_its_whole_group_and_reaped() {
    let d = scratch("timeout");
    let script = format!(
        "sleep 30 & echo $! > {g}; echo $$ > {c}; wait",
        g = d.join("grand").display(),
        c = d.join("child").display()
    );
    let t0 = std::time::Instant::now();
    let r = Child::new("sh")
        .args(["-c", &script])
        .run(Deadline::millis(400));
    let took = t0.elapsed();
    match r {
        Err(ChildFail::TimedOut { after, .. }) => assert_eq!(after, Deadline::millis(400)),
        other => panic!("该超时：{other:?}"),
    }
    assert!(
        took < std::time::Duration::from_millis(400 + 1_500),
        "超时之后还拖了这么久：{took:?}"
    );
    let grand = std::fs::read_to_string(d.join("grand")).expect("孙进程 pid");
    let child = std::fs::read_to_string(d.join("child")).expect("子进程 pid");
    assert_eq!(
        gone_within(grand.trim(), 100),
        Ok(()),
        "孙进程 {grand} 没跟着组一起没"
    );
    assert_eq!(
        gone_within(child.trim(), 100),
        Ok(()),
        "直接子进程 {child} 没被收尸（'Z' = 僵尸）"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 期限内结束：退出码与两条输出原样；stdin 是空设备（`cat` 立刻读到 EOF）。
#[cfg(unix)]
#[test]
fn a_child_within_its_deadline_hands_back_status_and_both_streams() {
    let out = Child::new("sh")
        .args(["-c", "cat; printf out; printf err >&2; exit 3"])
        .run(Deadline::secs(10))
        .expect("跑完");
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(out.stdout, b"out");
    assert_eq!(out.stderr, b"err");
}

#[test]
fn a_missing_program_is_not_found() {
    let r = Child::new("/nonexistent/ccm-child-test-prog").run(Deadline::secs(5));
    assert!(matches!(r, Err(ChildFail::NotFound(_))), "{r:?}");
}

/// 环境：自有那几格**无条件**摘（不论继承来的还是清空后白名单里列的）；`pass_own` 显式交的到得了；别的照旧。
#[test]
fn own_envs_never_reach_a_child_except_through_pass_own() {
    let c = Child::new("env")
        .inherit_only(&["PATH", "CCM_LISTEN_PORT"])
        .env("KEEP_ME", "1")
        .pass_own("CCM_LISTEN_TOKEN_FILE", "/iso/token");
    let cmd = c.command();
    let set: std::collections::BTreeMap<String, Option<String>> = cmd
        .get_envs()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect();
    for k in OWN_ENVS {
        let want = (k == "CCM_LISTEN_TOKEN_FILE").then(|| "/iso/token".to_string());
        assert_eq!(set.get(k).cloned().flatten(), want, "{k}：{set:?}");
    }
    assert_eq!(set.get("KEEP_ME"), Some(&Some("1".to_string())));
    // 不清空环境的那一形：继承来的自有格逐个显式摘掉。
    let inherit = Child::new("env").command();
    let removed: Vec<String> = inherit
        .get_envs()
        .filter(|(_, v)| v.is_none())
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    for k in OWN_ENVS {
        assert!(removed.iter().any(|r| r == k), "{k} 没被摘：{removed:?}");
    }
}

/// 行为：真起一个 `env`，自有格只剩 `pass_own` 交的那一格。
#[cfg(unix)]
#[test]
fn a_real_child_sees_only_the_own_env_passed_on_purpose() {
    let out = Child::new("env")
        .env("KEEP_ME", "1")
        .pass_own("CCM_LISTEN_PORT", "7")
        .run(Deadline::secs(10))
        .expect("起 env");
    let text = String::from_utf8_lossy(&out.stdout);
    let names: std::collections::BTreeMap<&str, &str> =
        text.lines().filter_map(|l| l.split_once('=')).collect();
    for k in OWN_ENVS {
        if k == "CCM_LISTEN_PORT" {
            assert_eq!(names.get(k), Some(&"7"), "{text}");
        } else {
            assert!(!names.contains_key(k), "子进程拿到了 {k}：{text}");
        }
    }
    assert_eq!(names.get("KEEP_ME"), Some(&"1"), "正控：{text}");
}

/// 脱离起：自成进程组（组号 == 自己的 pid）。
#[cfg(target_os = "linux")]
#[test]
fn a_detached_child_leads_its_own_group() {
    let d = scratch("detach");
    let f = d.join("pgid");
    let pid = Child::new("sh")
        .args([
            "-c",
            &format!("ps -o pgid= -p $$ > {}.t && mv {0}.t {0}", f.display()),
        ])
        .detach()
        .expect("脱离起");
    let mut got = String::new();
    for _ in 0..250 {
        if let Ok(s) = std::fs::read_to_string(&f) {
            got = s;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(got.trim(), pid.to_string());
    let _ = std::fs::remove_dir_all(&d);
}

/// 显式交自有格的口（`pass_own`）生产段**恰好一个调用点**，在起常驻后端那一处；正控：原语里的定义不算调用点。
#[test]
fn pass_own_has_exactly_one_production_caller_and_it_starts_the_resident() {
    let src = crate::guard_support::src_root();
    let mut hits: Vec<(String, usize)> = Vec::new();
    for (p, raw) in guard_core::scan_tree_excluding(&src, &["rs"], &[]) {
        let rel = p
            .strip_prefix(&src)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        let n = crate::guard_support::production_code(&raw)
            .matches(".pass_own(")
            .count();
        if n > 0 {
            hits.push((rel, n));
        }
    }
    assert_eq!(
        hits,
        vec![("control/resident.rs".to_string(), 1)],
        "自有环境（监听口 · 钥匙文件 · 诊断文件）只许起常驻后端那一处显式交"
    );
}

/// 总期限：第一发卡住只等到总期限（不是它自己的期限）；剩下的那一发**不起**、直接回超时；话里报的是总期限。
#[cfg(unix)]
#[test]
fn a_command_budget_cuts_the_stuck_child_short_and_spawns_nothing_after_it_is_spent() {
    let d = scratch("budget");
    let mark = d.join("second-ran");
    let t0 = std::time::Instant::now();
    let _b = Budget::start(Deadline::millis(400));
    let first = Child::new("sh")
        .args(["-c", "sleep 30"])
        .run(Deadline::secs(5));
    let second = Child::new("sh")
        .args(["-c", &format!("touch {}", mark.display())])
        .run(Deadline::secs(5));
    let took = t0.elapsed();
    for r in [first, second] {
        match r {
            Err(ChildFail::TimedOut { after, .. }) => assert_eq!(after, Deadline::millis(400)),
            other => panic!("该超时：{other:?}"),
        }
    }
    assert!(
        took < std::time::Duration::from_millis(400 + 1_500),
        "没按总期限回：{took:?}"
    );
    assert!(!mark.exists(), "总期限用完了还起了下一发");
    let _ = std::fs::remove_dir_all(&d);
}

/// 嵌套只收紧、不放宽；里层守卫掉了还原成外层那一个；最外层掉了线程上什么都不剩。
#[test]
fn nested_budgets_only_tighten_and_each_guard_restores_what_was_there() {
    let outer = Budget::start(Deadline::secs(60));
    {
        let _looser = Budget::start(Deadline::secs(600));
        let s = Span::here().expect("装着");
        assert_eq!(s.total, Deadline::secs(60), "更松的那一个把外层放宽了");
        {
            let _tighter = Budget::start(Deadline::millis(300));
            assert_eq!(Span::here().expect("装着").total, Deadline::millis(300));
        }
        assert_eq!(
            Span::here().expect("装着").total,
            Deadline::secs(60),
            "里层掉了没还原"
        );
    }
    drop(outer);
    assert!(Span::here().is_none(), "最外层掉了，线程上还挂着期限");
}

/// 线程池复用同一根线程跑下一条命令：上一条的总期限（正常结束或中途 panic）不残留，下一条照自己的期限跑完。
#[cfg(unix)]
#[test]
fn a_reused_thread_carries_no_budget_into_the_next_command() {
    std::thread::spawn(|| {
        {
            let _b = Budget::start(Deadline::millis(50));
        }
        let _ = std::panic::catch_unwind(|| {
            let _b = Budget::start(Deadline::millis(50));
            panic!("上一条命令中途炸了");
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(Span::here().is_none(), "上一条的总期限还挂在线程上");
        let out = Child::new("sh")
            .args(["-c", "sleep 0.2; exit 0"])
            .run(Deadline::secs(5))
            .expect("下一条照自己的期限跑完");
        assert!(out.status.success());
    })
    .join()
    .expect("线程");
}

/// 长寿子进程：关掉它的 stdin ⇒ 读到头就自己退的程序自己退了（输出读到头、退出码 0，不是被杀的）——拿着它的那一方还没放手。
/// （终端订阅收 tmux 控制模式客户端就走这一条：直接杀会让 tmux 3.6a 的 server 段错误。）
#[cfg(unix)]
#[test]
fn closing_a_streaming_childs_stdin_lets_it_leave_on_its_own() {
    use std::io::Read;
    let mut s = Child::new("sh")
        .args(["-c", "cat; echo bye-zq"])
        .stream()
        .expect("起得来");
    let mut out = s.take_stdout().expect("输出流");
    s.close_stdin();
    let mut got = String::new();
    out.read_to_string(&mut got).expect("读到头");
    assert_eq!(
        got, "bye-zq\n",
        "stdin 关了，cat 读到头之后那一行该印出来、再退"
    );
    let status = s.child.wait().expect("收尸");
    assert!(status.success(), "它是自己退的，不是被杀的：{status:?}");
}
