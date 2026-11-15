//! **退出之前先排空停不下来的那一档** —— 判据。
//!
//! 守的要求（住址）：
//! - 要求：「后端收 SIGTERM 先排空在飞写（有上限）再退」；
//! - 审计 E §E2：「流模式后端 `exit(0)` 或被 SIGKILL 时，**不等正在跑的阻塞写做完**」；
//! - 「**值**住后端 · **执行**住通信层」—— 这里的「上限」由叫它退的那一方执行（第二次停机信号 ⇒ 立刻退）。
//!
//! 设计与读数住 §1。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | D1 | 闸本体：有票时 `drained()` 不完成、最后一张票落下即完成；关闸之后 `enter()` 取不到；`close()` 回的数 == 在飞票数 | 两向，逐拍 poll（不靠睡） |
//! | D2 | 真子进程 ＋ 真信号：SIGTERM 之后在飞的阻塞命令没做完，进程就**不退**、新来的阻塞命令回 `shutting_down`；放开它 ⇒ 退 0。另一趟：再补一次 SIGTERM ⇒ 不等了 | re-exec 测试二进制当子进程；PATH 上一个假 `tmux` 卡在一根 FIFO 上 |
//! | D5 | 同 D2 的台架（写者是生产那一个）：在飞那条在排空末尾做完 ⇒ 它的应答写到了 stdout 才退 0 | 压一百趟，趟趟要到 |
//! | D6 | 同 D4 的台架，另登记一个不返回的写者：命令做完、写者没写完就到期限 ⇒ 照实说「还有 1 个写者没写完，排着的应答没写出」再退 0 | 一趟 |
//! | D7 | 闸的写者那一半：`finishing()` 叫收尾之前不完成、之后完成；`writers_done()` 有写者时不完成、最后一个落下即完成 | 逐拍 poll |
//! | D8 | 同 D2 的台架跑常驻那条连接的收场 `stream_ends`：对端关了写半边、读还开着 ⇒ 已收的那条阻塞命令做完，应答照样写到 stdout，流随后自己收场 | 一趟 |
//! | D3 | 接线：取票恰好一处、在 `SpawnBlocking` 那一支、在 `spawn_blocking(` 之前；`stream/entry.rs` 流模式那几段 `exit(` 零处（流模式的 exit 只住 `exit_after_drain`），`exit_after_drain(` 调用点 == 登记的五处 | 文本，两向 ＋ 正控 |

use super::*;
use crate::wire::drain::{shutdown_listener, DRAIN_DEADLINE, SHUTTING_DOWN};

// ── D1 ────────────────────────────────────────────────────────────────────

/// 不靠睡地 poll 一次：返回「这一次完成没有」。
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

// ── D7 ────────────────────────────────────────────────────────────────────

#[test]
fn d7_writers_finish_only_when_told_and_the_gate_waits_for_the_last() {
    let d: &'static Drain = Box::leak(Box::new(Drain::new()));
    {
        let w = d.writers_done();
        tokio::pin!(w);
        assert!(polled_ready(w.as_mut()), "没有写者，writers_done 却不完成");
    }
    let a = d.writer();
    let b = d.writer();
    assert_eq!(d.writers(), 2);
    let f = d.finishing();
    tokio::pin!(f);
    assert!(!polled_ready(f.as_mut()), "没叫收尾，finishing 就完成了");
    let w = d.writers_done();
    tokio::pin!(w);
    assert!(
        !polled_ready(w.as_mut()),
        "还有 2 个写者，writers_done 就完成了"
    );
    d.finish_writers();
    assert!(polled_ready(f.as_mut()), "叫了收尾，finishing 仍不完成");
    {
        let late = d.finishing();
        tokio::pin!(late);
        assert!(
            polled_ready(late.as_mut()),
            "叫过收尾之后才来等的写者等不到"
        );
    }
    drop(a);
    assert!(
        !polled_ready(w.as_mut()),
        "还有 1 个写者，writers_done 就完成了"
    );
    drop(b);
    assert!(
        polled_ready(w.as_mut()),
        "最后一个写者落下之后 writers_done 仍不完成"
    );
}

// ── D2 ────────────────────────────────────────────────────────────────────

const CHILD_MARK: &str = "CCM_HX1_DRAIN_CHILD";
const CHILD_TEST_NAME: &str = "stream::inbound::drain_tests::d2_child_harness";
const READY_LINE: &str = "HX1-HARNESS-READY";
/// D4：交给子进程的短期限（毫秒）。
const CHILD_DEADLINE_MS: &str = "CCM_HX1_DRAIN_DEADLINE_MS";
/// 子进程跑哪一形：`stuck` ＝ D6（另登记一个一直不返回的写者）· `halfclose` ＝ D8（常驻那条连接的收场 `stream_ends`）· 别的 ＝ 收信号排空。
const CHILD_MODE: &str = "CCM_HX1_DRAIN_MODE";
/// D8：子进程里那条流收场之后说的那一句。
const HALFCLOSE_ENDED: &str = "HX1-HALFCLOSE-ENDED";

/// 子进程那一半：与 `stream/entry.rs::run_over_stdio` 收信号那一支同形 —— stdin 进 `inbound::spawn`、应答写 stdout，
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
        let (tx, rx) = mpsc::channel::<Frame>(REPLY_CHANNEL_CAPACITY);
        let reader = spawn(
            tokio::io::stdin(),
            tx.clone(),
            crate::stream::inbound::WatchDesk::new(|_| {
                tokio::sync::oneshot::channel::<Vec<crate::wire::WatchFrom>>().1
            }),
            crate::platform::fs::Own::for_tests(),
            crate::wire::HelloFlushed::for_tests(),
        );
        let mode = std::env::var(CHILD_MODE).unwrap_or_default();
        // D6：一个写者一直不返回（替卡在不读的对端上的那一个）—— 登记了就握到退出。
        let _stuck = (mode == "stuck").then(|| crate::wire::drain::DRAIN.writer());
        let stop = shutdown_listener();
        let mut out = tokio::io::stdout();
        out.write_all(format!("{READY_LINE}\n").as_bytes())
            .await
            .expect("写就绪行");
        out.flush().await.expect("flush");
        // 写者是生产那一个（`stream/writer.rs::writer_task`）：出方向一直开着（`_line_tx` 握到退出，同 stdio 收信号那一支握着这条流在 watcher 上的那一份），
        //   tap 那一路一直没有东西。
        let (_line_tx, line_rx) = crate::observe::watcher::frame_channel(
            crate::observe::watcher::CHANNEL_MAX_BYTES.as_len(),
        );
        if mode == "halfclose" {
            // D8：常驻那条连接的收场（`stream/entry.rs::stream_ends`），读写两半都是生产那一个。
            //   应答那一路的发送端只留读循环与在飞命令手里的（同生产那一支：起收场之前放掉自己那一份）。
            drop(tx);
            crate::stream::entry::stream_ends(
                crate::stream::writer::writer_task(
                    out,
                    line_rx,
                    rx,
                    NoTap,
                    Default::default(),
                    None,
                ),
                reader,
            )
            .await;
            tracing::info!("{HALFCLOSE_ENDED}");
            return;
        }
        drop(tx);
        let writer = async move {
            crate::stream::writer::writer_task(out, line_rx, rx, NoTap, Default::default(), None)
                .await;
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

/// 子进程台架的 tap 那一路：一直没有东西（生产是活卡 hub）。
struct NoTap;

impl crate::stream::tap::TapSource for NoTap {
    async fn next(&mut self) -> Option<Frame> {
        std::future::pending().await
    }
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
        // 马上被起 ⇒ 经 `write_exe` 写（本进程写的写端会漏进别的判据线程 fork 出来的子进程 ⇒ ETXTBSY）。
        test_home::write_exe(&tmux, &script);
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
    /// 入方向；D8 放掉它 ＝ 关上对端的写半边。
    stdin: Option<std::process::ChildStdin>,
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

/// 一个等待的上界（测试侧；后端生产段零定时器那条铁律不管测试段）。只为「坏了的时候别把 CI 挂死」：兜底的那一个数。
const PATIENCE: std::time::Duration = guard_core::wall::CEILING;

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

fn start_child_with(rig: &Rig, deadline_ms: Option<u64>, mode: &str) -> Child {
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
        .env(CHILD_MODE, mode)
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
        stdin: Some(stdin),
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
    let w = c.stdin.as_mut().expect("入方向还开着");
    w.write_all(line.as_bytes()).expect("写命令");
    w.write_all(b"\n").expect("写换行");
    w.flush().expect("flush");
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

/// 等它退之前说的最后那一句到了，再交回 stderr 全文。
/// 不能「进程退了就读全文」：stderr 那根管子由另一条线程逐行收，进程退了那一刻，最后一两行可能还在管子里没收上来
///   （机器忙时撞得到：退出码已经是 0，全文里却还只有「收尾」那一句）。等的是那一行这件事，兜底是那个统一的上限。
fn said_through(c: &Child, last: &str, what: &str) -> String {
    wait_line(&c.err, |l| l.contains(last), what);
    c.err_all.lock().unwrap().join("\n")
}

const CAPTURE_A: &str = r#"{"id":"a","cmd":"terminal-preview","args":{"sid":"hx1-a"}}"#;
const CAPTURE_B: &str = r#"{"id":"b","cmd":"terminal-preview","args":{"sid":"hx1-b"}}"#;

/// 起子进程 → 发一条会卡住的阻塞命令 → 等假 tmux 真起来 → SIGTERM → 等到「收尾：还有 1 条」那句。
fn up_to_draining(tag: &str) -> (Rig, Child) {
    up_to_draining_with(tag, None, "drain")
}

fn up_to_draining_with(tag: &str, deadline_ms: Option<u64>, mode: &str) -> (Rig, Child) {
    let rig = Rig::new(tag);
    let mut c = start_child_with(&rig, deadline_ms, mode);
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
    let err = said_through(&c, "在跑的那几条都做完了", "「都做完了」那句");
    let st = wait_exit(&mut c);
    assert_eq!(st.code(), Some(0), "排空完应退 0：{st:?}；stderr：{err}");
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
    let err = said_through(
        &c,
        "还有 1 条没做完",
        "「又收到一次停机信号」之后说还剩什么那句",
    );
    let st = wait_exit(&mut c);
    assert_eq!(st.code(), Some(0), "{st:?}；stderr：{err}");
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
    let (rig, mut c) = up_to_draining_with("deadline", Some(800), "drain");
    let err = said_through(&c, "还有 1 条没做完", "「排空期限到了」之后说还剩什么那句");
    let st = wait_exit(&mut c);
    assert_eq!(st.code(), Some(0), "{st:?}；stderr：{err}");
    assert!(
        err.contains("排空期限") && err.contains("terminal-preview（id=a）"),
        "到点了应当说出哪一条没做完：{err}"
    );
    assert!(!err.contains("都做完了"), "{err}");
    rig.release();
}

// ── D5 ────────────────────────────────────────────────────────────────────

/// 压几趟。
const D5_ROUNDS: usize = 100;
/// 同时跑几个子进程。
const D5_LANES: usize = 8;

/// 一趟：在飞那条在排空末尾做完 ⇒ 子进程退之前它的应答写到了 stdout。回：没到的话那一趟的 stderr。
fn d5_round(tag: &str) -> Result<(), String> {
    let (rig, mut c) = up_to_draining(tag);
    rig.release();
    let st = wait_exit(&mut c);
    // 子进程退了 ⇒ stdout 那根管子关上，收行的线程读完就断开：把它写出去的每一行都收齐。
    let mut got_a = false;
    loop {
        match c.out.recv_timeout(PATIENCE) {
            Ok(l) => got_a |= l.contains(r#""id":"a""#),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(e) => panic!("等子进程的 stdout 关上没等到（{e:?}）"),
        }
    }
    if got_a && st.code() == Some(0) {
        return Ok(());
    }
    Err(format!(
        "退出码 {st:?}，a 的应答{}；stderr：{:?}",
        if got_a { "到了" } else { "没到" },
        c.err_all.lock().unwrap()
    ))
}

/// **排空完成之后，最后一条应答写出去了才退**。
/// 守的：在飞的阻塞命令在排空末尾做完 ⇒ 它的最终应答经写者到了对端，进程才退 —— 不是「票落就退、应答来不来得及写看运气」。
/// 形状：同 D2 的真子进程台架（写者是生产那一个）；放开 gate ⇒ 子进程退 0，stdout 上收得到 `id=a` 那一帧。
/// 旧次序（票在阻塞闭包里先落、应答后送，排空一完就 `exit`）是个窗口，单趟多半撞不上 ⇒ 压 [`D5_ROUNDS`] 趟、趟趟要到。
#[test]
fn d5_the_last_reply_is_written_before_the_drained_exit() {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let lost: Vec<String> = std::thread::scope(|s| {
        let lanes: Vec<_> = (0..D5_LANES)
            .map(|_| {
                s.spawn(|| {
                    let mut lost = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if i >= D5_ROUNDS {
                            return lost;
                        }
                        if let Err(e) = d5_round(&format!("last{i}")) {
                            lost.push(format!("第 {i} 趟：{e}"));
                        }
                    }
                })
            })
            .collect();
        lanes
            .into_iter()
            .flat_map(|h| h.join().expect("压测线程"))
            .collect()
    });
    assert!(
        lost.is_empty(),
        "{D5_ROUNDS} 趟里有 {} 趟排空完就退、最后一条应答没写出去：\n{}",
        lost.len(),
        lost.join("\n")
    );
}

// ── D6 ────────────────────────────────────────────────────────────────────

/// **命令都做完了、写者没写完就到期限 ⇒ 照实说还有写者没写完、排着的应答没写出，然后退**（不说成「都写出去了」）。
/// 形状：同 D4 的台架，子进程里另登记一个一直不返回的写者（替卡在不读的对端上的那一个），期限 3 秒（够放开之后命令做完）；放开 gate ⇒
/// 命令做完、生产写者照收尾返回，那一个不返回 ⇒ 到点退 0，stderr 那一行说「还有 1 个写者没写完」。
#[test]
fn d6_the_deadline_says_replies_were_left_unwritten() {
    let (rig, mut c) = up_to_draining_with("unwritten", Some(3000), "stuck");
    rig.release();
    let err = said_through(&c, "排着的应答没写出", "「排空期限到了」之后说还剩什么那句");
    let st = wait_exit(&mut c);
    assert_eq!(st.code(), Some(0), "{st:?}；stderr：{err}");
    assert!(
        err.contains("还有 1 个写者没写完，排着的应答没写出"),
        "命令做完、写者没写完就到点，应当照实说应答没写出：{err}"
    );
    assert!(!err.contains("应答都写出去了"), "{err}");
}

// ── D8 ────────────────────────────────────────────────────────────────────

/// **对端关了写半边、读还开着 ⇒ 已收的命令的应答照样写到**（常驻那条连接的收场 `stream_ends`）。
/// 形状：同 D2 的台架，子进程跑 `stream_ends`（读写两半都是生产那一个）；发一条会卡住的阻塞命令 → 等假 tmux 起来 →
/// 关上子进程的 stdin（对端关写）→ 等它说读到 EOF → 放开 gate ⇒ stdout 上收得到 `id=a` 那一帧，流随后自己收场。
#[test]
fn d8_a_half_closed_stream_still_gets_its_replies() {
    let rig = Rig::new("halfclose");
    let mut c = start_child_with(&rig, None, "halfclose");
    send_line(&mut c, CAPTURE_A);
    rig.wait_started();
    drop(c.stdin.take());
    wait_line(
        &c.err,
        |l| l.contains("入方向读到 EOF"),
        "「入方向读到 EOF」那句",
    );
    rig.release();
    let got = wait_line(
        &c.out,
        |l| l.contains(r#""id":"a""#),
        "a 的应答（对端只关了写半边）",
    );
    assert!(got.contains(r#""id":"a""#), "{got}");
    let err = said_through(&c, HALFCLOSE_ENDED, "流收场那一句");
    let st = wait_exit(&mut c);
    assert_eq!(st.code(), Some(0), "{st:?}；stderr：{err}");
}

// ── D9 · D10 ──────────────────────────────────────────────────────────────
//
// 常驻那条连接读到 EOF 之后要等在飞命令都交了应答才收场（D8）⇒ 连接撤了之后才登记上的那一条不许留着、握着应答那一路：
// 不然这条流永远不收场，常驻的连接计数不归零。登记发生在处理器过了闸门之后（别的任务 · 阻塞线程里），撤可能先到。

/// 一条挂上等答的批准（替身家）。
fn d9_ask(sid: &str) -> crate::agents::AskIn {
    crate::agents::AskIn {
        sid: sid.into(),
        kind: crate::agents::AskKind::Approve,
        tool: "Bash".into(),
        what: Some("touch a".into()),
        detail: "touch a".into(),
        suggestions: vec![],
        questions: vec![],
        agent_id: None,
        agent_type: None,
        blocks_terminal: false,
        input: serde_json::json!({"command": "touch a"}),
    }
}

/// **连接撤了之后才挂上的请求：不挂，等答的那一头当场收到「撤了」**（`control::asks` 的登记表）。
#[test]
fn d9_an_ask_hung_after_its_connection_withdrew_is_not_kept() {
    let sid = "d9-late";
    let desk = crate::control::asks::Desk::new();
    let conn = desk.conn();
    drop(desk); // 读循环读到 EOF、先撤了
    let (_id, mut rx) = crate::control::asks::open_id(crate::agents::fake::ASK, d9_ask(sid), conn);
    assert!(
        crate::control::asks::pending(sid).is_empty(),
        "连接撤了之后挂上的那一条留在了登记表里"
    );
    assert_eq!(
        crate::control::asks::ended(sid),
        None,
        "界面从没见过的那一条，不该改这条会话「上一条请求怎么了」那一格"
    );
    assert!(
        matches!(
            rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed)
        ),
        "等答的那一头没当场收到「撤了」—— 处理器会一直握着应答那一路"
    );
}

/// **连接撤了之后才起的登录作业：不留，当场回「本次登录不存在」**（`accounts::sign_in` 的作业表）。
/// 替身家的登录程序一起来就退（不印链接）：撤销没认上时，作业照留、回的是「退了」那一句。
#[test]
fn d10_a_login_started_after_its_connection_withdrew_is_not_kept() {
    let face = crate::agents::fake::NO_PASTE_FACE;
    let root = std::env::temp_dir().join(format!(
        "ccm-d10-login-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(root.join("bin")).expect("台架目录");
    test_home::write_exe(&root.join("bin").join(face.program), "#!/bin/sh\nexit 0\n");
    let conn = crate::control::asks::Desk::new().conn();
    drop(crate::accounts::sign_in::Desk::of(conn)); // 读循环读到 EOF、先撤了
    let got = crate::accounts::sign_in::start(
        &crate::accounts::sign_in::SignInAsk {
            face,
            target: "d10-late",
            dir: None,
            then: &[],
            conn,
            cwd: Some(root.clone()),
            path: Some(format!("{}:/usr/bin:/bin", root.join("bin").display())),
        },
        Box::new(|_, _| {}),
    );
    let _ = std::fs::remove_dir_all(&root);
    let said = got.expect_err("连接撤了之后还起得来登录作业");
    assert_eq!(
        said.said,
        copy_core::copy_text("beLogin.finish.noJob", &[]),
        "连接撤了之后起的作业应当当场回「本次登录不存在」：{said:?}"
    );
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
    let inbound =
        guard_core::production_code(include_str!("../../../src/backend/stream/inbound/mod.rs"));
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

    // `process::exit(` 的所在函数那一格挪进 [`x15_the_process_exits_only_in_main_and_exit_after_drain`]（全树，不只进程入口）。
    let main = guard_core::production_code(include_str!("../../../src/backend/stream/entry.rs"));
    let mut drains: Vec<String> = call_sites(&main, "exit_after_drain")
        .into_iter()
        .map(|i| enclosing_fn(&main, i))
        .collect();
    drains.sort();
    assert_eq!(
        drains,
        vec![
            "run",             // 常驻载体收到停机信号
            "run_over_stdio",  // hello 写不出去
            "run_over_stdio",  // 收到停机信号（写者照常跑）
            "run_over_stdio",  // 对端走了（写不出去）
            "serve_listening", // 最后一个客户走了且退出行为是「结束」
        ],
        "流模式的收场口集合变了"
    );

    // 生产入口交给本体的期限恰是 `DRAIN_DEADLINE`（判据用短期限走的是同一个本体）。收场那几样住 `drain.rs`。
    let drain = guard_core::production_code(include_str!("../../../src/backend/wire/drain.rs"));
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

    // 常驻那条连接（D8 的台架自己放 sender，钉不到生产那一句）：起收场之前放掉自己那一份应答 sender ——
    //   放在收场之后，读到 EOF 时写者等不到「应答那一路没人再送」，这条流永远不收场。
    let ends: Vec<usize> = call_sites(&main, "stream_ends")
        .into_iter()
        .filter(|i| !main[..*i].ends_with("fn "))
        .collect();
    assert_eq!(ends.len(), 1, "`stream_ends(` 应恰好一处调用：{ends:?}");
    let before = main[..ends[0]].trim_end();
    assert!(
        before.ends_with("drop(reply_tx);"),
        "常驻那条连接进 `stream_ends` 之前没先放掉自己那一份应答 sender：{:?}",
        before.lines().last()
    );
}

/// **退出口只有进程入口 `run` 与 `exit_after_drain`**：后端生产树里 `process::exit(` 的所在函数，全树逐处现打。
///
/// 期望（异源：取自 ⑮ 那句裁决与进程入口的分派形状，不从被扫的源码现推）：
/// `stream/entry.rs::run` 三处（ccm 那一趟 · 一次性查询 · 监听口配置不成立或绑不上口，`claim_then_log` 交回的码）——
/// 都发生在**一条命令都还没收**之前、没有可排空的；`wire/drain.rs::exit_after_drain_within` 一处
/// （`exit_after_drain` 的本体：生产入口只经它，期限由 d3 钉）。别处一处都不许有 —— 模块里想退就把退出码交回调用方。
#[test]
fn x15_the_process_exits_only_in_main_and_exit_after_drain() {
    let root = guard_core::addr::backend_src_root();
    let mut found: Vec<(String, String)> = Vec::new();
    let mut scanned = 0usize;
    for (path, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let prod = guard_core::production_code(&src);
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
        ("stream/entry.rs", "run"),
        ("stream/entry.rs", "run"),
        ("stream/entry.rs", "run"),
        ("wire/drain.rs", "exit_after_drain_within"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    assert_eq!(
        found, want,
        "后端生产树里 `process::exit(` 的所在函数变了 —— 退出口只许是 `run` 与 `exit_after_drain`（⑮）；\
         模块里想退，把退出码交回 `run`"
    );
    // 正控：数法认得出多出来的一处 `exit(`，也认得出它住哪个函数。
    let planted = "fn planted() { std::process::exit(0); }\n";
    let at = call_sites(planted, "process::exit");
    assert_eq!(at.len(), 1);
    assert_eq!(enclosing_fn(planted, at[0]), "planted");
}

// ── A1（同住本文件：都是进程入口 `stream/entry.rs` 流模式那几行的接线）────────────────────

/// 〔NT2 问 3 ＋ RT1 F3〕后端 `tracing` 只在 stderr 是终端时上色。
/// 守的要求：「写进文件 / monitor 日志的 stderr 关 ANSI 颜色」。
/// 形状：`stream/entry.rs` 生产段里 `.with_ansi(` 恰好一处，参数恰是「stderr 是不是终端」（一次性模式另与上「不是一次性」：它写诊断文件）；`fmt()` 恰好一处（没有第二个不设它的初始化）。
#[test]
fn a1_the_backend_log_is_colored_only_on_a_terminal() {
    let main = guard_core::production_code(include_str!("../../../src/backend/stream/entry.rs"));
    let at: Vec<usize> = main.match_indices(".with_ansi(").map(|(i, _)| i).collect();
    assert_eq!(at.len(), 1, "`.with_ansi(` 不是恰好一处：{at:?}");
    let arg = &main[at[0] + ".with_ansi(".len()..];
    let arg = &arg[..arg.find('\n').unwrap_or(arg.len())];
    // 一次性模式写的是诊断文件（不是 stderr）⇒ 那一形恒不上色：`… && !oneshot)`。
    assert!(
        [
            "std::io::IsTerminal::is_terminal(&std::io::stderr()))",
            "std::io::IsTerminal::is_terminal(&std::io::stderr()) && !oneshot)"
        ]
        .iter()
        .any(|w| arg.starts_with(w)),
        "上不上色不再看 stderr 是不是终端（或一次性模式写文件也上色）：{arg}"
    );
    assert_eq!(
        main.matches("tracing_subscriber::fmt()").count(),
        1,
        "多了一个 tracing 初始化"
    );
}
