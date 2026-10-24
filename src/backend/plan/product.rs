//! 一份 `pb dump`（形状 4，pb 0.3.8）⇒ 界面直接排版的成品（[`super::wire`] 里的类型）。纯函数：不读盘、不起进程。
//!
//! 换个方向排、换个名字，再把界面要的数与先后算好（界面照画、不数不排）：
//! - 每格「指着它的」边：把全片的边倒过来排；每份文件还挂在哪几格：把全片的文件声明倒过来查；
//! - 块的接手 · 每格在长它的 · 谁的 update 收得了它（`signer`）· 每条签收的「由」· 记原话的人：经 [`super::WhoPort`] 对到会话；
//! - 顶层进度：顶层格按 pb 给的状态数一数；块根格的直接子做完几格 · 没做完的格数 · 签收新的在上 · 行尾原因短词，也在这里数好、排好、写好；
//!   凭据类的格（`evidence`）不算活，一个分母都不进；
//! - 「编号 标题」：每片一张 `labels`（编号 ⇒「编号 标题」），凡出现编号界面照它写，界面不拼；
//! - 话里提到的格（`refs`）：pb 的 `start · end` 按 Unicode 码位，换成界面切字用的 UTF-16 下标（[`utf16_at`]）。
//!
//! 整份 dump 里不带 agent 看到的那一段（0.3.8 起点到一格再 `pb dump --cell` 取，见帧面 `plan-cell-view`）。
//! 只读 pb 认得的那几个键，造的是线上类型：pb 那边多出来的键不带过来（形状版本没变就不该多，多了说明该升形状版本了）。

use super::wire::{
    PlanArchived, PlanBlock, PlanCell, PlanCheck, PlanDomain, PlanEdgeWords, PlanEdges, PlanFile,
    PlanFileState, PlanKids, PlanKind, PlanListSlice, PlanListWorkspace, PlanPendingSet, PlanPhase,
    PlanProgress, PlanRead, PlanRecord, PlanRed, PlanRef, PlanRefs, PlanSessionBlock, PlanSetsTo,
    PlanSign, PlanSignAt, PlanSlice, PlanStatusCode, PlanUnbuilt, PlanUndecidable, PlanUnowned,
    PlanUnsplit, PlanWho, PlanWhyCode, PlanWhyShort,
};
use super::{Live, WhoPort, Whose};
use crate::wire::SessionFaceExt as _;
use copy_core::copy_text;
use serde_json::Value;
use std::collections::BTreeMap;

/// 四种边（pb 的边名）。
pub(crate) const EDGES: [&str; 4] = ["to", "with", "after", "replaces"];

/// pb 给的状态里「做完了」「不做了」那两个值（进度要数它们；其余都算没做完）。
pub(crate) const DONE: &str = "做完了";
pub(crate) const DROPPED: &str = "不做了";

/// pb 的三种状态 ⇒ 线上的码（`statusCode`）；pb 给了别的字 ⇒ `None`（界面照出原话、当没做完画）。凭据类的格看 `evidence`（码 `evidence`），不认它的状态字。
pub(crate) fn status_code(s: Option<&str>) -> Option<PlanStatusCode> {
    match s? {
        DONE => Some(PlanStatusCode::Done),
        "没做完" => Some(PlanStatusCode::Open),
        DROPPED => Some(PlanStatusCode::Dropped),
        _ => None,
    }
}

/// 没做完的原因 ⇒ `whyCode`：`没签` ⇒ `nosign` · `等上一级收下` ⇒ `upper` ·
/// `里面 d/m 做完了` ⇒ `inside {done, of}`；认不出 ⇒ `None`（界面照出原话）。拆 pb 原话只在这一处。
pub(crate) fn why_code(s: Option<&str>) -> Option<PlanWhyCode> {
    match s? {
        "没签" => Some(PlanWhyCode::Nosign),
        "等上一级收下" => Some(PlanWhyCode::Upper),
        s => s
            .strip_prefix("里面 ")
            .and_then(|r| r.strip_suffix(" 做完了"))
            .and_then(|r| r.split_once('/'))
            .and_then(|(d, m)| {
                Some(PlanWhyCode::Inside {
                    done: d.trim().parse().ok()?,
                    of: m.trim().parse().ok()?,
                })
            }),
    }
}

/// 行尾那一个原因短词（`whyShort`）：没签 · d/m · 等收下（`wait`：等收下用正文色）；认不出 ⇒ pb 原话；没有原因 ⇒ `None`。
fn why_short(s: Option<&str>) -> Option<PlanWhyShort> {
    let (text, wait) = match why_code(s) {
        Some(PlanWhyCode::Nosign) => (copy_text("plan.whyShort.nosign", &[]), false),
        Some(PlanWhyCode::Upper) => (copy_text("plan.whyShort.upper", &[]), true),
        Some(PlanWhyCode::Inside { done, of }) => (
            copy_text(
                "plan.whyShort.inside",
                &[("done", &done.to_string()), ("of", &of.to_string())],
            ),
            false,
        ),
        None => (s?.to_string(), false),
    };
    Some(PlanWhyShort { text, wait })
}

/// 对账的四种（在 · 缺 · 空 · 坏）⇒ `stateCode`；认不出 ⇒ `None`。
pub(crate) fn file_code(s: Option<&str>) -> Option<PlanFileState> {
    match s? {
        "在" => Some(PlanFileState::Ok),
        "缺" => Some(PlanFileState::Missing),
        "空" => Some(PlanFileState::Empty),
        "坏" => Some(PlanFileState::Broken),
        _ => None,
    }
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

fn is_evidence(c: &Value) -> bool {
    c.get("evidence").and_then(Value::as_bool) == Some(true)
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
                tmux: None,
            },
            None => crate::wire::Life::Ended,
        })
    };
    let (kind, sid, needs, face) = match who(id) {
        Whose::Session { sid, live } => ("session", Some(sid), needs_of(&live), face_of(&live)),
        Whose::Subagent { parent, live } => {
            ("subagent", Some(parent), needs_of(&live), face_of(&live))
        }
        Whose::Unknown => (
            "unknown",
            None,
            None,
            crate::wire::SessionFace::of(crate::wire::Life::Unclear),
        ),
    };
    Some(PlanWho {
        id: id.to_string(),
        kind: kind.to_string(),
        sid,
        needs,
        face,
    })
}

fn who_at(v: &Value, k: &str, who: WhoPort) -> Option<PlanWho> {
    who_of(v.get(k).and_then(Value::as_str), who)
}

/// 领域表里的类（一片的 · 能选的那几档的，同一形）。
fn kinds_of(v: &Value) -> Vec<PlanKind> {
    arr(v, "kinds")
        .map(|k| {
            let w = |e: &str| k.get("edge_words").and_then(|w| s(w, e));
            PlanKind {
                name: s(k, "name"),
                edge_words: PlanEdgeWords {
                    to: w("to"),
                    with_: w("with"),
                    after: w("after"),
                    replaces: w("replaces"),
                },
                evidence: is_evidence(k),
                values: strs(k.get("values")),
            }
        })
        .collect()
}

/// 阶段表（一片的 · 能选的那几档的，同一形）。
fn phases_of(v: &Value) -> Vec<PlanPhase> {
    arr(v, "phases")
        .map(|p| PlanPhase {
            name: s(p, "name"),
            does: s(p, "does"),
            marks: strs(p.get("marks")),
        })
        .collect()
}

/// 整份加工。`rev` 是这一份输出的摘要、`read_at` 是读到的时刻（[`super::book`] 给）；`repo` 是工作区 `.env` 里的当前片（`current` 那一格照它）。
/// 要你看 · 认可 · 退回 · 时刻的字此刻都空着：本子（[`super::book`]）· 认可记录（[`super::review`]）· [`with_time_texts`] 再填。
pub(crate) fn make(doc: &Value, who: WhoPort, rev: String, read_at: u64) -> PlanRead {
    let repo = doc.get("repo").and_then(Value::as_str);
    let slices: Vec<PlanSlice> = arr(doc, "slices").map(|sl| slice(sl, repo, who)).collect();
    let by_session = by_session(&slices);
    let domains = arr(doc, "domains")
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
        .collect();
    PlanRead {
        pb: s(doc, "pb"),
        workspace: s(doc, "workspace").unwrap_or_default(),
        repo: s(doc, "repo"),
        auto: flag(doc, "auto"),
        domains,
        slices,
        by_session,
        rev,
        read_at,
        read_at_text: None,
        stale: None,
        need_count: 0,
    }
}

/// 会话 ⇒ 它接手的那一块（会话头那一枚标 · 「这个会话是哪一片哪一块的接手」）：
/// `{片, 块, 块根格（编号）, 块根格标题（顶块 ⇒ null）, 顶块不顶块, 阶段, 站在哪一格（编号 ＋ 标题）, via: session|subagent}`。
/// 子 agent 接的块记在父会话名下；一个会话名下几块 ⇒ 自己接的压过子 agent 替它接的，同档取片与块的先后里第一块。
/// 对不上会话的接手不进表。
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

/// 一片里记下的原话：谁记的照 pb 给的那一格对到会话（人敲的 pb 写它自己的那个字，对不上 ⇒ `unknown`，界面照出那个字）。
fn records_of(sl: &Value, who: WhoPort) -> Vec<PlanRecord> {
    arr(sl, "records")
        .map(|r| PlanRecord {
            n: r.get("n").and_then(Value::as_u64),
            summary: s(r, "summary"),
            at: s(r, "at"),
            at_text: None,
            by: who_at(r, "by", who),
            quotes: strs(r.get("quotes")),
        })
        .collect()
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

/// 一片。带 `error` 的片只出名字、领域、仓库位置、记下的原话与那一句（pb 那时给不出别的），`bare` 说「只有头几格」。
pub(crate) fn slice(sl: &Value, repo: Option<&str>, who: WhoPort) -> PlanSlice {
    let name = s(sl, "name").unwrap_or_default();
    let mut out = PlanSlice {
        current: repo == Some(name.as_str()),
        name,
        domain: s(sl, "domain"),
        // 仓库相对工作区根在哪（`pb repo <路径>` 登记的；没登记 pb 也给片名）：照 pb 给的，这里不补。
        path: s(sl, "path"),
        // 只写文档：仓库还没接过来（人 `pb take` 之后才守）。
        docs_only: flag(sl, "docs_only"),
        records: records_of(sl, who),
        error: None,
        stale: None,
        bare: false,
        kinds: Vec::new(),
        phases: Vec::new(),
        done: false,
        top: Vec::new(),
        progress: None,
        open_cells: 0,
        recent_signs: Vec::new(),
        blocks: Vec::new(),
        unbuilt_blocks: Vec::new(),
        unowned: PlanUnowned {
            paths: None,
            why: None,
        },
        unsplit: Vec::new(),
        pending_sets: Vec::new(),
        check: PlanCheck {
            unreadable: Vec::new(),
            red: Vec::new(),
            undecidable: Vec::new(),
        },
        labels: BTreeMap::new(),
        cells: Vec::new(),
        archived: Vec::new(),
        needs: Vec::new(),
        need_count: 0,
        returned: None,
    };
    if let Some(e) = sl.get("error").filter(|e| !e.is_null()) {
        // 读不成又没读好过：形状照常（空的）。
        out.error = Some(said(e));
        out.bare = true;
        return out;
    }
    let cells_in: Vec<&Value> = arr(sl, "cells").collect();
    let id_of = |c: &Value| {
        c.get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };

    // 倒过来的两张索引：谁的边指着它 · 同一份文件还挂在哪几格。
    let mut pointed: BTreeMap<String, BTreeMap<&str, Vec<String>>> = BTreeMap::new();
    let mut by_path: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in &cells_in {
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

    out.cells = cells_in
        .iter()
        .map(|c| {
            let id = id_of(c);
            let back = pointed.get(&id);
            let files = arr(c, "files")
                .map(|f| {
                    let path = f.get("path").and_then(Value::as_str).unwrap_or_default();
                    let state = f.get("state").and_then(Value::as_str);
                    PlanFile {
                        path: path.to_string(),
                        state: state.map(str::to_string),
                        state_code: file_code(state),
                        note: s(f, "note"),
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
            let signs = arr(c, "signs")
                .map(|g| PlanSign {
                    at: s(g, "at"),
                    at_text: None,
                    by: who_at(g, "by", who),
                    value: s(g, "value"),
                    via: s(g, "via"),
                    done: flag(g, "done"),
                    reason: s(g, "reason"),
                    refs: refs16(g.get("reason").and_then(Value::as_str), g.get("refs")),
                })
                .collect();
            let sets = arr(c, "sets")
                .map(|x| PlanSetsTo {
                    target: s(x, "target"),
                    value: s(x, "value"),
                })
                .collect();
            let refs = c.get("refs");
            let status = c.get("status").and_then(Value::as_str);
            let why = c.get("why").and_then(Value::as_str);
            let title = c.get("title").and_then(Value::as_str);
            let body = c.get("body").and_then(Value::as_str);
            PlanCell {
                edges: edges_of(|k| strs(c.get("edges").and_then(|e| e.get(k)))),
                pointed_by: edges_of(|k| back.and_then(|b| b.get(k)).cloned().unwrap_or_default()),
                files,
                evidence: is_evidence(c),
                status: status.map(str::to_string),
                status_code: if is_evidence(c) {
                    Some(PlanStatusCode::Evidence)
                } else {
                    status_code(status)
                },
                why: why.map(str::to_string),
                why_code: why_code(why),
                why_short: why_short(why),
                signs,
                counts: s(c, "counts"),
                sets,
                block: s(c, "block"),
                owner: who_at(c, "owner", who),
                // 谁的 update 执行得了收下它的 sets（凭据类的 · 有子的格 ⇒ pb 给 null）。
                signer: who_at(c, "signer", who),
                covers: strs(c.get("covers")),
                covered_by: strs(c.get("covered_by")),
                refs: PlanRefs {
                    title: refs16(title, refs.and_then(|r| r.get("title"))),
                    body: refs16(body, refs.and_then(|r| r.get("body"))),
                },
                returned: None,
                kind: s(c, "kind"),
                parent: s(c, "parent"),
                children: strs(c.get("children")),
                title: title.map(str::to_string),
                body: body.map(str::to_string),
                id,
            }
        })
        .collect();

    out.top = strs(sl.get("top"));
    let find = |id: &str| out.cells.iter().find(|c| c.id == id);
    let mut progress = PlanProgress {
        done: 0,
        open: 0,
        dropped: 0,
    };
    for t in &out.top {
        let c = find(t);
        if c.is_some_and(|c| c.evidence) {
            continue;
        }
        match c.and_then(|c| c.status.as_deref()) {
            Some(DONE) => progress.done += 1,
            Some(DROPPED) => progress.dropped += 1,
            _ => progress.open += 1,
        }
    }
    let blocks = arr(sl, "blocks")
        .map(|b| {
            let cells = strs(b.get("cells"));
            let inside = cells
                .first()
                .and_then(|r| find(r))
                .map(|root| kids_done(&out.cells, root));
            PlanBlock {
                id: s(b, "id").unwrap_or_default(),
                cells,
                inside,
                dir: s(b, "dir"),
                owner: who_at(b, "owner", who),
                phase: s(b, "phase"),
                at: s(b, "at"),
                row: flag(b, "row"),
            }
        })
        .collect();
    out.progress = Some(progress);
    out.blocks = blocks;

    // 没做完的格（不是做完了、也不是不做了；凭据类的不算活）。
    out.open_cells = n32(out
        .cells
        .iter()
        .filter(|c| !c.evidence && !matches!(c.status.as_deref(), Some(DONE | DROPPED)))
        .count() as u64);
    // 全片的签收，新的在上；读不出时刻的排后（同为读不出的照原来的先后）。
    let mut recent: Vec<(Option<i64>, PlanSignAt)> = out
        .cells
        .iter()
        .flat_map(|c| {
            c.signs.iter().enumerate().map(|(i, g)| {
                let ms =
                    g.at.as_deref()
                        .and_then(crate::common::time::parse_iso8601_ms);
                (
                    ms,
                    PlanSignAt {
                        cell: c.id.clone(),
                        i: n32(i as u64),
                    },
                )
            })
        })
        .collect();
    recent.sort_by(|a, b| b.0.cmp(&a.0));
    out.recent_signs = recent.into_iter().map(|(_, at)| at).collect();

    out.unbuilt_blocks = arr(sl, "unbuilt_blocks")
        .map(|b| PlanUnbuilt {
            id: s(b, "id"),
            cells: strs(b.get("cells")),
        })
        .collect();
    let unowned = sl.get("unowned");
    out.unowned = PlanUnowned {
        paths: unowned
            .and_then(|u| u.get("paths"))
            .filter(|p| p.is_array())
            .map(|p| strs(Some(p))),
        why: unowned.and_then(|u| s(u, "why")),
    };
    out.unsplit = arr(sl, "unsplit")
        .map(|u| PlanUnsplit {
            id: s(u, "id"),
            path: s(u, "path"),
        })
        .collect();
    out.pending_sets = arr(sl, "pending_sets")
        .map(|p| PlanPendingSet {
            source: s(p, "source"),
            target: s(p, "target"),
            value: s(p, "value"),
            why_not: s(p, "why_not"),
        })
        .collect();
    let check = sl.get("check");
    out.check = PlanCheck {
        unreadable: check
            .and_then(|c| c.get("unreadable"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        red: check
            .into_iter()
            .flat_map(|c| arr(c, "red"))
            .map(|r| PlanRed {
                rule: s(r, "rule"),
                what: s(r, "what"),
                block: s(r, "block"),
                fix: s(r, "fix"),
                refs: refs16(r.get("what").and_then(Value::as_str), r.get("refs")),
            })
            .collect(),
        undecidable: check
            .into_iter()
            .flat_map(|c| arr(c, "undecidable"))
            .map(|r| PlanUndecidable {
                rule: s(r, "rule"),
                why: s(r, "why"),
            })
            .collect(),
    };
    out.archived = arr(sl, "archived")
        .map(|a| PlanArchived {
            id: s(a, "id").unwrap_or_default(),
            title: s(a, "title"),
            kind: s(a, "kind"),
            replaced_by: s(a, "replaced_by"),
        })
        .collect();
    out.labels = out
        .cells
        .iter()
        .map(|c| (c.id.as_str(), c.title.as_deref()))
        .chain(
            out.archived
                .iter()
                .map(|a| (a.id.as_str(), a.title.as_deref())),
        )
        .filter(|(id, _)| !id.is_empty())
        .map(|(id, title)| (id.to_string(), label(id, title)))
        .collect();
    out.kinds = kinds_of(sl);
    out.phases = phases_of(sl);
    out.done = flag(sl, "done");
    out
}

/// 一格的直接子里做完了几格 / 一共几格（不做了的 · 凭据类的不算）。
fn kids_done(cells: &[PlanCell], root: &PlanCell) -> PlanKids {
    let mut k = PlanKids { done: 0, of: 0 };
    for id in &root.children {
        let Some(c) = cells.iter().find(|c| c.id == *id) else {
            continue;
        };
        if c.evidence {
            continue;
        }
        match c.status.as_deref() {
            Some(DROPPED) => {}
            Some(DONE) => {
                k.done += 1;
                k.of += 1;
            }
            _ => k.of += 1,
        }
    }
    k
}

/// `plan-list` 里一个工作区那一行（标过认可的成品 ⇒ 去掉格与签收，只留切换与计数要的）。
pub(crate) fn list_row(v: &PlanRead) -> PlanListWorkspace {
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

/// 一片的一句话摘要（`plan-list` 那一行要的）：片名 · 领域 · 当前片没有 · 顶层进度 · 要你看的数（标过认可之后才有）· 读不成的那一句。
pub(crate) fn slice_summary(sl: &PlanSlice) -> PlanListSlice {
    PlanListSlice {
        name: sl.name.clone(),
        domain: sl.domain.clone(),
        current: sl.current,
        progress: sl.progress.clone(),
        need_count: sl.need_count,
        error: sl.error.clone(),
        stale: sl.stale.clone(),
    }
}

/// 时刻写成给人看的字（界面照抄，不换算）：读到的时刻 · 读不成那一份的时刻 · 签收 · 记原话 · 退回的时刻旁边填 `*Text`，
/// 按 `now_ms` 与看的那一台的时区经 [`crate::common::time::fmt_at`] 写。签收 · 记原话的时刻是 ISO 串，读不成 ⇒ 不填。
pub(crate) fn with_time_texts(v: &mut PlanRead, now_ms: i64, tz: &crate::Tz) {
    let text =
        |ms: i64| crate::common::time::fmt_at(ms.div_euclid(1000), now_ms.div_euclid(1000), tz);
    let ms = |n: u64| i64::try_from(n).unwrap_or(i64::MAX);
    let iso = |at: &Option<String>| {
        at.as_deref()
            .and_then(crate::common::time::parse_iso8601_ms)
            .map(text)
    };
    v.read_at_text = Some(text(ms(v.read_at)));
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
        for r in &mut sl.records {
            r.at_text = iso(&r.at);
        }
        for c in &mut sl.cells {
            if let Some(r) = &mut c.returned {
                r.at_text = Some(text(ms(r.at)));
            }
            for g in &mut c.signs {
                g.at_text = iso(&g.at);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/product_tests.rs"]
mod tests;
