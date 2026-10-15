//! 远端一台的那条流，monitor 这一侧。**重连循环不在这里**：断线重连 · 握手 · ensure · 版本门（旧了换装）· 退避都是
//! `resident-client` 那一条（[`Feed`]，手机同一份；全仓只这一处，`one_loop_guard`）。这里只接宿主那几根线：
//! - 链路（[`DeskLink`]）：本机常驻后端给的那条（`dial_host`）· 每一轮之前的部署预检 · 版本门问本机常驻后端（装得了后端）；
//! - 循环交出来的事落到哪（[`consume_remote`]）：登记表 · 机器状态 · 会话账本 · 会话流 · 攒批 · 旁路快照。

use super::*;
use crate::event_replay::EventReplay;
use crate::session_book::{In as BookIn, LiveMeta};
use resident_client::feed::{
    Boxed, Event, Exec, Feed, Install, Problem, Progress, State, Transport,
};
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
        p.text.clone().into()
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

    fn installer(&self) -> Option<&dyn Install> {
        Some(self)
    }
}

impl Install for DeskLink {
    fn verdict<'a>(
        &'a self,
        mine: &'a str,
        theirs: &'a str,
        replaced: bool,
    ) -> Boxed<'a, Result<resident_client::gate::Verdict, Problem>> {
        Box::pin(async move {
            crate::remote_resident::ask_verdict(mine, theirs, replaced)
                .await
                .map_err(problem_of)
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
    let feed = Feed::start(feed_config(&origin), Some(link), queue_sink(tx));
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
        match crate::sftp::ensure_backend_deployed(cfg).await {
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

/// 一条连接上 monitor 这边的那几样（接上时开、断了就丢）：攒批 ＋ 静默窗 ＋ 旁路快照（[`LineIntake`]，本机那条流用的是同一个）· 后台会话过滤 · 认不出的帧记账。
struct Conn {
    client: Arc<crate::inbound_client::InboundClient>,
    intake: LineIntake,
    /// 没开「显示后台会话」⇒ 那台的 bg 会话（`session_added.background`）连同它的行 · 状态 · 去向一起不进（本机那条流同一个口径）。
    bg: super::local::BgHide,
    /// 这条流上跳过了几帧认不出的。
    tally: crate::frame_tally::FrameTally,
    /// 解不出来的帧每种说一次（日志 ＋ 这台的健康信息）。
    unread: UnreadNotes,
}

/// **循环交出来的事 → monitor**（这台那条队列读到头为止）：
/// - `Attached` ＋ 紧跟的 `Up`：登记这条连接的客户端 · 资产同步 · 记账（版本提示 · 自证记忆）· 订了这台的订阅收 `Seen`；
/// - 帧：解成 [`InboundFrame`]，行进攒批、会话起停的成品原样交 `session_book`（后端裁、monitor 只转交）；
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
            Event::State(State::Up { notice, .. }) => {
                let Some((hello, client)) = attached.take() else {
                    continue; // 同一条连接补交的那一句（装得了后端的这一侧不会有）
                };
                conn = Some(on_attached(
                    &cfg,
                    Round {
                        replay: &replay,
                        health: &health,
                        remote_older: notice.is_some(),
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
                Some(c) => dispatch(c, &host_label, &replay, &health, &line).await,
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
            // monitor 的在看名单经登记表上的客户端报（`stream_watch.rs`，本机远端同一处），不经循环 ⇒ 这一件不会来。
            Event::WatchFrom(_) => {}
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
        bg: super::local::BgHide::new(crate::load_show_bg_sessions()),
        tally: crate::frame_tally::FrameTally::new(format!("stream_source {host_label}")),
        unread: UnreadNotes::new(host_label.clone(), host_label.clone()),
    };
    let say_health = |p: crate::ui_contract::RemoteHealthPayload| {
        if let Err(e) = health(p) {
            tracing::warn!("stream_source remote-health (frame) emit failed: {e}");
        }
    };
    // 循环那一侧认过它是 hello（不然接不上）；这一侧的帧类型解不出 ⇒ 两边对 hello 的读法漂了，说出来。
    let Some(InboundFrame::Hello {
        v,
        build_id,
        host_arch,
        claude_dir,
        homes,
        commands,
        ..
    }) = conn.unread.take(hello, &mut conn.tally, &say_health)
    else {
        tracing::warn!("stream_source [{host_label}] hello 这一侧解不出：{hello}");
        return conn;
    };
    on_hello(
        host_label,
        round,
        HelloSeen {
            v,
            build_id,
            host_arch,
            // 日志报的是解析后的 Claude home（优先 `homes`、回退 `claude_dir`），同时把原样的 `homes` 一起打出来：排障时一眼看出这台后端发没发新字段。
            claude_home: claude_home_from_hello(&homes, &claude_dir).to_string(),
            homes,
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

/// 一帧：解成 [`InboundFrame`] 分派。未知 kind / 形状不对 → 记账、每种说一次、跳过，绝不中断流。
async fn dispatch(
    c: &mut Conn,
    host_label: &str,
    replay: &Arc<EventReplay>,
    health: &HealthOut,
    line: &str,
) {
    let host_label = host_label.to_string();
    let say_health = |p: crate::ui_contract::RemoteHealthPayload| {
        if let Err(e) = health(p) {
            tracing::warn!("stream_source remote-health (frame) emit failed: {e}");
        }
    };
    let frame = c.unread.take(line, &mut c.tally, &say_health);
    let frame = frame.filter(|f| !c.bg.hides(f));
    let intake = &mut c.intake;
    // SessionRemoved 是唯一顺序敏感的攒批边界：它的行必须先落前端，否则归档后迟到的行把 Tab 复活成僵尸 live。
    // SessionAdded / Overflow / 坏帧不作边界（多个小会话的 snapshot 才能聚成大批；行先于 Added 到达无妨：前端 ensureTab 见行即建）。
    // `session_state`（可重连 / 已结束的成品）同理：它说的「离开了」必须排在这个会话的行之后。
    if matches!(
        frame,
        Some(InboundFrame::SessionRemoved { .. } | InboundFrame::SessionState { .. })
    ) {
        intake.flush().await;
    }
    match frame {
        Some(InboundFrame::Line {
            session_id,
            path,
            seq,
            record,
            cwd,
            end,
            rid,
        }) => {
            // 进攒批缓冲（达 cap / 批龄立即整批出）；静默窗口 / SessionRemoved 边界触发的 flush 在收事那一头。
            intake
                .line(JsonlLine {
                    session_id,
                    path: std::path::PathBuf::from(path),
                    seq,
                    record,
                    cwd,
                    end: Some(end),
                    rid,
                    catch_up: false,
                })
                .await;
        }
        Some(InboundFrame::SessionAdded {
            sid,
            background,
            attachable,
            cwd,
            project_dir,
            name,
            path: _,
            activity,
            naming,
            waiting_for,
            container,
            // pid 只给本机那条流用（本机 ↗ 绑窗口）；远端这条流的声明去掉了它（`resident_client::attach::remote_stream_view`）。
            pid: _,
        }) => on_session_added(
            &host_label,
            sid,
            LiveMeta {
                background,
                attachable,
                cwd,
                project_dir,
                name,
                activity,
                naming,
                waiting_for,
                container,
                pid: None,
            },
        ),
        // 读完了：有记录文件就排旁路快照（无 path = 会话刚起还没写 jsonl ⇒ 无历史可拉，后续行天然从 tail 全量到达）。
        //   队列按 sid 幂等（重复宣告不重拉）。
        Some(InboundFrame::SessionPrimed { sid, path, lines }) => {
            intake.announced(&sid, path, lines)
        }
        Some(InboundFrame::SessionStatus {
            sid,
            activity,
            naming,
            waiting_for,
        }) => {
            // 红绿灯这一跳也要看得见：「全绿」既可能是都在忙，也可能是 status 一条都没到。
            tracing::info!(
                "session-status: [{host_label}] sid={sid} activity={activity:?} \
                 waiting_for={waiting_for:?} → 成品交出口"
            );
            crate::session_book::feed(BookIn::Status {
                origin: host_label.clone(),
                sid,
                activity,
                naming,
                waiting_for,
            });
        }
        Some(InboundFrame::SessionRuns { sid, runs, ended }) => {
            crate::session_book::feed(BookIn::Runs {
                origin: host_label.clone(),
                sid,
                runs,
                ended,
            });
        }
        Some(InboundFrame::SessionBranch {
            sid,
            path,
            at,
            off,
            sum,
        }) => {
            crate::session_book::feed(BookIn::Branch {
                origin: crate::origin::Origin(host_label.clone()),
                sid,
                path,
                at,
                off,
                sum,
            });
        }
        Some(InboundFrame::SessionRemoved { sid }) => {
            // 只剩内容流的边界：残批已在上面冲掉；这里摘排队中的快照 ＋ 给在途的打取消标记（归档后迟到的快照行会经「见行复活」造出僵尸 tab）、续点作废。
            // 它离开之后是什么由下一帧 `session_state` 说。
            tracing::info!(
                "session-removed: [{host_label}] sid={sid}（内容流收口；去向看 session_state）"
            );
            intake.removed(&sid);
        }
        // 后端裁好的去向（可重连 / 已结束）原样交出口（残批已在上面冲掉）。
        Some(InboundFrame::SessionState { sid, state, words }) => {
            tracing::info!("session-state: [{host_label}] sid={sid} → {state:?}");
            crate::session_book::feed(BookIn::Left {
                origin: host_label.clone(),
                sid,
                fate: state,
                words: Some(words),
            });
        }
        Some(InboundFrame::Overflow {
            dropped,
            lost,
            lost_truncated,
        }) => on_overflow(&host_label, health, dropped, &lost, lost_truncated),
        // 那台的某样东西变了 ⇒ 订了这台 `changed/<topic>` 的订阅收一格（格体原样；账号表 · 设置页 · 任务面板 · 额度 · 计划据此重读）。
        Some(InboundFrame::Changed { topic, cell }) => {
            replay.changed(&crate::origin::Origin(host_label.clone()), &topic, cell);
        }
        // 那台一条活会话的记录文件不见了 / 被改过已从头重读 ⇒ 残批先冲、再交那个会话的内容流一格。
        Some(InboundFrame::SessionFileNotice { sid, path, change }) => {
            intake.notice(&sid, &path, change).await;
        }
        // 那台的活会话清单报完了（后端把它压到第一份 tmux 快照之后、那一份推出的可重连之后才放）。
        //   与上面的宣告同一条流、同序交出口 ⇒ 前端收到它时，这台全部的活会话与可重连会话都已报过。
        Some(InboundFrame::SessionsReplayed) => {
            tracing::info!("sessions-replayed: [{host_label}] 活会话清单报完了 → 成品交出口");
            crate::session_book::feed(BookIn::Listed {
                origin: host_label.clone(),
            });
        }
        // hello 只在接上那一刻来（循环那一侧握手时读走、交 `Attached`）；应答由循环交回请求方。流中间再来 ⇒ 协议对不上，照实说、丢掉。
        Some(
            InboundFrame::Hello { .. }
            | InboundFrame::Reply { .. }
            | InboundFrame::Cancelled { .. },
        ) => {
            tracing::warn!("stream_source [{host_label}] 流中间来了 hello / 应答帧，丢掉：{line}");
        }
        // 链路帧只该出现在**本机后端**那条流上（monitor 只在那里开链路）。
        // 远端后端发来 ⇒ 协议对不上，照实说、丢掉。
        Some(InboundFrame::LinkData { link, .. } | InboundFrame::LinkEnd { link, .. }) => {
            tracing::warn!(
                "stream_source [{host_label}] 远端后端发来了链路帧（link={link}）—— monitor 没在远端开过链路，丢掉"
            );
        }
        // 传输帧同理：传输台住**本机**后端，远端后端发来 ⇒ 协议对不上，照实说、丢掉。
        Some(InboundFrame::Transfer { id, .. }) => {
            tracing::warn!(
                "stream_source [{host_label}] 远端后端发来了传输帧（id={id}）—— 传输台在本机后端，丢掉"
            );
        }
        // 测试连接的进度帧同理：测试连接在**本机**后端里跑，远端后端发来 ⇒ 协议对不上，照实说、丢掉。
        Some(InboundFrame::Probe { ticket, .. }) => {
            tracing::warn!(
                "stream_source [{host_label}] 远端后端发来了测试连接的进度帧（ticket={ticket}）—— 测试连接在本机后端，丢掉"
            );
        }
        // 远端中转住进远端常驻后端（进程内），它抄出来的 SSE 事件沿这条流回来 ⇒ 与本机那条流同一个口转前端
        //   （origin = 这台；标签就是 claude 自己的 sid，不用对账）。从不阻塞、不进内容通道。
        Some(InboundFrame::LiveCard(l)) => crate::live_card::deliver(&host_label, l),
        // 终端实时预览：这台推来的一屏 / 收尾 ⇒ 交订了这台 `terminal-screen/<票>` 的那条订阅（从不阻塞）。
        Some(
            InboundFrame::TerminalScreen { ticket, cell }
            | InboundFrame::TerminalFollowEnd { ticket, cell },
        ) => crate::terminal_screen_relay::deliver(
            &crate::origin::Origin(host_label.clone()),
            &ticket,
            cell,
        ),
        // 一轮结束：在不在看都来 ⇒ 交这台的会话流一格（系统通知认它）。
        Some(InboundFrame::TurnEnd(turn)) => {
            replay.on_turn_end(&crate::origin::Origin(host_label.clone()), turn)
        }
        // 不认识的种类 / 形状不对：`take` 已记账、每种说过一次；跳过，绝不中断流。
        None => {}
    }
}

/// 这一条连接定下来、hello 那一臂要用的几样。
#[derive(Clone, Copy)]
struct Round<'a> {
    replay: &'a Arc<EventReplay>,
    health: &'a HealthOut,
    /// 接上那一刻本机常驻后端答的「那台比手上这一版旧」—— 版本提示那句话按它挑。
    remote_older: bool,
    /// 这一轮连接的起点（`[perf]` 埋点从这里算）。
    t_connect_start: std::time::Instant,
}

/// hello 帧里要读的那几样。
struct HelloSeen {
    v: u64,
    build_id: String,
    host_arch: String,
    /// 解析后的 Claude home（[`claude_home_from_hello`]）。
    claude_home: String,
    homes: Vec<AgentHome>,
    commands: Vec<String>,
}

/// hello 那一臂：打日志、记账、版本提示、自证记忆。
fn on_hello(host_label: String, round: Round, hello: HelloSeen) {
    let Round {
        replay,
        health,
        remote_older,
        t_connect_start,
    } = round;
    let HelloSeen {
        v,
        build_id,
        host_arch,
        claude_home,
        homes,
        commands,
    } = hello;
    tracing::info!(
        "stream_source backend hello: v={v} build_id={build_id} host_arch={host_arch} claude_home={claude_home} homes={homes:?} cmds={commands:?}"
    );
    // 订了这台会话流的那些订阅原位收一格 `Seen`（`Item::Seen`）。
    replay.origin_seen(&crate::origin::Origin(host_label.clone()), true);
    // issue #33：版本协商。不兼容/偏旧经 SS-F remote-health 通道醒目提示；类别（要更新 · 较新 · 不可比）
    // 这里判好（`version_health_kind`），前端按类别挑标题。不 hard-disconnect（向前兼容）。
    // 手上没带后端字节（「我这一版」是 `None`）⇒ 同一条通道说一句「版本不可比」，照常接。
    let mine = crate::byte_table::my_backend_id();
    let rel = version_relation(v, &build_id, remote_older, mine);
    crate::machine_state::up(&host_label, &build_id, rel);
    if let (Some(kind), Some(msg)) = (
        version_health_kind(rel),
        version_warning(v, &build_id, &host_label, remote_older, mine),
    ) {
        tracing::warn!("stream_source remote [{host_label}] version: {msg}");
        let payload = crate::ui_contract::RemoteHealthPayload {
            origin: host_label.clone(),
            kind: kind.to_string(),
            message: msg,
            detail: String::new(),
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
    if mine == Some(build_id.as_str()) {
        record_verified_build(&host_label, &build_id);
    } else {
        forget_verified_build(&host_label);
    }
}

/// 新宣告一个活会话：成品（元信息 ＋ 初始灯 ＋ 容器）原样交 `session_book`（本机那条流同一个口）。旁路快照等它读完那一帧（`session_primed`）再排。
fn on_session_added(host_label: &str, sid: String, meta: LiveMeta) {
    // ★★ 这一跳要看得见（「帧到 monitor 了吗」）；每个会话一次，不淹日志。
    tracing::info!("session-added: [{host_label}] sid={sid} → 成品交出口");
    crate::session_book::feed(BookIn::Live {
        origin: host_label.to_string(),
        sid: sid.clone(),
        meta,
    });
}

/// 那台管道拥塞丢了帧：日志 ＋ 经 `remote-health` 告诉用户（丢了的不可恢复帧点名）。
fn on_overflow(
    host_label: &str,
    health: &HealthOut,
    dropped: u64,
    lost: &[LostFrameInfo],
    lost_truncated: bool,
) {
    // issue #32：远端管道拥塞丢了 dropped 帧。warn + 经 SS-F remote-health
    // 通道提示用户（前端按 origin 节流弹 toast）。
    //
    // ★**这里此前对用户说了一句假话**：「重开该会话可看完整
    // 历史」只对**内容帧**成立。状态增量帧（session_added/session_removed/
    // tmux_session_closed/session_status）是一次差分的结果、**别处不存在**，
    // 重开会话补不回来 —— 那正是 B-3 的正题。backend 从 `p1x` 起会把这些帧的
    // 身份放进 `Overflow.lost`；有身份就说实话，并点名是哪几个会话。
    tracing::warn!(
        "stream_source remote [{host_label}] overflow: backend dropped {dropped} frame(s), \
         {} unrecoverable{}",
        lost.len(),
        if lost_truncated {
            " (list truncated)"
        } else {
            ""
        }
    );
    let message = overflow_health_message(host_label, dropped, lost, lost_truncated);
    let payload = crate::ui_contract::RemoteHealthPayload {
        origin: host_label.to_string(),
        kind: "overflow".to_string(),
        message,
        detail: String::new(),
    };
    if let Err(e) = health(payload) {
        tracing::warn!("stream_source remote-health emit failed: {e}");
    }
}

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

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/frame_dispatch_shape.rs"]
mod frame_dispatch_shape;
