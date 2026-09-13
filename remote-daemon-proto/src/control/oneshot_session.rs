//! `K-R87`（2026-09-13）：**带看门狗的一次性会话** —— 起一个到点**自己会死**的 tmux 会话。
//!
//! # 它补的是哪个洞
//!
//! `K-R86` 刚把 [`super::capture_pane`] 做出来（抓一屏，只读，抓完即返回）。
//! 那是「看得见」那一半；本模块是**另一半**：**起一个有寿命的会话**。
//!
//! 在它之前 daemon 会建会话（[`super::launch`]）、会杀会话（[`super::kill`]），
//! **而「建出来的这个到点自己没」这件事一处都没有** —— monitor 侧的用量探针
//! （`src-tauri/src/account_usage.rs`）今天靠一条穿过 SSH 的 shell 串自己编排它，
//! 那条串里那一句 `setsid sh -c 'sleep N; tmux kill-session …'` 就是本模块要接过来的东西。
//!
//! ⚠ **它只是原语。** monitor 那条探针一个字节没动 —— 本件**不接线**。
//!
//! # 🔴 看门狗为什么可以存在（零定时器铁律的边界，逐字核过）
//!
//! `crate::no_timer_guard` 头注**逐字**写着它扫的人群是「本 crate `src/` 递归全部 `.rs`
//! …剥掉测试段之后的**源码文本**；依赖 crate 的源码、以及**被起进程的行为，都不在里面**」。
//! ⇒ 起一个**外部进程**让它到点回来杀会话，这个形状**在人群之外，合法**。
//!
//! 🔴 **反过来这条是铁律**：把那个「等 N 秒」搬进本文件（`thread::sleep` / 任何会让线程
//! 自己醒来的构件）**当场撞铁律** —— 那正是 `KR87D1` 死值验第三刀切的地方。
//!
//! ⚠ **别把「`no_timer_guard` 绿」读成「本件没引入自己醒来的东西」**：本件**确实**引入了
//! 一个会自己醒来的进程，只是它长在人群外面。这与 `no_timer_guard::g6_reach` 登记的
//! 那条反例（依赖 crate 里那条带期限的等待线程）**是同一格**，头注第三段已经如实登记过它。
//!
//! # 🔴 看门狗那条串**一个字都不过 shell 解析**（这是本模块的设计支点）
//!
//! monitor 那条是「渲染一整条 shell 串 → 交给远端的 `bash -lic`」，于是引号 / 转义 /
//! 注入是一整类必须一直防的问题（`account_usage` 那边靠 `shell_quote` 一层层包）。
//!
//! 本模块**不渲染串**：脚本是一个**常量**（[`WATCHDOG_SCRIPT`]），会话名 · 秒数 · socket ·
//! 句柄全部作为**位置参数**交给 `sh`，脚本里用 `"$1"` / `"$@"` 取。
//! ⇒ 这条路上**没有一处需要 quote**，也因此**不许**在这里长出第五份 POSIX quote
//! （`src-tauri/src/quote_singleton_guard.rs` 数着那件事，唯一实现住 `shell-quote-core`）。
//! 由本模块测试段的 `the_watchdog_hands_the_shell_a_constant_script_and_positional_arguments` 钉住。
//!
//! # 名字：**前缀专属，撞名不接回**（`KR87D2`）
//!
//! 会话名由 daemon **铸**出来：[`ONESHOT_PREFIX`] ＋ 调用方给的 slug。
//! 撞名（那个名字已经有人占着）⇒ **拒绝并说清**，绝不静默接回。
//! 先例逐字在 `control/ccm/mod.rs` 的 `NAME_TAKEN_FMT`：
//! 「已被占用 —— 拒绝静默接回别人的会话（C14：spawn 就是起）」。
//!
//! ⚠ 与 [`super::launch`] 的 `Mode::CreateOrAttach` **刻意相反**：那一条是幂等的
//! （会话在就什么都不做、回 `created:false`），而**那正是本模块不许有的行为** ——
//! 一次性会话接回一个别人的会话，等于给别人的会话挂上一条到点杀它的看门狗。
//!
//! ⚠ **`is_oneshot_name` 答的是「这个名字在不在 daemon 的名字空间里」，
//! 不是「这个会话是 daemon 建的」** —— 用户手工建一个同前缀的会话，它照样答 true。
//! 这**正是**撞名必须拒绝的理由，别把两句话读成一句。
//!
//! # 看门狗起不来 ⇒ **不许回「成功」，而且不留孤儿**（`KR87D3`）
//!
//! `platform/cfgless_guard.rs` 头注逐字：「门后那一臂**不许凭空返回一个「成功」值**」。
//! 这里的对应物是：看门狗那条外部进程起不来 ⇒ 刚建出来的那个会话**没有到点自己会死的保证**
//! ⇒ ① 把它杀掉（回滚）· ② 回一条 `watchdog_failed`，并在话里**说清回滚成没成**。
//!
//! ⚠ **它逮得住的只有「launcher 自己起不来」这一档**（`spawn()` 的 `Err`，内核级）。
//! `setsid` 起得来、而它 fork 之后 exec `sh` 失败（或那台机器没有 `sleep` / `tmux`）——
//! 那是 fork 之后的事，父进程结构上拿不到。**如实登记：回了 `Ok` 不等于看门狗真的武装好了。**
//! 要那一格得由调用方自己去看那个会话到点在不在（本模块的判据就是那样断的）。
//!
//! # ⚠ 一条平台账，写出来别装作没有
//!
//! 这条路是 **POSIX-only**：`setsid`（util-linux）＋ `sh`＋`sleep`。按 `K33` 裁定二
//! 「平台差异只许住适配层」它该住 `platform/`，**今天没住** —— 本件写区不含 `platform/`
//! （同 `K-R52` 给 `observe/watcher.rs` 那两处起 `sh` 写的签字栏：「该进适配层，今天没进」）。
//!
//! ⚠ 而 `platform/cfgless_guard.rs` 的 `posix-shell-sh` 那条针认的是
//! `Command::new("sh")` 这个**形状**，本处起的是 launcher、把 `sh` 当**参数**递进去
//! ⇒ **它看不见本处**。这不是判据坏了，是本处落在它的盲区里 —— 登记在这儿，交 PM 定夺。

use crate::common::tmux_utf8::UTF8_CLIENT_FLAG;
use std::process::{Command, Stdio};

/// 命令级错误：`(code, message)`。与 [`super::launch`] / [`super::kill`] /
/// [`super::capture_pane`] 同型。
pub(crate) type CmdErr = (&'static str, String);

/// daemon 一次性会话的**专属名字前缀**。
///
/// 🔴 它是 `KR87D2` 的承重物：本模块**只**在这个前缀底下铸名，也**只**对这个前缀底下的
/// 名字下手。改它等于把整个名字空间搬家，届时「谁是一次性会话」这个问题的答案全部改写。
pub(crate) const ONESHOT_PREFIX: &str = "ccm-oneshot-";

/// slug 的长度上界（字符数）。
///
/// 同 [`super::launch`] 的 `MAX_FIELD_BYTES`：这是「一个人读得懂的名字」的宽松上界，
/// **不是安全阈值**。tmux 自己对会话名没有这个限制。
pub(crate) const MAX_SLUG_CHARS: usize = 64;

/// 存活秒数的**下界**。
///
/// 它是**形状**不是策略：`0` 秒的看门狗等于没有看门狗 —— 会话在建出来的同一刻被杀，
/// 调用方拿到的是一个**已经死了**的名字，而那条回包会说它成功了。
///
/// # 🔴 上界：**只有 `u32` 那一个，而这是刻意的**〔`K-R87` 现打裁定，别当省略〕
///
/// 第一版这里还有一个 `MAX_TTL_SECS = 86_400`（一天）。`byte_cap_registry` 那条
/// 「每一处上限都要说清它管什么量、超限怎么办」当场逮到它，而回去重读那条上界的理由时
/// 发现它**说不出一句机制**：「多久算太久」是**偏好**，而 `K37` 逐字裁了
/// 「**后端只给机制，不给偏好**」。
/// ⇒ 处置是**重新裁定**（把那条上界撤掉），**不是**把它改名躲开判据
/// —— 改名会掉进同一份登记的默认拒绝那条（`every_size_typed_constant_is_either_a_cap_or_registered_as_not_one`），
/// 而躲判据本身就是本仓明令禁止的出路。
///
/// ⚠ **代价如实登记**：秒数给得足够大时，「到点自己会死」这条性质就**接近空**
/// （它仍然成立，只是那个「点」在很远的地方）。**那一格由调用方负责** ——
/// daemon 这一侧能给的机制是「到你给的那一刻它会死」，选那一刻是调用方的事。
/// ⚠ 另一条出路（PM 若认为上界该留）：往 `src-tauri/src/byte_cap_registry.rs` 的
/// `NOT_A_SIZE_CAP` 加一行说明「它量的是时间（秒），不是体量」——
/// **那份文件不在本件写区**，所以本轮走的是上面那条。
pub(crate) const MIN_TTL_SECS: u32 = 1;

/// 交给 `sh` 的那个 **`-c` 脚本 —— 它是常量，不是渲染出来的串**。
///
/// 🔴 **每一个变量都走位置参数**：`$1` = 秒数，`shift` 之后 `"$@"` = 要跑的那条 tmux argv。
/// ⇒ 会话名 / 句柄 / socket 路径**一个字都不进这段文本**，也就没有任何 quote 可写错。
///
/// 🔴 **`"$@"` 上那对双引号不是排版**：去掉之后 shell 会对每个位置参数做分词与 glob，
/// 而句柄形如 `$3`、目标形如 `=名:` —— 那时杀的可能是别的东西，或者什么都杀不到。
///
/// ⚠ 这里刻意用 `exec`：`sh` 把自己换成 tmux，少留一层进程。
pub(crate) const WATCHDOG_SCRIPT: &str = "sleep \"$1\"; shift; exec tmux \"$@\"";

/// 看门狗那条外部命令的 launcher —— **生产恒是它**。
///
/// 它做的事只有一件：把接下来那个进程放进**一个新的会话**（`setsid(2)`），
/// 于是它不再属于 daemon 这一趟 CLI 的进程组 —— 终端上一个 Ctrl-C 打到前台进程组时，
/// 它不会跟着死。**看门狗必须活得比起它的人久，这就是它存在的全部理由。**
pub(crate) const WATCHDOG_LAUNCHER: &str = "setsid";

/// 递给 `sh` 的程序名（`$0`）。只为了让 `ps` 上那一行看得出这是谁。
pub(crate) const WATCHDOG_ARGV0: &str = "ccm-watchdog";

/// POSIX shell 与它的「跑这段脚本」旗。**分成具名常量**是为了让
/// `crate::platform::cfgless_guard` 那条盲区（见模块头注末段）在这里指得住。
pub(crate) const POSIX_SHELL: &str = "sh";
/// 见 [`POSIX_SHELL`]。
pub(crate) const SHELL_SCRIPT_FLAG: &str = "-c";

/// `new-session -P -F <这个>` —— 让 tmux 在建完的同一刻把**句柄**打出来。
///
/// 🔴 拿句柄不是顺手：`#{session_id}`（tmux 的 `$N`）在 server 生命周期内唯一且**不复用**，
/// 而名字会被重新绑定。看门狗与回滚这两处都是**破坏性**动作 ⇒ 一律对句柄下手，不对名字
/// （与 `control/kill.rs` 头注那段 TOCTOU 分析同一条纪律）。
/// ★ 它**不额外起一次进程**：句柄是建会话那一次调用顺带打出来的。
pub(crate) const SESSION_ID_FMT: &str = "#{session_id}";

/// 一次性会话的**结局**：铸出来的名字 ＋ 它的句柄 ＋ 它还能活多久。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Oneshot {
    /// 铸出来的完整会话名（[`ONESHOT_PREFIX`] ＋ slug）。
    pub(crate) name: String,
    /// `#{session_id}`，形如 `$3`。
    pub(crate) handle: String,
    /// 看门狗会在多少秒后回来杀它。
    pub(crate) ttl_secs: u32,
}

/// 这个名字在不在 daemon 的一次性会话名字空间里。
///
/// ⚠ **它不答「这个会话是 daemon 建的」**（模块头注那一段）。
pub(crate) fn is_oneshot_name(name: &str) -> bool {
    name.starts_with(ONESHOT_PREFIX)
}

/// slug 允许的字符：`[A-Za-z0-9_-]`。
///
/// 比 `control/kill.rs::parse_name` **严格更窄**，三条理由各自独立：
/// ① `:` / `=` 是 tmux 目标语法的一部分（那一条的原话）；
/// ② 会话名要过 `common/session_snapshot.rs` 那张快照的 TAB 切分 —— 名字里带 TAB / 换行
///    会让那一行被判成段数下溢、**整行当坏数据丢掉**，于是这个会话在快照上凭空消失；
/// ③ 铸名方是 daemon 自己，**不是转发用户输入** ⇒ 值域由铸名方定，收窄零代价。
fn slug_char_ok(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// slug → 完整会话名。**唯一铸名处。**
pub(crate) fn mint_name(slug: &str) -> Result<String, CmdErr> {
    if slug.is_empty() {
        return Err((
            "invalid_args",
            "slug 为空 —— 一次性会话的名字是 daemon 铸的，而铸名要有个后缀".to_string(),
        ));
    }
    let n = slug.chars().count();
    if n > MAX_SLUG_CHARS {
        return Err((
            "invalid_args",
            format!("slug 有 {n} 个字符，上限 {MAX_SLUG_CHARS}"),
        ));
    }
    if let Some(bad) = slug.chars().find(|c| !slug_char_ok(*c)) {
        return Err((
            "invalid_args",
            format!(
                "slug 里有 {bad:?} —— 只收 ASCII 字母 / 数字 / `-` / `_`（理由见 `slug_char_ok`）：{slug:?}"
            ),
        ));
    }
    Ok(format!("{ONESHOT_PREFIX}{slug}"))
}

/// 存活秒数的形状校验。
pub(crate) fn checked_ttl(raw: &str) -> Result<u32, CmdErr> {
    // 上界就是 `u32` 本身 —— 那是**机制**（这个数要放得进一个 32 位无符号整数），
    // 不是**偏好**（「多久算太久」）。理由全文在 [`MIN_TTL_SECS`] 的头注。
    let secs: u32 = raw.parse().map_err(|_| {
        (
            "invalid_args",
            format!("存活秒数不是一个放得进 32 位无符号整数的十进制整数：{raw:?}"),
        )
    })?;
    if secs < MIN_TTL_SECS {
        return Err((
            "invalid_args",
            format!("存活秒数 {secs} 小于 {MIN_TTL_SECS}（理由见 `MIN_TTL_SECS`）"),
        ));
    }
    Ok(secs)
}

/// 备一条 tmux 命令。**全 crate 本模块唯一一处 `Command::new("tmux")`**。
///
/// `socket`：`None` = 让 tmux 自己按环境解析默认 socket（**生产恒 `None`**）；
/// `Some(p)` = 显式 `-S <p>`。它**不是配置口**，理由与 `capture_pane::spawn_capture`
/// 逐字同一条：要让「这个会话到点真的不在了」在一个**隔离的 tmux server** 上测得出来。
fn tmux_cmd(socket: Option<&str>) -> Command {
    let mut c = Command::new("tmux");
    if let Some(s) = socket {
        c.args(["-S", s]);
    }
    c.stdin(Stdio::null());
    c
}

/// tmux 起不来那一档的**唯一造句处**（同 `capture_pane::tmux_unavailable`）。
fn tmux_unavailable(e: &std::io::Error) -> CmdErr {
    (
        "no_tmux",
        format!("起不来 tmux（这台机装了吗？PATH 里有吗？）：{e}"),
    )
}

/// 建会话那一次调用的 argv。
///
/// 🔴 [`UTF8_CLIENT_FLAG`] **必须排在子命令之前**（`K-R12`：放后面是
/// `rc=1 + unknown flag -u`，而这里要读它的 stdout 拿句柄，那个响错会被读成「建不出来」）。
pub(crate) fn new_session_argv(name: &str) -> [&str; 8] {
    [
        UTF8_CLIENT_FLAG,
        "new-session",
        "-d",
        "-P",
        "-F",
        SESSION_ID_FMT,
        "-s",
        name,
    ]
}

/// 「这个目标在不在」那一次调用的 argv。
pub(crate) fn has_session_argv(target: &str) -> [&str; 4] {
    [UTF8_CLIENT_FLAG, "has-session", "-t", target]
}

/// 「杀掉这个句柄」那一次调用的 argv。**目标是句柄，不是名字**（见 [`SESSION_ID_FMT`]）。
pub(crate) fn kill_argv(handle: &str) -> [&str; 4] {
    [UTF8_CLIENT_FLAG, "kill-session", "-t", handle]
}

/// 看门狗那条外部命令的 argv（**不含 launcher 自己**）。
///
/// 形状：`sh -c <常量脚本> <argv0> <秒数> <tmux 的 argv…>`。
/// 后半段就是 [`kill_argv`]（前面可能挂着 `-S <socket>`）—— **本进程要它到点跑的，
/// 与本进程回滚时自己跑的，是同一条命令**。
pub(crate) fn watchdog_args(socket: Option<&str>, secs: u32, handle: &str) -> Vec<String> {
    let mut v = vec![
        POSIX_SHELL.to_string(),
        SHELL_SCRIPT_FLAG.to_string(),
        WATCHDOG_SCRIPT.to_string(),
        WATCHDOG_ARGV0.to_string(),
        secs.to_string(),
    ];
    if let Some(s) = socket {
        v.push("-S".to_string());
        v.push(s.to_string());
    }
    v.extend(kill_argv(handle).iter().map(|a| (*a).to_string()));
    v
}

/// 句柄的形状：`$` ＋ 十进制数字。
///
/// tmux 打回来的东西过不了这一关 ⇒ **不许拿它去下杀命令**（那时我们不知道自己在杀什么）。
fn checked_handle(raw: &str) -> Result<String, CmdErr> {
    let t = raw.trim();
    let ok = t.len() >= 2 && t.starts_with('$') && t[1..].chars().all(|c| c.is_ascii_digit());
    if !ok {
        return Err((
            "create_failed",
            format!(
                "tmux 建完会话打回来的句柄不是 `$<数字>` 的形状：{raw:?} —— \
                 拿它去下杀命令等于不知道自己在杀什么"
            ),
        ));
    }
    Ok(t.to_string())
}

/// 起那个会话（**还没挂看门狗**）。撞名 ⇒ `name_taken`，绝不接回。
fn create_session(socket: Option<&str>, name: &str) -> Result<String, CmdErr> {
    let out = tmux_cmd(socket)
        .args(new_session_argv(name))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| tmux_unavailable(&e))?;
    if out.status.success() {
        return checked_handle(&String::from_utf8_lossy(&out.stdout));
    }
    // 建不出来有两种：名字被占了（**幂等短路是别的模块的事，这里必须拒**），
    // 与真的建不出来。分不出来就等于把「接回别人的会话」藏进一句模糊的错误里。
    let target = super::launch::exact_target(name);
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if session_exists(socket, &target)? {
        return Err((
            "name_taken",
            format!(
                "tmux 会话名 {name} 已被占用 —— 拒绝静默接回别人的会话。\
                 一次性会话会给它挂一条到点杀它的看门狗，接回去等于替别人的会话定了死期。\
                 换一个 slug。tmux 原话：{stderr}"
            ),
        ));
    }
    Err((
        "create_failed",
        format!("建不出会话 {name}，而它也不存在。tmux 原话：{stderr}"),
    ))
}

/// 这个目标在不在。
fn session_exists(socket: Option<&str>, target: &str) -> Result<bool, CmdErr> {
    let st = tmux_cmd(socket)
        .args(has_session_argv(target))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| tmux_unavailable(&e))?;
    Ok(st.success())
}

/// 对句柄下 `kill-session`。回 `true` = tmux 说杀成了。
fn kill_handle(socket: Option<&str>, handle: &str) -> bool {
    tmux_cmd(socket)
        .args(kill_argv(handle))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|st| st.success())
        .unwrap_or(false)
}

/// 把看门狗放出去。**这一处是 `KR87D3` 的刀口。**
///
/// `launcher` 生产恒是 [`WATCHDOG_LAUNCHER`]；可注入是为了让「它起不来」这一档
/// 测得出来（理由同 [`tmux_cmd`] 的 `socket`，**不是配置口**）。
fn spawn_watchdog(
    launcher: &str,
    socket: Option<&str>,
    secs: u32,
    handle: &str,
) -> Result<(), CmdErr> {
    Command::new(launcher)
        .args(watchdog_args(socket, secs, handle))
        // 🔴 三条都必须 null：看门狗要活到 `secs` 秒之后，而它一旦继承了本进程的
        //    stdout，调用方读我们的 stdout 到 EOF 时就会跟着挂那么久。
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            (
                "watchdog_failed",
                format!("起不来看门狗 {launcher}（这台机上有吗？PATH 里有吗？）：{e}"),
            )
        })
}

/// [`start`] 的本体，**socket 与 launcher 由调用方给**（理由见 [`tmux_cmd`]）。
pub(crate) fn start_on(
    socket: Option<&str>,
    launcher: &str,
    slug: &str,
    ttl_secs: u32,
) -> Result<Oneshot, CmdErr> {
    let name = mint_name(slug)?;
    let handle = create_session(socket, &name)?;
    if let Err((code, why)) = spawn_watchdog(launcher, socket, ttl_secs, &handle) {
        // 🔴 **不留孤儿**：会话已经建出来了，而它没有到点自己会死的保证 ⇒ 现在就杀掉。
        //    回滚成没成**要说出来** —— 说不出来的那一档正是「假装成功」的近亲。
        let rolled_back = kill_handle(socket, &handle);
        let tail = if rolled_back {
            format!("刚建出来的会话 {name}（{handle}）已经回滚杀掉，没有留下孤儿")
        } else {
            format!(
                "而且**回滚也失败了** —— 会话 {name}（{handle}）现在是一个没有死期的孤儿，\
                 得有人去杀它"
            )
        };
        return Err((code, format!("{why}；{tail}")));
    }
    Ok(Oneshot {
        name,
        handle,
        ttl_secs,
    })
}

/// 起一个到点自己会死的一次性 tmux 会话。
///
/// ⚠ **刻意不过 §34 那三道门**（Gate 2 身份 / Gate 3 单窗口）：那三道门挡的是
/// 「往一个**不是本工具管理的**会话下手」，而本模块从头到尾只碰自己刚铸出来的名字
/// —— 撞名那一档在 [`create_session`] 里**拒掉**，根本走不到下手那一步。
/// **别顺手给它加门，也别顺手把那三道门搬过来。**
pub(crate) fn start(slug: &str, ttl_secs: u32) -> Result<Oneshot, CmdErr> {
    start_on(None, WATCHDOG_LAUNCHER, slug, ttl_secs)
}

/// 一次性 CLI 入口：`<slug> <存活秒数>`。
///
/// 成功 ⇒ stdout 一行紧凑 JSON `{session, handle, ttlSecs}` ＋ exit 0；
/// 失败 ⇒ stderr 一行 `{code, message}` ＋ exit 2（同 `--resolve` 那套信封）。
///
/// ⚠ **子命令那个 `--` 字面量刻意留在 `main.rs`，本文件一个都没有**：
/// `protocol_doc_guard::dispatch_registry_is_complete` 按「生产段里出现 `"--` 字面量」
/// 收人，本文件一旦持有它就得进 `DISPATCH_FILES`（那份文件不在本件写区）。
pub(crate) fn run(args: &[String]) -> i32 {
    let (Some(slug), Some(ttl)) = (args.get(1), args.get(2)) else {
        return emit_err((
            "invalid_args",
            "这条子命令收两个位置参数：会话名后缀 ＋ 存活秒数（秒数没有默认值，\
             后端只给机制不给偏好）"
                .to_string(),
        ));
    };
    if args.len() > 3 {
        return emit_err((
            "invalid_args",
            format!("多余参数 {:?} —— 这条子命令只收两个位置参数", &args[3..]),
        ));
    }
    let secs = match checked_ttl(ttl) {
        Ok(s) => s,
        Err(e) => return emit_err(e),
    };
    match start(slug, secs) {
        Ok(o) => {
            let line = serde_json::json!({
                "session": o.name,
                "handle": o.handle,
                "ttlSecs": o.ttl_secs,
            });
            println!("{line}");
            0
        }
        Err(e) => emit_err(e),
    }
}

/// 错误信封：stderr 一行 JSON ＋ 退出码 2。
///
/// ⚠ **如实登记：这是本 crate 第 4 份同形的 `emit_err`** —— 另三处现打在
/// `control/cli_control.rs` · `control/capture_pane.rs` · `control/resolve_query.rs`。
/// 收口要动那三份文件，**本件写区不含它们** ⇒ 登记在这里，交 PM 立跟进件，
/// 不在这儿假装它是新东西。
fn emit_err((code, message): CmdErr) -> i32 {
    let line = serde_json::json!({ "code": code, "message": message });
    eprintln!("{line}");
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 隔离 socket 的路径 —— 每个测试一个，**显式 `-S`**，不碰任何默认 socket。
    fn iso_socket(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ccm-kr87-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join("sock")
    }

    /// 在隔离 socket 上跑一条 tmux 命令（**夹具侧**，不是被测行为）。
    fn tmux_on(sock: &std::path::Path, args: &[&str]) -> std::process::Output {
        Command::new("tmux")
            .arg("-S")
            .arg(sock)
            .args(args)
            .output()
            .expect("tmux 不可执行 —— 本测试要求环境有 tmux（刻意不静默跳过）")
    }

    /// 那个会话此刻在不在（走**生产**那条 `has-session`）。
    fn alive(sock: &std::path::Path, name: &str) -> bool {
        session_exists(
            Some(&sock.to_string_lossy()),
            &super::super::launch::exact_target(name),
        )
        .expect("has-session 问得出来")
    }

    /// 夹具侧的有界等待：等到 `f()` 为真，或超出上限就返回 false。
    ///
    /// 🔴 这段循环住在**测试段**（`production_code` 剥掉它）——
    /// 零定时器铁律禁的是**生产段**里的构件，本 crate 的判据全部按剥完之后的文本扫。
    fn wait_until(mut f: impl FnMut() -> bool, max_polls: u32) -> bool {
        for _ in 0..max_polls {
            if f() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        f()
    }

    // ══════════════════════════ `KR87D1` ══════════════════════════

    /// ★★ `KR87D1` 的正题：**那个会话到点真的不在了** —— 而同一台 server 上
    /// 没挂看门狗的那个**还在**。
    ///
    /// # 失效方向（件文件逐字点名的那个）：别判「串里有 `setsid` / `sleep` 字面量」
    ///
    /// `control/ccm/plan.rs` 今天就有一条同形的串，按字面量判会**恒绿**。
    /// 本条断的是**结果**：起一个 ttl=1 的会话，① 刚建出来时它在 ② 到点之后它不在。
    ///
    /// # 🔴 那个阴性对照不是装饰
    ///
    /// 同一台 server、同一段时间里另起一个**没有看门狗**的会话。
    /// 没有它，「会话不在了」还可以是「server 塌了」「socket 被换了」「夹具把整棵树清了」——
    /// 三种都会让本条以正确的理由绿掉，而看门狗一行没跑。
    #[test]
    fn the_session_is_gone_at_its_deadline_while_its_neighbour_stays() {
        let sock = iso_socket("live");
        let s = sock.to_string_lossy().into_owned();
        // 阴性对照：一个**没有看门狗**的普通会话，由夹具直接建。
        let control = "kr87-plain";
        let out = tmux_on(
            &sock,
            &["-f", "/dev/null", "new-session", "-d", "-s", control],
        );
        assert!(
            out.status.success(),
            "隔离 socket 上建对照会话失败：{}",
            String::from_utf8_lossy(&out.stderr)
        );

        let o = start_on(Some(&s), WATCHDOG_LAUNCHER, "live", 1)
            .expect("一次性会话必须起得起来，起不来说明这条原语根本没通");
        assert_eq!(o.name, format!("{ONESHOT_PREFIX}live"));
        assert!(o.handle.starts_with('$'), "句柄形状不对：{:?}", o.handle);
        assert!(
            alive(&sock, &o.name),
            "刚建出来的一次性会话居然不在 —— 那后面「它没了」证明不了任何事"
        );

        // 到点之后它必须没。上限给足（40 × 50ms = 2s 之外再加 ttl 那 1s 的余量）。
        let gone = wait_until(|| !alive(&sock, &o.name), 80);
        assert!(
            gone,
            "过了 {} 秒那个一次性会话还在 —— 看门狗没起作用（或根本没起来）",
            o.ttl_secs
        );

        // ★ 阴性对照仍在：证明上面那个「没了」是被杀的，不是整台 server 塌了。
        assert!(
            alive(&sock, control),
            "同一台 server 上没挂看门狗的那个会话也没了 —— \
             那说明消失的原因不是看门狗，本条上面那格是假绿"
        );

        let _ = tmux_on(&sock, &["kill-session", "-t", "=kr87-plain:"]);
    }

    /// ★ 看门狗那条 argv **逐元素**：脚本是常量、变量全走位置参数。
    ///
    /// 这一格与上面那条互补：上面断「它真的死了」，这一条断「它是**照这个形状**死的」——
    /// 换成把名字渲染进脚本串（那时就要写 quote 了）这一条会红。
    #[test]
    fn the_watchdog_hands_the_shell_a_constant_script_and_positional_arguments() {
        let args = watchdog_args(Some("/tmp/kr87/sock"), 7, "$9");
        assert_eq!(
            args,
            vec![
                "sh",
                "-c",
                "sleep \"$1\"; shift; exec tmux \"$@\"",
                "ccm-watchdog",
                "7",
                "-S",
                "/tmp/kr87/sock",
                "-u",
                "kill-session",
                "-t",
                "$9",
            ],
            "看门狗的 argv 变了 —— 脚本必须是常量，秒数 / socket / 句柄一律走位置参数"
        );
        // 生产形（无 socket）少那两格，别的逐字相同。
        let prod = watchdog_args(None, 7, "$9");
        assert_eq!(
            prod.len() + 2,
            args.len(),
            "带不带 socket 只该差 `-S <路径>` 两格"
        );
        assert!(
            !prod.iter().any(|a| a == "-S"),
            "生产形里出现了 `-S` —— socket 不是配置口"
        );
        // ★ 反向：脚本里**不许**出现被 quote 的内容。句柄与目标只能以位置参数出现。
        assert!(
            !WATCHDOG_SCRIPT.contains('9') && !WATCHDOG_SCRIPT.contains("kill"),
            "脚本里出现了这一趟的具体值 —— 它就不再是常量了：{WATCHDOG_SCRIPT:?}"
        );
    }

    /// ★ 看门狗跑的那条 tmux 命令，与本进程回滚时跑的**是同一条**。
    ///
    /// 两处各写一份的话，改了一处不会红，而「到点杀的是别的东西」这种错**只在到点那一刻**现形。
    ///
    /// # ⚠ 它要断的是**两侧**，而第一版只断了一侧（收工前自查补的）
    ///
    /// 第一版只比「看门狗那条 argv 的尾巴 == [`kill_argv`]」—— 那证的是**看门狗**这一侧。
    /// **回滚**那一侧（[`kill_handle`]）当时**没有任何东西在证它也走同一份** ⇒
    /// 把 `kill_handle` 里的 argv 换成手抄的一条，本条**照样绿**，而它的标题写着「是同一条」。
    /// ⇒ 这正是本轮头号病理（**量具的作用域对不上它声称守的面**）长在我自己的判据上。
    /// 下面第二格补的就是那一侧：`kill_handle` 的**函数体**里必须真的出现那次调用。
    #[test]
    fn the_watchdog_runs_the_same_kill_command_the_rollback_runs() {
        // ① 看门狗那一侧：argv 的尾巴逐元素就是 `kill_argv` 那一份。
        let mine: Vec<String> = kill_argv("$4").iter().map(|a| (*a).to_string()).collect();
        let w = watchdog_args(None, 3, "$4");
        assert!(
            w.ends_with(&mine),
            "看门狗那条命令的尾巴不是 `kill_argv` 那一份：{w:?} / {mine:?}"
        );
        // ② 回滚那一侧：`kill_handle` 的函数体里必须真的调它。
        //    ⚠ 针**在测试段**，而扫的是**生产段**（`production_code` 剥掉测试段）
        //    ⇒ 本条不会在自己的文本里找到自己（`ratchet_guard` 头注那条纪律）。
        let prod = crate::guard_support::production_code(include_str!("oneshot_session.rs"));
        assert!(
            prod.len() > 3_000,
            "剥完只剩 {} 字节 —— 剥过头了，下面那格在空转",
            prod.len()
        );
        let after = prod
            .split("fn kill_handle(")
            .nth(1)
            .expect("生产段里找不到 `kill_handle` —— 回滚那一处换了住址，回来重判本条");
        // ⚠ **切到下一个顶层 `fn` 为止，刻意不按大括号切**：`readonly_guard` 的剥法靠
        //    大括号配平找测试模块的边界，而**注释或字符串里出现一个落单的右大括号字面量**
        //    会让它提前收尾 ——「剥完仍有测试属性残留在生产段里」当场红。
        //    ⚙ 本轮现打踩了**两次**：第一次是这里原本写成按大括号切，第二次是
        //    **解释这件事的那句注释自己写了一个落单的右大括号**。逐字记在这里。
        let body = after.split("\nfn ").next().unwrap_or(after);
        assert!(
            body.contains("kill_argv("),
            "回滚那一处没走 `kill_argv` —— 它和看门狗从此各写一份 argv，\
             而「到点杀的是别的东西」这种错只在到点那一刻现形。那一段是：{body:?}"
        );
    }

    // ══════════════════════════ `KR87D2` ══════════════════════════

    /// ★★ `KR87D2` 的正题：**撞名拒绝，而且不碰别人那个会话一根汗毛。**
    ///
    /// 三格，缺一不可：
    /// 1. 回的是 `name_taken`（不是 `Ok`，也不是被压成 `create_failed`）；
    /// 2. 那个既有会话**当场还在**（我们没顺手杀它、没接回它）；
    /// 3. 🔴 **过了那个 ttl 它还在** —— 也就是我们**没有偷偷给别人的会话挂上看门狗**。
    ///    只断前两格的话，「先接回、再挂狗」这种实现会全绿，而它正是本条要防的那一下。
    #[test]
    fn a_taken_name_is_refused_and_the_other_session_is_left_untouched() {
        let sock = iso_socket("taken");
        let s = sock.to_string_lossy().into_owned();
        let squatted = format!("{ONESHOT_PREFIX}taken");
        let out = tmux_on(
            &sock,
            &["-f", "/dev/null", "new-session", "-d", "-s", &squatted],
        );
        assert!(
            out.status.success(),
            "建占位会话失败：{}",
            String::from_utf8_lossy(&out.stderr)
        );

        let e = start_on(Some(&s), WATCHDOG_LAUNCHER, "taken", 1)
            .expect_err("撞名不许回成功 —— 那就是静默接回");
        assert_eq!(
            e.0, "name_taken",
            "撞名回的码是 `{}` —— 它被压进别的档里了，调用方分不出「换个名字」与「这台机器坏了」。实得：{e:?}",
            e.0
        );
        assert!(alive(&sock, &squatted), "撞名之后别人的会话被我们弄没了");

        // ★ 第三格：ttl 都过完了，它还得在。
        let still = !wait_until(|| !alive(&sock, &squatted), 60);
        assert!(
            still,
            "撞名被拒之后，别人的会话在 ttl 到点时消失了 —— \
             说明我们**先接回、再挂了看门狗**，那正是 `KR87D2` 要防的那一下"
        );

        let _ = tmux_on(&sock, &["kill-session", "-t", &format!("={squatted}:")]);
    }

    /// ★ `KR87D2` 第三刀：探针会话与用户会话**在同一张真快照上分得开**。
    ///
    /// 语料是**真 tmux 打回来的那一份会话清单**（不是手写的假串）。
    #[test]
    fn a_real_listing_separates_the_oneshot_namespace_from_user_sessions() {
        let sock = iso_socket("census");
        let s = sock.to_string_lossy().into_owned();
        let user = "kr87census-cc";
        assert!(
            tmux_on(&sock, &["-f", "/dev/null", "new-session", "-d", "-s", user])
                .status
                .success(),
            "建用户会话失败"
        );
        let o = start_on(Some(&s), WATCHDOG_LAUNCHER, "census", 60).expect("一次性会话起得起来");

        let out = tmux_on(&sock, &["list-sessions", "-F", "#{session_name}"]);
        assert!(out.status.success(), "列不出会话");
        let names: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        assert!(
            names.contains(&o.name) && names.iter().any(|n| n == user),
            "这张快照里两个会话不是都在，下面的划分是空转的：{names:?}"
        );
        let ours: Vec<&String> = names.iter().filter(|n| is_oneshot_name(n)).collect();
        assert_eq!(
            ours,
            vec![&o.name],
            "同一张快照上「哪些是一次性会话」划错了：{names:?}"
        );

        let _ = tmux_on(&sock, &["kill-session", "-t", &format!("={}:", o.name)]);
        let _ = tmux_on(&sock, &["kill-session", "-t", &format!("={user}:")]);
    }

    /// ★ 名字空间的边界：**只有前缀底下的才算我们的**，形近的一律不算。
    #[test]
    fn only_names_under_the_prefix_belong_to_the_oneshot_namespace() {
        assert!(is_oneshot_name(&format!("{ONESHOT_PREFIX}x")));
        assert!(is_oneshot_name(ONESHOT_PREFIX));
        for outsider in ["ccm-oneshot", "x-ccm-oneshot-y", "ccm-onesho-x", "", "cc-x"] {
            assert!(
                !is_oneshot_name(outsider),
                "{outsider:?} 被算成了 daemon 的一次性会话 —— 名字空间漏了"
            );
        }
    }

    /// ★ 铸名：形状过不了的 slug **在起进程之前**就被拒。
    #[test]
    fn a_slug_that_would_break_the_target_or_the_snapshot_never_reaches_tmux() {
        for bad in ["", "a:b", "=a", "a b", "a\nb", "a'b", "a$b", "a/b", "甲"] {
            let e = mint_name(bad).expect_err("坏形状的 slug 不许放行");
            assert_eq!(
                e.0, "invalid_args",
                "slug {bad:?} 没被形状检查拦住。实得：{e:?}"
            );
        }
        assert_eq!(
            mint_name("usage-acct1").expect("正常 slug 要过"),
            format!("{ONESHOT_PREFIX}usage-acct1")
        );
        let long: String = "a".repeat(MAX_SLUG_CHARS + 1);
        assert!(mint_name(&long).is_err(), "超长 slug 该被拒");
    }

    /// ★ 存活秒数的形状：**没有默认值**，`0` 拒，放不进 `u32` 的拒。
    ///
    /// ⚠ 这里**刻意不断「超过某个上界要拒」** —— 那条上界本轮被撤掉了，
    /// 理由（它是偏好不是机制）逐字在 `MIN_TTL_SECS` 的头注。
    /// 断一条不存在的规矩比不断更坏（`testing.md` 诚实边界第 4 条）。
    #[test]
    fn the_deadline_has_no_default_and_zero_is_refused() {
        assert_eq!(checked_ttl("30").expect("正常值要过"), 30);
        for bad in ["", "0", "-1", "1.5", "abc", "99999999999", " 30"] {
            let e = checked_ttl(bad).expect_err("坏形状的秒数不许放行");
            assert_eq!(e.0, "invalid_args", "秒数 {bad:?} 没被拦住：{e:?}");
        }
        assert!(checked_ttl(&MIN_TTL_SECS.to_string()).is_ok());
        // 上界只有 `u32` 那一个：它自己放得进去，就该过。
        assert_eq!(
            checked_ttl(&u32::MAX.to_string()).expect("`u32::MAX` 放得进 u32，该过"),
            u32::MAX
        );
    }

    // ══════════════════════════ `KR87D3` ══════════════════════════

    /// ★★ `KR87D3` 的正题：**看门狗起不来时回的是错误，而且没留下孤儿。**
    ///
    /// 三格：① 不是 `Ok` ② 码是 `watchdog_failed`（不是被压进 `create_failed`）
    /// ③ 🔴 刚建出来那个会话**已经不在了** —— 「说得出」与「不留孤儿」是两件事，各断各的。
    #[test]
    fn a_watchdog_that_cannot_start_is_never_reported_as_success() {
        let sock = iso_socket("nodog");
        let s = sock.to_string_lossy().into_owned();
        // 一个**一定起不来**的 launcher。名字取中性，且不取自本夹具的目录名（`6g`）。
        let e = start_on(Some(&s), "kr87-no-such-program", "nodog", 60)
            .expect_err("看门狗起不来时不许回成功 —— 那就是「凭空返回一个成功值」");
        assert_eq!(
            e.0, "watchdog_failed",
            "看门狗起不来回的码是 `{}` —— 压进别的档里，调用方就分不出「这台机器缺 setsid」\
             与「tmux 建不出会话」。实得：{e:?}",
            e.0
        );
        assert!(
            !alive(&sock, &format!("{ONESHOT_PREFIX}nodog")),
            "看门狗没起来，而那个会话还留在盘上 —— 那是一个**没有死期的孤儿**，\
             正是 `KR87D3` 标题里那半句"
        );
        // 回滚成没成必须在话里说出来（说不出来的那一档是「假装成功」的近亲）。
        assert!(e.1.contains("回滚"), "错误话里没说回滚这件事：{}", e.1);
    }

    /// ★ 成功那一档**不是**靠「没报错」判的：起得来的时候要真的拿到名字与句柄。
    ///
    /// 没有这一条，把 `start_on` 改成恒 `Err` 也能让上面那几条全绿。
    #[test]
    fn the_success_arm_really_carries_a_name_and_a_handle() {
        let sock = iso_socket("ok");
        let s = sock.to_string_lossy().into_owned();
        let o = start_on(Some(&s), WATCHDOG_LAUNCHER, "ok", 60).expect("这一趟必须成功");
        assert!(is_oneshot_name(&o.name), "回的名字不在专属前缀底下：{o:?}");
        assert!(
            o.handle.len() >= 2 && o.handle.starts_with('$'),
            "回的句柄不是 `$<数字>`：{o:?}"
        );
        assert_eq!(o.ttl_secs, 60);
        assert!(alive(&sock, &o.name), "回了成功，而那个会话并不在");
        let _ = tmux_on(&sock, &["kill-session", "-t", &format!("={}:", o.name)]);
    }

    /// ★ 句柄形状校验真的会拒 —— tmux 打回来的东西不对时**不许**拿它去下杀命令。
    #[test]
    fn a_handle_that_is_not_a_session_id_is_refused_before_anything_is_killed() {
        for bad in ["", "$", "3", "$3a", "abc", "$3 $4", "-t"] {
            assert!(
                checked_handle(bad).is_err(),
                "句柄 {bad:?} 没被形状检查拦住"
            );
        }
        assert_eq!(checked_handle(" $12\n").expect("正常句柄要过"), "$12");
    }

    /// ★ CLI 那一层的形状：参数个数不对时**在起任何进程之前**就拒。
    #[test]
    fn the_cli_arm_refuses_the_wrong_number_of_positional_arguments() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // 只有子命令自己 / 少一个 / 多一个 —— 三种都必须 exit 2。
        assert_eq!(run(&v(&["x"])), 2);
        assert_eq!(run(&v(&["x", "slug"])), 2);
        assert_eq!(run(&v(&["x", "slug", "10", "extra"])), 2);
        // 秒数形状不对也走同一条早退（不许摸 tmux）。
        assert_eq!(run(&v(&["x", "slug", "0"])), 2);
    }
}
