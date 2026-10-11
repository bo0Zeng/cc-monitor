//! **一轮的摘要**（过程行与轮次刻度读它）：从你的一句话到下一句话算一轮；
//! 每轮出边界 · 工具调用数 · 思考段数 · 失败数 · 起止时刻 · 露着的是哪几条 · 过程段 · 你那句的第一行 · 回复头三行。
//!
//! 与大纲同一个来源：「你说的一句」就是 [`super::user_inputs::user_input_of`] 认的那一条（口径不另写）；子运行的记录不算进主线的轮。
//! 界面只按它排版（折哪几条、每段过程行写什么、刻度悬停写什么），不从流上自己攒、不自己划段。
//!
//! **划段**（[`Open::close`]，只住这里）：一轮里带正文的 assistant 记录（中间说的话与结论都算）和停下这一轮的那一条
//! （中断标记 / 报错卡）露着；被它们隔开的、连续的其余记录（工具调用与结果 · 思考 · 派 agent 与交回 · 后台通知 · 重试 · 来话 · 压缩）
//! 各成一段过程，一段一行。「工具 · 正文 · 工具」是两段，正文在两段之间露着。
//!
//! 按段取：`from` 是某一轮开头那一行的字节位置（`at`）或 0；还在跑的最后一轮下次从它的 `at` 再取一次，整轮重算（后端零状态）。
//! `from` 之前的那半轮不出（它的开头不在这一段里）。

use super::facts_query::{what_of, Needs, NeedsKind, PendingCall};
use super::user_inputs::user_input_given;
use copy_core::copy_text;
use serde_json::Value;

/// 你那句留多少字。
pub(crate) const SAID_MAX: usize = 50;
/// 回复头留多少字、几行。
pub(crate) const REPLY_MAX: usize = 120;
pub(crate) const REPLY_LINES: usize = 3;

/// 一轮。键名全称。
#[derive(Debug, Default, Clone, serde::Serialize, PartialEq, Eq)]
pub(crate) struct TurnRow {
    /// 这一轮开头那一行（你说的那句）的字节位置 ⇒ 下次从这里接着取；这一轮的范围到下一轮的 `at`（最后一轮到文件尾）。
    pub(crate) at: u64,
    /// 你那句的 uuid（跳过去用）。
    pub(crate) uuid: String,
    /// 起：你那句的时刻；止：这一轮最后一条主线记录的时刻（还没有回应 ⇒ 同起）。
    pub(crate) start: String,
    pub(crate) end: String,
    /// 起止各自在看的那一台钟上的钟面 `HH:MM`（答 `history-turns` 那一下按请求的时区写，[`TurnRow::stamp`]；解不出 ⇒ 空串）。界面照抄、不换算。
    #[serde(rename = "startText")]
    pub(crate) start_text: String,
    #[serde(rename = "endText")]
    pub(crate) end_text: String,
    /// 你那句的第一行（≤ [`SAID_MAX`] 字）。
    pub(crate) said: String,
    /// 整轮合计：工具调用几次（派 agent 不算）· 思考几段 · 派出几个 agent · 后台任务通知几条（交回过的子 agent 的收场通知不算）·
    /// API 重试几次 · 另一会话 / 子 agent 来话几段（交回不算）。
    pub(crate) tools: u32,
    pub(crate) thinking: u32,
    pub(crate) agents: u32,
    pub(crate) background: u32,
    pub(crate) retries: u32,
    pub(crate) peers: u32,
    /// 失败合计：工具（含派 agent）结果标了出错、且不是人拒的 ＋ 后台任务 / 子 agent 收场说失败。
    pub(crate) fails: u32,
    /// 露着的那几条（uuid，文件序）：带正文的 assistant 记录（中间说的话与结论）＋ 停下这一轮的那一条中断标记 / 报错卡。
    pub(crate) shown: Vec<String>,
    /// 回复头三行（≤ [`REPLY_MAX`] 字）：最后一次工具调用之后的结论正文的开头；没有结论 ⇒ 空串。
    pub(crate) reply: String,
    /// 这一轮收尾了：后面又有你的一句，或 Claude 说完了（`end_turn`），或你中断了它。
    pub(crate) done: bool,
    /// 此刻：没在跑（收尾了 / 没有活进程）· 在跑 · 在等你（[`dress_live`] 按这台的会话事实现判）。
    pub(crate) phase: Phase,
    /// 过程段（文件序）：一段一行，界面把那一行插在 `after` 那张卡后面、把这一段的卡收进去。空 ⇒ 这一轮什么都不折。
    pub(crate) folds: Vec<Fold>,
    /// 这一轮里落成了人话的排队插话（排队那一条的记录 id）：下一句你说的原文与它一字不差 ⇒ 那句话已经作为你的一句进了记录，
    /// 界面不再单独出排队那一条（同一句不上屏两次）；还在排队 / 下一句不是它 ⇒ 不在这里（照常露着）。
    pub(crate) landed: Vec<String>,
    /// 最后一段过程是这一轮的尾巴（后面没有露着的了）：还在跑时「现在：…」「等你批准：…」写在它上面。
    #[serde(skip)]
    pub(crate) tail_fold: bool,
    /// 这一轮里还没有结果的工具调用（文件序）：拼「现在：…」「等你批准：…」用，不出帧。
    #[serde(skip)]
    pub(crate) pending: Vec<PendingCall>,
}

/// 一段过程（一行）。
#[derive(Debug, Default, Clone, serde::Serialize, PartialEq, Eq)]
pub(crate) struct Fold {
    /// 这一段跟在哪一条后面：这一轮你那句，或露着的某一条（uuid）。一段只有一个 `after`，界面按它认段。
    pub(crate) after: String,
    /// 那一行的字（后端写好，界面按语气排版、段间加分隔）。
    pub(crate) parts: Vec<Part>,
    /// 那一行右端那一截。
    pub(crate) span: Span,
    /// 此刻：只有还没收尾的那一轮的尾巴那一段会是在跑 / 在等你（[`dress_live`]）。
    pub(crate) phase: Phase,
    /// 这一段第一条 / 最后一条记录的时刻（右端那一截按它写，不出帧）。
    #[serde(skip)]
    start: String,
    #[serde(skip)]
    end: String,
    /// `start` 的钟面（[`TurnRow::stamp`] 写；还在跑时「HH:MM 起」用）。
    #[serde(skip)]
    start_text: String,
}

/// 一轮 / 一段此刻的样子。
#[derive(Debug, Default, Clone, Copy, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Phase {
    #[default]
    Idle,
    Running,
    Awaiting,
}

/// 过程行的一段字。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub(crate) struct Part {
    pub(crate) text: String,
    pub(crate) tone: Tone,
}

/// 一段字的语气（闭集住 `common/cells.rs`，格目录按这个类型认「语气」格）。
pub(crate) use crate::common::cells::Tone;

/// 右端那一截：字里若有 `{dur}`，界面填 `to − from`（`to` 缺 ⇒ 现在 − `from`，毫秒）的用时；别的照抄。
#[derive(Debug, Default, Clone, serde::Serialize, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) text: String,
    pub(crate) from: Option<i64>,
    pub(crate) to: Option<i64>,
}

/// 用时那一格的占位（界面填）。
const DUR_SLOT: &str = "{dur}";

/// 一条主线记录在这一轮里记下的（收尾时按它划段、数合计）。
#[derive(Default)]
struct Item {
    uuid: Option<String>,
    ts: String,
    /// 露着（带正文 · 停下这一轮的那一条）。
    shown: bool,
    tools: u32,
    thinking: u32,
    agents: u32,
    retries: u32,
    peers: u32,
    fails: u32,
    compacts: u32,
    /// 收场通知（task-id · 失败了没有），收尾时按 `agent_runs` 分后台任务与子 agent。
    notices: Vec<(Option<String>, bool)>,
}

/// 一段过程攒到的（各样计数）。
#[derive(Default)]
struct Tally {
    tools: u32,
    thinking: u32,
    agents: u32,
    background: u32,
    retries: u32,
    peers: u32,
    fails: u32,
    compacts: u32,
}

impl Tally {
    fn add(&mut self, it: &Item, agent_runs: &[String]) {
        self.tools += it.tools;
        self.thinking += it.thinking;
        self.agents += it.agents;
        self.retries += it.retries;
        self.peers += it.peers;
        self.fails += it.fails;
        self.compacts += it.compacts;
        for (task, failed) in &it.notices {
            if !task.as_ref().is_some_and(|t| agent_runs.contains(t)) {
                self.background += 1;
            }
            if *failed {
                self.fails += 1;
            }
        }
    }

    /// 那一行的字：段里什么都没数到、或只有一条压缩摘要（本来就是一行，不再套一层）⇒ 不出行（`None`，这一段不折）。
    fn parts(&self) -> Option<Vec<Part>> {
        let counted = [
            self.tools,
            self.thinking,
            self.agents,
            self.background,
            self.retries,
            self.peers,
        ];
        let others: u32 = counted.iter().sum();
        if others == 0 && self.compacts <= 1 {
            return None;
        }
        let mut out = vec![plain(copy_text("rsTurns.proc.head", &[]))];
        let n = |v: u32| v.to_string();
        let counts = [
            (
                self.tools,
                copy_text("rsTurns.proc.tools", &[("n", &n(self.tools))]),
            ),
            (
                self.thinking,
                copy_text("rsTurns.proc.thinking", &[("n", &n(self.thinking))]),
            ),
            (
                self.agents,
                copy_text("rsTurns.proc.agents", &[("n", &n(self.agents))]),
            ),
            (
                self.background,
                copy_text("rsTurns.proc.background", &[("n", &n(self.background))]),
            ),
            (
                self.retries,
                copy_text("rsTurns.proc.retries", &[("n", &n(self.retries))]),
            ),
            (
                self.peers,
                copy_text("rsTurns.proc.peers", &[("n", &n(self.peers))]),
            ),
        ];
        out.extend(
            counts
                .into_iter()
                .filter(|(v, _)| *v > 0)
                .map(|(_, t)| plain(t)),
        );
        if self.fails > 0 {
            out.push(Part {
                text: copy_text("rsTurns.proc.fails", &[("n", &n(self.fails))]),
                tone: Tone::Fail,
            });
        }
        Some(out)
    }
}

#[derive(Default)]
struct Open {
    row: TurnRow,
    reply_text: String,
    /// 派出 agent 的那几次工具调用 id（它们的结果不算工具，算 agent）。
    agent_calls: Vec<String>,
    /// 子 agent 的运行 id（派出调用的结果说出来的 · 交回的）：它们的收场通知不算后台任务。
    agent_runs: Vec<String>,
    /// 这一轮里的主线记录（文件序）。
    items: Vec<Item>,
    /// 停下这一轮的那一条（中断标记 / 报错卡，`items` 的下标）；后面又有 assistant 记录 ⇒ 作废。
    stop: Option<usize>,
    /// 这一轮里的排队插话（记录 id · 原文），等下一句你说的来对。
    queued: Vec<(String, String)>,
}

impl TurnRow {
    /// 答的那一下按看的那一台的时区写起止的钟面与每段右端那一截（扫出来的那一份进了索引缓存、不带钟面：几个看的人时区可能不同）。
    /// 在 [`dress_live`] 之前调（还在跑的那一段的右端由它按钟面改写）。
    pub(crate) fn stamp(&mut self, tz: &crate::common::time::Tz) {
        let face = |t: &str| crate::common::time::iso_hm(t, tz).unwrap_or_default();
        self.start_text = face(&self.start);
        self.end_text = face(&self.end);
        for f in &mut self.folds {
            f.start_text = face(&f.start);
            f.span = done_span(&f.start, &f.end, &f.start_text, &face(&f.end));
        }
    }
}

impl Open {
    /// 收这一轮：合计 · 露着的 · 划段（每段的字与右端那一截）· 回复头。
    fn close(mut self) -> TurnRow {
        if let Some(i) = self.stop.take() {
            self.items[i].shown = true;
        }
        let mut total = Tally::default();
        for it in &self.items {
            total.add(it, &self.agent_runs);
        }
        let r = &mut self.row;
        (
            r.tools,
            r.thinking,
            r.agents,
            r.background,
            r.retries,
            r.peers,
            r.fails,
        ) = (
            total.tools,
            total.thinking,
            total.agents,
            total.background,
            total.retries,
            total.peers,
            total.fails,
        );
        r.reply = reply_head(&self.reply_text);
        // 划段：露着的那一条把过程隔开；一段从「你那句 / 露着的某一条」之后起，到下一条露着的为止。
        let mut after = r.uuid.clone();
        let mut seg: Option<(Tally, String, String)> = None;
        let mut folds = Vec::new();
        let flush =
            |seg: &mut Option<(Tally, String, String)>, after: &str, folds: &mut Vec<Fold>| {
                if let Some((tally, start, end)) = seg.take() {
                    if let Some(parts) = tally.parts() {
                        folds.push(Fold {
                            after: after.to_string(),
                            parts,
                            span: Span::default(),
                            phase: Phase::Idle,
                            start,
                            end,
                            start_text: String::new(),
                        });
                    }
                }
            };
        for it in &self.items {
            if let (true, Some(u)) = (it.shown, it.uuid.as_ref()) {
                flush(&mut seg, &after, &mut folds);
                r.shown.push(u.clone());
                after = u.clone();
                continue;
            }
            let (tally, start, end) = seg.get_or_insert_with(Default::default);
            tally.add(it, &self.agent_runs);
            if !it.ts.is_empty() {
                if start.is_empty() {
                    *start = it.ts.clone();
                }
                *end = it.ts.clone();
            }
        }
        let open_tail = seg.is_some();
        let before = folds.len();
        flush(&mut seg, &after, &mut folds);
        r.tail_fold = open_tail && folds.len() > before;
        r.folds = folds;
        // 右端那一截带钟面 ⇒ 答的那一下随钟面一起写（[`TurnRow::stamp`]）。
        self.row
    }
}

fn plain(text: String) -> Part {
    Part {
        text,
        tone: Tone::Plain,
    }
}

/// 收尾了 / 没在跑的那一段的右端：起止钟面（同一个 ⇒ 一个）＋ 用时（不满一秒不写）。
fn done_span(start: &str, end: &str, start_text: &str, end_text: &str) -> Span {
    let when = if start_text == end_text {
        start_text.to_string()
    } else {
        copy_text(
            "rsTurns.span.range",
            &[("from", start_text), ("to", end_text)],
        )
    };
    let ms = crate::common::time::parse_iso8601_ms;
    match (ms(start), ms(end)) {
        (Some(a), Some(b)) if b - a >= 1000 => Span {
            text: copy_text("rsTurns.span.done", &[("when", &when), ("dur", DUR_SLOT)]),
            from: Some(a),
            to: Some(b),
        },
        _ => Span {
            text: when,
            from: None,
            to: None,
        },
    }
}

/// 一步的说法：工具名 ＋ 主参数一行（没有 ⇒ 只工具名）。
fn step_of(name: &str, what: Option<&str>) -> String {
    match what {
        Some(w) => format!("{name} {w}"),
        None => name.to_string(),
    }
}

/// **正在跑的那一轮**按这台此刻的会话事实补上「现在 / 在等你」那一截（判定同会话事实：`needs` 由 [`super::facts_query::needs_of`] 判，
/// 活不活看这台 pidfile），写在这一轮尾巴那一段上。收尾了的轮不动；尾巴不是一段过程（最后露着的是正文）⇒ 只改 `phase`。
pub(crate) fn dress_live(row: &mut TurnRow, needs: Option<&Needs>, live: bool) {
    if row.done {
        return;
    }
    let phase = if needs.is_some() {
        Phase::Awaiting
    } else if live {
        Phase::Running
    } else {
        return;
    };
    row.phase = phase;
    let pending = row.pending.last().cloned();
    let Some(fold) = row.folds.last_mut().filter(|_| row.tail_fold) else {
        return;
    };
    fold.phase = phase;
    let start = crate::common::time::parse_iso8601_ms(&fold.start);
    let since = Span {
        text: copy_text(
            "rsTurns.span.since",
            &[("hm", &fold.start_text), ("dur", DUR_SLOT)],
        ),
        from: start,
        to: None,
    };
    if let Some(n) = needs {
        let step = step_of(n.tool.as_deref().unwrap_or_default(), n.what.as_deref());
        let text = match n.kind {
            NeedsKind::Approve | NeedsKind::Plan if n.tool.is_some() => {
                copy_text("rsTurns.proc.awaitApprove", &[("step", &step)])
            }
            NeedsKind::Answer => copy_text(
                "rsTurns.proc.awaitAnswer",
                &[("step", n.what.as_deref().unwrap_or(&step))],
            ),
            _ => copy_text("rsTurns.proc.awaitYou", &[]),
        };
        fold.parts.push(Part {
            text,
            tone: Tone::Need,
        });
        fold.span = match n.since_ms {
            Some(t) => Span {
                text: copy_text("rsTurns.span.waited", &[("dur", DUR_SLOT)]),
                from: Some(t as i64),
                to: None,
            },
            None => since,
        };
        return;
    }
    if let Some(p) = pending {
        let step = step_of(&p.name, p.what.as_deref());
        fold.parts.push(Part {
            text: copy_text("rsTurns.proc.now", &[("step", &step)]),
            tone: Tone::Now,
        });
    }
    fold.span = since;
}

/// 回复头：头三行**正文**——代码块整块不算（围栏连同里面的行），空行不算；只有代码 ⇒ 「仅代码」，不露代码原文。
/// 行内的排版记号（粗体 / 斜体的 `**` `__`、行内代码的反引号、标题的 `#`、引用的 `>`）去掉，只留字（悬停卡是纯文本）。
fn reply_head(reply_text: &str) -> String {
    let mut fenced = false;
    let mut code = false;
    let mut head: Vec<String> = Vec::new();
    for l in reply_text.lines().map(str::trim) {
        if l.starts_with("```") {
            fenced = !fenced;
            code = true;
            continue;
        }
        if fenced {
            continue;
        }
        let l = plain_line(l);
        if !l.is_empty() {
            head.push(l);
            if head.len() == REPLY_LINES {
                break;
            }
        }
    }
    if head.is_empty() && code {
        copy_text("rsTurns.reply.codeOnly", &[])
    } else {
        clip(&head.join("\n"), REPLY_MAX)
    }
}

/// 一行正文去掉行内排版记号：行首的 `#` / `>`，以及 `**` `__` 与反引号。
fn plain_line(l: &str) -> String {
    let l = l.trim_start_matches(['#', '>']).trim_start();
    l.replace("**", "").replace("__", "").replace('`', "")
}

/// 截到 `max` 个字（Unicode 标量），截了加省略号。
fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// 一条主线记录喂进开着的那一轮。
fn feed(tree: &str, t: &mut Open, v: &Value) {
    let ts = v.get("timestamp").and_then(Value::as_str).unwrap_or("");
    let uuid = v
        .get("uuid")
        .and_then(Value::as_str)
        .filter(|u| !u.is_empty());
    let mut it = Item {
        uuid: uuid.map(str::to_string),
        ts: ts.to_string(),
        ..Item::default()
    };
    let here = t.items.len();
    let blocks = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array);
    let links = crate::agents::run_faces(tree).child_links(v);
    match v.get("type").and_then(Value::as_str) {
        Some("assistant") => {
            if !ts.is_empty() {
                t.row.end = ts.to_string();
            }
            t.stop = None;
            let failed = v.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true);
            let spawned: Vec<&str> = links
                .iter()
                .filter(|l| l.run.is_none())
                .filter_map(|l| l.tool.as_deref())
                .collect();
            let mut text = String::new();
            for b in blocks.into_iter().flatten() {
                match b.get("type").and_then(Value::as_str) {
                    Some("tool_use") => {
                        let id = b.get("id").and_then(Value::as_str).unwrap_or_default();
                        if spawned.contains(&id) {
                            it.agents += 1;
                            t.agent_calls.push(id.to_string());
                        } else {
                            it.tools += 1;
                        }
                        if let Some(name) = b.get("name").and_then(Value::as_str) {
                            t.row.pending.retain(|p| p.id != id);
                            t.row.pending.push(PendingCall {
                                id: id.to_string(),
                                name: name.to_string(),
                                what: what_of(name, b.get("input")),
                                at: None,
                                at_ms: None,
                                state: Default::default(),
                                why: None,
                                text: None,
                                why_text: None,
                            });
                        }
                        // 又调了工具 ⇒ 之前的正文是中间的话，不是结论（回复头不取它；它照样露着）。
                        t.reply_text.clear();
                    }
                    Some("thinking") | Some("redacted_thinking") => it.thinking += 1,
                    Some("text") => {
                        if let Some(s) = b.get("text").and_then(Value::as_str) {
                            text.push_str(s);
                        }
                    }
                    _ => {}
                }
            }
            if failed {
                t.stop = uuid.map(|_| here);
                t.row.done = true;
            } else if !text.trim().is_empty() {
                // 带正文 ⇒ 露着（同一条里又调了工具也一样：那几次调用只进这一轮的合计）。
                it.shown = uuid.is_some();
                if it.tools + it.agents == 0 && t.reply_text.chars().count() < REPLY_MAX * 4 {
                    // 结论正文攒够回复头要的那么多就停（四倍余量给空行与短行）。
                    if !t.reply_text.is_empty() {
                        t.reply_text.push('\n');
                    }
                    t.reply_text.push_str(&text);
                }
            }
            let stop = v
                .get("message")
                .and_then(|m| m.get("stop_reason"))
                .and_then(Value::as_str);
            if stop == Some("end_turn") {
                t.row.done = true;
            }
        }
        Some("user") => {
            if !ts.is_empty() {
                t.row.end = ts.to_string();
            }
            // 派出调用的结果说出了子 agent 的运行 id ⇒ 它的收场通知归 agent。
            for l in &links {
                if let (Some(tool), Some(run)) = (&l.tool, &l.run) {
                    if t.agent_calls.contains(tool) && !t.agent_runs.contains(run) {
                        t.agent_runs.push(run.clone());
                    }
                }
            }
            let tur = v.get("toolUseResult");
            for b in blocks.into_iter().flatten() {
                if b.get("type").and_then(Value::as_str) != Some("tool_result") {
                    continue;
                }
                if let Some(id) = b.get("tool_use_id").and_then(Value::as_str) {
                    t.row.pending.retain(|p| p.id != id);
                }
                if b.get("is_error").and_then(Value::as_bool) == Some(true)
                    && !crate::agents::step_result_of(tree, b, tur).rejected
                {
                    it.fails += 1;
                }
            }
            if let Some(said) = crate::agents::user_text_of(tree, v) {
                use crate::agents::Speaker;
                match said.speaker {
                    Speaker::TaskNotification {
                        task_id, status, ..
                    } => it
                        .notices
                        .push((task_id, status.as_deref() == Some("failed"))),
                    Speaker::AgentMessage {
                        handback: true,
                        from,
                        ..
                    } => t.agent_runs.extend(from),
                    Speaker::AgentMessage { .. } | Speaker::PeerSession { .. } => it.peers += 1,
                    Speaker::CompactSummary => it.compacts += 1,
                    Speaker::Interrupt => {
                        t.stop = uuid.map(|_| here);
                        t.row.done = true;
                        t.row.pending.clear();
                    }
                    _ => {}
                }
            }
        }
        Some("system") if v.get("subtype").and_then(Value::as_str) == Some("api_error") => {
            it.retries += 1;
        }
        _ => {}
    }
    t.items.push(it);
}

/// 从 `from` 起逐行扫，每收尾一轮交一条；返回 `(轮数, end)`，`end` ＝ 最后一个完整行的末字节（残尾不计）。
pub(crate) fn scan_turns<R: std::io::BufRead>(
    mut r: R,
    from: u64,
    mut on_row: impl FnMut(&TurnRow) -> std::io::Result<()>,
) -> std::io::Result<(u64, u64)> {
    let mut scan = TurnScan::default();
    let mut end = from;
    let mut count: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let read = r.read_until(b'\n', &mut buf)?;
        if read == 0 || buf.last() != Some(&b'\n') {
            break;
        }
        let at = end;
        end += read as u64;
        let Some(v) = super::record_scan::parse_record(&buf[..buf.len() - 1]) else {
            continue;
        };
        if let Some(row) = scan.line(at, &v) {
            on_row(&row)?;
            count += 1;
        }
    }
    if let Some(row) = scan.finish() {
        on_row(&row)?;
        count += 1;
    }
    Ok((count, end))
}

/// 逐条记录攒轮次：[`TurnScan::line`] 收一条（`at` = 它的字节位置），新的一句收尾了上一轮就交出上一轮；
/// [`TurnScan::finish`] 交出还开着的那一轮。
pub(crate) struct TurnScan {
    tree: &'static str,
    open: Option<Open>,
}

impl Default for TurnScan {
    fn default() -> Self {
        TurnScan {
            tree: crate::agents::record_tree_kind().unwrap_or_default(),
            open: None,
        }
    }
}

impl TurnScan {
    pub(crate) fn line(&mut self, at: u64, v: &Value) -> Option<TurnRow> {
        let spoke = crate::agents::user_text_of(self.tree, v);
        if let Some(said) = user_input_given(v, spoke.as_ref()) {
            let speech = spoke.and_then(|s| s.speech()).unwrap_or_default();
            let first = speech
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("");
            let closed = self.open.take().map(|mut t| {
                t.row.done = true;
                let said = speech.trim();
                t.row.landed = t
                    .queued
                    .iter()
                    .filter(|(_, q)| q == said)
                    .map(|(id, _)| id.clone())
                    .collect();
                t.close()
            });
            self.open = Some(Open {
                row: TurnRow {
                    at,
                    uuid: said.uuid,
                    end: said.timestamp.clone(),
                    start: said.timestamp,
                    said: clip(first, SAID_MAX),
                    ..TurnRow::default()
                },
                ..Open::default()
            });
            return closed;
        }
        if crate::agents::run_of_record(self.tree, v).is_some() {
            return None;
        }
        if let (Some(q), Some(t)) = (
            crate::agents::queued_text_of(self.tree, v),
            self.open.as_mut(),
        ) {
            t.queued.push((crate::agents::line_id(at), q));
        }
        if let Some(t) = self.open.as_mut() {
            feed(self.tree, t, v);
        }
        None
    }

    pub(crate) fn finish(self) -> Option<TurnRow> {
        self.open.map(Open::close)
    }
}

/// 某一条记录（`uuid`）落在第几轮、那一轮从何时起（分叉框顶上那一句）。轮的口径同 [`scan_turns`]：你说的一句起一轮。
/// 找不到那一条 / 它在第一句之前 ⇒ `None`。
pub(crate) fn turn_at<R: std::io::BufRead>(r: R, uuid: &str) -> Option<(u64, String)> {
    let tree = crate::agents::record_tree_kind().unwrap_or_default();
    let mut n: u64 = 0;
    let mut start = String::new();
    for line in r.lines() {
        let line = line.ok()?;
        let Ok(v) = serde_json::from_str::<Value>(line.trim_start_matches('\u{feff}').trim())
        else {
            continue;
        };
        let spoke = crate::agents::user_text_of(tree, &v);
        if let Some(said) = user_input_given(&v, spoke.as_ref()) {
            n += 1;
            start = said.timestamp;
        }
        if v.get("uuid").and_then(Value::as_str) == Some(uuid) {
            return (n > 0).then_some((n, start));
        }
    }
    None
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/turns_tests.rs"]
mod tests;
