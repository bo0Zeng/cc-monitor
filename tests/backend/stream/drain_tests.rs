//! **退出之前先排空停不下来的那一档** —— 判据。
//!
//! 守的要求（住址）：
//! - 要求：「后端收 SIGTERM 先排空在飞写（有上限）再退」；
//! - 审计 E §E2：「流模式后端 `exit(0)` 或被 SIGKILL 时，**不等正在跑的阻塞写做完**」；
//! -：「**值**住后端 · **执行**住通信层」—— 本件的「上限」由叫它退的那一方执行（第二次停机信号 ⇒ 立刻退）。
//! 设计与读数住 §1。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | D1 | 闸本体：有票时 `drained()` 不完成、最后一张票落下即完成；关闸之后 `enter()` 取不到；`close()` 回的数 == 在飞票数 | 两向，逐拍 poll（不靠睡） |
//! | D2 | 真子进程 ＋ 真信号：SIGTERM 之后在飞的阻塞命令没做完，进程就**不退**、新来的阻塞命令回 `shutting_down`；放开它 ⇒ 退 0。另一趟：再补一次 SIGTERM ⇒ 不等了 | re-exec 测试二进制当子进程；PATH 上一个假 `tmux` 卡在一根 FIFO 上 |
//! | D3 | 接线：取票恰好一处、在 `SpawnBlocking` 那一支、在 `spawn_blocking(` 之前；`main.rs` 生产段 `exit(` 零处（流模式的 exit 只住 `exit_after_drain`），`exit_after_drain(` 调用点 == 登记的五处 | 文本，两向 ＋ 正控 |

use super::*;

// ── D1 ────────────────────────────────────────────────────────────────────

/// 不靠睡地 poll 一次：返回「这一拍完成没有」。
fn polled_ready<F: std::future::Future>(f: std::pin::Pin<&mut F>) -> bool {
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    f.poll(&mut cx).is_ready()
}

#[test]
fn d1_the_gate_waits_for_every_ticket_and_refuses_after_close() {
    let d: &'static Drain = Box::leak(Box::new(Drain::new()));
    // 空闸：一拍就完成（正控 —— 不然下面「没完成」那几格可能只是它永远不完成）。
    {
        let w = d.drained();
        tokio::pin!(w);
        assert!(polled_ready(w.as_mut()), "没有在飞的票，drained 却不完成");
    }
    let a = d
        .enter("files-put（id=a）".into())
        .expect("没关闸时取得到票");
    let b = d
        .enter("terminal-preview（id=b）".into())
        .expect("没关闸时取得到第二张");
    assert_eq!(
        d.in_flight_names(),
        vec![
            "files-put（id=a）".to_string(),
            "terminal-preview（id=b）".to_string()
        ],
        "在飞的名字（按起跑先后）"
    );
    assert_eq!(d.in_flight(), 2);
    assert_eq!(d.close(), 2, "close 回的数应 == 关的那一刻在飞的票数");
    assert!(
        d.enter("x".into()).is_none(),
        "关闸之后还取得到票 —— 收场期间会再起新的阻塞活"
    );
    let w = d.drained();
    tokio::pin!(w);
    assert!(
        !polled_ready(w.as_mut()),
        "还有 2 张票在飞，drained 就完成了"
    );
    drop(a);
    assert_eq!(
        d.in_flight_names(),
        vec!["terminal-preview（id=b）".to_string()],
        "落下的那张要从名单里摘掉"
    );
    assert!(
        !polled_ready(w.as_mut()),
        "还有 1 张票在飞，drained 就完成了"
    );
    drop(b);
    assert!(
        polled_ready(w.as_mut()),
        "最后一张票落下之后 drained 仍不完成"
    );
    assert_eq!(d.close(), 0, "关过再关：回此刻在飞数");
}

// ── D2 ────────────────────────────────────────────────────────────────────

const CHILD_MARK: &str = "CCM_HX1_DRAIN_CHILD";
const CHILD_TEST_NAME: &str = "stream::inbound::drain_tests::d2_child_harness";
const READY_LINE: &str = "HX1-HARNESS-READY";
/// D4：交给子进程的短期限（毫秒）。
const CHILD_DEADLINE_MS: &str = "CCM_HX1_DRAIN_DEADLINE_MS";

/// 子进程那一半：与 `main.rs::run_over_stdio` 收信号那一支同形 —— stdin 进 `inbound::spawn`、应答写 stdout，
/// 收到停机信号 ⇒ `exit_after_drain`（写者照常跑）。**跑的全是生产那几样**：取票的门 · 闸 · 收场函数 · 信号监听 ·
/// 一条真的阻塞命令（`terminal-preview` 起 `tmux`，PATH 上那个是假的）。只有「把它们拼起来」这一层是台架（接线由 D3 钉）。
#[test]
#[ignore = "HX1 D2 的子进程那一半：只由 d2_* 父进程 re-exec 起来"]
fn d2_child_harness() {
    if std::env::var(CHILD_MARK).is_err() {
        return;
    }
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        use tokio::io::AsyncWriteExt as _;
        let (tx, mut rx) = mpsc::channel::<Frame>(REPLY_CHANNEL_CAPACITY);
        let _reader = spawn(
            tokio::io::stdin(),
            tx,
            crate::stream::wire::HelloFlushed::for_tests(),
        );
        let stop = shutdown_listener();
        let mut out = tokio::io::stdout();
        out.write_all(format!("{READY_LINE}\n").as_bytes())
            .await
            .expect("写就绪行");
        out.flush().await.expect("flush");
        let writer = async move {
            while let Some(f) = rx.recv().await {
                let line = crate::stream::wire::to_line(&f).expect("serialize");
                if out.write_all(line.as_bytes()).await.is_err() || out.flush().await.is_err() {
                    return;
                }
            }
        };
        tokio::pin!(writer);
        // 期限：生产那一个（`DRAIN_DEADLINE`）；D4 那一趟交一个短的（量「到点就退」）。
        let deadline = std::env::var(CHILD_DEADLINE_MS)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(std::time::Duration::from_millis)
            .unwrap_or(DRAIN_DEADLINE);
        tokio::select! {
            _ = &mut writer => {}
            _ = stop => {
                exit_after_drain_within("收到停机信号", Some(writer), deadline).await
            }
        }
    });
}

/// 一台假 `tmux`：头一次被起来就先往 `ready` 那根 FIFO 说一声，再卡在 `gate` 那根 FIFO 上，直到父进程放开。
struct Rig {
    dir: std::path::PathBuf,
    ready: std::path::PathBuf,
    gate: std::path::PathBuf,
}

impl Rig {
    fn new(tag: &str) -> Rig {
        let dir = std::env::temp_dir().join(format!(
            "ccm-hx1-drain-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).expect("台架目录");
        let ready = dir.join("ready.fifo");
        let gate = dir.join("gate.fifo");
        for f in [&ready, &gate] {
            let st = std::process::Command::new("mkfifo")
                .arg(f)
                .status()
                .expect("mkfifo");
            assert!(st.success(), "mkfifo {} 失败", f.display());
        }
        // 只有头一次被起来时报到并卡住；同一条命令后面再起的几次 tmux（列客户端那一下）直接退。
        let script = format!(
            "#!/bin/sh\nif mkdir '{}' 2>/dev/null; then echo started > '{}'; cat '{}' > /dev/null; fi\nexit 0\n",
            dir.join("once").display(),
            ready.display(),
            gate.display()
        );
        let tmux = bin.join("tmux");
        std::fs::write(&tmux, script).expect("写假 tmux");
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Rig { dir, ready, gate }
    }

    /// 等假 tmux 真被起来（读 `ready` 那根 FIFO：没人写就一直卡着 —— 事件，不是睡）。
    fn wait_started(&self) {
        let got = std::fs::read_to_string(&self.ready).expect("读 ready FIFO");
        assert_eq!(got.trim(), "started");
    }

    /// 放开卡着的那个假 tmux。
    fn release(&self) {
        std::fs::write(&self.gate, b"go\n").expect("写 gate FIFO");
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        // 判据中途红了（panic）也要把卡在 gate 上的假 tmux 放掉：非阻塞地开一次写端 —— 有人在读就放开它，没人读就当场 ENXIO。
        use std::os::unix::fs::OpenOptionsExt as _;
        let _ = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&self.gate);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 子进程 ＋ 它两根输出管子的逐行收集。
struct Child {
    proc: std::process::Child,
    stdin: std::process::ChildStdin,
    out: std::sync::mpsc::Receiver<String>,
    err: std::sync::mpsc::Receiver<String>,
    err_all: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

/// 判据红了（panic）也收掉子进程（第 21 条：自己起的进程用完收掉）—— 死值验那一趟实测：K1 刀下子进程永远等不到排空，
/// 父进程红了之后它一直活着、还攥着 cargo 的输出管子。
impl Drop for Child {
    fn drop(&mut self) {
        if matches!(self.proc.try_wait(), Ok(None)) {
            let _ = self.proc.kill();
        }
        let _ = self.proc.wait();
    }
}

/// 一个等待的上界（测试侧；后端生产段零定时器那条铁律不管测试段）。只为「坏了的时候别把 CI 挂死」。
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(30);

fn lines_of<R: std::io::Read + Send + 'static>(
    r: R,
    keep: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
) -> std::sync::mpsc::Receiver<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead as _;
        for line in std::io::BufReader::new(r).lines().map_while(Result::ok) {
            if let Some(k) = &keep {
                k.lock().unwrap().push(line.clone());
            }
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    rx
}

fn start_child(rig: &Rig) -> Child {
    start_child_with(rig, None)
}

fn start_child_with(rig: &Rig, deadline_ms: Option<u64>) -> Child {
    let path = format!(
        "{}:{}",
        rig.dir.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut proc = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([
            CHILD_TEST_NAME,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_MARK, "1")
        .env("PATH", path)
        .env("HOME", &rig.dir)
        .env("RUST_LOG", "info")
        .env(
            CHILD_DEADLINE_MS,
            deadline_ms.map(|v| v.to_string()).unwrap_or_default(),
        )
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("起子进程");
    let stdin = proc.stdin.take().expect("stdin");
    let out = lines_of(proc.stdout.take().expect("stdout"), None);
    let err_all = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err = lines_of(proc.stderr.take().expect("stderr"), Some(err_all.clone()));
    let c = Child {
        proc,
        stdin,
        out,
        err,
        err_all,
    };
    // libtest 在跑之前先印「test <名字> ... 」且不换行 ⇒ 就绪行接在它后面，按结尾认。
    wait_line(&c.out, |l| l.ends_with(READY_LINE), "子进程的就绪行");
    c
}

fn wait_line(
    rx: &std::sync::mpsc::Receiver<String>,
    want: impl Fn(&str) -> bool,
    what: &str,
) -> String {
    let until = std::time::Instant::now() + PATIENCE;
    loop {
        let left = until.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(l) if want(&l) => return l,
            Ok(_) => continue,
            Err(e) => panic!("等 {what} 没等到（{e:?}）"),
        }
    }
}

fn send_line(c: &mut Child, line: &str) {
    use std::io::Write as _;
    c.stdin.write_all(line.as_bytes()).expect("写命令");
    c.stdin.write_all(b"\n").expect("写换行");
    c.stdin.flush().expect("flush");
}

fn sigterm(c: &Child) {
    // SAFETY: 发给我们自己起的、尚未被收尸的子进程。
    let rc = unsafe { libc::kill(c.proc.id() as libc::pid_t, libc::SIGTERM) };
    assert_eq!(rc, 0, "发 SIGTERM 失败");
}

fn wait_exit(c: &mut Child) -> std::process::ExitStatus {
    let until = std::time::Instant::now() + PATIENCE;
    loop {
        if let Some(st) = c.proc.try_wait().expect("try_wait") {
            return st;
        }
        assert!(
            std::time::Instant::now() < until,
            "子进程过了 {PATIENCE:?} 还没退；stderr：{:?}",
            c.err_all.lock().unwrap()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

const CAPTURE_A: &str = r#"{"id":"a","cmd":"terminal-preview","args":{"sid":"hx1-a"}}"#;
const CAPTURE_B: &str = r#"{"id":"b","cmd":"terminal-preview","args":{"sid":"hx1-b"}}"#;

/// 起子进程 → 发一条会卡住的阻塞命令 → 等假 tmux 真起来 → SIGTERM → 等到「收尾：还有 1 条」那句。
fn up_to_draining(tag: &str) -> (Rig, Child) {
    up_to_draining_with(tag, None)
}

fn up_to_draining_with(tag: &str, deadline_ms: Option<u64>) -> (Rig, Child) {
    let rig = Rig::new(tag);
    let mut c = start_child_with(&rig, deadline_ms);
    send_line(&mut c, CAPTURE_A);
    rig.wait_started();
    sigterm(&c);
    wait_line(
        &c.err,
        |l| l.contains("收尾：还有 1 条"),
        "「收尾：还有 1 条」那句（排空开始）",
    );
    (rig, c)
}

#[test]
fn d2_sigterm_waits_for_the_blocking_command_and_refuses_new_ones() {
    let (rig, mut c) = up_to_draining("wait");
    // 排空中：进程还在（那条阻塞命令没做完之前它不可能退 —— 假 tmux 还卡在 gate 上）。
    assert!(
        c.proc.try_wait().expect("try_wait").is_none(),
        "收到 SIGTERM 之后、在飞那条还卡着，进程就退了 —— 没有排空"
    );
    // 排空中新来的阻塞命令：一个字节不动、回协议级 `shutting_down`。
    send_line(&mut c, CAPTURE_B);
    let refused = wait_line(&c.out, |l| l.contains(r#""id":"b""#), "b 的应答");
    let v: serde_json::Value = serde_json::from_str(&refused).expect("应答是 JSON");
    assert_eq!(v["ok"], serde_json::json!(false), "{refused}");
    assert_eq!(v["code"], serde_json::json!(SHUTTING_DOWN), "{refused}");
    // 放开 ⇒ 在飞那条做完 ⇒ 退 0，并说「都做完了」。
    rig.release();
    let st = wait_exit(&mut c);
    assert_eq!(st.code(), Some(0), "排空完应退 0：{st:?}");
    let err = c.err_all.lock().unwrap().join("\n");
    assert!(err.contains("在跑的那几条都做完了"), "stderr：{err}");
    assert!(!err.contains("又收到一次停机信号"), "stderr：{err}");
}

#[test]
fn d2_a_second_sigterm_stops_waiting() {
    let (rig, mut c) = up_to_draining("again");
    assert!(
        c.proc.try_wait().expect("try_wait").is_none(),
        "第一次 SIGTERM 就退了 —— 没有排空"
    );
    sigterm(&c);
    let st = wait_exit(&mut c);
    assert_eq!(st.code(), Some(0), "{st:?}");
    let err = c.err_all.lock().unwrap().join("\n");
    assert!(
        err.contains("又收到一次停机信号") && err.contains("还有 1 条没做完"),
        "第二次 SIGTERM 应当不等、并说还剩几条：{err}"
    );
    // 假 tmux 还卡着（它是孙进程，子进程退了它还在）—— 放开它，别留垃圾。
    rig.release();
}

// ── D4 ────────────────────────────────────────────────────────────────────

/// **到期限仍没排空 ⇒ 说出哪几条没做完，然后退**（不再无限期留着）。
/// 守的要求：「后端自己兜一个退出排空期限 …… 到点仍未排空 ⇒ 记一行日志说哪几条没做完，然后退出」；
/// `INVARIANTS §48.2`「脱离后不留僵尸」。形状：同 D2 的真子进程台架，期限交 800ms、gate 一直不放 ⇒
/// 子进程自己退 0，stderr 里那一行点名 `terminal-preview（id=a）`；对照：D2 那一趟（期限是生产的 30 秒）放开之前一直在。
#[test]
fn d4_the_drain_deadline_names_what_was_left_and_exits() {
    let (rig, mut c) = up_to_draining_with("deadline", Some(800));
    let st = wait_exit(&mut c);
    assert_eq!(st.code(), Some(0), "{st:?}");
    let err = c.err_all.lock().unwrap().join("\n");
    assert!(
        err.contains("排空期限") && err.contains("terminal-preview（id=a）"),
        "到点了应当说出哪一条没做完：{err}"
    );
    assert!(!err.contains("都做完了"), "{err}");
    rig.release();
}

// ── D3 ────────────────────────────────────────────────────────────────────

/// 数 `needle` 调用点（`needle` 后面紧跟 `(`，前一个字符不是标识符字符）。
fn call_sites(code: &str, needle: &str) -> Vec<usize> {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    code.match_indices(needle)
        .filter(|(i, _)| !code[..*i].chars().next_back().is_some_and(ident))
        .filter(|(i, _)| code[i + needle.len()..].trim_start().starts_with('('))
        .map(|(i, _)| i)
        .collect()
}

/// 所在函数名（往回找最近的 `fn 名字`）。
fn enclosing_fn(code: &str, at: usize) -> String {
    let head = &code[..at];
    let Some(i) = head.rfind("fn ") else {
        return String::new();
    };
    head[i + 3..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

#[test]
fn d3_the_ticket_is_taken_once_before_the_blocking_spawn_and_every_stream_exit_drains() {
    let inbound = crate::guard_support::production_code(include_str!(
        "../../../src/backend/stream/inbound/mod.rs"
    ));
    // 取票：恰好一处，在 `handle_line`（`SpawnBlocking` 那一支）里，且在 `spawn_blocking(` 之前。
    let enters = call_sites(&inbound, "DRAIN.enter");
    assert_eq!(enters.len(), 1, "取票应恰好一处：{enters:?}");
    assert_eq!(enclosing_fn(&inbound, enters[0]), "handle_line");
    let blocking = call_sites(&inbound, "spawn_blocking");
    assert_eq!(
        blocking.len(),
        1,
        "`spawn_blocking(` 应恰好一处：{blocking:?}"
    );
    assert!(enters[0] < blocking[0], "取票排在起跑之后了");
    let arm = inbound[..enters[0]]
        .rfind("Disposition::SpawnBlocking")
        .expect("那一支");
    let after_arm = arm + "Disposition::SpawnBlocking".len();
    assert!(
        !inbound[after_arm..enters[0]].contains("Disposition::"),
        "取票不在 SpawnBlocking 那一支里"
    );

    // `process::exit(` 的所在函数那一格挪进 [`x15_the_process_exits_only_in_main_and_exit_after_drain`]（全树，不只 `main.rs`）。
    let main = crate::guard_support::production_code(include_str!("../../../src/backend/main.rs"));
    let mut drains: Vec<String> = call_sites(&main, "exit_after_drain")
        .into_iter()
        .map(|i| enclosing_fn(&main, i))
        .collect();
    drains.sort();
    assert_eq!(
        drains,
        vec![
            "main",            // 常驻载体收到停机信号
            "run_over_stdio",  // hello 写不出去
            "run_over_stdio",  // 收到停机信号（写者照常跑）
            "run_over_stdio",  // 对端走了（写不出去）
            "serve_listening", // 最后一个客户走了且退出行为是「结束」
        ],
        "流模式的收场口集合变了"
    );

    // 生产入口交给本体的期限恰是 `DRAIN_DEADLINE`（判据用短期限走的是同一个本体）。收场那几样住 `drain.rs`。
    let drain = crate::guard_support::production_code(include_str!(
        "../../../src/backend/stream/inbound/drain.rs"
    ));
    let within: Vec<usize> = call_sites(&drain, "exit_after_drain_within");
    assert_eq!(within.len(), 1, "本体只该被生产入口调一处：{within:?}");
    assert_eq!(enclosing_fn(&drain, within[0]), "exit_after_drain");
    assert_eq!(
        guard_core::find_pinned(
            &drain,
            "exit_after_drain_within(why, writer, DRAIN_DEADLINE)"
        ),
        Ok(within[0]),
        "生产入口交的期限不是 DRAIN_DEADLINE"
    );
}

/// **退出口只有 `main` 与 `exit_after_drain`**：后端生产树里 `process::exit(` 的所在函数，全树逐处现打。
///
/// 期望（异源：取自 ⑮ 那句裁决与 `main.rs` 的分派形状，不从被扫的源码现推）：
/// `main.rs::main` 三处（ccm 那一趟 · 一次性查询 · 监听口配置不成立或绑不上口，`claim_then_log` 交回的码）——
/// 都发生在**一条命令都还没收**之前、没有可排空的；`stream/inbound/drain.rs::exit_after_drain_within` 一处
/// （`exit_after_drain` 的本体：生产入口只经它，期限由 d3 钉）。别处一处都不许有 —— 模块里想退就把退出码交回调用方。
#[test]
fn x15_the_process_exits_only_in_main_and_exit_after_drain() {
    let root = crate::guard_support::src_root();
    let mut found: Vec<(String, String)> = Vec::new();
    let mut scanned = 0usize;
    for (path, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let prod = crate::guard_support::production_code(&src);
        for i in call_sites(&prod, "process::exit") {
            found.push((rel.clone(), enclosing_fn(&prod, i)));
        }
    }
    assert!(
        scanned >= 100,
        "只扫到 {scanned} 份后端源文件 —— 遍历坏了，本条在空转"
    );
    found.sort();
    let want: Vec<(String, String)> = [
        ("main.rs", "main"),
        ("main.rs", "main"),
        ("main.rs", "main"),
        ("stream/inbound/drain.rs", "exit_after_drain_within"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    assert_eq!(
        found, want,
        "后端生产树里 `process::exit(` 的所在函数变了 —— 退出口只许是 `main` 与 `exit_after_drain`（⑮）；\
         模块里想退，把退出码交回 `main`"
    );
    // 正控：数法认得出多出来的一处 `exit(`，也认得出它住哪个函数。
    let planted = "fn planted() { std::process::exit(0); }\n";
    let at = call_sites(planted, "process::exit");
    assert_eq!(at.len(), 1);
    assert_eq!(enclosing_fn(planted, at[0]), "planted");
}

// ── A1（同住本文件：都是 `main.rs` 流模式那几行的接线）────────────────────

/// 〔NT2 问 3 ＋ RT1 F3〕后端 `tracing` 只在 stderr 是终端时上色。
/// 守的要求：「写进文件 / monitor 日志的 stderr 关 ANSI 颜色」。
/// 形状：`main.rs` 生产段里 `.with_ansi(` 恰好一处，参数恰是「stderr 是不是终端」；`fmt()` 恰好一处（没有第二个不设它的初始化）。
#[test]
fn a1_the_backend_log_is_colored_only_on_a_terminal() {
    let main = crate::guard_support::production_code(include_str!("../../../src/backend/main.rs"));
    let at: Vec<usize> = main.match_indices(".with_ansi(").map(|(i, _)| i).collect();
    assert_eq!(at.len(), 1, "`.with_ansi(` 不是恰好一处：{at:?}");
    let arg = &main[at[0] + ".with_ansi(".len()..];
    let arg = &arg[..arg.find('\n').unwrap_or(arg.len())];
    assert!(
        arg.starts_with("std::io::IsTerminal::is_terminal(&std::io::stderr()))"),
        "上不上色不再看 stderr 是不是终端：{arg}"
    );
    assert_eq!(
        main.matches("tracing_subscriber::fmt()").count(),
        1,
        "多了一个 tracing 初始化"
    );
}
