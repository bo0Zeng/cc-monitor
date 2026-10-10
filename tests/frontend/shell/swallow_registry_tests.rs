//! **业务路径零裸吞** —— 人群判据与登记表（设计与射程住 `src/frontend/shell/src/swallow_registry.rs` 头注）。
//!
//! （逐字）「**处置不是别吞，是吞了要留一行日志**」。
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
    /// （`stream/inbound/` 里 `cancel` 那一臂的头注：「丢一条 cancel 应答，远比堵死读循环便宜」）；调用方按自己的期限收场。
    Backpressure,
    /// 别路在改：这一处住在另一路的写区里，本路不碰；那一路合并时这一行要随之删 / 改。
    OtherLane,
}

/// **允许吞的，逐条登记**：`(文件, 语句原文归一化, 类, 补一句)` —— 同一文件里逐字相同的语句是同一个理由，一行就够。
///
/// 归一化 = 字符串字面量之外的空白压成一个空格、`.` `(` `)` `,` 两侧的空格去掉、`,)` 收成 `)`（rustfmt 换不换行不影响键）；
/// 超过 [`KEY_CHARS`] 个字符的截断。**不含行号**。
const ALLOWED: &[(&str, &str, Why, &str)] = &[
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] create that file to configure one; it is plain JSON:\\n{}\", store::template());", Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] credentials file: {}\", loaded.path.display());", Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] credentials permissions too wide: {how}\");", Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] credentials permissions unknown: {why}\");", Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] credentials problem: {p}\");", Why::Diag, "`announce` / `announce_unresolved` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] credentials: auth_style must be one of: {legal}\");", Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] credentials: configured, {rows} account(s) routable\");", Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] credentials: not configured\");", Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] credentials: this account cannot be used: {:?} - {}\", r.id, r.why);", Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] credentials: this account is not on the default path: {:?} - {}\", note.id, note.what);", Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/upstream_select/creds.rs", "let _ = writeln!(out, \"[apikey] how to fix: {fix}\");", Why::Diag, "`announce` 往它的诊断出口（stderr）印上游选择的状态行"),
    ("src/backend/accounts/manage/mcp_share_watch.rs", "let _ = live.kick.send(());", Why::PeerGone, "踢一下各号 MCP 的同步线程：线程不在了就没人要这一下（起不来那一次已经出声）"),
    ("src/backend/accounts/manage/mcp_share_watch.rs", "let _ = to_worker.send(());", Why::PeerGone, "文件事件转给同步线程：线程不在了就没人要这一下"),
    ("src/backend/platform/watch_file.rs", "let _ = tx.send(());", Why::PeerGone, "盯盘那一口把文件事件转给收的线程（轮换 · 计划共用）：线程不在了就没人要这一下"),
    ("src/backend/accounts/manage/mcp_share_watch.rs", "let _ = live.watcher.unwatch(gone);", Why::CleanupAfterFailure, "名单里离开的目录卸掉 watch：多半那个号的目录已经删了，卸不掉不改名单"),
    ("src/backend/accounts/manage/exec.rs", "let _ = door::link(d, &home, &self.rel(at)?, target);", Why::CleanupAfterFailure, "私有化自检没过：删掉落位的那一份之后把原来那条链接建回去；主错误已在回、备份里有原件"),
    ("src/backend/accounts/manage/exec.rs", "let _ = door::remove(d, &home, &self.rel(&tmp)?, false);", Why::CleanupAfterFailure, "导入凭据的临时件：改权限 / 换名失败之后删它；主错误已在回，删不掉只剩一份 0600 的临时件"),
    ("src/backend/accounts/manage/exec.rs", "let _ = door::remove(d, &home, &self.rel(at)?, dir);", Why::CleanupAfterFailure, "私有化自检没过：先删掉落位的那一份再建回链接；主错误已在回"),
    ("src/backend/accounts/manage/exec.rs", "let _ = door::remove(r.d, &r.r.home, &rel, dir);", Why::CleanupAfterFailure, "私有化的临时副本：复制期间源被改了 / 改权限失败之后删它；主错误已在回"),
    ("src/backend/common/own_state.rs", "let _ = std::fs::remove_file(&tmp);", Why::CleanupAfterFailure, "后端自有状态文件原子写的旁名（自己建的那一个）：写或换名失败之后删它；主错误已在回，删不掉只剩一份 0600 的旁名"),
    ("src/backend/accounts/quota/rotation.rs", "let _ = changes().send(sid.clone());", Why::NotAnError, "进程内「某会话变了」的广播：此刻没有流连接订它 ⇒ 发不出是正常的，盘上那份照旧在，客户端重问就有"),
    ("src/backend/plan/watch.rs", "let _ = changes().send((target.to_string_lossy().to_string(), now.0, now.1));", Why::NotAnError, "进程内「某工作区的计划变了」的广播：此刻没有流连接订它 ⇒ 发不出是正常的，计划在盘上，客户端重问就有"),
    ("src/backend/faces/plan_review_face.rs", "let _ = crate::plan::watch::changes().send((ws.to_string(), rev, n));", Why::NotAnError, "认可改了之后推一帧新的数：此刻没有流连接订它 ⇒ 发不出是正常的，客户端重问 `plan-read` 就有"),
    ("src/backend/accounts/quota/rotation.rs", "let _ = rules_changes().send(());", Why::NotAnError, "进程内「规则表变了」的广播：此刻没有流连接订它 ⇒ 发不出是正常的，盘上那份照旧在，客户端重问就有"),
    ("src/backend/control/cli_args.rs", "let _ = std::io::BufRead::fill_buf(&mut br);", Why::Signal, "读入参的静默窗只等「第一个字节（或 EOF）到了」这件事；读错不在这里报 —— 紧接着那一趟 `read_input` 读同一个读端会再拿到它、按 `stdin_read_failed` 报"),
    ("src/backend/control/cli_args.rs", "let _ = tx.send(Ev::Done(read_input(br, one_line)));", Why::PeerGone, "静默窗到点之后等的那一方已经回了 `no_input`、进程正要退：读线程这时才读完，没人要这个结果"),
    ("src/backend/stream/detail.rs", "let _ = writeln!(err, \"{line}\");", Why::Diag, "CLI 面的失败信封写 stderr（各 CLI 出口都经 `Failed::emit_to` 这一处）：写不进去就没有第二个地方可以说（退出码 2 照样回）"),
    ("src/backend/control/resolve_query.rs", "let _ = writeln!(out, \"{json}\");", Why::Diag, "`--resolve` 的成品写 stdout：写不进去 ⇒ 调用方已经走了（管道断），没有第二个地方可以说"),
    ("src/backend/control/resolve_query.rs", "let _ = writeln!(err, \"{}\", error_envelope(code, message));", Why::Diag, "`--resolve` 的失败信封写 stderr：写不进去就没有第二个地方可以说（退出码 2 照样回）"),
    ("src/backend/control/cli_control.rs", "let _ = writeln!(out, \"{}\", crate::common::time::fill_at(&line, tz));", Why::Diag, "CLI 面的应答写 stdout（JSON 那一形与 `--text` 那一形同一行）：写不进去 ⇒ 调用方已经走了（管道断），没有第二个地方可以说"),
    ("src/backend/control/launch_account.rs", "let _ = std::fs::remove_file(&path);", Why::CleanupAfterFailure, "清陈旧便条（进程不在 / pid 被复用）：删不掉下次认便条时再清，它对不上进程不会被认"),
    ("src/backend/control/files_commit.rs", "let _ = std::fs::remove_file(&at);", Why::CleanupAfterFailure, "写块失败 / 收拾旧块：主错误已在回；删不掉等孤儿扫（7 天）"),
    ("src/backend/control/files_commit.rs", "let _ = std::fs::remove_file(&side);", Why::CleanupAfterFailure, "跨盘提交抄写失败后删自己这一趟的旁名；主错误已在回"),
    ("src/backend/control/files_commit.rs", "let _ = std::fs::remove_file(&staged);", Why::CleanupAfterFailure, "目标已落好之后删暂存件；删不掉由孤儿扫（`sweep_stale`）按期限收（该处注释原话）"),
    ("src/backend/control/files_write.rs", "std::fs::remove_file(&bak).ok();", Why::CleanupAfterFailure, "写失败之后删自己这一趟建的旁名 / 半成品；主错误已在回"),
    // 解压落一份文件写一半 / 设权限失败 ⇒ 删自己 `O_EXCL` 刚建的那一份；上传块形拼暂存件失败 ⇒ 删自己刚建的 `<key>.part`。
    ("src/backend/control/files_extract.rs", "std::fs::remove_file(&at).ok();", Why::CleanupAfterFailure, "解压写一份失败之后删自己 `O_EXCL` 刚建的那一份；主错误已在回（随后整趟回滚）"),
    ("src/backend/control/files_upload_chunks.rs", "std::fs::remove_file(p).ok();", Why::CleanupAfterFailure, "拼暂存件失败之后删自己 `O_EXCL` 刚建的那一份；主错误已在回，块由 `drop_chunks` 收"),
    ("src/backend/control/files_write.rs", "std::fs::remove_file(&side).ok();", Why::CleanupAfterFailure, "写失败之后删自己这一趟建的旁名 / 半成品；主错误已在回"),
    ("src/backend/control/tmux_hook.rs", "let _ = crate::platform::signal::send_sigusr1(pid);", Why::PeerGone, "信号送不到 = 那个进程已不在；发之前有进程身份复核（做得好的对照组）"),
    ("src/backend/control/transfer.rs", "let _ = forward.await;", Why::Reap, "等进度转发任务收尾（它自己只往已结束的票上报数）"),
    ("src/backend/control/transfer.rs", "let _ = rf.shutdown().await;", Why::DeadLink, "关远端文件句柄；传输的结局已经定了"),
    ("src/backend/control/transfer.rs", "let _ = sftp::remove(s, &part).await;", Why::CleanupAfterFailure, "撤了 ⇒ 删暂存区里的半截（用户说了不要）；删不掉只是暂存区里多一份，孤儿扫会收"),
    ("src/backend/control/transfer.rs", "let _ = std::fs::remove_file(&at);", Why::CleanupAfterFailure, "DP1 之后只清**空的** `.part`（一个字节都没落）；清不掉只剩一个 0 字节文件，主错误已在回"),
    ("src/backend/dial/link.rs", "let _ = replies.send(Frame::LinkEnd { link: link.clone(), error, }).await;", Why::PeerGone, ""),
    ("src/backend/dial/probe.rs", "let _ = up_w.shutdown().await;", Why::DeadLink, "测试连接探完关上行写半边（结局已定，这条探活链路随即整条丢掉）"),
    ("src/backend/dial/pool.rs", "let _ = self.freed.set(freed);", Why::SetOnce, "族的铃只挂一次"),
    ("src/backend/dial/sftp.rs", "let _ = s.sftp().remove_file(b).await;", Why::CleanupAfterFailure, "部署换名成功后删自己挪走的旧备份件 / 换名失败而落点已被别的部署者占上时删它；删不掉只剩一份 `.bak`"),
    ("src/backend/dial/sftp.rs", "let _ = s.sftp().rename(b, rel.clone()).await;", Why::CleanupAfterFailure, "换名失败、落点还空着 ⇒ 把自己挪走的旧文件挪回去（主错误已在回）"),
    ("src/backend/dial/sftp.rs", "let _ = tokio::io::copy(&mut (&mut *input).take(size), &mut tokio::io::sink()).await;", Why::Drain, ""),
    ("src/backend/dial/sftp.rs", "let _ = write_line(out, &refused(\"-\", \"bad_request\", &e.said, e.raw.as_deref())).await;", Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = out.flush().await;", Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = self.close().await;", Why::Reap, "放弃 / 提前收工时向远端发关通道：对端撤活只是尽力"),
    ("src/backend/dial/uses.rs", "let _ = write_line(out, &got).await;", Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(crate::common::contract::malformed(\"use=capture without `cap", Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(crate::common::contract::malformed(\"use=forward without `for", Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;", Why::DeadLink, ""),
    // 开 sftp 子系统没成那一条（同上：写不进去说明界面已经走了；SFTP 那一下的原话先进了日志）。
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp).because(stages.why())).await;", Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(Said::with_raw(copy_text(\"beUses.exec.failed\", &[]), &e), fp", Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(Said::with_raw(copy_text(\"beUses.forward.bindFailed\", &[ (\"p", Why::DeadLink, ""),
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &fail(e)).await;", Why::DeadLink, ""),
    ("src/backend/files/browse_watch.rs", "let _ = self.inner.unwatch(&super::raw::to_path_buf(d));", Why::Reap, "撤不再看的目录的 watch：目录已删时 unwatch 本来就会失败"),
    ("src/backend/files/mod.rs", "let _ = write!(out, \"{b:02x}\");", Why::InfallibleWrite, "`out` 是 `String`（十六进制摘要）"),
    ("src/backend/stream/inbound/mod.rs", "let _ = gate_rx.await;", Why::Signal, ""),
    ("src/backend/stream/inbound/mod.rs", "let _ = gate_tx.send(());", Why::Signal, ""),
    ("src/backend/stream/inbound/mod.rs", "let _ = replies.send(frame).await;", Why::PeerGone, ""),
    ("src/backend/stream/inbound/mod.rs", "let _ = replies.try_send(Frame::Cancelled { id: target });", Why::Backpressure, ""),
    ("src/backend/stream/inbound/mod.rs", "let _ = replies.try_send(Frame::err(&req.id, \"not_cancellable\", &copy_text(\"beInbound.dispatch.cannotCancel\", &[])));", Why::Backpressure, ""),
    ("src/backend/stream/inbound/mod.rs", "let _ = replies.try_send(Frame::ok(&req.id));", Why::Backpressure, ""),
    ("src/backend/stream/inbound/mod.rs", "let _ = replies_sup.send(Frame::err(&id_sup, \"handler_panicked\", &copy_text(\"beInbound.spawnHandler.crashed\", &[]))).awa", Why::PeerGone, ""),
    // 多客户：流结束那一格带上连接号（原 `done.send(())`）；认证通过的连接交主循环 —— 主循环不在了 = 进程在收尾。
    ("src/backend/main.rs", "let _ = done.send(id).await;", Why::Signal, ""),
    ("src/backend/main.rs", "let _ = attached.send(Attached { reader: r, writer: w, hello_flushed, flags: flags.unwrap_or(None), }).await;", Why::PeerGone, ""),
    ("src/backend/main.rs", "let _ = listen::write_line(&mut w, &listen::refusal_line(reason)).await;", Why::DeadLink, ""),
    ("src/backend/observe/one_wait.rs", "let _ = t.send(());", Why::PeerGone, "一次性等待到了：等的那一侧可能已经撤了（撤单 / 期限到），没人要这一声"),
    ("src/backend/observe/watcher.rs", "let _ = debouncer.watcher().unwatch(dir);", Why::Reap, "撤旧 inode 上的 watch：目录被删 / 换过 inode 时 unwatch 本来就会失败"),
    ("src/backend/observe/watcher.rs", "let _ = debouncer.watcher().unwatch(gone);", Why::Reap, "账号目录里不在了的号目录撤掉 watch：目录已删时 unwatch 本来就会失败"),
    ("src/backend/observe/watcher.rs", "let _ = debouncer.watcher().unwatch(sessions);", Why::Reap, "撤旧 inode 上的 watch：目录被删 / 换过 inode 时 unwatch 本来就会失败"),
    ("src/backend/observe/watcher.rs", "let _ = debouncer.watcher().unwatch(sock_dir);", Why::Reap, "撤旧 inode 上的 watch：目录被删 / 换过 inode 时 unwatch 本来就会失败"),
    ("src/backend/observe/watcher.rs", "let _ = self.0.send(WatchEvent::Notify(event));", Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = self.0.send(WatchEvent::Poke);", Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = self.0.send(WatchEvent::Shutdown);", Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = tx.send(WatchEvent::TmuxObserved(run_tmux_probe()));", Why::PeerGone, ""),
    // +2：对齐的应答发回等它的那一方（它可能已退）· 对每份 watcher 发对齐（那份可能正在退出；它丢了应答端，`resync` 不会挂住）。
    ("src/backend/observe/watcher.rs", "let _ = done.send(got);", Why::PeerGone, ""),
    // +1：SIGUSR1 戳名单上的每一份（`PokeSlot` 并进来；那一份可能正在退出）。
    ("src/backend/observe/watcher.rs", "let _ = w.send(WatchEvent::Poke);", Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = w.send(WatchEvent::Resync { only: only.map(str::to_string), done: tx.clone(), });", Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = tx.send(WatchEvent::TmuxProbeDue);", Why::PeerGone, ""),
    ("src/backend/observe/watcher.rs", "let _ = tx.send(target.death_event(pid));", Why::PeerGone, ""),
    ("src/backend/platform/signal.rs", "let _ = t.recv().await;", Why::Signal, "装不上 SIGTERM 时退回只等 SIGINT（从 `main.rs` 下沉来）"),
    ("src/backend/platform/signal.rs", "let _ = tokio::signal::ctrl_c().await;", Why::Signal, "从 `main.rs` 下沉来的停机信号监听"),
    ("src/backend/control/terminal_follow.rs", "let _ = f.tx.send(Ev::Stop);", Why::PeerGone, "退订 / 连接走：那张票的订阅线程已经自己停了（终端没了、推了收尾帧）⇒ 「停」没人收"),
    ("src/backend/control/terminal_follow.rs", "let _ = self.replies.blocking_send(Frame::TerminalFollowEnd { ticket: self.ticket.clone(), why, said: end_said(why), });", Why::PeerGone, "订它的那条流连接已经走了 ⇒ 收尾帧没人收（票表随连接一起丢）"),
    ("src/backend/control/terminal_follow.rs", "let _ = tx.send(Ev::Closed);", Why::PeerGone, "订阅线程已经停了（退订 / 推了收尾帧）⇒ 「客户端退了」没人收"),
    ("src/backend/platform/child.rs", "let _ = self.child.wait();", Why::Reap, "长寿子进程放手：杀组之后收尸；收不了也没有更好的补救（组已经杀了）"),
    ("src/backend/platform/child.rs", "let _ = child.wait();", Why::Reap, "Windows 臂只等直接子进程退出（句柄占着进程号）；退出码由随后那一下 `wait` 取"),
    ("src/backend/platform/child.rs", "let _ = done_tx.send(status.map(|s| Output { status: s, stdout: o, stderr: e, }));", Why::PeerGone, "发起方过了期限已放手、回了超时，这份结果没人收"),
    ("src/backend/platform/child.rs", "let _ = ev_tx.send(Ev::GaveUp);", Why::PeerGone, "等待线程已收尾退出 ⇒ 放手那一声没人收"),
    ("src/backend/platform/child.rs", "let _ = piped_tx.send(Ev::Piped(o, e));", Why::PeerGone, "等待线程已按放手收尾 ⇒ 读完的输出没人收"),
    ("src/backend/platform/child.rs", "let _ = r.read_to_end(&mut buf);", Why::Drain, "读子进程输出到底；读错只少尾巴几个字节，退出码与超时照旧判"),
    ("src/backend/control/resident.rs", "let _ = log_dir_chain(&h);", Why::Diag, "诊断文件那层目录建不了 ⇒ 子进程装不上 stderr 文件、照旧 null（「写不进去不拖垮后端」）；这是一次性子命令，stderr 只许一行 JSON 信封，没有第二个地方可说"),
    ("src/backend/faces/read_face.rs", "let _ = BACKEND_LOG.set(path);", Why::SetOnce, "`main.rs` 装上 stderr 诊断文件之后交一次"),
    ("src/backend/relay/listen.rs", "let _ = comms_outward::refuse_busy(&mut s);", Why::DeadLink, "回一句「忙」给被拒的那条连接"),
    ("src/backend/relay/listen.rs", "let _ = comms_outward::refuse_busy(&mut stream);", Why::DeadLink, "回一句「忙」给被拒的那条连接"),
    ("src/comms/outward/server.rs", "let _ = down.set_nonblocking(false);", Why::DeadLink, "已经答完的那条连接上排掉已到的字节，排不掉就算了"),
    ("src/comms/outward/server.rs", "let _ = down.set_nonblocking(true);", Why::DeadLink, "已经答完的那条连接上排掉已到的字节，排不掉就算了"),
    ("src/comms/outward/tee.rs", "let _ = self.port.offer(TapEvent { stream: id.stream.to_string(), owner: id.owner.to_string(), resp: at.resp, n, body, }", Why::Backpressure, "投不进就丢：号照占，缺口在接收侧按号算得出（「SSE 保快、jsonl 保对」）"),
    ("src/comms/outward/tee.rs", "let _ = self.port.offer(TapEvent { stream: id.stream.to_string(), owner: id.owner.to_string(), resp: at.resp, n: at.n, b", Why::Backpressure, "同上（收尾那一件）"),
    // NDJSON 那一形的四行（写线程 · 两处 `write_all` · `flush`）随独立 `--relay` 删了。
    ("src/backend/stderr_log.rs", "let _ = f.write_all(roll_note(&self.old).as_bytes());", Why::Diag, "「写不进去不拖垮后端」（脱离载体那一格）"),
    ("src/backend/stderr_log.rs", "let _ = self.fresh();", Why::Diag, "「写不进去不拖垮后端」（脱离载体那一格）"),
    ("src/backend/stderr_log.rs", "let _ = append_line(p, &self.0);", Why::Diag, "一次性模式的诊断行：写不进诊断文件也不许回落 stderr（stderr 是协议通道），丢这一行、命令照常"),
    ("src/common/creds-core/src/perm.rs", "let _ = CloseHandle(token);", Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/common/creds-core/src/perm.rs", "let _ = GetTokenInformation(token, TokenUser, None, 0, &mut need);", Why::NotAnError, "第一次调用只为问缓冲区要多大，按约定一定回「缓冲区不够」"),
    ("src/common/creds-core/src/perm.rs", "let _ = LocalFree(HLOCAL(psd.0));", Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/common/creds-core/src/perm.rs", "let _ = LocalFree(HLOCAL(s.0 as *mut core::ffi::c_void));", Why::Reap, "Windows 句柄 / 内存释放"),
    // `cc_bus.rs` 那两行（本机 shell 读收尸）随那条读删了。
    ("src/frontend/shell/src/inbound_client.rs", "let _ = w.shutdown().await;", Why::DeadLink, ""),
    ("src/frontend/shell/src/local_backend.rs", "let _ = c.kill();", Why::Reap, ""),
    ("src/frontend/shell/src/local_backend.rs", "let _ = c.wait();", Why::Reap, ""),
    ("src/frontend/shell/src/local_backend.rs", "let _ = std::fs::remove_file(&tmp);", Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回，删不掉只剩一份临时件"),
    ("src/frontend/shell/src/local_backend.rs", "let _ = std::fs::remove_file(ent.path());", Why::CleanupAfterFailure, "清过期的释放半成品（`STALE_PARTIAL_AGE`）· 换版时挪开的旧 `ccm`；删不掉下次再清"),
    ("src/frontend/shell/src/local_backend.rs", "let _ = std::io::copy(&mut o, &mut std::io::sink());", Why::Drain, ""),
    // 载荷那一层渲 env 那四行 `let _ = write!(out, "export …")` 摘了：`export` / `unset` 的写法搬进 `platform::shell::posix`
    //   （返回 `String`、调用处 `push_str`，不再有吞）；载荷那一层后来整层删了。
    // 下面六行随 `bind.rs` 的 Win32 读法搬进 `platform/{pid,hwnd}.rs`（处数不变）。
    ("src/frontend/shell/src/platform/pid.rs", "let _ = CloseHandle(handle);", Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/frontend/shell/src/platform/pid.rs", "let _ = CloseHandle(snap);", Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/frontend/shell/src/platform/hwnd.rs", "let _ = EnumWindows(Some(cb), LPARAM(0));", Why::NotAnError, "回调里自己收结果；回调提前停时它回 Err 是约定"),
    ("src/frontend/shell/src/platform/hwnd.rs", "let _ = GetWindowThreadProcessId(hwnd, Some(&mut cur_owner));", Why::NotAnError, "要的是出参里的属主 pid，返回值（线程 id）用不上"),
    ("src/frontend/shell/src/platform/hwnd.rs", "let _ = ShowWindow(h, SW_RESTORE);", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/bind.rs", "let _ = std::fs::remove_file(&p);", Why::CleanupAfterFailure, "撤死进程留下的登记文件；删不掉下次重扫再撤"),
    ("src/frontend/shell/src/bind.rs", "let _ = std::fs::remove_file(file);", Why::CleanupAfterFailure, "bash / zsh 那一份记录用完 / 认不出 / 作废就删；删不掉下一个 monitor 起来按身份再核、再删"),
    ("src/frontend/shell/src/platform/hwnd.rs", "let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_pid)) };", Why::NotAnError, "要的是出参里的属主 pid，返回值（线程 id）用不上"),
    ("src/frontend/shell/src/platform/hwnd.rs", "let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };", Why::NotAnError, "按属主筛的那一遍：同上，要的是出参里的属主 pid"),
    ("src/frontend/shell/src/platform/hwnd.rs", "let _ = EnumWindows(Some(cb), LPARAM(pid as isize));", Why::NotAnError, "回调里自己收结果、从不提前停；枚举失败 ⇒ 收到的是空表，调用方照「没有窗口」说"),
    ("src/frontend/shell/src/ccm_probe.rs", "let _ = child.kill();", Why::Reap, ""),
    ("src/frontend/shell/src/ccm_probe.rs", "let _ = child.wait();", Why::Reap, ""),
    // `capture_full` 交入参那条写线程：对面不读 stdin 就退了 ⇒ 写端断，结局由它的退出码与 stderr 说；等那条线程收尾同理。
    ("src/frontend/shell/src/ccm_probe.rs", "let _ = w.write_all(&bytes);", Why::DeadLink, ""),
    ("src/frontend/shell/src/ccm_probe.rs", "let _ = w.join();", Why::Signal, ""),
    ("src/comms/inward/chan/client.rs", "tx.send(Err(hop(0, \"read\", Reach::Unknown, HopFault::Dropped))).ok();", Why::PeerGone, ""),
    ("src/comms/inward/chan/client.rs", "tx.send(Job { head, body: Vec::new(), written: None, }).await.ok();", Why::PeerGone, ""),
    ("src/comms/inward/chan/client.rs", "tx.send(r).ok();", Why::PeerGone, ""),
    ("src/comms/inward/chan/client.rs", "w.send(()).ok();", Why::Signal, ""),
    ("src/comms/inward/chan/router.rs", "tx.send(out).await.ok();", Why::PeerGone, ""),
    ("src/frontend/filewin/src/proc.rs", "let _ = err.write_all(encode_ready(r).as_bytes()).and_then(|()| err.flush());", Why::DeadLink, "就绪那一行写不出去 = 起它的 monitor 已经不在了（stderr 管子断了），没人收"),
    ("src/comms/inward/chan/wire.rs", "rx.wait_for(|c| *c).await.ok();", Why::Signal, ""),
    ("src/frontend/shell/src/config.rs", "let _ = std::fs::remove_file(&tmp);", Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回"),
    // 窗口那几问的答复送回等答的那一趟（它已收场 ⇒ 没人要）· 暂存件收尾删不掉交孤儿扫（不盖下载 / 复制本身的结局）。
    ("src/frontend/filewin/src/extract.rs", "tx.send(fresh).ok();", Why::PeerGone, "解压撞名那一问的答复；等答的那一趟已收场就没人要"),
    ("src/frontend/filewin/src/cross_copy.rs", "tx.send(overwrite).ok();", Why::PeerGone, "复制到另一台「盖不盖」那一问的答复；等答的那一趟已收场就没人要"),
    ("src/frontend/filewin/src/cross_copy.rs", "let _ = super::source::ask(line, &local, \"files-delete\", &serde_json::json!({ \"root\": staging, \"rel\": rel }), super::wri", Why::CleanupAfterFailure, "清本机暂存件（成败都清）；删不掉只剩一份垃圾，不改复制本身的结局"),
    ("src/frontend/filewin/src/cross_copy.rs", "let _ = super::source::ask(line, &to, \"files-delete\", &serde_json::json!({ \"root\": bstaging, \"rel\": format!(\"{k}.part\") ", Why::CleanupAfterFailure, "半路失败之后清 B 那头开过单的暂存件；主错误已在回，删不掉交那台的孤儿扫"),
    // `history.rs` 那一处（读整份会话末块交前端、前端已走）随那条命令退役。
    ("src/frontend/shell/src/platform/terminal.rs", "let _ = child.wait();", Why::Reap, ""), // 原 `launch.rs`
    // 下面九行原住 `lib.rs`（主窗口拉前 · 单实例回调）：随两个平台臂搬进 `platform/window.rs`，处数不变。
    ("src/frontend/shell/src/platform/window.rs", "let _ = AttachThreadInput(fg_thread, cur_thread, false);", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/platform/window.rs", "let _ = BringWindowToTop(h);", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/platform/window.rs", "let _ = SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/platform/window.rs", "let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/platform/window.rs", "let _ = ShowWindow(h, SW_RESTORE);", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/platform/window.rs", "let _ = ShowWindow(h, SW_SHOW);", Why::WindowBestEffort, ""),
    // F5 重放那两处裸 `let _ = handle.emit(…)`（容器 · 可重连）随起停事件并进会话流删了（就绪点在流里原位交成品）。
    ("src/frontend/shell/src/lib.rs", "let _ = w.set_focus();", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/lib.rs", "let _ = w.show();", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/lib.rs", "let _ = w.unminimize();", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/lib.rs", "let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational));", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/platform/window.rs", "let _ = win.show();", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/platform/window.rs", "let _ = win.unminimize();", Why::WindowBestEffort, ""),
    // 单实例回调那三行收进 `platform::window::raise_main`（与点通知共用，失败各留一行日志），这里不再有。
    ("src/frontend/shell/src/lib.rs", "let _ = window.set_focus();", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/link_mux.rs", "let _ = slot.tx.send(Piece::End(Some(copy_core::backend_old(&copy_core::local_machine()))));", Why::PeerGone, ""),
    ("src/frontend/shell/src/link_mux.rs", "let _ = slot.tx.send(Piece::End(Some(why.to_string())));", Why::PeerGone, ""),
    ("src/frontend/shell/src/link_mux.rs", "let _ = slot.tx.send(Piece::End(error));", Why::PeerGone, ""),
    ("src/frontend/shell/src/logging.rs", "let _ = h.emit(crate::ui_error::EVENT, p);", Why::Diag, "把一条要让用户知道的出错推给界面；推不上它照样进了日志文件"),
    // 下面两行随 Job Object 那一段搬进 `platform/spawn.rs`（处数不变）。
    ("src/frontend/shell/src/platform/spawn.rs", "let _ = CloseHandle(job);", Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/frontend/shell/src/platform/spawn.rs", "let _ = windows::Win32::Foundation::CloseHandle(h);", Why::Reap, "Windows 句柄 / 内存释放"),
    ("src/frontend/shell/src/stream_source/run.rs", "let _ = frame_tx.send(Err(\"ssh backend stdout closed (EOF / connection dropped)\".to_string())).await;", Why::PeerGone, ""),
    ("src/frontend/shell/src/stream_source/run.rs", "let _ = frame_tx.send(Err(format!(\"ssh backend stdout read error: {e}\"))).await;", Why::PeerGone, ""),
    ("src/common/host-core/src/atomic.rs", "let _ = std::fs::remove_file(&tmp);", Why::CleanupAfterFailure, "原子写的临时件：换名失败之后删它；主错误已在回，删不掉只剩一份临时件"),
    // capture 带 stdin 那一形：写那一行失败时回一行失败的 ack；ack 本身写不出去 ⇒ 链路已死，同上面那几条。
    ("src/backend/dial/uses.rs", "let _ = write_stages_then_ack(out, stages, &DialAck::failed(Said::with_raw(copy_text(\"beUses.exec.stdinLost\", &[]), &e),", Why::DeadLink, ""),
    // `agent_home` 的可重入挂法（`rewatch_agent_home`）：同上面三个目录那一族。
    ("src/backend/observe/watcher.rs", "let _ = debouncer.watcher().unwatch(agent_home);", Why::Reap, "撤旧 inode 上的 watch：目录被删 / 换过 inode 时 unwatch 本来就会失败"),
    ("src/backend/control/resident.rs", "let _ = tokio::io::copy(&mut down, &mut stdout).await;", Why::DeadLink, "小中继下行：套接字那头走了 / stdout 写不出去（ssh 通道关了），两种都是这一趟结束，退出就是交代"),
    ("src/backend/control/resident.rs", "let _ = tokio::io::AsyncWriteExt::flush(&mut stdout).await;", Why::DeadLink, "小中继收尾 flush：ssh 通道已经关了就写不出去，退出就是交代"),
    ("src/backend/control/resident.rs", "let _ = std::fs::remove_file(&rec);", Why::Reap, "升级那一跳停完旧版之后收掉它的旧记录：结局已经写进日志；删不掉只剩一份陈记录，下一个新版起来会再核一次身份（ESRCH / exe 对不上照样答对）"),
    ("src/backend/control/resident.rs", "let _ = std::fs::remove_file(data_home.join(relay_route_core::LEGACY_LISTEN_TOKEN_NAME));", Why::Reap, "升级那一跳顺手收掉旧版的钥匙文件：新版不读它，删不掉只是留一个没人用的文件"),
    ("src/backend/control/resident.rs", "let _ = std::fs::remove_file(&path);", Why::Reap, "停完之后收掉还指着它的那份 pid 记录：结局（graceful / killed）已经定了；删不掉只剩一份陈记录，下次认身份时 ESRCH / exe 对不上照样答对；一次性子命令，stderr 只许一行 JSON 信封"),
];

/// 键的长度上限（字符）。长语句（带一整句报错的 `write_stages_then_ack(…)`）截到这里就认得出。
const KEY_CHARS: usize = 120;

/// 人群的四棵根（仓根相对）。
const ROOTS: &[&str] = &["src/backend", "src/frontend/shell/src", "src/common"];

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
    // 壳那棵根的人群声明带进来的兄弟包（`host-core` · `comms-inward` …）也住别的根下（`src/common` · `src/comms`）：同一份只数一次。
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.dedup_by(|a, b| a.0 == b.0);
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
    let mut table: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    for (f, stmt, _, _) in ALLOWED {
        let fresh = table.insert((f.to_string(), stmt.to_string()));
        assert!(fresh, "登记表里同一个键出现了两行：{f} · {stmt}");
    }
    let unlisted: Vec<String> = disk
        .keys()
        .filter(|k| !table.contains(*k))
        .map(|(f, s)| format!("    ({f:?}, {s:?}, Why::?, \"\"),"))
        .collect();
    let dead: Vec<String> = table
        .iter()
        .filter(|k| !disk.contains_key(*k))
        .map(|(f, s)| format!("    {f} · {s}"))
        .collect();
    assert!(
        unlisted.is_empty() && dead.is_empty(),
        "生产代码里的裸吞与登记表对不上。\n\
         ★ 盘上有、表里没有 —— 要么别吞（说出来 / 往上抛），要么登记为什么可以丢（{}）：\n{}\n\
         ★ 表里有、盘上没有（修掉了 / 搬走了就删那一行）：\n{}",
        "：处置不是别吞，是吞了要留一行日志",
        unlisted.join("\n"),
        dead.join("\n")
    );
    // 每一行都要么有类的理由、要么别路那一类必须写清是哪一路。
    for (f, stmt, why, note) in ALLOWED {
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
    // 壳那棵根的人群声明带进来的兄弟包（`host-core` · `comms-inward` …）也住别的根下（`src/common` · `src/comms`）：同一份只数一次。
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.dedup_by(|a, b| a.0 == b.0);
    let derived = test_only_modules(&files);
    for known in [
        "src/backend/alloc_probe.rs",
        "src/frontend/shell/src/needle_anchor_registry.rs",
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
