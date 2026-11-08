//! 一份 `pb dump`（形状 8，pb 0.7.2）⇒ 界面直接排版的成品（契约 [`cmd_wire::plan`] 里的类型）。纯函数：不读盘、不起进程。
//!
//! 换个方向排、换个名字，再把界面要的数与先后算好（界面照画、不数不排）：
//! - 每格「指着它的」边：把全片的边倒过来排；每份文件还挂在哪几格：把全片的文件声明倒过来查；
//! - 权力范围的负责人 · 节点的负责人（`assignee`）· 写得动它元数据的那一位（`signer`）：经 [`super::WhoPort`] 对到会话；
//! - 算不算活、签收了没有：收的值 ＝ 领域表 `rules[]` 里加 · 去状态的那个值（[`done_value`]），节点的类许记它才算活；
//!   顶层进度 · 根节点底下签收了几格 · 没签收的格数 · 行尾原因短词照它数好、写好。本族不写死哪个值算「做完」；
//! - 状态那一行字（同 `pb render`：值与笔数，` · ` 隔开）后端写好；
//! - 「编号 标题」：每片一张 `labels`（编号 ⇒「编号 标题」），凡出现编号界面照它写，界面不拼；
//! - 话里提到的格（`refs`）：pb 的 `start · end` 按 Unicode 码位，换成界面切字用的 UTF-16 下标（[`utf16_at`]）；
//! - 审图（`audits`）一份一项：agent 按的经 [`super::WhoPort`] 对到会话，审哪 · 人按的 · 结果 · 时刻那几句后端写好。
//!
//! 顶层与每一格的子照 pb 给的先后（pb 照 after 排好），这里不重排；阶段照领域表给的那几个。
//!
//! 格只收 permanent 那一种图的节点（排期的节点不进大纲、不画圈、不进倒排；它们的「编号 标题」照样进 `labels`）。
//! 整份 dump 里不带 agent 看到的那一段（点到一格再 `pb dump --node` 取，见帧面 `plan-cell-view`）。
//! 只读 pb 认得的那几个键，造的是线上类型：pb 那边多出来的键不带过来。

use super::{Live, WhoPort, Whose};
use crate::wire::SessionFaceExt as _;
use cmd_wire::plan::{
    PlanArchived, PlanAudit, PlanAuditBy, PlanAuditSaid, PlanAuditStatus, PlanBlock, PlanBlockSaid,
    PlanCell, PlanCellSaid, PlanCheck, PlanDomain, PlanEdgeWords, PlanEdges, PlanFile,
    PlanFileState, PlanKids, PlanKind, PlanListSlice, PlanListWorkspace, PlanLook, PlanPendingSet,
    PlanPhase, PlanProgress, PlanRead, PlanReadError, PlanRed, PlanRef, PlanRefs, PlanSessionBlock,
    PlanSessionSaid, PlanSetsTo, PlanSlice, PlanSliceSaid, PlanStale, PlanState, PlanStatusCode,
    PlanTakeover, PlanUnbuilt, PlanUndecidable, PlanUnowned, PlanWho, PlanWhyCode, PlanWhyShort,
};
use copy_core::copy_text;
use serde_json::Value;
use std::collections::BTreeMap;

/// 画线的四种边（pb 的边名）。
pub(crate) const EDGES: [&str; 4] = ["to", "with", "after", "replaces"];

/// 待定那一类（pb 三张表都登的类名）与人定了的值（pb 的协议值：只由原话节点记）。
pub(crate) const PENDING_KIND: &str = "待定";
pub(crate) const DECIDED: &str = "decided";

/// pending set 的代号：源节点在划出的权力范围里，等上一级删掉划出那一项（交回等收）。
const WAITS_UPPER: &str = "source_delegated";

/// 收的值：领域表 `rules[]` 里加 · 去状态（effect 为 `add` · `remove`）的那个值，取第一条。表里没有 ⇒ `None`（研究）。
pub(crate) fn done_value(repo: &Value) -> Option<String> {
    arr(repo, "rules")
        .filter(|r| {
            matches!(
                r.get("effect").and_then(Value::as_str),
                Some("add" | "remove")
            )
        })
        .find_map(|r| r.get("arg").and_then(Value::as_str).map(str::to_string))
}

/// pb 的 `file_check` ⇒ 对账码；认不出 ⇒ `None`。
pub(crate) fn file_code(s: Option<&str>) -> Option<PlanFileState> {
    match s? {
        "present" => Some(PlanFileState::Ok),
        "uncommitted" => Some(PlanFileState::Uncommitted),
        "missing" => Some(PlanFileState::Missing),
        "empty" => Some(PlanFileState::Empty),
        "invalid" => Some(PlanFileState::Broken),
        _ => None,
    }
}

/// 文件核对的码 ⇒ 那一格写的字（界面照抄）。
pub(crate) fn file_text(c: PlanFileState) -> String {
    match c {
        PlanFileState::Ok => copy_text("plan.fileState.ok", &[]),
        PlanFileState::Uncommitted => copy_text("plan.fileState.uncommitted", &[]),
        PlanFileState::Missing => copy_text("plan.fileState.missing", &[]),
        PlanFileState::Empty => copy_text("plan.fileState.empty", &[]),
        PlanFileState::Broken => copy_text("plan.fileState.broken", &[]),
    }
}

/// 一个状态值写成的字：pb 的协议值（`decided`）照文案表写；领域表自己登的值本来就是字，照原样。
pub(crate) fn value_words(v: &str) -> String {
    if v == DECIDED {
        copy_text("bePlan.state.decided", &[])
    } else {
        v.to_string()
    }
}

/// 待执行那一笔写成的字：值的字；去一笔 ⇒ `bePlan.state.removeValue`。没有值 ⇒ `None`。
fn set_said(value: Option<&str>, op: &str) -> Option<String> {
    let w = value_words(value?);
    Some(if op == "remove" {
        copy_text("bePlan.state.removeValue", &[("value", &w)])
    } else {
        w
    })
}

/// 状态那一行字：值照给的先后（写成字，[`value_words`]），笔数大于一写在值后面，用文案表的分隔；一个值都没有 ⇒ `None`。
pub(crate) fn state_line(state: &[PlanState]) -> Option<String> {
    (!state.is_empty()).then(|| {
        state
            .iter()
            .map(|x| {
                let w = value_words(&x.value);
                if x.count == 1 {
                    w
                } else {
                    format!("{w} {}", x.count)
                }
            })
            .collect::<Vec<_>>()
            .join(&copy_text("kit.text.sep", &[]))
    })
}

/// 原因 ⇒ 行尾那一个短词。
fn why_short(code: &PlanWhyCode) -> PlanWhyShort {
    let (text, wait) = match code {
        PlanWhyCode::Nosign => (copy_text("plan.whyShort.nosign", &[]), false),
        PlanWhyCode::Upper => (copy_text("plan.whyShort.upper", &[]), true),
        PlanWhyCode::Undecided => (copy_text("plan.whyShort.undecided", &[]), false),
        PlanWhyCode::Inside { done, of } => (
            copy_text(
                "plan.whyShort.inside",
                &[("done", &done.to_string()), ("of", &of.to_string())],
            ),
            false,
        ),
    };
    PlanWhyShort { text, wait }
}

/// 一格画哪一种：待定没人定另画一种；否则照状态码；类不许记收的值 ⇒ 不算活。
fn look_of(status: Option<PlanStatusCode>, undecided: bool) -> PlanLook {
    match (undecided, status) {
        (true, _) => PlanLook::Undecided,
        (false, Some(PlanStatusCode::Done)) => PlanLook::Done,
        (false, Some(PlanStatusCode::Open)) => PlanLook::Open,
        (false, None) => PlanLook::None,
    }
}

/// 行右端那一句：状态那一行与原因短词，有几段连几段（文案表的分隔）；都没有 ⇒ `None`。
fn end_of(state_line: Option<&str>, why: Option<&PlanWhyShort>) -> Option<String> {
    let parts: Vec<&str> = [state_line, why.map(|w| w.text.as_str())]
        .into_iter()
        .flatten()
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(&copy_text("kit.text.sep", &[])))
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

/// pb 那一句（读不成的原因）：是字照用，不是字写成 JSON 原文（不丢）。
fn said(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

fn strs(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn arr<'a>(v: &'a Value, k: &str) -> impl Iterator<Item = &'a Value> {
    v.get(k).and_then(Value::as_array).into_iter().flatten()
}

fn flag(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool) == Some(true)
}

fn n32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// 一段话的第 `cp` 个码位在 UTF-16 里是第几个单元（越过末尾 ⇒ 停在末尾）。
pub(crate) fn utf16_at(text: &str, cp: u64) -> u64 {
    text.chars()
        .take(usize::try_from(cp).unwrap_or(usize::MAX))
        .map(|c| c.len_utf16() as u64)
        .sum()
}

/// 一段话里提到的格（pb 原样：码位）⇒ 线上那一份（UTF-16 下标）。
pub(crate) fn refs16(text: Option<&str>, refs: Option<&Value>) -> Vec<PlanRef> {
    let text = text.unwrap_or_default();
    refs.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|r| {
            let at = |k: &str| {
                n32(utf16_at(
                    text,
                    r.get(k).and_then(Value::as_u64).unwrap_or(0),
                ))
            };
            PlanRef {
                start: at("start"),
                end: at("end"),
                ids: strs(r.get("ids")),
            }
        })
        .collect()
}

/// 一个 id 对到的会话（线上那一格）。没有 id · 空 id ⇒ `None`。
pub(crate) fn who_of(id: Option<&str>, who: WhoPort) -> Option<PlanWho> {
    let id = id.filter(|i| !i.is_empty())?;
    let needs_of = |l: &Option<Live>| {
        l.as_ref()
            .and_then(|l| serde_json::to_value(l.needs).ok())
            .and_then(|v| v.as_str().map(str::to_string))
    };
    let face_of = |l: &Option<Live>| {
        crate::wire::SessionFace::of(match l {
            Some(l) => crate::wire::Life::Live {
                activity: l.activity,
                host: None,
            },
            None => crate::wire::Life::Ended,
        })
    };
    let (kind, sid, needs, face, label) = match who(id) {
        Whose::Session { sid, live } => {
            ("session", Some(sid), needs_of(&live), face_of(&live), None)
        }
        Whose::Subagent {
            parent,
            parent_title,
            live,
        } => {
            let label = copy_text("plan.who.sub", &[("session", &parent_title)]);
            (
                "subagent",
                Some(parent),
                needs_of(&live),
                face_of(&live),
                Some(label),
            )
        }
        Whose::Unknown => (
            "unknown",
            None,
            None,
            crate::wire::SessionFace::of(crate::wire::Life::Unclear),
            Some(copy_text("plan.who.unknown", &[("id", &short_id(id))])),
        ),
    };
    Some(PlanWho {
        id: id.to_string(),
        kind: kind.to_string(),
        sid,
        needs,
        face,
        label,
    })
}

fn who_at(v: &Value, k: &str, who: WhoPort) -> Option<PlanWho> {
    who_of(v.get(k).and_then(Value::as_str), who)
}

/// 领域表里的类（一片的 · 能选的那几档的，同一形）。
fn kinds_of(v: &Value) -> Vec<PlanKind> {
    arr(v, "kinds")
        .map(|k| {
            let w = |e: &str| k.get("edge_labels").and_then(|w| s(w, e));
            PlanKind {
                name: s(k, "name"),
                edge_words: PlanEdgeWords {
                    to: w("to"),
                    with_: w("with"),
                    after: w("after"),
                    replaces: w("replaces"),
                },
                values: strs(k.get("values")),
            }
        })
        .collect()
}

/// 阶段表（一片的 · 能选的那几档的，同一形）。
fn phases_of(v: &Value) -> Vec<PlanPhase> {
    arr(v, "phases")
        .map(|p| PlanPhase { name: s(p, "name") })
        .collect()
}

/// 能选的领域（`pb dump` 与 `pb dump --domains` 顶上那一列，同一形）：表读不成 ⇒ `error`、类与阶段空。
pub(crate) fn domains_of(doc: &Value) -> Vec<PlanDomain> {
    arr(doc, "domains")
        .map(|d| match d.get("error").filter(|e| !e.is_null()) {
            Some(e) => PlanDomain {
                name: s(d, "name"),
                error: Some(said(e)),
                kinds: Vec::new(),
                phases: Vec::new(),
            },
            None => PlanDomain {
                name: s(d, "name"),
                error: None,
                kinds: kinds_of(d),
                phases: phases_of(d),
            },
        })
        .collect()
}

/// 整份加工。`rev` 是这一份输出的摘要、`read_at` 是读到的时刻（[`super::book`] 给）。
/// 要你看 · 认可 · 退回 · 时刻的字此刻都空着：本子（[`super::book`]）· 认可记录（[`super::review`]）· [`with_time_texts`] 再填。
/// 随数据变的那几句（`said`）这里先写一遍；要时刻的那两句由 [`with_time_texts`] 写完时刻再整份重写。
pub fn make(doc: &Value, who: WhoPort, rev: String, read_at: u64) -> PlanRead {
    let repo = doc.get("current_repo").and_then(Value::as_str);
    let slices: Vec<PlanSlice> = arr(doc, "repos").map(|r| slice(r, repo, who)).collect();
    let by_session = by_session(&slices);
    let domains = domains_of(doc);
    let mut v = PlanRead {
        pb: s(doc, "pb"),
        workspace: s(doc, "workspace").unwrap_or_default(),
        repo: repo.map(str::to_string),
        auto: flag(doc, "auto_continue"),
        domains,
        slices,
        by_session,
        rev,
        read_at,
        read_at_text: None,
        seen: None,
        stale: None,
        need_count: 0,
    };
    write_said(&mut v);
    v
}

/// 会话 ⇒ 它负责的那个权力范围（会话头那一枚标）：
/// `{片, 权力范围, 根节点（编号）, 根节点标题（最上面那一个 ⇒ null）, 是不是最上面那一个, 阶段, 站在哪一格（编号 ＋ 标题）, via: session|subagent}`。
/// 子 agent 负责的记在父会话名下；一个会话名下几个 ⇒ 自己负责的压过子 agent 替它负责的，同档取片与权力范围的先后里第一个。
/// 对不上会话的负责人不进表。
pub(crate) fn by_session(slices: &[PlanSlice]) -> BTreeMap<String, PlanSessionBlock> {
    let mut out = BTreeMap::new();
    for pass in ["session", "subagent"] {
        for sl in slices {
            let title_of = |id: &str| {
                sl.cells
                    .iter()
                    .find(|c| c.id == id)
                    .and_then(|c| c.title.clone())
            };
            for b in &sl.blocks {
                let Some(owner) = &b.owner else { continue };
                let Some(sid) = owner.sid.as_deref() else {
                    continue;
                };
                if owner.kind != pass || out.contains_key(sid) {
                    continue;
                }
                let top = b.id == super::needs::TOP_BLOCK;
                let root = b.cells.first();
                out.insert(
                    sid.to_string(),
                    PlanSessionBlock {
                        slice: sl.name.clone(),
                        block: b.id.clone(),
                        cell: root.cloned(),
                        title: if top {
                            None
                        } else {
                            root.and_then(|r| title_of(r))
                        },
                        top,
                        phase: b.phase.clone(),
                        at: b.at.clone(),
                        at_title: b.at.as_deref().and_then(title_of),
                        via: pass.to_string(),
                        said: Default::default(),
                    },
                );
            }
        }
    }
    out
}

/// 「编号 标题」（没有标题 ⇒ 只写编号）。拼这一形只在这一处。
pub(crate) fn label(id: &str, title: Option<&str>) -> String {
    match title.filter(|t| !t.is_empty()) {
        Some(t) => format!("{id} {t}"),
        None => id.to_string(),
    }
}

/// 一格的四种边（`edges` 那一段，或倒过来的那一份）。
fn edges_of(get: impl Fn(&str) -> Vec<String>) -> PlanEdges {
    PlanEdges {
        to: get("to"),
        with_: get("with"),
        after: get("after"),
        replaces: get("replaces"),
    }
}

fn op_of(v: &Value) -> String {
    s(v, "op").unwrap_or_else(|| "add".to_string())
}

/// 空的一片（读不成那一刻先给这一形，再往里填）。
fn empty_slice(sl: &Value, repo: Option<&str>) -> PlanSlice {
    let name = s(sl, "name").unwrap_or_default();
    PlanSlice {
        current: repo == Some(name.as_str()),
        name,
        domain: s(sl, "domain"),
        // 仓库相对工作区根在哪（`pb repo <路径>` 登记的）：照 pb 给的，这里不补。
        path: s(sl, "path"),
        // 只做计划：人还没 `pb adopt`。
        docs_only: flag(sl, "plan_only"),
        error: None,
        stale: None,
        bare: false,
        kinds: Vec::new(),
        phases: Vec::new(),
        done_value: None,
        top: Vec::new(),
        progress: None,
        open_cells: 0,
        blocks: Vec::new(),
        unbuilt_blocks: Vec::new(),
        unowned: PlanUnowned {
            paths: None,
            why: None,
        },
        pending_sets: Vec::new(),
        check: PlanCheck {
            unreadable: Vec::new(),
            unparsed: Vec::new(),
            red: Vec::new(),
            undecidable: Vec::new(),
        },
        labels: BTreeMap::new(),
        cells: Vec::new(),
        archived: Vec::new(),
        audits: Vec::new(),
        needs: Vec::new(),
        need_count: 0,
        returned: None,
        said: Default::default(),
    }
}

/// 一片（pb 的一个仓库）。带 `error` 的只出名字、领域、仓库位置与那一句（pb 那时给不出别的），`bare` 说「只有头几格」。
pub(crate) fn slice(sl: &Value, repo: Option<&str>, who: WhoPort) -> PlanSlice {
    let mut out = empty_slice(sl, repo);
    if let Some(e) = sl.get("error").filter(|e| !e.is_null()) {
        // 读不成又没读好过：形状照常（空的）。
        out.error = Some(said(e));
        out.bare = true;
        return out;
    }
    out.kinds = kinds_of(sl);
    out.phases = phases_of(sl);
    let done = done_value(sl);
    out.done_value = done.clone();
    // 算活的类：许记收的值。
    let work_kinds: Vec<String> = match &done {
        Some(v) => out
            .kinds
            .iter()
            .filter(|k| k.values.contains(v))
            .filter_map(|k| k.name.clone())
            .collect(),
        None => Vec::new(),
    };
    // permanent 那一种图（排期那一种不进格）。表里没登 ⇒ 都收。
    let permanent = arr(sl, "graphs")
        .find(|g| g.get("lifetime").and_then(Value::as_str) == Some("permanent"))
        .and_then(|g| s(g, "name"));
    let nodes: Vec<&Value> = arr(sl, "nodes").collect();
    let kept: Vec<&Value> = nodes
        .iter()
        .copied()
        .filter(|n| permanent.is_none() || s(n, "graph") == permanent)
        .collect();
    let id_of = |c: &Value| s(c, "id").unwrap_or_default();
    let kept_ids: Vec<String> = kept.iter().map(|c| id_of(c)).collect();
    let is_cell = |id: &str| kept_ids.iter().any(|k| k == id);

    out.pending_sets = arr(sl, "pending_sets")
        .map(|p| {
            let value = s(p, "value");
            let op = op_of(p);
            PlanPendingSet {
                source: s(p, "source"),
                target: s(p, "target"),
                said: set_said(value.as_deref(), &op),
                value,
                op,
                blocked_reason: s(p, "blocked_reason"),
                blocked_code: s(p, "blocked_code"),
            }
        })
        .collect();

    // 倒过来的两张索引：谁的边指着它 · 同一份文件还挂在哪几格。
    let mut pointed: BTreeMap<String, BTreeMap<&str, Vec<String>>> = BTreeMap::new();
    let mut by_path: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in &kept {
        let id = id_of(c);
        for k in EDGES {
            for t in strs(c.get("edges").and_then(|e| e.get(k))) {
                pointed
                    .entry(t)
                    .or_default()
                    .entry(k)
                    .or_default()
                    .push(id.clone());
            }
        }
        for f in arr(c, "files") {
            if let Some(p) = f.get("path").and_then(Value::as_str) {
                by_path.entry(p.to_string()).or_default().push(id.clone());
            }
        }
    }

    // 算活的格此刻签收了没有（先一遍，子的数要用）。
    let status_of = |c: &Value| -> Option<PlanStatusCode> {
        let kind = s(c, "kind")?;
        let v = done.as_deref()?;
        if !work_kinds.contains(&kind) {
            return None;
        }
        let signed = arr(c, "state").any(|x| {
            x.get("value").and_then(Value::as_str) == Some(v)
                && x.get("count").and_then(Value::as_u64).unwrap_or(0) > 0
        });
        Some(if signed {
            PlanStatusCode::Done
        } else {
            PlanStatusCode::Open
        })
    };
    let statuses: BTreeMap<String, Option<PlanStatusCode>> =
        kept.iter().map(|c| (id_of(c), status_of(c))).collect();
    let kids_of = |children: &[String]| -> PlanKids {
        let mut k = PlanKids { done: 0, of: 0 };
        for ch in children {
            match statuses.get(ch).copied().flatten() {
                Some(PlanStatusCode::Done) => {
                    k.done += 1;
                    k.of += 1;
                }
                Some(PlanStatusCode::Open) => k.of += 1,
                None => {}
            }
        }
        k
    };

    out.cells = kept
        .iter()
        .map(|c| {
            let id = id_of(c);
            let back = pointed.get(&id);
            let files = arr(c, "files")
                .map(|f| {
                    let path = f.get("path").and_then(Value::as_str).unwrap_or_default();
                    PlanFile {
                        path: path.to_string(),
                        state_code: file_code(f.get("file_check").and_then(Value::as_str)),
                        check_text: file_code(f.get("file_check").and_then(Value::as_str))
                            .map(file_text),
                        note: s(f, "note").filter(|n| !n.is_empty()),
                        also_by: by_path
                            .get(path)
                            .into_iter()
                            .flatten()
                            .filter(|o| **o != id)
                            .cloned()
                            .collect(),
                    }
                })
                .collect();
            let state: Vec<PlanState> = arr(c, "state")
                .filter_map(|x| {
                    Some(PlanState {
                        value: s(x, "value")?,
                        count: n32(x.get("count").and_then(Value::as_u64).unwrap_or(0)),
                    })
                })
                .collect();
            let children = strs(c.get("children"));
            let status_code = statuses.get(&id).copied().flatten();
            let undecided = s(c, "kind").as_deref() == Some(PENDING_KIND)
                && !state.iter().any(|x| x.value == DECIDED && x.count > 0);
            let why_code = if undecided {
                Some(PlanWhyCode::Undecided)
            } else if status_code == Some(PlanStatusCode::Open) {
                let waits = out.pending_sets.iter().any(|p| {
                    p.target.as_deref() == Some(id.as_str())
                        && p.value == done
                        && p.op == "add"
                        && p.blocked_code.as_deref() == Some(WAITS_UPPER)
                });
                let kids = kids_of(&children);
                Some(if waits {
                    PlanWhyCode::Upper
                } else if kids.of > 0 {
                    PlanWhyCode::Inside {
                        done: kids.done,
                        of: kids.of,
                    }
                } else {
                    PlanWhyCode::Nosign
                })
            } else {
                None
            };
            let sets = arr(c.get("edges").unwrap_or(&Value::Null), "sets")
                .map(|x| PlanSetsTo {
                    target: s(x, "target"),
                    value: s(x, "value"),
                    op: op_of(x),
                })
                .collect();
            let refs = c.get("refs");
            let title = c.get("title").and_then(Value::as_str);
            let body = c.get("body").and_then(Value::as_str);
            let line = state_line(&state);
            let short = why_code.as_ref().map(why_short);
            let pending = out
                .pending_sets
                .iter()
                .any(|p| p.target.as_deref() == Some(id.as_str()));
            let quiet = (line.is_none() && !pending).then(|| {
                if !children.is_empty() && status_code.is_some() {
                    copy_text("plan.state.inner", &[])
                } else {
                    copy_text("plan.state.none", &[])
                }
            });
            PlanCell {
                edges: edges_of(|k| strs(c.get("edges").and_then(|e| e.get(k)))),
                pointed_by: edges_of(|k| back.and_then(|b| b.get(k)).cloned().unwrap_or_default()),
                files,
                end: end_of(line.as_deref(), short.as_ref()),
                quiet,
                state_line: line,
                state,
                evidenced_by: strs(c.get("evidenced_by")),
                look: look_of(status_code, undecided),
                status_code,
                undecided,
                why_short: short,
                why_code,
                sets,
                block: s(c, "children_in"),
                owner: who_at(c, "assignee", who),
                signer: who_at(c, "signer", who),
                covers: strs(c.get("edges").and_then(|e| e.get("covers"))),
                covered_by: strs(c.get("covered_by"))
                    .into_iter()
                    .filter(|r| is_cell(r))
                    .collect(),
                refs: PlanRefs {
                    title: refs16(title, refs.and_then(|r| r.get("title"))),
                    body: refs16(body, refs.and_then(|r| r.get("body"))),
                },
                returned: None,
                said: Default::default(),
                kind: s(c, "kind"),
                parent: s(c, "parent"),
                children,
                title: title.map(str::to_string),
                body: body.map(str::to_string),
                id,
            }
        })
        .collect();

    out.top = strs(sl.get("top_level"));
    out.progress = done.as_ref().map(|_| {
        let k = kids_of(&out.top);
        PlanProgress {
            done: k.done,
            of: k.of,
        }
    });
    let find = |id: &str| out.cells.iter().find(|c| c.id == id);
    out.blocks = arr(sl, "scopes")
        .map(|b| {
            let cells = strs(b.get("root_nodes"));
            let inside = cells
                .first()
                .and_then(|r| find(r))
                .map(|root| kids_of(&root.children))
                .filter(|k| k.of > 0);
            let mut block = PlanBlock {
                id: s(b, "id").unwrap_or_default(),
                cells,
                inside,
                dir: s(b, "dir"),
                owner: who_at(b, "assignee", who),
                phase: s(b, "phase"),
                at: s(b, "current_node"),
                listed: flag(b, "listed"),
                handoff: s(b, "handoff"),
                handoff_error: s(b, "handoff_error"),
                takeover: PlanTakeover::Unsure,
                groups: super::groups::nobody(),
                said: Default::default(),
            };
            block.takeover = super::needs::takeover(&block);
            block
        })
        .collect();
    out.open_cells = n32(out
        .cells
        .iter()
        .filter(|c| c.status_code == Some(PlanStatusCode::Open))
        .count() as u64);
    out.unbuilt_blocks = arr(sl, "pending_scopes")
        .map(|b| PlanUnbuilt {
            id: s(b, "id"),
            cells: strs(b.get("root_nodes")),
        })
        .collect();
    let unowned = sl.get("unowned");
    out.unowned = PlanUnowned {
        paths: unowned
            .and_then(|u| u.get("paths"))
            .filter(|p| p.is_array())
            .map(|p| strs(Some(p))),
        why: unowned.and_then(|u| s(u, "error")),
    };
    let check = sl.get("check").unwrap_or(&Value::Null);
    out.check = PlanCheck {
        unreadable: arr(check, "read_errors")
            .map(|e| PlanReadError {
                file: s(e, "file"),
                line: e.get("line").and_then(Value::as_u64),
                message: s(e, "message"),
            })
            .collect(),
        unparsed: strs(check.get("unparsed_nodes")),
        red: arr(check, "failures")
            .map(|r| PlanRed {
                rule: s(r, "rule"),
                what: s(r, "message"),
                block: s(r, "scope"),
                fix: s(r, "fix"),
                refs: refs16(r.get("message").and_then(Value::as_str), r.get("refs")),
            })
            .collect(),
        undecidable: arr(check, "unknown")
            .map(|r| PlanUndecidable {
                rule: s(r, "rule"),
                why: s(r, "reason"),
            })
            .collect(),
    };
    out.archived = arr(sl, "archived")
        .map(|a| PlanArchived {
            id: s(a, "id").unwrap_or_default(),
            title: s(a, "title"),
            kind: s(a, "kind"),
        })
        .collect();
    out.audits = arr(sl, "audits").map(|a| audit(a, who)).collect();
    out.labels = nodes
        .iter()
        .map(|c| (id_of(c), s(c, "title")))
        .chain(out.archived.iter().map(|a| (a.id.clone(), a.title.clone())))
        .filter(|(id, _)| !id.is_empty())
        .map(|(id, title)| {
            let l = label(&id, title.as_deref());
            (id, l)
        })
        .collect();
    out
}

/// 一份审图（pb 的 `audits[]`）。给人看的那几句先空着：[`write_said`] 照 `labels` 写，时刻由 [`with_time_texts`] 写。
fn audit(a: &Value, who: WhoPort) -> PlanAudit {
    let by = match a.get("by").and_then(Value::as_str) {
        Some("person") => Some(PlanAuditBy::Person),
        Some("agent") => Some(PlanAuditBy::Agent),
        _ => None,
    };
    let status = match a.get("status").and_then(Value::as_str) {
        Some("done") => Some(PlanAuditStatus::Done),
        Some("failed") => Some(PlanAuditStatus::Failed),
        Some("unfinished") => Some(PlanAuditStatus::Unfinished),
        _ => None,
    };
    let n = |k: &str| a.get(k).and_then(Value::as_u64).map(n32);
    PlanAudit {
        file: s(a, "file").unwrap_or_default(),
        who: match by {
            Some(PlanAuditBy::Agent) => who_at(a, "who", who),
            _ => None,
        },
        by,
        started_at: s(a, "started_at"),
        tree: s(a, "tree"),
        due: n("due").unwrap_or(0),
        nodes: strs(a.get("nodes")),
        status,
        failed_reason: s(a, "failed_reason").filter(|r| !r.is_empty()),
        check: n("check").unwrap_or(0),
        findings: n("findings"),
        said: PlanAuditSaid::default(),
    }
}

/// 一份审图那几句（时刻那一句不动：它由 [`with_time_texts`] 写）。
fn audit_said(a: &PlanAudit, labels: &BTreeMap<String, String>) -> PlanAuditSaid {
    let what = match a.tree.as_deref() {
        Some(super::needs::TOP_BLOCK) => copy_text("plan.audit.whole", &[]),
        Some(id) => {
            let l = labels.get(id).cloned().unwrap_or_else(|| id.to_string());
            copy_text("plan.audit.tree", &[("cell", &l)])
        }
        None => copy_text("plan.audit.changed", &[("n", &a.due.to_string())]),
    };
    let result = match a.status {
        Some(PlanAuditStatus::Done) => copy_text(
            "plan.audit.done",
            &[("n", &a.findings.unwrap_or(0).to_string())],
        ),
        Some(PlanAuditStatus::Failed) => copy_text(
            "plan.audit.failed",
            &[("why", a.failed_reason.as_deref().unwrap_or_default())],
        ),
        Some(PlanAuditStatus::Unfinished) | None => copy_text("plan.audit.unfinished", &[]),
    };
    PlanAuditSaid {
        what,
        by: (a.by == Some(PlanAuditBy::Person)).then(|| copy_text("plan.audit.person", &[])),
        result,
        at: a.said.at.clone(),
    }
}

/// `plan-list` 里一个工作区那一行（标过认可的成品 ⇒ 去掉格，只留切换与计数要的）。
pub fn list_row(v: &PlanRead) -> PlanListWorkspace {
    PlanListWorkspace {
        workspace: v.workspace.clone(),
        repo: v.repo.clone(),
        auto: v.auto,
        rev: v.rev.clone(),
        stale: v.stale.clone(),
        need_count: v.need_count,
        by_session: v.by_session.clone(),
        slices: v.slices.iter().map(slice_summary).collect(),
    }
}

/// 一片的一句话摘要（`plan-list` 那一行要的）：名字 · 领域 · 是不是当前仓库 · 顶层进度 · 要你看的数（标过认可之后才有）· 读不成的那一句。
pub(crate) fn slice_summary(sl: &PlanSlice) -> PlanListSlice {
    PlanListSlice {
        name: sl.name.clone(),
        domain: sl.domain.clone(),
        current: sl.current,
        progress: sl.progress.clone(),
        need_count: sl.need_count,
        error: sl.error.clone(),
        stale: sl.stale.clone(),
        said: sl.said.clone(),
    }
}

/// 时刻写成给人看的字（界面照抄，不换算）：读到的时刻 · 读不成那一份的时刻 · 退回的时刻旁边填 `*Text`，
/// 按 `now_ms` 与看的那一台的时区经 [`crate::common::time::fmt_at`] 写。
pub fn with_time_texts(v: &mut PlanRead, now_ms: i64, tz: &crate::common::time::Tz) {
    let text =
        |ms: i64| crate::common::time::fmt_at(ms.div_euclid(1000), now_ms.div_euclid(1000), tz);
    let ms = |n: u64| i64::try_from(n).unwrap_or(i64::MAX);
    v.read_at_text = Some(text(ms(v.read_at)));
    v.seen = Some(crate::common::last_seen_store::seen_at(
        v.read_at,
        &crate::common::time::TextClock::new(
            u64::try_from(now_ms.div_euclid(1000)).unwrap_or(0),
            tz,
        ),
    ));
    if let Some(st) = &mut v.stale {
        st.since_text = Some(text(ms(st.since)));
    }
    for sl in &mut v.slices {
        if let Some(st) = &mut sl.stale {
            st.since_text = Some(text(ms(st.since)));
        }
        if let Some(r) = &mut sl.returned {
            r.at_text = Some(text(ms(r.at)));
        }
        for a in &mut sl.audits {
            a.said.at = a.started_at.as_deref().map(|iso| {
                crate::common::time::parse_iso8601_ms(iso).map_or_else(|| iso.to_string(), text)
            });
        }
        for c in &mut sl.cells {
            if let Some(r) = &mut c.returned {
                r.at_text = Some(text(ms(r.at)));
            }
        }
    }
    write_said(v);
}

/// 随数据变的那几句（`said`）照这台的文案表写好：两端照摆、不拼。要时刻的字 ⇒ 由 [`with_time_texts`] 写完时刻接着写。
fn write_said(v: &mut PlanRead) {
    let sep = copy_text("kit.text.sep", &[]);
    let reason = |st: &PlanStale| st.said.clone().unwrap_or_default();
    let strip = |first: String, st: &PlanStale| {
        [
            first,
            copy_text(
                "plan.strip.stale",
                &[("time", st.since_text.as_deref().unwrap_or_default())],
            ),
        ]
        .join(&sep)
    };
    let whole = v.stale.as_ref().map(|st| {
        strip(
            copy_text("plan.strip.sliceUnreadable", &[("reason", &reason(st))]),
            st,
        )
    });
    let read_at = v
        .read_at_text
        .as_deref()
        .map(|t| copy_text("plan.overview.readAt", &[("time", t)]));
    let n = |x: u32| x.to_string();
    for sl in &mut v.slices {
        let top = sl.progress.as_ref().map(|p| {
            copy_text(
                "plan.overview.top",
                &[("done", &n(p.done)), ("of", &n(p.of))],
            )
        });
        let root = sl.progress.as_ref().map(|p| {
            copy_text(
                "plan.outline.rootProgress",
                &[("done", &n(p.done)), ("of", &n(p.of))],
            )
        });
        sl.said = PlanSliceSaid {
            strip: whole.clone().or_else(|| {
                sl.stale.as_ref().map(|st| {
                    let first = copy_text("plan.strip.sliceUnreadable", &[("reason", &reason(st))]);
                    strip(first, st)
                })
            }),
            bare: sl
                .error
                .as_deref()
                .map(|e| copy_text("plan.strip.sliceUnreadable", &[("reason", e)])),
            read_at: read_at.clone(),
            top,
            root,
        };
        let title_of = |id: &str| {
            sl.cells
                .iter()
                .find(|c| c.id == id)
                .and_then(|c| c.title.clone())
        };
        let names: Vec<String> = sl
            .blocks
            .iter()
            .map(|b| match b.cells.first() {
                _ if b.id == super::needs::TOP_BLOCK => copy_text("plan.head.top", &[]),
                Some(r) => title_of(r).unwrap_or_else(|| r.clone()),
                None => b.id.clone(),
            })
            .collect();
        for (b, name) in sl.blocks.iter_mut().zip(names) {
            b.said = PlanBlockSaid {
                name,
                inside: b.inside.as_ref().map(|k| {
                    copy_text(
                        "plan.whyShort.inside",
                        &[("done", &n(k.done)), ("of", &n(k.of))],
                    )
                }),
            };
        }
        let said = cell_said(sl, &sep);
        for (c, s) in sl.cells.iter_mut().zip(said) {
            c.said = s;
        }
        let audits: Vec<PlanAuditSaid> = sl
            .audits
            .iter()
            .map(|a| audit_said(a, &sl.labels))
            .collect();
        for (a, s) in sl.audits.iter_mut().zip(audits) {
            a.said = s;
        }
    }
    // 会话那一块的名 ＝ 那一块自己的名（同一句只写一处）。
    let block_name = |slice: &str, block: &str| {
        v.slices
            .iter()
            .find(|s| s.name == slice)
            .and_then(|s| s.blocks.iter().find(|b| b.id == block))
            .map(|b| b.said.name.clone())
    };
    let names: Vec<Option<String>> = v
        .by_session
        .values()
        .map(|b| block_name(&b.slice, &b.block))
        .collect();
    for (b, name) in v.by_session.values_mut().zip(names) {
        let name = name.unwrap_or_else(|| b.block.clone());
        let mark = [
            Some(b.slice.as_str()),
            Some(name.as_str()),
            b.phase.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(&sep);
        b.said = PlanSessionSaid { name, mark };
    }
}

/// 每一格的上面几层（「编号 标题」从顶层往下连）与块根那一句，照 `sl.cells` 的先后。
fn cell_said(sl: &PlanSlice, sep: &str) -> Vec<PlanCellSaid> {
    let roots: std::collections::BTreeSet<&str> = sl
        .blocks
        .iter()
        .filter(|b| b.id != super::needs::TOP_BLOCK)
        .flat_map(|b| b.cells.iter().map(String::as_str))
        .collect();
    let parent: BTreeMap<&str, Option<&str>> = sl
        .cells
        .iter()
        .map(|c| (c.id.as_str(), c.parent.as_deref()))
        .collect();
    let label_of = |id: &str| sl.labels.get(id).cloned().unwrap_or_else(|| id.to_string());
    sl.cells
        .iter()
        .map(|c| {
            let mut up = Vec::new();
            let mut at = c.parent.as_deref();
            while let Some(p) = at.filter(|_| up.len() <= parent.len()) {
                up.push(label_of(p));
                at = parent.get(p).copied().flatten();
            }
            up.reverse();
            PlanCellSaid {
                trail: (!up.is_empty()).then(|| up.join(sep)),
                block_root: roots
                    .contains(c.id.as_str())
                    .then(|| copy_text("plan.cell.blockRoot", &[("block", &label_of(&c.id))])),
            }
        })
        .collect()
}

/// 认不出的 id 写进那一句时截成前六位 ＋ 后四位（整串在复制那一下给）。
fn short_id(id: &str) -> String {
    let c: Vec<char> = id.chars().collect();
    if c.len() <= 12 {
        return id.to_string();
    }
    let head: String = c[..6].iter().collect();
    let tail: String = c[c.len() - 4..].iter().collect();
    format!("{head}\u{2026}{tail}")
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/product_tests.rs"]
mod tests;
