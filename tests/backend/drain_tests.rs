//! 〔HX1 · 4D〕**退出之前先排空停不下来的那一档** —— 判据。
//!
//! 守的要求（住址）：
//! - 主会话 4D 裁 D-a（`4d-lanes.md`「主会话本批裁的」）逐字：「后端收 SIGTERM 先排空在飞写（有上限）再退」；
//! - 审计 E §E2：「流模式后端 `exit(0)` 或被 SIGKILL 时，**不等正在跑的阻塞写做完**」；
//! - `设计/05 §3.3.2`：「**值**住后端 · **执行**住通信层」—— 本件的「上限」由叫它退的那一方执行（第二次停机信号 ⇒ 立刻退）。
//! 设计与读数住 `调研/第四波记录/HX1.md` §1。
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
    let a = d.enter().expect("没关闸时取得到票");
    let b = d.enter().expect("没关闸时取得到第二张");
    assert_eq!(d.in_flight(), 2);
    assert_eq!(d.close(), 2, "close 回的数应 == 关的那一刻在飞的票数");
    assert!(
        d.enter().is_none(),
        "关闸之后还取得到票 —— 收场期间会再起新的阻塞活"
    );
    let w = d.drained();
    tokio::pin!(w);
    assert!(
        !polled_ready(w.as_mut()),
        "还有 2 张票在飞，drained 就完成了"
    );
    drop(a);
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
const CHILD_TEST_NAME: &str = "inbound::drain_tests::d2_child_harness";
const READY_LINE: &str = "HX1-HARNESS-READY";

/// 子进程那一半：与 `main.rs::run_over_stdio` 收信号那一支同形 —— stdin 进 `inbound::spawn`、应答写 stdout，
/// 收到停机信号 ⇒ `exit_after_drain`（写者照常跑）。**跑的全是生产那几样**：取票的门 · 闸 · 收场函数 · 信号监听 ·
/// 一条真的阻塞命令（`capture-pane` 起 `tmux`，PATH 上那个是假的）。只有「把它们拼起来」这一层是台架（接线由 D3 钉）。
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
            crate::wire::HelloFlushed::for_tests(),
        );
        let stop = shutdown_listener();
        let mut out = tokio::io::stdout();
        out.write_all(format!("{READY_LINE}\n").as_bytes())
            .await
            .expect("写就绪行");
        out.flush().await.expect("flush");
        let writer = async move {
            while let Some(f) = rx.recv().await {
                let line = crate::wire::to_line(&f).expect("serialize");
                if out.write_all(line.as_bytes()).await.is_err() || out.flush().await.is_err() {
                    return;
                }
            }
        };
        tokio::pin!(writer);
        tokio::select! {
            _ = &mut writer => {}
            _ = stop => {
                exit_after_drain("收到停机信号", Some(writer)).await
            }
        }
    });
}

/// 一台假 `tmux`：被起来就先往 `ready` 那根 FIFO 说一声，再卡在 `gate` 那根 FIFO 上，直到父进程放开。
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
        let script = format!(
            "#!/bin/sh\necho started > '{}'\ncat '{}' > /dev/null\nexit 0\n",
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

const CAPTURE_A: &str = r#"{"id":"a","cmd":"capture-pane","args":{"name":"hx1-a"}}"#;
const CAPTURE_B: &str = r#"{"id":"b","cmd":"capture-pane","args":{"name":"hx1-b"}}"#;

/// 起子进程 → 发一条会卡住的阻塞命令 → 等假 tmux 真起来 → SIGTERM → 等到「收尾：还有 1 条」那句。
fn up_to_draining(tag: &str) -> (Rig, Child) {
    let rig = Rig::new(tag);
    let mut c = start_child(&rig);
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
        crate::guard_support::production_code(include_str!("../../src/backend/inbound.rs"));
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

    // `main.rs`：生产段零处 `exit(`（流模式的 exit 只住 `exit_after_drain`；一次性查询 / ccm / 监听口配置那几处例外逐条登记）。
    let main = crate::guard_support::production_code(include_str!("../../src/backend/main.rs"));
    let mut exits: Vec<String> = call_sites(&main, "process::exit")
        .into_iter()
        .map(|i| enclosing_fn(&main, i))
        .collect();
    exits.sort();
    // 登记：`main` 里三处（ccm 那一趟 · 一次性查询 · 监听口配置不成立）＋ `serve_listening` 绑不上口那一处。
    // 四处都发生在**一条命令都还没收**之前 ⇒ 没有可排空的。
    assert_eq!(
        exits,
        vec!["main", "main", "main", "serve_listening"],
        "`main.rs` 里 `process::exit(` 的所在函数集合变了 —— 流模式的退出口必须经 `inbound::exit_after_drain`"
    );
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

    // 正控：数法认得出多出来的一处 `exit(`。
    let planted = format!("{main}\nfn planted() {{ std::process::exit(0); }}\n");
    assert_eq!(call_sites(&planted, "process::exit").len(), 5);
    assert_eq!(
        enclosing_fn(
            &planted,
            *call_sites(&planted, "process::exit").last().unwrap()
        ),
        "planted"
    );
}
