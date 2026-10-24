//! **一台的常驻流**在手机这一侧的宿主：循环本身是 `resident-client` 那一份（[`Feed`]，桌面同一条循环），
//! 这里只接两根线 —— 下面那条 SSH（[`PhoneLink`]：拨号核心在这台手机上开的链路，拨号请求由 Kotlin 交那一份）· 往 Kotlin 交的事（一个回调，走消息）。
//!
//! 往 Kotlin 交的事一律 `(编号, 种类, 一段文本)`：`state`（这台通不通 ＋ 那台的状态成品，JSON：[`Facts`]；SSH 那层跟着它换连接）· `frame`（流上的一帧，原样一行；`line` 帧里解不开的那条记录换成认不出那一条）·
//! `watch_from`（JSON 数组）· `ssh`（SSH 够没够着那一台：`true` / `false`，与流通不通分开；SSH 那层跟它走）· `reply`（一问一答的结局，JSON，带 `call` 编号）·
//! `moved`（分块传走到哪，`transfer.rs`）。

use cmd_wire::record::OrUnread;
use deploy::machine::{fold, product, Entry, Fact};
use resident_client::client::CallError;
use resident_client::feed::{
    sink, Boxed, Config, Event, Exec, Feed, Problem, Progress, State, Transport,
};
use resident_client::watch::Desk;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncWriteExt, DuplexStream, Join};

/// 交给 Kotlin 的那一个口：`(这台的编号, 种类, 文本)`。
pub type OnEvent = Arc<dyn Fn(i64, &str, String) + Send + Sync>;

/// 握手每一行等多久（冷启要起常驻、读盘，给足）。
const HANDSHAKE: Duration = Duration::from_secs(20);

/// 一次性 exec 收全的上限（`--resident-ensure` 只回一行 JSON；上限只防对端吐无穷字节）。
const CAPTURE_MAX: u64 = 1 << 20;

/// 下面那条 SSH：拨号核心在这台手机上开的链路。`base` 是 Kotlin 那一侧连上时组好的拨号请求（主机 · 钥匙编号 · 跳板 · 记过的钥匙），
/// 每一趟在它上面填用法与命令，复用池里那条连接。
pub struct PhoneLink {
    base: serde_json::Value,
}

impl PhoneLink {
    pub fn new(base: serde_json::Value) -> Self {
        PhoneLink { base }
    }

    /// 在 `base` 上填这一趟的用法与命令，组成拨号请求（读不动 ⇒ 这条连接用不了）。
    fn request(&self, uses: serde_json::Value) -> Result<ssh_dial::DialRequest, Problem> {
        let mut req = self.base.clone();
        if let (Some(r), Some(u)) = (req.as_object_mut(), uses.as_object()) {
            for (k, v) in u {
                r.insert(k.clone(), v.clone());
            }
            r.insert("stages".into(), false.into());
        }
        serde_json::from_value(req).map_err(|e| {
            Problem::new(copy_core::copy_text("reason.link.lost", &[]), e.to_string())
        })
    }
}

impl Transport for PhoneLink {
    type Stream = Join<DuplexStream, DuplexStream>;

    /// 开一条长流：拨号核心报的进展原样交给 `progress`；开成之后那条通道的字节经一对内存管子对拷（随两头的读写端收工）。
    /// 开的那一段被丢下（循环到拨号期限放弃 · 宿主叫停）⇒ 这一拨随之收掉，不留一拨挂在 SYN 上占这个身份的拨号锁。
    fn stream(
        &self,
        command: String,
        progress: Progress,
    ) -> Boxed<'_, Result<Self::Stream, Problem>> {
        Box::pin(async move {
            let req = self.request(serde_json::json!({ "use": "stream", "command": command }))?;
            let stages = ssh_dial::StageSink::new(false);
            let opened = ssh_dial::uses::open(&req, &stages, |_| progress.step())
                .await
                .map_err(problem_of)?;
            let (up_w, up_r) = tokio::io::duplex(256 * 1024);
            let (mut down_w, down_r) = tokio::io::duplex(256 * 1024);
            tokio::spawn(async move {
                opened
                    .pump(up_r, &mut down_w, ssh_dial::uses::Control::none())
                    .await;
                let _ = down_w.shutdown().await;
            });
            Ok(tokio::io::join(down_r, up_w))
        })
    }

    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>> {
        Box::pin(async move {
            let req = self.request(serde_json::json!({
                "use": "capture",
                "command": command,
                "capture": { "max_bytes": CAPTURE_MAX },
            }))?;
            let got = ssh_dial::uses::capture(&req).await.map_err(problem_of)?;
            Ok(Exec {
                exit_status: got.exit_status,
                stdout: got.stdout,
                stderr: got.stderr,
            })
        })
    }
}

/// 拨号核心没开成的那一份 ⇒ 那一句（空 ⇒ 链路断了那一句）＋ 复制详情 ＋ 原因码与那台出示的 · 库里记下的指纹。
fn problem_of(f: ssh_dial::DialFail) -> Problem {
    Problem::new(
        if f.error.is_empty() {
            copy_core::copy_text("reason.link.lost", &[])
        } else {
            f.error
        },
        f.detail.unwrap_or_default(),
    )
    .because(f.reason, (f.fingerprint, f.expected_fingerprint))
}

// ═══ 表与口 ═════════════════════════════════════════════════════════════════════

fn feeds() -> &'static Mutex<HashMap<i64, Arc<Feed<PhoneLink>>>> {
    static F: OnceLock<Mutex<HashMap<i64, Arc<Feed<PhoneLink>>>>> = OnceLock::new();
    F.get_or_init(Default::default)
}

/// 每台此刻 SSH 那层的拨号请求（Kotlin 交的那一份；装后端那一趟在同一条池连接上开它自己的几条用法）。
fn bases() -> &'static Mutex<HashMap<i64, serde_json::Value>> {
    static B: OnceLock<Mutex<HashMap<i64, serde_json::Value>>> = OnceLock::new();
    B.get_or_init(Default::default)
}

/// 每台最近一次 hello 说做不到的那几条命令（`hello.unavailable`；读法同通信层那一份 `Offer::facts_of`）。
fn unavailables() -> &'static Mutex<HashMap<i64, Vec<String>>> {
    static U: OnceLock<Mutex<HashMap<i64, Vec<String>>>> = OnceLock::new();
    U.get_or_init(Default::default)
}

/// 此刻那一台的样子（文件页「能做什么」那一张要它）：断没断着 · 那台说做不到的那几条命令。这台不在 ⇒ 断着。
pub fn here(h: i64) -> pick_core::Here {
    pick_core::Here {
        offline: (!feed(h).is_some_and(|f| matches!(f.state(), State::Up { .. })))
            .then_some(pick_core::Cut::Machine),
        unavailable: unavailables()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&h)
            .cloned()
            .unwrap_or_default(),
    }
}

/// 这台此刻的拨号请求（SSH 那层没通 ⇒ `None`）。
pub(crate) fn base(h: i64) -> Option<serde_json::Value> {
    bases()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&h)
        .cloned()
}

pub(crate) fn feed(h: i64) -> Option<Arc<Feed<PhoneLink>>> {
    feeds()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&h)
        .cloned()
}

/// 一件事 ⇒ `(种类, 文本)`。帧按 `cmd-wire` 那一份类型解过、再按类型写一行（`line` 帧里解不开的那条记录成认不出那一条：[`OrUnread`]）；
/// 解不开 ⇒ `unread` 原样那一行；状态那一件经 [`Facts::round`]（要这台手上的事实）。Kotlin 用不着的（接上那一刻的 hello 与客户端：问答经 [`call`] 走 · 超长丢行的缺口）⇒ `None`。
pub fn event_of(e: &Event) -> Option<(&'static str, String)> {
    Some(match e {
        Event::Frame(line) => match cmd_wire::frame::decode::<OrUnread>(line)
            .and_then(|f| serde_json::to_string(&f))
        {
            Ok(typed) => ("frame", typed),
            Err(_) => ("unread", line.clone()),
        },
        Event::Ssh(reached) => ("ssh", reached.to_string()),
        Event::State(_) | Event::Attached { .. } | Event::Gap => return None,
    })
}

/// 一台手上的事实：成品收的那一份（[`Entry`]）＋ 那条流最新那一轮（部署每走一步重交 `state` 那一行时，通没通照它说）。
#[derive(Default)]
pub struct Facts {
    entry: Entry,
    last: Option<State>,
}

impl Facts {
    /// 那条流交来一轮 ⇒ `state` 那一行。
    pub fn round(&mut self, s: State, machine: &str) -> String {
        self.fold_round(&s);
        self.last = Some(s);
        self.line(machine)
    }

    /// 部署走到一步 ⇒ `state` 那一行（成品照那一步说：正在装 / 更新那一句与进度都在成品里）。
    pub fn step(&mut self, s: deploy::Step, machine: &str) -> String {
        fold(&mut self.entry, Fact::Step(s));
        self.line(machine)
    }

    /// 部署收了（成没成都算）⇒ 成品回到那条流最新那一轮；换了常驻的，接下来那几轮照常交。
    pub fn settle(&mut self, machine: &str) -> String {
        let last = self.last.clone().unwrap_or(State::Connecting);
        self.fold_round(&last);
        self.line(machine)
    }

    fn fold_round(&mut self, s: &State) {
        fold(
            &mut self.entry,
            Fact::Round {
                round: s.round(),
                version: None,
            },
        );
    }

    /// `state` 那一行：`state` 通没通（`connecting` · `up` 带第几条流 `epoch` · `down`）＋ `machine` 那台的状态成品
    /// （由 `deploy::machine` 定，同桌面那一份；这一端不自己装后端 ⇒ 没有后端那一轮给［安装］）。
    fn line(&self, machine: &str) -> String {
        let mut line = match &self.last {
            None | Some(State::Connecting) => serde_json::json!({ "state": "connecting" }),
            Some(State::Up { epoch, .. }) => serde_json::json!({ "state": "up", "epoch": epoch }),
            Some(State::Down { .. }) => serde_json::json!({ "state": "down" }),
        };
        line["machine"] =
            serde_json::to_value(product(&self.entry, machine, true, false)).unwrap_or_default();
        line.to_string()
    }
}

/// 每台手上的事实（那条流的每一轮 · 部署的每一步都收进同一份）。
fn facts() -> &'static Mutex<HashMap<i64, Arc<Mutex<Facts>>>> {
    static F: OnceLock<Mutex<HashMap<i64, Arc<Mutex<Facts>>>>> = OnceLock::new();
    F.get_or_init(Default::default)
}

/// 在编号 `h` 那台手上的事实里收一件、出那一行（那台不在 ⇒ `None`）。
pub(crate) fn note(h: i64, f: impl FnOnce(&mut Facts) -> String) -> Option<String> {
    let got = facts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&h)
        .cloned()?;
    let mut got = got.lock().unwrap_or_else(|e| e.into_inner());
    Some(f(&mut got))
}

/// 看一台（在当前运行时上起它的循环）。`mine` = 手上这一版的身份（版本门的对照物）。回编号。
pub fn open(machine: String, mine: Option<String>, tz: Option<String>, on_event: OnEvent) -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let name = machine.clone();
    let cfg = Config {
        machine,
        mine,
        tz,
        handshake: HANDSHAKE,
    };
    // 名单从空开始：一条会话都还没人看 ⇒ 那台一条都不发（屏上开了哪条再经 [`watch`] 报）。手机不读主线外清单 ⇒ 一份都不要。
    let rows_to = Arc::clone(&on_event);
    let desk = Desk::new(
        |_| Vec::new(),
        move |rows| {
            rows_to(
                id,
                "watch_from",
                serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into()),
            );
        },
    );
    let mine = Arc::new(Mutex::new(Facts::default()));
    facts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, Arc::clone(&mine));
    let feed = Feed::start(
        cfg,
        None,
        Arc::clone(&desk),
        sink(move |e: Event| {
            // 一组的进度按票在核心里接（`batch.rs`），不进 Kotlin 的帧流。
            if matches!(&e, Event::Frame(line) if crate::batch::took(line)) {
                return;
            }
            if let Event::Attached { hello, .. } = &e {
                // 这台说做不到的那几条（hello 按帧类型解，同通信层收 `Offer` 那一处）；解不开 ⇒ 空（没把握）。
                let ops = resident_client::attach::Hello::parse(hello)
                    .and_then(|h| h.frame)
                    .map(|h| comms_inward::chan::wire::Offer::of(&h).unavailable_ops())
                    .unwrap_or_default();
                unavailables()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id, ops);
            }
            let got = match &e {
                Event::State(s) => {
                    let mut facts = mine.lock().unwrap_or_else(|e| e.into_inner());
                    Some(("state", facts.round(s.clone(), &name)))
                }
                _ => event_of(&e),
            };
            if let Some((kind, body)) = got {
                on_event(id, kind, body);
            }
        }),
    );
    feeds()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, feed);
    watching()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, (desk, Watched::default()));
    id
}

/// SSH 那层此刻的连接：交拨号请求（JSON）⇒ 在它上面接；`None` ⇒ 没通。请求读不动 ⇒ `false`。
pub fn ssh(h: i64, base: Option<&str>) -> bool {
    let Some(f) = feed(h) else { return false };
    match base {
        None => {
            bases().lock().unwrap_or_else(|e| e.into_inner()).remove(&h);
            f.set_transport(None);
            true
        }
        Some(b) => match serde_json::from_str::<serde_json::Value>(b) {
            Ok(v) if v.is_object() => {
                bases()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(h, v.clone());
                f.set_transport(Some(Arc::new(PhoneLink::new(v))));
                true
            }
            _ => false,
        },
    }
}

/// 下一个问答编号（一问一答与分块传同一族：结局都经 `reply` 回、Kotlin 按它认）。
pub(crate) fn next_call() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// 一问一答：回问答编号；结局经 `reply` 那一种交回（`{call, …}`，[`reply_json`]）。这台不在 ⇒ 0。
pub fn call(h: i64, cmd: String, args: &str, within_ms: u64, on_event: OnEvent) -> i64 {
    let Some(f) = feed(h) else { return 0 };
    let id = next_call();
    let args: serde_json::Value = serde_json::from_str(args).unwrap_or(serde_json::Value::Null);
    let machine = f.machine().to_string();
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(within_ms.max(1));
        let got = match f.client_by(deadline).await {
            Some(c) => c.call_until(&cmd, args, deadline).await,
            None => Err(CallError::Disconnected),
        };
        on_event(h, "reply", reply_json(id, &machine, &cmd, got));
    });
    id
}

/// 多一处 / 少一处在看这条会话。名单变了 ⇒ 交那台的名单台（整份）。
pub fn watch(h: i64, sid: &str, holding: bool) {
    let mut t = watching().lock().unwrap_or_else(|e| e.into_inner());
    let Some((desk, w)) = t.get_mut(&h) else {
        return;
    };
    let moved = if holding { w.hold(sid) } else { w.release(sid) };
    if moved {
        desk.set(w.sids(), Default::default());
    }
}

/// 每台的名单台与「几处在看」的计数（[`open`] 起、[`close`] 摘）。
fn watching() -> &'static Mutex<HashMap<i64, (Arc<Desk>, Watched)>> {
    static W: OnceLock<Mutex<HashMap<i64, (Arc<Desk>, Watched)>>> = OnceLock::new();
    W.get_or_init(Default::default)
}

/// 此刻有人在看的那几条（每条记有几处在看：一处放手不影响别处）。名单 = 计数大于零的那几条。
#[derive(Debug, Default, Clone)]
pub struct Watched {
    held: BTreeMap<String, u32>,
}

impl Watched {
    /// 多一处在看。回名单变没变。
    pub fn hold(&mut self, sid: &str) -> bool {
        let n = self.held.entry(sid.to_string()).or_insert(0);
        *n += 1;
        *n == 1
    }

    /// 少一处在看。回名单变没变（本来就没在看 ⇒ 不变）。
    pub fn release(&mut self, sid: &str) -> bool {
        match self.held.get_mut(sid) {
            Some(n) if *n > 1 => {
                *n -= 1;
                false
            }
            Some(_) => {
                self.held.remove(sid);
                true
            }
            None => false,
        }
    }

    pub fn sids(&self) -> BTreeSet<String> {
        self.held.keys().cloned().collect()
    }
}

/// ［重试］（`retry`：断开这一条、马上整份重来）· 回前台 / 网络变了（断着才马上再接）。
pub fn kick(h: i64, retry: bool) {
    if let Some(f) = feed(h) {
        if retry {
            f.reconnect();
        } else {
            f.wake();
        }
    }
}

pub fn close(h: i64) {
    bases().lock().unwrap_or_else(|e| e.into_inner()).remove(&h);
    facts().lock().unwrap_or_else(|e| e.into_inner()).remove(&h);
    watching()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&h);
    unavailables()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&h);
    if let Some(f) = feeds().lock().unwrap_or_else(|e| e.into_inner()).remove(&h) {
        f.close();
    }
}

/// 一问一答的结局 ⇒ 交 Kotlin 的那一行：`{call, ok:true, data}` · `{call, ok:false, code, message, detail, data?}` ·
/// `{call, timeout:true}` · `{call, cancelled:true}` · `{call, down:true}` · `{call, unreadable:true, detail}`。
/// 成了的回包按两端同一份线上类型解（`cmd_wire::replies::decode_reply`）：解得开 ⇒ `data` 是按类型重写的那一份；
/// 形状对不上 ⇒ `unreadable`（serde 那一句进详情）；回包类型还不住那里的命令 ⇒ 原样。事前就拒的那几种（不认 · 形状不同 · 这台做不到 · 在等的太多）
/// 经通信层那一份分流器说成一句（`route_call_error`），照失败交。
pub fn reply_json(
    id: i64,
    machine: &str,
    cmd: &str,
    got: Result<Option<serde_json::Value>, CallError>,
) -> String {
    use comms_inward::backend_route::{route_call_error, Routed};
    let v = match &got {
        Ok(data) => {
            let data = data.clone().unwrap_or(serde_json::Value::Null);
            match cmd_wire::replies::decode_reply(cmd, data.clone()) {
                None => serde_json::json!({ "call": id, "ok": true, "data": data }),
                Some(Ok(typed)) => serde_json::json!({ "call": id, "ok": true, "data": typed }),
                Some(Err(why)) => serde_json::json!({
                    "call": id, "unreadable": true,
                    "detail": format!("{cmd}: {why}"),
                }),
            }
        }
        Err(CallError::Remote {
            code,
            message,
            detail,
            data,
        }) => serde_json::json!({
            "call": id, "ok": false, "code": code, "message": message, "detail": detail,
            "data": data.as_deref().and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok()),
        }),
        Err(CallError::Timeout { .. }) => serde_json::json!({ "call": id, "timeout": true }),
        Err(CallError::Cancelled) => serde_json::json!({ "call": id, "cancelled": true }),
        Err(CallError::Disconnected) => serde_json::json!({ "call": id, "down": true }),
        Err(e) => {
            let code = match e {
                CallError::Unsupported { .. } => "unsupported".to_string(),
                CallError::Differs { .. } => "differs".to_string(),
                CallError::Unavailable { code, .. } => code.clone(),
                _ => "busy".to_string(),
            };
            let (Routed::NoChannel(said) | Routed::Refused(said)) =
                route_call_error(e, machine, |_code, message| message.to_string());
            serde_json::json!({ "call": id, "ok": false, "code": code, "message": said, "detail": e.to_string() })
        }
    };
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一问一答的结局逐种落到 Kotlin 认的那一行；事前就拒的那几种照失败交、带码与一句话。
    #[test]
    fn every_outcome_lands_as_the_line_kotlin_reads() {
        let j = |r: Result<Option<serde_json::Value>, CallError>| -> serde_json::Value {
            serde_json::from_str(&reply_json(7, "box", "history-facts", r)).expect("JSON")
        };
        assert_eq!(
            j(Ok(Some(serde_json::json!({"a": 1})))),
            serde_json::json!({"call": 7, "ok": true, "data": {"a": 1}})
        );
        let failed = j(Err(CallError::Remote {
            code: "no_tmux".into(),
            message: "没有 tmux".into(),
            detail: "d".into(),
            data: Some("{\"x\":1}".into()),
        }));
        assert_eq!(failed["ok"], false);
        assert_eq!(failed["code"], "no_tmux");
        assert_eq!(failed["data"], serde_json::json!({"x": 1}));
        assert_eq!(j(Err(CallError::Cancelled))["cancelled"], true);
        assert_eq!(j(Err(CallError::Disconnected))["down"], true);
        let differs = j(Err(CallError::Differs {
            cmd: "history-search".into(),
            said: copy_core::backend_differs("box"),
        }));
        assert_eq!(differs["code"], "differs");
        assert_eq!(differs["message"], copy_core::backend_differs("box"));
        let old = j(Err(CallError::Unsupported {
            cmd: "x".into(),
            offered: vec![],
        }));
        assert_eq!(old["code"], "unsupported");
        assert_eq!(old["message"], copy_core::backend_old("box"));
    }

    /// 回包类型住 `cmd-wire` 的那几条：按类型解；缺一格 ⇒ 交 `unreadable`（不交 `ok`，Kotlin 不会拿去当成功用）；别的命令原样。
    #[test]
    fn typed_replies_are_decoded_here_once_and_a_short_one_is_unreadable() {
        let j = |cmd: &str, data: serde_json::Value| -> serde_json::Value {
            serde_json::from_str(&reply_json(9, "box", cmd, Ok(Some(data)))).expect("JSON")
        };
        let sent = serde_json::json!({"result": "delivered"});
        assert_eq!(
            j("terminal-input", sent.clone()),
            serde_json::json!({"call": 9, "ok": true, "data": sent})
        );
        let short = j("history-tail", serde_json::json!({"total": 1}));
        assert_eq!(short["unreadable"], true);
        assert!(short.get("ok").is_none());
        assert!(short["detail"]
            .as_str()
            .unwrap()
            .starts_with("history-tail: "));
        let raw = serde_json::json!({"anything": [1, 2]});
        assert_eq!(j("history-facts", raw.clone())["data"], raw);
    }

    /// 记录一条一条地降级：一页里混一条形状对不上的（见过的类、少了格）与一条没见过的类，
    /// 整页照常交 `ok`，好的那条原样，坏的那两条各成一张「认不出的记录」卡（`unread`，同桌面那几个文案键，带原样的类名与摘录）。
    #[test]
    fn one_unreadable_record_becomes_an_unread_card_and_the_page_stays_readable() {
        use copy_core::copy_text;
        let good = serde_json::json!({"agent": "claude", "by": "agent", "id": "@1", "t": "title", "text": "Fix"});
        let short = serde_json::json!({"agent": "claude", "id": "@2", "t": "reply", "model": "m"});
        let novel = serde_json::json!({"agent": "claude", "id": "@3", "t": "brandNew", "x": 1});
        let card = |raw: &serde_json::Value, why: &str, kind: &str, id: &str| {
            let text = if why == "parseFailed" {
                copy_text("record.unread.parseFailed", &[("type", kind)])
            } else {
                copy_text("record.unread.unknownType", &[("type", kind)])
            };
            serde_json::json!({
                "agent": "claude", "id": id, "t": "unread", "why": why, "type": kind,
                "text": text, "tone": "warn",
                "excerpt": raw.to_string(), "count": 1,
            })
        };
        let row = |r: &serde_json::Value| serde_json::json!({"end": 10, "hash": 1, "record": r, "cwd": "/w"});
        let read = serde_json::json!({"rows": [row(&good), row(&short), row(&novel)], "next": 30, "eof": true});
        let got: serde_json::Value =
            serde_json::from_str(&reply_json(4, "box", "history-read", Ok(Some(read))))
                .expect("JSON");
        assert_eq!(got["ok"], true, "{got}");
        assert_eq!(got["data"]["rows"][0]["record"], good);
        assert_eq!(
            got["data"]["rows"][1]["record"],
            card(&short, "parseFailed", "reply", "@2")
        );
        assert_eq!(
            got["data"]["rows"][2]["record"],
            card(&novel, "unknownType", "brandNew", "@3")
        );
        assert_eq!(got["data"]["rows"][1]["cwd"], "/w");

        let line = |seq: u64, r: &serde_json::Value| serde_json::json!({"session_id": "s", "path": "/p", "seq": seq, "end": (seq + 1) * 10, "cwd": null, "record": r});
        let lines = serde_json::json!({"from": 0, "next": 2, "end": 20, "eof": false, "lines": [line(0, &good), line(1, &short)]});
        let got: serde_json::Value =
            serde_json::from_str(&reply_json(5, "box", "history-lines", Ok(Some(lines))))
                .expect("JSON");
        assert_eq!(got["ok"], true, "{got}");
        assert_eq!(got["data"]["lines"][0]["record"], good);
        assert_eq!(
            got["data"]["lines"][1]["record"],
            card(&short, "parseFailed", "reply", "@2")
        );

        // 没有类名 · 没有身份的那一团：类名照桌面写「无类型」那一句，身份按原文摘出一个稳定的。
        let bare = serde_json::json!({"agent": 3});
        let read = serde_json::json!({"rows": [row(&bare)], "next": 1, "eof": true});
        let got: serde_json::Value =
            serde_json::from_str(&reply_json(6, "box", "history-read", Ok(Some(read))))
                .expect("JSON");
        let rec = &got["data"]["rows"][0]["record"];
        assert_eq!(rec["t"], "unread");
        assert_eq!(rec["type"], serde_json::Value::Null);
        assert_eq!(
            rec["text"],
            copy_text(
                "record.unread.unknownType",
                &[("type", &copy_text("record.unread.noType", &[]))]
            )
        );
        assert!(!rec["id"].as_str().unwrap().is_empty());

        // 流上的 `line` 帧：同一个解法（那一条成卡、帧照交）。
        let frame = serde_json::json!({"kind": "line", "session_id": "s", "path": "/p", "seq": 7, "byte_offset": 70, "record": short});
        let (k, b) = event_of(&Event::Frame(frame.to_string())).unwrap();
        assert_eq!(k, "frame");
        let v: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert_eq!(v["record"], card(&short, "parseFailed", "reply", "@2"));
        assert_eq!(
            (v["seq"].as_u64(), v["byte_offset"].as_u64()),
            (Some(7), Some(70))
        );
        let fine = serde_json::json!({"kind": "line", "session_id": "s", "path": "/p", "seq": 8, "byte_offset": 80, "record": good});
        let v: serde_json::Value =
            serde_json::from_str(&event_of(&Event::Frame(fine.to_string())).unwrap().1).unwrap();
        assert_eq!(v, fine);
    }

    /// 帧按 `cmd-wire` 那一份类型解过、再按类型写一行交 Kotlin（线上同名的格；类型里没有的格不交）；解不开 ⇒ 照实交认不出的那一行。
    #[test]
    fn frames_are_rewritten_by_type_and_an_unreadable_one_says_so() {
        let removed = r#"{"kind":"session_removed","sid":"s","cause":"gone","later":1}"#;
        let (k, b) = event_of(&Event::Frame(removed.into())).unwrap();
        assert_eq!(k, "frame");
        let v: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert_eq!(
            (v["kind"].as_str(), v["sid"].as_str()),
            (Some("session_removed"), Some("s"))
        );
        assert!(v.get("later").is_none(), "类型里没有的格不交：{v}");
        assert_eq!(
            event_of(&Event::Frame("{\"kind\":\"x\"}".into())),
            Some(("unread", "{\"kind\":\"x\"}".to_string()))
        );
    }

    /// 交 Kotlin 的几种事：状态那一件走 [`Facts::round`]（要这台手上的事实）· 帧原样。
    #[test]
    fn events_are_tagged_and_frames_go_through_untouched() {
        assert_eq!(
            event_of(&Event::State(State::Connecting)),
            None,
            "状态那一件经 Facts::round"
        );
        assert_eq!(event_of(&Event::Gap), None, "缺口 Kotlin 用不着");
        assert_eq!(
            event_of(&Event::Ssh(true)),
            Some(("ssh", "true".to_string()))
        );
    }

    /// 几处在看同一条：计数到零才出名单；两处 hold、放一处 ⇒ 名单不变。
    #[test]
    fn a_session_leaves_the_list_only_when_the_last_holder_lets_go() {
        let mut w = Watched::default();
        assert!(w.sids().is_empty(), "从空开始");
        assert!(w.hold("a"), "第一处在看 ⇒ 名单变了");
        assert!(!w.hold("a"), "第二处在看 ⇒ 名单没变");
        assert!(!w.release("a"));
        assert_eq!(w.sids().len(), 1);
        assert!(w.release("a"), "最后一处放手 ⇒ 名单变了");
        assert!(w.sids().is_empty());
        assert!(!w.release("a"), "本来就没在看 ⇒ 不变");
    }

    /// 部署收了（成没成都算）：成品回到那条流最新那一轮，不停在「正在装」；那条流一轮都没交过 ⇒ 照「正在连」说。
    #[test]
    fn a_finished_deploy_hands_the_product_back_to_the_last_round() {
        let v = |t: String| serde_json::from_str::<serde_json::Value>(&t).unwrap();
        let mut f = Facts::default();
        let before = v(f.round(
            State::Down {
                problem: Problem::new("远端没有后端", "exit: 127"),
                needs_hand: true,
                blocker: Some(cmd_wire::machine::Blocker::NoBackend),
            },
            "box",
        ));
        let busy = v(f.step(deploy::Step::Place { updating: false }, "box"));
        assert_eq!(
            (busy["state"].as_str(), busy["machine"]["state"].as_str()),
            (Some("down"), Some("installing"))
        );
        assert_eq!(v(f.settle("box")), before);
        let mut fresh = Facts::default();
        let line = v(fresh.step(deploy::Step::Plan, "box"));
        assert_eq!(line["state"], "connecting");
        assert_eq!(v(fresh.settle("box"))["machine"]["state"], "connecting");
    }

    /// 状态那一行的金样：一串事实由生产函数现产，与 `tests/__fixtures__/mobile-state-line.golden.json` 逐字相等；
    /// 手机那一侧（`ResidentFeedTest`）读同一份逐行解。只有通没通与成品，画法那几格（修法 · 版本那一句）不另带；部署那一趟的几步同在这一份里。
    #[test]
    fn the_state_line_matches_the_cross_language_golden() {
        use cmd_wire::attach::Relation;
        use cmd_wire::machine::{Blocker, Why};
        let up = |epoch, relation| State::Up {
            epoch,
            relation,
            differs: vec![],
        };
        let down = |problem: Problem, blocker: Option<Blocker>| State::Down {
            problem,
            needs_hand: blocker.is_some(),
            blocker,
        };
        let refused = Problem::new("连接失败 · 密钥被拒", "命令：dial\n原话：denied")
            .because(Some(Why::Auth), (None, None));
        let cases: Vec<(&str, State)> = vec![
            ("正在连", State::Connecting),
            ("连着", up(1, Relation::Same)),
            ("那台旧", up(2, Relation::Older)),
            ("那台新", up(3, Relation::Newer)),
            (
                "没有后端",
                down(
                    Problem::new("远端没有后端", "exit: 127"),
                    Some(Blocker::NoBackend),
                ),
            ),
            (
                "后端旧了",
                down(
                    Problem::new("后端要更新", "refused: x"),
                    Some(Blocker::OldBackend),
                ),
            ),
            ("密钥被拒", down(refused, None)),
            ("离线", down(Problem::new("box 离线", ""), None)),
            (
                "不是 Unix",
                down(
                    Problem::new("仅支持 Unix", "setsid"),
                    Some(Blocker::NotUnix),
                ),
            ),
        ];
        let no_backend = || {
            down(
                Problem::new("远端没有后端", "exit: 127"),
                Some(Blocker::NoBackend),
            )
        };
        // 部署那一趟：那条流先交一轮，再走到一步（通没通照那一轮说，成品照那一步说）。
        let deploying: Vec<(&str, State, deploy::Step)> = vec![
            ("部署在问那台", no_backend(), deploy::Step::Plan),
            (
                "正在装",
                no_backend(),
                deploy::Step::Place { updating: false },
            ),
            (
                "正在更新",
                up(2, Relation::Older),
                deploy::Step::Place { updating: true },
            ),
        ];
        let line = |text: String| serde_json::from_str::<serde_json::Value>(&text).unwrap();
        let mut out: Vec<serde_json::Value> = cases
            .into_iter()
            .map(|(name, s)| {
                let line = line(Facts::default().round(s, "box"));
                serde_json::json!({ "name": name, "line": line })
            })
            .collect();
        out.extend(deploying.into_iter().map(|(name, s, step)| {
            let mut f = Facts::default();
            f.round(s, "box");
            let line = line(f.step(step, "box"));
            serde_json::json!({ "name": name, "line": line })
        }));
        let golden = serde_json::json!({
            "说明": [
                "手机核心交 Kotlin 的 `state` 那一行（通没通 ＋ 那台的状态成品）的跨语言金样。",
                "Rust：src/mobile/core-rs/rust/resident.rs 按 name 走生产函数现产，逐字相等。",
                "Kotlin：core-claude 的 ResidentFeedTest 读同一份逐行解成 LinkState。",
                "机器名 · 那一句 · 详情是结构夹具，不取自任何真机。"
            ],
            "cases": out,
        });
        let text = serde_json::to_string_pretty(&golden).unwrap() + "\n";
        let rel = "tests/__fixtures__/mobile-state-line.golden.json";
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let want = std::fs::read_to_string(root.join(rel)).unwrap_or_default();
        let fresh = root.join(".build/golden").join(rel);
        if text != want {
            let _ = std::fs::create_dir_all(fresh.parent().unwrap());
            let _ = std::fs::write(&fresh, &text);
            panic!("{rel} 与现打的不一致：现打的在 .build/golden/{rel}；对过后跑 bash tests/scripts/bless-golden.sh 拷回（手机 ResidentFeedTest 读它）");
        }
        let _ = std::fs::remove_file(&fresh);
    }

    /// 拨号核心没开成的那一份（`DialFail`）原样换成 `Problem`：那一句 · 详情 · 原因码 · 看到的与库里那一把。
    #[test]
    fn a_dial_fail_becomes_the_problem_field_for_field() {
        let p = problem_of(ssh_dial::DialFail {
            error: "指纹变了".into(),
            detail: Some("d".into()),
            fingerprint: Some("SHA256:seen".into()),
            reason: Some(cmd_wire::machine::Why::HostKey),
            key_type: Some("ssh-ed25519".into()),
            expected_fingerprint: Some("SHA256:kept".into()),
        });
        assert_eq!((p.text.as_str(), p.detail.as_str()), ("指纹变了", "d"));
        assert_eq!(
            (p.why, p.seen_key.as_deref(), p.expected_key.as_deref()),
            (
                Some(cmd_wire::machine::Why::HostKey),
                Some("SHA256:seen"),
                Some("SHA256:kept")
            )
        );
    }

    /// 手机这一侧经有类型的口拨：拨一个没人听的本机口 ⇒ 一口回没开成（原因码「够不着」），不经 ack 那一行。
    #[test]
    fn a_capture_to_a_closed_port_comes_back_as_an_unreachable_problem() {
        let sock = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        drop(sock);
        let link =
            PhoneLink::new(serde_json::json!({ "host": "127.0.0.1", "port": port, "user": "u" }));
        let got = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(link.capture("true".into()));
        let p = got.err().expect("没人听的口居然开成了");
        assert_eq!(p.why, Some(cmd_wire::machine::Why::Unreachable), "{p:?}");
        assert!(!p.text.is_empty());
    }
}
