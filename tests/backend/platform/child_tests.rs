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
    let want: std::collections::BTreeSet<&str> = ["CCM_RESIDENT", "CCM_BACKEND_STDERR_LOG"]
        .into_iter()
        .collect();
    assert_eq!(got, want);
    // 远端 `--resident-ensure` 交给常驻载体的那几格 == 这一份 ＋ 中转口（中转口要往下传给窗格里的 ccm）。
    let handed: std::collections::BTreeSet<String> = crate::control::resident::child_env(
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
        "sleep {s} & echo $! > {g}; echo $$ > {c}; wait",
        s = 2 * guard_core::wall::CEILING_SECS,
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
        took < guard_core::wall::CEILING,
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
        .inherit_only(&["PATH", "CCM_RESIDENT"])
        .env("KEEP_ME", "1")
        .pass_own("CCM_BACKEND_STDERR_LOG", "/iso/stderr.log");
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
        let want = (k == "CCM_BACKEND_STDERR_LOG").then(|| "/iso/stderr.log".to_string());
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
        .pass_own("CCM_RESIDENT", "7")
        .run(Deadline::secs(10))
        .expect("起 env");
    let text = String::from_utf8_lossy(&out.stdout);
    let names: std::collections::BTreeMap<&str, &str> =
        text.lines().filter_map(|l| l.split_once('=')).collect();
    for k in OWN_ENVS {
        if k == "CCM_RESIDENT" {
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
    for _ in guard_core::wall::rounds() {
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
    // 每一发自己的期限是兜底上限的两倍、睡得更久 ⇒ 总期限不生效就撞下面那道（兜底的数）。
    let own = Deadline::secs(2 * guard_core::wall::CEILING_SECS);
    let first = Child::new("sh")
        .args([
            "-c",
            &format!("sleep {}", 4 * guard_core::wall::CEILING_SECS),
        ])
        .run(own);
    let second = Child::new("sh")
        .args(["-c", &format!("touch {}", mark.display())])
        .run(own);
    let took = t0.elapsed();
    for r in [first, second] {
        match r {
            Err(ChildFail::TimedOut { after, .. }) => assert_eq!(after, Deadline::millis(400)),
            other => panic!("该超时：{other:?}"),
        }
    }
    assert!(took < guard_core::wall::CEILING, "没按总期限回：{took:?}");
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
        // 不用先睡过那 50 ms：残留的期限要是还挂着，下面那条要跑 200 ms 的命令自己就会被它掐掉。
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

/// 〔环境里没有后端内部变量，也没有会话号变量〕会话环境里混进来的后端内部变量（monitor 给终端窗口导的后端二进制路径 · 旧版常驻后端留在长寿 tmux server
/// 全局环境里的监听口与钥匙文件 · 常驻开关 · 诊断文件）—— 起出来的每个子进程（agent 会话就在里面）一个都看不见；
/// 真要往下传的那几格（中转口 · 家）照旧到得了。外层带着这些变量把本测试二进制当子进程起、跑内层（进程环境不能在多线程测试里改）。
#[test]
fn a_child_never_inherits_the_backend_internal_families() {
    assert!(crate::platform::child_env::is_internal("CCM_LISTEN_PORT"));
    assert!(crate::platform::child_env::is_internal("CCM_BACKEND_BIN"));
    assert!(crate::platform::child_env::is_internal("CCM_RESIDENT"));
    assert!(!crate::platform::child_env::is_internal("CCM_RELAY_PORT"));
    assert!(!crate::platform::child_env::is_internal("CCM_DATA_DIR"));
    // Windows 上名字不分大小写：外层故意用大小写混写的那一形交（真机上漏下去的就是这一形），内层照样一个都不许看见。
    let spell = |k: &str| -> String {
        if cfg!(windows) {
            mixed_case(k)
        } else {
            k.to_string()
        }
    };
    let out = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([
            "platform::child::tests::internal_families_inner",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(INTERNAL_MARK, "1")
        .env(spell("CCM_LISTEN_PORT"), "52917")
        .env(
            spell("CCM_LISTEN_TOKEN_FILE"),
            "/h/.cc-monitor/listen-token",
        )
        .env(
            spell("CCM_BACKEND_STDERR_LOG"),
            "/h/.cc-monitor/logs/backend/stderr.log",
        )
        .env(spell("CCM_BACKEND_BIN"), "/h/.cc-monitor/bin/ccm")
        .env(spell("CCM_RESIDENT"), "1")
        .env("CCM_RELAY_PORT", "8788")
        .env("CCM_DATA_DIR", "/iso/home")
        .envs(
            crate::agents::self_sid_envs()
                .into_iter()
                .map(|k| (spell(k), "parent-sid")),
        )
        .output()
        .expect("起内层");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains(INTERNAL_READING), "内层没跑到：{text}");
    assert!(out.status.success(), "内层红了：{text}");
}

/// `CCM_LISTEN_PORT` → `Ccm_lIsTeN_…`：逐字母交替大小写（下划线不动），保证与全大写、全小写都不逐字相等。
fn mixed_case(k: &str) -> String {
    k.chars()
        .enumerate()
        .map(|(i, c)| {
            if i % 2 == 0 {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// 🔴**摘环境按本平台的大小写规矩比**：Windows 不分大小写（`ccm_listen_port` · `Claude_Code_Session_Id` 照样摘），
/// Unix 分（小写那一格是另一个变量，不归这层管）。两种规矩都量（不靠跑在哪个平台上），再钉住本平台用的是哪一种。
#[test]
fn internal_names_are_compared_the_way_this_platform_compares_env_names() {
    use crate::platform::child_env::{is_internal_as, NAMES_FOLD_CASE};
    let sid = ["CLAUDE_CODE_SESSION_ID"];
    for name in [
        "ccm_listen_port",
        "Ccm_Listen_Token_File",
        "ccm_backend_bin",
        "Ccm_Resident",
        "ccm_backend_stderr_log",
        "Claude_Code_Session_Id",
    ] {
        assert!(
            is_internal_as(name, &sid, true),
            "不分大小写时 {name} 没被摘"
        );
        assert!(
            !is_internal_as(name, &sid, false),
            "分大小写时 {name} 竟被当成了同一格"
        );
    }
    for name in [
        "ccm_relay_port",
        "Ccm_Data_Dir",
        "claude_config_dir",
        "ccm_listen",
    ] {
        assert!(
            !is_internal_as(name, &sid, true),
            "不分大小写时 {name} 被摘错了（它该往下传）"
        );
    }
    assert!(
        is_internal_as("CCM_LISTEN_PORT", &sid, false),
        "正控：逐字相等那一形没摘"
    );
    assert_eq!(NAMES_FOLD_CASE, cfg!(windows), "本平台的大小写规矩钉错了");
    assert_eq!(
        crate::platform::child_env::is_internal("ccm_listen_port"),
        cfg!(windows),
        "is_internal 没按本平台的规矩比"
    );
}

const INTERNAL_MARK: &str = "CCM_CHILD_INTERNAL_INNER";
const INTERNAL_READING: &str = "INTERNAL-FAMILIES-READ";

#[test]
#[ignore = "内层入口：只在被外层用 CCM_CHILD_INTERNAL_INNER 拉起时才做断言"]
fn internal_families_inner() {
    if std::env::var(INTERNAL_MARK).is_err() {
        return;
    }
    // 与 `main.rs` 同一个入口：登记会话号变量。
    crate::agents::install_child_env_filter();
    let out = if cfg!(windows) {
        Child::new("cmd").args(["/C", "set"])
    } else {
        Child::new("env")
    }
    .run(Deadline::secs(10))
    .expect("起 env");
    let text = String::from_utf8_lossy(&out.stdout);
    let names: std::collections::BTreeMap<&str, &str> =
        text.lines().filter_map(|l| l.split_once('=')).collect();
    let leaked: Vec<&str> = names
        .keys()
        .copied()
        .filter(|k| crate::platform::child_env::is_internal(k))
        .collect();
    assert_eq!(leaked, Vec::<&str>::new(), "后端内部变量漏给了子进程");
    let sids = crate::agents::self_sid_envs();
    assert!(
        !sids.is_empty(),
        "适配层一个「我是哪个会话」的变量都没登记 —— 下面那一格空转"
    );
    let leaked_sid: Vec<&str> = names
        .keys()
        .copied()
        .filter(|k| {
            sids.iter()
                .any(|s| crate::platform::child_env::same_name(s, k))
        })
        .collect();
    assert_eq!(
        leaked_sid,
        Vec::<&str>::new(),
        "会话号变量漏给了子进程（它起的会话会把那个会话错当成父）"
    );
    assert_eq!(
        names.get("CCM_RELAY_PORT"),
        Some(&"8788"),
        "正控：中转口没到"
    );
    assert_eq!(
        names.get("CCM_DATA_DIR"),
        Some(&"/iso/home"),
        "正控：家没到"
    );
    println!("{INTERNAL_READING}");
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

// ── 在伪终端里一问一答（`Child::converse_in_terminal`）──────────────────────────────────────────

/// 收它说的话，直到它退了（有界：判据的墙钟上限）。
#[cfg(unix)]
fn heard_until_exit(rx: &mpsc::Receiver<Heard>) -> (Vec<String>, Option<Option<i32>>) {
    let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
    let mut lines = Vec::new();
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(Heard::Out(l) | Heard::Err(l)) => lines.push(l),
            Ok(Heard::Exited(c)) => return (lines, Some(c)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    (lines, None)
}

/// 起在终端里（stdin · stdout 都是终端）；程序照交给它的 `PATH` 找；显式环境 · 工作目录照交；
/// 输出逐行交、去掉回车与控制序列（OSC 8 超链接只留可见的字）；写进去的一行它读得到；退出码最后一件。
#[cfg(unix)]
#[test]
fn a_terminal_conversation_runs_on_a_tty_and_hands_back_plain_lines() {
    use std::os::unix::fs::PermissionsExt;
    let d = scratch("pty-talk");
    std::fs::create_dir_all(d.join("bin")).unwrap();
    let prog = d.join("bin").join("talker");
    std::fs::write(
        &prog,
        "#!/bin/sh\n[ -t 0 ] && [ -t 1 ] || { echo notty; exit 3; }\n\
         echo \"env:$KEEP_ME own:${CCM_RESIDENT:-none}\"; pwd\n\
         printf 'Visit:\\n  \\033]8;;https://x.test/a?b=1\\007\\033[94mhttps://x.test/a?b=1\\033[39m\\033]8;;\\007\\n'\n\
         printf 'prompt: '\nread -r l\necho \"got:$l\"\nexit 4\n",
    )
    .unwrap();
    std::fs::set_permissions(&prog, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut conv = Child::new("talker")
        .env("PATH", format!("{}:/usr/bin:/bin", d.join("bin").display()))
        .env("KEEP_ME", "1")
        .current_dir(&d)
        .converse_in_terminal()
        .expect("起在终端里");
    let rx = conv.take_heard().expect("说的话");
    // 等它问了再答（提示符没有换行，不单独成一行；等 `Visit` 那两行到了就答）。
    let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
    let mut early = Vec::new();
    while !early.iter().any(|l: &String| l.contains("x.test"))
        && std::time::Instant::now() < deadline
    {
        if let Ok(Heard::Out(l)) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
            early.push(l);
        }
    }
    conv.say("hello").expect("写一行");
    let (rest, code) = heard_until_exit(&rx);
    let all: Vec<String> = early.into_iter().chain(rest).collect();
    assert_eq!(code, Some(Some(4)), "退出码最后一件：{all:?}");
    assert!(
        !all.iter().any(|l| l.contains("notty")),
        "不是终端：{all:?}"
    );
    assert!(
        all.contains(&"env:1 own:none".to_string()),
        "显式环境 · 自有格照摘：{all:?}"
    );
    let real = std::fs::canonicalize(&d).unwrap().display().to_string();
    assert!(
        all.iter()
            .any(|l| *l == real || *l == d.display().to_string()),
        "工作目录：{all:?}"
    );
    assert!(
        all.contains(&"  https://x.test/a?b=1".to_string()),
        "超链接只留可见的字：{all:?}"
    );
    assert!(
        all.iter().any(|l| l.ends_with("got:hello")),
        "写进去的那一行：{all:?}"
    );
    assert!(
        all.iter()
            .all(|l| !l.contains('\u{1b}') && !l.contains('\r') && !l.contains('\u{7}')),
        "控制序列 · 回车没去干净：{all:?}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 放手即收：它与它起的孙进程都没了，直接子进程已收尸。
#[cfg(target_os = "linux")]
#[test]
fn dropping_a_terminal_conversation_kills_its_whole_group() {
    let d = scratch("pty-drop");
    let script = format!(
        "sleep {s} & echo $! > {g}; echo $$ > {c}; echo ready; wait",
        s = 2 * guard_core::wall::CEILING_SECS,
        g = d.join("grand").display(),
        c = d.join("child").display()
    );
    let mut conv = Child::new("sh")
        .args(["-c", &script])
        .converse_in_terminal()
        .expect("起在终端里");
    let rx = conv.take_heard().expect("说的话");
    let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
    while std::time::Instant::now() < deadline {
        if let Ok(Heard::Out(l)) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
            if l == "ready" {
                break;
            }
        }
    }
    let grand = std::fs::read_to_string(d.join("grand")).expect("孙进程 pid");
    let child = std::fs::read_to_string(d.join("child")).expect("子进程 pid");
    drop(conv);
    assert_eq!(
        gone_within(grand.trim(), 100),
        Ok(()),
        "孙进程 {grand} 没跟着组一起没"
    );
    assert_eq!(
        gone_within(child.trim(), 100),
        Ok(()),
        "直接子进程 {child} 没被收尸"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 终端输出的一行 ⇒ 人看得见的字：CSI（颜色 · 光标）· OSC（含 OSC 8 超链接，BEL 或 ST 收尾）· 两字节转义 · 回车都去掉。
#[test]
fn terminal_text_keeps_only_what_is_visible() {
    assert_eq!(plain_line(b"\x1b[94mhi\x1b[39m\r"), "hi");
    assert_eq!(
        plain_line(b"\x1b]8;;https://a.test/x\x07link\x1b]8;;\x07"),
        "link"
    );
    assert_eq!(plain_line(b"\x1b]0;title\x1b\\a\x1b(Bb"), "ab");
    assert_eq!(plain_line(b"\x1b[2K\x1b[1Gprompt: "), "prompt: ");
    assert_eq!(plain_line("中文\u{1b}[0m".as_bytes()), "中文");
}

/// 一问一答那一种（管道 · 终端两种起法）放手时连整棵一起收 —— **Windows 那一侧**。
/// Windows 那段本机不编 ⇒ 把源码当数据读（同 `pidwatch_windows_shape_tests.rs` 的处境）；行为判据是下一条，只在 Windows 上跑。
/// 钉：① 不再拿伪终端库给的那一把（它只杀直接子进程，`claude` 起的孙进程会留下）；
/// ② 两种起法都交给 `os::Group` 里「整棵」那一种；③ Windows 的那一种是设了「关句柄即杀」的 Job。
#[test]
fn a_conversation_is_released_with_its_whole_tree_on_windows_too() {
    let src = guard_core::production_code(include_str!("../../../src/backend/platform/child.rs"));
    let between = |from: &str, to: &str| -> String {
        let at = src.find(from).unwrap_or_else(|| {
            panic!("找不到 `{from}` —— child.rs 的形状变了，本条的读法要跟着改")
        });
        let rest = &src[at..];
        let end = rest[from.len()..]
            .find(to)
            .map(|e| e + from.len())
            .unwrap_or_else(|| panic!("`{from}` 之后找不到 `{to}`"));
        rest[..end].to_string()
    };
    assert!(
        !src.contains("clone_killer"),
        "终端那一种放手时还在用伪终端库那一把（只杀直接子进程）"
    );
    let piped = between(
        "pub(crate) fn converse(self)",
        "pub(crate) fn converse_in_terminal",
    );
    assert!(
        piped.contains("os::Group::adopt_tree(&child)"),
        "管道那一种没交给整棵那一种 Group"
    );
    let terminal = between("pub(crate) fn converse_in_terminal", "fn pty_fail");
    assert!(
        terminal.contains("os::Group::tree_of_handle"),
        "终端那一种在 Windows 上没交给整棵那一种 Group"
    );
    let win = between("#[cfg(windows)]\nmod os {", "\n}\n");
    for must in [
        "fn tree_of_handle(",
        "fn adopt_tree(",
        "SetInformationJobObject(",
        "KILL_ON_JOB_CLOSE",
    ] {
        assert!(win.contains(must), "Windows 的 os 里少了 `{must}`");
    }
}

/// 同上一条的行为：Windows 上终端里起的那一个起了孙进程，放手 ⇒ 孙进程也没了。
#[cfg(windows)]
#[test]
fn dropping_a_terminal_conversation_on_windows_takes_its_grandchild_too() {
    let d = scratch("pty-drop-win");
    let grand = d.join("grand");
    let script = format!(
        "$p = Start-Process -PassThru -NoNewWindow -FilePath ping.exe -ArgumentList '-n','{n}','127.0.0.1'; \
         Set-Content -Path '{g}' -Value $p.Id; 'ready'; Start-Sleep -Seconds {n}",
        n = 2 * guard_core::wall::CEILING_SECS,
        g = grand.display()
    );
    let mut conv = Child::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .converse_in_terminal()
        .expect("起在终端里");
    let rx = conv.take_heard().expect("说的话");
    let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
    while std::time::Instant::now() < deadline {
        if let Ok(Heard::Out(l)) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
            if l.trim() == "ready" {
                break;
            }
        }
    }
    let pid = std::fs::read_to_string(&grand).expect("孙进程 pid");
    let pid = pid.trim().to_string();
    let alive = |pid: &str| {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .expect("tasklist");
        String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
    };
    assert!(alive(&pid), "放手之前孙进程 {pid} 就不在了（判据没起对）");
    drop(conv);
    let mut gone = false;
    for _ in guard_core::wall::rounds() {
        if !alive(&pid) {
            gone = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(gone, "孙进程 {pid} 没跟着一起收");
    let _ = std::fs::remove_dir_all(&d);
}
