//! 跟读那一层的判据夹具：本包的判据与主 crate 挂的那几条（真打标 · 帧面宿主）经 `test-support` 共用。
//! 上层够不到读者的内部（读者状态 · 发帧口），只经这里的 [`Reader`] · [`LiveEar`] 驱动它。

use super::*;

/// 判据不等地取一帧（取走就把它的字节还回去，同 `recv`）：生产只 `recv`，这一个只给判据。
impl FrameRx {
    pub fn try_recv(&mut self) -> Result<Frame, tokio::sync::mpsc::error::TryRecvError> {
        let (frame, n) = self.rx.try_recv()?;
        self.held.fetch_sub(n, std::sync::atomic::Ordering::AcqRel);
        Ok(frame)
    }
}

/// 夹具那一份发帧口：一条连接（号 0）、不带会话账本（生产那一份是 [`FrameSink::hub`]，连接经 `join` 接上来）。
impl FrameSink {
    pub(in crate::observe::watcher) fn new(tx: FrameTx) -> Self {
        FrameSink {
            subs: vec![Sub::new(0, tx, crate::common::time::Tz::default())],
            ledger: None,
            priming: HashSet::new(),
            arrivals: crate::observe::arrivals::Arrivals::default(),
            list: std::sync::Arc::default(),
            faces: std::sync::Arc::default(),
        }
    }
}

/// 一帧的帧种（线上 `kind` 那一格）。
pub fn kind_of(f: &Frame) -> String {
    serde_json::to_value(f).unwrap()["kind"]
        .as_str()
        .unwrap()
        .to_string()
}

/// 写一份活进程的 pidfile（真 pid ＋ 真 procStart），不带状态那一格。
pub fn pidfile_of(sessions: &Path, pid: u32, sid: &str) -> PathBuf {
    let ticks = proc_starttime(pid).expect("子进程的 starttime 读不到 —— 夹具坏了");
    let at = sessions.join(format!("{pid}.json"));
    std::fs::write(
        &at,
        format!(
            r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#
        ),
    )
    .unwrap();
    at
}

/// 写一份活进程的 pidfile，带状态那一格（`status`）。
pub fn write_pidfile(sessions: &Path, pid: u32, sid: &str, status: &str) -> PathBuf {
    std::fs::create_dir_all(sessions).unwrap();
    let ticks = proc_starttime(pid).expect("子进程的 starttime 读不到");
    let p = sessions.join(format!("{pid}.json"));
    std::fs::write(
        &p,
        format!(
            r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"/x","kind":"interactive","procStart":"{ticks}","status":"{status}"}}"#
        ),
    )
    .unwrap();
    p
}

/// 起一个住在 `pane` 里的「claude」（环境读得到之后才交回），并在 `sessions` 下写它的 pidfile（`sid` · `status`）。
pub fn claude_in_pane(sessions: &Path, pane: &str, sid: &str, status: &str) -> std::process::Child {
    let kid = crate::platform::proc::test_support::spawn_settled_sleep(|c| {
        c.env("TMUX_PANE", pane);
    });
    write_pidfile(sessions, kid.id(), sid, status);
    kid
}

/// 一份读者（读者状态 ＋ 一条连接的发帧口 ＋ 那条连接的收端），打标那一手由调用方给（上层判据接控制面真打标的那一手）。
pub struct Reader {
    state: ReaderState,
    sink: FrameSink,
    rx: FrameRx,
}

impl Reader {
    /// 跟 `projects` 那棵记录树；打标那一手是 `tag`。
    pub fn new(projects: PathBuf, tag: Tag) -> Self {
        let (tx, rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
        let mut state = ReaderState::new(projects);
        state.tag = tag;
        Reader {
            state,
            sink: FrameSink::new(tx),
            rx,
        }
    }

    /// 一份 pidfile 出现 / 改写（同 pidfile 那一路的事件）：回这次打标真写了没有。
    pub fn session_added(&mut self, pidfile: &Path) -> bool {
        process_session_added(pidfile, &mut self.state, &mut self.sink)
    }

    /// 起步那一趟初扫（`sessions` 底下每份 pidfile）。
    pub fn initial_scan(&mut self, sessions: &Path) {
        initial_session_scan(sessions, &mut self.state, &mut self.sink);
    }

    /// 对在跟的会话整批（`only` 给了 ⇒ 只那一条）重打标：回真写了几处。
    pub fn retag(&self, only: Option<&str>) -> usize {
        retag_tracked(&self.state, only)
    }

    /// 拿盘上现实对这份读者的表（`only` 给了 ⇒ 只那一条），只对差异发帧。
    pub fn reconcile(&mut self, sessions: &Path, only: Option<&str>) -> Reconciled {
        reconcile_sessions(sessions, &mut self.state, &mut self.sink, only)
    }

    /// 到此刻为止发到那条连接上的帧，按先后取走。
    pub fn frames(&mut self) -> Vec<Frame> {
        let mut out = Vec::new();
        while let Ok(f) = self.rx.try_recv() {
            out.push(f);
        }
        out
    }
}

/// 一份「在跑的 watcher」只剩耳朵：进这台在跑的名单、装上「记录在不在」交会话账本那一口（同起 watcher 那一处），
/// 收这台交给每份 watcher 的事。放下就出名单。
pub struct LiveEar {
    id: u64,
    rx: std::sync::mpsc::Receiver<WatchEvent>,
}

impl LiveEar {
    pub fn enter() -> Self {
        crate::observe::record_hook::install(record_seen);
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        LiveEar {
            id: live_enter(tx),
            rx,
        }
    }

    /// 到此刻为止交到这份耳朵的「这条会话的记录在不在」，按到达先后。
    pub fn records_seen(&self, sid: &str) -> Vec<bool> {
        self.rx
            .try_iter()
            .filter_map(|ev| match ev {
                WatchEvent::RecordSeen { sid: s, present } if s == sid => Some(present),
                _ => None,
            })
            .collect()
    }
}

impl Drop for LiveEar {
    fn drop(&mut self) {
        live_leave(self.id);
    }
}
