//! `remote_ask` 的判据。
//!
//! # 守的要求（住址）
//!
//! - 要求：「**口收成一份**：把 `DialRemote` 从
//!   `asset_sync.rs` 提到中立住址 `src/backend/dial/remote_ask.rs`（逻辑一字不改），`asset_sync` 改调它；判据钉
//!   「**后端生产树里开远端一次性 exec 的只有这一处**」」。
//! - （逐字）：「观测方沿它本来就拥有的那条连接去拉被观测方。」
//!
//! # 判据
//!
//! 1. **一个家**（零命中 ＋ 正控，两向相等）：后端生产树里「开远端一次性 exec」的三个指纹 ——
//!    调 `uses::run(`（经池开一条链路）· 往拨号请求里钉 `abort_marker`（capture 见 hello 就收工）·
//!    `read_request(`（把一份组好的拨号请求读成可拨的形状）—— 在 `dial/` 之外**零处**；
//!    `dial/` 自己那几处是正控（同一识别器在 `dial/forwards.rs` 上命中，扫描没瞎）。
//! 2. **那几台只看机器表**：够到一台 ＝ 机器表里有它（按名字组请求）；「每一台」＝ 表里「连这台」的那几台；对面的资产目录 id 按名字记。
//! 3. **问法**：查不到那台 ⇒ 明说、对面一次都没被调；查得到 ⇒ 交给对面的恰是照表组好的那份拨号请求 ＋ 逐格引号的命令行，参数走 stdin 一行。
//! 4. **引号**：命令行交给真 `sh` 跑，每一格原样回来（异源：真 shell 对我们的引号）。
//!
//! # 买不到
//!
//! - 🔴 真远端：`DialRemote`（capture 那一跳）没对真 sshd 跑过（同 AS2）；判据 3 用替身对面。

use super::*;
use crate::assets::hub_wire::{SkillReadCmd, SkillReadReq};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;

/// 机器表里的一台（线上那一形读成的）。
fn host(v: Value) -> crate::dial::Host {
    crate::dial::machine::loose(&v).unwrap()
}
/// 生产段（剥测试模块 ＋ 整行注释 ＋ 行尾注释）—— 指纹只认代码，不认说起它的散文。
fn code_of(body: &str) -> String {
    let prod = guard_core::production_code(body);
    guard_core::strip_trailing_comments(&guard_core::strip_comment_lines(&prod))
}

/// 后端生产树里，含 `needle` 的文件（相对 `src/backend/`）。
fn homes_of(needle: &str) -> (std::collections::BTreeSet<String>, usize) {
    let root = guard_core::addr::backend_src_root();
    let mut scanned = 0usize;
    let mut homes = std::collections::BTreeSet::new();
    for (path, body) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        if code_of(&body).contains(needle) {
            homes.insert(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    (homes, scanned)
}

/// ★ 判据 1：开远端一次性 exec 的指纹，在 `dial/` 之外零处（本模块住 `dial/`）；`dial/` 里登记的那几份（含本模块）是正控。
#[test]
fn only_this_module_opens_a_one_shot_exec_on_a_remote() {
    // (指纹, dial/ 里应当命中的那几份 —— 正控)
    let prints: &[(&str, &[&str])] = &[
        ("uses::run(", &["dial/forwards.rs"]),
        ("uses::capture(", &["dial/remote_ask.rs"]),
        ("abort_marker", &["ssh-dial/uses.rs", "dial/remote_ask.rs"]),
        ("read_request(", &["dial/mod.rs", "dial/remote_ask.rs"]),
    ];
    for (needle, inside_dial) in prints {
        let (homes, scanned) = homes_of(needle);
        assert!(scanned > 100, "只扫到 {scanned} 份后端源码 —— 遍历坏了");
        let outside: std::collections::BTreeSet<String> = homes
            .iter()
            // 拨号核心那个包（`ssh-dial/`，后端与手机同一份）也是拨号层的家。
            .filter(|p| !(p.starts_with("dial/") || p.starts_with("ssh-dial/")))
            .cloned()
            .collect();
        // 本模块住进了 `dial/`（审计二 B2e）⇒ `dial/` 之外一处都不许有；本模块还经这一跳由下面的正控钉着（它在登记的那几份里）。
        assert!(
            outside.is_empty(),
            "`{needle}` 在 `dial/` 之外有家：{outside:?} —— 又长出一个自己跑远端 exec 的地方（该改调 `remote_ask::ask`）"
        );
        // 正控：同一识别器在 `dial/` 里命中登记的那几份（扫描没瞎、剥法没把代码剥掉）。
        for p in *inside_dial {
            assert!(
                homes.contains(*p),
                "正控失败：`{needle}` 在 `{p}` 里没命中 —— 识别器瞎了，上面那条零命中不可信"
            );
        }
    }
}

/// 一个记账的替身对面：记下每次被交的 (拨号请求, 命令)，答一个固定串。
#[derive(Default)]
struct Recorder {
    calls: StdMutex<Vec<(Value, String, Option<String>)>>,
    n: AtomicUsize,
}

impl Remote for Recorder {
    fn run<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        self.n.fetch_add(1, Ordering::SeqCst);
        self.calls
            .lock()
            .unwrap()
            .push((dial.clone(), command, stdin));
        Box::pin(async { Ok("答".to_string()) })
    }
}

/// 问的那一条：随便一条 typed 内层命令（入参带个引号，看它原样进 stdin）。
fn probe() -> SkillReadReq {
    SkillReadReq {
        name: "it's".to_string(),
        project: None,
    }
}

/// ★ 判据 3（反向）：查不到那台 ⇒ 明说是哪台、对面一次都没被调（不猜、不回落）。
#[tokio::test]
async fn an_unregistered_origin_is_said_and_never_dialed() {
    let table = crate::dial::remote_ask::test_peers::empty();
    let far = Recorder::default();
    let e = ask_json::<SkillReadCmd>("nowhere", &probe(), &crate::Tz::default(), &table, &far)
        .await
        .expect_err("机器表里没有的那台不许问出东西来");
    assert_eq!(e.code.as_deref(), Some("unreachable"));
    assert_eq!(e.message, unreachable_message("nowhere"));
    assert!(
        e.message.contains("nowhere"),
        "那句话要点名是哪台：{}",
        e.message
    );
    assert_eq!(far.n.load(Ordering::SeqCst), 0, "表里没有也去拨了");
}

/// ★ 判据 3（正向）：交给对面的恰是照机器表组好的那份拨号请求 ＋ 逐格引号的命令行（看的人那一台的时区经 `--tz` 转过去），参数走 stdin 一行。
#[tokio::test]
async fn a_machine_in_the_table_is_asked_with_its_composed_dial_and_a_quoted_command() {
    let table = Table::of(vec![host(
        json!({"label": "dev", "host": "192.0.2.2", "user": "u", "keyPath": "/k"}),
    )]);
    let far = Recorder::default();
    let tz = crate::Tz::iana("Asia/Shanghai").unwrap();
    let _ = ask_json::<SkillReadCmd>("dev", &probe(), &tz, &table, &far)
        .await
        .expect_err("替身答的不是 JSON");
    let calls = far.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    // 组好的那一形（蛇形键，`DialRequest` 的形状）：地址 · 用户 · 钥匙照表，线上没交来的什么都不在里头。
    let d = &calls[0].0;
    assert_eq!(
        (&d["host"], &d["port"], &d["user"], &d["key_path"]),
        (&json!("192.0.2.2"), &json!(22), &json!("u"), &json!("/k"))
    );
    assert_eq!(d["endpoints"], json!([{"host": "192.0.2.2", "port": 22}]));
    assert!(
        d.get("machine").is_none() && d.get("prefer").is_none(),
        "交给对面的还是线上那一形：{d}"
    );
    // 期望值手写成字面量（不拿 `command_line` 去比它自己 —— 那样两侧同源，拿掉引号也恒绿）。
    // 那台后端恒在固定落点：`"$HOME"` 在那台上展开（fish 的双引号里同样展开），其后是安全字节。
    assert_eq!(
        calls[0].1,
        r#""$HOME"/.cc-monitor/bin/ccm -- '--skill-read' '--stdin-line' '--tz' 'Asia/Shanghai'"#
    );
    assert_eq!(
        calls[0].2.as_deref(),
        Some("{\"name\":\"it's\",\"project\":null}\n"),
        "入参类型序列化的那一份一行写进 stdin"
    );
}

/// 看的人就在这一台（终端里敲的那一趟）：转给那台的命令行里恒带 `--tz <这台的名>`；不带 ⇒ 那台按它自己的钟写。
#[tokio::test]
async fn a_viewer_on_this_machine_hands_its_zone_name_to_the_far_one() {
    let table = crate::dial::remote_ask::test_peers::named(&["dev"]);
    let far = Recorder::default();
    let here = crate::Tz::named(crate::common::time::TZ_HERE).unwrap();
    let _ = ask_json::<SkillReadCmd>("dev", &probe(), &here, &table, &far)
        .await
        .expect_err("替身答的不是 JSON");
    let calls = far.calls.lock().unwrap().clone();
    let zone = crate::platform::local_tz::iana_name().unwrap_or_else(|| "UTC".to_string());
    assert!(
        calls[0].1.ends_with(&format!(" '--tz' '{zone}'")),
        "{}",
        calls[0].1
    );
}

/// ★ 判据 2：「每一台」＝ 机器表里「连这台」的那几台（照表里的先后）；对面的资产目录 id 按名字记、跟着出现在那一列里。
#[test]
fn every_machine_means_the_ones_the_table_connects_to() {
    let t = Table::of(vec![
        host(json!({"label": "a", "host": "a", "user": "u"})),
        host(json!({"label": "off", "host": "o", "user": "u", "connect": false})),
        host(json!({"label": "b", "host": "b", "user": "u"})),
    ]);
    assert_eq!(t.connected(), vec!["a", "b"]);
    assert!(t.has("off"), "不连的那台也在表里（点名问它照样问得到）");
    t.set_peer("b", "p");
    let rows: Vec<(String, Option<String>)> = reach_rows(&t)
        .into_iter()
        .map(|r| (r.origin.to_string(), r.machine))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("a".to_string(), None),
            ("b".to_string(), Some("p".to_string()))
        ]
    );
}

/// 机器表里那几格组不成请求 ⇒ 那一码照原样交出去（跳板成环不回落直连）；表里没有 ⇒ `unreachable`。
#[test]
fn a_dial_the_table_cannot_compose_says_why() {
    let t = Table::of(vec![
        host(json!({"label": "A", "host": "a", "user": "u", "jump": "B"})),
        host(json!({"label": "B", "host": "b", "user": "u", "jump": "A"})),
    ]);
    assert_eq!(t.dial("A").unwrap_err().code.as_deref(), Some("bad_jump"));
    assert_eq!(
        t.dial("C").unwrap_err().code.as_deref(),
        Some("unreachable")
    );
}

/// ★ 判据 4：命令行交给真 `sh`，每一格原样回来（带单引号 / 双引号 / `$HOME` / 反引号 / 空格 / 中文）。
#[cfg(unix)]
#[test]
fn a_real_posix_shell_reads_every_argument_back_verbatim() {
    let tricky = ["it's", "say \"hi\"", "$HOME", "`id`", "a b", "中文-项目"];
    let mut argv = vec!["%s\\n"];
    argv.extend(tricky.iter());
    // 落点是固定常量 ⇒ 把打头那一格换成 `printf` 再交给真 `sh`（量的是 argv 那几格的引号）。
    let line = command_line(&argv).replacen(relay_route_core::BACKEND_LANDING_SHELL, "printf", 1);
    assert!(line.starts_with("printf "), "命令行不以落点打头：{line}");
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&line)
        .output()
        .expect("起 sh");
    assert!(out.status.success(), "sh 没跑通：{line}");
    let got: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(
        got,
        tricky.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    // 落点那一格在真 `sh` 里展开成「那台的家目录 ＋ `/.cc-monitor/bin/ccm`」（家目录带空格也不拆词）。
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "printf '%s' {}",
            relay_route_core::BACKEND_LANDING_SHELL
        ))
        .env("HOME", "/h o/me")
        .output()
        .expect("起 sh");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("/h o/me/{}", relay_route_core::BACKEND_LANDING_REL)
    );
}

// ═══ 外层被丢 ⇒ 内层服务任务一起收 ═══════════════════════════════════════════════════
//
// 守的要求（住址，纪律 19）：红线（逐字）「复用后它占掉共享连接一个槽永不释放，局部卡死升级成全局卡死」·
// 「在飞槽位一定回收」。
//（`spawn` 出去的任务，句柄被丢 = 脱钩；`task.abort()` 只写在正常返回那一支）。

/// 一个会在被丢时报信的哨兵（代表内层任务手里攥着的那一格）。
struct Sentinel(Option<tokio::sync::oneshot::Sender<()>>);
impl Drop for Sentinel {
    fn drop(&mut self) {
        if let Some(t) = self.0.take() {
            let _ = t.send(());
        }
    }
}
/// 一趟收全的替身结果（拨号核心 `uses::capture` 那一形）。
fn captured(stdout: &str, stderr: &str, exit_status: u32) -> crate::dial::Captured {
    crate::dial::Captured {
        stdout: stdout.into(),
        stderr: stderr.into(),
        exit_status: Some(exit_status),
    }
}

/// A4 ★ 远端永不答：外层 future 被丢（= 后端收到 `cancel` 打断了外层）⇒ 那一趟收全一起被丢、那一格放掉。
/// 另一向：正常答完 ⇒ 结果照常、那一格同样放掉。异源：真 tokio 期限 ＋ 哨兵（不看源码）。
#[tokio::test]
async fn an_abandoned_ask_takes_its_inner_task_down_with_it() {
    // ① 永不答：那一趟攥着哨兵、永远不回（远端不答的那一形）。
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let outer = pull_over_coded(async move {
        let _slot = Sentinel(Some(tx));
        std::future::pending::<Result<crate::dial::Captured, ssh_dial::DialFail>>().await
    });
    // 外层等一会儿还没答 ⇒ 调用方期限到点 ⇒ 外层被丢（`timeout` 到点即丢它）。
    let gave_up = tokio::time::timeout(std::time::Duration::from_millis(200), outer).await;
    assert!(gave_up.is_err(), "永不答的那一趟竟然答了");
    tokio::time::timeout(guard_core::wall::CEILING, rx)
        .await
        .expect("外层被丢 10 秒了，那一趟还攥着那一格 —— 它脱钩了（格永远占着）")
        .expect("哨兵没报信就没了");

    // ② 另一向：正常答完 ⇒ 结果照常，那一格同样放掉。
    let (tx2, rx2) = tokio::sync::oneshot::channel::<()>();
    let got = pull_over_coded(async move {
        let _slot = Sentinel(Some(tx2));
        Ok(captured("答", "", 0))
    })
    .await
    .map_err(|s| s.message);
    assert_eq!(got, Ok("答".to_string()));
    tokio::time::timeout(guard_core::wall::CEILING, rx2)
        .await
        .expect("答完 10 秒了，那一趟还在")
        .expect("哨兵没报信就没了");
}

/// 没开成（拨号核心的 `DialFail`）⇒ 那一句是「那台够不着」、带拨号核心写好的那一句。
#[tokio::test]
async fn a_capture_that_never_opened_says_unreachable_with_the_dial_words() {
    let e = pull_over_coded(async {
        Err(ssh_dial::DialFail {
            error: "拨号那一句".into(),
            detail: None,
            fingerprint: None,
            reason: None,
            key_type: None,
            expected_fingerprint: None,
        })
    })
    .await
    .unwrap_err();
    assert_eq!(
        e.message,
        copy_text("beRemoteAsk.run.unreachable", &[("why", "拨号那一句")])
    );
}

/// 交给 capture 的 stdin 真进了拨号请求的 `capture.stdin`（缺席 = 一个字节不写），
/// 命令原样、用法是 capture —— 生产那一个对面（`DialRemote`）就是拿这份请求去跑 `ssh_dial::uses::capture` 的。
/// ⚠ 买不到：「写进远端进程 stdin」那一跳要真 sshd（不进门禁）。
#[test]
fn a_capture_dial_carries_the_stdin_line_verbatim_and_only_when_given() {
    let dial = Table::of(vec![crate::dial::machine::loose(
        &json!({"label": "h", "host": "h", "port": 22, "user": "u", "keyPath": "/k"}),
    )
    .unwrap()])
    .dial("h")
    .unwrap();
    let with = crate::dial::remote_ask::for_capture(
        &dial,
        "'/b' '--x'".into(),
        Some("{\"a\":1}\n".into()),
    )
    .expect("拼得出请求");
    assert_eq!(with.command, "'/b' '--x'");
    assert_eq!(with.use_, crate::dial::Use::Capture);
    assert!(!with.stages && !with.probe, "一次性 capture 走池、不要阶段");
    let cap = with.capture.expect("capture 参数在");
    assert_eq!(
        cap.stdin.as_deref(),
        Some("{\"a\":1}\n"),
        "载荷没进 capture.stdin"
    );
    assert_eq!(
        cap.abort_marker.as_deref(),
        Some(cmd_wire::dial::OLD_BACKEND_MARKER)
    );
    let without =
        crate::dial::remote_ask::for_capture(&dial, "'/b'".into(), None).expect("拼得出请求");
    assert_eq!(
        without.capture.expect("capture 参数在").stdin,
        None,
        "没给 stdin 却写了"
    );
}

/// ★ 远端那一跳没成时**码随原话一起交回**：那台 CLI 信封 `{code, message}` 的码原样进 [`Said`]，
/// 不压成一个；信封读不出来（不是 JSON）⇒ 码缺席、原话照交。
#[tokio::test]
async fn a_failed_remote_command_keeps_its_envelope_code() {
    let run = |stderr: &'static str| pull_over_coded(async move { Ok(captured("", stderr, 2)) });
    let e = run("{\"code\":\"stale\",\"message\":\"盘上那份变了\"}")
        .await
        .unwrap_err();
    assert_eq!(e.code.as_deref(), Some("stale"));
    assert!(e.message.contains("盘上那份变了"), "{e:?}");
    // 不是信封的原样 stderr：句子只说「后端报错」，那台的原话记日志、不上句子。
    let e = run("bash: boom").await.unwrap_err();
    assert_eq!(e.code, None);
    assert_eq!(e.message, copy_text("beRemoteAsk.run.failed", &[]), "{e:?}");
}

/// ★ 问别台常驻那一跳（[`ask_resident`]）在那台起的 `--resident-call` 是一次性的：发起方一走（ssh 断开 ⇒ 它 stdout 那根管子的读端没人了），
/// 就不再等那台常驻、当场收工 —— 不攥着一条常驻连接空等到发起方给的期限（10-28 沙箱 sshd 实测：断开后照活到期限）。
/// 读端还在 ⇒ 照旧等到期限、报 `timed_out`（「发起方走了」不是一碰就响）；读端已关 ⇒ 交回「走了」，不等到期限。
/// 扮那台常驻的：绑了套接字、不收不说（连得上，hello 永远不来）。
#[tokio::test]
#[cfg(unix)]
async fn a_resident_call_stops_waiting_once_the_asker_is_gone() {
    use crate::control::resident::{call_while, stdout_gone};
    use crate::platform::child::Deadline;
    let dh = std::env::temp_dir().join("ra-gone");
    let _ = std::fs::remove_dir_all(&dh);
    let run = relay_route_core::listen_dir_for(&dh);
    std::fs::create_dir_all(&run).unwrap();
    std::fs::set_permissions(&run, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let _mute = own_chan::bind(&relay_route_core::listen_socket_for(&dh)).expect("绑");
    let asked = crate::wire::listen::ResidentCall {
        cmd: "machines-list".into(),
        args: json!({}),
    };
    let tz = crate::Tz::default();

    // 读端一直在：等到期限。
    let (rd, wr) = std::io::pipe().unwrap();
    let got = call_while(
        &dh,
        &asked,
        Some(300),
        &tz,
        Deadline::millis(300),
        stdout_gone(wr.into()),
    )
    .await;
    let code = got.map(|r| r.map_err(|f| f.code));
    assert_eq!(
        code,
        Some(Err(crate::wire::listen::CALL_TIMED_OUT.to_string())),
        "读端还在却当成发起方走了"
    );
    drop(rd);

    // 读端已关：不等那 30 秒期限（还在等 ⇒ 30 秒后交回 `timed_out`，下面那一条红）。
    let (rd, wr) = std::io::pipe().unwrap();
    drop(rd);
    let got = call_while(
        &dh,
        &asked,
        Some(30_000),
        &tz,
        Deadline::secs(30),
        stdout_gone(wr.into()),
    )
    .await;
    let _ = std::fs::remove_dir_all(&dh);
    assert!(
        got.is_none(),
        "发起方走了（读端关了）还在等那台常驻、等到了期限：{:?}",
        got.map(|r| r.map_err(|f| f.code))
    );
}
