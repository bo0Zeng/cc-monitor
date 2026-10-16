//! **一台的常驻流**在手机这一侧的宿主：循环本身是 `resident-client` 那一份（[`Feed`]，桌面同一条循环），
//! 这里只接两根线 —— 下面那条 SSH（[`PhoneLink`]：拨号核心在这台手机上开的链路，拨号请求由 Kotlin 交那一份）· 往 Kotlin 交的事（一个回调，走消息）。
//!
//! 往 Kotlin 交的事一律 `(编号, 种类, 一段文本)`：`state`（这台通不通，JSON；SSH 那层跟着它换连接）· `frame`（流上的一帧，原样一行；`line` 帧里解不开的那条记录换成认不出那一条）·
//! `watch_from`（JSON 数组）· `ssh`（SSH 够没够着那一台：`true` / `false`，与流通不通分开；SSH 那层跟它走）· `reply`（一问一答的结局，JSON，带 `call` 编号）。

use resident_client::client::CallError;
use resident_client::feed::{
    sink, Boxed, Config, Event, Exec, Feed, Problem, Progress, State, Transport,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, Join};

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

    /// 在这条连接上开一条用法链路，读到 ack；回下行读端与上行写端。没开成 ⇒ 那一句 ＋ 详情。
    /// 拨号核心报的进展（ack 之前的 `progress` 行）原样交给 `progress`。
    async fn open(
        &self,
        uses: serde_json::Value,
        progress: &Progress,
    ) -> Result<(DuplexStream, DuplexStream), Problem> {
        let mut req = self.base.clone();
        if let (Some(r), Some(u)) = (req.as_object_mut(), uses.as_object()) {
            for (k, v) in u {
                r.insert(k.clone(), v.clone());
            }
            r.insert("stages".into(), false.into());
            r.insert("progress".into(), true.into());
        }
        let req: ssh_dial::DialRequest = serde_json::from_value(req).map_err(|e| {
            Problem::new(
                copy_core::copy_text("mobSsh.conn.unavailable", &[]),
                e.to_string(),
            )
        })?;
        let (up_w, up_r) = tokio::io::duplex(256 * 1024);
        let (mut down_w, mut down_r) = tokio::io::duplex(256 * 1024);
        let dialing = tokio::spawn(async move {
            let stages = ssh_dial::StageSink::new(false);
            ssh_dial::uses::run_with(
                &req,
                &stages,
                up_r,
                &mut down_w,
                ssh_dial::uses::Control::none(),
            )
            .await;
            let _ = down_w.shutdown().await;
        });
        // 等 ack 那一段被丢下（循环到拨号期限放弃 · 宿主叫停）⇒ 这一拨的任务一并收掉：挂在 SYN 上的那一拨不留着占这个身份的拨号锁，
        // 下一拨才拨得出去。拿到 ack 之后它就是这条链路本身，随两头的读写端收工。
        let dialing = AbortOnDrop(Some(dialing.abort_handle()));
        let ack = read_ack(&mut down_r, progress).await?;
        dialing.keep();
        Ok((down_r, up_w)).and_then(|pair| {
            if ack_ok(&ack) {
                Ok(pair)
            } else {
                Err(ack_problem(&ack))
            }
        })
    }
}

/// 被丢下就 abort 那个任务（[`AbortOnDrop::keep`] 之后不管它）。
struct AbortOnDrop(Option<tokio::task::AbortHandle>);

impl AbortOnDrop {
    fn keep(mut self) {
        self.0 = None;
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            h.abort();
        }
    }
}

impl Transport for PhoneLink {
    type Stream = Join<DuplexStream, DuplexStream>;

    fn stream(
        &self,
        command: String,
        progress: Progress,
    ) -> Boxed<'_, Result<Self::Stream, Problem>> {
        Box::pin(async move {
            let (down, up) = self
                .open(
                    serde_json::json!({ "use": "stream", "command": command }),
                    &progress,
                )
                .await?;
            Ok(tokio::io::join(down, up))
        })
    }

    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>> {
        Box::pin(async move {
            let (mut down, _up) = self
                .open(
                    serde_json::json!({
                        "use": "capture",
                        "command": command,
                        "capture": { "max_bytes": CAPTURE_MAX },
                    }),
                    &Progress::default(),
                )
                .await?;
            let line = read_line(&mut down).await.ok_or_else(|| {
                Problem::new(copy_core::copy_text("mobSsh.conn.unavailable", &[]), "")
            })?;
            Ok(exec_of(&line))
        })
    }
}

/// 读到 ack 那一行（阶段行跳过、进展行交给 `progress`）。链路没给就收工 ⇒ 当这条连接用不了。
async fn read_ack(
    down: &mut DuplexStream,
    progress: &Progress,
) -> Result<serde_json::Value, Problem> {
    loop {
        let line = read_line(down).await.ok_or_else(|| {
            Problem::new(copy_core::copy_text("mobSsh.conn.unavailable", &[]), "")
        })?;
        let v: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
        if v.get("progress").is_some() {
            progress.step();
        } else if v.get("stage").is_none() {
            return Ok(v);
        }
    }
}

/// 一字节一字节读一行（ack 之后紧跟的是 exec 的字节，不许预读过头）。
async fn read_line(down: &mut DuplexStream) -> Option<String> {
    let mut out = Vec::new();
    let mut one = [0u8; 1];
    loop {
        match down.read(&mut one).await {
            Ok(0) | Err(_) => return None,
            Ok(_) if one[0] == b'\n' => return Some(String::from_utf8_lossy(&out).into_owned()),
            Ok(_) => out.push(one[0]),
        }
    }
}

pub fn ack_ok(ack: &serde_json::Value) -> bool {
    ack.get("ok").and_then(serde_json::Value::as_bool) == Some(true)
}

/// 没拨成的 ack ⇒ 那一句（拨号核心写好）＋ 复制详情 ＋ 原因码与那台出示的指纹。
pub fn ack_problem(ack: &serde_json::Value) -> Problem {
    let s = |k: &str| {
        ack.get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let text = s("error");
    Problem::new(
        if text.is_empty() {
            copy_core::copy_text("mobSsh.conn.unavailable", &[])
        } else {
            text
        },
        s("detail"),
    )
    .because(
        serde_json::from_value(ack["reason"].clone()).ok(),
        ack.get("fingerprint")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    )
}

/// `use: capture` 那一行 `{exit_status, stdout, stderr}`。
pub fn exec_of(line: &str) -> Exec {
    let v: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
    let s = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Exec {
        exit_status: v
            .get("exit_status")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u32::try_from(n).ok()),
        stdout: s("stdout"),
        stderr: s("stderr"),
    }
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

/// 一件事 ⇒ `(种类, 文本)`（帧原样，只有 `line` 帧里解不开的那条记录换成认不出那一条：`cmd_wire::record::mend_line_frame`）；Kotlin 用不着的（接上那一刻的 hello 与客户端：问答经 [`call`] 走 · 超长丢行的缺口）⇒ `None`。
pub fn event_of(e: &Event) -> Option<(&'static str, String)> {
    Some(match e {
        Event::State(s) => ("state", state_json(s)),
        Event::Frame(line) => (
            "frame",
            cmd_wire::record::mend_line_frame(line).unwrap_or_else(|| line.clone()),
        ),
        Event::WatchFrom(rows) => (
            "watch_from",
            serde_json::to_string(rows).unwrap_or_else(|_| "[]".into()),
        ),
        Event::Ssh(reached) => ("ssh", reached.to_string()),
        Event::Attached { .. } | Event::Gap => return None,
    })
}

/// 状态那一行（要不要「更新后端」由循环定好在 `fix` 里：通着 · 断着同一格）。
pub fn state_json(s: &State) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| r#"{"state":"connecting"}"#.to_string())
}

/// 看一台（在当前运行时上起它的循环）。`mine` = 手上这一版的身份（版本门的对照物）。回编号。
pub fn open(machine: String, mine: Option<String>, tz: Option<String>, on_event: OnEvent) -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let cfg = Config {
        machine,
        mine,
        tz,
        handshake: HANDSHAKE,
    };
    let feed = Feed::start(
        cfg,
        None,
        sink(move |e: Event| {
            if let Some((kind, body)) = event_of(&e) {
                on_event(id, kind, body);
            }
        }),
    );
    // 一条会话都还没人看 ⇒ 那台一条都不发（屏上开了哪条再经 [`watch`] 报）。
    feed.watch_none();
    feeds()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, feed);
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

/// 一问一答：回问答编号；结局经 `reply` 那一种交回（`{call, …}`，[`reply_json`]）。这台不在 ⇒ 0。
pub fn call(h: i64, cmd: String, args: &str, within_ms: u64, on_event: OnEvent) -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    let Some(f) = feed(h) else { return 0 };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
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

/// 多一处 / 少一处在看这条会话。
pub fn watch(h: i64, sid: &str, holding: bool) {
    if let Some(f) = feed(h) {
        f.watch(sid, holding);
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
    /// 开链路等 ack 那一段被丢下（循环到拨号期限放弃 · 宿主叫停）⇒ 那一拨的任务跟着收掉（不留一拨挂在 SYN 上占拨号锁）；
    /// 拿到 ack 之后 `keep` ⇒ 不管它（它就是那条链路本身）。
    #[test]
    fn a_dial_dropped_before_its_ack_takes_its_task_down() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(dropped_dial_takes_its_task_down());
    }

    async fn dropped_dial_takes_its_task_down() {
        let stuck = tokio::spawn(std::future::pending::<()>());
        drop(AbortOnDrop(Some(stuck.abort_handle())));
        // 单线程运行时里收掉一个任务只要调度器转几圈：数圈数，不看墙钟。
        let mut stuck = stuck;
        let ended = tokio::select! {
            biased;
            r = &mut stuck => Some(r),
            () = async {
                for _ in 0..64 {
                    tokio::task::yield_now().await;
                }
            } => None,
        };
        assert!(
            matches!(ended, Some(Err(ref e)) if e.is_cancelled()),
            "丢下了，那一拨还挂着"
        );
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let kept = tokio::spawn(async move {
            let _ = rx.await;
        });
        AbortOnDrop(Some(kept.abort_handle())).keep();
        tx.send(()).unwrap();
        assert!(kept.await.is_ok(), "拿到 ack 之后不该被收掉");
    }

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
            let key = if why == "parseFailed" {
                "record.unread.parseFailed"
            } else {
                "record.unread.unknownType"
            };
            serde_json::json!({
                "agent": "claude", "id": id, "t": "unread", "why": why, "type": kind,
                "text": copy_text(key, &[("type", kind)]), "tone": "warn",
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

        // 流上的 `line` 帧：同一个解法（那一条成卡、帧照交）；别的帧原样。
        let frame = serde_json::json!({"kind": "line", "session_id": "s", "path": "/p", "seq": 7, "byte_offset": 70, "record": short});
        let (k, b) = event_of(&Event::Frame(frame.to_string())).unwrap();
        assert_eq!(k, "frame");
        let v: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert_eq!(v["record"], card(&short, "parseFailed", "reply", "@2"));
        assert_eq!(
            (v["seq"].as_u64(), v["byte_offset"].as_u64()),
            (Some(7), Some(70))
        );
        let fine = serde_json::json!({"kind": "line", "session_id": "s", "path": "/p", "seq": 8, "record": good});
        let v: serde_json::Value =
            serde_json::from_str(&event_of(&Event::Frame(fine.to_string())).unwrap().1).unwrap();
        assert_eq!(v, fine);
    }

    /// 交 Kotlin 的几种事：状态是带 `state` 标签的 JSON · 帧原样。
    #[test]
    fn events_are_tagged_and_frames_go_through_untouched() {
        let (k, b) = event_of(&Event::State(State::Up {
            epoch: 3,
            relation: cmd_wire::attach::Relation::Newer,
            differs: vec![],
            notice: Some(Problem::new("box 后端较新 · 不会换回", "")),
            fix: None,
        }))
        .unwrap();
        assert_eq!(k, "state");
        let v: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert_eq!(v["state"], "up");
        assert_eq!(v["epoch"], 3);
        assert_eq!(v["relation"], "newer");
        assert!(v.get("fix").is_none(), "那台新 ⇒ 不点名「更新」：{v}");
        let (k, b) = event_of(&Event::State(State::Down {
            problem: Problem::new("box 离线", ""),
            needs_hand: true,
            unsupported: false,
            fix: None,
        }))
        .unwrap();
        assert_eq!(k, "state");
        let v: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert_eq!(
            (
                v["state"].as_str(),
                v["needs_hand"].as_bool(),
                v["problem"]["text"].as_str()
            ),
            (Some("down"), Some(true), Some("box 离线"))
        );
        assert_eq!(
            event_of(&Event::Frame("{\"kind\":\"x\"}".into())),
            Some(("frame", "{\"kind\":\"x\"}".to_string()))
        );
        assert_eq!(event_of(&Event::Gap), None, "缺口 Kotlin 用不着");
        assert_eq!(
            event_of(&Event::Ssh(true)),
            Some(("ssh", "true".to_string()))
        );
    }

    /// ack 与 capture 那一行的读法。
    #[test]
    fn the_ack_and_the_capture_line_are_read_as_the_dial_core_writes_them() {
        assert!(ack_ok(&serde_json::json!({"ok": true})));
        let p = ack_problem(&serde_json::json!({"ok": false, "error": "拨号失败", "detail": "d"}));
        assert_eq!((p.text.as_str(), p.detail.as_str()), ("拨号失败", "d"));
        assert_eq!((p.why, p.seen_key), (None, None));
        let p = ack_problem(&serde_json::json!({
            "ok": false, "error": "指纹变了", "reason": "host_key", "fingerprint": "SHA256:seen"
        }));
        assert_eq!(
            (p.why, p.seen_key.as_deref()),
            (Some(cmd_wire::machine::Why::HostKey), Some("SHA256:seen"))
        );
        assert_eq!(
            exec_of(r#"{"exit_status":127,"stdout":"","stderr":"nope"}"#),
            Exec {
                exit_status: Some(127),
                stdout: String::new(),
                stderr: "nope".into()
            }
        );
        assert_eq!(
            exec_of(r#"{"exit_status":null,"stdout":"x","stderr":""}"#).exit_status,
            None
        );
    }
}
