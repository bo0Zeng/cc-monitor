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

    /// 同 [`Self::session`]，另回它的 pidfile（要一份一份地按次序宣告时用）。
    fn session_and_pidfile(&mut self, sid: &str) -> (PathBuf, PathBuf) {
        let rec = self.session(sid);
        let pid = self.kids.last().expect("刚起的那一个").id();
        (rec, self.sessions.join(format!("{pid}.json")))
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
    let (cap, load) = (pool.pool.size(), pool.pool.load());
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

/// ⑤ 先读正在看的（read8a ②）：两份都没预读完时报名单 [B] ⇒ B 先读完（按完成次序判）。
/// 池子先被卡住的几份占满，A、B 依宣告次序排在后面；报名单 [B] 之后空出一位 ⇒ 交出去的是 B，不是先排的 A。已在读的不撤。
#[test]
fn the_watched_session_is_primed_first() {
    let _iso = crate::control::identity_tag::door::isolate();
    let mut rig = Rig::new("watched");
    let (ev_tx, ev_rx) = std::sync::mpsc::channel::<WatchEvent>();
    let pool = PrimePool::start(ev_tx);
    let cap = pool.pool.size();
    let mut state = rig.state();
    state.pool = Some(pool);
    let (tx, _rx) = tokio::sync::mpsc::channel::<Frame>(256);
    let mut sink = FrameSink::new(tx);
    // 先把池子占满（各卡在读里）。
    let mut stuck = Vec::new();
    for i in 0..cap {
        let (rec, pidfile) = rig.session_and_pidfile(&format!("p-w-stuck-{i}"));
        let w = stuck_record(&rec, b"{\"cwd\":\"/w\"}\n");
        process_session_added(&pidfile, &mut state, &mut sink);
        stuck.push((rec, w));
    }
    // A 先宣告、B 后宣告：两份都排着。
    for sid in ["p-w-a", "p-w-b"] {
        let (rec, pidfile) = rig.session_and_pidfile(sid);
        std::fs::write(&rec, lines(0..4)).unwrap();
        process_session_added(&pidfile, &mut state, &mut sink);
    }
    assert_eq!(state.priming.len(), cap + 2, "都还没读完");
    // 这条连接说它在看 B。
    let _ = watch_sessions(0, vec!["p-w-b".to_string()], &mut state, &mut sink);
    // 放开一份卡住的 ⇒ 空出一位。
    let (rec, w) = stuck.remove(0);
    release(&rec, w);
    let mut order = Vec::new();
    let d = next_primed(&ev_rx);
    order.push(d.sid.clone());
    primed(d, &mut state, &mut sink);
    let d = next_primed(&ev_rx);
    order.push(d.sid.clone());
    primed(d, &mut state, &mut sink);
    // 其余放开、收齐。
    for (rec, w) in stuck {
        release(&rec, w);
    }
    while !state.priming.is_empty() {
        let d = next_primed(&ev_rx);
        order.push(d.sid.clone());
        primed(d, &mut state, &mut sink);
    }
    let at = |sid: &str| order.iter().position(|s| s == sid).unwrap();
    assert_eq!(order[1], "p-w-b", "空出的那一位交给了在看的 B：{order:?}");
    assert!(
        at("p-w-b") < at("p-w-a"),
        "B 要先于先排的 A 读完：{order:?}"
    );
}

/// 链上的几行（带一处回退：u2 · a2 是主线外的），后面接结构性的假行。
fn chained(n: std::ops::Range<usize>) -> String {
    let rows = [
        r#"{"type":"user","uuid":"u1","parentUuid":null,"timestamp":"2026-10-01T00:00:00.000Z","message":{"role":"user","content":"first ask"}}"#,
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-10-01T00:00:01.000Z","message":{"model":"m-x","stop_reason":"end_turn","content":[{"type":"text","text":"ok"}]}}"#,
        r#"{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2026-10-01T00:00:02.000Z","message":{"role":"user","content":"old wording"}}"#,
        r#"{"type":"assistant","uuid":"a2","parentUuid":"u2","timestamp":"2026-10-01T00:00:03.000Z","message":{"model":"m-x","stop_reason":"end_turn","content":[{"type":"text","text":"r2"}]}}"#,
        r#"{"type":"user","uuid":"u3","parentUuid":"a1","timestamp":"2026-10-01T00:00:04.000Z","message":{"role":"user","content":"new wording"}}"#,
    ];
    rows.join("\n") + "\n" + lines(n).as_str()
}

/// 一趟预读（夹具没有池 ⇒ 就地跑、就地装回）：回读了多少字节 · 读完那一帧的 `lines` · 主线外清单。
fn prime_once(
    rig: &Rig,
    home: &crate::observe::skeleton::Skeletons,
    rec: &Path,
) -> (u64, Vec<String>, Vec<String>) {
    let mut state = rig.state();
    state.skeletons = Some(home.clone());
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(256);
    let mut sink = FrameSink::new(tx);
    initial_session_scan(&rig.sessions, &mut state, &mut sink);
    let got: Vec<String> = brief(&drain(&mut rx))
        .into_iter()
        .filter(|s| s.starts_with("primed"))
        .collect();
    let off = state
        .branches
        .get(&path_key(rec))
        .map(|c| c.off().to_vec())
        .unwrap_or_default();
    (state.prime_bytes, got, off)
}

/// ⑥ 骨架落盘（数字节）：重启后（新的一份读者、同一个骨架的家）记录没变 ⇒ 预读读 0 字节、
/// 行数与主线外清单跟整份读的一样；追加 k 字节 ⇒ 读 k ＋ 见证；改写过（同长）⇒ 整份。
#[test]
fn a_restart_reads_only_what_the_skeleton_has_not_seen() {
    let _iso = crate::control::identity_tag::door::isolate();
    let mut rig = Rig::new("skel");
    let rec = rig.session("p-skel");
    let body = chained(0..40);
    std::fs::write(&rec, &body).unwrap();
    let (d, own) = crate::observe::skeleton::tests::sandbox("prime-skel");
    let home = crate::observe::skeleton::Skeletons::at(own, 0);

    let (bytes, got, off) = prime_once(&rig, &home, &rec);
    assert_eq!(bytes, body.len() as u64, "头一次读整份");
    assert_eq!(got, vec!["primed p-skel Some(45)".to_string()]);
    assert_eq!(off, ["u2", "a2"], "夹具的回退没认出来");

    let (bytes, got2, off2) = prime_once(&rig, &home, &rec);
    assert_eq!(bytes, 0, "重启后没变，还是读了记录");
    assert_eq!(
        (got2, off2),
        (got, off.clone()),
        "照骨架装回的跟整份读的不一样"
    );

    let more = lines(40..43);
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&rec).unwrap();
        f.write_all(more.as_bytes()).unwrap();
    }
    let (bytes, got3, off3) = prime_once(&rig, &home, &rec);
    assert_eq!(
        bytes,
        more.len() as u64 + TAIL_PROBE,
        "追加 k 字节应只读 k ＋ 见证"
    );
    assert_eq!(got3, vec!["primed p-skel Some(48)".to_string()]);
    assert_eq!(off3, off, "接着骨架读，链索引丢了");

    // 改写（同长、内容变了、修改时刻变了）⇒ 整份。
    let now = std::fs::read_to_string(&rec).unwrap();
    let flipped = now.replace("old wording", "odd wording");
    assert_eq!(flipped.len(), now.len());
    std::fs::write(&rec, &flipped).unwrap();
    let f = std::fs::OpenOptions::new().write(true).open(&rec).unwrap();
    f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
        .unwrap();
    drop(f);
    let (bytes, _, _) = prime_once(&rig, &home, &rec);
    assert_eq!(bytes, now.len() as u64, "改写过还照骨架读了");
    let _ = std::fs::remove_dir_all(&d);
}
