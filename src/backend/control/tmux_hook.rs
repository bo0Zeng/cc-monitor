//! tmux hook → backend 的通知通路：「几个 tmux 会话里杀掉其中一个」没有内核事件源（pidfd 只看 server 进程、
//! socket inotify 只看 server 生死），tmux 自己的 `session-closed` hook 就是那个信号。
//!
//! # 通路（零文件系统写）
//!
//! ```text
//! tmux hook (全局段 [50, 100) 里每个后端一格, run-shell -b)
//! └─> <backend exe> --tmux-notify <backend_pid> <backend_starttime>
//! ├─ 读 /proc/<pid>/stat 校验 starttime 相符（挡 PID 复用误伤无关进程）
//! └─ kill(pid, SIGUSR1)
//! backend: SIGUSR1 流（main.rs）
//! └─> WatcherPoke::poke() ⇒ 往统一 channel 发一拍 ⇒ 立刻重探
//! ```
//!
//! # 不传会话名
//!
//! 信号无载荷：会话名要经 shell 引号（名字里的 `"` / `$(...)` 能破坏命令串甚至注入），写事件日志又违反「backend 只读」。
//! 代价是信号会合并（几个会话同时关可能只来一次）—— 重探 ＋ 与上一份快照差分，一次能报出所有消失的会话。
//! （hook 里取 `#{@ccm_sid}` 会拿到空：那是窗格级 option，hook 上下文里未必绑到目标；哪天想顺便带名字，先看这一条。）

use crate::platform::child::{Child, ChildFail, Deadline};
use std::collections::BTreeMap;
use std::path::Path;

/// hook 槽位段：`[HOOK_SLOT_BASE, HOOK_SLOT_BASE + HOOK_SLOT_COUNT)`。每个后端实例占其中一格（三个事件同一个下标）：
/// 同一台 tmux server 上可以有几个流模式后端（两台机器的 monitor 都连着它），各占各的格，起时清掉死 pid 的格。
/// 用下标而不是追加到一串未知 hook 后面，是为了可撤销（`tmux set-hook -gu 'session-closed[<下标>]'`）。
pub(crate) const HOOK_SLOT_BASE: u32 = 50;
/// 段长。一格都挑不出来 ⇒ 这台 server 上同时活着这么多个后端 —— 大声说、不装（不去盖别人的）。
pub(crate) const HOOK_SLOT_COUNT: u32 = 50;

/// 装哪几个 hook。`session-renamed` 也要 —— 名字是 monitor 侧查表的键，
/// 改名不通知的话下一次差分会把它误判成「一个消失了、一个新出现」。
pub(crate) const HOOK_EVENTS: [&str; 3] = ["session-created", "session-closed", "session-renamed"];

/// POSIX 单引号包裹。**只用于我们自己产生的路径/数字**（exe 路径、pid、starttime），
/// 不用于任何来自 tmux 或用户的字符串 —— 那条路本设计里根本不存在（见模块头注）。
fn sq(s: &str) -> String {
    // 实现住 `shell-quote-core`；保留本地名字，调用点零改。
    shell_quote_core::posix_quote(s)
}

/// 一条 hook 的 tmux 命令参数（不含 `tmux` 本身），供 `Command::args` 直接用。
///
/// 形如：`set-hook -g session-closed[<槽>] run-shell -b '<exe> --tmux-notify <pid> <start>'`
///
/// **`run-shell -b`**：`-b` 是后台执行 —— 不加的话 tmux 会**同步等**这条命令跑完，
/// 把会话关闭这条路径卡在我们的进程启动上。
pub(crate) fn hook_set_args(
    event: &str,
    slot: u32,
    exe: &Path,
    pid: u32,
    starttime: u64,
) -> Vec<String> {
    // 本二进制就叫 `ccm` ⇒ 叫后端子命令一律 `<exe> -- <子命令>`（没有 `--` 会整行交给 claude）。
    let payload = format!(
        "{} -- {NOTIFY_FLAG} {pid} {starttime}",
        sq(&exe.to_string_lossy())
    );
    vec![
        "set-hook".into(),
        "-g".into(),
        format!("{event}[{slot}]"),
        format!("run-shell -b {}", sq(&payload)),
    ]
}

/// 摘一格（生产只用它摘**死槽**：载荷认得出、那个 pid 的 starttime 对不上的那几格）。
///
/// **停机时不摘自己那一格**：留着的 hook 指向一个已死的 pid，`notify` 那边 starttime 校验不过就静默 no-op；
/// 下一个起来的后端会把它当死槽清掉；server 重启 hook 本就没了。
pub(crate) fn hook_unset_args(event: &str, slot: u32) -> Vec<String> {
    vec!["set-hook".into(), "-gu".into(), format!("{event}[{slot}]")]
}

/// 读一个事件的全部全局 hook 的参数。
pub(crate) fn hook_show_args(event: &str) -> Vec<String> {
    vec!["show-hooks".into(), "-g".into(), event.into()]
}

/// 载荷里认「这是一个 ccm 后端装的 hook」的那个旗标 —— 与 `main.rs` 分派的那一臂同一个字。
const NOTIFY_FLAG: &str = "--tmux-notify";

/// 一格里住着谁。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Occupant {
    /// 不是我们认得的载荷（用户自己的 hook）—— **永远不碰**。
    Foreign,
    /// 一个 ccm 后端装的：`(pid, starttime)`。
    Backend(u32, u64),
}

/// 解 `tmux show-hooks -g <事件>` 的回话：`<事件>[<下标>] <命令>` 一行一条（空槽只打一个事件名，跳过）。
/// 载荷里认得出 `--tmux-notify <pid> <starttime>` ⇒ [`Occupant::Backend`]；否则 [`Occupant::Foreign`]。纯函数。
/// tmux 会把载荷重新引一遍（`"'/p a/exe' --tmux-notify 123 456"`），所以只认旗标与后面两个数，不认引号。
pub(crate) fn parse_show_hooks(event: &str, out: &str) -> BTreeMap<u32, Occupant> {
    let mut got = BTreeMap::new();
    for line in out.lines() {
        let Some(rest) = line.trim_start().strip_prefix(event) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix('[') else {
            continue;
        };
        let Some(close) = rest.find(']') else {
            continue;
        };
        let Ok(index) = rest[..close].parse::<u32>() else {
            continue;
        };
        got.insert(index, occupant_of(&rest[close + 1..]));
    }
    got
}

fn occupant_of(cmd: &str) -> Occupant {
    let Some(at) = cmd.find(&format!(" {NOTIFY_FLAG} ")) else {
        return Occupant::Foreign;
    };
    let mut nums = cmd[at + NOTIFY_FLAG.len() + 2..]
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty());
    match (
        nums.next().and_then(|p| p.parse().ok()),
        nums.next().and_then(|s| s.parse().ok()),
    ) {
        (Some(pid), Some(start)) => Occupant::Backend(pid, start),
        _ => Occupant::Foreign,
    }
}

/// 全段的现状：`(事件下标 0..3, 槽) → 住着谁`。空的那几格不在表里。
pub(crate) type Board = BTreeMap<(usize, u32), Occupant>;

/// 一次装之前要做的事（纯函数 [`plan`] 的产物）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    /// 要摘的格：段内、载荷认得出、那个后端已经不在了（死槽），以及**自己**落在别的格里的旧条目。
    pub(crate) unset: Vec<(usize, u32)>,
    /// 自己该装进的那一格。`None` = 段里一格都挑不出来。
    pub(crate) slot: Option<u32>,
}

/// 首选格：`BASE + pid % COUNT`（两个同时起的后端大概率落在不同格，撞了由装完那一读兜）。
pub(crate) fn preferred_slot(pid: u32) -> u32 {
    HOOK_SLOT_BASE + pid % HOOK_SLOT_COUNT
}

/// 选格 ＋ 清死槽。**纯函数**：`alive(pid, start)` 由调用方给（生产 = 真 `/proc`）。
///
/// - 段内 `Backend(p, s)` 且不是自己、`alive` 说不在 ⇒ 摘（死槽）；段外一格都不看；`Foreign` 一格都不碰。
/// - 已有一格三个事件都是自己 ⇒ 复用那一格；否则从首选格起绕段一圈，挑第一格「三个事件在这一格都空、或都是自己」。
/// - 自己落在挑中那一格**之外**的条目 ⇒ 摘（上一趟撞格之后留下的）。
pub(crate) fn plan(board: &Board, me: (u32, u64), alive: &dyn Fn(u32, u64) -> bool) -> Plan {
    let in_range = |slot: u32| (HOOK_SLOT_BASE..HOOK_SLOT_BASE + HOOK_SLOT_COUNT).contains(&slot);
    let mut unset: Vec<(usize, u32)> = Vec::new();
    let mut live = Board::new();
    for (&(ev, slot), &occ) in board {
        if !in_range(slot) {
            continue;
        }
        match occ {
            Occupant::Backend(p, s) if (p, s) != me && !alive(p, s) => unset.push((ev, slot)),
            _ => {
                live.insert((ev, slot), occ);
            }
        }
    }
    let mine = |slot: u32, ev: usize| live.get(&(ev, slot)) == Some(&Occupant::Backend(me.0, me.1));
    let usable = |slot: u32| {
        (0..HOOK_EVENTS.len()).all(|ev| !live.contains_key(&(ev, slot)) || mine(slot, ev))
    };
    let whole = (0..HOOK_SLOT_COUNT)
        .map(|k| HOOK_SLOT_BASE + k)
        .find(|&slot| (0..HOOK_EVENTS.len()).all(|ev| mine(slot, ev)));
    let start = preferred_slot(me.0) - HOOK_SLOT_BASE;
    let slot = whole.or_else(|| {
        (0..HOOK_SLOT_COUNT)
            .map(|k| HOOK_SLOT_BASE + (start + k) % HOOK_SLOT_COUNT)
            .find(|&slot| usable(slot))
    });
    if let Some(chosen) = slot {
        for (&(ev, s), _) in live.iter().filter(|(&(_, s), _)| s != chosen) {
            if mine(s, ev) {
                unset.push((ev, s));
            }
        }
    }
    unset.sort_unstable();
    Plan { unset, slot }
}

/// 跑一条 tmux 命令：`Ok((成功?, stdout))`；起不来 tmux ⇒ `Err`。生产 = [`run_tmux`]；判据喂一张内存里的 hook 表。
pub(crate) type TmuxRun<'a> = &'a mut dyn FnMut(&[String]) -> Result<(bool, String), RunErr>;

/// 生产那一个执行器。**socket 定位**：不传 `-L`/`-S`，靠继承的 `TMUX_TMPDIR` / 默认 socket ——
/// backend 与它观测的那台 server 本来就在同一套 socket 语义下（`tmux ls` 探测也是这么跑的）。
fn run_tmux(args: &[String]) -> Result<(bool, String), RunErr> {
    run_tmux_via(Child::new("tmux"), HOOK_TMUX_WITHIN, args)
}

/// 装钩子那几发 tmux 各自的期限：在 watcher 的事件循环里就地跑，一发 5 s（同 watcher 探测 tmux 的期限）；
/// 超时 ⇒ 这一轮不装（[`install_hooks_with`]），流照走。
const HOOK_TMUX_WITHIN: Deadline = Deadline::secs(5);

/// [`run_tmux`] 的本体（命令与期限由调用方给：判据换一个不应答的假 tmux ＋ 短期限）。
pub(crate) fn run_tmux_via(
    tmux: Child,
    within: Deadline,
    args: &[String],
) -> Result<(bool, String), RunErr> {
    // 读它的回话 ⇒ 打印通道按 UTF-8（`common/tmux_utf8.rs` 那条规矩：本机 argv 直传用旗，且排在子命令之前）。
    let out = tmux
        .arg(crate::common::tmux_utf8::UTF8_CLIENT_FLAG)
        .args(args)
        .run(within)
        .map_err(|e| match e {
            ChildFail::TimedOut { .. } => RunErr::TimedOut(e.to_string()),
            e => RunErr::NotRun(e.to_string()),
        })?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

/// 一发 tmux 没答成：起不来 / 过了期限没答（后者 ⇒ 这一轮整个不装）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunErr {
    NotRun(String),
    TimedOut(String),
}

impl std::fmt::Display for RunErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunErr::NotRun(s) | RunErr::TimedOut(s) => f.write_str(s),
        }
    }
}

/// 读全段现状（三个事件各问一次）。任何一问没答成 ⇒ `Err(那个事件名 / 起不来的原因)`（判不了就不清、不挑；只进日志）。
fn read_board(run: TmuxRun<'_>) -> Result<Board, RunErr> {
    let mut board = Board::new();
    for (ev, event) in HOOK_EVENTS.iter().enumerate() {
        match run(&hook_show_args(event))? {
            (true, out) => {
                for (slot, occ) in parse_show_hooks(event, &out) {
                    board.insert((ev, slot), occ);
                }
            }
            (false, _) => return Err(RunErr::NotRun(format!("show-hooks -g {event}"))),
        }
    }
    Ok(board)
}

/// 在活着的 tmux server 上装 / 重装三个 hook。
///
/// 每次感知到 server 存在（含复活）都要调：hook 活在 server 内存里，server 一重启就全没了。
/// 失败只 warn 不致命（这一类事件退回靠别的事件触发重探），但要说出来，不然「hook 通路没生效」是静默降级。
/// 返回装成功的条数（供日志与测试用）。
pub(crate) fn install_hooks(exe: &Path, pid: u32, starttime: u64) -> usize {
    install_hooks_with(&mut run_tmux, exe, pid, starttime, &backend_alive)
}

/// 「装那一格的那个后端还在不在」—— 与 [`notify`] 同一条身份校验（pid 的 starttime 对得上）。
pub(crate) fn backend_alive(pid: u32, starttime: u64) -> bool {
    crate::platform::proc::proc_starttime(pid) == Some(starttime)
}

/// [`install_hooks`] 的本体（执行器与「那个后端还在不在」由调用方给）。
///
/// 读段 → [`plan`]（摘死槽 · 挑一格）→ 装三条 → **再读一遍核「三个事件这一格都是我」**；
/// 不是（与另一个同时起的后端撞了同一格）⇒ 重来一趟（有界：段长）。读不了段（老 tmux 不认 `show-hooks` 那一形）
/// ⇒ 不清死槽、直接装进首选格并说一句（装不装由 tmux 答）。
pub(crate) fn install_hooks_with(
    run: TmuxRun<'_>,
    exe: &Path,
    pid: u32,
    starttime: u64,
    alive: &dyn Fn(u32, u64) -> bool,
) -> usize {
    let me = (pid, starttime);
    for _ in 0..HOOK_SLOT_COUNT {
        let board = match read_board(run) {
            Ok(b) => b,
            Err(RunErr::TimedOut(e)) => {
                tracing::warn!("读 tmux 现有的 hook：{e} ⇒ 这一轮不装");
                return 0;
            }
            Err(e) => {
                tracing::warn!(
                    "读不了 tmux 现有的 hook（{e}）⇒ 不清死槽，直接装进首选格 [{}]",
                    preferred_slot(pid)
                );
                return set_three(run, preferred_slot(pid), exe, pid, starttime).unwrap_or(0);
            }
        };
        let p = plan(&board, me, alive);
        for &(ev, slot) in &p.unset {
            let event = HOOK_EVENTS[ev];
            match run(&hook_unset_args(event, slot)) {
                Ok((true, _)) => {}
                Err(RunErr::TimedOut(e)) => {
                    tracing::warn!("摘 tmux hook {event}[{slot}]：{e} ⇒ 这一轮不装");
                    return 0;
                }
                _ => tracing::warn!("摘 tmux hook {event}[{slot}] 没成"),
            }
        }
        let Some(slot) = p.slot else {
            tracing::warn!(
                "tmux hook 段 [{HOOK_SLOT_BASE}, {}) 里一格都挑不出来（这台 server 上同时活着的后端太多）⇒ 不装，\
                 多会话里关掉其中一个要等别的事件才看得见",
                HOOK_SLOT_BASE + HOOK_SLOT_COUNT
            );
            return 0;
        };
        let Some(n) = set_three(run, slot, exe, pid, starttime) else {
            return 0;
        };
        let ours = match read_board(run) {
            Ok(after) => (0..HOOK_EVENTS.len())
                .all(|ev| after.get(&(ev, slot)) == Some(&Occupant::Backend(pid, starttime))),
            // 装完读不回：信装的那一步的回话。
            Err(_) => true,
        };
        if ours {
            return n;
        }
        tracing::info!("tmux hook 槽 [{slot}] 被另一个同时起的后端占了 ⇒ 换一格再装");
    }
    tracing::warn!("tmux hook 换了 {HOOK_SLOT_COUNT} 次格都被别人抢走 ⇒ 不装了");
    0
}

/// 装成的条数；`None` = tmux 过了期限没答（这一轮整个不装）。
fn set_three(run: TmuxRun<'_>, slot: u32, exe: &Path, pid: u32, starttime: u64) -> Option<usize> {
    // 三条在**一次** tmux 调用里（命令之间用 `;` 分开）：tmux 把一个客户端的一串命令排进它自己的队列一口气跑完，
    //   另一个后端的那一串插不进中间 ⇒ 两个同时起的后端撞了同一格时，这一格要么整格是我、要么整格是它（装完那一读据此换格），
    //   不会出现「三个事件一半是我一半是它」、再被换格那一趟摘掉一半的残格。
    let mut args: Vec<String> = Vec::new();
    for event in HOOK_EVENTS {
        if !args.is_empty() {
            args.push(";".into());
        }
        args.extend(hook_set_args(event, slot, exe, pid, starttime));
    }
    match run(&args) {
        Ok((true, _)) => Some(HOOK_EVENTS.len()),
        Ok((false, _)) => {
            tracing::warn!("装 tmux hook [{slot}] 失败");
            Some(0)
        }
        Err(RunErr::TimedOut(e)) => {
            tracing::warn!("装 tmux hook [{slot}]：{e} ⇒ 这一轮不装");
            None
        }
        Err(e) => {
            tracing::warn!("装 tmux hook [{slot}] 起不来 tmux：{e}");
            Some(0)
        }
    }
}

/// `--tmux-notify <pid> <starttime>`：hook 子进程走这条。
///
/// **fail-closed 的是「误伤」而不是「漏报」**：校验不过就**什么都不做**并退 0。
/// 退 0 是有意的 —— 这是 tmux 的 `run-shell -b` 子进程，非零退出只会在 tmux 里堆错误，
/// 而「backend 已经不在了」是完全正常的情况（用户关掉 monitor 之后 hook 还留着）。
pub fn notify(args: &[String]) -> i32 {
    let (Some(pid_s), Some(start_s)) = (args.get(1), args.get(2)) else {
        eprintln!("用法: --tmux-notify <backend_pid> <backend_starttime>");
        return 2;
    };
    let (Ok(pid), Ok(want_start)) = (pid_s.parse::<u32>(), start_s.parse::<u64>()) else {
        eprintln!("--tmux-notify: pid / starttime 必须是整数");
        return 2;
    };

    // PID 复用防御：backend 退出后那个 pid 可能已被别的进程占用，给它发 SIGUSR1 轻则无效、重则终止一个无关进程
    // （SIGUSR1 的默认处置就是终止）⇒ 必须比对 starttime。
    match crate::platform::proc::proc_starttime(pid) {
        Some(actual) if actual == want_start => {}
        _ => return 0, // 不是那个后端（或它已经不在）⇒ 静默不做事
    }

    // 发信号那一步在 `platform::signal`（平台原语只许在 platform/；措辞不写那个 libc 函数名，免得「本层还有没有平台原语」的 grep 白查一趟）。
    // 身份校验留在这里 —— 那是域判断。发失败不是错误：校验之后、发信号之前后端可能已经退了。
    let _ = crate::platform::signal::send_sigusr1(pid);
    0
}

#[cfg(test)]
#[path = "../../../tests/backend/control/tmux_hook_tests.rs"]
mod tests;
