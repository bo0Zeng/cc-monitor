//! 远端流主循环：重连、起流、逐帧分派。

use super::*;
use crate::copy_table::copy_text;
use crate::event_replay::EventReplay;
use crate::session_book::{In as BookIn, LiveMeta};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::BufReader;

/// `remote-health` 的出口：宿主（`lib.rs::remote_health_out`）拿窗口把手造它，本模块只调它、不认识 GUI 宿主（`backend_client_guard_tests.rs::GUARDED`）。
/// 回 `Err(原话)` ＝ 没发出去，各发射点那句 warn 照旧。
pub(crate) type HealthOut =
    Arc<dyn Fn(crate::ui_contract::RemoteHealthPayload) -> Result<(), String> + Send + Sync>;

/// 握手完成 ⇒ 写半边解冻。见证只能由一帧真的 Hello 换出来（`BackendHello::from_hello_frame`）⇒ 「hello 之前不许写」在 monitor 侧是类型上的事实。
/// 第二次 hello（不该有）时 `parked` 已被 `take` 走，静默跳过。抽成函数是为了可测：埋在 `stream_loop` 中段时没有任何判据碰得到它。
fn attach_inbound_client<W>(
    host_label: &str,
    parked: &mut Option<crate::inbound_client::ParkedWriter<W>>,
    frame: Option<&InboundFrame>,
) -> Option<std::sync::Arc<crate::inbound_client::InboundClient>>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let witness = crate::inbound_client::BackendHello::from_hello_frame(frame?)?;
    let client = parked.take()?.into_client(witness);
    crate::inbound_client::register(host_label, client.clone());
    Some(client)
}

/// 把一帧入方向应答路由回请求方。返回是否真的交到了某个等待者手上。
/// 没有客户端 = backend 在 hello 之前就回了应答（协议倒错），照实报、不静默。抽成函数同样是为了可测。
fn route_inbound_frame(
    host_label: &str,
    client: Option<&std::sync::Arc<crate::inbound_client::InboundClient>>,
    frame: InboundFrame,
) -> bool {
    let (kind, id) = match &frame {
        InboundFrame::Reply { id, .. } => ("reply", id.clone()),
        InboundFrame::Cancelled { id } => ("cancelled", id.clone()),
        other => {
            tracing::warn!("route_inbound_frame 收到非入方向帧，忽略：{other:?}");
            return false;
        }
    };
    let Some(c) = client else {
        tracing::warn!(
            "stream_source [{host_label}] 收到 {kind}(id={id})，但本连接还没有入方向客户端 —— backend 在 hello 之前就回应答了？"
        );
        return false;
    };
    match frame {
        InboundFrame::Reply {
            id,
            ok,
            code,
            message,
            detail,
            data,
        } => c.route_reply(&id, ok, code, message, detail, data),
        InboundFrame::Cancelled { id } => c.route_cancelled(&id),
        _ => false,
    }
}

/// `stream_loop` 那条接缝的判据：`inbound_client` 的单测走自造客户端、e2e 走真后端二进制，两者之间的接缝
/// （hello 臂解冻写半边并登记 · `reply` 臂路由应答 · 控制通道往返的探针真发字节）只有这里判。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/seam_tests.rs"]
mod seam_tests;

/// 写半边只许经 `inbound_client::park` 出手：在这里直接 `write_all` 一行会静默绕过 `ParkedWriter` 那层「Hello 之前不许写」的类型保证。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/write_half_guard.rs"]
mod write_half_guard;

/// SSH-remote 数据源主循环。
///
/// 连接远端、exec backend、把 backend stdout 的 line-delimited JSON 帧逐行解析后分发：
/// - `hello` → log（证明 backend runtime 起来了）+ 置 `connected`（标记本次连接已健康，供重连循环判定是否重置退避）。
/// - `line` → 组 [`JsonlLine`] 交 [`LineIntake`]（本机那条流用的是同一个）：攒批后 `crate::batch_to_payloads(...)` →
///   `replay.on_line_batch_awaited(&app, ...)`（前端按 seq 自动排序）。
/// - 会话起停的成品（`session_added` · `session_status` · `session_state` · `sessions_replayed`）→ 原样交 `session_book::feed`
///   （后端裁、monitor 只转交；本机那条流同一个口）；连接断了 ⇒ `session_book::In::LinkLost`。
/// - 未知 kind / garbage → `tracing::warn!` 跳过，绝不中断流。
///
/// stdout EOF / 读错误 → 返回 `Err`，调用方据此大声报「connection dropped」，不静默冻结。
pub async fn run(
    cfg: RemoteConfig,
    replay: Arc<EventReplay>,
    health: HealthOut,
    connected: Arc<AtomicBool>,
) -> Result<(), String> {
    tracing::info!(
        "stream_source connecting to {}@{}:{}",
        cfg.user,
        cfg.host,
        cfg.port
    );

    // 重连循环：每轮跑一次 stream_loop。失败 / 掉线后按指数退避（2→4→8→16→30s 封顶）重连；本轮站住了则下次以 MIN 快速重连。
    // 唯一的等待是 tokio::time::sleep（async、非阻塞），绝不 std::thread::sleep（INVARIANT §10）。
    let mut backoff = RECONNECT_MIN;
    // hello 自愈账本：存上一轮 backend 自报的能力 token 集。None = 尚未收到能力声明；Some(caps) = 下一轮据此发 flag 升级。
    // 带 flag 的一轮连 hello 都没收到（旧后端把未知参数当一次性查询退出）⇒ 清账回退降级，防止 flagged 重连死循环。
    let mut hello_confirmed: Option<Vec<String>> = None;
    // 这台是不是「永久不支持」（非 unix）：`stream_loop` 接不上常驻时写，本循环读完即清。
    let mut unsupported: Option<String> = None;
    let mut unsupported_code: Option<&'static str> = None;
    loop {
        connected.store(false, Ordering::Release);
        crate::machine_state::connecting(&cfg.origin_label(), "deploy");
        // 本轮连接的起点。退避重置的判据是「活过多久」，不是「握没握上手」。
        let conn_started = std::time::Instant::now();
        let result = stream_loop(
            &cfg,
            &replay,
            &health,
            &connected,
            &mut hello_confirmed,
            &mut unsupported,
            &mut unsupported_code,
        )
        .await;
        // 这条连接没了 ⇒ 订了这台会话流的那些订阅原位收一格 `Unseen`（不是终点）。
        replay.origin_seen(&crate::origin::Origin(cfg.origin_label()), false);
        // 这一轮一次都没握上手 ⇒ 那台记成「离线」（文件窗口那一条由「重新连接中…」换成「离线 · 采样 …」＋［重新连接］）。
        if !connected.load(Ordering::Acquire) {
            crate::inbound_client::note_round_failed(&cfg.origin_label());
            match unsupported_code.take() {
                Some(code) => crate::machine_state::unsupported(&cfg.origin_label(), code),
                None => crate::machine_state::down(&cfg.origin_label()),
            }
        }
        if hello_confirmed.is_some() && !connected.load(Ordering::Acquire) {
            tracing::warn!(
                "stream_source hello 自愈轮未收到 hello,回退降级模式(backend 可能被换旧)"
            );
            hello_confirmed = None;
        }
        // 连接断了 ≠ 会话死了：这台的成品整份作废，当时活的 / 可重连的一律「说不清」；
        //   重连之后那台的新连接自己重报一遍（它的账本从 tmux 推出可重连，`observe/session_ledger.rs`）。
        tracing::info!(
            "stream_source [{}] connection ended ⇒ 这台的会话成品作废（说不清）",
            cfg.origin_label()
        );
        crate::session_book::feed(BookIn::LinkLost {
            origin: cfg.origin_label(),
        });
        match &result {
            Ok(()) => tracing::warn!("stream_source stream returned Ok unexpectedly; reconnecting"),
            Err(e) => tracing::warn!("stream_source remote source ended: {e}"),
        }
        // 两段式（非冗余）：先按当前 backoff 睡，再在仍没连上时翻倍 ⇒ 首次失败只等 MIN，退避序列是 2→4→8→16→30。
        // sleep 期间 `connected` 不会变（其唯一写者 stream_loop 已返回）。「连上过」不等于「站住了」：判据是这条连接活过 MIN_HEALTHY_UPTIME ——
        // hello-then-die 的后端若每轮都算连上过，退避永远重置回 2s，SSH 握手会一直砸在那台撑不住的机器上。
        if should_reset_backoff(connected.load(Ordering::Acquire), conn_started.elapsed()) {
            backoff = RECONNECT_MIN; // 本次真站住过 → 下次立即快速重连
        }
        let wait = match after_round(unsupported.take(), backoff) {
            AfterRound::RetryIn(d) => d,
            AfterRound::Stop(why) => {
                // 出声（界面 toast），然后这条流就此收工；机器页「起」会重起一条、再试一次。
                let payload = crate::ui_contract::RemoteHealthPayload {
                    origin: cfg.origin_label(),
                    kind: "unsupported".to_string(),
                    message: why.clone(),
                    detail: String::new(),
                };
                if let Err(e) = health(payload) {
                    tracing::warn!("stream_source remote-health (unsupported) emit failed: {e}");
                }
                return Err(why);
            }
        };
        tracing::info!("stream_source reconnecting in {:?}", wait);
        // 「重新连接」（文件窗口那一条上的按钮，经通道 `link-retry`）⇒ 不等退避睡满，当场再连一轮、退避从头算。
        let kick = crate::inbound_client::kick_handle(&cfg.origin_label());
        let kicked = tokio::select! {
            () = tokio::time::sleep(wait) => false,
            () = kick.notified() => true,
        };
        if kicked {
            tracing::info!(
                "stream_source [{}] 重新连接：不等退避，立刻重拨",
                cfg.origin_label()
            );
            backoff = RECONNECT_MIN;
        } else if !connected.load(Ordering::Acquire) {
            backoff = next_backoff(backoff); // 仍没连上 → 指数退避增长
        }
    }
}

/// [`run`] 的内层流循环：connect → exec backend → 逐帧 dispatch。**所有**提前返回
/// （`?` / EOF / 读错误）都把 result 冒泡给 [`run`]，由后者统一说「这台的成品作废」，故本函数自身不管断连。
async fn stream_loop(
    cfg: &RemoteConfig,
    replay: &Arc<EventReplay>,
    health: &HealthOut,
    connected: &Arc<AtomicBool>,
    hello_confirmed: &mut Option<Vec<String>>,
    unsupported: &mut Option<String>,
    unsupported_code: &mut Option<&'static str>,
) -> Result<(), String> {
    // issue #15 / #30：远端行的 origin 标签 = 该机器的稳定身份（label，默认 host）。
    // 前端据此给该 Tab 标题加 `[label]` 前缀以区分本地/各远端机器。进 loop 前 clone。
    let host_label = cfg.origin_label();

    let Opened {
        stream,
        with_bg,
        tail_only,
        t_connect_start,
    } = open_round(cfg, health, hello_confirmed, unsupported, unsupported_code).await?;

    // 这条 channel 是双工的：`split_and_park` 一步切开并把写半边停住 —— `ParkedWriter` 身上没有任何写方法，收到 hello 才换得出能发命令的客户端。
    // 切与停必须是同一步：中间留一个裸 `WriteHalf` 就是一个「Hello 之前能写」的窗口（见 `inbound_client` 头注）。
    // 接上那一刻本机常驻后端答的「那台比手上这一版旧」—— 版本提示那句话按它挑。
    let remote_older = stream.remote_is_older();
    let (stream, parked) = crate::inbound_client::split_and_park(stream);
    let mut parked = Some(parked);
    // 本连接的入方向客户端（收到 hello 后才有）。函数任何退出路径经 guard 摘除注册表
    // 并叫醒还在等应答的调用方 —— 同 `SnapshotQueueCloser` 的形状。
    let mut inbound: Option<std::sync::Arc<crate::inbound_client::InboundClient>> = None;
    struct InboundCloser(
        String,
        Option<std::sync::Arc<crate::inbound_client::InboundClient>>,
    );
    impl Drop for InboundCloser {
        fn drop(&mut self) {
            if let Some(c) = self.1.take() {
                crate::inbound_client::unregister(&self.0, &c);
            }
        }
    }
    let mut inbound_guard = InboundCloser(host_label.clone(), None);
    let round = Round {
        replay,
        health,
        connected,
        with_bg,
        tail_only,
        remote_older,
        t_connect_start,
    };

    // 攒批 ＋ 静默窗 ＋ 旁路快照收成 [`LineIntake`]（本机那条流用的是同一个）；每连接一套，函数任何退出路径随 `intake` 被丢掉而关闭队列
    // （已入队项仍会被分发器拉完）。
    let mut intake = LineIntake::open(host_label.clone(), tail_only, replay, health);
    // 这条流上跳过了几帧认不出的（读任务那边另有一本记非 UTF-8 行）。
    let mut tally = crate::frame_tally::FrameTally::new(format!("stream_source {host_label}"));
    // 解不出来的帧每种说一次（日志 ＋ 这台的健康信息）。
    let mut unread = UnreadNotes::new(host_label.clone(), host_label.clone());
    let say_health = |p: crate::ui_contract::RemoteHealthPayload| {
        if let Err(e) = health(p) {
            tracing::warn!("stream_source remote-health (frame) emit failed: {e}");
        }
    };

    let mut frame_rx = spawn_frame_reader(stream, host_label.clone());

    loop {
        // pending 非空 → 带静默窗口收帧：窗口内没有新帧就先 flush 再回到阻塞收（`LineIntake::recv_or_flush`）。
        let Some(msg) = intake.recv_or_flush(&mut frame_rx).await else {
            // reader task 没投 Err 就消失（理论不可达）——同样明确报错走重连。
            intake.flush().await;
            return Err("ssh backend frame channel closed".to_string());
        };
        let line = match msg {
            Ok(Some(l)) => l,
            Ok(None) => {
                intake.lost().await;
                continue;
            }
            Err(e) => {
                // EOF / 读错：flush 残余并等它发完再报错 —— run() 随后的断连归档必须晚于这些行到达前端。
                intake.flush().await;
                return Err(e);
            }
        };
        let line = line.as_str();

        let frame = unread.take(line, &mut tally, &say_health);
        // SessionRemoved 是唯一顺序敏感的攒批边界：它的行必须先落前端，否则归档后迟到的行把 Tab 复活成僵尸 live。
        // SessionAdded / Hello / Overflow / 坏帧不作边界（多个小会话的 snapshot 才能聚成大批；行先于 Added 到达无妨：前端 ensureTab 见行即建）。
        // `session_state`（可重连 / 已结束的成品）同理：它说的「离开了」必须排在这个会话的行之后。
        if matches!(
            frame,
            Some(InboundFrame::SessionRemoved { .. } | InboundFrame::SessionState { .. })
        ) {
            intake.flush().await;
        }

        // 握手完成 ⇒ 写半边解冻。放在 match 之前：Hello 那条臂按值解构了帧。
        if let Some(client) = attach_inbound_client(&host_label, &mut parked, frame.as_ref()) {
            inbound_guard.1 = Some(client.clone());
            inbound = Some(client);
            // 「这台的长连接能问话了」由下面 Hello 臂里的 `replay.origin_seen(.., true)` 说（订了这台 `accounts-changed` 的订阅原位收 `Seen`，`event_replay` 头注那张表）。
            // 连上那一刻：让本机常驻后端沿池里那条 SSH 同步资产目录（后台跑，零判定）。
            let accepts = inbound
                .as_ref()
                .is_some_and(|c| c.accepts(crate::asset_sync::REMOTE_NEEDS));
            crate::asset_sync::on_remote_ready(cfg, accepts);
        }

        match frame {
            Some(InboundFrame::Hello {
                v,
                build_id,
                host_arch,
                claude_dir,
                homes,
                capabilities,
                commands,
                ..
            }) => on_hello(
                host_label.clone(),
                round,
                hello_confirmed,
                HelloSeen {
                    v,
                    build_id,
                    host_arch,
                    // 日志报的是解析后的 Claude home（优先 `homes`、回退 `claude_dir`），同时把原样的 `homes` 一起打出来：排障时一眼看出这台后端发没发新字段。
                    claude_home: claude_home_from_hello(&homes, &claude_dir).to_string(),
                    homes,
                    capabilities,
                    commands,
                },
            )?,
            Some(InboundFrame::Line {
                session_id,
                path,
                seq,
                message,
                cwd,
                end,
                rid,
            }) => {
                // 进攒批缓冲（达 cap / 批龄立即整批出）；静默窗口 / SessionRemoved 边界触发的 flush 在循环头。
                intake
                    .line(JsonlLine {
                        session_id,
                        path: std::path::PathBuf::from(path),
                        seq,
                        message,
                        cwd,
                        end: Some(end),
                        rid,
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
                path,
                lines,
                activity,
                activity_text,
                activity_tone,
                waiting_for,
                container,
                // pid 只给本机那条流用（本机 ↗ 绑窗口）；远端这一支不读。
                pid: _,
            }) => on_session_added(
                &host_label,
                &intake,
                sid,
                LiveMeta {
                    background,
                    attachable,
                    cwd,
                    project_dir,
                    name,
                    activity,
                    activity_text,
                    activity_tone,
                    waiting_for,
                    container,
                    pid: None,
                },
                path,
                lines,
            ),
            Some(InboundFrame::SessionStatus {
                sid,
                activity,
                activity_text,
                activity_tone,
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
                    activity_text,
                    activity_tone,
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
            Some(InboundFrame::SessionRemoved { sid }) => {
                // 只剩内容流的边界：残批已在循环头冲掉；这里摘排队中的快照 ＋ 给在途的打取消标记（归档后迟到的快照行会经「见行复活」造出僵尸 tab）、续点作废。
                // 它离开之后是什么由下一帧 `session_state` 说。
                tracing::info!(
                    "session-removed: [{host_label}] sid={sid}（内容流收口；去向看 session_state）"
                );
                intake.removed(&sid);
            }
            // 后端裁好的去向（可重连 / 已结束）原样交出口（残批已在循环头冲掉）。
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
            // 入方向应答 —— 交给本连接的客户端按 `id` 路由回请求方。
            Some(f @ (InboundFrame::Reply { .. } | InboundFrame::Cancelled { .. })) => {
                route_inbound_frame(&host_label, inbound.as_ref(), f);
            }
            // 那台的账号清单变了 ⇒ 经通道 `subscribe`：订了这台 `accounts-changed` 的订阅收一格 `Frame`（账号表与 chip 据此重取）。
            Some(InboundFrame::AccountsChanged) => {
                replay.accounts_changed(&crate::origin::Origin(host_label.clone()));
            }
            // 那台的配置文件变了 ⇒ 订了这台 `profiles-changed` 的订阅收一格（设置窗「别名与配置文件」那一页据此重读）。
            Some(InboundFrame::ProfilesChanged) => {
                replay.profiles_changed(&crate::origin::Origin(host_label.clone()));
            }
            // 某个会话的任务清单变了 ⇒ 订了这台 `session-tasks` 的订阅收一格 `{sid}`。
            Some(InboundFrame::TasksChanged { sid }) => {
                replay.tasks_changed(&crate::origin::Origin(host_label.clone()), &sid);
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
            Some(InboundFrame::Tap(t)) => crate::session_tap::deliver(&host_label, t),
            // 那台的额度账 / 某个会话的轮换变了 ⇒ 订了这台 `quota-changed` 的订阅收一格（`{"quota":true}` / `{sid}`）。
            Some(InboundFrame::QuotaChanged) => {
                replay.quota_changed(&crate::origin::Origin(host_label.clone()), None);
            }
            Some(InboundFrame::RotationChanged { sid }) => {
                replay.quota_changed(&crate::origin::Origin(host_label.clone()), Some(&sid));
            }
            Some(InboundFrame::RotationRulesChanged) => {
                replay.rotation_rules_changed(&crate::origin::Origin(host_label.clone()));
            }
            // 终端实时预览：这台推来的一屏 / 收尾 ⇒ 交订了这台 `terminal-screen/<票>` 的那条订阅（从不阻塞）。
            Some(
                InboundFrame::TerminalScreen { ticket, cell }
                | InboundFrame::TerminalFollowEnd { ticket, cell },
            ) => crate::terminal_screen_relay::deliver(
                &crate::origin::Origin(host_label.clone()),
                &ticket,
                cell,
            ),
            // 认识但不消费（理由在变体上）。
            Some(InboundFrame::TurnEnd) => {}
            // 不认识的种类 / 形状不对：`take` 已记账、每种说过一次；跳过，绝不中断流。
            None => {}
        }
    }
}

/// 本轮起流的结果：接上的那条流、本轮发的流模式 flag、本轮连接的起点。
struct Opened {
    stream: crate::remote_resident::Replayed,
    with_bg: bool,
    tail_only: bool,
    t_connect_start: std::time::Instant,
}

/// 起流：部署预检（这台自证过就是期望 build 则跳过）→ 按能力定流模式 flag → 接那台的常驻后端。
/// 起不来 ⇒ `Err`（非 unix 另记进 `unsupported`，[`run`] 据此停下）。
async fn open_round(
    cfg: &RemoteConfig,
    health: &HealthOut,
    hello_confirmed: &Option<Vec<String>>,
    unsupported: &mut Option<String>,
    unsupported_code: &mut Option<&'static str>,
) -> Result<Opened, String> {
    let host_label = cfg.origin_label();

    // 冷启动最贵的三段（用户点开应用到看见远端会话之间唯一的那条链）埋 `[perf]`：不改任何行为，只让下一次讨论有数可依。
    let t_connect_start = std::time::Instant::now();

    // 连接前确保远端后端已部署到固定落点（`~/.cc-monitor/bin/ccm`）；这一版没带字节 ⇒ `byte_table::choose` 回「这一版没带」→ 优雅 no-op。
    // best-effort：部署失败不阻断（手动部署的后端仍可连）。
    // 上一次这台机器的后端自报过就是期望 build ⇒ 跳过预检那两条连接（`VERIFIED_BUILD` 头注：记的是 hello 自证，不是预检结论）。
    // 跳过时 `confirmed_build` 直接给「我这一版」—— 给 `None` 会让下面的 caps 阶梯掉进空集全降级（省两条连接换来一轮降级 + 一轮升级重连）。
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
                // 不阻断（手动部署的后端照样能连），但那句话要到界面上。
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

    // 流模式门控：从后端声明的能力 token 决定发哪些 flag。旧后端会把未知参数当一次性查询处理后退出（无 hello → 重连死循环，§26），
    // 故只对声明了对应能力的后端发 flag。能力两条来源，hello 自愈账本优先：
    // ① `hello_confirmed`（上一轮 backend 自报的能力）—— 最权威，收过真 hello 才有。
    // ② 否则部署侧确认了当前内嵌 build（`confirmed_build == 我这一版`）→ 用内嵌后端的能力常量预知，省第一轮往返（乐观路径）。
    // ③ 都没有 → 空集 → 全降级（连接正常、功能退化）。
    // hello 优先于部署侧：②可能是陈旧内嵌的身份 ≠ 期望 → 空集 → 靠 hello 自愈救；`hello_confirmed` 只在收到真声明时写入，优先采纳恒安全。
    let caps: Vec<String> = hello_confirmed.clone().unwrap_or_else(|| {
        if mine.is_some() && confirmed_build.as_deref() == mine {
            embedded_backend_capabilities()
        } else {
            Vec::new()
        }
    });
    let (with_bg, tail_only) = decide_stream_flags(&caps, crate::load_show_bg_sessions());
    let t_exec = std::time::Instant::now();
    // 起流失败就抹掉自证记忆 —— 否则一台后端被删 / 被换旧的机器会每一轮都跳预检、每一轮都失败。代价是多一次重连（`VERIFIED_BUILD` 头注）。
    // 接那台的常驻后端（没有就起一个；与本机同形）。起不了常驻（非 unix / 太旧）就是一次失败、说清为什么，不回落到随 SSH 生死的流模式。
    let flags = (with_bg, tail_only);
    crate::machine_state::connecting(&host_label, "attach");
    let stream: crate::remote_resident::Replayed = match crate::remote_resident::attach(cfg, flags)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            // 非 unix ⇒ 记进这台的连接状态（`run` 据此停下，不再按退避重连）。
            if let crate::remote_resident::AttachErr::Unsupported(why, code) = &e {
                *unsupported = Some(why.said.clone());
                *unsupported_code = Some(code);
            }
            let said = e.said();
            // 这一轮没成的那一句与详情进那台的状态成品（收尾时机器那一行的［复制详情］跟它）。
            crate::machine_state::round_failed(&crate::origin::Origin(host_label.clone()), &said);
            let e = format!("{said}: {}", said.raw());
            if skip_preflight {
                tracing::warn!(
                    "stream_source [{host_label}] 跳过预检后起流失败，抹掉自证记忆，下一轮重新预检: {e}"
                );
                forget_verified_build(&host_label);
            }
            return Err(e);
        }
    };
    tracing::info!(
        "[perf] stream_source [{host_label}] 起流 {}ms（SSH 登录 + exec backend；\
         with_bg={with_bg} tail_only={tail_only}）\
         · 自本轮连接开始 T+{}ms",
        t_exec.elapsed().as_millis(),
        t_connect_start.elapsed().as_millis()
    );
    Ok(Opened {
        stream,
        with_bg,
        tail_only,
        t_connect_start,
    })
}

/// 帧读取在独立 task、经 channel 交回：攒批需要「带静默窗口的读」，而 tokio 的 read_line 不是 cancellation-safe（timeout 取消会丢 buffer 里的半帧）；
/// mpsc::Receiver::recv 是 cancel-safe 的。reader task 在 EOF / 读错时投递 Err 后退出；本函数返回时 rx drop → task 的 send 失败 → 自然退出，不泄漏。
/// `Ok(None)`：这里有一行超长、整行丢了 ⇒ 主循环原位给订阅一格 `Gap`（`to_seq` 缺）。
fn spawn_frame_reader<R>(
    stream: R,
    reader_host: String,
) -> tokio::sync::mpsc::Receiver<Result<Option<String>, String>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let (frame_tx, frame_rx) = tokio::sync::mpsc::channel::<Result<Option<String>, String>>(1024);
    tauri::async_runtime::spawn(async move {
        let mut reader = BufReader::new(stream);
        // 按 `\n` 切（协议保证每帧一行、帧内换行已被后端转义，见 src/backend/stream/wire.rs）。用 [`read_capped_line`]：对端是远端进程，
        // 「一条永远不结束的行」就是无界堆分配。
        let mut buf: Vec<u8> = Vec::new();
        // 这条流上有几行不是合法 UTF-8（按替换字符读的）—— 计数、按 2 的幂次说、流结束出总账。
        let mut tally = crate::frame_tally::FrameTally::new(format!("stream_source {reader_host}"));
        loop {
            match read_capped_line(&mut reader, &mut buf, BACKEND_FRAME_LINE_CAP).await {
                Ok(CappedLine::Eof) => {
                    // EOF：backend 退出 / channel 关闭。明确报错，不静默冻结。
                    let _ = frame_tx
                        .send(Err(
                            "ssh backend stdout closed (EOF / connection dropped)".to_string()
                        ))
                        .await;
                    break;
                }
                Ok(CappedLine::TooLong(bytes)) => {
                    // 超限 = 丢弃 + 原位说出来，绝不静默。不走 Err 臂（那会被当成致命错误去重连，而坏的只是这一行）：与行同一条路交 `Ok(None)`，主循环原位给订阅一格 `Gap`。
                    tracing::warn!(
                        "stream_source remote [{reader_host}] line too long: {bytes} bytes \
                         (cap {BACKEND_FRAME_LINE_CAP}); line dropped"
                    );
                    if frame_tx.send(Ok(None)).await.is_err() {
                        break;
                    }
                }
                Ok(CappedLine::Line) => {
                    // 非 UTF-8 不该让整条连接死掉 —— 但记账、说出来。
                    if std::str::from_utf8(&buf).is_err() {
                        if let Some(n) = tally.note_bad_utf8(&buf) {
                            tracing::warn!("{n}");
                        }
                    }
                    let text = String::from_utf8_lossy(&buf);
                    let line = text.trim_end_matches(['\n', '\r']);
                    if line.is_empty() {
                        continue;
                    }
                    if frame_tx.send(Ok(Some(line.to_string()))).await.is_err() {
                        break; // 主循环已退出（重连中）
                    }
                }
                Err(e) => {
                    let _ = frame_tx
                        .send(Err(format!("ssh backend stdout read error: {e}")))
                        .await;
                    break;
                }
            }
        }
    });
    frame_rx
}

/// 本轮连接定下来、hello 那一臂要用的几样（起流之后就不变了）。
#[derive(Clone, Copy)]
struct Round<'a> {
    replay: &'a Arc<EventReplay>,
    health: &'a HealthOut,
    connected: &'a Arc<AtomicBool>,
    /// 本轮实际发的流模式 flag。
    with_bg: bool,
    tail_only: bool,
    /// 接上那一刻本机常驻后端答的「那台比手上这一版旧」—— 版本提示那句话按它挑。
    remote_older: bool,
    /// 本轮连接的起点（`[perf]` 埋点从这里算）。
    t_connect_start: std::time::Instant,
}

/// hello 帧里流循环要读的那几样。
struct HelloSeen {
    v: u64,
    build_id: String,
    host_arch: String,
    /// 解析后的 Claude home（[`claude_home_from_hello`]）。
    claude_home: String,
    homes: Vec<AgentHome>,
    capabilities: Vec<String>,
    commands: Vec<String>,
}

/// hello 那一臂：打日志、记账、置「连上了」、版本提示、自证记忆、升级判定、降级提示。
/// `Err` ＝ 值得带 flag 重连升级一轮（由 [`run`] 照常重连）。
fn on_hello(
    host_label: String,
    round: Round,
    hello_confirmed: &mut Option<Vec<String>>,
    hello: HelloSeen,
) -> Result<(), String> {
    let Round {
        replay,
        health,
        connected,
        with_bg,
        tail_only,
        remote_older,
        t_connect_start,
    } = round;
    let HelloSeen {
        v,
        build_id,
        host_arch,
        claude_home,
        homes,
        capabilities,
        commands,
    } = hello;
    tracing::info!(
        "stream_source backend hello: v={v} build_id={build_id} host_arch={host_arch} claude_home={claude_home} homes={homes:?} caps={capabilities:?} cmds={commands:?}"
    );
    // 记下我们不认识的能力 token（多半是远端后端比 monitor 新：手工装 / 关了自动部署的用户会长期不一致）。只记账，记在这台名下；
    // 不认识的 token 本来就按保守缺省忽略。
    note_unknown_capabilities(&crate::origin::Origin(host_label.clone()), &capabilities);
    // 标记本次连接已健康(收到 backend hello)，供 run() 重连循环判定是否重置退避。
    connected.store(true, Ordering::Release);
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
    // F66（#58③）：本轮若跑在降级模式（未开 tail_only）——用 backend **自报的
    // 能力**判断能否升级，不再靠 build_id 精确匹配（闭合 2026-07-09 事故）：
    // ① backend 声明了**能开本轮没开的 flag** 的能力 → 记 hello 自愈账（存能力集）,
    //    立即重连升级（connected 已置 true → 退避重置 MIN,~2s 内带 flag 回来）。
    //    **防无限循环**：仅当「下一轮据此算出的 flag 严格优于本轮」才重连——flag 数
    //    有限（2）、每次升级严格增开，最多 2 轮收敛。
    // ② backend 无任何能力声明（真旧后端）→ 降级可见化（否则用户看到「bg 会话
    //    消失+拥塞复发」却无从归因，实测连环误诊）——经 remote-health 提示。
    tracing::info!(
        "[perf] stream_source [{host_label}] 首个 hello T+{}ms（自本轮连接开始）· \
         caps={capabilities:?}",
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
    // 升级判定无条件问 `should_upgrade_reconnect`（两项都写全 `&& !cur_*`，记账后 `next==cur` ⇒ 恒 false）。
    let show_bg = crate::load_show_bg_sessions();
    let next = decide_stream_flags(&capabilities, show_bg);
    if should_upgrade_reconnect((with_bg, tail_only), next) {
        *hello_confirmed = Some(capabilities.clone());
        return Err(copy_text("rsSshSource.upgrade.reconnect", &[]));
    }
    // ⚠ 「旧后端降级可见化」那一格**仍然**留在 `!tail_only` 里 —— 它问的是
    //   另一件事（「这台后端一条能力都没声明」），口径一个字没动。
    if !tail_only {
        if capabilities.is_empty() {
            let message = match mine {
                Some(m) => copy_text(
                    "rsSshSource.health.degraded",
                    &[
                        ("build", &build_id.to_string()),
                        ("expected", &m.to_string()),
                    ],
                ),
                None => copy_text(
                    "rsSshSource.health.degradedNoOwnBytes",
                    &[("build", &build_id.to_string())],
                ),
            };
            let payload = crate::ui_contract::RemoteHealthPayload {
                origin: host_label.clone(),
                kind: "degraded".to_string(),
                message,
                detail: String::new(),
            };
            if let Err(e) = health(payload) {
                tracing::warn!("stream_source remote-health (degraded) emit failed: {e}");
            }
        }
    }
    Ok(())
}

/// 新宣告一个活会话：成品（元信息 ＋ 初始灯 ＋ 容器）原样交 `session_book`（本机那条流同一个口），有记录文件就排旁路快照。
fn on_session_added(
    host_label: &str,
    intake: &LineIntake,
    sid: String,
    meta: LiveMeta,
    path: Option<String>,
    lines: Option<u64>,
) {
    // ★★ 这一跳要看得见（「帧到 monitor 了吗」）；每个会话一次，不淹日志。
    tracing::info!("session-added: [{host_label}] sid={sid} → 成品交出口");
    crate::session_book::feed(BookIn::Live {
        origin: host_label.to_string(),
        sid: sid.clone(),
        meta,
    });
    // Batch8-F26：tail-only 下历史改走旁路快照——宣告带 path 即入队
    // （无 path = 会话刚起还没写 jsonl → 无历史可拉，后续行天然从
    // tail 全量到达，无需快照）。队列按 sid 幂等（重复宣告不重拉）。
    intake.announced(&sid, path, lines);
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

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/frame_dispatch_shape.rs"]
mod frame_dispatch_shape;
