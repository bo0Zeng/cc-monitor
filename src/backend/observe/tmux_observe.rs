//! tmux 观测：四态 · `NO_TMUX` 哨兵 · `tmux ls` · 关闭集合 diff · socket 目录（块）。
//!
//! 整块从 `observe/watcher.rs` 搬来（余下那一刀），逐字不变；
//! 只把 watcher 主循环要用的几项升成 `pub(super)`。事件总线、主循环、jsonl 增量读、
//! pidfile 生命周期仍住 `watcher.rs`。

use crate::platform::proc::pid_alive;
use std::path::PathBuf;

/// P3：主循环对 tmux server 的认知。**三值**——`Unknown` 与 `Gone` 必须分开：
/// 只有在 `Alive` 时才敢把「`tmux ls` rc=1」判成真异常（见 `classify_with_server_state`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ServerState {
    /// 还没探到过（backend 刚起 / 探测失败）。
    Unknown,
    /// 已探到并挂了 pidfd 看守。
    Alive(u32),
    /// pidfd 报过死，或探测明确说没有 server。
    Gone,
}

/// P3：**收紧 P1 那处刻意的保守**。
///
/// P1 把 `tmux ls` rc=1 一律判成"确证零会话"，并留了一条注释说
/// 「P3 持有 pidfd 后可把『server 活着但 rc=1』归 `Unobservable`」。这里落地。
///
/// **实现上刻意不依赖"pidfd 是否已经醒过"**——那会有个危险的失效模式：若 pidfd 路
/// 因任何原因没醒，状态永远停在 `Alive`，rc=1 就被永久压成 `Unobservable` ⇒ 永不 retire。
/// 改成**直接查 `/proc` 里那个 server pid 还在不在**：
/// - 不在了 ⇒ 就是真的没 server（`NoServer` 原样通过，顺带把状态推到 `Gone`）
/// - 还在 ⇒ 「server 明明活着、`tmux ls` 却连不上」= 真异常（socket 权限/被换掉之类）
///   ⇒ `Unobservable`（保守跳过，不误 retire）
///
/// 一次 `/proc` 存在性读，无定时器、无状态耦合、无挂死风险。
pub(super) fn classify_with_server_state(
    obs: TmuxObservation,
    server: ServerState,
) -> TmuxObservation {
    match (&obs, server) {
        (TmuxObservation::NoServer, ServerState::Alive(pid)) if pid_alive(pid) => {
            tracing::warn!(
                "tmux ls 报 rc=1 但记着的 server pid {pid} 仍在 /proc ⇒ 观测无效（不当零会话）"
            );
            TmuxObservation::Unobservable
        }
        _ => obs,
    }
}

/// B2：`tmux ls -F` 格式串（真 TAB 分列）。name⇥path⇥cmd⇥attached⇥windows⇥@ccm_sid（活动窗格的）⇥session_id⇥各窗格的 @ccm_sid。
///
/// `@ccm_sid` 打在**窗格**上（一个会话里可以跑几个 claude）：末列逐窗口、逐窗格展开（解析见
/// `common::session_snapshot::pane_sids`）；`#{session_id}` 是会话账本认会话的句柄（会话改名它不变）。
pub(crate) const TMUX_LS_FMT: &str = "#{session_name}\t#{pane_current_path}\t#{pane_current_command}\t#{?session_attached,1,0}\t#{session_windows}\t#{@ccm_sid}\t#{session_id}\t#{W:#{P:#{@ccm_sid} }}";

/// `TMUX_LS_FMT` 的列数 —— [`tab_underflow`] 的 N。**改格式串必须同步这个数**。
pub(crate) const TMUX_LS_FMT_FIELDS: usize = 8;

// ★★ **K-R12 下一拍（09-04）：这两个口径的家搬到了 `crate::common::tmux_utf8`。**
//
// 上一拍这里各写了一份（`TMUX_UTF8_ENV` 与 `tmux_tab_underflow`〔散文墓碑〕），而 `control/gate.rs`
// 也各写了一份，头注里逐字登记着「三份口径今天**靠人对齐**」——
// 成因是 `layering_guard` 钉死 `control/` 不许引用 `observe/` ⇒ 共用的家只能是 `common/`，
// 而它当时不在写区。本拍写区含 `common/` ⇒ 两份都归位，两层各自 `use` 同一个家。
//
// 口径本身、两种表示为什么是**一个**口径、以及「什么形态的调用点该用哪种表示」
// 全部只有一个住址：那个模块的头注。**这里不复述**（`brief` 13b：闭集只许一个住址）。
//
// 只留本调用点专属的两句：
//
// ① 本模块这两处都是 `sh -c <脚本字符串>` ⇒ 按那张表用 **env 形**（挂 env 不动脚本串）。
// ② [`query_tmux_server`] 那条脚本在测试里有一份**逐字复制**
//    （`tests::tmux_server_query_yields_nothing_without_a_server`）⇒ 动脚本串会让两份漂开，
//    而挂 env 一个字节都不用动它。
use crate::common::tmux_utf8::{tab_underflow, UTF8_CLIENT_ENV};
use crate::platform::child::Deadline;

// ---------- P1（zero-poll-liveness）：tmux 观测的取值（原 `tmux_sessions` 帧的 `observation` 格；那一帧删了，只喂会话账本） ----------
//
// **双写点**：与 monitor `src/frontend/shell/src/tmux.rs` 的同名 const 逐字节一致，由 monitor 侧
// `observation_tokens_double_write_point_stays_in_sync`〔散文墓碑〕 测试钉住（`include_str!` 读本文件 +
// 锚定 const 定义行）。**改本处必须同步 monitor**，同 `TMUX_LS_FMT` 的纪律。
/// backend 确证零会话（rc=0 但 stdout 空 = `exit-empty off`；或 rc=1 = server 不在）。
const OBS_ZERO_SESSIONS: &str = "zero_sessions";
/// 远端没装 tmux——与既有 `NO_TMUX` 哨兵同义，显式化。
const OBS_NO_TMUX: &str = "no_tmux";
/// 观测无效（`tmux ls` 以非 0/1 退出、或 exec 本身失败）⇒ monitor 必须跳过，绝不当零会话。
const OBS_UNOBSERVABLE: &str = "unobservable";

/// P1（zero-poll-liveness）：探测脚本里「PATH 中没有 tmux」的约定退出码。
///
/// **为什么要一个专用 rc 而不是让脚本 `printf 'NO_TMUX'`**：P1 之前脚本用
/// `tmux ls … || true` 把 tmux 自己的 rc **吞掉了**，于是「零会话」「`tmux ls` 出错」
/// 「exec 失败」三种语义全压成同一个空串，monitor 只能一律保守跳过 ⇒ 就是
/// `src/doc/INVARIANTS.md` §24bis 那条残留 bug 的根。改成 `exec tmux …` 让 tmux 的 rc
/// 原样成为 `sh` 的 rc，无 tmux 那格才需要一个不与 tmux 冲突的自定义值。
///
/// 97 是任意选的哨兵值（tmux 只用 0/1）。
const TMUX_PROBE_NO_TMUX_RC: i32 = 97;

/// P1：`tmux ls` 一次观测的四态分类（**P0 实测定死**，见
/// `.claude/planned-build/zero-poll-liveness/features/P0-machine-facts.md` §3 ④）。
///
/// P0 实测的状态空间（隔离 socket）：
/// - rc=0 + stdout 非空 → 有会话
/// - rc=0 + stdout 空 → **server 活着但零会话**（只在 `exit-empty off` 下出现；
///   默认 `exit-empty on` 时 server 随最后一个会话一起退出，走下一格）
/// - rc=1 → server 不在（socket 存在但无 server / socket 根本不存在，两种 stderr 措辞）
/// - 其他 rc → 观测无效
///
/// **前三格里后两格对 retire 决策完全等价**（都是"零会话"），区别只对 P3 的复活监视有意义
/// ⇒ 折成一个 `ZeroSessions`，P3 加细分时**不必改帧契约**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TmuxObservation {
    /// rc=0 + stdout 非空：`tmux ls -F` 原文。
    Sessions(String),
    /// **P3 细分**：rc=0 但 stdout 空 = **server 活着、零会话**（只在 `exit-empty off` 下出现）。
    ServerEmpty,
    /// **P3 细分**：rc=1 = **server 不在**（socket 存在但无 server / socket 根本不存在）。
    ///
    /// 与 `ServerEmpty` **在 wire 上是同一个取值**（`zero_sessions`）——两者对 retire 决策
    /// 完全等价，区别只对 P3 的复活监视与"真异常"判定有意义。**这正是 P0/P1 预判的
    /// "P3 加细分时不必改帧契约"**：`observation_parts` 把两者映射到同一格。
    NoServer,
    /// PATH 里没有 tmux。
    NoTmux,
    /// 观测无效（非 0/1/97 的 rc、被信号杀、exec 失败）⇒ monitor 必须跳过。
    Unobservable,
}

/// P1：`sh -c` 探测脚本。`command -v` 门控解析 PATH（同 monitor `list_remote_tmux`）；
/// **`exec` 让 tmux 的 rc 原样成为 sh 的 rc**（这是 P1 的关键改动——原先 `|| true` 吞了 rc）。
///
/// **提成独立函数是为了可测**：真机 tmux 的四种 rc 由 P0 实测过，但脚本本身（`command -v`
/// 门控 + `exec` 的 rc 透传）要能在 CI 上用**假 tmux** 验证，不能只信字符串断言。
/// ★★ **探测必须有上界**〔audit-0805 F09 / 报告 I-2〕。
///
/// # 上界
///
/// 探测跑在一次性后台线程里，`watch_loop` 的 `tmux_inflight` 去重标志只在收到 `TmuxObserved` 时清：
/// 探测永不返回 ⇒ 标志永远为真 ⇒ 此后一次探测都不会再发起、不出声。
/// ⇒ 两发探测都经起子进程原语带期限跑（[`TMUX_PROBE_WITHIN`]，到点杀整组），超时落 `Unobservable`。
fn tmux_probe_script() -> String {
    // `command -v` 分支与 `exec` 的写法住 `platform::shell::posix`（产出逐字节不变）。
    use crate::platform::shell::posix;
    posix::if_command(
        "tmux",
        &posix::exec(&[format!("tmux ls -F '{TMUX_LS_FMT}' 2>/dev/null")]),
        &format!("exit {TMUX_PROBE_NO_TMUX_RC}"),
    )
}

/// 两发探测（`tmux ls` · `display-message`）各自的期限。`tmux ls` 在健康机器上是毫秒级；
/// 5 s 容忍一次慢盘 / 高负载，又远短于「面板不更新」被注意到的时间尺度。
/// 超时 ⇒ `Unobservable`（「我没看清」，不是「零会话」，后者会误 retire 活会话）。
const TMUX_PROBE_WITHIN: Deadline = Deadline::secs(5);

/// P1：把探测的 (rc, stdout) 折成四态。**可单测**（判据只有 rc + stdout 的形状，
/// 刻意**不看 stderr**——P0 实测 stderr 有两种措辞，且拿英文消息当判据本身就是错的）。
///
/// ⚠ **K-R12 起判据多了一条**：rc=0 且 stdout 非空时，还要看**段数有没有下溢**
/// （见 [`tab_underflow`]）。下溢 ⇒ `Unobservable` + 一条 warn ——
/// 所以本函数不再是严格无副作用的（脏输入那一支会打一行日志），返回值仍然只由入参决定。
///
/// `code == None` = 被信号杀（如 tmux 卡死后探测线程连带被清）⇒ 观测无效。
fn classify_tmux_probe(code: Option<i32>, stdout: &str) -> TmuxObservation {
    match code {
        Some(0) if stdout.trim().is_empty() => TmuxObservation::ServerEmpty,
        // ★★ K-R12 `J1`：**raw 的入口**。有任何一行段数下溢 ⇒ 通道被改写 ⇒ `Unobservable`。
        //
        // 为什么归 `Unobservable` 而不是新造一档：`observation` 那三个 token 是与 monitor
        // 逐字节对拍的**双写点**（`OBS_*`），新增一个要动 wire 契约；而 `Unobservable` 的
        // 语义**本来就是这个** —— 「观测无效 ⇒ monitor 必须跳过，**绝不当零会话**」。
        // 🔴 这一档的处置差别是**要命的**：当成 `Sessions` 会让 [`session_names`] 把整行当
        // 会话名 ⇒ 下一轮差分把**所有活会话**报成消失（`diff_closed` 的 `Sessions` 分支）；
        // 归 `Unobservable` 则「什么都不结论、快照不动」，等下一次成功观测。
        Some(0)
            if stdout
                .lines()
                .any(|l| !l.trim().is_empty() && tab_underflow(l, TMUX_LS_FMT_FIELDS)) =>
        {
            tracing::warn!(
                "CCM_TMUX_UNPARSABLE tmux ls 有行切出的段数 < {TMUX_LS_FMT_FIELDS} —— \
                 tmux 打印通道被改写（K-R12：客户端不是 UTF-8 ⇒ TAB 与非 ASCII 变 `_`），\
                 整趟观测判为无效（绝不当零会话）。原样回包首行：{:?}",
                stdout.lines().next().unwrap_or("")
            );
            TmuxObservation::Unobservable
        }
        Some(0) => TmuxObservation::Sessions(stdout.to_string()),
        // rc=1 = server 不在。**一处刻意的保守**：socket 权限异常这类罕见情形也会落这里
        // ⇒ 理论上可能误 retire。缓解：socket 路径 uid 隔离（`/tmp/tmux-<uid>/`），同 uid 下
        // 权限异常几乎不可能。**P3 落地后有更强判据**：那时后端持有 server 的 pidfd，
        // 「pidfd 说 server 活着但 tmux ls rc=1」= 真异常 ⇒ 归 `Unobservable`。
        // 该升级**不改帧契约**（`ZeroSessions` 语义不变），所以 P1 现在就能安全落地。
        Some(1) => TmuxObservation::NoServer,
        Some(TMUX_PROBE_NO_TMUX_RC) => TmuxObservation::NoTmux,
        _ => TmuxObservation::Unobservable,
    }
}

/// B2：在**本机**（backend 就在远端主机）跑 `tmux ls` 取观测。`sh -c` + `command -v` 门控
/// （同 monitor `list_remote_tmux` 命令）解析 PATH。**只读**（tmux ls 不改任何状态）。
///
/// P1 起返回四态分类而非裸 `String`——见 [`TmuxObservation`]。
///
/// 带期限（[`TMUX_PROBE_WITHIN`]）；仍只在一次性后台线程里调（见 `watch_loop` 的 `tmux_inflight`），
/// 不跑在 watch_loop 线程上 —— 卡到期限那几秒也不该冻住 reader。
fn run_tmux_ls() -> TmuxObservation {
    // 🔴 `K-R55`（09-11）：起 shell 这一跳住适配层（`K33` 裁定二）。
    //    先前这里是裸 `Command::new("sh")` —— `K-R52` 的 A2「真漏」堆头一条。
    //    非 unix 上 `None` ⇒ 与搬之前逐字同一个落点（那时是 `output()` 返 `Err`）。
    let Some(cmd) = crate::platform::shell::posix_shell(&tmux_probe_script()) else {
        tracing::warn!("本平台没有 POSIX shell ⇒ tmux ls 这一跳没有实现，整趟观测判为无效");
        return TmuxObservation::Unobservable;
    };
    match cmd
        // K-R12：挂 env 不改脚本串。家在 `common::tmux_utf8`。
        .env(UTF8_CLIENT_ENV.0, UTF8_CLIENT_ENV.1)
        .run(TMUX_PROBE_WITHIN)
    {
        Ok(out) => classify_tmux_probe(out.status.code(), &String::from_utf8_lossy(&out.stdout)),
        Err(e) => {
            tracing::warn!("tmux ls 本地执行失败: {e}");
            TmuxObservation::Unobservable
        }
    }
}

/// P3：一次 tmux 探测的完整结果——观测分类 + （能拿到时）server 的 pid 与 socket 路径。
///
/// **为什么和 `tmux ls` 同一趟拿**：`display-message` 是另一个 subprocess，而
/// `run_tmux_ls` 头注那条约束（无超时 ⇒ 只能在一次性后台线程里跑）对它同样适用。
/// 放同一个探测线程里 ⇒ 不新增线程、不阻塞主循环（P2 建立的硬约束）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TmuxProbe {
    pub(super) obs: TmuxObservation,
    /// tmux server 进程的 pid（`#{pid}`）——给 pidfd 看守用。拿不到 = 没有 server。
    pub(super) server_pid: Option<u32>,
    /// 这个 server 的 socket 绝对路径（`#{socket_path}`）——给"复活"inotify 用。
    pub(super) socket_path: Option<PathBuf>,
}

/// P3：问 tmux server 要 pid 与 socket 路径。**只读**、无副作用；
/// **死 socket 上调用不会把 server 拉活**（P0 实测）。
///
/// 与 `run_tmux_ls` 同一套 `sh -c` + `command -v` 门控；rc≠0（没有 server）⇒ 全 None。
fn query_tmux_server() -> (Option<u32>, Option<PathBuf>) {
    // 一行两列（TAB 分隔），避免两次 subprocess。
    // 写法住 `platform::shell::posix`（产出逐字节不变）。
    let script = crate::platform::shell::posix::if_command(
        "tmux",
        &crate::platform::shell::posix::exec(&[
            "tmux display-message -p '#{pid}\t#{socket_path}' 2>/dev/null",
        ]),
        "exit 97",
    );
    // 🔴 `K-R55`（09-11）：同上，起 shell 这一跳住适配层。
    let Some(cmd) = crate::platform::shell::posix_shell(&script) else {
        tracing::warn!("本平台没有 POSIX shell ⇒ 问不出 tmux server 的 pid 与 socket 路径");
        return (None, None);
    };
    let out = match cmd
        // K-R12：挂 env 而不是往 `script` 里插旗 —— 那条脚本在测试里有一份**逐字复制**
        // （`tests::tmux_server_query_yields_nothing_without_a_server`），改串会让两份漂开。
        .env(UTF8_CLIENT_ENV.0, UTF8_CLIENT_ENV.1)
        .run(TMUX_PROBE_WITHIN)
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("tmux display-message 执行失败: {e}");
            return (None, None);
        }
    };
    if out.status.code() != Some(0) {
        return (None, None); // 没有 server / 没有 tmux
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or("");
    // ★ K-R12 `J1`：这条一行两列，段数下溢 ⇒ 通道被改写。
    //
    // ⚠ 这一处**今天已经是 fail-closed 的，但完全静默**：脏了以后 `pid` 那一段是
    // `9_/tmp/tmux-1000/default`，`parse::<u32>()` 自然失败 ⇒ `(None, None)`
    // ⇒ 落进 `match probe.server_pid` 的 `_ => {}` 那条**什么都不打的**臂
    // ⇒ `spawn_pid_watcher` 与 `socket_path` 那路 inotify 一次都不装，**而外面看起来一切正常**。
    // 那正是本件立件时看到的症状。处置不变（拿不到就是拿不到），**加的是那句话**。
    if !line.is_empty() && tab_underflow(line, 2) {
        tracing::warn!(
            "CCM_TMUX_UNPARSABLE display-message 切出 {} 段 < 2 —— \
             tmux 打印通道被改写（K-R12：客户端不是 UTF-8 ⇒ TAB 变 `_`），\
             server 的 pid/socket 这一趟拿不到（pidfd 与 socket inotify 都装不上）。原样回包：{line:?}",
            line.split('\t').count()
        );
        return (None, None);
    }
    let mut it = line.split('\t');
    let pid = it.next().and_then(|x| x.trim().parse::<u32>().ok());
    let sock = it
        .next()
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(PathBuf::from);
    (pid, sock)
}

/// 把 watch 挂到某个目录上，**可重入**：目录没了就翻记账，在就先 `unwatch` 再 `watch`。
///
/// # 为什么每个挂点都要这一套
///
/// inotify 的 watch 绑在 **inode** 上。本仓 08-13 一天之内在**三处**踩到同一个形状：
/// `sessions/`（第十拍）· tmux socket 目录（第二十二拍）· `projects/`（本拍）——
/// 症状都是「目录被删掉再重建之后，那一路的帧永远不来，而且没有任何错误」。
/// ⇒ 与其每处各写一遍，不如**只有一份**：改一处漏两处正是本仓一路在收的那族。
///
/// ⚠ 不做重扫：`sessions/` 那侧需要「挂上顺带把已有 pidfile 过一遍」，那是它**特有**的
/// （见 `rewatch_sessions`）；`projects/` 的历史由 `process_jsonl` 按 sid 决定要不要读，
/// 在这里重扫会把**整棵历史**拉一遍 —— 那正是 `P0` 立件时要消灭的行为。

/// tmux socket 目录 = 「本平台的临时根」＋ tmux 自己那条 `tmux-<uid>` 约定。
///
/// 🔴 `K-R55`（09-11）：**两个平台原语搬进了 [`crate::platform::paths`]**，这里只剩组合。
/// 先前这一段是「只修一半」的活体：`uid` 那半有两条 `#[cfg]` 臂，而兜底根目录那半
/// 是一句裸 `PathBuf::from("/tmp")`（`K-R52` 的 A2「真漏」堆第二条）。
/// ⇒ 两半现在住在一起，本函数留下的是 **tmux 的约定**（读 `TMUX_TMPDIR`、拼 `tmux-<uid>`），
/// 那不是平台语义，不该进适配层。
pub(super) fn tmux_socket_dir() -> PathBuf {
    let base = std::env::var_os("TMUX_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(crate::platform::paths::temp_root);
    base.join(format!("tmux-{}", crate::platform::paths::current_uid()))
}
/// P3：一次完整探测（跑在一次性后台线程里）。
///
/// 只在"可能有 server"时才问 pid/socket——`NoTmux`/`Unobservable` 下问了也是白问。
/// 注意 **`ServerEmpty` 也要问**：`exit-empty off` 下 server 活着但零会话。
pub(super) fn run_tmux_probe() -> TmuxProbe {
    let obs = run_tmux_ls();
    let (server_pid, socket_path) = match &obs {
        TmuxObservation::Sessions(_) | TmuxObservation::ServerEmpty | TmuxObservation::NoServer => {
            query_tmux_server()
        }
        TmuxObservation::NoTmux | TmuxObservation::Unobservable => (None, None),
    };
    TmuxProbe {
        obs,
        server_pid,
        socket_path,
    }
}

/// P1：四态 → wire。**`raw` 载荷刻意与 P1 之前逐字节一致**，新信息全部走 additive 的
/// `observation` 字段 ⇒ **旧 monitor 行为零变化**（有会话时它照旧解析 raw；零会话/出错时
/// 它看到空 raw、照旧保守跳过 = 今天的行为，无回归）。新 monitor 读 `observation` 才能
/// 区分"确证零会话"与"观测失败"，从而修掉灰灯卡死。
/// P5：从 `tmux ls` 的 `raw` 里取**会话名集合**。
///
/// 只依赖「名字在第一列」这一点 —— 列的构成由 `TMUX_LS_FMT` 定（**红线：不改它**），
/// 这里刻意不解析其余列，免得再造一处对格式的依赖。
fn session_names(raw: &str) -> std::collections::BTreeSet<String> {
    raw.lines()
        .map(|l| l.split('\t').next().unwrap_or(""))
        .map(str::trim)
        .filter(|n| !n.is_empty() && *n != "NO_TMUX")
        .map(str::to_string)
        .collect()
}

/// 一行 `tmux ls` 切成列；段数不等于 `TMUX_LS_FMT_FIELDS`（下溢 = 通道被改写 · 过溢 = 路径里有真 TAB）或名字空 ⇒ 整行丢掉
/// （`K-R12 J1`：通道被改写就不当好数据；本函数也被「raw 从别处来」的路径调得到，不许假设上游已经筛过）。
/// 回 `(会话名, 句柄, 这个会话挂着的 sid)`，sid 活动窗格那个在前。
fn ls_row(line: &str) -> Option<(&str, &str, Vec<String>)> {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() != TMUX_LS_FMT_FIELDS {
        return None;
    }
    let name = cols[0].trim();
    if name.is_empty() || name == "NO_TMUX" {
        return None;
    }
    let sids = crate::common::session_snapshot::pane_sids(cols[5], cols[7]);
    Some((name, cols[6].trim(), sids))
}

/// `K-R96`：把这一份观测切成**那张唯一的会话快照**认的行（`crate::common::session_snapshot::SessionRow`：
/// 会话名 ＋ 它某个窗格上的 `@ccm_sid`，挂着几个就几行）。
pub(crate) fn session_rows(raw: &str) -> Vec<crate::common::session_snapshot::SessionRow> {
    raw.lines()
        .filter_map(ls_row)
        .flat_map(|(name, _, sids)| crate::common::session_snapshot::rows_of(name, sids))
        .collect()
}

/// 会话账本读的那一份：会话句柄（`#{session_id}`，改名不变）→ 它各窗格挂着的 sid。
pub(crate) fn ledger_view(
    raw: &str,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    raw.lines()
        .filter_map(ls_row)
        .filter(|(_, id, _)| !id.is_empty())
        .map(|(_, id, sids)| (id.to_string(), sids.into_iter().collect()))
        .collect()
}

/// P5：与上一份快照差分，返回**这一轮消失了的会话名**（升序，`BTreeSet` 保证稳定）。
///
/// **差分而不是逐事件**，因为 SIGUSR1 会合并：一串 hook 同时打进来可能只醒一次，
/// 逐事件必漏；差分则一次报全。
///
/// **三态语义（最要紧的是第三条）**：
/// - `Sessions(raw)` ⇒ 现集 = raw 里的名字；消失 = 旧集 − 现集
/// - `ServerEmpty` / `NoServer` ⇒ 现集为空 ⇒ 旧集里**全部**算消失（server 没了，会话自然都没了）
/// - `NoTmux` / `Unobservable` ⇒ **「不知道」，绝不当作「都没了」** —— 观测失败时报一堆
///   死亡帧，会把活着的会话全部误 retire。此时**旧快照原样保留**，等下一次成功观测。
pub(super) fn diff_closed(
    prev: &mut Option<std::collections::BTreeSet<String>>,
    obs: &TmuxObservation,
) -> Vec<String> {
    diff_closed_into(prev, obs, crate::common::session_snapshot::global())
}

/// [`diff_closed`] 的本体，**快照由调用方给**。
///
/// 分出这一层只为一件事：让「发布进去的到底是什么」测得了，而**不去碰进程内那一份**
/// （测试是并行跑的，往全局那份里写会让别的判据随机红 —— 那是最贵的一种假红）。
fn diff_closed_into(
    prev: &mut Option<std::collections::BTreeSet<String>>,
    obs: &TmuxObservation,
    snapshot: &crate::common::session_snapshot::SessionSnapshot,
) -> Vec<String> {
    // ★★ `K-R96`（09-12）：观测到什么，就**往那张唯一的会话快照里焐一份**。
    //
    // `R52` 裁定一之后，「谁还活着」在本 crate 里只有一个数据结构
    //（`common::session_snapshot`）：control 侧的 Gate 判活与 `ccm` 铸名避让向它要，
    // observe 这边把每一轮观测发布进去。
    // ⚠ **焐热不等于「问过了」**：那边的 `query()` 一定重探（那是 `R52` 允许 Gate
    //   读快照的前提）。这里发布买的是「同一张表」，不是「省一次探测」。
    // ⚠ 三态的处置与下面 `now` 那三支**逐字同源**，刻意写在一起：
    //   观测无效那一支**什么都不发布**（快照不动），绝不把「不知道」写成「都没了」。
    let now = match obs {
        TmuxObservation::Sessions(raw) => {
            snapshot.publish(session_rows(raw));
            session_names(raw)
        }
        TmuxObservation::ServerEmpty | TmuxObservation::NoServer => {
            snapshot.publish(Vec::new());
            Default::default()
        }
        // 观测无效 ⇒ 什么都不结论，快照不动。
        TmuxObservation::NoTmux | TmuxObservation::Unobservable => return Vec::new(),
    };
    let closed = match prev.as_ref() {
        Some(old) => old.difference(&now).cloned().collect(),
        // 第一次观测没有「上一份」可比 ⇒ 不报任何死亡（否则后端一启动就诬告一批）。
        None => Vec::new(),
    };
    *prev = Some(now);
    closed
}

/// 一份 tmux 观测（[`observation_parts`] 的产物）**能不能拿来收割**：有会话 · 确证零会话 ⇒ 能；
/// 没装 tmux · 观测无效 ⇒ 不能（「不知道」绝不当成「都没了」）。与 [`observation_parts`] 读同一组 `OBS_*`，一个家。
pub(crate) fn tmux_view_is_observable(raw: &str, observation: Option<&str>) -> bool {
    match observation {
        Some(OBS_NO_TMUX) | Some(OBS_UNOBSERVABLE) => false,
        Some(OBS_ZERO_SESSIONS) if raw.trim().is_empty() => true,
        _ => !session_rows(raw).is_empty(),
    }
}

/// 一份观测 → 会话账本读的那两格 `(tmux ls 原文, 观测取值)`（原 `tmux_sessions` 帧的载荷，那一帧不再上线）。
pub(super) fn observation_parts(obs: TmuxObservation) -> (String, Option<&'static str>) {
    match obs {
        // 有会话：原文本身就说明是有会话。
        TmuxObservation::Sessions(raw) => (raw, None),
        // P3：两个细分映射到**同一个**取值（对收割完全等价）。
        TmuxObservation::ServerEmpty | TmuxObservation::NoServer => {
            (String::new(), Some(OBS_ZERO_SESSIONS))
        }
        TmuxObservation::NoTmux => (String::new(), Some(OBS_NO_TMUX)),
        TmuxObservation::Unobservable => (String::new(), Some(OBS_UNOBSERVABLE)),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/tmux_observe_tests.rs"]
mod tests;
