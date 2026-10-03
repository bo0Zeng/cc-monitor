//! 终端管理 L1 的三条命令：`terminals-list`（名单）· `terminal-preview`（抓一屏）· `terminal-input`（送字 / 送键）。
//!
//! 两个前端共用，**形状与宿主无关**：目标用名单里的不透明句柄（`terminal`）或会话 ID（`sid`）指，
//! 回话不带任何 tmux 目标串。这一版宿主只有 tmux：一个 tmux 会话 = 一个终端（看它当前窗口的当前窗格）；
//! 托管终端做出来之后换实现、不换形状。
//!
//! # 纪律
//!
//! - **只认名单里的终端**：句柄 / sid 先在这一刻的名单里对上，对上了才对那个会话的 `#{session_id}` 下手；
//!   不收任意 tmux 目标串。
//! - **送字 / 送键过身份门**（与 `launch` 的 `send-into` 同一道：`gate::identity`，含「哪个前端的会话」那一维）；
//!   抓屏只读、不过门（同 `capture-pane`）。
//! - **先看见再送**：送的时候带上看到的那一屏的指纹（`seen_screen`），画面变了就不送、把新指纹交回去。
//! - 只抓一次、只送一次：轮询归调用方（零定时器）。

use crate::common::child_env::WithoutOwnEnv;
use crate::common::tmux_utf8::UTF8_CLIENT_FLAG;
use crate::control::gate::{self, Who};
use serde_json::{json, Map, Value};
use std::process::{Command, Stdio};

/// 命令级错误：`(code, message)`。与 [`super::launch`] / [`super::gate`] 同型。
pub(crate) type CmdErr = (&'static str, String);

/// 句柄前缀（不透明：前端不拼、不解析；后端只按名单对）。
const HANDLE_PREFIX: &str = "tmux-";

/// `scrollback` 的上限（行）。超了截到这里，回话里 `capped: true`。
pub(crate) const MAX_SCROLLBACK: u32 = 2000;

/// 目标那一格的长度上限（字节）：名单里的句柄 / sid 都远小于它，只防一个巨串。
const MAX_TARGET_BYTES: usize = 256;

/// 名单那一趟问 tmux 的格式：会话 ID · 名 · 窗口数 · 最后活动 · `@ccm_sid` · `@ccm_agent` · `@ccm_client`
/// · 当前窗格前台程序 · 工作目录 · 窗格标题（后两格是自由文本，排在最后）。
const SESSIONS_FMT: &str = "#{session_id}\t#{session_name}\t#{session_windows}\t#{session_activity}\t#{@ccm_sid}\t#{@ccm_agent}\t#{@ccm_client}\t#{pane_current_command}\t#{pane_current_path}\t#{pane_title}";
const SESSIONS_FIELDS: usize = 10;

/// 连着各会话的 tmux 客户端：会话 ID · 接上的时刻 · 最后活动。
const CLIENTS_FMT: &str = "#{session_id}\t#{client_created}\t#{client_activity}";

/// 终端前台若是这些 shell ⇒ 程序已退出、只剩 shell。
const SHELLS: &[&str] = &[
    "bash", "zsh", "fish", "sh", "dash", "ksh", "tcsh", "csh", "nu", "pwsh",
];

/// 送键那张有限键表：线上的名字 → tmux 的键名。不收任意转义串。
pub(crate) const KEYS: &[(&str, &str)] = &[
    ("esc", "Escape"),
    ("ctrl-c", "C-c"),
    ("ctrl-d", "C-d"),
    ("up", "Up"),
    ("down", "Down"),
    ("left", "Left"),
    ("right", "Right"),
    ("tab", "Tab"),
    ("shift-tab", "BTab"),
    ("enter", "Enter"),
    ("backspace", "BSpace"),
    ("page-up", "PPage"),
    ("page-down", "NPage"),
];

/// 问哪台 tmux：生产恒是默认 socket（`socket: None`）；判据在隔离 socket 上起真 tmux。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct On<'a> {
    pub(crate) socket: Option<&'a str>,
}

impl On<'_> {
    /// 本模块唯一起进程的那一处（名单 · 身份门的探测 · 送字送键都经它造的命令）。
    fn cmd(&self) -> Command {
        let mut c = Command::new("tmux").without_own_env();
        if let Some(s) = self.socket {
            c.args(["-S", s]);
        }
        c
    }

    fn read(&self, args: &[&str]) -> std::io::Result<std::process::Output> {
        self.cmd()
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    }
}

/// 名单里的一行（一个 tmux 会话）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TermRow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) windows: u32,
    pub(crate) activity: u64,
    pub(crate) sid: String,
    pub(crate) agent: String,
    pub(crate) client: String,
    pub(crate) program: String,
    pub(crate) cwd: String,
    pub(crate) title: String,
}

impl TermRow {
    fn handle(&self) -> String {
        format!("{HANDLE_PREFIX}{}", self.id.trim_start_matches('$'))
    }

    /// 喂身份门的那四格（与 `gate::probe` 探到的同形）。
    fn probed(&self) -> gate::Probed {
        gate::Probed {
            session_id: self.id.clone(),
            ccm_sid: self.sid.clone(),
            windows: self.windows,
            client: self.client.clone(),
        }
    }
}

/// `list-sessions` 的原文 ⇒ 行（纯函数）。回 `(行, 有没有读不懂的行)`。
pub(crate) fn parse_rows(text: &str) -> (Vec<TermRow>, bool) {
    let mut rows = Vec::new();
    let mut odd = false;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.splitn(SESSIONS_FIELDS, '\t').collect();
        let windows = f.get(2).and_then(|w| w.parse().ok());
        let activity = f.get(3).and_then(|a| a.parse().ok());
        match (
            f.len() == SESSIONS_FIELDS && f[0].starts_with('$'),
            windows,
            activity,
        ) {
            (true, Some(windows), Some(activity)) => rows.push(TermRow {
                id: f[0].into(),
                name: f[1].into(),
                windows,
                activity,
                sid: f[4].into(),
                agent: f[5].into(),
                client: f[6].into(),
                program: f[7].into(),
                cwd: f[8].into(),
                title: f[9].into(),
            }),
            _ => odd = true,
        }
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    (rows, odd)
}

/// 这台的名单（读两次 tmux：会话 · 客户端）。没装 tmux / 没起 server ⇒ 空名单（不是错）。
/// 回 `(行, 各会话的客户端 [(会话 ID, 接上时刻, 最后活动)], 报全了没有)`。
fn rows_on(on: On<'_>) -> Result<(Vec<TermRow>, Vec<(String, u64, u64)>, bool), CmdErr> {
    let out = match on.read(&[UTF8_CLIENT_FLAG, "list-sessions", "-F", SESSIONS_FMT]) {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((vec![], vec![], true)),
        Err(e) => return Err(super::capture_pane::tmux_unavailable(&e)),
    };
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
        let no_server = super::capture_pane::NO_SERVER_NEEDLES
            .iter()
            .any(|(n, _)| said.contains(n));
        if no_server {
            return Ok((vec![], vec![], true));
        }
        return Err((
            "unobservable",
            crate::common::contract::malformed(&format!(
                "tmux list-sessions failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )),
        ));
    }
    let (rows, odd) = parse_rows(&String::from_utf8_lossy(&out.stdout));
    let clients = match on.read(&[UTF8_CLIENT_FLAG, "list-clients", "-F", CLIENTS_FMT]) {
        Ok(o) if o.status.success() => parse_clients(&String::from_utf8_lossy(&o.stdout)),
        _ => vec![],
    };
    Ok((rows, clients, !odd))
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

/// 一个终端在名单里的样子（成品；句子由前端按自己的文案说）。
pub(crate) fn terminal_json(
    row: &TermRow,
    clients: &[(String, u64, u64)],
    requester: Option<&str>,
) -> Value {
    let who = gate::identity(&row.name, &row.probed(), requester);
    let not = |why: &str| json!({ "no": why });
    let input = match who {
        Who::Pass => json!(true),
        Who::OtherClient => not("not-yours"),
        Who::NotOurs => not("not-managed"),
    };
    let end = match who {
        Who::Pass if row.windows == 1 => json!(true),
        Who::Pass => not("other-windows"),
        Who::OtherClient => not("not-yours"),
        Who::NotOurs => not("not-managed"),
    };
    let shell = SHELLS.contains(&row.program.as_str());
    let state = match (row.sid.is_empty(), shell) {
        (false, true) => "program-exited",
        (true, true) => "idle",
        _ => "running",
    };
    let mut t = Map::new();
    t.insert("terminal".into(), row.handle().into());
    t.insert("host".into(), "tmux".into());
    t.insert("tmux_name".into(), row.name.clone().into());
    let title = if !row.sid.is_empty() && !shell && !row.title.is_empty() {
        row.title.clone()
    } else {
        row.program.clone()
    };
    t.insert("title".into(), title.into());
    t.insert("program".into(), row.program.clone().into());
    t.insert("cwd".into(), row.cwd.clone().into());
    if !row.sid.is_empty() {
        let mut s = Map::new();
        s.insert("sid".into(), row.sid.clone().into());
        if !row.agent.is_empty() {
            s.insert("agent".into(), row.agent.clone().into());
        }
        t.insert("session".into(), Value::Object(s));
    }
    t.insert("purpose".into(), "normal".into());
    t.insert(
        "started_by".into(),
        json!({
            "client": (!row.client.is_empty()).then(|| row.client.clone()),
            "mine": who == Who::Pass,
        }),
    );
    let attached: Vec<Value> = clients
        .iter()
        .filter(|(id, _, _)| *id == row.id)
        .map(|(_, since, last)| {
            json!({ "kind": "terminal-window", "since": since, "last_activity": last })
        })
        .collect();
    t.insert("clients".into(), Value::Array(attached));
    t.insert("input".into(), "shared".into());
    t.insert("state".into(), state.into());
    t.insert("last_activity".into(), row.activity.into());
    t.insert(
        "can".into(),
        json!({ "preview": true, "input": input, "end": end }),
    );
    Value::Object(t)
}

/// `terminals-list` 的成品构造器（纯）。
pub(crate) fn list_reply(
    rows: &[TermRow],
    clients: &[(String, u64, u64)],
    complete: bool,
    requester: Option<&str>,
) -> Value {
    let terminals: Vec<Value> = rows
        .iter()
        .map(|r| terminal_json(r, clients, requester))
        .collect();
    json!({ "terminals": terminals, "complete": complete })
}

/// 帧面 / CLI 面入口：`terminals-list`。入 `{client?}`（自报的前端，决定每一行的 `mine` 与 `can`）。
pub(crate) fn list_on(on: On<'_>, args: &Value) -> Result<Value, CmdErr> {
    let requester = gate::requester_of(args)?;
    let (rows, clients, complete) = rows_on(on)?;
    Ok(list_reply(&rows, &clients, complete, requester.as_deref()))
}

/// 目标怎么指：恰好给一个（`terminal` 句柄 · `sid` 会话 ID）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Handle(String),
    Sid(String),
}

pub(crate) fn target_of(args: &Value) -> Result<Target, CmdErr> {
    let text = |k: &str| match args.get(k) {
        None => Ok(None),
        Some(Value::String(s)) if !s.is_empty() && s.len() <= MAX_TARGET_BYTES => {
            Ok(Some(s.clone()))
        }
        Some(_) => Err(bad_target()),
    };
    match (text("terminal")?, text("sid")?) {
        (Some(h), None) => Ok(Target::Handle(h)),
        (None, Some(s)) => Ok(Target::Sid(s)),
        _ => Err(bad_target()),
    }
}

fn bad_target() -> CmdErr {
    (
        "bad_target",
        crate::common::contract::malformed(
            "give exactly one of `terminal` (a handle from terminals-list) / `sid` (a non-empty string)",
        ),
    )
}

/// 名单里对上那一个：句柄恰好一个；sid ＝ 带着它（`@ccm_sid`）的那个会话，多个 ⇒ 说不准。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Found<'a> {
    One(&'a TermRow),
    NotKnown,
    Ambiguous,
}

pub(crate) fn find<'a>(rows: &'a [TermRow], t: &Target) -> Found<'a> {
    let hits: Vec<&TermRow> = rows
        .iter()
        .filter(|r| match t {
            Target::Handle(h) => r.handle() == *h,
            Target::Sid(s) => r.sid == *s,
        })
        .collect();
    match hits.as_slice() {
        [one] => Found::One(one),
        [] => Found::NotKnown,
        _ => Found::Ambiguous,
    }
}

/// 那个窗格此刻的宽 · 高 · 光标列 · 光标行 · 光标可见（`display-message -p`，只读）。
const GEOMETRY_FMT: &str =
    "#{pane_width}\t#{pane_height}\t#{cursor_x}\t#{cursor_y}\t#{cursor_flag}";

/// 一次预览：那一屏的原文（可能带 SGR）＋ 尺寸与光标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct View {
    pub(crate) text: String,
    pub(crate) cols: u32,
    pub(crate) rows: u32,
    pub(crate) cursor_x: u32,
    pub(crate) cursor_y: u32,
    pub(crate) cursor_visible: bool,
}

/// 抓一次预览（`id` 是名单里那个会话的 `#{session_id}`）：先抓屏（`capture_pane` 那一处）、再问尺寸与光标；
/// 两下都只读。问尺寸那一下回空 ⇒ 会话在两下之间没了 ⇒ `no_such_session`。
fn view_on(on: On<'_>, id: &str, color: bool, back: u32) -> Result<View, CmdErr> {
    let text = super::capture_pane::capture_with_on(on.socket, id, color, back)?;
    let gone = || {
        (
            "no_such_session",
            crate::common::contract::malformed("the terminal went away while it was being read"),
        )
    };
    let out = on
        .read(&[
            UTF8_CLIENT_FLAG,
            "display-message",
            "-p",
            "-t",
            id,
            GEOMETRY_FMT,
        ])
        .map_err(|e| super::capture_pane::tmux_unavailable(&e))?;
    let geo = String::from_utf8_lossy(&out.stdout);
    let n: Vec<u32> = geo
        .trim_end_matches(['\n', '\r'])
        .split('\t')
        .map(|x| x.trim().parse().unwrap_or(u32::MAX))
        .collect();
    match n.as_slice() {
        [cols, rows, x, y, flag] if out.status.success() && !n.contains(&u32::MAX) => Ok(View {
            text,
            cols: *cols,
            rows: *rows,
            cursor_x: *x,
            cursor_y: *y,
            cursor_visible: *flag == 1,
        }),
        _ => Err(gone()),
    }
}

/// 一屏的指纹：去掉颜色、每行去掉行尾空白、只看可见那几行（最后 `rows` 行），FNV-1a 64 位。
/// 预览给出它；送字送键时带回来比，画面变了就不送。
pub(crate) fn fingerprint(plain_lines: &[String], rows: u32) -> String {
    let skip = plain_lines.len().saturating_sub(rows as usize);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, l) in plain_lines[skip..].iter().enumerate() {
        if i > 0 {
            h = (h ^ u64::from(b'\n')).wrapping_mul(0x0100_0000_01b3);
        }
        for b in l.trim_end().bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

/// 抓回来的原文按行切（去掉最后那个换行带出来的空行）。
fn split_lines(text: &str) -> Vec<&str> {
    let mut v: Vec<&str> = text.split('\n').collect();
    if v.last() == Some(&"") {
        v.pop();
    }
    v
}

/// 一段 SGR 属性。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Style {
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

const NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// 256 色里的一格 ⇒ 16 色名或 `#rrggbb`。
fn color_256(n: u32) -> String {
    match n {
        0..=7 => NAMES[n as usize].into(),
        8..=15 => format!("bright-{}", NAMES[(n - 8) as usize]),
        16..=231 => {
            let lv = |x: u32| if x == 0 { 0 } else { 55 + 40 * x };
            let i = n - 16;
            format!("#{:02x}{:02x}{:02x}", lv(i / 36), lv(i / 6 % 6), lv(i % 6))
        }
        _ => {
            let g = 8 + 10 * (n.min(255) - 232);
            format!("#{g:02x}{g:02x}{g:02x}")
        }
    }
}

impl Style {
    fn is_plain(&self) -> bool {
        *self == Style::default()
    }

    /// 吃一串 SGR 参数（`ESC [ … m` 里那一段）。
    fn apply(&mut self, params: &str) {
        let p: Vec<u32> = params
            .split([';', ':'])
            .map(|x| x.parse().unwrap_or(0))
            .collect();
        let mut i = 0;
        while i < p.len() {
            let ext = |at: usize| -> (Option<String>, usize) {
                match p.get(at + 1) {
                    Some(5) => (p.get(at + 2).map(|n| color_256(*n)), 3),
                    Some(2) => match (p.get(at + 2), p.get(at + 3), p.get(at + 4)) {
                        (Some(r), Some(g), Some(b)) => (
                            Some(format!(
                                "#{:02x}{:02x}{:02x}",
                                r.min(&255),
                                g.min(&255),
                                b.min(&255)
                            )),
                            5,
                        ),
                        _ => (None, p.len()),
                    },
                    _ => (None, p.len()),
                }
            };
            let mut step = 1;
            match p[i] {
                0 => *self = Style::default(),
                1 => self.bold = true,
                2 => self.dim = true,
                3 => self.italic = true,
                4 => self.underline = true,
                7 => self.inverse = true,
                22 => (self.bold, self.dim) = (false, false),
                23 => self.italic = false,
                24 => self.underline = false,
                27 => self.inverse = false,
                n @ 30..=37 => self.fg = Some(NAMES[(n - 30) as usize].into()),
                n @ 90..=97 => self.fg = Some(format!("bright-{}", NAMES[(n - 90) as usize])),
                39 => self.fg = None,
                n @ 40..=47 => self.bg = Some(NAMES[(n - 40) as usize].into()),
                n @ 100..=107 => self.bg = Some(format!("bright-{}", NAMES[(n - 100) as usize])),
                49 => self.bg = None,
                38 => (self.fg, step) = ext(i),
                48 => (self.bg, step) = ext(i),
                _ => {}
            }
            i += step;
        }
    }

    fn span(&self, from: usize, to: usize) -> Value {
        let mut m = Map::new();
        m.insert("from".into(), from.into());
        m.insert("to".into(), to.into());
        if let Some(c) = &self.fg {
            m.insert("fg".into(), c.clone().into());
        }
        if let Some(c) = &self.bg {
            m.insert("bg".into(), c.clone().into());
        }
        for (k, on) in [
            ("bold", self.bold),
            ("dim", self.dim),
            ("italic", self.italic),
            ("underline", self.underline),
            ("inverse", self.inverse),
        ] {
            if on {
                m.insert(k.into(), true.into());
            }
        }
        Value::Object(m)
    }
}

/// 带 SGR 的一行 ⇒ `(纯文字, 属性段)`。属性段按字符列计（`from` 含、`to` 不含），默认样子不出段；
/// 别的转义（OSC 等）丢掉。行与行之间属性接着传（`style` 由调用方带着走）。
fn parse_line(raw: &str, style: &mut Style) -> (String, Vec<Value>) {
    let mut text = String::new();
    let mut spans = Vec::new();
    let mut col = 0usize;
    let mut start = 0usize;
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            text.push(c);
            col += 1;
            continue;
        }
        match chars.next() {
            Some('[') => {
                let mut params = String::new();
                let mut fin = None;
                for d in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&d) {
                        fin = Some(d);
                        break;
                    }
                    params.push(d);
                }
                if fin == Some('m') {
                    let before = style.clone();
                    style.apply(&params);
                    if *style != before {
                        if !before.is_plain() && col > start {
                            spans.push(before.span(start, col));
                        }
                        start = col;
                    }
                }
            }
            Some(']') => {
                while let Some(d) = chars.next() {
                    if d == '\u{7}' || (d == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    if !style.is_plain() && col > start {
        spans.push(style.span(start, col));
    }
    (text, spans)
}

/// `terminal-preview` 的成品构造器（纯）：抓回来的原文 ＋ 尺寸光标 ⇒ 回话。
pub(crate) fn preview_reply(view: &View, color: bool, capped: bool, captured_at: u64) -> Value {
    let mut style = Style::default();
    let mut plain = Vec::new();
    let lines: Vec<Value> = split_lines(&view.text)
        .into_iter()
        .map(|raw| {
            let (text, spans) = parse_line(raw, &mut style);
            plain.push(text.clone());
            if color {
                json!({ "text": text, "spans": spans })
            } else {
                json!({ "text": text })
            }
        })
        .collect();
    let back = plain.len().saturating_sub(view.rows as usize);
    json!({
        "screen": fingerprint(&plain, view.rows),
        "cols": view.cols,
        "rows": view.rows,
        "cursor": { "x": view.cursor_x, "y": view.cursor_y, "visible": view.cursor_visible },
        "lines": lines,
        "scrollback_lines": back,
        "capped": capped,
        "captured_at": captured_at,
    })
}

fn bool_arg(args: &Value, k: &str, default: bool) -> Result<bool, CmdErr> {
    match args.get(k) {
        None => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err((
            "invalid_args",
            crate::common::contract::malformed(&format!("`{k}` must be a boolean")),
        )),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn not_known(why: &str) -> CmdErr {
    (
        "not_known",
        crate::common::contract::malformed(&format!(
            "no such terminal in the list right now ({why})"
        )),
    )
}

/// 帧面 / CLI 面入口：`terminal-preview`。入 `{terminal | sid, color?: true, scrollback?: 0}`。只读、不过身份门。
pub(crate) fn preview_on(on: On<'_>, args: &Value) -> Result<Value, CmdErr> {
    let target = target_of(args)?;
    let color = bool_arg(args, "color", true)?;
    let asked = match args.get("scrollback") {
        None => 0,
        Some(v) => v.as_u64().ok_or((
            "invalid_args",
            crate::common::contract::malformed("`scrollback` must be a non-negative integer"),
        ))?,
    };
    let back = asked.min(u64::from(MAX_SCROLLBACK)) as u32;
    let (rows, _, _) = rows_on(on)?;
    let row = match find(&rows, &target) {
        Found::One(r) => r,
        Found::NotKnown => return Err(not_known("not-known")),
        Found::Ambiguous => {
            return Err((
                "ambiguous",
                crate::common::contract::malformed(
                    "more than one terminal carries this sid; point at one by `terminal`",
                ),
            ))
        }
    };
    let view = view_on(on, &row.id, color, back)?;
    Ok(preview_reply(
        &view,
        color,
        asked > u64::from(MAX_SCROLLBACK),
        now_secs(),
    ))
}

/// 这一刻那一屏的指纹（送之前比 `seen_screen` 用；同 [`preview_reply`] 的算法）。
fn screen_now(on: On<'_>, id: &str) -> Result<String, CmdErr> {
    let v = view_on(on, id, false, 0)?;
    let mut style = Style::default();
    let plain: Vec<String> = split_lines(&v.text)
        .into_iter()
        .map(|l| parse_line(l, &mut style).0)
        .collect();
    Ok(fingerprint(&plain, v.rows))
}

/// 送什么：一段字面字（＋ 要不要补回车）或一个键（tmux 键名）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Input {
    Text { text: String, enter: bool },
    Key(&'static str),
}

pub(crate) fn input_of(args: &Value) -> Result<Input, CmdErr> {
    let bad = |m: &str| ("invalid_args", crate::common::contract::malformed(m));
    match (args.get("text"), args.get("key")) {
        (Some(Value::String(t)), None) => {
            super::launch::check_input_text(t)?;
            Ok(Input::Text {
                text: t.clone(),
                enter: bool_arg(args, "enter", true)?,
            })
        }
        (None, Some(Value::String(k))) => {
            if args.get("enter").is_some() {
                return Err(bad("`enter` only goes with `text`"));
            }
            KEYS.iter()
                .find(|(wire, _)| wire == k)
                .map(|(_, tmux)| Input::Key(tmux))
                .ok_or_else(|| bad(&format!("unknown key {k:?}")))
        }
        _ => Err(bad(
            "give exactly one of `text` (a string) / `key` (a name from the key table)",
        )),
    }
}

/// `terminal-input` 的回话构造器（纯）。
pub(crate) fn input_reply(result: &str, why: Option<&str>, screen: Option<&str>) -> Value {
    let mut m = Map::new();
    m.insert("result".into(), result.into());
    if let Some(w) = why {
        m.insert("why".into(), w.into());
    }
    if let Some(s) = screen {
        m.insert("screen".into(), s.into());
    }
    Value::Object(m)
}

/// 帧面 / CLI 面入口：`terminal-input`。入 `{terminal | sid, text, enter?: true | key, seen_screen?, take?, client?}`。
/// 送不了的（不在名单 · 别的前端的 · 不归我们管 · 画面变了 · 已经没了）回 `result: "refused"` ＋ `why`，不是错；
/// 形状不对才是错（`bad_target` · `invalid_args`）。`take` 在 tmux 上无所谓（各端都能打字）。
pub(crate) fn input_on(on: On<'_>, args: &Value) -> Result<Value, CmdErr> {
    let target = target_of(args)?;
    let input = input_of(args)?;
    bool_arg(args, "take", false)?;
    let requester = gate::requester_of(args)?;
    let seen = match args.get("seen_screen") {
        None => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => {
            return Err((
                "invalid_args",
                crate::common::contract::malformed("`seen_screen` must be a string"),
            ))
        }
    };
    let (rows, _, _) = rows_on(on)?;
    let row = match find(&rows, &target) {
        Found::One(r) => r,
        Found::NotKnown => return Ok(input_reply("refused", Some("not-known"), None)),
        Found::Ambiguous => return Ok(input_reply("refused", Some("ambiguous"), None)),
    };
    // 身份门照 `send-into` 那一道：此刻再探一次（名单那一刻之后可能换了人）。
    let Some(p) = gate::probe_with(on.cmd(), &row.id)? else {
        return Ok(input_reply("refused", Some("ended"), None));
    };
    match gate::identity(&row.name, &p, requester.as_deref()) {
        Who::Pass => {}
        Who::OtherClient => return Ok(input_reply("refused", Some("not-yours"), None)),
        Who::NotOurs => return Ok(input_reply("refused", Some("not-managed"), None)),
    }
    if let Some(seen) = seen {
        let now = screen_now(on, &p.session_id)?;
        if now != seen {
            return Ok(input_reply("refused", Some("screen-changed"), Some(&now)));
        }
    }
    let tmux = |a: &[&str]| super::launch::ran(on.cmd(), a);
    let sent = match &input {
        Input::Text { text, enter } => {
            super::launch::type_literal(&p.session_id, text, *enter, &tmux)?
        }
        Input::Key(k) => super::launch::press_key(&p.session_id, k, &tmux)?,
    };
    if sent.ok {
        return Ok(input_reply("delivered", None, None));
    }
    // tmux 回了非零：会话还在 ⇒ 不知道送没送到（不重发）；不在了 ⇒ 已经没了。
    match gate::probe_with(on.cmd(), &p.session_id)? {
        Some(_) => Ok(input_reply("unsure", None, None)),
        None => Ok(input_reply("refused", Some("ended"), None)),
    }
}

/// 帧面入口（生产：默认 socket）。
pub(crate) fn list_for_inbound(args: &Value) -> Result<Value, (String, String)> {
    list_on(On::default(), args).map_err(|(c, m)| (c.to_string(), m))
}

pub(crate) fn preview_for_inbound(args: &Value) -> Result<Value, (String, String)> {
    preview_on(On::default(), args).map_err(|(c, m)| (c.to_string(), m))
}

pub(crate) fn input_for_inbound(args: &Value) -> Result<Value, (String, String)> {
    input_on(On::default(), args).map_err(|(c, m)| (c.to_string(), m))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/terminals_tests.rs"]
mod tests;
