//! 会话消息里提到的格（帧面 `plan-refs`）：认编号是 pb 的事（`pb dump --refs <仓库>`，标准输入给一段话），本族不认编号、不写正则，
//! 只做三样换方向的事（纯函数，起 pb 的那一下由调用方给）：
//! - 认哪一片（[`at_for`]）：会话是某块的接手 ⇒ 那块的片（成品的 `by_session`，子 agent 记在父会话名下）；
//!   否则工作目录落在哪个读好过的工作区（最深那个）⇒ 仓库目录含它的那片，没有 ⇒ 当前片；都不中 ⇒ 不翻。
//! - 一批正文用 [`SEAM`] 接成一段交 pb，回的码位下标照接缝拆回各段、换成 UTF-16（[`super::product::utf16_at`]）。
//! - 每处写好 `label`：`ids` 里每个的标题照先后接起来（编号本身留在正文里，不重复）。
//!
//! 一段一条记进 [`Memo`]（键 ＝ 工作区 ＋ 片 ＋ 成品摘要 ＋ 正文散列）：一批全中 ⇒ 不起 pb。pb 起不来 · 读不成 · 回了 `error` ⇒ 那一批每段空、不记。
//! 已知边界：某段有没合上的代码围栏 ⇒ 同批排在它后面的几段 pb 当代码、认不出；不在这里数围栏（那是 pb 的认法）。

use cmd_wire::plan::{PlanMsgRef, PlanMsgRefs, PlanRead};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// 一批正文之间的接缝（空一行：pb 的认法里编号不跨行，接缝不造出、也不吞掉一处）。
pub(crate) const SEAM: &str = "\n\n";

/// 照哪一片认：工作区根 · 片名 · 那一份成品的摘要（进缓存键：图变了旧的不作数）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct At {
    pub(crate) workspace: String,
    pub(crate) slice: String,
    pub(crate) rev: String,
}

/// 会话 `sid`（工作目录 `cwd`）该照哪一片认。`docs` 是读好过的各工作区成品。
pub(crate) fn at_for(docs: &[PlanRead], sid: &str, cwd: Option<&str>) -> Option<At> {
    if let Some((d, b)) = docs
        .iter()
        .find_map(|d| d.by_session.get(sid).map(|b| (d, b)))
    {
        return Some(At {
            workspace: d.workspace.clone(),
            slice: b.slice.clone(),
            rev: d.rev.clone(),
        });
    }
    let cwd = cwd?.trim_end_matches('/');
    let doc = docs
        .iter()
        .filter(|d| !d.workspace.is_empty() && within(cwd, &d.workspace))
        .max_by_key(|d| d.workspace.len())?;
    let sl = doc
        .slices
        .iter()
        .find(|sl| super::owners::repo_dir(&doc.workspace, sl).is_some_and(|r| within(cwd, &r)))
        .or_else(|| doc.slices.iter().find(|sl| sl.current))?;
    Some(At {
        workspace: doc.workspace.clone(),
        slice: sl.name.clone(),
        rev: doc.rev.clone(),
    })
}

/// `dir` 是不是 `root` 本身或在它底下（按路径段，`/` 分隔）。
fn within(dir: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    dir.strip_prefix(root)
        .is_some_and(|r| r.is_empty() || r.starts_with('/'))
}

/// 几段接成一段（[`SEAM`] 隔开），连同每段在接好的那一段里从第几个码位起。
pub(crate) fn joined(texts: &[&str]) -> (String, Vec<u64>) {
    let mut out = String::new();
    let mut starts = Vec::with_capacity(texts.len());
    let mut at = 0u64;
    for (i, t) in texts.iter().enumerate() {
        if i > 0 {
            out.push_str(SEAM);
            at += SEAM.chars().count() as u64;
        }
        starts.push(at);
        out.push_str(t);
        at += t.chars().count() as u64;
    }
    (out, starts)
}

/// pb 回的那一份（`refs` 按接好的那一段的码位 · `nodes` 各自的标题）⇒ 每段各自的几处（UTF-16 下标 ＋ 标签）。
/// 跨了接缝的 · 越界的 · 一个标题都没有的不算。
pub(crate) fn split(reply: &Value, texts: &[&str], starts: &[u64]) -> Vec<Vec<PlanMsgRef>> {
    let mut out: Vec<Vec<PlanMsgRef>> = vec![Vec::new(); texts.len()];
    let nodes = reply.get("nodes");
    let lens: Vec<u64> = texts.iter().map(|t| t.chars().count() as u64).collect();
    for r in reply
        .get("refs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let num = |k: &str| r.get(k).and_then(Value::as_u64);
        let (Some(start), Some(end)) = (num("start"), num("end")) else {
            continue;
        };
        let Some(i) = (0..texts.len()).find(|&i| starts[i] <= start && end <= starts[i] + lens[i])
        else {
            continue;
        };
        if start >= end {
            continue;
        }
        let ids: Vec<String> = r
            .get("ids")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let label = label(&ids, nodes);
        if label.is_empty() {
            continue;
        }
        let at = |cp: u64| {
            u32::try_from(super::product::utf16_at(texts[i], cp - starts[i])).unwrap_or(u32::MAX)
        };
        out[i].push(PlanMsgRef {
            start: at(start),
            end: at(end),
            ids,
            label,
        });
    }
    out
}

/// 跟在一处后面的那串字：每个 id 的标题（pb 回的 `nodes.<编号>.title`）照先后接；没标题的那个不写。
fn label(ids: &[String], nodes: Option<&Value>) -> String {
    let titles: Vec<&str> = ids
        .iter()
        .filter_map(|id| {
            nodes
                .and_then(|n| n.get(id))
                .and_then(|n| n.get("title"))
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
        })
        .collect();
    titles.join(&copy_core::copy_text("plan.page.listSep", &[]))
}

/// 工作区 · 片 · 成品摘要 · 正文的长度与两份 64 位散列（标准库的，种子固定；撞了只会给错一处标题，图一变就作废）。
type Key = (String, String, String, usize, u64, u64);

/// 认过的每一段（条数到顶 ⇒ 整张清掉重来）。
pub(crate) struct Memo {
    cap: usize,
    seen: Mutex<HashMap<Key, Vec<PlanMsgRef>>>,
}

impl Memo {
    pub(crate) fn new(cap: usize) -> Memo {
        Memo {
            cap,
            seen: Mutex::new(HashMap::new()),
        }
    }

    fn key(at: &At, text: &str) -> Key {
        use std::hash::{Hash as _, Hasher as _};
        let digest = |salt: u8| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            salt.hash(&mut h);
            text.hash(&mut h);
            h.finish()
        };
        (
            at.workspace.clone(),
            at.slice.clone(),
            at.rev.clone(),
            text.len(),
            digest(0),
            digest(1),
        )
    }
}

/// 进程里那一份。
pub(crate) fn memo() -> &'static Memo {
    static M: std::sync::OnceLock<Memo> = std::sync::OnceLock::new();
    M.get_or_init(|| Memo::new(4096))
}

/// 一批正文照 `at` 那一片认：缓存里有的照用，余下的接成一段只问一次 `ask`（交接好的那一段，回 pb 那一份；不成 ⇒ `None`）。
/// 空段不问。
pub(crate) fn refs_in(
    memo: &Memo,
    at: &At,
    texts: &[String],
    ask: &dyn Fn(&str) -> Option<Value>,
) -> Vec<Vec<PlanMsgRef>> {
    let keys: Vec<Key> = texts.iter().map(|t| Memo::key(at, t)).collect();
    let mut out: Vec<Option<Vec<PlanMsgRef>>> = {
        let seen = memo.seen.lock().unwrap_or_else(|e| e.into_inner());
        texts
            .iter()
            .zip(&keys)
            .map(|(t, k)| {
                if t.is_empty() {
                    Some(Vec::new())
                } else {
                    seen.get(k).cloned()
                }
            })
            .collect()
    };
    let miss: Vec<usize> = (0..texts.len()).filter(|&i| out[i].is_none()).collect();
    if !miss.is_empty() {
        let parts: Vec<&str> = miss.iter().map(|&i| texts[i].as_str()).collect();
        let (one, starts) = joined(&parts);
        let got = ask(&one).filter(|r| r.get("error").is_none_or(Value::is_null));
        if let Some(reply) = got {
            let each = split(&reply, &parts, &starts);
            let mut seen = memo.seen.lock().unwrap_or_else(|e| e.into_inner());
            if seen.len() + each.len() > memo.cap {
                seen.clear();
            }
            for (&i, refs) in miss.iter().zip(each) {
                seen.insert(keys[i].clone(), refs.clone());
                out[i] = Some(refs);
            }
        }
    }
    out.into_iter().map(Option::unwrap_or_default).collect()
}

/// `plan-refs` 的本体：认片 ⇒ 认这一批；认不出片 ⇒ 两样 `null`、每段空，不起 pb。
/// 回复正文是树（`trees`）：投成一段字（[`md_core::ref_text`]）跟人说的话一起认，认出的几处插回树上（[`md_core::with_refs`]）。
pub(crate) fn answer(
    docs: &[PlanRead],
    sid: &str,
    cwd: Option<&str>,
    texts: &[String],
    trees: &[md_core::Doc],
    memo: &Memo,
    ask: &dyn Fn(&At, &str) -> Option<Value>,
) -> PlanMsgRefs {
    let Some(at) = at_for(docs, sid, cwd) else {
        return PlanMsgRefs {
            workspace: None,
            slice: None,
            refs: vec![Vec::new(); texts.len()],
            docs: vec![None; trees.len()],
        };
    };
    let all: Vec<String> = texts
        .iter()
        .cloned()
        .chain(trees.iter().map(md_core::ref_text))
        .collect();
    let mut refs = refs_in(memo, &at, &all, &|one| ask(&at, one));
    let of_trees = refs.split_off(texts.len());
    PlanMsgRefs {
        refs,
        docs: trees
            .iter()
            .zip(of_trees)
            .map(|(doc, found)| {
                let at: Vec<md_core::RefAt> = found
                    .into_iter()
                    .map(|r| md_core::RefAt {
                        start: r.start as usize,
                        end: r.end as usize,
                        ids: r.ids,
                        label: r.label,
                    })
                    .collect();
                (!at.is_empty()).then(|| md_core::with_refs(doc, &at))
            })
            .collect(),
        workspace: Some(at.workspace),
        slice: Some(at.slice),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/refs_tests.rs"]
mod tests;
