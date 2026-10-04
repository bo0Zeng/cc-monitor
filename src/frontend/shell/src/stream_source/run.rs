//! 远端流主循环：重连、起流、逐帧分派。

use super::*;
use crate::copy_table::copy_text;
use crate::event_replay::EventReplay;
use crate::session_book::{In as BookIn, LiveMeta};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::BufReader;

/// 〔「先剥宿主耦合，再搬」〕`remote-health` 的出口：宿主（`lib.rs::remote_health_out`）拿窗口把手造它，
/// 本模块只调它、不认识 GUI 宿主（`backend_client_guard_tests.rs::GUARDED`）。回 `Err(原话)` ＝ 没发出去，各发射点那句 warn 照旧。
pub(crate) type HealthOut =
    Arc<dyn Fn(crate::ui_contract::RemoteHealthPayload) -> Result<(), String> + Send + Sync>;

/// U8a-2a：**握手完成 ⇒ 写半边解冻。**
///
/// 见证只能由一帧真的 Hello 换出来（`BackendHello::from_hello_frame`）⇒
/// 「hello 之前不许写」在 monitor 侧是类型上的事实，不是一条纪律。
/// 第二次 hello（不该有）时 `parked` 已被 `take` 走，静默跳过。
///
/// **抽成函数是为了让它可测**：D 审计变异 MU13 —— 把这段逻辑整个删掉（写半边永不解冻、
/// 客户端永不登记）⇒ `cargo test` **全绿**。它埋在 `stream_loop` 中段时没有任何判据碰得到。
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

/// U8a-2a：把一帧入方向应答路由回请求方。返回是否真的交到了某个等待者手上。
///
/// 没有客户端 = backend 在 hello 之前就回了应答（协议倒错），照实报、不静默。
///
/// **抽成函数同样是为了可测**（D 审计变异 MU12：把 `route_reply` 换成丢弃 ⇒ 全绿）。
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
            data,
        } => c.route_reply(&id, ok, code, message, data),
        InboundFrame::Cancelled { id } => c.route_cancelled(&id),
        _ => false,
    }
}

/// U8a-2a：`stream_loop` 那条**接缝**的判据。
///
/// # 为什么单独立一个模块
///
/// D 审计做了三次变异，三次 `cargo test` **全绿**：
/// - MU13：hello 臂里不 `into_client`/不 `register`（写半边永不解冻）
/// - MU12：`reply` 臂里不 `route_reply`（应答收到就扔）
/// - MU14：控制通道往返的探针直接返回 `"control=ok(0ms)"`，一个字节都不发
///
/// 也就是「把发送端接上」这件事本身删掉之后 CI 一片绿 —— `inbound_client` 的单测走的是
/// 自造客户端，e2e 走的是真后端二进制，**两者之间的接缝没有任何判据**。
/// 这个模块就是那条接缝。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/seam_tests.rs"]
mod seam_tests;

/// U8a-2a：**写半边只许经 `inbound_client::park` 出手。**
///
/// 这条护栏存在的理由是它守的东西刚变过：这个文件从「只读一条流」变成了「双工」。
/// 一旦有人为了图省事在这里直接 `write_all` 一行，`ParkedWriter` 那层
/// 「Hello 之前不许写」的类型保证就被绕过了 —— 而且是**静默**绕过（编译、测试全绿）。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/write_half_guard.rs"]
mod write_half_guard;

/// SSH-remote 数据源主循环（S5）。
///
/// 连接远端、exec backend、把 backend stdout 的 line-delimited JSON 帧逐行解析后分发：
/// - `hello` → log（证明 backend runtime 起来了）+ 置 `connected`（标记本次连接已健康，
///   供重连循环判定是否重置退避）。
/// - `line` → 组 [`JsonlLine`] 交 [`LineIntake`]（本机那条流用的是同一个）：
///   攒批后 `crate::batch_to_payloads(...)` → `replay.on_line_batch_awaited(&app, ...)`
///   （前端按 seq 自动排序）。
/// - 会话起停的成品（`session_added` · `session_status` · `session_state` · `sessions_replayed`）→ 原样交
///   `session_book::feed`（后端裁、monitor 只转交；本机那条流同一个口）；连接断了 ⇒ `session_book::In::LinkLost`。
/// - 未知 kind / garbage → `tracing::warn!` 跳过，绝不中断流。
///
/// stdout EOF / 读错误 → 返回 `Err`，调用方（S8/S9）据此大声报"connection dropped"，
/// 不静默冻结。
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

    // 重连循环：每轮跑一次 stream_loop。失败/掉线后按指数退避（2→4→8→16→30s 封顶）重连；
    // 本轮**连上过**（收到 backend hello，connected=true）则下次立即以 MIN 快速重连。
    // INVARIANT §10：唯一的等待是 tokio::time::sleep（async、非阻塞），绝不 std::thread::sleep。
    let mut backoff = RECONNECT_MIN;
    // v2.22.1 hello 自愈账本:上一轮 hello 自证 backend==当前版本时记账,下一轮以此
    // 越过「部署侧确认失败」的降级(内嵌清单缺失的 CI 安装包 v2.19-v2.22 全中招)。
    // 若带 flag 的一轮连 hello 都没收到(真·旧后端把未知参数当一次性查询退出),
    // 清账回退降级,防止 flagged 重连死循环。
    // F66（#58③）：hello 自愈账本——存上一轮 backend **自报的能力 token 集**（原为
    // build_id）。None = 尚未收到能力声明；Some(caps) = 下一轮据此发 flag 升级。
    // 回退清账语义（`:is_some()` 那段）不变。
    let mut hello_confirmed: Option<Vec<String>> = None;
    // 这台是不是「永久不支持」（非 unix）：`stream_loop` 接不上常驻时写，本循环读完即清。
    let mut unsupported: Option<String> = None;
    loop {
        connected.store(false, Ordering::Release);
        // F05：本轮连接的起点。退避重置的判据是「活过多久」，不是「握没握上手」。
        let conn_started = std::time::Instant::now();
        let result = stream_loop(
            &cfg,
            &replay,
            &health,
            &connected,
            &mut hello_confirmed,
            &mut unsupported,
        )
        .await;
        // 这条连接没了 ⇒ 订了这台会话流的那些订阅原位收一格 `Unseen`（不是终点）。
        replay.origin_seen(&crate::origin::Origin(cfg.origin_label()), false);
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
        // 两段式（非冗余）：先按**当前** backoff 睡，再在仍没连上时翻倍。这样首次失败也只等
        // MIN，退避序列是 2→4→8→16→30；若收成单个 if/else（睡前就翻倍），首次失败会直接等 4s。
        // sleep 期间 `connected` 不会变（其唯一写者 stream_loop 已返回），故两次 load 读到同值。
        // F05（报告 I-1）：**「连上过」不等于「站住了」**。判据从「收到过 hello」换成
        // 「这条连接活过 MIN_HEALTHY_UPTIME」——hello-then-die 的后端此前每轮都算连上过，
        // 退避永远重置回 2s、每分钟约 90 次 SSH 握手砸在那台已经撑不住的机器上。
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
                };
                if let Err(e) = health(payload) {
                    tracing::warn!("stream_source remote-health (unsupported) emit failed: {e}");
                }
                return Err(why);
            }
        };
        tracing::info!("stream_source reconnecting in {:?}", wait);
        tokio::time::sleep(wait).await;
        if !connected.load(Ordering::Acquire) {
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
) -> Result<(), String> {
    // issue #15 / #30：远端行的 origin 标签 = 该机器的稳定身份（label，默认 host）。
    // 前端据此给该 Tab 标题加 `[label]` 前缀以区分本地/各远端机器。进 loop 前 clone。
    let host_label = cfg.origin_label();

    let Opened {
        stream,
        with_bg,
        tail_only,
        t_connect_start,
    } = open_round(cfg, health, hello_confirmed, unsupported).await?;

    // U8a-2a：这条 channel 是**双工**的，此前只用了读半边。`split_and_park` 一步切开并
    // 把写半边停住 —— `ParkedWriter` 身上没有任何写方法，要等收到 hello 才换得出能发命令的
    // 客户端。切与停必须是同一步：中间留一个裸 `WriteHalf` 就等于留了一个「Hello 之前能写」
    // 的窗口（D 审计实测过那个窗口，两条护栏都拦不住）。见 `inbound_client` 头注。
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

    // Batch8-F26：旁路快照基础设施（仅 tail-only 生效；每连接一套，函数任何
    // 退出路径随 `intake` 被丢掉而关闭队列——已入队项仍会被分发器拉完，独立连接自灭）。
    // 攒批 ＋ 静默窗 ＋ 旁路快照收成 [`LineIntake`]，本机那条流用的是同一个。
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
                // EOF/读错：flush 残余（at-least-once 安全；重连会从 seq 0 重放，
                // 但没有理由主动丢已收到的行）**并等它发完**再报错——run() 随后的
                // 断连归档（announced 清算）必须晚于这些行到达前端（审计 R1）。
                intake.flush().await;
                return Err(e);
            }
        };
        let line = line.as_str();

        let frame = unread.take(line, &mut tally, &say_health);
        // SessionRemoved 是唯一顺序敏感的攒批边界：它的行必须先落前端，否则
        // 归档后迟到的行把 Tab 复活成僵尸 live（审计 R1/R2）。SessionAdded /
        // Hello / Overflow / 坏帧**不再**作边界——多小会话的 snapshot 才能聚
        // 成大批跨过阈值（行先于 Added 到达无妨：前端 ensureTab 见行即建）。
        // `session_state`（可重连 / 已结束的成品）同理：它说的「离开了」必须排在这个会话的行之后。
        if matches!(
            frame,
            Some(InboundFrame::SessionRemoved { .. } | InboundFrame::SessionState { .. })
        ) {
            intake.flush().await;
        }

        // U8a-2a：**握手完成 ⇒ 写半边解冻。** 放在 match 之前是因为 Hello 那条臂按值解构了帧。
        if let Some(client) = attach_inbound_client(&host_label, &mut parked, frame.as_ref()) {
            inbound_guard.1 = Some(client.clone());
            inbound = Some(client);
            // 「这台的长连接能问话了」—— 前端的账号刷新在这一刻强制拉一次。
            // 原先这里发一个裸 Tauri 事件（`remote-backend-ready`）；今天由下面 Hello 臂里既有的
            //   `replay.origin_seen(.., true)` 说（订了这台 `accounts-changed` 的订阅原位收 `Seen`，`event_replay` 头注那张表）——
            //   同一个时刻、同一个事实，只留一个家。
            // 连上那一刻：让本机常驻后端沿池里那条 SSH 同步资产目录（后台跑，零判定）。
            let accepts = inbound
                .as_ref()
                .is_some_and(|c| c.accepts(crate::asset_sync::REMOTE_NEEDS));
            crate::asset_sync::on_remote_ready(cfg, accepts);
            // 升级那一格：连上那一刻后台看一眼旧版 `~/.local/bin/ccm`，认出是我们放的就删（`ccm_legacy`）。
            crate::ccm_legacy::on_remote_ready(cfg);
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
                    // `S4`：日志报的是**解析后**的 Claude home（优先 `homes`、回退 `claude_dir`），
                    // 同时把原样的 `homes` 一起打出来 —— 排障时要能一眼看出
                    // 「这台后端到底发没发新字段」，那正是 additive 迁移期最常问的问题。
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
                // Batch5-F17：进攒批缓冲（达 cap/批龄立即整批出）；静默窗口/
                // SessionRemoved 边界触发的 flush 在循环头。
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
                session_kind,
                attachable,
                cwd,
                project_dir,
                name,
                path,
                lines,
                status,
                waiting_for,
                container,
                // pid 只给本机那条流用（本机 ↗ 绑窗口）；远端这一支不读。
                pid: _,
            }) => on_session_added(
                &host_label,
                &intake,
                sid,
                LiveMeta {
                    kind: session_kind,
                    attachable,
                    cwd,
                    project_dir,
                    name,
                    status,
                    waiting_for,
                    container,
                    pid: None,
                },
                path,
                lines,
            ),
            Some(InboundFrame::SessionStatus {
                sid,
                status,
                waiting_for,
            }) => {
                // ★★〔08-14 实机排障补〕红绿灯这一跳也要看得见：「全绿」既可能是都在忙，也可能是 status 一条都没到。
                tracing::info!(
                    "session-status: [{host_label}] sid={sid} status={status:?} \
                     waiting_for={waiting_for:?} → 成品交出口"
                );
                crate::session_book::feed(BookIn::Status {
                    origin: host_label.clone(),
                    sid,
                    status,
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
                // 只剩内容流的边界：残批已在循环头冲掉；这里摘排队中的快照 ＋ 给在途的打取消标记
                //   （Batch8 D-B1：归档后迟到的快照行会经「见行复活」造出僵尸 tab）、续点作废。它离开之后是什么由下一帧 `session_state` 说。
                tracing::info!(
                    "session-removed: [{host_label}] sid={sid}（内容流收口；去向看 session_state）"
                );
                intake.removed(&sid);
            }
            // 后端裁好的去向（可重连 / 已结束）原样交出口（残批已在循环头冲掉）。
            Some(InboundFrame::SessionState { sid, state }) => {
                tracing::info!("session-state: [{host_label}] sid={sid} → {state:?}");
                crate::session_book::feed(BookIn::Left {
                    origin: host_label.clone(),
                    sid,
                    fate: state,
                });
            }
            Some(InboundFrame::Overflow {
                dropped,
                lost,
                lost_truncated,
            }) => on_overflow(&host_label, health, dropped, &lost, lost_truncated),
            // U8a-2a：入方向应答 —— 交给本连接的客户端按 `id` 路由回请求方。
            Some(f @ (InboundFrame::Reply { .. } | InboundFrame::Cancelled { .. })) => {
                route_inbound_frame(&host_label, inbound.as_ref(), f);
            }
            // 那台的账号清单变了 ⇒ 告诉前端（账号表与 chip 据此重取）。
            // 经通道 `subscribe`：订了这台 `accounts-changed` 的订阅收一格 `Frame`（原先是一个裸 Tauri 事件）。
            Some(InboundFrame::AccountsChanged) => {
                replay.accounts_changed(&crate::origin::Origin(host_label.clone()));
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
            // 认识但不消费的两种（理由在变体上）。
            Some(InboundFrame::TurnEnd | InboundFrame::QuotaChanged) => {}
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
) -> Result<Opened, String> {
    let host_label = cfg.origin_label();

    // ★〔audit-0805 F05 下半，报告 §4.3〕**冷启动最贵的三段此前一个 `[perf]` 都没有**。
    //
    // 实测（08-06）：全仓 `[perf]` 前端 13 处、monitor Rust 14 处，而 `stream_source/` **0 处** ——
    // 偏偏这里是「用户点开应用到看见远端会话」之间**唯一**的那条链。
    // 后果不是「不知道快慢」，是**报告里那些 50-200ms 的数字是外部常识值、不是本仓证据**
    //（那条诚实边界就挂在这上面）。没有埋点，「冷启动三连接合并省了多少」
    // 这句话永远只能靠推。
    //
    // ⚠ 埋点本身**不改任何行为**，也不该改：它只是让下一次讨论有数可依。
    let t_connect_start = std::time::Instant::now();

    // issue #29（F08）：连接前确保远端后端已（自动）部署到固定落点（`~/.cc-monitor/bin/ccm`）。
    // 嵌入二进制就位前（F08b 未做）`byte_table::choose` 回「这一版没带」→ ensure_backend_deployed
    // 优雅 no-op。**best-effort**：部署失败仅 warn，不阻断——手动部署的后端仍可连。
    // ★ F05 下半：**上一次这台机器的后端自报过就是期望 build ⇒ 跳过预检那两条连接**。
    // 判据与记忆的语义见 `VERIFIED_BUILD` 头注（记的是 hello 自证，不是预检结论）。
    // 跳过时 `confirmed_build` 直接给 `EXPECTED` —— 若给 `None`，下面的 caps 阶梯会掉进
    // ③ 空集全降级，那就**比不跳还糟**（省两条连接换来一轮降级 + 一轮升级重连）。
    let verified = verified_build_of(&host_label);
    let skip_preflight = preflight_can_be_skipped(verified.as_deref(), EXPECTED_BACKEND_BUILD_ID);
    let confirmed_build = if skip_preflight {
        Some(EXPECTED_BACKEND_BUILD_ID.to_string())
    } else {
        match crate::sftp::ensure_backend_deployed(cfg).await {
            Ok(c) => Some(c),
            Err(e) => {
                // **不阻断**（手动部署的后端照样能连），但那句话要到界面上 ——
                //   从前这里只 `warn!`、拒绝那几形更是 `debug!` ＋ `Ok(None)`，用户看到的是「什么都没发生」。
                let msg = e.say();
                tracing::warn!(
                    "stream_source [{host_label}] 后端没部署上（继续尝试连接已有后端）: {msg}"
                );
                let payload = crate::ui_contract::RemoteHealthPayload {
                    origin: host_label.clone(),
                    kind: "deploy".to_string(),
                    message: msg,
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

    // F66（#58③）流模式门控：**从后端声明的能力 token 决定发哪些 flag**，不再靠
    // build_id 精确匹配。旧后端会把未知参数当一次性查询处理后退出（无 hello → 重连
    // 死循环，§26），故只对**声明了对应能力**的后端发 flag（声明 = 自证会剥离该 flag）。
    // 能力两条来源，hello 自愈账本优先：
    //   ① `hello_confirmed`（上一轮 backend **自报**的能力）—— 最权威，收过真 hello 才有。
    //   ② 否则部署侧确认了当前内嵌 build（`confirmed_build == EXPECTED`）→ 用内嵌后端的
    //      能力常量**预知**，省第一轮「降级→收 hello→重连升级」往返（乐观路径）。
    //   ③ 都没有 → 空集 → 全降级（= 2.18.0 行为，连接正常、功能退化）。
    // **hello 优先**于部署侧（②可能是陈旧内嵌的身份 ≠ 期望 → 空集 → 靠 hello 自愈救，
    //  见 v2.22.1 无限重连教训；`hello_confirmed` 只在收到真声明时写入，优先采纳恒安全）。
    let caps: Vec<String> = hello_confirmed.clone().unwrap_or_else(|| {
        if confirmed_build.as_deref() == Some(EXPECTED_BACKEND_BUILD_ID) {
            embedded_backend_capabilities()
        } else {
            Vec::new()
        }
    });
    let (with_bg, tail_only) = decide_stream_flags(&caps, crate::load_show_bg_sessions());
    let t_exec = std::time::Instant::now();
    // ★ F05 下半：起流失败就抹掉自证记忆 —— 否则一台后端被删/被换旧的机器会
    // **每一轮都跳预检、每一轮都失败**，永远等不到重新部署。代价是多一次重连，
    // 那正是 `VERIFIED_BUILD` 头注里如实写下的那个退化。
    // 接那台的**常驻后端**（没有就起一个；与本机同形）。这是远端唯一的一形：
    //   起不了常驻（非 unix / 太旧）就是一次失败、说清为什么，不回落到随 SSH 生死的流模式。
    let flags = (with_bg, tail_only);
    let stream: crate::remote_resident::Replayed = match crate::remote_resident::attach(cfg, flags)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            // 非 unix ⇒ 记进这台的连接状态（`run` 据此停下，不再按退避重连）。
            if let crate::remote_resident::AttachErr::Unsupported(why) = &e {
                *unsupported = Some(why.clone());
            }
            let e = e.said();
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

/// Batch5-F17：帧读取挪进独立 task、经 channel 交回——攒批需要"带静默窗口
/// 的读"，而 tokio 的 read_line **不是 cancellation-safe**（timeout 取消会
/// 丢 buffer 里的半帧）；mpsc::Receiver::recv 是 cancel-safe 的，超时打在
/// recv 上帧零丢失。reader task 在 EOF/读错时投递 Err 后退出；本函数返回
/// （重连）时 rx drop → task 的 send 失败 → task 自然退出，不泄漏。
/// `Ok(None)`：这里有一行超长、整行丢了（说不出是哪个会话的哪一行）⇒ 主循环原位给订阅一格 `Gap`（`to_seq` 缺）。
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
        // 按 `\n` 切（协议保证每帧一行、帧内换行已被后端转义成 `\n` 两字符，
        // 见 src/backend/stream/wire.rs）。
        // ★ F10b：从无界 `read_line` 换成 [`read_capped_line`] —— 无界读遇「一条永远不结束
        // 的行」就是无界堆分配，而对端是**远端进程**（它坏掉或不是我们的后端都可能）。
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
                    // 超限语义 = **丢弃 + 原位说出来**，绝不静默（定框 E4）。
                    // 不走 Err 臂（那会被当成致命错误去重连，而坏的只是这一行），
                    //   也不再走旁路健康提示：与行同一条路交 `Ok(None)`，主循环原位给订阅一格 `Gap`（本机两条载体同形）。
                    tracing::warn!(
                        "stream_source remote [{reader_host}] line too long: {bytes} bytes \
                         (cap {BACKEND_FRAME_LINE_CAP}); line dropped"
                    );
                    if frame_tx.send(Ok(None)).await.is_err() {
                        break;
                    }
                }
                Ok(CappedLine::Line) => {
                    // 非 UTF-8 不该让整条连接死掉（与全批 exec 输出读取同一取舍）—— 但**记账、说出来**（W5-VIS）。
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
    // U-CC1：记下**我们不认识的**能力 token。多半是远端后端比 monitor 新
    // （自动部署会把它拉回同一个 build，但手工装 / 关了自动部署的用户会长期不一致）。
    // 只记账，行为一字不改：不认识的 token 本来就按保守缺省忽略。
    // 记在这台名下。
    note_unknown_capabilities(
        &crate::origin::Origin(host_label.clone()),
        &capabilities,
        &build_id,
    );
    // 标记本次连接已健康(收到 backend hello)，供 run() 重连循环判定是否重置退避。
    connected.store(true, Ordering::Release);
    // 订了这台会话流的那些订阅原位收一格 `Seen`（`Item::Seen`）。
    replay.origin_seen(&crate::origin::Origin(host_label.clone()), true);
    // issue #33：版本协商。不兼容/偏旧经 SS-F remote-health 通道醒目提示（前端
    // headlineFor 已含 version case，零前端改动）。不 hard-disconnect（向前兼容）。
    if let Some(msg) = version_warning(v, &build_id, &host_label, remote_older) {
        tracing::warn!("stream_source remote [{host_label}] version: {msg}");
        let payload = crate::ui_contract::RemoteHealthPayload {
            origin: host_label.clone(),
            kind: "version".to_string(),
            message: msg,
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
    // build_id 不是期望值 ⇒ **抹掉**（这台机器上装的不是当前 build，
    // 下一轮必须照跑预检去部署），不是「留着上次的」。
    if build_id == EXPECTED_BACKEND_BUILD_ID {
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
            let payload = crate::ui_contract::RemoteHealthPayload {
                origin: host_label.clone(),
                kind: "degraded".to_string(),
                message: copy_text(
                    "rsSshSource.health.degraded",
                    &[
                        ("build", &build_id.to_string()),
                        ("expected", &EXPECTED_BACKEND_BUILD_ID.to_string()),
                    ],
                ),
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
    };
    if let Err(e) = health(payload) {
        tracing::warn!("stream_source remote-health emit failed: {e}");
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/frame_dispatch_shape.rs"]
mod frame_dispatch_shape;
