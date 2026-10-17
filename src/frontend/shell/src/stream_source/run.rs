//! 远端一台的那条流，monitor 这一侧。**重连循环不在这里**：断线重连 · 握手 · ensure · 版本门（旧了换装）· 退避都是
//! `resident-client` 那一条（[`Feed`]，手机同一份；全仓只这一处，`one_loop_guard`）。这里只接宿主那几根线：
//! - 链路（[`DeskLink`]）：本机常驻后端给的那条（`dial_host`）· 每一轮之前的部署预检 · 装得了后端（那台旧了换装）；
//! - 循环交出来的事落到哪（[`consume_remote`]）：登记表 · 机器状态 · 会话账本 · 会话流 · 攒批 · 旁路快照。

use super::*;
use crate::event_replay::EventReplay;
use crate::session_book::In as BookIn;
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
fn said_of(p: &Problem) -> crate::detail::Said {
    if p.detail.trim().is_empty() {
        crate::detail::Said::written(p.text.clone())
    } else {
        crate::detail::Said::with_raw(p.text.clone(), &p.detail)
    }
}

fn problem_of(s: crate::detail::Said) -> Problem {
    Problem::new(s.said, s.detail)
}

/// 远端那台的链路：本机常驻后端开的（`dial_host`）。monitor 装得了后端 ⇒ 每一轮先部署预检、版本门问本机常驻后端。
pub(crate) struct DeskLink {
    cfg: RemoteConfig,
    health: HealthOut,
}

impl Transport for DeskLink {
    type Stream = crate::dial_host::DialStream;

    fn stream(
        &self,
        command: String,
        progress: Progress,
    ) -> Boxed<'_, Result<Self::Stream, Problem>> {
        Box::pin(async move {
            let host_label = self.cfg.origin_label();
            let t_exec = std::time::Instant::now();
            // 接成了就是订阅：摘掉一次性总时限（握手那两行各有自己的期限，`HANDSHAKE_LINE`）。
            let got = crate::dial_host::stream(&self.cfg, &command, &mut || progress.step())
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
                &self.cfg,
                &command,
                Some(resident_client::feed::OLD_BACKEND_MARKER),
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

    fn before_round(&self) -> Boxed<'_, ()> {
        Box::pin(preflight(&self.cfg, &self.health))
    }

    fn installs(&self) -> bool {
        true
    }
}

/// 开那条链路花了多久（SSH 登录 ＋ exec 小中继）：冷启动最贵的三段之一，埋 `[perf]`。
fn perf_dialed(host_label: &str, t_exec: std::time::Instant) {
    tracing::info!(
        "[perf] stream_source [{host_label}] 起流 {}ms（SSH 登录 + exec backend）",
        t_exec.elapsed().as_millis()
    );
}

/// 每台一条的循环（［重新连接］· 机器页「起」经 [`wake`] 叫醒它）。只记弱引用：流任务停了，它就跟着没了。
fn feeds() -> &'static Mutex<HashMap<String, Weak<Feed<DeskLink>>>> {
    static F: std::sync::OnceLock<Mutex<HashMap<String, Weak<Feed<DeskLink>>>>> =
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
pub async fn run(cfg: RemoteConfig, replay: Arc<EventReplay>, health: HealthOut) {
    let origin = cfg.origin_label();
    tracing::info!(
        "stream_source connecting to {}@{}:{}",
        cfg.user,
        cfg.host,
        cfg.port
    );
    let (tx, rx) = tokio::sync::mpsc::channel(EVENT_QUEUE);
    let link = Arc::new(DeskLink {
        cfg: cfg.clone(),
        health: health.clone(),
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
        .insert(origin, Arc::downgrade(&feed));
    let _closing = Closing(feed);
    consume_remote(cfg, rx, replay, health).await;
}

/// 每一轮开拨之前：部署预检（这台自证过就是期望 build 则跳过）。部署失败不阻断（手动部署的后端照样能连），但那句话要到界面上。
async fn preflight(cfg: &RemoteConfig, health: &HealthOut) {
    let host_label = cfg.origin_label();
    crate::machine_state::connecting(&host_label, "deploy");
    // 冷启动最贵的三段（用户点开应用到看见远端会话之间唯一的那条链）埋 `[perf]`：不改任何行为，只让下一次讨论有数可依。
    let t_connect_start = std::time::Instant::now();
    // 连接前确保远端后端已部署到固定落点（`~/.cc-monitor/bin/ccm`）；这一版没带字节 ⇒ `byte_table::choose` 回「这一版没带」→ 优雅 no-op。
    // 上一次这台机器的后端自报过就是期望 build ⇒ 跳过预检那两条连接（`VERIFIED_BUILD` 头注：记的是 hello 自证，不是预检结论）。
    // 跳过时 `confirmed_build` 记「我这一版」（只进 `[perf]` 那一行日志）。
    // 「我这一版」只有一个值：手上那份内嵌字节自报的 id；没带字节 ⇒ `None` ⇒ 不跳、不乐观。
    let mine = crate::byte_table::my_backend_id();
    let verified = verified_build_of(&host_label);
    let skip_preflight = preflight_can_be_skipped(verified.as_deref(), mine);
    let confirmed_build = if skip_preflight {
        mine.map(str::to_string)
    } else {
        match crate::deploy::ensure_backend_deployed(cfg).await {
            Ok(c) => Some(c),
            Err(e) => {
                let said = e.said();
                tracing::warn!(
                    "stream_source [{host_label}] 后端没部署上（继续尝试连接已有后端）: {said}: {}",
                    said.raw()
                );
                let payload = crate::ui_contract::RemoteHealthPayload {
                    origin: host_label.clone(),
                    kind: "deploy".to_string(),
                    message: said.said,
                    detail: said.detail,
                };
                if let Err(e) = health(payload) {
                    tracing::warn!("stream_source remote-health (deploy) emit failed: {e}");
                }
                None
            }
        }
    };
    tracing::info!(
        "[perf] stream_source [{host_label}] 部署预检 {}ms（ensure_backend_deployed；\
         confirmed_build={:?}；skip={skip_preflight}）",
        t_connect_start.elapsed().as_millis(),
        confirmed_build.as_deref()
    );
    // 接那台的常驻后端（没有就起一个；与本机同形）。起不了常驻（非 unix / 太旧）就是一次失败、说清为什么，不回落到随 SSH 生死的流模式。
    crate::machine_state::connecting(&host_label, "attach");
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
    cfg: RemoteConfig,
    mut rx: tokio::sync::mpsc::Receiver<Event>,
    replay: Arc<EventReplay>,
    health: HealthOut,
) {
    let host_label = cfg.origin_label();
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
            Event::State(State::Up {
                relation, notice, ..
            }) => {
                let Some((hello, client)) = attached.take() else {
                    continue;
                };
                conn = Some(on_attached(
                    &cfg,
                    Round {
                        replay: &replay,
                        health: &health,
                        relation,
                        notice: notice.as_ref(),
                        t_connect_start: round_started,
                    },
                    &hello,
                    client,
                ));
            }
            Event::State(State::Connecting) => round_started = std::time::Instant::now(),
            Event::State(State::Down {
                problem,
                unsupported,
                ..
            }) => {
                attached = None;
                match conn.take() {
                    Some(mut c) => {
                        // 残批先冲完并等它发完 —— 下面的断连归档必须晚于这些行到达前端。
                        c.intake.flush().await;
                        crate::inbound_client::unregister(&host_label, &c.client);
                        tracing::warn!("stream_source [{host_label}] remote source ended");
                    }
                    None => on_round_failed(&host_label, &health, &problem, unsupported),
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
    cfg: &RemoteConfig,
    round: Round,
    hello: &str,
    client: Arc<crate::inbound_client::InboundClient>,
) -> Conn {
    let Round { replay, health, .. } = round;
    let host_label = cfg.origin_label();
    crate::inbound_client::register(&host_label, client.clone());
    // 连上那一刻：让本机常驻后端沿池里那条 SSH 同步资产目录（后台跑，零判定）。
    crate::asset_sync::on_remote_ready(cfg, client.accepts(crate::asset_sync::REMOTE_NEEDS));
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
    // 这一轮没成的那一句与详情进那台的状态成品（收尾时机器那一行的［复制详情］跟它）。
    crate::machine_state::round_failed(&crate::origin::Origin(host_label.to_string()), &said);
    // 没接上 ⇒ 抹掉自证记忆：否则一台后端被删 / 被换旧的机器会每一轮都跳预检、每一轮都失败。代价是多一次预检（`VERIFIED_BUILD` 头注）。
    forget_verified_build(host_label);
    if unsupported {
        crate::machine_state::unsupported(host_label, crate::machine_state::NOT_UNIX);
        let payload = crate::ui_contract::RemoteHealthPayload {
            origin: host_label.to_string(),
            kind: "unsupported".to_string(),
            message: said.said.clone(),
            detail: said.detail.clone(),
        };
        if let Err(e) = health(payload) {
            tracing::warn!("stream_source remote-health (unsupported) emit failed: {e}");
        }
    } else {
        crate::machine_state::down(host_label);
    }
}

/// 这一条连接定下来、hello 那一臂要用的几样。
#[derive(Clone, Copy)]
struct Round<'a> {
    replay: &'a Arc<EventReplay>,
    health: &'a HealthOut,
    /// 那台相对手上这一版（那台在 attach 回话里判的）。
    relation: cmd_wire::attach::Relation,
    /// 版本那一句（同一版 ⇒ 无；字由循环那一侧写）。
    notice: Option<&'a Problem>,
    /// 这一轮连接的起点（`[perf]` 埋点从这里算）。
    t_connect_start: std::time::Instant,
}

/// hello 帧里要读的那几样。
struct HelloSeen {
    build_id: String,
    host_arch: String,
    commands: Vec<String>,
}

/// hello 那一臂：打日志、记账、版本提示、自证记忆。
fn on_hello(host_label: String, round: Round, hello: HelloSeen) {
    let Round {
        replay,
        health,
        relation,
        notice,
        t_connect_start,
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
    // 版本：新旧那台判好（attach 回话），那一句循环那一侧写好；这里按新旧挑类别（界面按它挑标题），照常接、不断开。
    crate::machine_state::up(&host_label, &build_id, relation);
    if let (Some(kind), Some(p)) = (version_health_kind(relation), notice) {
        tracing::warn!("stream_source remote [{host_label}] version: {}", p.text);
        let payload = crate::ui_contract::RemoteHealthPayload {
            origin: host_label.clone(),
            kind: kind.to_string(),
            message: p.text.clone(),
            detail: p.detail.clone(),
        };
        if let Err(e) = health(payload) {
            tracing::warn!("stream_source remote-health (version) emit failed: {e}");
        }
    }
    tracing::info!(
        "[perf] stream_source [{host_label}] 首个 hello T+{}ms（自这一轮连接开始）",
        t_connect_start.elapsed().as_millis()
    );
    // ★ F05 下半：**自证记忆的唯一写入点**。backend 自己说它是谁，我们才记。
    // build_id 不是「我这一版」（或手上没带字节、无从比）⇒ **抹掉**（这台机器上装的不是当前 build，
    // 下一轮必须照跑预检去部署），不是「留着上次的」。
    if crate::byte_table::my_backend_id() == Some(build_id.as_str()) {
        record_verified_build(&host_label, &build_id);
    } else {
        forget_verified_build(&host_label);
    }
}

/// 版本那条健康信息（`RemoteHealthPayload::kind`）的三类：界面按它挑标题（`remote-health.ts::headlineFor`），不自己判。
pub(super) const VERSION_KIND_OLDER: &str = "version-older";
pub(super) const VERSION_KIND_NEWER: &str = "version-newer";
pub(super) const VERSION_KIND_INCOMPARABLE: &str = "version-incomparable";

/// 新旧 → 健康信息的类别（同 ⇒ 不说）。
fn version_health_kind(rel: cmd_wire::attach::Relation) -> Option<&'static str> {
    use cmd_wire::attach::Relation as R;
    match rel {
        R::Same => None,
        R::Older => Some(VERSION_KIND_OLDER),
        R::Newer => Some(VERSION_KIND_NEWER),
        R::Incomparable => Some(VERSION_KIND_INCOMPARABLE),
    }
}

/// 冷启动预检的自证记忆：远端已经是当前 build 时，每次重连都付的那两条预检连接（`byte_table::probe_key` 的 `uname -s -m` 一次性 exec ·
/// 读远端 `.build_id` 的那一趟）可以跳过。记的是 hello 自报的 build_id（「这台机器上一次真的跑起来的后端就是当前期望的那个」），不是预检算出来的结论。
/// 跳的条件只有一个：记忆里那台机器的 build_id 恰好等于「我这一版」（`byte_table::my_backend_id`）；其余一律照跑。
/// 记忆过期（远端二进制被删 / 换旧）⇒ 这一轮 exec 失败，失败路径清掉记忆 ⇒ 下一轮重新预检并部署。代价是多一次重连。
static VERIFIED_BUILD: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// 纯函数：这一轮能不能跳过那两条预检连接。`verified` = [`VERIFIED_BUILD`] 里这台机器的记录（`None` = 没记过）；`mine` = 「我这一版」（`None` = 手上没带后端字节）。
/// 逐字相等，不是包含（前缀相等会把 `abc123` 与 `abc123-dirty` 混为一谈）；`mine` 是 `None` ⇒ 恒不跳。
fn preflight_can_be_skipped(verified: Option<&str>, mine: Option<&str>) -> bool {
    matches!((verified, mine), (Some(v), Some(m)) if v == m)
}

/// 读这台机器的自证记录。
fn verified_build_of(origin: &str) -> Option<String> {
    VERIFIED_BUILD.lock().ok()?.as_ref()?.get(origin).cloned()
}

/// 记下「这台机器上一次真的跑起来的后端是 `build_id`」。
///
/// ⚠ **只许在收到 hello 的那一处调**（backend 自报）。别处调就把「自证」变回了「猜」。
fn record_verified_build(origin: &str, build_id: &str) {
    if let Ok(mut g) = VERIFIED_BUILD.lock() {
        g.get_or_insert_with(HashMap::new)
            .insert(origin.to_string(), build_id.to_string());
    }
}

/// 抹掉这台机器的自证记录（连接没起来 / backend 换了身份）。
fn forget_verified_build(origin: &str) {
    if let Ok(mut g) = VERIFIED_BUILD.lock() {
        if let Some(m) = g.as_mut() {
            m.remove(origin);
        }
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/coldstart_preflight_guard.rs"]
mod coldstart_preflight_guard;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/coldstart_perf_guard.rs"]
mod coldstart_perf_guard;

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
