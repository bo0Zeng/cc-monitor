//! **通用投影** `project(成品, 声明)`：出口交的声明（`view`）挑格、去格，核心只这一处做。
//!
//! 声明是数据（`src/doc/ARCHITECTURE.md` §「核心与适配层」）：出口在请求信封里交 `view`，核心照它把成品裁好再装运；
//! 各命令不再自己认「要不要正文」一类的开关。词表今天只两个：
//!
//! - `cells: {<成品>: [<路径>…]}` —— 只要这几格（连同通往它们的那几层）；别的格不发。
//! - `omit:  {<成品>: [<路径>…]}` —— 这几格不要；别的照发。同一件成品两个都给 ⇒ 先挑后去。
//!
//! 路径写法与格目录（[`super::cells_catalog`]）同一种：`a.b` 嵌套 · `a[]` 列表每项 · `a.*` 以 id 为键的表每项 ·
//! `a[k=v]` 列表里按判别格挑的那一种 · `a{k=v}` 非列表的那一种（根上写 `{k=v}`）。**不写挑法 ＝ 每一种都算**：
//! `blocks[type=tool_use].input` 同时管说的与回的那两种记录里的 `blocks`。
//!
//! # 校验：认不出的不静默放过
//!
//! 声明里每条路径都要落在格目录那件成品的某一格上（是某一格本身、或某几格的上一层）；挑法里的 `k=v` 要是目录里见过的那一种。
//! 认不出的词 · 成品 · 路径 ⇒ [`parse_view`] 回错（帧面与 CLI 面都成 `bad_args`，那几处写进复制详情）。
//! 往透传的一团（`type: object`，如工具入参）里面再点格 ⇒ 认不出（目录不往里分格，核心也就不担保里面有什么）。
//!
//! # 去格不是删格
//!
//! 投影只是「这一次不发」：成品的格（与冻结成品的格目录金样）一格不动。去掉的格是**删掉**，不置 `null` / 空 ——
//! 给个空值等于说「这一条没有这一格」，那是假话。

use super::cells_catalog::{cells_of, PRODUCTS};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// 路径里的一步。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    /// `a`
    Key(String),
    /// `*`：以 id 为键的表的每一项。
    All,
    /// `[]`：列表的每一项。
    Each,
    /// `[k=v]`：列表里判别格 `k` 是 `v` 的那几项。
    EachWhere(String, String),
    /// `{k=v}`：这一层判别格 `k` 是 `v` 时才往下走。
    Where(String, String),
}

/// 一条路径（解析过）。
pub(crate) type Path = Vec<Step>;

/// 解析一条路径。写坏了（括号不配 · 挑法里没有 `=` · 空段）⇒ `None`。
pub(crate) fn parse_path(s: &str) -> Option<Path> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    let mut need_seg = true; // 刚过一个 `.`（或在开头）：下面要来一段
    while i < b.len() {
        match b[i] {
            b'.' => {
                if need_seg {
                    return None;
                }
                need_seg = true;
                i += 1;
            }
            b'[' | b'{' => {
                let close = if b[i] == b'[' { ']' } else { '}' };
                let len = s[i + 1..].find(close)?;
                let inner = &s[i + 1..i + 1 + len];
                let list = b[i] == b'[';
                if list && inner.is_empty() {
                    out.push(Step::Each);
                } else {
                    let (k, v) = inner.split_once('=')?;
                    if k.is_empty() || v.is_empty() {
                        return None;
                    }
                    out.push(if list {
                        Step::EachWhere(k.to_string(), v.to_string())
                    } else {
                        Step::Where(k.to_string(), v.to_string())
                    });
                }
                // 列表的挑法要跟在一段名字后面（`[]` 不能打头）；非列表的挑法可以打头（根上那一种）。
                if list && need_seg {
                    return None;
                }
                need_seg = false;
                i += len + 2;
            }
            _ => {
                if !need_seg {
                    return None; // `a[]b`：挑法后面直接接名字
                }
                let end = s[i..].find(['.', '[', '{']).map_or(s.len(), |k| i + k);
                let name = &s[i..end];
                out.push(if name == "*" {
                    Step::All
                } else {
                    Step::Key(name.to_string())
                });
                need_seg = false;
                i = end;
            }
        }
    }
    if need_seg || out.is_empty() {
        return None;
    }
    Some(out)
}

/// 一件成品在目录里的样子：每格的路径（解析过）＋ 目录里见过的挑法 `(k, v)`。
struct Known {
    cells: Vec<Path>,
    picks: BTreeSet<(String, String)>,
}

fn known() -> &'static BTreeMap<&'static str, Known> {
    static K: OnceLock<BTreeMap<&'static str, Known>> = OnceLock::new();
    K.get_or_init(|| {
        PRODUCTS
            .iter()
            .map(|p| {
                let cells: Vec<Path> = cells_of(p)
                    .0
                    .iter()
                    .filter_map(|c| parse_path(&c.path))
                    .collect();
                let picks = cells
                    .iter()
                    .flatten()
                    .filter_map(|s| match s {
                        Step::EachWhere(k, v) | Step::Where(k, v) => Some((k.clone(), v.clone())),
                        _ => None,
                    })
                    .collect();
                (p.name, Known { cells, picks })
            })
            .collect()
    })
}

/// 声明里的这条路径落在目录这一格（`c`）上吗：是它本身，或它的上一层。
/// 声明没写的挑法（`{t=said}`）⇒ 每一种都算，跳过；声明写了而目录这一处没有（每一种都有、提到挑法外面的格）⇒ 也算。
fn covers(p: &[Step], c: &[Step]) -> bool {
    match (p.first(), c.first()) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(Step::Where(k, v)), Some(Step::Where(ck, cv))) if k == ck => {
            v == cv && covers(&p[1..], &c[1..])
        }
        (Some(Step::Where(..)), _) => covers(&p[1..], c),
        (_, Some(Step::Where(..))) => covers(p, &c[1..]),
        (Some(Step::Key(a)), Some(Step::Key(b))) => a == b && covers(&p[1..], &c[1..]),
        (Some(Step::All), Some(Step::All)) => covers(&p[1..], &c[1..]),
        (Some(Step::Each), Some(Step::Each | Step::EachWhere(..))) => covers(&p[1..], &c[1..]),
        (Some(Step::EachWhere(k, v)), Some(Step::EachWhere(ck, cv))) => {
            k == ck && v == cv && covers(&p[1..], &c[1..])
        }
        (Some(Step::EachWhere(..)), Some(Step::Each)) => covers(&p[1..], &c[1..]),
        _ => false,
    }
}

/// 一份声明（解析、校验过）：成品名 → 那几条路径。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct View {
    pub(crate) cells: BTreeMap<String, Vec<Path>>,
    pub(crate) omit: BTreeMap<String, Vec<Path>>,
}

impl View {
    /// 声明里点到的成品（`cells` 与 `omit` 合起来）。
    pub(crate) fn products(&self) -> BTreeSet<&str> {
        self.cells
            .keys()
            .chain(self.omit.keys())
            .map(String::as_str)
            .collect()
    }
}

/// 声明的词表（核心只认这几个）。
pub(crate) const WORDS: &[&str] = &["cells", "omit"];

/// 解析、校验一份声明。缺 / `null` ⇒ `Ok(None)`（全量）。认不出的一处 ⇒ `Err(那几处，一行一处)`。
pub(crate) fn parse_view(v: &Value) -> Result<Option<View>, String> {
    let o = match v {
        Value::Null => return Ok(None),
        Value::Object(o) => o,
        _ => return Err("view: not an object".to_string()),
    };
    let mut bad = Vec::new();
    let mut view = View::default();
    for (word, body) in o {
        let into = match word.as_str() {
            "cells" => &mut view.cells,
            "omit" => &mut view.omit,
            _ => {
                bad.push(format!(
                    "view: unknown word `{word}` (known: {})",
                    WORDS.join(", ")
                ));
                continue;
            }
        };
        let Some(by) = body.as_object() else {
            bad.push(format!("view.{word}: not an object of product → [path]"));
            continue;
        };
        for (product, paths) in by {
            let Some(k) = known().get(product.as_str()) else {
                bad.push(format!("view.{word}: unknown product `{product}`"));
                continue;
            };
            let Some(list) = paths.as_array() else {
                bad.push(format!("view.{word}.{product}: not a list of paths"));
                continue;
            };
            let mut got = Vec::new();
            for p in list {
                let Some(s) = p.as_str() else {
                    bad.push(format!("view.{word}.{product}: a path is not a string"));
                    continue;
                };
                let Some(path) = parse_path(s) else {
                    bad.push(format!(
                        "view.{word}.{product}: unknown cell `{s}` (malformed path)"
                    ));
                    continue;
                };
                let picks_known = path.iter().all(|st| match st {
                    Step::EachWhere(a, b) | Step::Where(a, b) => {
                        k.picks.contains(&(a.clone(), b.clone()))
                    }
                    _ => true,
                });
                let has_key = path.iter().any(|st| !matches!(st, Step::Where(..)));
                if !picks_known || !has_key || !k.cells.iter().any(|c| covers(&path, c)) {
                    bad.push(format!("view.{word}.{product}: unknown cell `{s}`"));
                    continue;
                }
                got.push(path);
            }
            into.insert(product.clone(), got);
        }
    }
    if bad.is_empty() {
        Ok(Some(view))
    } else {
        Err(bad.join("\n"))
    }
}

fn tag_is(v: &Value, k: &str, want: &str) -> bool {
    v.get(k).and_then(Value::as_str) == Some(want)
}

/// 去掉一条路径落到的那几格。回 `true` ＝ 这一整团就是要去的（由上一层删）。
/// 以 `a[]` / `a.*` 收尾 ＝ 去掉 `a` 这一格本身（不留一个空表冒充「没有」）。
fn omit_at(v: &mut Value, p: &[Step]) -> bool {
    let Some((first, rest)) = p.split_first() else {
        return true;
    };
    match first {
        Step::Key(a) => {
            if let Some(o) = v.as_object_mut() {
                if o.get_mut(a).is_some_and(|x| omit_at(x, rest)) {
                    o.remove(a);
                }
            }
            false
        }
        Step::Where(k, want) => tag_is(v, k, want) && omit_at(v, rest),
        Step::All => {
            if rest.is_empty() {
                return true;
            }
            if let Some(o) = v.as_object_mut() {
                o.retain(|_, x| !omit_at(x, rest));
            }
            false
        }
        Step::Each => {
            if rest.is_empty() {
                return true;
            }
            if let Some(a) = v.as_array_mut() {
                a.retain_mut(|x| !omit_at(x, rest));
            }
            false
        }
        Step::EachWhere(k, want) => {
            if let Some(a) = v.as_array_mut() {
                a.retain_mut(|x| !(tag_is(x, k, want) && omit_at(x, rest)));
            }
            false
        }
    }
}

/// 只留下这几条路径落到的格（连同通往它们的那几层）。一格都没落到 ⇒ `None`。
fn keep_at(v: &Value, paths: &[&[Step]]) -> Option<Value> {
    // 这一层的挑法先判掉：判别格对不上的那条路径在这里就不算了。
    let mut here: Vec<&[Step]> = Vec::new();
    for p in paths {
        let mut p: &[Step] = p;
        let mut ok = true;
        while let Some((Step::Where(k, want), rest)) = p.split_first() {
            if !tag_is(v, k, want) {
                ok = false;
                break;
            }
            p = rest;
        }
        if ok {
            here.push(p);
        }
    }
    if here.is_empty() {
        return None;
    }
    if here.iter().any(|p| p.is_empty()) {
        return Some(v.clone());
    }
    match v {
        Value::Object(o) => {
            let mut out = serde_json::Map::new();
            for (key, x) in o {
                let sub: Vec<&[Step]> = here
                    .iter()
                    .filter_map(|p| match &p[0] {
                        Step::Key(a) if a == key => Some(&p[1..]),
                        Step::All => Some(&p[1..]),
                        _ => None,
                    })
                    .collect();
                if sub.is_empty() {
                    continue;
                }
                if let Some(k) = keep_at(x, &sub) {
                    out.insert(key.clone(), k);
                }
            }
            (!out.is_empty()).then_some(Value::Object(out))
        }
        Value::Array(items) => {
            let out: Vec<Value> = items
                .iter()
                .filter_map(|x| {
                    let sub: Vec<&[Step]> = here
                        .iter()
                        .filter_map(|p| match &p[0] {
                            Step::Each => Some(&p[1..]),
                            Step::EachWhere(k, want) if tag_is(x, k, want) => Some(&p[1..]),
                            _ => None,
                        })
                        .collect();
                    if sub.is_empty() {
                        None
                    } else {
                        keep_at(x, &sub)
                    }
                })
                .collect();
            (!out.is_empty()).then_some(Value::Array(out))
        }
        _ => None,
    }
}

/// **投影一件成品**：照声明里这件成品那一份先挑（`cells`）、后去（`omit`）。声明没点这件成品 ⇒ 原样。
pub(crate) fn project(product: &str, v: &mut Value, view: &View) {
    if let Some(paths) = view.cells.get(product) {
        let refs: Vec<&[Step]> = paths.iter().map(Vec::as_slice).collect();
        *v = keep_at(v, &refs).unwrap_or_else(|| Value::Object(Default::default()));
    }
    if let Some(paths) = view.omit.get(product) {
        for p in paths {
            omit_at(v, p);
        }
    }
}

/// 应答里成品住的那几处，逐处投影。`places` ＝ `(在应答里的路径, 成品名)`，按登记次序（外层的成品排在它里面那件之前）。
/// 路径写法同上，只用 `a.b` 与 `a[]`；空串 ＝ 整份应答就是那件成品。
pub(crate) fn project_reply(reply: &mut Value, places: &[(&str, &str)], view: &View) {
    for (at, product) in places {
        if !view.products().contains(product) {
            continue;
        }
        let path = if at.is_empty() {
            Vec::new()
        } else {
            parse_path(at).unwrap_or_default()
        };
        visit(reply, &path, &mut |v| project(product, v, view));
    }
}

/// 走到 `path` 落到的每一团。
pub(crate) fn visit(v: &mut Value, path: &[Step], f: &mut dyn FnMut(&mut Value)) {
    let Some((first, rest)) = path.split_first() else {
        f(v);
        return;
    };
    match first {
        Step::Key(a) => {
            if let Some(x) = v.get_mut(a) {
                visit(x, rest, f);
            }
        }
        Step::Each => {
            if let Some(a) = v.as_array_mut() {
                for x in a {
                    visit(x, rest, f);
                }
            }
        }
        Step::All => {
            if let Some(o) = v.as_object_mut() {
                for x in o.values_mut() {
                    visit(x, rest, f);
                }
            }
        }
        Step::EachWhere(..) | Step::Where(..) => {}
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/project_tests.rs"]
mod tests;
