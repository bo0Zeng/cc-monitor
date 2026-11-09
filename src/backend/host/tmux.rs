//! tmux 那一种宿主：起 `tmux` 进程只在这一份文件里（[`child`]，与观测那两跳 `sh -c`）。
//!
//! 别处要对 tmux 下命令，经 [`child`] 拿一个已指好 server 的命令，argv 由调用方给；
//! 经 [`child`] 起的算调用方那一处起进程：[`child`] 在后端 `clippy.toml` 的名单上，每个调用点放行、写清发什么。
//! 名单（`list-panes` · `list-clients` 两发）、一次列全部会话（`list-sessions`，快照的探测器）也住这里。

use super::TermRow;
use crate::common::session_snapshot::SessionRow;
use crate::common::tmux_utf8::{tab_underflow, UTF8_CLIENT_ENV, UTF8_CLIENT_FLAG};
use crate::platform::child::{Child, ChildFail, Deadline};
use copy_core::copy_text;

/// 命令级错误：码 ＋ 那一句 ＋ 下层原话。
use crate::common::said::Said;

/// 起 `tmux` 的唯一构造：`socket`：`None` = 让 tmux 按环境解析默认 socket（生产恒 `None`）；
/// `Some(p)` = 显式 `-S <p>`（判据在隔离 socket 上起真 tmux）。
#[expect(
    clippy::disallowed_methods,
    reason = "起 `tmux` 的唯一构造：只指 server、不定子命令；发什么由各调用点放行时写清"
)]
pub fn child(socket: Option<&str>) -> Child {
    let c = Child::new("tmux");
    match socket {
        Some(s) => c.args(["-S", s]),
        None => c,
    }
}

/// 对 tmux 下一发的期限（列名单 · 问尺寸 · 抓屏 · 打标 · 建会话 · 送键 · 装钩子 · 停会话 · 观测探测）：一发 5 s。
/// 健康机器上 tmux 一发是毫秒级；5 s 容忍一次慢盘 / 高负载，又远短于界面等这些命令的预算与「面板不更新」被注意到的时间。
/// 起 tmux 的各处一律取它，不另写字面量；整条命令的总预算（各命令的 `*_CAP`）是几发加起来的上限，另算。
pub const TMUX_WITHIN: Deadline = Deadline::secs(5);

/// 句柄前缀（不透明：前端不拼、不解析；后端只按名单对）。
const HANDLE_PREFIX: &str = "tmux-";

/// 名单那一趟问 tmux 的格式（逐窗格）：会话 ID · 窗格 ID · 是不是当前窗格 · 是不是当前窗口 · 名 · 窗口数 · 最后活动 ·
/// `@ccm_sid`（窗格上的，没有就是会话那一级的）· `@ccm_agent` · `@ccm_client` · 前台程序 · 工作目录 · 窗格标题
/// （后两格是自由文本，排在最后）。
const PANES_FMT: &str = "#{session_id}\t#{pane_id}\t#{pane_active}\t#{window_active}\t#{session_name}\t#{session_windows}\t#{session_activity}\t#{@ccm_sid}\t#{@ccm_agent}\t#{@ccm_client}\t#{pane_current_command}\t#{pane_current_path}\t#{pane_title}";
const PANES_FIELDS: usize = 13;

/// 连着各会话的 tmux 客户端：会话 ID · 接上的时刻 · 最后活动。
/// 控制模式客户端（终端实时预览那个只读、不改尺寸的订阅者，`terminal_follow.rs`）整行留空 ⇒ 解析时丢掉：它不是终端窗口。
const CLIENTS_FMT: &str =
    "#{?client_control_mode,,#{session_id}\t#{client_created}\t#{client_activity}}";

/// 问哪台 tmux：生产恒是默认 socket（`socket: None`）；判据在隔离 socket 上起真 tmux。
#[derive(Debug, Clone, Copy, Default)]
pub struct On<'a> {
    pub socket: Option<&'a str>,
}

impl On<'_> {
    /// 只读一发（名单 · 问尺寸 · 问版本）。
    #[expect(
        clippy::disallowed_methods,
        reason = "只读 tmux：终端名单两发、调用方问尺寸光标 · 问版本；不许经它发改状态的子命令"
    )]
    pub fn read(&self, args: &[&str]) -> Result<std::process::Output, ChildFail> {
        child(self.socket).args(args).run(TMUX_WITHIN)
    }
}

/// 终端句柄：tmux 会话 ID（`$N`）＋ 挂着 sid 的窗格（`%M`；没有 ⇒ 只指会话）。名单发的、容器那一格报的都由它算（一个家）。
pub fn handle_of(session_id: &str, pane: Option<&str>) -> String {
    let id = session_id.trim_start_matches('$');
    match pane {
        Some(p) => format!("{HANDLE_PREFIX}{id}-{}", p.trim_start_matches('%')),
        None => format!("{HANDLE_PREFIX}{id}"),
    }
}

/// `list-panes -a` 的一行。
struct PaneLine<'a> {
    id: &'a str,
    pane: &'a str,
    current: bool,
    f: Vec<&'a str>,
    windows: u32,
    activity: u64,
}

/// `list-panes -a` 的原文 ⇒ 行（纯函数）。回 `(行, 有没有读不懂的行)`。
///
/// 每个 tmux 会话：挂着 sid 的窗格各一行（同一个 sid 挂在几个窗格上 ⇒ 当前窗格那个，否则第一个）；一个都没挂 ⇒ 当前窗格那一行代表整个会话。
pub fn parse_rows(text: &str) -> (Vec<TermRow>, bool) {
    let mut odd = false;
    let mut panes: Vec<PaneLine> = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.splitn(PANES_FIELDS, '\t').collect();
        let windows = f.get(5).and_then(|w| w.parse().ok());
        let activity = f.get(6).and_then(|a| a.parse().ok());
        match (
            f.len() == PANES_FIELDS && f[0].starts_with('$') && f[1].starts_with('%'),
            windows,
            activity,
        ) {
            (true, Some(windows), Some(activity)) => panes.push(PaneLine {
                id: f[0],
                pane: f[1],
                current: f[2] == "1" && f[3] == "1",
                windows,
                activity,
                f,
            }),
            _ => odd = true,
        }
    }
    let row = |l: &PaneLine, pane: Option<String>, shared: bool| TermRow {
        host: crate::wire::TerminalHost::Tmux,
        id: l.id.into(),
        pane,
        shared,
        name: l.f[4].into(),
        windows: l.windows,
        activity: l.activity,
        sid: l.f[7].into(),
        agent: l.f[8].into(),
        client: l.f[9].into(),
        program: l.f[10].into(),
        cwd: l.f[11].into(),
        title: l.f[12].into(),
        input: super::Input::Shared,
    };
    let mut rows = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for l in &panes {
        if seen.contains(&l.id) {
            continue;
        }
        seen.push(l.id);
        let mine: Vec<&PaneLine> = panes.iter().filter(|q| q.id == l.id).collect();
        // 挂着同一个 sid 的几个窗格 ⇒ 照门那一份挑法（[`carrier_by`]）挑一个。
        let mut tagged: Vec<&PaneLine> = Vec::new();
        for q in mine.iter().filter(|q| !q.f[7].is_empty()) {
            if tagged.iter().any(|t| t.f[7] == q.f[7]) {
                continue;
            }
            if let Some(c) = carrier_by(&mine, q.f[7], |p| p.f[7], |p| p.current) {
                tagged.push(*c);
            }
        }
        if tagged.is_empty() {
            let cur = mine.iter().find(|q| q.current).unwrap_or(&mine[0]);
            rows.push(row(cur, None, false));
        } else {
            let shared = tagged.len() > 1;
            rows.extend(tagged.iter().map(|q| row(q, Some(q.pane.into()), shared)));
        }
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    (rows, odd)
}

/// 挂着 `sid` 的那一个：同一个 sid 挂在几处（会话那一级的旧标签往下透）⇒ 当前窗格那个，否则第一个；
/// 空 `sid` ⇒ 没有（没挂标签的窗格不算「挂着空 sid」）。名单去重（[`parse_rows`]）与门（[`carrier`]）同一个挑法。
pub fn carrier_by<'a, T>(
    items: &'a [T],
    sid: &str,
    sid_of: impl Fn(&T) -> &str,
    current: impl Fn(&T) -> bool,
) -> Option<&'a T> {
    if sid.is_empty() {
        return None;
    }
    let mut hits = items.iter().filter(|p| sid_of(p) == sid);
    let first = hits.clone().next();
    hits.find(|p| current(p)).or(first)
}

/// 一个会话里的一个窗格（[`panes_on`] 的一行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneTag {
    /// `#{pane_id}`（`%N`，server 生命周期内不复用 ⇒ 对它下手同对 `#{session_id}` 下手一样关得住 TOCTOU）。
    pub pane: String,
    /// 根进程 pid（杀成之后按它认 cc-bus 名册）。
    pub pid: u32,
    /// 它挂着的 `@ccm_sid`（窗格上的；没有就是会话那一级的；都没有 ⇒ 空串）。
    pub sid: String,
    /// 是不是会话此刻的当前窗格（当前窗口的活动窗格）。
    pub current: bool,
}

/// 列一个会话全部窗格的格式：句柄 · 根进程 pid · `@ccm_sid` · 是不是当前窗格（窗格活动、窗口也活动 ⇒ `11`）。
const SESSION_PANES_FMT: &str =
    "#{pane_id}\t#{pane_pid}\t#{@ccm_sid}\t#{pane_active}#{window_active}";

/// 这个会话（`#{session_id}` 或 `=名:`）的全部窗格。只读 tmux；问不到 ⇒ 空。
pub fn panes_on(on: On<'_>, session: &str) -> Vec<PaneTag> {
    on.read(&[
        UTF8_CLIENT_FLAG,
        "list-panes",
        "-F",
        SESSION_PANES_FMT,
        "-s",
        "-t",
        session,
    ])
    .map(|o| parse_panes(&String::from_utf8_lossy(&o.stdout)))
    .unwrap_or_default()
}

/// [`panes_on`] 的原文 ⇒ 行（纯函数）。读不懂的行丢掉（段数不对 = 打印通道被改写，宁可当它不在）。
pub fn parse_panes(text: &str) -> Vec<PaneTag> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            match f.as_slice() {
                [pane, pid, sid, cur] if pane.starts_with('%') => Some(PaneTag {
                    pane: pane.to_string(),
                    pid: pid.trim().parse().ok()?,
                    sid: sid.to_string(),
                    current: *cur == "11",
                }),
                _ => None,
            }
        })
        .collect()
}

/// 挂着 `sid` 的那个窗格（挑法见 [`carrier_by`]）。
pub fn carrier<'a>(panes: &'a [PaneTag], sid: &str) -> Option<&'a PaneTag> {
    carrier_by(panes, sid, |p| p.sid.as_str(), |p| p.current)
}

/// tmux 的 pane id 形状：`%` + 十进制数字。空串必须挡住：`-t ''` 不报错，静默解析成「当前 / 最近的那个会话」。
pub fn pane_id_ok(pane: &str) -> bool {
    pane.strip_prefix('%')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `$TMUX` 的值（`socket,pid,session`）⇒ socket 路径；空 ⇒ 没有。
pub fn socket_of(tmux_env: &str) -> Option<&str> {
    tmux_env.split(',').next().filter(|s| !s.is_empty())
}

/// `list-clients` 的原文 ⇒ `[(会话 ID, 接上时刻, 最后活动)]`（纯函数；读不懂的行丢掉）。
pub fn parse_clients(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            match f.as_slice() {
                [id, since, last] => {
                    Some((id.to_string(), since.parse().ok()?, last.parse().ok()?))
                }
                _ => None,
            }
        })
        .collect()
}

/// 名单 ＋ 各会话的客户端 `[(会话 ID, 接上时刻, 最后活动)]` ＋ 报全了没有。
pub type Listed = (Vec<TermRow>, Vec<(String, u64, u64)>, bool);

/// 这台 tmux 的名单（读两次 tmux：窗格 · 客户端）。没装 tmux / 没起 server ⇒ 空名单（不是错）。
pub fn rows_on(on: On<'_>) -> Result<Listed, Said> {
    Ok(rows_found_on(on)?.unwrap_or((vec![], vec![], true)))
}

/// 同 [`rows_on`]，但没装 tmux ⇒ `None`（与「装了、没起 server」分开）。
pub fn rows_found_on(on: On<'_>) -> Result<Option<Listed>, Said> {
    let out = match on.read(&[UTF8_CLIENT_FLAG, "list-panes", "-F", PANES_FMT, "-a"]) {
        Ok(o) => o,
        Err(ChildFail::NotFound(_)) => return Ok(None),
        Err(e) => return Err(tmux_unavailable(e)),
    };
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
        let no_server = NO_SERVER_NEEDLES.iter().any(|(n, _)| said.contains(n));
        if no_server {
            return Ok(Some((vec![], vec![], true)));
        }
        return Err(crate::wire::spec::fail(
            "unobservable",
            crate::common::contract::malformed("tmux list-panes failed"),
        )
        .and_raw(Some(&String::from_utf8_lossy(&out.stderr))));
    }
    let (rows, odd) = parse_rows(&String::from_utf8_lossy(&out.stdout));
    let clients = match on.read(&[UTF8_CLIENT_FLAG, "list-clients", "-F", CLIENTS_FMT]) {
        Ok(o) if o.status.success() => parse_clients(&String::from_utf8_lossy(&o.stdout)),
        _ => vec![],
    };
    Ok(Some((rows, clients, !odd)))
}

/// 这台有没有 tmux（查不动 ⇒ `None`）：「文件与数据」那一份 · 别名页表单 · 起新会话框同一个判法（[`rows_here`]）。
pub fn tmux_here() -> Option<bool> {
    match rows_here() {
        Ok(Some(_)) => Some(true),
        Ok(None) => Some(false),
        Err(_) => None,
    }
}

/// 这台此刻的名单（生产：默认 socket）；没装 tmux ⇒ `None`。批量停 / 起按 sid 认窗格用它。
pub(crate) fn rows_here() -> Result<Option<Vec<TermRow>>, Said> {
    Ok(rows_found_on(On::default())?.map(|(rows, _, _)| rows))
}

/// stderr 里「这台机上一个 tmux server 都没有」的形状。
///
/// `(针, 为什么它算这一档)` —— 加一条要连理由一起加。
pub const NO_SERVER_NEEDLES: &[(&str, &str)] = &[
    (
        "error connecting to",
        "🔴 **真 tmux 打回来的那一句**（沙箱 tmux，显式 `-S <不存在的路径>`）：\
         逐字 `error connecting to /tmp/…/sock (No such file or directory)`。\
         ⚠ 它与下面那条**不是同一句话**：只登记下面那条的话，它落进兜底档 `capture_failed`。\
         「tmux 断开时会说 no server running」是个想当然，实测把它证伪了一半。",
    ),
    (
        "no server running",
        "tmux 在**默认 socket**（生产走的那条）上找不到 server 时的原话\
         （`no server running on /tmp/tmux-<uid>/default`）。\
         ⚠ **这一句没有在真 tmux 上打过**：本模块的判据只造得出显式 `-S` 那条路，\
         而默认 socket 在沙箱里没法造出「无 server」态（那台沙箱自己在用 tmux）。\
         ⇒ 它今天只由 `every_registered_needle_lands_in_its_own_bucket` \
         用合成样本证明「落对了档」，**不是**「见过它」。",
    ),
];

/// 「tmux 这个程序起不来」那一档的唯一造句处。这一档要与「会话不存在」分得开，而沙箱门禁里摘不掉 tmux ⇒
/// 判据只能打这一处：钉的是「这一档存在、且与另外两档不同码不同话」，不是在没装 tmux 的机器上实测过。超时那一档另说（`child_timed_out`）。
pub fn tmux_unavailable(e: ChildFail) -> Said {
    let (code, said) = e.into_cmd_said("no_tmux", |why| {
        copy_text("reason.tmux.startFailed", &[("why", why)])
    });
    crate::wire::spec::fail(code, said)
}

/// 列整台会话的格式串（会话快照的探测器 [`list_sessions`] 与观测那一跳 `tmux ls` 同一份；真 TAB 分列）：
/// name⇥path⇥cmd⇥attached⇥windows⇥@ccm_sid（活动窗格的）⇥session_id⇥各窗格的 @ccm_sid。
///
/// `@ccm_sid` 打在**窗格**上（一个会话里可以跑几个 claude）：末列逐窗口、逐窗格展开（解析见
/// `common::session_snapshot::pane_sids`）；`#{session_id}` 是会话账本认会话的句柄（会话改名它不变）。
pub const TMUX_LS_FMT: &str = "#{session_name}\t#{pane_current_path}\t#{pane_current_command}\t#{?session_attached,1,0}\t#{session_windows}\t#{@ccm_sid}\t#{session_id}\t#{W:#{P:#{@ccm_sid} }}";

/// `TMUX_LS_FMT` 的列数 —— `tab_underflow` 的 N。**改格式串必须同步这个数**。
pub const TMUX_LS_FMT_FIELDS: usize = 8;

/// 一行 `tmux ls` 切成列；段数不等于 `TMUX_LS_FMT_FIELDS`（下溢 = 通道被改写 · 过溢 = 路径里有真 TAB）或名字空 ⇒ 整行丢掉
/// （通道被改写就不当好数据；本函数也被「raw 从别处来」的路径调得到，不许假设上游已经筛过）。
/// 回 `(会话名, 句柄, 这个会话挂着的 sid)`，sid 活动窗格那个在前。
pub fn ls_row(line: &str) -> Option<(&str, &str, Vec<String>)> {
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

/// `tmux ls -F TMUX_LS_FMT` 的原文 ⇒ 那张唯一的会话快照认的行（会话名 ＋ 它某个窗格上的 `@ccm_sid`，挂着几个就几行）。
pub fn session_rows(raw: &str) -> Vec<SessionRow> {
    raw.lines()
        .filter_map(ls_row)
        .flat_map(|(name, _, sids)| crate::common::session_snapshot::rows_of(name, sids))
        .collect()
}

/// 全 crate 唯一一处「一次列全部 tmux 会话」的起进程点：一次调用列全部，不是每个成员探一次。
/// 不看退出码（同 `control/gate.rs::probe_with`）：没有任何会话时 `tmux ls` 是非零 + 空输出，那是「一个都没有」。
/// `-u` 必须排在子命令之前：放到后面是 `rc=1 + unknown flag -u`，会被压成「一个会话都没有」（`session_snapshot_tests` 那条判据钉住次序）。
/// 段数下溢的行出声、整行不当好数据（`INVARIANTS §49`）。
#[expect(
    clippy::disallowed_methods,
    reason = "全 crate 唯一一处一次列全部 tmux 会话（会话快照的探测器）：只读 tmux，不改任何状态"
)]
pub(crate) fn list_sessions() -> Result<Vec<SessionRow>, Said> {
    let out = child(None)
        .args([UTF8_CLIENT_FLAG, "list-sessions", "-F", TMUX_LS_FMT])
        .run(TMUX_WITHIN)
        .map_err(|e| {
            // 不走 [`tmux_unavailable`]：快照的失败只交码与那一句，系统原话记一行日志、不随它走（原话位留空）。
            let (c, s) = e.into_cmd_said("no_tmux", |why| {
                copy_text("reason.tmux.startFailed", &[("why", why)])
            });
            Said::new(c, crate::common::said::IntoNote::into_note(s))
        })?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if !line.trim().is_empty() && tab_underflow(line, TMUX_LS_FMT_FIELDS) {
            tracing::warn!(
                "CCM_TMUX_UNPARSABLE list-sessions 切出 {} 段 < {TMUX_LS_FMT_FIELDS} —— \
                 tmux 打印通道被改写（客户端不是 UTF-8 ⇒ TAB 变 `_`），\
                 整行不当好数据。原样回包：{line:?}",
                line.split('\t').count()
            );
        }
    }
    Ok(session_rows(&text))
}

/// 观测那一跳 `tmux ls` 的 `sh -c` 脚本：`command -v` 门控解析 PATH；**`exec` 让 tmux 的 rc 原样成为 sh 的 rc**。
/// 找不到 tmux ⇒ `exit <no_tmux_rc>`。`fmt` 是观测侧的格式串。
pub fn ls_script(fmt: &str, no_tmux_rc: i32) -> String {
    // `command -v` 分支与 `exec` 的写法住 `platform::shell::posix`（产出逐字节不变）。
    use crate::platform::shell::posix;
    posix::if_command(
        "tmux",
        &posix::exec(&[format!("tmux ls -F '{fmt}' 2>/dev/null")]),
        &format!("exit {no_tmux_rc}"),
    )
}

/// 观测那一跳 `display-message` 的 `sh -c` 脚本：一行两列（server 的 pid · socket 路径）；没有 tmux ⇒ `exit 97`。
pub fn server_script() -> String {
    // 写法住 `platform::shell::posix`（产出逐字节不变）。
    crate::platform::shell::posix::if_command(
        "tmux",
        &crate::platform::shell::posix::exec(&[
            "tmux display-message -p '#{pid}\t#{socket_path}' 2>/dev/null",
        ]),
        "exit 97",
    )
}

/// 经 POSIX shell 跑观测那两条脚本之一（[`ls_script`] · [`server_script`]），带期限、挂 UTF-8 那个 env。
/// 本平台没有 POSIX shell ⇒ `None`（观测侧判成观测无效）。
pub fn via_shell(
    script: &str,
    within: Deadline,
) -> Option<Result<std::process::Output, ChildFail>> {
    let cmd = crate::platform::shell::posix_shell(script)?;
    Some(
        cmd
            // 挂 env 不改脚本串。家在 `common::tmux_utf8`。
            .env(UTF8_CLIENT_ENV.0, UTF8_CLIENT_ENV.1)
            .run(within),
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/host/tmux_within_tests.rs"]
mod within_tests;

#[cfg(test)]
#[path = "../../../tests/backend/host/tmux_tests.rs"]
mod tests;
