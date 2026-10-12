//! 先宣告、后读（read8a：宣告不等记录 ＋ 整台一份读者）：会话一认出来就报，记录在后台线程池里预读；
//! 读完之前它一帧行都不出，读完先交「从第几行起」再接着出；整台一份读者，几条连接接上来都不重读。
//! 判据只数格数与先后，不按墙钟。卡住的记录源是一条命名管道（写端开着、不写 ⇒ 预读那一趟停在读里）。
//! 语料是结构性的假行（`{"n":…}`），不含任何真会话正文。

use super::*;

fn sleeper() -> std::process::Child {
    crate::control::identity_tag::tests::spawn_settled_sleep(|c| {
        c.env_remove("TMUX_PANE").env_remove("TMUX");
    })
}

struct Rig {
    dir: PathBuf,
    sessions: PathBuf,
    proj: PathBuf,
    kids: Vec<std::process::Child>,
}

impl Rig {
    fn new(tag: &str) -> Rig {
        let dir = std::env::temp_dir().join(format!("ccm-prime-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sessions = dir.join("sessions");
        let proj = dir.join("projects").join("-w");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::create_dir_all(&proj).unwrap();
        Rig {
            dir,
            sessions,
            proj,
            kids: Vec::new(),
        }
    }

    /// 一个活会话（一个 `sleep` 进程当它）：写 pidfile，回它的记录路径（记录由调用方造）。
    fn session(&mut self, sid: &str) -> PathBuf {
        let kid = sleeper();
        let pid = kid.id();
        let ticks = proc_starttime(pid).expect("子进程的 starttime 读不到 —— 夹具坏了");
        std::fs::write(
            self.sessions.join(format!("{pid}.json")),
            format!(r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"/w","kind":"interactive","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        self.kids.push(kid);
        self.proj.join(format!("{sid}.jsonl"))
    }

    fn state(&self) -> ReaderState {
        ReaderState::new(self.dir.join("projects"))
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        for k in &mut self.kids {
            let _ = k.kill();
            let _ = k.wait();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 一条命名管道当记录：写端由夹具开着（读的那一头读完已有的就停在读里，直到写端关掉）。
fn stuck_record(path: &Path, head: &[u8]) -> std::fs::File {
    use std::io::Write;
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: 路径是本夹具造的、以 NUL 结尾的串。
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0, "mkfifo");
    // 读写打开不会阻塞（Linux 上命名管道的 O_RDWR）；它留着 ⇒ 读那一头永远等得到写端。
    let mut w = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    w.write_all(head).unwrap();
    w
}

/// 放开一条卡住的记录：先把同名换成一份普通文件（预读之后取名字那一步开的是它，不会再停在管道上），再关写端（读的那一头读到尽头）。
fn release(path: &Path, w: std::fs::File) {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, b"{\"cwd\":\"/w\"}\n").unwrap();
    std::fs::rename(&tmp, path).unwrap();
    drop(w);
}

fn lines(n: std::ops::Range<usize>) -> String {
    n.map(|i| format!("{{\"n\":{i}}}\n")).collect()
}

fn drain(rx: &mut tokio::sync::mpsc::Receiver<Frame>) -> Vec<Frame> {
    let mut v = Vec::new();
    while let Ok(f) = rx.try_recv() {
        v.push(f);
    }
    v
}

fn brief(frames: &[Frame]) -> Vec<String> {
    frames
        .iter()
        .map(|f| match f {
            Frame::SessionAdded { sid, .. } => format!("added {sid}"),
            Frame::SessionPrimed { sid, lines, .. } => format!("primed {sid} {lines:?}"),
            Frame::Line {
                session_id, seq, ..
            } => format!("line {session_id} {seq}"),
            other => other.loss_identity().kind.to_string(),
        })
        .collect()
}

/// 等池子交回下一趟预读（交回走的就是 watcher 的事件通道）。
fn next_primed(ev: &std::sync::mpsc::Receiver<WatchEvent>) -> PrimeDone {
    loop {
        match ev
            .recv_timeout(guard_core::wall::CEILING)
            .expect("预读一直没交回")
        {
            WatchEvent::Primed(d) => return d,
            _ => continue,
        }
    }
}

/// ① 一份预读卡住时：别的会话照样宣告、照样读完；初扫照样回来（之后才起第一份 tmux 观测）；
/// 「清单报完了」照样出（第一份 tmux 观测一到就放）。卡住那一条读完之前它的读完那一帧不出。
#[test]
fn a_stuck_prime_holds_back_nothing_but_its_own_session() {
    let _iso = crate::control::identity_tag::door::isolate();
    let mut rig = Rig::new("stuck");
    let stuck = rig.session("p-stuck");
    let w = stuck_record(&stuck, b"{\"cwd\":\"/w\"}\n");
    let fine = rig.session("p-fine");
    std::fs::write(&fine, lines(0..3)).unwrap();
    let (ev_tx, ev_rx) = std::sync::mpsc::channel::<WatchEvent>();
    let mut state = rig.state();
    state.pool = Some(PrimePool::start(ev_tx));
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::hub();
    sink.subs.push(Sub::new(0, tx, crate::Tz::default()));

    // 初扫回来了（卡住的那一趟在池子里，不挡本线程）⇒ watch_loop 接着起第一份 tmux 观测。
    initial_session_scan(&rig.sessions, &mut state, &mut sink);
    sink.observing();
    sink.tmux(TmuxObservation::NoServer);
    let mut got = brief(&drain(&mut rx));
    got.sort();
    assert_eq!(
        got,
        vec!["added p-fine", "added p-stuck", "sessions_replayed"],
        "一份预读卡住，别的会话的宣告 · 清单报完了被它挡住了"
    );
    // 没卡的那一条照样读完。
    let d = next_primed(&ev_rx);
    assert_eq!(d.sid, "p-fine", "先交回的应是没卡的那一条");
    primed(d, &mut state, &mut sink);
    assert_eq!(brief(&drain(&mut rx)), vec!["primed p-fine Some(3)"]);
    // 卡住那一条：写端一关就读完（读完那一帧这时才出）。
    release(&stuck, w);
    let d = next_primed(&ev_rx);
    assert_eq!(d.sid, "p-stuck");
    primed(d, &mut state, &mut sink);
    assert!(
        brief(&drain(&mut rx))
            .first()
            .is_some_and(|f| f.starts_with("primed p-stuck")),
        "卡住那一条读完之后才出它的读完那一帧"
    );
}

/// ② 同一条会话：读完之前一行 `line` 都不出（文件事件照来、照不读）；读完先交读完那一帧，
/// 之后第一帧 `line` 的 `seq` ＝ 那一帧的 `lines`，往后连续。
#[test]
fn nothing_streams_before_the_prime_lands_and_then_it_streams_on_from_its_count() {
    let _iso = crate::control::identity_tag::door::isolate();
    let mut rig = Rig::new("order");
    let rec = rig.session("p-order");
    std::fs::write(&rec, lines(0..5)).unwrap();
    let (ev_tx, ev_rx) = std::sync::mpsc::channel::<WatchEvent>();
    let mut state = rig.state();
    state.pool = Some(PrimePool::start(ev_tx));
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
    let mut sink = FrameSink::new(tx);
    initial_session_scan(&rig.sessions, &mut state, &mut sink);
    // 预读读完了、还没交回本线程：这时记录又长了三行，文件事件来了。
    let d = next_primed(&ev_rx);
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&rec).unwrap();
        f.write_all(lines(5..8).as_bytes()).unwrap();
    }
    process_jsonl(&rec, &mut state, &mut sink);
    assert_eq!(
        brief(&drain(&mut rx)),
        vec!["added p-order", "sessions_replayed"],
        "预读没交回之前出了行"
    );
    primed(d, &mut state, &mut sink);
    assert_eq!(
        brief(&drain(&mut rx)),
        vec![
            "primed p-order Some(5)",
            "line p-order 5",
            "line p-order 6",
            "line p-order 7"
        ],
        "读完那一帧要排在第一行之前，第一行的号 ＝ 它的 lines，往后连续"
    );
}

/// ③ 同时在预读的份数不超过池子的线程数（数在跑的任务）：比线程数多两份卡住的记录，在跑的恰好到上限、多出来的排着。
#[test]
fn no_more_primes_run_at_once_than_the_pool_has_threads() {
    let _iso = crate::control::identity_tag::door::isolate();
    let mut rig = Rig::new("cap");
    let (ev_tx, ev_rx) = std::sync::mpsc::channel::<WatchEvent>();
    let pool = PrimePool::start(ev_tx);
    let (cap, load) = (pool.0.size(), pool.0.load());
    let mut ends = Vec::new();
    for i in 0..cap + 2 {
        let p = rig.session(&format!("p-cap-{i}"));
        let w = stuck_record(&p, b"{\"cwd\":\"/w\"}\n");
        ends.push((p, w));
    }
    let mut state = rig.state();
    state.pool = Some(pool);
    let (tx, _rx) = tokio::sync::mpsc::channel::<Frame>(256);
    let mut sink = FrameSink::new(tx);
    initial_session_scan(&rig.sessions, &mut state, &mut sink);
    // 等池子把能取的都取走（只等「在跑的到了上限」这件事，判的是份数）。
    for _ in guard_core::wall::rounds() {
        if load.running() == cap {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        load.running(),
        cap,
        "卡住的记录比线程多，在跑的应恰好到上限"
    );
    for (p, w) in ends {
        release(&p, w);
    }
    for _ in 0..cap + 2 {
        primed(next_primed(&ev_rx), &mut state, &mut sink);
    }
    assert!(
        load.high() <= cap,
        "同时在预读的到过 {} 份，超过线程数 {cap}",
        load.high()
    );
    assert!(state.priming.is_empty(), "每一份都交回了");
}

/// ④ 整台一份读者：同一份记录，桌面 · 手机 · 重连三条连接接上来，整份预读只发生一次（数读了多少字节）；
/// 每条连接拿到的是同一个续点（宣告 ＋ 读完那一帧，`lines` ＝ 此刻已计完的行数）。
#[test]
fn three_connections_on_one_record_prime_it_once() {
    let _iso = crate::control::identity_tag::door::isolate();
    let mut rig = Rig::new("once");
    let rec = rig.session("p-once");
    let body = lines(0..40);
    std::fs::write(&rec, &body).unwrap();
    let mut state = rig.state();
    let mut sink = FrameSink::hub();
    initial_session_scan(&rig.sessions, &mut state, &mut sink);
    assert_eq!(state.prime_bytes, body.len() as u64, "宣告那一趟读一整份");
    let mut seen = Vec::new();
    for id in 1..=3 {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        join(
            Sub::new(id, tx, crate::Tz::default()),
            &mut state,
            &mut sink,
        );
        let got = brief(&drain(&mut rx));
        if id == 2 {
            sink.leave(2); // 手机断了，重连的那条是第三条
        }
        seen.push(got);
    }
    assert_eq!(
        state.prime_bytes,
        body.len() as u64,
        "多一条连接就多读一遍（每连接一份读者那一形回来了）"
    );
    for got in &seen {
        assert_eq!(
            got,
            &vec![
                "added p-once".to_string(),
                "primed p-once Some(40)".to_string()
            ]
        );
    }
}
