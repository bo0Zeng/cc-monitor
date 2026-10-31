//! tmux 那一种宿主：起 `tmux` 进程只在这一份文件里（[`child`]，与观测那两跳 `sh -c`）。
//!
//! 别处要对 tmux 下命令，经 [`child`] 拿一个已指好 server 的命令，argv 由调用方给；
//! 起进程登记表（`readonly_guard::spawn_registry`）把经 [`child`] 起的算进调用方那份文件，写面逐个调用方登记。
//! 名单（`list-panes` · `list-clients` 两发）、一次列全部会话（`list-sessions`，快照的探测器）也住这里。

use super::TermRow;
use crate::common::session_snapshot::SessionRow;
use crate::common::tmux_utf8::{UTF8_CLIENT_ENV, UTF8_CLIENT_FLAG};
use crate::platform::child::{Child, ChildFail, Deadline};
use copy_core::copy_text;

/// 命令级错误：码 ＋ 那一句 ＋ 下层原话。
use crate::common::said::Said;

/// 起 `tmux` 的唯一构造：`socket`：`None` = 让 tmux 按环境解析默认 socket（生产恒 `None`）；
/// `Some(p)` = 显式 `-S <p>`（判据在隔离 socket 上起真 tmux）。
pub(crate) fn child(socket: Option<&str>) -> Child {
    let c = Child::new("tmux");
    match socket {
        Some(s) => c.args(["-S", s]),
        None => c,
    }
}

/// 列名单 · 问尺寸那几发只读 tmux 的期限：tmux 一发 5 s（同 watcher 探测 tmux 的期限）。
const READ_TMUX_WITHIN: Deadline = Deadline::secs(5);

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
pub(crate) struct On<'a> {
    pub(crate) socket: Option<&'a str>,
}

impl On<'_> {
    /// 只读一发（名单 · 问尺寸 · 问版本）。
    pub(crate) fn read(&self, args: &[&str]) -> Result<std::process::Output, ChildFail> {
        child(self.socket).args(args).run(READ_TMUX_WITHIN)
    }
}

/// 终端句柄：tmux 会话 ID（`$N`）＋ 挂着 sid 的窗格（`%M`；没有 ⇒ 只指会话）。名单发的、容器那一格报的都由它算（一个家）。
pub(crate) fn handle_of(session_id: &str, pane: Option<&str>) -> String {
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
pub(crate) fn parse_rows(text: &str) -> (Vec<TermRow>, bool) {
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
        let mut tagged: Vec<&PaneLine> = Vec::new();
        for q in mine.iter().filter(|q| !q.f[7].is_empty()) {
            match tagged.iter().position(|t| t.f[7] == q.f[7]) {
                Some(k) if q.current => tagged[k] = q,
                Some(_) => {}
                None => tagged.push(q),
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

/// `list-clients` 的原文 ⇒ `[(会话 ID, 接上时刻, 最后活动)]`（纯函数；读不懂的行丢掉）。
pub(crate) fn parse_clients(text: &str) -> Vec<(String, u64, u64)> {
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
pub(crate) type Listed = (Vec<TermRow>, Vec<(String, u64, u64)>, bool);

/// 这台 tmux 的名单（读两次 tmux：窗格 · 客户端）。没装 tmux / 没起 server ⇒ 空名单（不是错）。
pub(crate) fn rows_on(on: On<'_>) -> Result<Listed, Said> {
    Ok(rows_found_on(on)?.unwrap_or((vec![], vec![], true)))
}

/// 同 [`rows_on`]，但没装 tmux ⇒ `None`（与「装了、没起 server」分开）。
pub(crate) fn rows_found_on(on: On<'_>) -> Result<Option<Listed>, Said> {
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
pub(crate) fn tmux_here() -> Option<bool> {
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
pub(crate) const NO_SERVER_NEEDLES: &[(&str, &str)] = &[
    (
        "error connecting to",
        "🔴 **真 tmux 打回来的那一句**（沙箱 tmux，显式 `-S <不存在的路径>`）：\
         逐字 `error connecting to /tmp/…/sock (No such file or directory)`。\
         ⚠ 它与下面那条**不是同一句话**，而我先前只登记了下面那条 ⇒ \
         第一趟门禁当场红（落进兜底档 `capture_failed`）。**留着这行来历**：\
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
pub(crate) fn tmux_unavailable(e: ChildFail) -> Said {
    let (code, said) = e.into_cmd_said("no_tmux", |why| {
        copy_text("reason.tmux.startFailed", &[("why", why)])
    });
    crate::wire::spec::fail(code, said)
}

/// 一次列全部会话（快照的探测器）的格式串。**三列**用 TAB 分隔：会话名（tmux 转义成字面 `\t`）· 活动窗格的 `@ccm_sid` · 各窗格的 `@ccm_sid`
/// （见 `common::session_snapshot::pane_sids`，会话 id 的字符集 `[A-Za-z0-9-]`）⇒ 下溢与过溢都不会由合法内容触发，下溢只可能是打印通道被改写。
/// 列数 `LIST_FMT_FIELDS` 住快照那一侧（切行在那里）。
pub(crate) const LIST_FMT: &str = "#{session_name}\t#{@ccm_sid}\t#{W:#{P:#{@ccm_sid} }}";

/// 期限：tmux 一发 5 s（同 watcher 探测 tmux 的期限；界面等它的命令预算都在 10 s 以上）。
const LIST_SESSIONS_WITHIN: Deadline = Deadline::secs(5);

/// 全 crate 唯一一处「一次列全部 tmux 会话」的起进程点：一次调用列全部，不是每个成员探一次。
/// 不看退出码（同 `control/gate.rs::probe`）：没有任何会话时 `tmux ls` 是非零 + 空输出，那是「一个都没有」。
/// `-u` 必须排在子命令之前：放到后面是 `rc=1 + unknown flag -u`，会被压成「一个会话都没有」（`host_tests` 那条判据钉住次序）。
pub(crate) fn list_sessions() -> Result<Vec<SessionRow>, Said> {
    let out = child(None)
        .args([UTF8_CLIENT_FLAG, "list-sessions", "-F", LIST_FMT])
        .run(LIST_SESSIONS_WITHIN)
        .map_err(|e| {
            // 句子只说原因词；系统原话记一行日志（快照的失败只交码与那一句，原话不随它走）。
            let (c, s) = e.into_cmd_said("no_tmux", |why| {
                copy_text("reason.tmux.startFailed", &[("why", why)])
            });
            Said::new(c, crate::common::said::IntoNote::into_note(s))
        })?;
    Ok(crate::common::session_snapshot::parse_rows(
        &String::from_utf8_lossy(&out.stdout),
    ))
}

/// 观测那一跳 `tmux ls` 的 `sh -c` 脚本：`command -v` 门控解析 PATH；**`exec` 让 tmux 的 rc 原样成为 sh 的 rc**。
/// 找不到 tmux ⇒ `exit <no_tmux_rc>`。`fmt` 是观测侧的格式串。
pub(crate) fn ls_script(fmt: &str, no_tmux_rc: i32) -> String {
    // `command -v` 分支与 `exec` 的写法住 `platform::shell::posix`（产出逐字节不变）。
    use crate::platform::shell::posix;
    posix::if_command(
        "tmux",
        &posix::exec(&[format!("tmux ls -F '{fmt}' 2>/dev/null")]),
        &format!("exit {no_tmux_rc}"),
    )
}

/// 观测那一跳 `display-message` 的 `sh -c` 脚本：一行两列（server 的 pid · socket 路径）；没有 tmux ⇒ `exit 97`。
pub(crate) fn server_script() -> String {
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
pub(crate) fn via_shell(
    script: &str,
    within: Deadline,
) -> Option<Result<std::process::Output, ChildFail>> {
    let cmd = crate::platform::shell::posix_shell(script)?;
    Some(
        cmd
            // K-R12：挂 env 不改脚本串。家在 `common::tmux_utf8`。
            .env(UTF8_CLIENT_ENV.0, UTF8_CLIENT_ENV.1)
            .run(within),
    )
}
