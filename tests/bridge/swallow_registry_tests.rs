//! 〔W5-VIS〕**业务路径零裸吞** —— 人群判据与登记表（设计与射程住 `src/bridge/src/swallow_registry.rs` 头注）。
//!
//! 要求住址：`设计/15 §4.7 S5`（逐字）「**处置不是别吞，是吞了要留一行日志**」。
//!
//! 三条：① 人群 == 登记表（两向：没登记的新裸吞 · 修掉了还留着的死行）② 量具正控（合成语料：该认的认出、不该认的不认）
//! ③ 测试专用模块确实被排除（按 `mod` 声明派生出来的那一组里有已知的那几份）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// 为什么这一处可以丢掉结果。**每一类一句理由**（写在变体上）；行上的第五栏可以再补一句。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // 各类由登记表逐行引用；某一类一行都没有时不该让编译器替它说话
enum Why {
    /// 收尸：杀 / 等子进程、关句柄（Windows `CloseHandle` / `LocalFree`）—— 收尾那一步本身失败，没有更好的补救，
    /// 而要报的主结局已经在别处定了。
    Reap,
    /// 对端已走：往一个接收端可能已经不在的通道里发（`send` / `try_send` / `emit`）。接收端不在 = 没人要这个结果。
    PeerGone,
    /// 写回一条正在出错 / 已断的流：拒绝行、错误帧、刷新、关写半边。写不进去说明对面已经走了，无处可报。
    DeadLink,
    /// 失败后收拾：主错误已经在回（或这一趟已经判失败），这一步只是清临时件 / 半成品；清不掉只剩一份垃圾，不改结局。
    CleanupAfterFailure,
    /// 窗口尽力而为：恢复 / 显示 / 聚焦 / 置顶。做不成不改语义（拉前有自己的失败报告那条路）。
    WindowBestEffort,
    /// 写进 `String`：`fmt::Write for String` 不会失败。
    InfallibleWrite,
    /// 等一个信号 / 一次性通知：只要「等到了」，结果本身没有信息量。
    Signal,
    /// 读完丢弃：排空管道 / 丢掉剩下的字节，只为让对面不堵。
    Drain,
    /// 诊断输出本身：往 stdout / stderr / 日志写失败时，没有第二个地方可以说。
    Diag,
    /// 结果是「有没有」而不是「成没成」（`Option`），`None` 是常态；该说的在被调的那一侧说了。
    NotAnError,
    /// 只设一次：`OnceLock::set` 第二次回 `Err` 是设计（第一次那个值留着）。
    SetOnce,
    /// 控制面让位：应答通道满了宁丢这一条应答也不等 —— 等就把读循环堵死、后面的 `cancel` 连解析都轮不到
    /// （`inbound.rs` 里 `cancel` 那一臂的头注：「丢一条 cancel 应答，远比堵死读循环便宜」）；调用方按自己的期限收场。
    Backpressure,
    /// 别路在改：这一处住在另一路的写区里，本路不碰；那一路合并时这一行要随之删 / 改。
    OtherLane,
}

/// **允许吞的，逐条登记**：`(文件, 语句原文归一化, 同文件同文处数, 类, 补一句)`。
///
/// 归一化 = 字符串字面量之外的空白压成一个空格、`.` `(` `)` `,` 两侧的空格去掉、`,)` 收成 `)`（rustfmt 换不换行不影响键）；
/// 超过 [`KEY_CHARS`] 个字符的截断。**不含行号**。
const ALLOWED: &[(&str, &str, usize, Why, &str)] = &[
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] create that file to configure one; it is plain JSON:\\n{}\", store::template());", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] credentials file: {}\", loaded.path.display());", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] credentials permissions too wide: {how}\");", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] credentials permissions unknown: {why}\");", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] credentials problem: {p}\");", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] credentials: auth_style must be one of: {legal}\");", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] credentials: configured, {rows} account(s) routable\");", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] credentials: not configured\");", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] credentials: this account cannot be used: {:?} - {}\", r.id, r.why);", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] credentials: this account is not on the default path: {:?} - {}\", note.id, note.what);", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/creds.rs", "let _ = writeln!(out, \"[apikey] how to fix: {fix}\");", 1, Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream/file_face.rs", "let _ = std::fs::remove_file(&tmp);", 1, Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回，删不掉只剩一份临时件"),
    ("src/backend/assets/asset_catalog.rs", "let _ = std::fs::remove_file(&tmp);", 1, Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回，删不掉只剩一份临时件"),
    ("src/backend/control/exit_policy.rs", "let _ = std::fs::remove_file(&tmp);", 1, Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回，删不掉只剩一份临时件"),
    ("src/backend/control/files_commit.rs", "let _ = std::fs::remove_file(&at);", 2, Why::CleanupAfterFailure, "写块失败 / 收拾旧块：主错误已在回；删不掉等孤儿扫（7 天）"),
    ("src/backend/control/files_commit.rs", "let _ = std::fs::remove_file(&side);", 1, Why::CleanupAfterFailure, "跨盘提交抄写失败后删自己这一趟的旁名；主错误已在回"),
    ("src/backend/control/files_commit.rs", "let _ = std::fs::remove_file(&staged);", 1, Why::CleanupAfterFailure, "目标已落好之后删暂存件；删不掉由孤儿扫（`sweep_stale`）按期限收（该处注释原话）"),
    ("src/backend/control/files_write.rs", "std::fs::remove_file(&bak).ok();", 1, Why::CleanupAfterFailure, "写失败之后删自己这一趟建的旁名 / 半成品；主错误已在回"),
    // 〔FILES2 · 第四波〕解压落一份文件写一半 / 设权限失败 ⇒ 删自己 `O_EXCL` 刚建的那一份；上传块形拼暂存件失败 ⇒ 删自己刚建的 `<key>.part`。
    ("src/backend/control/files_extract.rs", "std::fs::remove_file(&at).ok();", 1, Why::CleanupAfterFailure, "解压写一份失败之后删自己 `O_EXCL` 刚建的那一份；主错误已在回（随后整趟回滚）"),
    ("src/backend/control/files_upload_chunks.rs", "std::fs::remove_file(p).ok();", 1, Why::CleanupAfterFailure, "拼暂存件失败之后删自己 `O_EXCL` 刚建的那一份；主错误已在回，块由 `drop_chunks` 收"),
    // 〔FIX5〕`(&land)` 那一行 3 → 0：复制也先落旁名，三处清理并成 `land_copy` 里一处删 `side`；`(&side)` 3 → 5：多的是那一处 ＋ 读改写新建那一形不覆盖上位失败那一处。
    ("src/backend/control/files_write.rs", "std::fs::remove_file(&side).ok();", 5, Why::CleanupAfterFailure, "写失败之后删自己这一趟建的旁名 / 半成品；主错误已在回"),
    ("src/backend/control/tmux_hook.rs", "let _ = crate::platform::signal::send_sigusr1(pid);", 1, Why::PeerGone, "信号送不到 = 那个进程已不在；发之前有进程身份复核（`15 §4.7` 做得好的对照组）"),
    ("src/backend/control/transfer.rs", "let _ = forward.await;", 1, Why::Reap, "等进度转发任务收尾（它自己只往已结束的票上报数）"),
    ("src/backend/control/transfer.rs", "let _ = rf.shutdown().await;", 1, Why::DeadLink, "关远端文件句柄；传输的结局已经定了"),
    ("src/backend/control/transfer.rs", "let _ = sftp::remove(s, &part).await;", 1, Why::CleanupAfterFailure, "撤了 ⇒ 删暂存区里的半截（用户说了不要）；删不掉只是暂存区里多一份，孤儿扫会收"),
    ("src/backend/control/transfer.rs", "let _ = std::fs::remove_file(&at);", 1, Why::CleanupAfterFailure, "DP1 之后只清**空的** `.part`（一个字节都没落）；清不掉只剩一个 0 字节文件，主错误已在回"),
    ("src/backend/dial/link.rs", "let _ = replies.send(Frame::LinkEnd { link: link.clone(), error, }).await;", 1, Why::PeerGone, ""),
    ("src/backend/dial/probe.rs", "let _ = up_w.shutdown().await;", 1, Why::DeadLink, "〔MIG-1 续〕测试连接探完关上行写半边（结局已定，这条探活链路随即整条丢掉）"),
    ("src/backend/dial/pool.rs", "let _ = self.freed.set(freed);", 1, Why::SetOnce, "族的铃只挂一次"),
    ("src/backend/dial/sftp.rs", "let _ = s.sftp().remove_file(b).await;", 2, Why::CleanupAfterFailure, "〔HX2〕部署换名成功后删自己挪走的旧备份件 / 换名失败而落点已被别的部署者占上时删它；删不掉只剩一份 `.bak`"),
    ("src/backend/dial/sftp.rs", "let _ = s.sftp().remove_file(tmp.clone()).await;", 3, Why::CleanupAfterFailure, "〔HX2〕删**自己这一趟**建的临时件（名字唯一，只这一趟知道）；主错误已在回"),
    ("src/backend/dial/sftp.rs", "let _ = s.sftp().rename(b, rel.clone()).await;", 1, Why::CleanupAfterFailure, "〔HX2〕换名失败、落点还空着 ⇒ 把自己挪走的旧文件挪回去（主错误已在回）"),
    ("src/backend/dial/sftp.rs", "let _ = tokio::io::copy(&mut (&mut *input).take(size), &mut tokio::io::sink()).await;", 1, Why::Drain, ""),
    ("src/backend/dial/sftp.rs", "let _ = write_line(out, &refused(\"bad_request\", &e)).await;", 1, Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = out.flush().await;", 1, Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = self.close().await;", 1, Why::Reap, "放弃 / 提前收工时向远端发关通道：对端撤活只是尽力（`05 §3.3.3`）"),
    ("src/backend/dial/uses.rs", "let _ = write_line(out, &got).await;", 1, Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(crate::common::contract::malformed(\"use=capture without `cap", 1, Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(crate::common::contract::malformed(\"use=forward without `for", 1, Why::DeadLink, ""),
    // 〔HOST〕隧道那一臂的两条失败 ack（同上几行：写不进去说明界面已经走了）。
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(crate::common::contract::malformed(\"use=tunnel without `tunn", 1, Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(copy_text(\"beUses.tunnel.unreachable\", &[(\"port\", &port.to_s", 1, Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;", 4, Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(copy_text(\"beUses.exec.failed\", &[(\"e\", &e.to_string())]), f", 1, Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(copy_text(\"beUses.forward.bindFailed\", &[ (\"port\", &spec.loc", 1, Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &fail(e)).await;", 1, Why::DeadLink, ""),
    ("src/backend/files/browse_watch.rs", "let _ = self.inner.unwatch(&super::raw::to_path_buf(d));", 1, Why::Reap, "撤不再看的目录的 watch：目录已删时 unwatch 本来就会失败"),
    ("src/backend/files/mod.rs", "let _ = write!(out, \"{b:02x}\");", 1, Why::InfallibleWrite, "`out` 是 `String`（十六进制摘要）"),
    ("src/backend/history/history_annotations.rs", "let _ = std::fs::remove_file(&tmp);", 1, Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回，删不掉只剩一份临时件"),
    ("src/backend/stream/inbound.rs", "let _ = gate_rx.await;", 1, Why::Signal, ""),
    ("src/backend/stream/inbound.rs", "let _ = gate_tx.send(());", 1, Why::Signal, ""),
    ("src/backend/stream/inbound.rs", "let _ = replies.send(frame).await;", 1, Why::PeerGone, ""),
    ("src/backend/stream/inbound.rs", "let _ = replies.try_send(Frame::Cancelled { id: target });", 1, Why::Backpressure, ""),
    ("src/backend/stream/inbound.rs", "let _ = replies.try_send(err(&req.id, \"not_cancellable\", &copy_text(\"beInbound.dispatch.cannotCancel\", &[])));", 1, Why::Backpressure, ""),
    ("src/backend/stream/inbound.rs", "let _ = replies.try_send(ok(&req.id));", 1, Why::Backpressure, ""),
    ("src/backend/stream/inbound.rs", "let _ = replies_sup.send(err(&id_sup, \"handler_panicked\", &copy_text(\"beInbound.spawnHandler.crashed\", &[]))).await;", 1, Why::PeerGone, ""),
    // 〔HOST〕多客户：流结束那一格带上连接号（原 `done.send(())`）；认证通过的连接交主循环 —— 主循环不在了 = 进程在收尾。
    ("src/backend/main.rs", "let _ = done.send(id).await;", 1, Why::Signal, ""),
    ("src/backend/main.rs", "let _ = attached.send(Attached { reader: r, writer: w, hello_flushed, flags: flags.unwrap_or(None), }).await;", 1, Why::PeerGone, ""),
    ("src/backend/main.rs", "let _ = listen::write_line(&mut w, &listen::refusal_line(reason)).await;", 1, Why::DeadLink, ""),
    ("src/backend/observe/watcher.rs", "let _ = debouncer.watcher().unwatch(dir);", 2, Why::Reap, "撤旧 inode 上的 watch：目录被删 / 换过 inode 时 unwatch 本来就会失败"),
    ("src/backend/observe/watcher.rs", "let _ = debouncer.watcher().unwatch(sessions);", 2, Why::Reap, "撤旧 inode 上的 watch：目录被删 / 换过 inode 时 unwatch 本来就会失败"),
    ("src/backend/observe/watcher.rs", "let _ = debouncer.watcher().unwatch(sock_dir);", 2, Why::Reap, "撤旧 inode 上的 watch：目录被删 / 换过 inode 时 unwatch 本来就会失败"),
    ("src/backend/observe/watcher.rs", "let _ = self.0.send(WatchEvent::Notify(event));", 1, Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = self.0.send(WatchEvent::Poke);", 1, Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = self.0.send(WatchEvent::Shutdown);", 1, Why::PeerGone, ""),
    // 〔RESYNC〕基数 3 → 1：起探测收成 `start_tmux_probe` 一处（三处搬进去，不是删）。
    ("src/backend/observe/watcher.rs", "let _ = tx.send(WatchEvent::TmuxObserved(run_tmux_probe()));", 1, Why::PeerGone, ""),
    // 〔RESYNC〕+2：对齐的应答发回等它的那一方（它可能已退）· 对每份 watcher 发对齐（那份可能正在退出；它丢了应答端，`resync` 不会挂住）。
    ("src/backend/observe/watcher.rs", "let _ = done.send(got);", 1, Why::PeerGone, ""),
    // 〔RESYNC 续〕+1：SIGUSR1 戳名单上的每一份（`PokeSlot` 并进来；那一份可能正在退出）。
    ("src/backend/observe/watcher.rs", "let _ = w.send(WatchEvent::Poke);", 1, Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = w.send(WatchEvent::Resync { only: only.map(str::to_string), done: tx.clone(), });", 1, Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = tx.send(WatchEvent::TmuxProbeDue);", 1, Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = tx.send(target.death_event(pid));", 1, Why::PeerGone, ""),
    ("src/backend/platform/signal.rs", "let _ = t.recv().await;", 1, Why::Signal, "装不上 SIGTERM 时退回只等 SIGINT（〔HX1〕从 `main.rs` 下沉来）"),
    ("src/backend/platform/signal.rs", "let _ = tokio::signal::ctrl_c().await;", 2, Why::Signal, "〔HX1〕从 `main.rs` 下沉来的停机信号监听"),
    ("src/backend/plugin/invoke.rs", "let _ = (&mut *r).take(keep.saturating_add(1)).read_to_end(out).await;", 1, Why::Drain, "留前 keep＋1 字节；读坏了留下的就是那一截，成败按退出码判（`run_abortable` 头注「每条流」）"),
    ("src/backend/plugin/invoke.rs", "let _ = crate::platform::signal::kill_group(g);", 1, Why::Reap, ""),
    ("src/backend/plugin/invoke.rs", "let _ = tokio::io::copy(r, &mut tokio::io::sink()).await;", 1, Why::Drain, ""),
    ("src/backend/relay/door.rs", "let _ = std::fs::remove_file(&tmp);", 1, Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回，删不掉只剩一份临时件"),
    ("src/backend/control/resident.rs", "let _ = std::fs::remove_file(&tmp);", 1, Why::CleanupAfterFailure, "〔HOST〕钥匙 / pid 文件的原子写临时件：换名失败之后删它；主错误已在回"),
    ("src/backend/control/resident.rs", "let _ = log_dir_chain(&h);", 1, Why::Diag, "〔GAP1〕诊断文件那层目录建不了 ⇒ 子进程装不上 stderr 文件、照旧 null（「写不进去不拖垮后端」`15 §4.7 S1`）；这是一次性子命令，stderr 只许一行 JSON 信封，没有第二个地方可说"),
    ("src/backend/faces/read_face.rs", "let _ = BACKEND_LOG.set(path);", 1, Why::SetOnce, "〔GAP1〕`main.rs` 装上 stderr 诊断文件之后交一次"),
    ("src/backend/relay/listen.rs", "let _ = server::respond_and_drain(&mut s, server::BUSY, \"busy\");", 1, Why::DeadLink, "回一句「忙」给被拒的那条连接"),
    ("src/backend/relay/listen.rs", "let _ = server::respond_and_drain(&mut stream, server::BUSY, \"busy\");", 1, Why::DeadLink, "回一句「忙」给被拒的那条连接"),
    ("src/backend/relay/server.rs", "let _ = down.set_nonblocking(false);", 1, Why::DeadLink, "已经答完的那条连接上排掉已到的字节，排不掉就算了"),
    ("src/backend/relay/server.rs", "let _ = down.set_nonblocking(true);", 1, Why::DeadLink, "已经答完的那条连接上排掉已到的字节，排不掉就算了"),
    ("src/backend/relay/tee.rs", "let _ = self.port.offer(TapEvent { stream: id.stream.to_string(), resp: at.resp, n, body: TapBody::Data(payload.to_strin", 1, Why::Backpressure, "〔TAP〕投不进就丢：号照占，缺口在接收侧按号算得出（`20 §8`「SSE 保快、jsonl 保对」）"),
    ("src/backend/relay/tee.rs", "let _ = self.port.offer(TapEvent { stream: id.stream.to_string(), resp: at.resp, n: at.n, body: TapBody::End { broken },", 1, Why::Backpressure, "〔TAP〕同上（收尾那一件）"),
    // 〔DEL〕NDJSON 那一形的四行（写线程 · 两处 `write_all` · `flush`）随独立 `--relay` 删了。
    ("src/backend/assets/skill_ledger.rs", "let _ = std::fs::remove_file(&tmp);", 1, Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回，删不掉只剩一份临时件"),
    ("src/backend/stderr_log.rs", "let _ = f.write_all(roll_note(&self.old).as_bytes());", 1, Why::Diag, "「写不进去不拖垮后端」（`15 §4.7 S1` 脱离载体那一格）"),
    ("src/backend/stderr_log.rs", "let _ = self.fresh();", 1, Why::Diag, "「写不进去不拖垮后端」（`15 §4.7 S1` 脱离载体那一格）"),
    ("src/common/creds-core/src/perm.rs", "let _ = CloseHandle(token);", 1, Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/common/creds-core/src/perm.rs", "let _ = GetTokenInformation(token, TokenUser, None, 0, &mut need);", 1, Why::NotAnError, "第一次调用只为问缓冲区要多大，按约定一定回「缓冲区不够」"),
    ("src/common/creds-core/src/perm.rs", "let _ = LocalFree(HLOCAL(psd.0));", 5, Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/common/creds-core/src/perm.rs", "let _ = LocalFree(HLOCAL(s.0 as *mut core::ffi::c_void));", 2, Why::Reap, "Windows 句柄 / 内存释放"),
    // 〔SH1 · V136〕`cc_bus.rs` 那两行（本机 shell 读收尸）随那条读删了。
    ("src/bridge/src/backend/control/inbound_client.rs", "let _ = w.shutdown().await;", 1, Why::DeadLink, ""),
    ("src/bridge/src/backend/control/local_backend.rs", "let _ = c.kill();", 3, Why::Reap, ""),
    ("src/bridge/src/backend/control/local_backend.rs", "let _ = c.wait();", 1, Why::Reap, ""),
    // 〔E2〕3 → 2：逐字节副本那一处删了。
    ("src/bridge/src/backend/control/local_backend.rs", "let _ = std::fs::remove_file(&tmp);", 2, Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回，删不掉只剩一份临时件"),
    // 〔E2〕1 → 2：+1 收换版时挪开的旧 `ccm`（`.old`，Windows 上正在跑的删不掉）；删不掉下次放置时再清。
    ("src/bridge/src/backend/control/local_backend.rs", "let _ = std::fs::remove_file(ent.path());", 2, Why::CleanupAfterFailure, "清过期的释放半成品（`STALE_PARTIAL_AGE`）· 换版时挪开的旧 `ccm`；删不掉下次再清"),
    ("src/bridge/src/backend/control/local_backend.rs", "let _ = std::io::copy(&mut o, &mut std::io::sink());", 1, Why::Drain, ""),
    // 〔OSA · V156〕`launch_render/payload.rs::render_env_ops` 那四行 `let _ = write!(out, "export …")` 摘了：
    //   `export` / `unset` 的写法搬进 `platform::shell::posix`，那里返回 `String`，调用处 `push_str`，不再有吞。
    ("src/bridge/src/bind.rs", "let _ = CloseHandle(handle);", 2, Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/bridge/src/bind.rs", "let _ = CloseHandle(snap);", 1, Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/bridge/src/bind.rs", "let _ = EnumWindows(Some(cb), LPARAM(0));", 1, Why::NotAnError, "回调里自己收结果；回调提前停时它回 Err 是约定"),
    ("src/bridge/src/bind.rs", "let _ = GetWindowThreadProcessId(hwnd, Some(&mut cur_owner));", 1, Why::NotAnError, "要的是出参里的属主 pid，返回值（线程 id）用不上"),
    ("src/bridge/src/bind.rs", "let _ = ShowWindow(h, SW_RESTORE);", 1, Why::WindowBestEffort, ""),
    ("src/bridge/src/bind.rs", "let _ = std::fs::remove_file(&p);", 1, Why::CleanupAfterFailure, "撤死进程留下的登记文件；删不掉下次重扫再撤"),
    ("src/bridge/src/bind.rs", "let _ = std::fs::remove_file(await_file);", 3, Why::CleanupAfterFailure, "等待文件用完就删；删不掉只剩一份无主的等待文件，下次按身份再核"),
    ("src/bridge/src/bind.rs", "let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_pid)) };", 1, Why::NotAnError, "要的是出参里的属主 pid，返回值（线程 id）用不上"),
    ("src/bridge/src/ccm_probe.rs", "let _ = child.kill();", 1, Why::Reap, ""),
    ("src/bridge/src/ccm_probe.rs", "let _ = child.wait();", 1, Why::Reap, ""),
    ("src/bridge/src/chan/client.rs", "tx.send(Err(hop(0, \"read\", Reach::Unknown, HopFault::Dropped))).ok();", 1, Why::PeerGone, ""),
    ("src/bridge/src/chan/client.rs", "tx.send(Job { head, body: Vec::new(), written: None, }).await.ok();", 1, Why::PeerGone, ""),
    ("src/bridge/src/chan/client.rs", "tx.send(r).ok();", 1, Why::PeerGone, ""),
    ("src/bridge/src/chan/client.rs", "w.send(()).ok();", 1, Why::Signal, ""),
    ("src/bridge/src/chan/router.rs", "tx.send(out).await.ok();", 1, Why::PeerGone, ""),
    ("src/bridge/src/chan/router.rs", "write_frame(&mut wr, &Head::Denied, &[]).await.ok();", 1, Why::DeadLink, ""),
    ("src/bridge/src/chan/wire.rs", "rx.wait_for(|c| *c).await.ok();", 1, Why::Signal, ""),
    ("src/bridge/src/config.rs", "let _ = std::fs::remove_file(&tmp);", 1, Why::CleanupAfterFailure, "〔CFG1〕原子写的临时件：换名失败之后删它；主错误已在回"),
    ("src/bridge/src/filewin/scale.rs", "let _ = render_headless(&ctx, rows, screen, off);", 1, Why::NotAnError, "量渲染耗时，只要时间不要画出来的东西"),
    // 〔FILES2 · 第四波〕窗口那几问的答复送回等答的那一趟（它已收场 ⇒ 没人要）· 暂存件收尾删不掉交孤儿扫（不盖下载 / 复制本身的结局）。
    ("src/bridge/src/filewin/extract.rs", "tx.send(fresh).ok();", 1, Why::PeerGone, "解压撞名那一问的答复；等答的那一趟已收场就没人要"),
    ("src/bridge/src/filewin/cross_copy.rs", "tx.send(overwrite).ok();", 1, Why::PeerGone, "复制到另一台「盖不盖」那一问的答复；等答的那一趟已收场就没人要"),
    ("src/bridge/src/filewin/cross_copy.rs", "let _ = super::source::ask(line, &local, \"files-delete\", &serde_json::json!({ \"root\": staging, \"rel\": rel }), super::wri", 1, Why::CleanupAfterFailure, "清本机暂存件（成败都清）；删不掉只剩一份垃圾，不改复制本身的结局"),
    ("src/bridge/src/filewin/cross_copy.rs", "let _ = super::source::ask(line, &to, \"files-delete\", &serde_json::json!({ \"root\": bstaging, \"rel\": format!(\"{k}.part\") ", 1, Why::CleanupAfterFailure, "半路失败之后清 B 那头开过单的暂存件；主错误已在回，删不掉交那台的孤儿扫"),
    // 〔MOD〕`history.rs` 那一处（读整份会话末块交前端、前端已走）随那条命令退役。
    ("src/bridge/src/launch.rs", "let _ = child.wait();", 1, Why::Reap, ""),
    ("src/bridge/src/lib.rs", "let _ = AttachThreadInput(fg_thread, cur_thread, false);", 1, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = BringWindowToTop(h);", 1, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);", 1, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);", 1, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = ShowWindow(h, SW_RESTORE);", 1, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = ShowWindow(h, SW_SHOW);", 1, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = local_cache.record(sid, pid, bind_registry);", 1, Why::NotAnError, "`None` = 这个会话不是经 cc 起的 / 还没握手完（常态）；绑上了 `record` 自己记日志"),
    // 〔MIG-1〕F5 重放那两处裸 `let _ = handle.emit(…)`（容器 · 可重连）〔MIG-1 · ⑬〕随起停事件并进会话流删了（就绪点在流里原位交成品）。
    ("src/bridge/src/lib.rs", "let _ = w.set_focus();", 2, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = w.show();", 2, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = w.unminimize();", 2, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = win.set_focus();", 1, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = win.show();", 2, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = win.unminimize();", 2, Why::WindowBestEffort, ""),
    ("src/bridge/src/lib.rs", "let _ = window.set_focus();", 1, Why::WindowBestEffort, ""),
    ("src/bridge/src/link_mux.rs", "let _ = slot.tx.send(Piece::End(Some(copy_text(\"rsLinkMux.data.noCredit\", &[]))));", 1, Why::PeerGone, ""),
    ("src/bridge/src/link_mux.rs", "let _ = slot.tx.send(Piece::End(Some(why.to_string())));", 1, Why::PeerGone, ""),
    ("src/bridge/src/link_mux.rs", "let _ = slot.tx.send(Piece::End(error));", 1, Why::PeerGone, ""),
    ("src/bridge/src/local_backend_host.rs", "let _ = sock.set_read_timeout(None);", 1, Why::NotAnError, "这条 socket 下一行就转成非阻塞交给 tokio：`SO_RCVTIMEO` / `SO_SNDTIMEO` 对非阻塞读写不起作用，摘不掉也没有残留"),
    ("src/bridge/src/local_backend_host.rs", "let _ = sock.set_write_timeout(None);", 1, Why::NotAnError, "这条 socket 下一行就转成非阻塞交给 tokio：`SO_RCVTIMEO` / `SO_SNDTIMEO` 对非阻塞读写不起作用，摘不掉也没有残留"),
    ("src/bridge/src/logging.rs", "let _ = h.emit(ERROR_EVENT, p);", 1, Why::Diag, "把一条错误日志推给界面；推不上它照样进了日志文件"),
    ("src/bridge/src/spawn_managed.rs", "let _ = CloseHandle(job);", 2, Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/bridge/src/spawn_managed.rs", "let _ = windows::Win32::Foundation::CloseHandle(h);", 1, Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/bridge/src/ssh_source.rs", "let _ = frame_tx.send(Err(\"ssh backend stdout closed (EOF / connection dropped)\".to_string())).await;", 1, Why::PeerGone, ""),
    ("src/bridge/src/ssh_source.rs", "let _ = frame_tx.send(Err(format!(\"ssh backend stdout read error: {e}\"))).await;", 1, Why::PeerGone, ""),
    ("src/bridge/src/utils.rs", "let _ = std::fs::remove_file(&tmp);", 1, Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回，删不掉只剩一份临时件"),
    ("src/panorama-engine/main.rs", "let _ = std::io::stderr().write_all(stderr.as_bytes());", 1, Why::Diag, ""),
    ("src/panorama-engine/main.rs", "let _ = std::io::stdout().write_all(stdout.as_bytes());", 1, Why::Diag, ""),
    // 〔W5-AUX · `设计/96 §3.6`〕capture 带 stdin 那一形：写那一行失败时回一行失败的 ack；ack 本身写不出去 ⇒ 链路已死，同上面那几条。
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(copy_text(\"beUses.exec.stdinLost\", &[(\"e\", &e.to_string())])", 1, Why::DeadLink, ""),
    // 〔VIS2 · 09-26〕`agent_home` 的可重入挂法（`rewatch_agent_home`，`设计/15 §4.7 S3`）：同上面三个目录那一族。
    ("src/backend/observe/watcher.rs", "let _ = debouncer.watcher().unwatch(agent_home);", 2, Why::Reap, "撤旧 inode 上的 watch：目录被删 / 换过 inode 时 unwatch 本来就会失败"),
    ("src/backend/control/resident.rs", "let _ = std::fs::remove_file(&path);", 1, Why::Reap, "〔STOP〕停完之后收掉还指着它的那份 pid 记录：结局（graceful / killed）已经定了；删不掉只剩一份陈记录，下次认身份时 ESRCH / exe 对不上照样答对；一次性子命令，stderr 只许一行 JSON 信封"),
];

/// 键的长度上限（字符）。长语句（带一整句报错的 `write_stages_then_ack(…)`）截到这里就认得出。
const KEY_CHARS: usize = 120;

/// 人群的四棵根（仓根相对）。
const ROOTS: &[&str] = &[
    "src/backend",
    "src/bridge/src",
    "src/common",
    "src/panorama-engine",
];

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// 从 `i`（一个 `"` 或 `r"` / `r#"` 的起点）跳过整个字符串字面量，回字面量之后那一位。不是字面量起点 ⇒ `None`。
fn skip_string(s: &[u8], i: usize) -> Option<usize> {
    if s[i] == b'"' {
        let mut j = i + 1;
        while j < s.len() {
            match s[j] {
                b'\\' => j += 2,
                b'"' => return Some(j + 1),
                _ => j += 1,
            }
        }
        return Some(s.len());
    }
    if s[i] == b'r' && (i == 0 || !is_ident(s[i - 1])) {
        let mut j = i + 1;
        let mut hashes = 0;
        while j < s.len() && s[j] == b'#' {
            hashes += 1;
            j += 1;
        }
        if j < s.len() && s[j] == b'"' {
            let close: Vec<u8> = std::iter::once(b'"')
                .chain(std::iter::repeat_n(b'#', hashes))
                .collect();
            let body = &s[j + 1..];
            return Some(
                body.windows(close.len())
                    .position(|w| w == close.as_slice())
                    .map_or(s.len(), |p| j + 1 + p + close.len()),
            );
        }
    }
    None
}

/// 字符字面量（`';'` · `'\''` · `'\n'` · 一个多字节字符）：回它之后那一位；是生命周期 / 不是字面量 ⇒ `None`。
fn skip_char(s: &[u8], i: usize) -> Option<usize> {
    if s[i] != b'\'' || i + 1 >= s.len() {
        return None;
    }
    if s[i + 1] == b'\\' {
        return s[i + 2..]
            .iter()
            .position(|&b| b == b'\'')
            .map(|p| i + 2 + p + 1);
    }
    let len = match s[i + 1] {
        b if b < 0x80 => 1,
        b if b >= 0xF0 => 4,
        b if b >= 0xE0 => 3,
        _ => 2,
    };
    (s.get(i + 1 + len) == Some(&b'\'')).then_some(i + 1 + len + 1)
}

/// 字符串 / 字符字面量占的字节（`true` = 在字面量里）。
fn literal_mask(s: &[u8]) -> Vec<bool> {
    let mut mask = vec![false; s.len()];
    let mut i = 0;
    while i < s.len() {
        if let Some(j) = skip_string(s, i).or_else(|| skip_char(s, i)) {
            mask[i..j.min(s.len())].iter_mut().for_each(|x| *x = true);
            i = j;
        } else {
            i += 1;
        }
    }
    mask
}

/// 字符串字面量之外：空白压成一个空格、`.` `(` `)` `,` `;` 两侧的空格去掉、`,)` 收成 `)`；截到 [`KEY_CHARS`]。
fn normalize(stmt: &str) -> String {
    let s = stmt.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    let mut pending_space = false;
    while i < s.len() {
        if let Some(j) = skip_string(s, i).or_else(|| skip_char(s, i)) {
            if pending_space && !matches!(out.last(), Some(b'(' | b'.' | b' ') | None) {
                out.push(b' ');
            }
            pending_space = false;
            out.extend_from_slice(&s[i..j]);
            i = j;
            continue;
        }
        let b = s[i];
        if b.is_ascii_whitespace() {
            pending_space = true;
            i += 1;
            continue;
        }
        let tight = matches!(b, b'.' | b')' | b',' | b';');
        if pending_space && !tight && !matches!(out.last(), Some(b'(' | b'.') | None) {
            out.push(b' ');
        }
        pending_space = false;
        if b == b')' && out.last() == Some(&b',') {
            out.pop();
        }
        out.push(b);
        i += 1;
    }
    let full = String::from_utf8(out).expect("只在 ASCII 位置上切过");
    full.chars().take(KEY_CHARS).collect()
}

/// `let _ = <rhs>` 的 rhs 是不是「压未用参数」的形（标识符 / 元组 / 引用，没有调用）或以 `?` 结尾（错误照旧上抛）。
fn not_a_swallow(rhs: &str) -> bool {
    let r = rhs.trim().trim_end_matches(';').trim();
    if r.ends_with('?') {
        return true;
    }
    let bare = r.trim_start_matches(['&', '*']);
    let is_names = bare
        .bytes()
        .all(|b| is_ident(b) || matches!(b, b'(' | b')' | b',' | b' '));
    let has_call = bare
        .as_bytes()
        .windows(2)
        .any(|w| is_ident(w[0]) && w[1] == b'(');
    is_names && !has_call
}

/// 一份**生产文本**（已剥测试段与注释）里的裸吞，归一化之后的语句。
///
/// - `let _ =` 那一形：从它往后走到**同一层**的第一个 `;`（括号 / 花括号成对跳过、字面量里的不算）。
/// - `.ok();` 那一形：从它往回走到语句的起点 —— 同一层的上一个 `;`、所在块的 `{`、或上一条块语句收尾的 `}`
///   （`}` 后面紧跟 `.` 的是方法链的一部分，不算收尾）。
fn swallows(prod: &str) -> Vec<String> {
    let s = prod.as_bytes();
    let lit = literal_mask(s);
    let mut out = Vec::new();
    // ① `let _ = …;`
    let head = b"let _ =";
    for at in 0..s.len().saturating_sub(head.len()) {
        if &s[at..at + head.len()] != head || lit[at] || (at > 0 && is_ident(s[at - 1])) {
            continue;
        }
        let mut depth: i32 = 0;
        let mut end = None;
        for (q, &c) in s.iter().enumerate().skip(at + head.len()) {
            if lit[q] {
                continue;
            }
            match c {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b';' if depth == 0 => {
                    end = Some(q);
                    break;
                }
                _ => {}
            }
            if depth < 0 {
                break; // 走出了所在的块还没见到 `;` —— 不是一条完整语句
            }
        }
        let Some(end) = end else { continue };
        let stmt = &prod[at..=end];
        if not_a_swallow(&stmt[head.len()..]) {
            continue;
        }
        out.push(normalize(stmt));
    }
    // ② 语句级 `….ok();`
    let tail = b".ok();";
    for at in 0..(s.len() + 1).saturating_sub(tail.len()) {
        if &s[at..at + tail.len()] != tail || lit[at] {
            continue;
        }
        let mut depth: i32 = 0;
        let mut start = 0;
        for q in (0..at).rev() {
            if lit[q] {
                continue;
            }
            match s[q] {
                b')' | b']' => depth += 1,
                b'}' => {
                    let next = s[q + 1..at]
                        .iter()
                        .zip(&lit[q + 1..at])
                        .find(|(b, l)| !**l && !b.is_ascii_whitespace())
                        .map(|(b, _)| *b);
                    if depth == 0 && next != Some(b'.') {
                        start = q + 1;
                        break;
                    }
                    depth += 1;
                }
                b'(' | b'[' | b'{' => {
                    if depth == 0 {
                        start = q + 1;
                        break;
                    }
                    depth -= 1;
                }
                b';' if depth == 0 => {
                    start = q + 1;
                    break;
                }
                _ => {}
            }
        }
        let stmt = prod[start..at + tail.len()].trim();
        let folded = stmt.starts_with("let ")
            || stmt.starts_with("return ")
            || stmt.split_once('=').is_some_and(|(lhs, rhs)| {
                !rhs.starts_with('=')
                    && !lhs.ends_with(['!', '<', '>', '='])
                    && lhs
                        .trim()
                        .bytes()
                        .all(|b| is_ident(b) || b == b'.' || b == b'*')
            });
        if !folded {
            out.push(normalize(stmt));
        }
    }
    out
}

/// 测试专用模块（`#[cfg(test)]` 紧跟 `mod x;` 引进来的文件）：按 `mod` 声明派生，规范化成真实路径。
fn test_only_modules(files: &[(PathBuf, String)]) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    for (path, src) in files {
        let lines: Vec<&str> = src.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim();
            let Some(after) = t.strip_prefix("#[cfg(test)]") else {
                continue;
            };
            let mut path_attr: Option<String> = None;
            let mut cands: Vec<&str> = vec![after.trim()];
            cands.extend(lines.iter().skip(i + 1).take(5).map(|x| x.trim()));
            for c in cands {
                if c.is_empty() || c.starts_with("//") {
                    continue;
                }
                if let Some(p) = c
                    .strip_prefix("#[path = \"")
                    .and_then(|r| r.strip_suffix("\"]"))
                {
                    path_attr = Some(p.to_string());
                    continue;
                }
                if c.starts_with("#[") {
                    continue;
                }
                let decl = c
                    .trim_start_matches("pub(crate) ")
                    .trim_start_matches("pub ")
                    .strip_prefix("mod ")
                    .and_then(|r| r.split(';').next())
                    .filter(|n| !n.is_empty() && n.bytes().all(is_ident))
                    .filter(|_| c.ends_with(';') || c.contains(';'));
                if let Some(name) = decl {
                    let dir = path.parent().expect("文件有父目录");
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let targets: Vec<PathBuf> = match &path_attr {
                        Some(p) => vec![dir.join(p)],
                        None if matches!(stem, "lib" | "mod" | "main") => {
                            vec![
                                dir.join(format!("{name}.rs")),
                                dir.join(name).join("mod.rs"),
                            ]
                        }
                        None => vec![
                            dir.join(stem).join(format!("{name}.rs")),
                            dir.join(stem).join(name).join("mod.rs"),
                        ],
                    };
                    for t in targets {
                        if let Ok(real) = std::fs::canonicalize(&t) {
                            out.insert(real);
                        }
                    }
                }
                break;
            }
        }
    }
    out
}

/// 盘上的人群：`(仓根相对路径, 归一化语句) → 处数`。
fn population() -> BTreeMap<(String, String), usize> {
    let root = crate::guard_support::repo_root();
    let mut files = Vec::new();
    for sub in ROOTS {
        files.extend(guard_core::scan_tree_excluding(
            &root.join(sub),
            &["rs"],
            &[],
        ));
    }
    let test_only = test_only_modules(&files);
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let mut out: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (path, raw) in &files {
        let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if test_only.contains(&real) || !real.starts_with(&root) {
            continue;
        }
        let rel = real
            .strip_prefix(&root)
            .unwrap_or(&real)
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with("tests/") {
            continue; // `#[path]` 挂进来的测试文件不在四棵根里，这一格只是保险
        }
        for stmt in swallows(&guard_core::production_code(raw)) {
            *out.entry((rel.clone(), stmt)).or_insert(0) += 1;
        }
    }
    out
}

/// ① ★ **人群 == 登记表**（两向）。红的时候诊断直接印成登记表的行形，填上类与理由即可。
#[test]
fn w5vis_every_bare_swallow_in_production_is_registered_with_a_reason() {
    let disk = population();
    let mut table: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (f, stmt, n, _, _) in ALLOWED {
        let prev = table.insert((f.to_string(), stmt.to_string()), *n);
        assert!(prev.is_none(), "登记表里同一个键出现了两行：{f} · {stmt}");
    }
    let unlisted: Vec<String> = disk
        .iter()
        .filter(|(k, n)| table.get(*k) != Some(n))
        .map(|((f, s), n)| format!("    ({f:?}, {s:?}, {n}, Why::?, \"\"),"))
        .collect();
    let dead: Vec<String> = table
        .iter()
        .filter(|(k, n)| disk.get(*k) != Some(n))
        .map(|((f, s), n)| {
            format!(
                "    {f} · {s} · 登记 {n} 处，盘上 {} 处",
                disk.get(&(f.clone(), s.clone())).copied().unwrap_or(0)
            )
        })
        .collect();
    assert!(
        unlisted.is_empty() && dead.is_empty(),
        "生产代码里的裸吞与登记表对不上。\n\
         ★ 盘上有、表里没有（或处数不同）—— 要么别吞（说出来 / 往上抛），要么登记为什么可以丢（{}）：\n{}\n\
         ★ 表里有、盘上没有（修掉了 / 搬走了就删那一行）：\n{}",
        "`设计/15 §4.7 S5`：处置不是别吞，是吞了要留一行日志",
        unlisted.join("\n"),
        dead.join("\n")
    );
    // 每一行都要么有类的理由、要么别路那一类必须写清是哪一路。
    for (f, stmt, _, why, note) in ALLOWED {
        if *why == Why::OtherLane {
            assert!(
                !note.trim().is_empty(),
                "{f} · {stmt}：「别路在改」要写明是哪一路、合并时怎么处置"
            );
        }
    }
}

/// ② 量具正控：合成语料 —— 该认的恰好认出，不该认的一个不认（两向）。
#[test]
fn w5vis_the_swallow_detector_sees_exactly_the_bare_forms() {
    let synthetic = "fn a() {\n\
        let _ = foo();\n\
        let _ = pid;\n\
        let _ = (pid, name);\n\
        let _ = &perms;\n\
        let _ = origin.route(\"x\")?;\n\
        let _ = w\n            .send(err(\n                &id,\n                \"a; b\",\n            ))\n            .await;\n\
        baz().ok();\n\
        let y = q().ok();\n\
        z = r().ok();\n\
        return t().ok();\n\
        if let Ok(x) = u() { v(x, |k| { k.ok(); }).ok(); }\n\
        let s = \"let _ = not_code(); x.ok();\";\n\
    }\n";
    let got: BTreeSet<String> = swallows(synthetic).into_iter().collect();
    let want: BTreeSet<String> = [
        "let _ = foo();",
        "let _ = w.send(err(&id, \"a; b\")).await;",
        "baz().ok();",
        "v(x, |k| { k.ok(); }).ok();",
        "k.ok();",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(got, want, "量具认出的与手写期望不相等");
}

/// ③ 测试专用模块确实被排除：派生出来的那一组里有两份已知的、生产段里带 `let _ =` 的测试专用文件（正控）。
#[test]
fn w5vis_test_only_modules_are_derived_and_excluded() {
    let root = crate::guard_support::repo_root();
    let mut files = Vec::new();
    for sub in ROOTS {
        files.extend(guard_core::scan_tree_excluding(
            &root.join(sub),
            &["rs"],
            &[],
        ));
    }
    let derived = test_only_modules(&files);
    for known in [
        "src/backend/alloc_probe.rs",
        "src/bridge/src/needle_anchor_registry.rs",
    ] {
        let real = std::fs::canonicalize(root.join(known)).expect("已知的那份不在了 —— 改本条");
        assert!(
            derived.contains(&real),
            "`{known}` 是测试专用模块，却没被派生出来 —— 排除坏了"
        );
    }
    let pop = population();
    assert!(
        !pop.keys().any(|(f, _)| f == "src/backend/alloc_probe.rs"),
        "测试专用模块的语句进了人群"
    );
    assert!(
        !pop.is_empty(),
        "人群是空的 —— 扫描面坏了，① 那条会零命中地绿"
    );
}
