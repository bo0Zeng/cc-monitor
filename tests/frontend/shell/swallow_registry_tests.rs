//! **业务路径零裸吞** —— 人群判据与登记表（设计与射程住 `tests/frontend/shell/swallow_registry.rs` 头注）。
//!
//! （逐字）「**处置不是别吞，是吞了要留一行日志**」。
//!
//! 三条：① 人群 == 登记表（两向：没登记的新裸吞 · 修掉了还留着的死行）② 量具正控（合成语料：该认的认出、不该认的不认）。

use std::collections::{BTreeMap, BTreeSet};

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
    /// 别处在改：这一处由另一件改动收，那一件进来时这一行要随之删 / 改。
    OtherLane,
}

/// **允许吞的，逐条登记**：`(文件, 语句原文归一化, 类, 补一句)` —— 同一文件里逐字相同的语句是同一个理由，一行就够。
///
/// 归一化 = 字符串字面量之外的空白压成一个空格、`.` `(` `)` `,` 两侧的空格去掉、`,)` 收成 `)`（rustfmt 换不换行不影响键）；
/// 超过 [`KEY_CHARS`] 个字符的截断。**不含行号**。
const ALLOWED: &[(&str, &str, Why, &str)] = &[
    ("src/backend/platform/local_tz.rs", "#[cfg(unix)] let link = crate::platform::fs::Ro::read_link(\"/etc/localtime\").ok();", Why::NotAnError, "这台的 /etc/localtime 不是链（没有 · 是一份拷贝）是常态：说不出时区 ⇒ 调用方写「时区不明」"),
    ("src/backend/platform/watch_file.rs", "let _ = events.send(Some(p));", Why::PeerGone, "盯盘那一口把文件事件转给收的线程（轮换 · 计划 · 一次性等待 · 各号同步 · 浏览保鲜共用）：线程不在了就没人要这一下"),
    ("src/backend/platform/watch_file.rs", "let _ = self.tx.send(None);", Why::PeerGone, "踢收的线程一下（各号 MCP 同步做完加号 / 建库之后）：线程不在了就没人要这一下"),
    ("src/backend/host/managed/attach.rs", "let _ = qtx.send(l);", Why::PeerGone, "瘦客户端底行那一句转给接着的那一趟：那一趟已经断开 ⇒ 没人要这一句"),
    ("src/backend/platform/watch_file.rs", "let _ = self.inner.unwatch(&gone.0);", Why::Reap, "名单里离开的目录卸掉 watch：目录已删时 unwatch 本来就会失败，名单照样摘"),
    ("src/backend/platform/fs/own.rs", "let _ = std::fs::remove_file(&tmp);", Why::CleanupAfterFailure, "后端自有状态文件原子写的旁名（自己建的那一个）：写或换名失败之后删它；主错误已在回，删不掉只剩一份 0600 的旁名"),
    ("src/backend/control/machine_state.rs", "let _ = changes().send((machine.to_string(), p.clone()));", Why::NotAnError, "进程内「某台的状态成品变了」的广播：此刻没有流连接订它 ⇒ 发不出是正常的，表在进程里，客户端重问 `machine-read` 就有"),
    ("src/backend/control/machine_state.rs", "let _ = changes().send(p);", Why::NotAnError, "同上（机器表改了之后每台现推的那一份）"),
    ("src/backend/control/place_self.rs", "let _ = own.delete_file(&p);", Why::CleanupAfterFailure, "自报不是这一版的暂存件当场删：主失败已经在回（`not_self`），删不掉只剩一份残件，下一趟部署按陈旧残件清"),
    ("src/backend/history/needs_changed.rs", "let _ = changes().send(sid);", Why::PeerGone, "敲需手动的铃：此刻没有 watcher 在订（一次性子命令 · 判据）⇒ 没人收（下一个连上来的照常重问）"),
    ("src/backend/dial/opened_windows.rs", "let _ = self.0.delete_file(&f);", Why::CleanupAfterFailure, "清陈旧的窗口标签记录（那个 shell 不在 / 进程号被复用）：删不掉下一次 ↗ 再清，它对不上进程不会被认"),
    ("src/backend/platform/fs/face.rs", "at::unlink(&to.dir, &to.name, 0).ok();", Why::CleanupAfterFailure, "`link ＋ unlink` 改名时 `unlink` 失败 ⇒ 撤掉刚建的那个名字；主错误（`unlink` 那一下）已在回"),
    ("src/backend/control/tmux_hook.rs", "let _ = crate::platform::signal::send_sigusr1(pid);", Why::PeerGone, "信号送不到 = 那个进程已不在；发之前有进程身份复核（做得好的对照组）"),
    ("src/backend/control/transfer_copy.rs", "let _ = crate::dial::sftp::remove(&self.to, &staging_part(&key)).await;", Why::CleanupAfterFailure, "一份文件没传成 ⇒ 删落点那台暂存区里的半截；删不掉只是暂存区里多一份，孤儿扫会收"),
    ("src/backend/control/transfer_copy.rs", "let _ = self.face.delete_file(&at);", Why::CleanupAfterFailure, "本机中转那一份不论成败都删；删不掉只是暂存区里多一份，孤儿扫会收"),
    ("src/backend/control/transfer_copy.rs", "let _ = face.own_dir(&at);", Why::CleanupAfterFailure, "建本机暂存区：建不成时下一步落暂存件那一下照实报错，不在这里先报"),
    ("src/backend/control/transfer.rs", "let _ = forward.await;", Why::Reap, "等进度转发任务收尾（它自己只往已结束的票上报数）"),
    ("src/backend/control/transfer.rs", "let _ = rf.shutdown().await;", Why::DeadLink, "关远端文件句柄；传输的结局已经定了"),
    ("src/backend/control/transfer.rs", "let _ = sftp::remove(s, &part).await;", Why::CleanupAfterFailure, "撤了 ⇒ 删暂存区里的半截（用户说了不要）；删不掉只是暂存区里多一份，孤儿扫会收"),
    ("src/backend/control/transfer.rs", "let _ = face.delete_file(&at);", Why::CleanupAfterFailure, "DP1 之后只清**空的** `.part`（一个字节都没落）；清不掉只剩一个 0 字节文件，主错误已在回"),
    ("src/backend/dial/probe.rs", "let _ = tx.send(Ev::Feed(e));", Why::PeerGone, "测试连接那一台常驻流交来的事转进这一趟：这一趟已经收场（结局出了 · 到点被丢）就没人要这一下"),
    ("src/backend/platform/signal.rs", "let _ = t.recv().await;", Why::Signal, "装不上 SIGTERM 时退回只等 SIGINT（从 `main.rs` 下沉来）"),
    ("src/backend/platform/signal.rs", "let _ = tokio::signal::ctrl_c().await;", Why::Signal, "从 `main.rs` 下沉来的停机信号监听"),
    ("src/backend/control/terminal_follow.rs", "let _ = f.tx.send(Ev::Stop);", Why::PeerGone, "退订 / 连接走：那张票的订阅线程已经自己停了（终端没了、推了收尾帧）⇒ 「停」没人收"),
    ("src/backend/control/terminal_follow.rs", "let _ = self.replies.blocking_send(Frame::TerminalFollowEnd(crate::wire::TerminalFollowEnd { ticket: self.ticket.clone()", Why::PeerGone, "订它的那条流连接已经走了 ⇒ 收尾帧没人收（票表随连接一起丢）"),
    ("src/backend/control/terminal_follow.rs", "let _ = tx.send(Ev::Closed);", Why::PeerGone, "订阅线程已经停了（退订 / 推了收尾帧）⇒ 「客户端退了」没人收"),
    ("src/backend/platform/child.rs", "let _ = r.join();", Why::Reap, "一问一答的子进程收尾：等两条读线程读到头；读线程自己不会 panic（只往通道里送行），join 失败没有更好的补救"),
    ("src/backend/platform/child.rs", "let _ = tx.send(Heard::Exited(status.ok().and_then(|s| s.code())));", Why::PeerGone, "拿着它的那一方已放手（作业收了）⇒ 「退了」那一件没人收"),
    ("src/backend/platform/child.rs", "let _ = child.kill();", Why::Reap, "伪终端里起的那一个说不出 pid（杀不准它那一组）⇒ 当场杀掉、收尸、照实报起不来；杀 / 收失败没有更好的补救"),
    ("src/backend/platform/child.rs", "let _ = reader.join();", Why::Reap, "伪终端那一问一答收尾：等读线程读到头；读线程自己不会 panic（只往通道里送行），join 失败没有更好的补救"),
    ("src/backend/platform/child.rs", "let _ = tx.send(Heard::Exited(status.ok().map(|s| i32::try_from(s.exit_code()).unwrap_or(i32::MAX))));", Why::PeerGone, "拿着它的那一方已放手（作业收了）⇒ 「退了」那一件没人收"),
    ("src/backend/platform/child.rs", "let _ = tx.send(Heard::Out(plain_line(&buf)));", Why::PeerGone, "伪终端读到头时没收尾的那半行：拿着它的那一方已放手 ⇒ 没人收"),
    ("src/backend/platform/child.rs", "let _ = self.child.wait();", Why::Reap, "长寿子进程放手：杀组之后收尸；收不了也没有更好的补救（组已经杀了）"),
    ("src/backend/platform/child.rs", "let _ = child.wait();", Why::Reap, "Windows 臂只等直接子进程退出（句柄占着进程号）；退出码由随后那一下 `wait` 取"),
    ("src/backend/platform/child.rs", "let _ = done_tx.send(status.map(|s| Output { status: s, stdout: o, stderr: e, }));", Why::PeerGone, "发起方过了期限已放手、回了超时，这份结果没人收"),
    ("src/backend/platform/child.rs", "let _ = ev_tx.send(Ev::GaveUp);", Why::PeerGone, "等待线程已收尾退出 ⇒ 放手那一声没人收"),
    ("src/backend/platform/child.rs", "let _ = piped_tx.send(Ev::Piped(o, e));", Why::PeerGone, "等待线程已按放手收尾 ⇒ 读完的输出没人收"),
    ("src/backend/platform/child.rs", "let _ = r.read_to_end(&mut buf);", Why::Drain, "读子进程输出到底；读错只少尾巴几个字节，退出码与超时照旧判"),
    ("src/backend/control/resident.rs", "let _ = log_dir_chain(own);", Why::Diag, "诊断文件那层目录建不了 ⇒ 子进程装不上 stderr 文件、照旧 null（「写不进去不拖垮后端」）；这是一次性子命令，stderr 只许一行 JSON 信封，没有第二个地方可说"),
    // NDJSON 那一形的四行（写线程 · 两处 `write_all` · `flush`）随独立 `--relay` 删了。
    // `cc_bus.rs` 那两行（本机 shell 读收尸）随那条读删了。
    // 载荷那一层渲 env 那四行 `let _ = write!(out, "export …")` 摘了：`export` / `unset` 的写法搬进 `platform::shell::posix`
    //   （返回 `String`、调用处 `push_str`，不再有吞）；载荷那一层后来整层删了。
    // 窗口那几问的答复送回等答的那一趟（它已收场 ⇒ 没人要）· 暂存件收尾删不掉交孤儿扫（不盖下载 / 复制本身的结局）。
    // `history.rs` 那一处（读整份会话末块交前端、前端已走）随那条命令退役。
    // F5 重放那两处裸 `let _ = handle.emit(…)`（容器 · 可重连）随起停事件并进会话流删了（就绪点在流里原位交成品）。
    ("src/frontend/shell/src/lib.rs", "let _ = w.set_focus();", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/lib.rs", "let _ = w.show();", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/lib.rs", "let _ = w.unminimize();", Why::WindowBestEffort, ""),
    ("src/frontend/shell/src/lib.rs", "let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational));", Why::WindowBestEffort, ""),
    // 单实例回调那三行收进 `platform::window::raise_main`（与点通知共用，失败各留一行日志），这里不再有。
    ("src/frontend/shell/src/lib.rs", "let _ = window.set_focus();", Why::WindowBestEffort, ""),
    // `agent_home` 的可重入挂法（`rewatch_agent_home`）：同上面三个目录那一族。
    ("src/backend/control/resident.rs", "let _ = tokio::io::copy(&mut down, &mut stdout).await;", Why::DeadLink, "小中继下行：套接字那头走了 / stdout 写不出去（ssh 通道关了），两种都是这一趟结束，退出就是交代"),
    ("src/backend/control/resident.rs", "let _ = tokio::io::AsyncWriteExt::flush(&mut stdout).await;", Why::DeadLink, "小中继收尾 flush：ssh 通道已经关了就写不出去，退出就是交代"),
    ("src/backend/control/resident.rs", "let _ = own.delete_file(&rec_own);", Why::Reap, "升级那一跳停完旧版之后收掉它的旧记录：结局已经写进日志；删不掉只剩一份陈记录，下一个新版起来会再核一次身份（ESRCH / exe 对不上照样答对）"),
    ("src/backend/control/resident.rs", "let _ = own.delete_file(&token);", Why::Reap, "升级那一跳顺手收掉旧版的钥匙文件：新版不读它，删不掉只是留一个没人用的文件"),
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

/// 盘上的人群：`(仓根相对路径, 归一化语句) → 处数`。
fn population() -> BTreeMap<(String, String), usize> {
    let root = guard_core::addr::repo_root();
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
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let mut out: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (path, raw) in &files {
        let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if !real.starts_with(&root) {
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
