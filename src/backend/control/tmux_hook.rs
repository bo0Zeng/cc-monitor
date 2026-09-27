//! P4b（zero-poll-liveness）：**tmux hook → backend** 的通知通路。
//!
//! # 它解决什么
//!
//! 「多个 tmux 会话里杀掉其中一个」是**唯一**没有内核事件源的场景：pidfd 只看 server 进程
//! （server 还活着）、socket inotify 只看 server 生死。它此前只能靠 8s 轮询兜住（P4 之前
//! 实测 ~16s）。tmux 自己知道这件事 —— `session-closed` hook 就是那个信号。
//!
//! # 通路（**零文件系统写**）
//!
//! ```text
//! tmux hook (全局段 [50, 100) 里每个后端一格, run-shell -b)
//!    └─> <backend exe> --tmux-notify <backend_pid> <backend_starttime>
//!           ├─ 读 /proc/<pid>/stat 校验 starttime 相符（挡 PID 复用误伤无关进程）
//!           └─ kill(pid, SIGUSR1)
//!    backend: SIGUSR1 流（main.rs，P4a 已就位）
//!           └─> WatcherPoke::poke() ⇒ 往统一 channel 发一拍 ⇒ 立刻重探
//! ```
//!
//! # 为什么不传会话名（这不是偷懒，是设计）
//!
//! 原方案让 hook 把 `#{hook_session_name}` 写进一个事件日志文件。两个问题：
//! ① **撞红线 I7「backend 只读」**——`readonly_guard` 当场拦下了那个建目录调用。
//!    （**这里刻意不逐字写出那个函数名**：`readonly_guard` 连注释一起扫，
//!    是 fail-closed 的设计；在后端源码的散文里引用它的禁用模式会让全局守卫红。
//!    本轮实测栽过一次 —— 处置是改措辞，**不是**去把那道红线守卫改成剥注释。）
//! ② 会话名要经 shell 引号 —— 名字里有 `"` 或 `$(...)` 就能破坏命令串甚至注入，
//!    原方案只能「接受并登记」这个面。
//!
//! 现在**名字根本不传**：信号无载荷 ⇒ 注入面消失、日志不存在、backend 写归零。
//! 代价是信号会合并（多个会话同时关可能只来一次）—— 靠**重探 + 与上一份快照差分**
//! 天然免疫，差分一次能报出所有消失的会话，比逐条事件更稳。
//!
//! # `#{@ccm_sid}` 是个陷阱（P0 实测）
//!
//! hook 里用 `#{@ccm_sid}` 取值会拿到**空**（那是会话级 option，hook 执行上下文里未必绑到
//! 目标会话）⇒ 下游把活会话当成灰的。P0 的结论是：**hook 只用 `#{hook_session_name}`**，
//! 名字→sid 的映射由消费侧查表。本模块**连名字都不传**，所以这条陷阱在这里已不适用，
//! 但注释留着 —— 将来若有人想「顺便把名字带上」，得先回头看这一条。

use std::collections::BTreeMap;
use std::path::Path;

/// 〔HX2 · 主会话 D-b「tmux hook 槽位按实例区分、起时清死 pid 的槽」〕hook 槽位**段**：`[HOOK_SLOT_BASE, HOOK_SLOT_BASE + HOOK_SLOT_COUNT)`。
///
/// 每个后端实例占其中**一格**（三个事件同一个下标）。调研实测全局 `[50]` 空着；仍用下标（而不是追加到一串未知 hook 后面）
/// 是为了**可撤销**（`tmux set-hook -gu 'session-closed[<下标>]'`）。
/// 〔墓碑 —— 从前是一个固定槽 `HOOK_SLOT = 50`：同一台 tmux server 上第二个流模式后端（两台机器的 monitor 都连着它）
///  把第一个的槽盖掉，先起的那个从此收不到通知；后起的退了，槽里是死 pid，没人清（审计 `E-compat.md` §E5）。〕
pub(crate) const HOOK_SLOT_BASE: u32 = 50;
/// 段长。一格都挑不出来 ⇒ 这台 server 上同时活着这么多个后端 —— 大声说、不装（不去盖别人的）。
pub(crate) const HOOK_SLOT_COUNT: u32 = 50;

/// 装哪几个 hook。`session-renamed` 也要 —— 名字是 monitor 侧查表的键，
/// 改名不通知的话下一次差分会把它误判成「一个消失了、一个新出现」。
pub(crate) const HOOK_EVENTS: [&str; 3] = ["session-created", "session-closed", "session-renamed"];

/// POSIX 单引号包裹。**只用于我们自己产生的路径/数字**（exe 路径、pid、starttime），
/// 不用于任何来自 tmux 或用户的字符串 —— 那条路本设计里根本不存在（见模块头注）。
fn sq(s: &str) -> String {
    // U8c-2b-0（账本 S5）：实现收进 `shell-quote-core`（P4c 前叫 `launch-core`）——
    // 此前全仓有**四份逐字节相同**的
    // POSIX 单引号 quote。保留本地名字，调用点零改。
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
    // 〔V151〕本二进制就叫 `ccm` ⇒ 叫后端子命令一律 `<exe> -- <子命令>`（没有 `--` 会整行交给 claude）。
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

/// 摘一格（〔HX2〕生产只用它摘**死槽**：载荷认得出、那个 pid 的 starttime 对不上的那几格）。
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

/// 解 `tmux show-hooks -g <事件>` 的回话：`<事件>[<下标>] <命令>` 一行一条（下标之外的行 —— 空槽只打一个事件名 —— 跳过）。
/// 载荷里认得出 `--tmux-notify <pid> <starttime>` ⇒ [`Occupant::Backend`]；否则 [`Occupant::Foreign`]。**纯函数**。
///
/// 实测（tmux 3.6）：`session-closed[50] run-shell -b "'/p a/exe' --tmux-notify 123 456"`（tmux 自己重新引了一遍，
/// 所以只认旗标与后面两个数，不认引号）。
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

/// 〔HX2〕选格 ＋ 清死槽。**纯函数**：`alive(pid, start)` 由调用方给（生产 = 真 `/proc`）。
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
pub(crate) type TmuxRun<'a> = &'a mut dyn FnMut(&[String]) -> Result<(bool, String), String>;

/// 生产那一个执行器。**socket 定位**：不传 `-L`/`-S`，靠继承的 `TMUX_TMPDIR` / 默认 socket ——
/// backend 与它观测的那台 server 本来就在同一套 socket 语义下（`tmux ls` 探测也是这么跑的）。
fn run_tmux(args: &[String]) -> Result<(bool, String), String> {
    // 读它的回话 ⇒ 打印通道按 UTF-8（`common/tmux_utf8.rs` 那条规矩：本机 argv 直传用旗，且排在子命令之前）。
    let out = std::process::Command::new("tmux")
        .arg(crate::common::tmux_utf8::UTF8_CLIENT_FLAG)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

/// 读全段现状（三个事件各问一次）。任何一问没答成 ⇒ `Err(那个事件名 / 起不来的原因)`（判不了就不清、不挑；只进日志）。
fn read_board(run: TmuxRun<'_>) -> Result<Board, String> {
    let mut board = Board::new();
    for (ev, event) in HOOK_EVENTS.iter().enumerate() {
        match run(&hook_show_args(event))? {
            (true, out) => {
                for (slot, occ) in parse_show_hooks(event, &out) {
                    board.insert((ev, slot), occ);
                }
            }
            (false, _) => return Err(format!("show-hooks -g {event}")),
        }
    }
    Ok(board)
}

/// **在活着的 tmux server 上装/重装三个 hook。**
///
/// 每次感知到 server 存在（含**复活**）都要调 —— hook 活在 server 内存里，
/// server 一重启就全没了。P3 把「server 起来了」变成了事件 ⇒ 这里有现成的时机。
///
/// **失败只 warn 不致命**：装不上 hook = 这一类事件退回靠别的事件触发重探，
/// 不是致命错。**但要说出来**，否则「hook 通路没生效」会变成静默降级。
///
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
/// 〔HX2〕读段 → [`plan`]（摘死槽 · 挑一格）→ 装三条 → **再读一遍核「三个事件这一格都是我」**；
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
            Err(e) => {
                tracing::warn!(
                    "读不了 tmux 现有的 hook（{e}）⇒ 不清死槽，直接装进首选格 [{}]",
                    preferred_slot(pid)
                );
                return set_three(run, preferred_slot(pid), exe, pid, starttime);
            }
        };
        let p = plan(&board, me, alive);
        for &(ev, slot) in &p.unset {
            let event = HOOK_EVENTS[ev];
            match run(&hook_unset_args(event, slot)) {
                Ok((true, _)) => {}
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
        let n = set_three(run, slot, exe, pid, starttime);
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

fn set_three(run: TmuxRun<'_>, slot: u32, exe: &Path, pid: u32, starttime: u64) -> usize {
    // 〔HX2〕三条在**一次** tmux 调用里（命令之间用 `;` 分开）：tmux 把一个客户端的一串命令排进它自己的队列一口气跑完，
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
        Ok((true, _)) => HOOK_EVENTS.len(),
        Ok((false, _)) => {
            tracing::warn!("装 tmux hook [{slot}] 失败");
            0
        }
        Err(e) => {
            tracing::warn!("装 tmux hook [{slot}] 起不来 tmux：{e}");
            0
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

    // ★ PID 复用防御：光看 /proc/<pid> 存在是不够的 —— backend 退出后那个 pid 可能已经
    // 被**别的进程**占用，给它发 SIGUSR1 轻则无效、重则打断一个无关进程（很多程序把
    // SIGUSR1 当自定义控制信号，默认处置更是直接终止）。必须比对 starttime。
    match crate::platform::proc::proc_starttime(pid) {
        Some(actual) if actual == want_start => {}
        _ => return 0, // 不是那个后端（或它已经不在）⇒ 静默不做事
    }

    // U3：发信号那一步下沉到 `platform::signal`（§1.1-1：平台原语只许在 platform/）。
    // 措辞刻意不写出那个 libc 函数名 —— 「本层还有没有平台原语」是靠 grep 查的，
    // 注释里留一个会让下一个人白查一趟（同 §41.4 第 1 条纪律的形状）。
    // **身份校验留在这里**——那是域判断（「这个 pid 是不是我那个后端」），不是平台能力。
    // 发失败仍不是错误：竞态（校验之后、发信号之前后端退出了）。
    let _ = crate::platform::signal::send_sigusr1(pid);
    0
}

#[cfg(test)]
#[path = "../../../tests/backend/control/tmux_hook_tests.rs"]
mod tests;
