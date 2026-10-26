//! 远端一台的那条流，monitor 这一侧。**重连循环不在这里**：断线重连 · 握手 · ensure · 版本门（旧了换装）· 退避都是
//! `resident-client` 那一条（[`Feed`]，手机同一份；全仓只这一处，`one_loop_guard`）。这里只接宿主那几根线：
//! - 链路（[`DeskLink`]）：本机常驻后端给的那条（`dial_host`）。这一条不装后端：装 · 版本门 · 那台的状态成品都归本机后端自己那条（`control/feeds.rs`）；
//! - 循环交出来的事落到哪（[`consume_remote`]）：登记表 · 会话账本 · 会话流 · 攒批 · 旁路快照。

use super::*;
use crate::event_replay::EventReplay;
use crate::session_book::In as BookIn;
use cmd_wire::machine::Blocker;
use resident_client::feed::{Boxed, Event, Exec, Feed, Problem, Progress, State, Transport};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

/// `remote-health` 的出口：宿主（`lib.rs::remote_health_out`）拿窗口把手造它，本模块只调它、不认识 GUI 宿主（`backend_client_guard_tests.rs::GUARDED`）。
/// 回 `Err(原话)` ＝ 没发出去，各发射点那句 warn 照旧。
pub(crate) type HealthOut =
    Arc<dyn Fn(crate::ui_contract::RemoteHealthPayload) -> Result<(), String> + Send + Sync>;

/// 循环交出来的事排队的深度：monitor 这边收慢了，循环就停在交的那一步、不再往下读（背压回到那台）。
const EVENT_QUEUE: usize = 1024;

/// 握手每一行等多久：冷启要起常驻、读盘（两行合计不超过从前一趟链路的总时限 `dial_host::ONE_SHOT_DEADLINE`）。
const HANDSHAKE_LINE: std::time::Duration = std::time::Duration::from_secs(60);

/// 「这台」那几句的读法：给人看的那一句 ＋ 复制详情。
pub(crate) fn said_of(p: &Problem) -> crate::detail::Said {
    if p.detail.trim().is_empty() {
        crate::detail::Said::written(p.text.clone())
    } else {
        crate::detail::Said::with_raw(p.text.clone(), &p.detail)
    }
}

/// 链路没开成 ⇒ 交给循环的那一份（原因码与那台出示的指纹随它走）。
fn problem_of(u: crate::dial_host::Unopened) -> Problem {
    Problem::new(u.said.said, u.said.detail).because(u.why, (u.seen_key, u.expected_key))
}

/// 远端那台的链路：本机常驻后端开的（`dial_host`）。不装后端（那台旧了照接，换装由本机后端那一条做）。
pub(crate) struct DeskLink {
    /// 那台的名字（怎么拨照本机后端的机器表）。
    machine: String,
}

impl Transport for DeskLink {
    type Stream = crate::dial_host::DialStream;

    fn stream(
        &self,
        command: String,
        progress: Progress,
    ) -> Boxed<'_, Result<Self::Stream, Problem>> {
        Box::pin(async move {
            let host_label = self.machine.clone();
            let t_exec = std::time::Instant::now();
            // 接成了就是订阅：摘掉一次性总时限（握手那两行各有自己的期限，`HANDSHAKE_LINE`）。
            let got = crate::dial_host::stream(&self.machine, &command, &mut || progress.step())
                .await
                .map(crate::dial_host::DialStream::lives_long)
                .map_err(problem_of);
            perf_dialed(&host_label, t_exec);
            got
        })
    }

    fn capture(&self, command: String) -> Boxed<'_, Result<Exec, Problem>> {
        Box::pin(async move {
            // 不认 `--resident-ensure` 的老后端会掉进流模式永不 EOF ⇒ 见到 hello 就提前收（读法认它、说「要更新」）。
            let exec = connect_and_exec_capture(
                &self.machine,
                &command,
                Some(cmd_wire::dial::OLD_BACKEND_MARKER),
            )
            .await
            .map_err(problem_of)?;
            Ok(Exec {
                exit_status: exec.exit_status,
                stdout: exec.stdout,
                stderr: exec.stderr,
            })
        })
    }
}

/// 开那条链路花了多久（SSH 登录 ＋ exec 小中继）：冷启动最贵的三段之一，埋 `[perf]`。
fn perf_dialed(host_label: &str, t_exec: std::time::Instant) {
    tracing::info!(
        "[perf] stream_source [{host_label}] 起流 {}ms（SSH 登录 + exec backend）",
        t_exec.elapsed().as_millis()
    );
}

/// 每台一条的循环在壳这一侧被用到的那几样：叫醒 · 握手时那台报的构建。
pub(crate) trait Looped: Send + Sync {
    fn wake(&self);
    fn theirs(&self) -> Option<String>;
}

impl<T: Transport> Looped for Feed<T> {
    fn wake(&self) {
        Feed::wake(self);
    }
    fn theirs(&self) -> Option<String> {
        Feed::theirs(self)
    }
}

/// 每台一条的循环（［重新连接］· 机器页「起」经 [`wake`] 叫醒它）。只记弱引用：流任务停了，它就跟着没了。
pub(crate) fn feeds() -> &'static Mutex<HashMap<String, Weak<dyn Looped>>> {
    static F: std::sync::OnceLock<Mutex<HashMap<String, Weak<dyn Looped>>>> =
        std::sync::OnceLock::new();
    F.get_or_init(Default::default)
}

/// 「重新连接」：那台的循环断着 ⇒ 不等退避马上再接（此刻正在拨 ⇒ 这一轮没成就不睡；连上那一刻按过的作废）。
pub(crate) fn wake(origin: &str) {
    let feed = feeds()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(origin)
        .and_then(Weak::upgrade);
    if let Some(f) = feed {
        f.wake();
    }
}

/// 那台手上那条循环握手时那台报的构建标识（诊断信息用）；没握过手 ⇒ `None`。
pub(crate) fn theirs_of(origin: &str) -> Option<String> {
    feeds()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(origin)
        .and_then(Weak::upgrade)
        .and_then(|f| f.theirs())
}

/// 流任务被停（`abort`）⇒ 这台的循环一并收掉。
struct Closing(Arc<Feed<DeskLink>>);

impl Drop for Closing {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// 循环交事的口：排进 [`consume_remote`] 读的那条队列（排满了就等）。
pub(crate) fn queue_sink(tx: tokio::sync::mpsc::Sender<Event>) -> resident_client::feed::Sink {
    Arc::new(move |e| {
        let tx = tx.clone();
        Box::pin(async move {
            // 排满了就在这里等（背压）；收的那一头没了 ＝ 这台的流任务正在收工，交不交都一样。
            if tx.send(e).await.is_err() {
                tracing::debug!("stream_source 收事那一头已收工，这一件不交了");
            }
        })
    })
}

/// 循环的定值：这台叫什么 · 手上这一版 · 看的这一台的时区。
pub(crate) fn feed_config(origin: &str) -> resident_client::feed::Config {
    resident_client::feed::Config {
        machine: origin.to_string(),
        mine: crate::byte_table::my_backend_id().map(str::to_string),
        tz: host_core::viewer_tz(),
        handshake: HANDSHAKE_LINE,
    }
}

/// **一台远端的那条流**：起它的循环（[`Feed`]）、收它交出来的事，直到这个任务被停（机器页「停」· 机器表改了）。
/// 那台不是 Unix ⇒ 循环停在「要人动手」、不按退避空转；机器页「起」叫醒它再试一次（`backend_control::backend_start`）。
pub async fn run(machine: String, replay: Arc<EventReplay>, health: HealthOut) {
    let origin = machine.clone();
    tracing::info!("stream_source connecting to [{machine}]");
    let (tx, rx) = tokio::sync::mpsc::channel(EVENT_QUEUE);
    let link = Arc::new(DeskLink {
        machine: machine.clone(),
    });
    let feed = Feed::start(
        feed_config(&origin),
        Some(link),
        crate::stream_watch::desk(&crate::origin::Origin(origin.clone())),
        queue_sink(tx),
    );
    feeds()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(origin, Arc::downgrade(&(feed.clone() as Arc<dyn Looped>)));
    let _closing = Closing(feed);
    consume_remote(machine, rx, replay, health).await;
}

/// 一条连接上 monitor 这边的那几样（接上时开、断了就丢）：攒批 ＋ 静默窗 ＋ 旁路快照（[`LineIntake`]，本机那条流用的是同一个）· 认不出的帧记账。
struct Conn {
    client: Arc<crate::inbound_client::InboundClient>,
    intake: LineIntake,
    /// 这条流上跳过了几帧认不出的。
    tally: crate::frame_tally::FrameTally,
    /// 解不出来的帧每种说一次（日志 ＋ 这台的健康信息）。
    unread: UnreadNotes,
}

/// **循环交出来的事 → monitor**（这台那条队列读到头为止）：
/// - `Attached` ＋ 紧跟的 `Up`：登记这条连接的客户端 · 资产同步 · 记账（版本提示 · 自证记忆）· 订了这台的订阅收 `Seen`；
/// - 帧：按类型解、经 [`route`] · [`apply`] 落下（本机那条流同一处）；
/// - 断了（`Down`）：残批冲完、摘登记、这台的成品整份作废（说不清）、订阅收 `Unseen`；这一轮没接上 ⇒ 那台记「离线」（不是 Unix ⇒「不支持」并出声）。
pub(crate) async fn consume_remote(
    machine: String,
    mut rx: tokio::sync::mpsc::Receiver<Event>,
    replay: Arc<EventReplay>,
    health: HealthOut,
) {
    let host_label = machine.clone();
    let mut conn: Option<Conn> = None;
    // 接上了、还没交 `Up`：那一行 hello 与它的客户端（版本那句话要等 `Up` 里的那一句）。
    let mut attached: Option<(String, Arc<crate::inbound_client::InboundClient>)> = None;
    // 这一轮连接的起点（`[perf]` 首个 hello 从这里算）。
    let mut round_started = std::time::Instant::now();
    loop {
        let got = match conn.as_mut() {
            Some(c) => c.intake.recv_or_flush(&mut rx).await,
            None => rx.recv().await,
        };
        let Some(ev) = got else {
            if let Some(mut c) = conn.take() {
                c.intake.flush().await;
                crate::inbound_client::unregister(&host_label, &c.client);
            }
            return;
        };
        match ev {
            Event::Attached { hello, client } => attached = Some((hello, client)),
            Event::State(State::Up { .. }) => {
                let Some((hello, client)) = attached.take() else {
                    continue;
                };
                conn = Some(on_attached(
                    &machine,
                    Round {
                        replay: &replay,
                        health: &health,
                        t_connect_start: round_started,
                    },
                    &hello,
                    client,
                ));
            }
            Event::State(State::Connecting) => round_started = std::time::Instant::now(),
            Event::State(State::Down {
                problem, blocker, ..
            }) => {
                attached = None;
                match conn.take() {
                    Some(mut c) => {
                        // 残批先冲完并等它发完 —— 下面的断连归档必须晚于这些行到达前端。
                        c.intake.flush().await;
                        crate::inbound_client::unregister(&host_label, &c.client);
                        tracing::warn!("stream_source [{host_label}] remote source ended");
                    }
                    None => on_round_failed(
                        &host_label,
                        &health,
                        &problem,
                        blocker == Some(Blocker::NotUnix),
                    ),
                }
                // 这条连接没了 ⇒ 订了这台会话流的那些订阅原位收一格 `Unseen`（不是终点）。
                replay.origin_seen(&crate::origin::Origin(host_label.clone()), false);
                // 连接断了 ≠ 会话死了：这台的成品整份作废，当时活的 / 可重连的一律「说不清」；
                //   重连之后那台的新连接自己重报一遍（它的账本从 tmux 推出可重连，`observe/session_ledger.rs`）。
                tracing::info!(
                    "stream_source [{host_label}] connection ended ⇒ 这台的会话成品作废（说不清）"
                );
                crate::session_book::feed(BookIn::LinkLost {
                    origin: host_label.clone(),
                });
            }
            Event::Frame(line) => match conn.as_mut() {
                Some(c) => {
                    let say = |p| {
                        if let Err(e) = health(p) {
                            tracing::warn!("stream_source remote-health (frame) emit failed: {e}");
                        }
                    };
                    let origin = crate::origin::Origin(host_label.clone());
                    let frame = c.unread.take(&line, None, &mut c.tally, &say);
                    let ordered =
                        frame.and_then(|f| route(Via::Remote, &origin, Some(&c.client), f));
                    if let Some(item) = ordered {
                        apply(&origin, item, &mut c.intake, &replay, &health).await;
                    }
                }
                None => tracing::warn!(
                    "stream_source [{host_label}] 还没接上就来了一帧，丢掉：{}",
                    line.chars().take(200).collect::<String>()
                ),
            },
            // 这里有一行超长、整行丢了 ⇒ 订阅原位一格 `Gap`（`to_seq` 缺）。
            Event::Gap => {
                if let Some(c) = conn.as_mut() {
                    c.intake.lost().await;
                }
            }
            // 桌面没有另一层 SSH 跟着它走（终端走本机后端那一套）⇒ 用不着。
            Event::Ssh(_) => {}
        }
    }
}

/// 接上那一刻：登记客户端（写半边解冻之后才有它）· 资产同步 · hello 那一臂的记账。
fn on_attached(
    machine: &str,
    round: Round,
    hello: &str,
    client: Arc<crate::inbound_client::InboundClient>,
) -> Conn {
    let Round { replay, health, .. } = round;
    let host_label = machine.to_string();
    crate::inbound_client::register(&host_label, client.clone());
    // 连上那一刻：让本机常驻后端沿池里那条 SSH 同步资产目录（后台跑，零判定）。
    crate::asset_sync::on_remote_ready(
        &host_label,
        client.accepts(crate::asset_sync::REMOTE_NEEDS),
    );
    // 〔一次性迁移 · 下一版删〕桌面旧书签文件里这台那一格交给这台后端。
    crate::bookmarks_migration::after_ready(crate::origin::Origin(host_label.clone()));
    let mut conn = Conn {
        client,
        intake: LineIntake::open(host_label.clone(), replay, health),
        tally: crate::frame_tally::FrameTally::new(format!("stream_source {host_label}")),
        unread: UnreadNotes::new(host_label.clone(), host_label.clone()),
    };
    let say_health = |p: crate::ui_contract::RemoteHealthPayload| {
        if let Err(e) = health(p) {
            tracing::warn!("stream_source remote-health (frame) emit failed: {e}");
        }
    };
    // 循环那一侧认过它是 hello（不然接不上）；这一侧的帧类型解不出 ⇒ 两边对 hello 的读法漂了，说出来。
    let Some(Frame::Hello(cmd_wire::frame::Hello {
        build_id,
        host_arch,
        commands,
        ..
    })) = conn.unread.take(hello, None, &mut conn.tally, &say_health)
    else {
        tracing::warn!("stream_source [{host_label}] hello 这一侧解不出：{hello}");
        return conn;
    };
    on_hello(
        host_label,
        round,
        HelloSeen {
            build_id,
            host_arch,
            commands,
        },
    );
    conn
}

/// 这一轮一次都没握上手 ⇒ 那台记成「离线」（文件窗口那一条由「重新连接中…」换成「离线 · 采样 …」＋［重新连接］）；
/// 不是 Unix ⇒ 记「不支持」、出声（界面 toast），循环停在那里等机器页「起」。
fn on_round_failed(host_label: &str, health: &HealthOut, problem: &Problem, unsupported: bool) {
    let said = said_of(problem);
    tracing::warn!(
        "stream_source [{host_label}] 这一轮没接上: {said}: {}",
        said.raw()
    );
    crate::inbound_client::note_round_failed(host_label);
    if unsupported {
        let payload = crate::ui_contract::RemoteHealthPayload {
            origin: host_label.to_string(),
            kind: "unsupported".to_string(),
            message: said.said.clone(),
            detail: said.detail.clone(),
        };
        if let Err(e) = health(payload) {
            tracing::warn!("stream_source remote-health (unsupported) emit failed: {e}");
        }
    }
}

/// 这一条连接定下来、hello 那一臂要用的几样。
#[derive(Clone, Copy)]
struct Round<'a> {
    replay: &'a Arc<EventReplay>,
    health: &'a HealthOut,
    /// 这一轮连接的起点（`[perf]` 埋点从这里算）。
    t_connect_start: std::time::Instant,
}

/// hello 帧里要读的那几样。
struct HelloSeen {
    build_id: String,
    host_arch: String,
    commands: Vec<String>,
}

/// hello 那一臂：打日志、记账。
fn on_hello(host_label: String, round: Round, hello: HelloSeen) {
    let Round {
        replay,
        t_connect_start,
        ..
    } = round;
    let HelloSeen {
        build_id,
        host_arch,
        commands,
    } = hello;
    tracing::info!(
        "stream_source backend hello: build_id={build_id} host_arch={host_arch} cmds={commands:?}"
    );
    // 订了这台会话流的那些订阅原位收一格 `Seen`（`Item::Seen`）。
    replay.origin_seen(&crate::origin::Origin(host_label.clone()), true);
    tracing::info!(
        "[perf] stream_source [{host_label}] 首个 hello T+{}ms（自这一轮连接开始）",
        t_connect_start.elapsed().as_millis()
    );
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/stream_flag_gate_tests.rs"]
mod stream_flag_gate_tests;

/// 收事那一头的接缝：overflow 那一句 · 「hello 之后才登记」等随循环搬走的那几条的去向（文件头注）。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/seam_tests.rs"]
mod seam_tests;

/// 写半边只许经 `split_and_park` 出手（住重连循环的握手那一步）：这里一个字节都不许自己往流里写、不许自己切流。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/write_half_guard.rs"]
mod write_half_guard;

/// 断线 → 重连 → 续上之后，订了这台的那几条订阅各收几格（数格、不看墙钟）。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/reconnect_cells_tests.rs"]
mod reconnect_cells_tests;
