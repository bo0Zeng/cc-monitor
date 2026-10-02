//! `remote_ask` 的判据。
//!
//! # 守的要求（住址）
//!
//! - 要求：「**口收成一份**：把 `DialRemote` ＋ 可达表从
//!   `asset_sync.rs` 提到中立住址 `src/backend/stream/remote_ask.rs`（逻辑一字不改），`asset_sync` 改调它；判据钉
//!   「**后端生产树里开远端一次性 exec 的只有这一处**」」。
//! - （逐字）：「观测方沿它本来就拥有的那条连接去拉被观测方。」
//!
//! # 判据
//!
//! 1. **一个家**（零命中 ＋ 正控，两向相等）：后端生产树里「开远端一次性 exec」的三个指纹 ——
//!    调 `uses::run(`（经池开一条链路）· 往拨号请求里钉 `abort_marker`（capture 见 hello 就收工）·
//!    `parse_request_value(`（把一份拨号请求读成可拨的形状）—— 在 `dial/` 之外**只**出现在本模块；
//!    `dial/` 自己那几处是正控（同一识别器在 `dial/link.rs` 上命中，扫描没瞎）。
//! 2. **可达表只有一个写口**：`remote-reach` 与 `assets-sync` 登记同一张表、同一个函数（行为：两条路登记后表逐格相等）。
//! 3. **问法**：查不到那台 ⇒ 明说、对面一次都没被调；查得到 ⇒ 交给对面的恰是表里那份拨号请求 ＋ 逐格引号的命令行。
//! 4. **引号**：命令行交给真 `sh` 跑，每一格原样回来（异源：真 shell 对我们的引号）。
//!
//! # 买不到
//!
//! - 🔴 真远端：`DialRemote`（capture 那一跳）没对真 sshd 跑过（同 AS2）；判据 3 用替身对面。

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;

/// 生产段（剥测试模块 ＋ 整行注释 ＋ 行尾注释）—— 指纹只认代码，不认说起它的散文。
fn code_of(body: &str) -> String {
    let prod = crate::guard_support::production_code(body);
    guard_core::strip_trailing_comments(&guard_core::strip_comment_lines(&prod))
}

/// 后端生产树里，含 `needle` 的文件（相对 `src/backend/`）。
fn homes_of(needle: &str) -> (std::collections::BTreeSet<String>, usize) {
    let root = crate::guard_support::src_root();
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

/// ★ 判据 1：开远端一次性 exec 的指纹，在 `dial/` 之外只住本模块（两向相等）；`dial/` 里的是正控。
#[test]
fn only_this_module_opens_a_one_shot_exec_on_a_remote() {
    // (指纹, dial/ 里应当命中的那几份 —— 正控)
    let prints: &[(&str, &[&str])] = &[
        ("uses::run(", &["dial/link.rs"]),
        ("abort_marker", &["dial/uses.rs"]),
        (
            "parse_request_value(",
            &["dial/link.rs", "dial/mod.rs", "dial/sftp.rs"],
        ),
    ];
    for (needle, inside_dial) in prints {
        let (homes, scanned) = homes_of(needle);
        assert!(scanned > 100, "只扫到 {scanned} 份后端源码 —— 遍历坏了");
        let outside: std::collections::BTreeSet<String> = homes
            .iter()
            .filter(|p| !p.starts_with("dial/"))
            .cloned()
            .collect();
        let want: std::collections::BTreeSet<String> = ["stream/remote_ask.rs".to_string()].into();
        assert_eq!(
            outside, want,
            "`{needle}` 在 `dial/` 之外的家对不上 —— 多 = 又长出一个自己跑远端 exec 的地方（该改调 `remote_ask::ask`）；\
             少 = 本模块不再经这一跳（空转）"
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

fn reach_args(origin: &str, host: &str) -> Value {
    json!({
        "origin": origin,
        "dial": {"machine": {"host": host, "port": 22, "user": "u", "keyPath": "/k"}},
    })
}

/// ★ 判据 3（反向）：查不到那台 ⇒ 明说是哪台、对面一次都没被调（不猜、不回落）。
#[tokio::test]
async fn an_unregistered_origin_is_said_and_never_dialed() {
    let table = Table::default();
    let far = Recorder::default();
    let e = ask_with("nowhere", &["--list-projects"], &table, &far)
        .await
        .expect_err("没登记的那台不许问出东西来");
    assert_eq!(e, unreachable_message("nowhere"));
    assert!(e.contains("[nowhere]"), "那句话要点名是哪台：{e}");
    assert_eq!(far.n.load(Ordering::SeqCst), 0, "没登记也去拨了");
}

/// ★ 判据 3（正向）：交给对面的恰是表里那份拨号请求 ＋ 逐格引号的命令行。
#[tokio::test]
async fn a_registered_origin_is_asked_with_exactly_its_dial_and_a_quoted_command() {
    let table = Table::default();
    answer_reach_with(&reach_args("dev", "10.0.0.2"), &table).unwrap();
    let far = Recorder::default();
    let out = ask_with("dev", &["--list-sessions", "-home-u-it's"], &table, &far)
        .await
        .unwrap();
    assert_eq!(out, "答");
    let calls = far.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, reach_args("dev", "10.0.0.2")["dial"]);
    // 期望值手写成字面量（不拿 `command_line` 去比它自己 —— 死值验 A3：那样两侧同源，拿掉引号也恒绿）。
    // 命令行里只剩落点与旗标；项目目录名（带 `'`）走 stdin 一行。
    // 那台后端恒在固定落点：`"$HOME"` 在那台上展开（fish 的双引号里同样展开），其后是安全字节。
    assert_eq!(
        calls[0].1,
        r#""$HOME"/.cc-monitor/bin/ccm -- '--list-sessions' '--stdin-line'"#
    );
    assert_eq!(calls[0].2.as_deref(), Some("[\"-home-u-it's\"]\n"));
}

/// ★ 守的要求（逐字）：「起远端后端的命令、历史跨机那几问的 argv 仍拼在远端命令行里 ⇒
/// 远端登录 shell 是 fish 之类时这几条仍不成立」。发的一侧（`ask_with`）交出去的命令行交真 `sh` 拆词、stdin 那一行交收的一侧
/// （`cli_control::expand_stdin_argv`）⇒ 拼回来的 argv 与调用方给的逐格相等；命令行里没有任何一格自由文本
/// （只剩后端路径与旗标 —— 那几格不含 `'` 与 `\`，fish 与 POSIX 单引号同读）。
#[cfg(unix)]
#[tokio::test]
async fn the_free_text_rides_stdin_and_the_remote_gets_the_argv_back_verbatim() {
    let table = Table::default();
    answer_reach_with(&reach_args("dev", "10.0.0.2"), &table).unwrap();
    let far = Recorder::default();
    let tricky = ["-home-u-it's", "照片 (2019) \\ $HOME `id`", "a & b; c"];
    for dir in tricky {
        ask_with("dev", &["--list-sessions", dir], &table, &far)
            .await
            .unwrap();
    }
    for ((_, line, stdin), dir) in far.calls.lock().unwrap().iter().zip(tricky) {
        assert!(!line.contains(dir), "自由文本进了远端命令行：{line}");
        assert!(
            !line.contains('\\') && line.matches('\'').count() % 2 == 0 && !line.contains("'\\''"),
            "命令行里有 fish 与 POSIX 读法不同的写法：{line}"
        );
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("set -- {line}; shift 2; printf '%s\\n' \"$@\""))
            .output()
            .expect("起 sh");
        let words: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        let stdin = stdin.clone().expect("自由文本没走 stdin");
        let got = crate::control::cli_control::expand_stdin_argv(words, stdin.as_bytes())
            .expect("收的一侧读得动");
        assert_eq!(got, vec!["--list-sessions".to_string(), dir.to_string()]);
    }
}

/// ★ 判据 2：`remote-reach` 与 `assets-sync` 登记的是同一张表、同一个写口 —— 两条路登记同一台之后表逐格相等；
/// 再登记一次换掉拨号请求、对面的 id 留着。
#[test]
fn both_doors_register_through_the_one_writer() {
    let a = Table::default();
    let b = Table::default();
    answer_reach_with(&reach_args("dev", "h1"), &a).unwrap();
    register(&b, &reach_args("dev", "h1")).unwrap();
    assert_eq!(*lock(&a), *lock(&b), "两扇门登记出来的表不一样");
    // 对面的 id（资产目录那一路拉回来之后写的）在再登记时留着。
    lock(&a).get_mut("dev").unwrap().peer = Some("p".into());
    answer_reach_with(&reach_args("dev", "h2"), &a).unwrap();
    let row = lock(&a).get("dev").cloned().unwrap();
    assert_eq!(row.peer.as_deref(), Some("p"));
    assert_eq!(row.dial["machine"]["host"], "h2");
    // 半给的入参拒，表不动。
    for bad in [
        json!({}),
        json!({"origin": ""}),
        json!({"origin": "x"}),
        json!({"origin": "x", "dial": "nope"}),
    ] {
        let before = lock(&a).clone();
        let e = answer_reach_with(&bad, &a).expect_err("半给的入参该拒");
        assert_eq!(e.0, "bad_args");
        assert_eq!(*lock(&a), before, "拒了还动了表：{bad}");
    }
}

/// 可达表有界：满了拒新的一台，已在表里的照样能再登记。
#[test]
fn the_table_is_bounded() {
    let t = Table::default();
    for i in 0..MAX_REACH {
        register(&t, &reach_args(&format!("m{i}"), "h")).unwrap();
    }
    assert_eq!(
        register(&t, &reach_args("one-more", "h")).unwrap_err().0,
        "bad_args"
    );
    register(&t, &reach_args("m0", "h9")).expect("已在表里的那台照样能再登记");
    assert_eq!(lock(&t).len(), MAX_REACH);
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

/// A4 ★ 远端永不答：外层 future 被丢（= 后端收到 `cancel` 打断了外层）⇒ 内层任务被收、那一格放掉。
/// 另一向：内层正常答完 ⇒ 结果照常、那一格同样放掉。异源：真 tokio 任务 ＋ 哨兵（不看源码）。
#[tokio::test]
async fn an_abandoned_ask_takes_its_inner_task_down_with_it() {
    // ① 永不答：内层攥着哨兵、永远等上行（上行那根管子外层一个字节都不写）。
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let outer = pull_over_coded(move |mut up_r, down_w| async move {
        let _slot = Sentinel(Some(tx));
        let _keep = down_w; // 下行不关：外层就一直等 ack（远端不答的那一形）
        let mut b = [0u8; 1];
        let _ = tokio::io::AsyncReadExt::read(&mut up_r, &mut b).await;
        std::future::pending::<()>().await;
    });
    // 外层等一会儿还没答 ⇒ 调用方期限到点 ⇒ 外层被丢（`timeout` 到点即丢它）。
    let gave_up = tokio::time::timeout(std::time::Duration::from_millis(200), outer).await;
    assert!(gave_up.is_err(), "永不答的那一趟竟然答了");
    tokio::time::timeout(std::time::Duration::from_secs(10), rx)
        .await
        .expect("外层被丢 10 秒了，内层任务还攥着那一格 —— 它脱钩了（格永远占着）")
        .expect("哨兵没报信就没了");

    // ② 另一向：正常答完 ⇒ 结果照常，那一格同样放掉。
    let (tx2, rx2) = tokio::sync::oneshot::channel::<()>();
    let got = pull_over_coded(move |_up_r, mut down_w| async move {
        let _slot = Sentinel(Some(tx2));
        let ack = "{\"ok\":true}\n";
        let res = "{\"stdout\":\"答\",\"stderr\":\"\",\"exit_status\":0}\n";
        let _ = tokio::io::AsyncWriteExt::write_all(&mut down_w, ack.as_bytes()).await;
        let _ = tokio::io::AsyncWriteExt::write_all(&mut down_w, res.as_bytes()).await;
        std::future::pending::<()>().await;
    })
    .await
    .map_err(|s| s.message);
    assert_eq!(got, Ok("答".to_string()));
    tokio::time::timeout(std::time::Duration::from_secs(10), rx2)
        .await
        .expect("答完 10 秒了，内层任务还在")
        .expect("哨兵没报信就没了");
}

/// ★ 〔`INVARIANTS §47` ②〕一次性子命令的 argv 是自由文本：拒绝集只收 NUL / CR / LF（**不拒 shell 元字符**），
/// 判不过一次都不拨；真实名字（带 `'` `(` `&` 的目录名 · 中文 · 空格）照发（拒过头同样违反 §47）。
#[tokio::test]
async fn one_shot_argv_refuses_only_what_the_quote_cannot_hold() {
    let table = Table::default();
    answer_reach_with(&reach_args("dev", "10.0.0.2"), &table).unwrap();
    let far = Recorder::default();
    for good in ["-home-u-Bob's notes", "照片 (2019)", "a & b; c"] {
        ask_with("dev", &["--list-sessions", good], &table, &far)
            .await
            .unwrap_or_else(|e| panic!("真实好值被拒了：{good:?} ⇒ {e}"));
    }
    assert_eq!(far.n.load(Ordering::SeqCst), 3);
    for bad in ["a\nb", "a\rb", "a\0b"] {
        let e = ask_with("dev", &["--search", bad], &table, &far)
            .await
            .expect_err(&format!("坏值拼进远端命令了：{bad:?}"));
        assert!(e.contains("dev") && e.contains(&format!("{bad:?}")), "{e}");
    }
    assert_eq!(far.n.load(Ordering::SeqCst), 3, "拒了却还是去拨了");
}

/// 交给 capture 的 stdin 真进了拨号请求的 `capture.stdin`（缺席 = 一个字节不写），
/// 命令原样、用法是 capture —— 生产那一个对面（`DialRemote`）就是拿这份请求去跑 `dial::uses::run` 的。
/// ⚠ 买不到：「写进远端进程 stdin」那一跳要真 sshd（读数见 `W5-AUX.md §7`，不进门禁）。
#[test]
fn the_capture_request_carries_the_stdin_line_verbatim_and_only_when_given() {
    let dial = json!({"machine": {"host": "h", "port": 22, "user": "u", "keyPath": "/k"}});
    let with = crate::stream::remote_ask::capture_request(
        &dial,
        "'/b' '--x'".into(),
        Some("{\"a\":1}\n".into()),
    )
    .expect("拼得出请求");
    assert_eq!(with.command, "'/b' '--x'");
    assert_eq!(with.use_, crate::dial::Use::Capture);
    let cap = with.capture.expect("capture 参数在");
    assert_eq!(
        cap.stdin.as_deref(),
        Some("{\"a\":1}\n"),
        "载荷没进 capture.stdin"
    );
    assert_eq!(
        cap.abort_marker.as_deref(),
        Some(crate::stream::remote_ask::HELLO_MARKER)
    );
    let without =
        crate::stream::remote_ask::capture_request(&dial, "'/b'".into(), None).expect("拼得出请求");
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
    let run = |stderr: &'static str| {
        pull_over_coded(move |_up_r, mut down_w| async move {
            let ack = "{\"ok\":true}\n";
            let res = format!(
                "{}\n",
                json!({ "stdout": "", "stderr": stderr, "exit_status": 2 })
            );
            let _ = tokio::io::AsyncWriteExt::write_all(&mut down_w, ack.as_bytes()).await;
            let _ = tokio::io::AsyncWriteExt::write_all(&mut down_w, res.as_bytes()).await;
            std::future::pending::<()>().await;
        })
    };
    let e = run("{\"code\":\"stale\",\"message\":\"盘上那份变了\"}")
        .await
        .unwrap_err();
    assert_eq!(e.code.as_deref(), Some("stale"));
    assert!(e.message.contains("盘上那份变了"), "{e:?}");
    let e = run("bash: boom").await.unwrap_err();
    assert_eq!(e.code, None);
    assert!(e.message.contains("boom"), "{e:?}");
}
