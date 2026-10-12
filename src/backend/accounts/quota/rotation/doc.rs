//! 副本（E15）：会话打开一份配置 ＝ 打开它的副本。改一格写 `copy`；跟原样相等就丢；保存（带 `ifRev`）· 另存为 · 还原 · 载入新版本 ·
//! 换配置先问 · 别处存了新版（没改过的跟上、改过的出 `newer`）。配置编辑器（设置窗）用同一套校验与比对。
//!
//! 参数值照这份策略的声明逐格补齐、校验（[`filled`] · [`validate`]）；两份内容的不同写成 `diff` 一条一格（[`diff`]）。

use super::{
    cell_err, name_key, AcctRow, Book, CellError, Config, ConfigBody, OpenConfig, Params, Who,
    NAME_MAX,
};
use crate::accounts::quota::strategy::form::{self, Decl, Type};
use crate::accounts::quota::strategy::{Lang, Strategy};
use copy_core::copy_text;
use serde::Serialize;
use serde_json::{json, Value};

/// 两份内容的一格不同：哪一行（整份 ⇒ 缺；`@start` · `@rest` · `@init` · 号 id）· 哪一格 · 原值 · 新值 · 写好的那一句。
/// 格名之外另有三种：`@strategy`（换了策略）· `@name`（改了名）· `@join`（这一行参与 / 不参与，值是真假）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ConfigDiff"
    )
)]
pub struct Diff {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub acct: Option<String>,
    pub k: String,
    #[cfg_attr(test, ts(type = "unknown"))]
    pub from: Value,
    #[cfg_attr(test, ts(type = "unknown"))]
    pub to: Value,
    pub text: String,
}

/// 新加的号缺省那一行在命令里的写法。
pub(crate) const INIT_ROW: &str = "@init";

/// ★ 照这份策略的声明补齐每一格（缺的格：每号先按 `acctInit` 再按 `init`；读不懂的格按 `init`；没声明的格丢掉）。
pub(crate) fn filled(body: &ConfigBody, s: &Strategy) -> ConfigBody {
    let mut errors = Vec::new();
    fill_into(body, s, &mut errors)
}

fn fill_into(
    body: &ConfigBody,
    s: &Strategy,
    errors: &mut Vec<(String, form::Code)>,
) -> ConfigBody {
    let acct_init = form::fill_checked(&s.acct_params, &body.acct_init, None, "acctInit", errors);
    let accounts = body
        .accounts
        .iter()
        .enumerate()
        .map(|(i, r)| AcctRow {
            who: r.who.clone(),
            params: form::fill_checked(
                &s.acct_params,
                &r.params,
                Some(&acct_init),
                &format!("accounts[{i}]"),
                errors,
            ),
        })
        .collect();
    ConfigBody {
        name: body.name.clone(),
        strategy: body.strategy.clone(),
        accounts,
        params: form::fill_checked(&s.params, &body.params, None, "params", errors),
        acct_init,
        parked: body.parked.clone(),
        tz: body.tz.clone(),
        tz_unknown: body.tz_unknown,
    }
}

/// 名字那一格的错（空 · 太长 · 本机重名；`id` ＝ 自己那份，不算重名）。
pub(crate) fn name_errors(book: &Book, id: Option<&str>, name: &str) -> Vec<CellError> {
    let n = name.trim();
    if n.is_empty() {
        return vec![cell_err("name".into(), "empty")];
    }
    if n.chars().count() > NAME_MAX {
        return vec![cell_err("name".into(), "tooLong")];
    }
    let dup = book
        .configs
        .iter()
        .any(|(cid, c)| Some(cid.as_str()) != id && name_key(&c.body.name) == name_key(n));
    if dup {
        return vec![cell_err("name".into(), "dup")];
    }
    Vec::new()
}

/// ★ 一份内容整份校验：策略要在、读得懂；号那几行谁是谁（起会话的号 · 其余订阅号各至多一行，点名的号过闸、不重复）；
/// 每一格照声明（错的格逐格回）。对 ⇒ 补齐后的那一份。
pub(crate) fn validate(
    body: &ConfigBody,
    s: Option<&Strategy>,
    account_ok: &dyn Fn(&str) -> bool,
) -> Result<ConfigBody, Vec<CellError>> {
    let mut errs: Vec<CellError> = Vec::new();
    let Some(s) = s.filter(|s| s.usable()) else {
        return Err(vec![cell_err("strategy".into(), "strategy")]);
    };
    let mut seen: Vec<&Who> = Vec::new();
    for (i, r) in body.accounts.iter().enumerate() {
        let bad = match &r.who {
            Who::Id(a) => !account_ok(a),
            _ => false,
        };
        if bad || seen.contains(&&r.who) {
            errs.push(cell_err(format!("accounts[{i}]"), "account"));
        }
        seen.push(&r.who);
    }
    let mut cells = Vec::new();
    let out = fill_into(body, s, &mut cells);
    for (cell, form::Code(code)) in cells {
        errs.push(cell_err(cell, code));
    }
    if errs.is_empty() {
        Ok(out)
    } else {
        Err(errs)
    }
}

/// 一格的值写给人看（照它的声明）。
pub(crate) fn value_text(d: Option<&Decl>, s: &Strategy, lang: Lang, v: &Value) -> String {
    let n = |x: &Value| {
        x.as_f64()
            .map(|f| format!("{}", f.round() as i64))
            .unwrap_or_default()
    };
    let Some(d) = d else {
        return match v {
            Value::String(s) => s.clone(),
            Value::Bool(true) => copy_text("beRotation.val.on", &[]),
            Value::Bool(false) => copy_text("beRotation.val.off", &[]),
            other => other.to_string(),
        };
    };
    match (d.ty, v) {
        (_, Value::Null) if d.ty != Type::Line => copy_text("beRotation.val.empty", &[]),
        (Type::Bool, Value::Bool(true)) => copy_text("beRotation.val.on", &[]),
        (Type::Bool, _) => copy_text("beRotation.val.off", &[]),
        (Type::Minutes, x) => copy_text("beRotation.val.minutes", &[("n", &n(x))]),
        (Type::Points, x) => copy_text("beRotation.val.points", &[("n", &n(x))]),
        (Type::Percent, x) => copy_text("beRotation.val.percent", &[("n", &n(x))]),
        (Type::OneOf, Value::String(o)) => s
            .texts
            .get(lang, &format!("{}_{o}", d.label))
            .unwrap_or(o)
            .to_string(),
        (Type::Line, Value::Null) => copy_text("beRotation.desc.full", &[]),
        (Type::Line, Value::Object(m)) => ["5h", "7d"]
            .iter()
            .map(|w| {
                let x = m.get(*w).map_or_else(
                    || copy_text("beRotation.desc.full", &[]),
                    |x| copy_text("beRotation.val.percent", &[("n", &n(x))]),
                );
                copy_text("beRotation.val.window", &[("w", w), ("v", &x)])
            })
            .collect::<Vec<_>>()
            .join(&copy_text("beRotation.desc.sep", &[])),
        (Type::Line, x) => copy_text("beRotation.val.percent", &[("n", &n(x))]),
        (Type::Slots, Value::Array(a)) if a.is_empty() => copy_text("beRotation.val.noSlots", &[]),
        (Type::Slots, Value::Array(a)) => a
            .iter()
            .map(|x| {
                let at = x.get("at").and_then(Value::as_str).unwrap_or("");
                match x.get("line").and_then(Value::as_i64) {
                    Some(0) => copy_text("beRotation.val.slotOff", &[("at", at)]),
                    Some(l) => {
                        copy_text("beRotation.val.slot", &[("at", at), ("n", &l.to_string())])
                    }
                    None => at.to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join(&copy_text("beRotation.desc.sep", &[])),
        (_, other) => other.to_string(),
    }
}

/// 一行在人眼里叫什么。
pub(crate) fn row_text(who: &str) -> String {
    match who {
        "@start" => copy_text("beRotation.row.start", &[]),
        "@rest" => copy_text("beRotation.row.rest", &[]),
        INIT_ROW => copy_text("beRotation.row.init", &[]),
        a => crate::accounts::quota::name_words::account_text(a),
    }
}

/// ★ 两份内容逐格比：`a` 是原样、`b` 是改过的；按 `b` 那份策略的声明写字（策略换了 ⇒ 只回 `@strategy` 那一条 ＋ 名字）。
pub(crate) fn diff(a: &ConfigBody, b: &ConfigBody, s: &Strategy, lang: Lang) -> Vec<Diff> {
    let mut out = Vec::new();
    let whole = |label: &str, from: &str, to: &str| {
        copy_text(
            "rot.doc.diffWhole",
            &[("label", label), ("from", from), ("to", to)],
        )
    };
    if a.name != b.name {
        out.push(Diff {
            acct: None,
            k: "@name".into(),
            from: json!(a.name),
            to: json!(b.name),
            text: whole(&copy_text("beRotation.row.name", &[]), &a.name, &b.name),
        });
    }
    if a.strategy != b.strategy {
        out.push(Diff {
            acct: None,
            k: "@strategy".into(),
            from: json!(a.strategy),
            to: json!(b.strategy),
            text: whole(
                &copy_text("beRotation.row.strategy", &[]),
                &a.strategy,
                &b.strategy,
            ),
        });
        return out;
    }
    let label = |d: &Decl| s.texts.get(lang, &d.label).unwrap_or(&d.label).to_string();
    for d in &s.params {
        let (x, y) = (
            a.params.get(&d.k).unwrap_or(&Value::Null),
            b.params.get(&d.k).unwrap_or(&Value::Null),
        );
        if x != y {
            out.push(Diff {
                acct: None,
                k: d.k.clone(),
                from: x.clone(),
                to: y.clone(),
                text: whole(
                    &label(d),
                    &value_text(Some(d), s, lang, x),
                    &value_text(Some(d), s, lang, y),
                ),
            });
        }
    }
    let cell = |who: &str, d: &Decl, x: &Value, y: &Value| Diff {
        acct: Some(who.to_string()),
        k: d.k.clone(),
        from: x.clone(),
        to: y.clone(),
        text: copy_text(
            "rot.doc.diffCell",
            &[
                ("acct", &row_text(who)),
                ("label", &label(d)),
                ("from", &value_text(Some(d), s, lang, x)),
                ("to", &value_text(Some(d), s, lang, y)),
            ],
        ),
    };
    let rows = |c: &ConfigBody| -> Vec<(String, Params)> {
        c.accounts
            .iter()
            .map(|r| (r.who.key().to_string(), r.params.clone()))
            .chain([(INIT_ROW.to_string(), c.acct_init.clone())])
            .collect()
    };
    let (ra, rb) = (rows(a), rows(b));
    for (who, pb) in &rb {
        match ra.iter().find(|(w, _)| w == who) {
            Some((_, pa)) => {
                for d in &s.acct_params {
                    let (x, y) = (
                        pa.get(&d.k).unwrap_or(&Value::Null),
                        pb.get(&d.k).unwrap_or(&Value::Null),
                    );
                    if x != y {
                        out.push(cell(who, d, x, y));
                    }
                }
            }
            None => out.push(join(who, false, true)),
        }
    }
    for (who, _) in &ra {
        if !rb.iter().any(|(w, _)| w == who) {
            out.push(join(who, true, false));
        }
    }
    out
}

fn join(who: &str, from: bool, to: bool) -> Diff {
    let w = |b: bool| {
        if b {
            copy_text("beRotation.val.joined", &[])
        } else {
            copy_text("beRotation.val.left", &[])
        }
    };
    Diff {
        acct: Some(who.to_string()),
        k: "@join".into(),
        from: json!(from),
        to: json!(to),
        text: copy_text(
            "rot.doc.diffCell",
            &[
                ("acct", &row_text(who)),
                ("label", &copy_text("beRotation.row.join", &[])),
                ("from", &w(from)),
                ("to", &w(to)),
            ],
        ),
    }
}

/// 换策略：旧的整份参数 · 新加的号缺省 · 每行参数收进 `parked[旧]`；新的那份有收起的 ⇒ 取回；没有 ⇒ 同名同类型的照搬，其余按 `init`。
pub(crate) fn switch_strategy(
    body: &ConfigBody,
    from: Option<&Strategy>,
    to: &Strategy,
) -> ConfigBody {
    let mut b = body.clone();
    let old = b.strategy.clone();
    let rows: Vec<Value> = b
        .accounts
        .iter()
        .map(|r| json!({"who": r.who.key(), "params": r.params}))
        .collect();
    b.parked.insert(
        old,
        json!({"params": b.params, "acctInit": b.acct_init, "accounts": rows}),
    );
    let keep = |decls: &[Decl], prev_decls: Option<&[Decl]>, prev: &Params| -> Params {
        decls
            .iter()
            .filter_map(|d| {
                let same = prev_decls.is_none_or(|p| p.iter().any(|x| x.k == d.k && x.ty == d.ty));
                prev.get(&d.k)
                    .filter(|_| same)
                    .map(|v| (d.k.clone(), v.clone()))
            })
            .collect()
    };
    let parked = b.parked.remove(&to.id);
    match parked {
        Some(p) => {
            let get = |k: &str| {
                p.get(k)
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default()
            };
            b.params = get("params");
            b.acct_init = get("acctInit");
            let saved: Vec<(String, Params)> = p
                .get("accounts")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|r| {
                            Some((
                                r.get("who")?.as_str()?.to_string(),
                                r.get("params")?.as_object()?.clone(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            for r in &mut b.accounts {
                let prev = r.params.clone();
                r.params = saved
                    .iter()
                    .find(|(w, _)| w == r.who.key())
                    .map(|(_, p)| p.clone())
                    .unwrap_or_else(|| {
                        keep(
                            &to.acct_params,
                            from.map(|f| f.acct_params.as_slice()),
                            &prev,
                        )
                    });
            }
        }
        None => {
            b.params = keep(&to.params, from.map(|f| f.params.as_slice()), &b.params);
            b.acct_init = keep(
                &to.acct_params,
                from.map(|f| f.acct_params.as_slice()),
                &b.acct_init,
            );
            for r in &mut b.accounts {
                r.params = keep(
                    &to.acct_params,
                    from.map(|f| f.acct_params.as_slice()),
                    &r.params,
                );
            }
        }
    }
    b.strategy = to.id.clone();
    filled(&b, to)
}

/// 会话那一份副本落定：跟那份配置的原样相等 ⇒ 丢掉 `copy`。
pub(crate) fn settle_copy(book: &Book, open: &mut OpenConfig) {
    if let (Some(copy), Some(c)) = (&open.copy, book.configs.get(&open.id)) {
        if *copy == c.body {
            open.copy = None;
            open.rev = c.rev;
        }
    }
}

/// 一份配置存了新版：没改过副本的会话 `rev` 跟上。
pub(crate) fn follow_rev(book: &mut Book, id: &str) {
    let Some(rev) = book.configs.get(id).map(|c| c.rev) else {
        return;
    };
    for s in book.sessions.values_mut() {
        if s.config.id == id && s.config.copy.is_none() {
            s.config.rev = rev;
        }
    }
}

/// 写一份配置（新建 / 覆盖）：`rev` ＋1 · 盖 `updatedAt` · `savedBy` · 时区；没改过副本的会话跟上。回新的 `rev`。
pub(crate) fn write_config(
    book: &mut Book,
    id: &str,
    mut body: ConfigBody,
    now: u64,
    by: &str,
    tz: Option<String>,
) -> u64 {
    if tz.is_some() {
        body.tz = tz;
        body.tz_unknown = false;
    }
    let rev = book.configs.get(id).map_or(1, |c| c.rev + 1);
    book.configs.insert(
        id.to_string(),
        Config {
            body,
            rev,
            updated_at: now,
            saved_by: by.to_string(),
        },
    );
    follow_rev(book, id);
    rev
}

/// 另存为没起名 ⇒ `配置 N`（第一个不重的 N）。
pub(crate) fn free_name(book: &Book) -> String {
    (1..)
        .map(|n| copy_text("beRotation.config.anonName", &[("n", &n.to_string())]))
        .find(|n| name_errors(book, None, n).is_empty())
        .unwrap_or_default()
}

/// 改副本的一格：哪一行（整份 ⇒ 缺；`@start` · `@rest` · `@init` · 号 id）· 哪一格 · 新值。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Set {
    pub(crate) acct: Option<String>,
    pub(crate) k: String,
    pub(crate) v: Value,
}

/// 一次改副本：改格 · 加号（新的一行抄 `acctInit`）· 不参与（删那一行）· 换策略。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Edit {
    pub(crate) set: Vec<Set>,
    pub(crate) add: Vec<String>,
    pub(crate) drop: Vec<String>,
    pub(crate) strategy: Option<String>,
}

/// 改完：改没改过（跟原样比）· 跟原样差哪几格 · 没收的那几格。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Edited {
    pub(crate) dirty: bool,
    pub(crate) diff: Vec<Diff>,
    pub(crate) errors: Vec<CellError>,
}

/// 一份内容上改一次（不碰会话 / 配置表；配置编辑器的草稿也走它）。收不下的格不改、记进 `errors`。
pub(crate) fn apply(
    base: &ConfigBody,
    e: &Edit,
    strategies: &[std::sync::Arc<Strategy>],
    account_ok: &dyn Fn(&str) -> bool,
) -> (ConfigBody, Vec<CellError>) {
    let mut b = base.clone();
    let mut errors = Vec::new();
    let find = |id: &str| {
        strategies
            .iter()
            .find(|s| s.id == id && s.usable())
            .cloned()
    };
    if let Some(to) = &e.strategy {
        match find(to) {
            Some(t) if *to != b.strategy => {
                let from = find(&b.strategy);
                b = switch_strategy(&b, from.as_deref(), &t);
            }
            Some(_) => {}
            None => errors.push(cell_err("strategy".into(), "strategy")),
        }
    }
    let Some(s) = find(&b.strategy) else {
        errors.push(cell_err("strategy".into(), "strategy"));
        return (b, errors);
    };
    for a in &e.drop {
        let who = Who::of_key(a);
        b.accounts.retain(|r| r.who != who);
    }
    for a in &e.add {
        let who = Who::of_key(a);
        let ok = match &who {
            Who::Id(x) => account_ok(x),
            _ => true,
        };
        if !ok {
            errors.push(cell_err(format!("add.{a}"), "account"));
        } else if b.row(&who).is_none() {
            b.accounts.push(AcctRow {
                who,
                params: b.acct_init.clone(),
            });
        }
    }
    for x in &e.set {
        let (decls, cell) = match &x.acct {
            None => (&s.params, format!("params.{}", x.k)),
            Some(a) => (&s.acct_params, format!("{a}.{}", x.k)),
        };
        let Some(d) = decls.iter().find(|d| d.k == x.k) else {
            errors.push(cell_err(cell, "type"));
            continue;
        };
        let v = match form::check_cell(d, &x.v) {
            Ok(v) => v,
            Err((form::Code(c), sub)) => {
                errors.push(cell_err(format!("{cell}{sub}"), c));
                continue;
            }
        };
        match x.acct.as_deref() {
            None => {
                b.params.insert(x.k.clone(), v);
            }
            Some(INIT_ROW) => {
                b.acct_init.insert(x.k.clone(), v);
            }
            Some(a) => {
                let who = Who::of_key(a);
                if b.row(&who).is_none() {
                    // `rest` 底下的一个号：给它单写一行（抄 `rest` 那一行的数，放在 `rest` 后面）。
                    let ok = matches!(&who, Who::Id(id) if account_ok(id));
                    let Some(at) = b
                        .accounts
                        .iter()
                        .position(|r| r.who == Who::Rest)
                        .filter(|_| ok)
                    else {
                        errors.push(cell_err(cell, "account"));
                        continue;
                    };
                    let params = b.accounts[at].params.clone();
                    b.accounts.insert(
                        at + 1,
                        AcctRow {
                            who: who.clone(),
                            params,
                        },
                    );
                }
                if let Some(r) = b.accounts.iter_mut().find(|r| r.who == who) {
                    r.params.insert(x.k.clone(), v);
                }
            }
        }
    }
    (filled(&b, &s), errors)
}

/// ★ 改会话的副本（`rotation-session-edit`）：照 `copy`（没有 ⇒ 那份配置的原样）改；跟原样相等就丢 `copy`。
/// 会话不在 ⇒ `None`。
pub(crate) fn edit_session(
    book: &mut Book,
    sid: &str,
    e: &Edit,
    strategies: &[std::sync::Arc<Strategy>],
    account_ok: &dyn Fn(&str) -> bool,
    lang: Lang,
) -> Option<Edited> {
    let s = book.sessions.get(sid)?;
    let o = book.opened(s)?;
    let (id, rev) = (o.id.to_string(), o.rev);
    let base = book.configs.get(&id)?.body.clone();
    let cur = o.body.clone();
    let (next, errors) = apply(&cur, e, strategies, account_ok);
    let mut open = OpenConfig {
        id,
        rev,
        copy: Some(next.clone()),
    };
    settle_copy(book, &mut open);
    let dirty = open.copy.is_some();
    if let Some(s) = book.sessions.get_mut(sid) {
        s.config = open;
    }
    let s = strategies
        .iter()
        .find(|s| s.id == next.strategy && s.usable());
    let diff = s.map(|s| diff(&base, &next, s, lang)).unwrap_or_default();
    Some(Edited {
        dirty,
        diff,
        errors,
    })
}

/// 存会话的副本那一下的结局。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Saved {
    Saved {
        id: String,
        rev: u64,
    },
    /// 那份配置读到之后别处存过新版（没 `force`）：此刻的版本 · 别处那一版跟本会话的差 · 本会话的改动。
    Clash {
        rev: u64,
        theirs: Vec<Diff>,
        mine: Vec<Diff>,
    },
    /// 会话不在 · 没改过（没什么可存）· 另存为的名字不收。
    Refused(Vec<CellError>),
}

/// ★ 存会话的副本（`rotation-session-save`）：`as_name` 有 ⇒ 另存为（新 id，本会话改开它）；否则写回（`rev` 不对且没 `force` ⇒ 撞上）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn save_session(
    book: &mut Book,
    sid: &str,
    if_rev: Option<u64>,
    as_name: Option<Option<String>>,
    force: bool,
    now: u64,
    tz: Option<String>,
    strategies: &[std::sync::Arc<Strategy>],
    lang: Lang,
) -> Saved {
    let Some(s) = book.sessions.get(sid) else {
        return Saved::Refused(vec![cell_err("sid".into(), "account")]);
    };
    let Some(o) = book.opened(s) else {
        return Saved::Refused(vec![cell_err("sid".into(), "account")]);
    };
    let (id, body, open_rev) = (o.id.to_string(), o.body.clone(), s.config.rev);
    let strategy = strategies
        .iter()
        .find(|x| x.id == body.strategy && x.usable())
        .cloned();
    if let Some(name) = as_name {
        let name = match name.filter(|n| !n.trim().is_empty()) {
            Some(n) => {
                let errs = name_errors(book, None, &n);
                if !errs.is_empty() {
                    return Saved::Refused(errs);
                }
                n.trim().to_string()
            }
            None => free_name(book),
        };
        let new_id = super::new_config_id(&book.configs);
        let body = ConfigBody { name, ..body };
        let rev = write_config(book, &new_id, body, now, sid, tz);
        if let Some(s) = book.sessions.get_mut(sid) {
            s.config = OpenConfig {
                id: new_id.clone(),
                rev,
                copy: None,
            };
        }
        return Saved::Saved { id: new_id, rev };
    }
    let Some(cur) = book.configs.get(&id).cloned() else {
        return Saved::Refused(vec![cell_err("config".into(), "account")]);
    };
    if s.config.copy.is_none() {
        return Saved::Saved { id, rev: cur.rev };
    }
    let stale = if_rev.map_or(cur.rev != open_rev, |r| r != cur.rev);
    if stale && !force {
        let (theirs, mine) = match &strategy {
            Some(st) => (
                diff(&body, &cur.body, st, lang),
                diff(&cur.body, &body, st, lang),
            ),
            None => (Vec::new(), Vec::new()),
        };
        return Saved::Clash {
            rev: cur.rev,
            theirs,
            mine,
        };
    }
    let rev = write_config(book, &id, body, now, sid, tz);
    if let Some(s) = book.sessions.get_mut(sid) {
        s.config = OpenConfig {
            id: id.clone(),
            rev,
            copy: None,
        };
    }
    Saved::Saved { id, rev }
}

/// 还原（`rotation-session-revert`）：丢 `copy`、`rev` 跟上那份配置此刻的（`to_latest` 同一件事：载入新版本）。
pub(crate) fn revert_session(book: &mut Book, sid: &str) -> bool {
    let rev = book
        .sessions
        .get(sid)
        .and_then(|s| book.configs.get(&s.config.id))
        .map(|c| c.rev);
    match (book.sessions.get_mut(sid), rev) {
        (Some(s), Some(rev)) => {
            s.config.copy = None;
            s.config.rev = rev;
            true
        }
        _ => false,
    }
}

/// 打开一份配置那一下的结局。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Opened {
    Opened,
    /// 本会话的副本有没存的改动（没 `discard`）：先问；`diff` ＝ 改了哪几格。
    Dirty(Vec<Diff>),
    Missing,
}

/// ★ 打开一份配置（`rotation-session-open`）：有 `copy` 且没 `discard` ⇒ 先问；否则打开，清手动调整与备忘。
pub(crate) fn open_session(
    book: &mut Book,
    sid: &str,
    id: &str,
    discard: bool,
    strategies: &[std::sync::Arc<Strategy>],
    lang: Lang,
) -> Opened {
    let Some(c) = book.configs.get(id).cloned() else {
        return Opened::Missing;
    };
    let Some(s) = book.sessions.get(sid) else {
        return Opened::Missing;
    };
    if let (Some(copy), false) = (&s.config.copy, discard) {
        let base = book.configs.get(&s.config.id).map(|x| x.body.clone());
        let st = strategies
            .iter()
            .find(|x| x.id == copy.strategy && x.usable());
        let d = match (base, st) {
            (Some(b), Some(st)) => diff(&b, copy, st, lang),
            _ => Vec::new(),
        };
        return Opened::Dirty(d);
    }
    if let Some(s) = book.sessions.get_mut(sid) {
        s.config = OpenConfig {
            id: id.to_string(),
            rev: c.rev,
            copy: None,
        };
        s.manual = super::Manual::default();
        s.memo = None;
    }
    Opened::Opened
}

/// 别处存了新版、本会话有 `copy` ⇒ 那一版的版本 · 时刻 · 谁存的（`rev` 跟打开时一样 ⇒ 没有）。
pub(crate) fn newer_of(book: &Book, s: &super::SessionEntry) -> Option<(u64, u64, String)> {
    let c = book.configs.get(&s.config.id)?;
    (s.config.copy.is_some() && c.rev > s.config.rev)
        .then(|| (c.rev, c.updated_at, c.saved_by.clone()))
}
