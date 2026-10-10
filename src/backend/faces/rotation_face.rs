//! 换号那一族的帧面宿主：把账号库（`accounts/manage`）读成上游选择换号要的那几格交进去 —— 上游选择与账号库管理互不认识，
//! 两边只在这里接上。

use crate::accounts::upstream_select::rotate::{LibAccount, Library, LibraryRead};
use std::path::Path;

/// 这台的账号库：清单里每个号（配置目录的末段 ＝ 路由第 2 段）＋ 账号 0。没建账号库 / 清单读不懂 ⇒ 空。
pub(crate) fn library_at(home: &Path) -> Library {
    let manifest = home
        .join(relay_route_core::ACCOUNTS_DIR_REL)
        .join(relay_route_core::ACCOUNTS_MANIFEST_NAME);
    let Some(m) = crate::accounts::manage::scan::manifest_text_at(&manifest)
        .ok()
        .and_then(|t| crate::accounts::manage::model::Manifest::parse(&t).ok())
    else {
        return Library::default();
    };
    let mut accounts = vec![LibAccount {
        id: crate::accounts::manage::model::ACCOUNT_ZERO.to_string(),
        dir: None,
        api: false,
    }];
    accounts.extend(m.managed().filter_map(|a| {
        Some(LibAccount {
            id: acct_core::apikey_account_id_of_dir(&a.config_dir)?,
            dir: Some(a.config_dir.clone().into()),
            api: a.is_api_key(),
        })
    }));
    Library {
        enabled: true,
        accounts,
    }
}

/// 交给中转装配的那一份读法（`main.rs` 起中转时递进去）。
pub fn library() -> LibraryRead {
    std::sync::Arc::new(library_at)
}

// ── 帧面命令：默认轮换 · 会话轮换 · 现在就换 ───────────────────────────────────────

use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::rotation::{
    self, Book, Rotation, RotationStore, SessionEntry, SwitchOutcome, SwitchRecord, SwitchWhy,
    Unready,
};
use crate::accounts::quota::show;
use crate::accounts::upstream_select::rotate::{account_ok, Hop};
use crate::stream::inbound::spec::wire;
use copy_core::copy_text;
use serde_json::{json, Map, Value};
use std::sync::Arc;

/// 失败带码 ＋ 那一句 ＋ 原话（写盘那几形的原话进复制详情，[`Fail`]）。
type Answer = Result<Value, Fail>;
use crate::stream::inbound::spec::Fail;

/// 一条命令要的几样：轮换的账本（经它判、经它写）· 这台 key 表里有哪几行（按量号接不接得上）。
/// `(agent, 号)` → 这台 key 表里那一行：没有 ⇒ `None`；有 ⇒ 接不接得上。
type Rows = Box<dyn Fn(&str, &str) -> Option<bool> + Send + Sync>;

/// 这台此刻活着的会话（sid 集合）。
type Live = Box<dyn Fn() -> std::collections::BTreeSet<String> + Send + Sync>;

pub(crate) use crate::observe::accounts_query::Doing;

/// 这台此刻活着的会话各在干什么（与主窗口标签页同一判；规则在用名单的状态）。
type DoingRead = Box<dyn Fn() -> std::collections::BTreeMap<String, Doing> + Send + Sync>;

pub(crate) struct Ctx {
    pub(crate) hop: Hop,
    /// 会话血缘（读：会话的父；跟随父会话按它填）。
    pub(crate) lineage: Arc<crate::lineage::LineageStore>,
    pub(crate) rows: Rows,
    pub(crate) live: Live,
    pub(crate) doing: DoingRead,
}

impl Ctx {
    /// 这台的生产那一份：账本 · 额度账（现读盘）· 家 · 账号库 · key 表都从真环境取。
    pub(crate) fn here() -> Self {
        let get = |k: &str| std::env::var(k).ok();
        let quota = Arc::new(Ledger::at(ledger::path_from(&get)));
        let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into));
        let rows = crate::accounts::upstream_select::file_face::machine_rows();
        Self {
            hop: Hop::new(
                Arc::new(RotationStore::at(rotation::path_now())),
                quota,
                home,
                library(),
                None,
            ),
            lineage: Arc::new(crate::lineage::LineageStore::at(crate::lineage::path_now())),
            rows: Box::new(move |agent, a| {
                (agent == crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT
                    && rows.iter().any(|r| r == a))
                .then_some(true)
            }),
            // 判活同历史清单那一处：pidfile 里的会话 id ＋ 进程还是同一个。
            live: Box::new(|| {
                crate::observe::accounts_query::live_session_ids(
                    &crate::observe::history_query::agent_home(),
                )
            }),
            doing: Box::new(|| {
                crate::observe::accounts_query::live_doing(
                    &crate::observe::history_query::agent_home(),
                )
            }),
        }
    }

    fn is_api(&self, agent: &str, a: &str) -> bool {
        (self.rows)(agent, a).is_some()
            || self
                .hop
                .library()
                .accounts
                .iter()
                .any(|x| x.id == a && x.api)
    }
}

fn bad(detail: &str) -> Fail {
    Fail::new("bad_args", crate::common::contract::malformed(detail))
}

fn store_path(ctx: &Ctx) -> Result<&Path, (&'static str, String)> {
    ctx.hop
        .store
        .path()
        .ok_or_else(|| ("io_failed", copy_text("beRotation.read.noHome", &[])))
}

/// 盘上那一份（三态，同额度账）。
fn read_book(ctx: &Ctx) -> (&'static str, Option<crate::common::said::Said>, Book) {
    match ctx.hop.store.path().map(rotation::read_at) {
        None => (
            "unreadable",
            Some(copy_text("beRotation.read.noHome", &[]).into()),
            Book::default(),
        ),
        Some(rotation::Read::Absent) => ("absent", None, Book::default()),
        Some(rotation::Read::Present(b)) => ("present", None, b),
        Some(rotation::Read::Unreadable(e)) => ("unreadable", Some(e), Book::default()),
    }
}

/// 凭据文件那一家（账号库是它的）：默认轮换里新勾的按量号按它判。
const LIBRARY_AGENT: &str = crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT;

/// `quota-read`：这台的额度账，每条带上显示态（「快满」按这台默认轮换的 N）；另给账号库里从没出过数的号、
/// 此刻发得出去的号、最早回来的那个。
/// 出口那一下给每个时刻添好显示的字（`common::time::with_texts`，按这台的本地钟）、每号添好几行（`faces/quota_rows.rs`）
/// 与开窗那一判（`warm`：quota-warm 照它发一句 / 睡到几点）。
pub(crate) fn answer_quota_read() -> Answer {
    answer_quota_read_with(&Ctx::here(), crate::accounts::quota::now_unix())
}

/// [`answer_quota_read`] 的本体（出口在 [`crate::faces::quota_read::reply_of`]：时刻字 · 每号几行 · 开窗那一判 · 号名 / 位名）。
pub(crate) fn answer_quota_read_with(ctx: &Ctx, now: u64) -> Answer {
    wire::<_, Fail>(&quota_read_with(ctx, now))
}

pub(crate) use crate::faces::quota_read::QuotaRead;

/// 读这台的额度账、给每个号判显示态，交出口写成成品（时刻字按这台此刻的本地钟）。
pub(crate) fn quota_read_with(ctx: &Ctx, now: u64) -> QuotaRead {
    use crate::faces::quota_read::{reply_of, Base, Seen, UnseenHead};
    let base = ledger::answer_of(ctx.hop.quota.path(), now);
    let seen = &base.accounts;
    let lib = ctx.hop.library();
    let rot = ctx.hop.store.now().default_rotation();
    let offset = crate::accounts::upstream_select::rotate::local_offset(now);
    let shown = |agent: &str, account: &str, o: Option<&ledger::Observed>| {
        let slot = crate::agents::window_slot_of(agent);
        let key = crate::agents::window_key_of(agent);
        let slot = |w: &str| slot.and_then(|f| f(w));
        let key = |w: &str| key.and_then(|f| f(w));
        let mut sh = show::show(
            o.map(|o| (&o.reading, o.seen_at)),
            ctx.hop
                .show_facts(agent, &lib, account, &|a| (ctx.rows)(agent, a)),
            &show::lines_of(&rot.cap, account, now, offset, &key, &slot),
            now,
            &slot,
        );
        if let Some(o) = o {
            sh.windows = show::windows_of(o, &key, now);
        }
        sh
    };
    let mut usable: Vec<String> = Vec::new();
    let mut earliest: Option<(u64, String)> = None;
    let mut rows: Vec<Seen> = Vec::new();
    for o in seen {
        let sh = shown(&o.agent, &o.account, Some(o));
        if show::usable(&sh) {
            usable.push(o.account.clone());
        }
        if let Some(at) = show::back_at(&sh, &o.reading, now) {
            if earliest.as_ref().is_none_or(|(t, _)| at < *t) {
                earliest = Some((at, o.account.clone()));
            }
        }
        rows.push(Seen {
            agent: o.agent.clone(),
            account: o.account.clone(),
            seen_at: o.seen_at,
            reading: serde_json::to_value(&o.reading).unwrap_or(Value::Null),
            windows_seen: (!o.windows_seen.is_empty())
                .then(|| serde_json::to_value(&o.windows_seen).unwrap_or(Value::Null)),
            show: sh,
        });
    }
    let unseen: Vec<UnseenHead> = lib
        .accounts
        .iter()
        .filter(|a| {
            !seen
                .iter()
                .any(|o| o.agent == LIBRARY_AGENT && o.account == a.id)
        })
        .map(|a| {
            let sh = shown(LIBRARY_AGENT, &a.id, None);
            if show::usable(&sh) {
                usable.push(a.id.clone());
            }
            UnseenHead {
                agent: LIBRARY_AGENT.to_string(),
                account: a.id.clone(),
                kind: sh.kind,
                login: sh.login,
                sub_id: sh.sub_id,
            }
        })
        .collect();
    reply_of(
        Base {
            state: base.state,
            reason: base.reason.clone(),
            detail: base.detail.clone(),
            path: base.path.clone(),
            now: base.now,
            usable_now: usable,
            earliest: earliest.map(|(at, account)| (account, at)),
        },
        rows,
        unseen,
        &crate::common::time::TextClock::here(now),
    )
}

// ── 规则表：读 · 存 · 改名 · 删 · 设为默认 ────────────────────────────────────────────

use crate::accounts::quota::rotation::{CellError, Rule, Source};
use crate::accounts::quota::rule_text;

/// 一条规则的线上形状：本身那几格 ＋ 后端算好的（谁在用 · 摘要 · 说明 · 这台没有的号 · 「无号可换」作不作数）。
fn rule_wire(
    ctx: &Ctx,
    book: &Book,
    id: &str,
    r: &Rule,
    live: &std::collections::BTreeSet<String>,
    doing: &std::collections::BTreeMap<String, Doing>,
) -> Value {
    let (mut sids, mut ended_sids): (Vec<&String>, Vec<&String>) = (Vec::new(), Vec::new());
    let mut follow = 0usize;
    for (sid, s) in &book.sessions {
        if book.rule_of(s) != Some(id) {
            continue;
        }
        if live.contains(sid) {
            sids.push(sid);
            if s.source == Source::Follow {
                follow += 1;
            }
        } else {
            ended_sids.push(sid);
        }
    }
    let lib = ctx.hop.library();
    let missing: Vec<&String> = r
        .rotation
        .order
        .iter()
        .filter_map(|slot| match slot {
            rotation::RotationSlot::Named(a) if !lib.accounts.iter().any(|x| &x.id == a) => Some(a),
            _ => None,
        })
        .collect();
    json!({
        "id": id,
        "name": r.name,
        "rotation": r.rotation,
        "rev": r.rev,
        "updatedAt": r.updated_at,
        "isDefault": book.default_rule == id,
        "users": {
            "live": sids.len(),
            "ended": ended_sids.len(),
            "follow": follow,
            "doing": sids.iter().map(|sid| ((*sid).clone(), doing_wire(doing.get(*sid))))
                .chain(ended_sids.iter().map(|sid| ((*sid).clone(), ended_wire())))
                .collect::<Map<String, Value>>(),
            "sids": sids,
            "endedSids": ended_sids,
        },
        "summary": rule_text::summary(&r.rotation),
        "explain": rule_text::explain(&r.rotation),
        "missing": missing,
        "atLimitApplies": rule_text::at_limit_applies(&r.rotation),
    })
}

/// 一个活会话的状态：`state`（轮换那一侧的判：`working` · `idle` · `needsYou`（带 `needs`：approve · answer · plan · network ·
/// worker · goal · choose · unknown）· `ended`）＋ 显示用的 `text` · `tone`（与主窗口同一处写：活动态的字 · 那一种「需手动」的字 ·
/// 去向的字）。说不清在干什么 ⇒ `working`；后台命令还在跑 ⇒ 也按 `working`（重启会把它掐掉），字照 activity 写。
fn doing_wire(d: Option<&Doing>) -> Value {
    use crate::agents::SessionActivity as A;
    use crate::observe::facts_query::{needs_words, NeedsKind};
    let activity = d.and_then(|d| d.activity);
    let (text, tone) = crate::stream::wire::activity_cells(activity);
    match activity {
        Some(A::NeedsYou) => {
            let kind = d.and_then(|d| d.needs).unwrap_or(NeedsKind::Unknown);
            json!({
                "state": "needsYou",
                "needs": kind,
                "text": needs_words(kind),
                "tone": crate::common::cells::Tone::Need,
            })
        }
        Some(A::Idle) => json!({"state": "idle", "needs": null, "text": text, "tone": tone}),
        Some(A::Working | A::BackgroundWork) | None => {
            json!({"state": "working", "needs": null, "text": text, "tone": tone})
        }
    }
}

/// 名单里一个已结束的会话（字与语气同 `session_state` 的去向）。
fn ended_wire() -> Value {
    let (text, _, tone) = crate::stream::wire::SessionFate::Ended.cells();
    json!({"state": "ended", "needs": null, "text": text, "tone": tone})
}

/// `rotation-rules-read` 的应答。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RulesRead {
    state: &'static str,
    reason: Value,
    detail: Value,
    path: Option<String>,
    default_rule: String,
    /// 每条一项，形状见 [`rule_wire`]。
    rules: Vec<Value>,
}

fn rules_wire(ctx: &Ctx) -> RulesRead {
    let (state, why, book) = read_book(ctx);
    let (reason, detail) = crate::stream::detail::unreadable("rotation-rules-read", why.as_ref());
    let live = (ctx.live)();
    let doing = (ctx.doing)();
    let mut rules: Vec<(&String, &Rule)> = book.rules.iter().collect();
    // 默认那条排最前，其余按名字。
    rules.sort_by(|a, b| {
        (a.0 != &book.default_rule)
            .cmp(&(b.0 != &book.default_rule))
            .then_with(|| a.1.name.cmp(&b.1.name))
    });
    RulesRead {
        state,
        reason,
        detail,
        path: ctx.hop.store.path().map(|p| p.display().to_string()),
        rules: rules
            .iter()
            .map(|(id, r)| rule_wire(ctx, &book, id, r, &live, &doing))
            .collect(),
        default_rule: book.default_rule,
    }
}

/// `rotation-rules-read`：这台的规则表。
pub(crate) fn answer_rules_read() -> Answer {
    answer_rules_read_with(&Ctx::here())
}

pub(crate) fn answer_rules_read_with(ctx: &Ctx) -> Answer {
    let mut v = wire::<_, Fail>(&rules_wire(ctx))?;
    crate::accounts::quota::name_words::with_names(&mut v);
    Ok(v)
}

/// 名称那一格的错：空 · 超长 · 与这台别的规则重名（不分大小写、去首尾空白）。
fn name_errors(book: &Book, id: Option<&str>, name: &str) -> Vec<CellError> {
    let code = if name.trim().is_empty() {
        Some("empty")
    } else if name.trim().chars().count() > rotation::RULE_NAME_MAX {
        Some("tooLong")
    } else if book.rules.iter().any(|(k, r)| {
        Some(k.as_str()) != id && rotation::name_key(&r.name) == rotation::name_key(name)
    }) {
        Some("dup")
    } else {
        None
    };
    code.map(|c| rotation::cell_err("name".into(), c, None))
        .into_iter()
        .collect()
}

/// 复制出来的那条取名：不重名照原名；重名 ⇒ 名后加 ` 2` · ` 3` … 取第一个不重的（超长照旧由 [`name_errors`] 拒）。
fn free_name(book: &Book, id: Option<&str>, name: &str) -> String {
    let taken = |n: &str| {
        book.rules.iter().any(|(k, r)| {
            Some(k.as_str()) != id && rotation::name_key(&r.name) == rotation::name_key(n)
        })
    };
    let base = name.trim();
    if !taken(base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken(n))
        .unwrap_or_default()
}

/// `rotation-rule-save` / `rotation-rule-rename` 的应答：按 `state` 分三支。
#[derive(serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub(crate) enum RuleSaved {
    /// 写成了：那一条（形状同 `rotation-rules-read` 的一项）。
    Saved { rule: Value },
    /// 逐格错，没写。
    Refused { errors: Vec<CellError> },
    /// 读到之后别处改过：此刻的版本，没写。
    Conflict { rev: u64 },
}

fn refused(errors: Vec<CellError>) -> Answer {
    wire(&RuleSaved::Refused { errors })
}

fn conflict(rev: u64) -> Answer {
    wire(&RuleSaved::Conflict { rev })
}

fn if_rev(args: &Value) -> Result<Option<u64>, Fail> {
    match args.get("ifRev") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| bad("`ifRev` must be a non-negative integer")),
    }
}

fn rule_id_arg(args: &Value, key: &str) -> Result<Option<String>, Fail> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s))
            if !s.is_empty()
                && s.len() <= 32
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') =>
        {
            Ok(Some(s.clone()))
        }
        Some(_) => Err(bad(&format!("`{key}` must be a rule id"))),
    }
}

/// 写成之后回那一条（线上形状）。
fn saved(ctx: &Ctx, id: &str) -> Answer {
    let (_, _, book) = read_book(ctx);
    let live = (ctx.live)();
    let r = book
        .rules
        .get(id)
        .ok_or_else(|| ("failed", format!("rule {id} vanished after write")))?;
    let doing = (ctx.doing)();
    wire(&RuleSaved::Saved {
        rule: rule_wire(ctx, &book, id, r, &live, &doing),
    })
}

/// `rotation-rule-save`：新建（不给 `id`）或整份改一条（给 `id` ＋ `ifRev`）。`{id?, name, rotation?, ifRev?, from?, dedupe?}`：
/// 新建时不给 `rotation` 就从 `from` 那条拷（`from: "blank"` ＝ 只有起始账号）；`dedupe: true`（复制 · 复制到别的机器）⇒
/// 重名不拒、名后加 ` 2` · ` 3` … 取第一个不重的。
/// 回 `{state: "saved", rule}` · `{state: "refused", errors: [{cell, code, with?}]}`（逐格，界面照它标红）·
/// `{state: "conflict", rev}`（别处先改过了）。形状不对 ⇒ `bad_args`；改的那条不在 ⇒ `no_such_rule`。
pub(crate) fn answer_rule_save(args: &Value) -> Answer {
    answer_rule_save_with(&Ctx::here(), args, crate::accounts::quota::now_unix())
}

pub(crate) fn answer_rule_save_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let id = rule_id_arg(args, "id")?;
    let mut name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("missing `name`"))?
        .to_string();
    let dedupe = match args.get("dedupe") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(bad("`dedupe` must be a boolean")),
    };
    let want_rev = if_rev(args)?;
    store_path(ctx)?;
    let book = ctx.hop.store.now();
    if dedupe {
        name = free_name(&book, id.as_deref(), &name);
    }
    let prior = id.as_deref().and_then(|i| book.rules.get(i));
    if id.is_some() && prior.is_none() {
        return Err(("no_such_rule", copy_text("beRotation.rule.gone", &[])).into());
    }
    let mut errors = name_errors(&book, id.as_deref(), &name);
    let rot = match (args.get("rotation"), rule_id_arg(args, "from")?) {
        (Some(v), _) => {
            errors.extend(rotation::cell_errors(v));
            if !errors.is_empty() {
                return refused(errors);
            }
            rotation::rotation_from(
                v,
                0..=1,
                &account_ok,
                &|a| ctx.is_api(LIBRARY_AGENT, a),
                prior.map(|p| &p.rotation),
            )
            .map_err(|e| bad(&e))?
        }
        (None, Some(from)) if prior.is_none() && from == "blank" => Rotation::default(),
        (None, Some(from)) if prior.is_none() => book
            .rules
            .get(&from)
            .map(|r| r.rotation.clone())
            .ok_or_else(|| ("no_such_rule", copy_text("beRotation.rule.gone", &[])))?,
        (None, _) => match prior {
            Some(p) => p.rotation.clone(),
            None => return Err(bad("missing `rotation` (or `from` for a new rule)")),
        },
    };
    if !errors.is_empty() {
        return refused(errors);
    }
    let name = name.trim().to_string();
    let wrote = rotation::face_change(&ctx.hop.store, |b| -> Result<String, u64> {
        match id.clone() {
            Some(id) => {
                let Some(r) = b.rules.get_mut(&id) else {
                    return Err(0);
                };
                if want_rev.is_some_and(|w| w != r.rev) {
                    return Err(r.rev);
                }
                if r.name != name || r.rotation != rot {
                    r.name = name.clone();
                    r.rotation = rot.clone();
                    r.rev += 1;
                    r.updated_at = now;
                }
                Ok(id)
            }
            None => {
                let id = rotation::new_rule_id(&b.rules);
                b.rules.insert(
                    id.clone(),
                    Rule {
                        name: name.clone(),
                        rotation: rot.clone(),
                        rev: 1,
                        updated_at: now,
                    },
                );
                Ok(id)
            }
        }
    })
    .map_err(|e| ("io_failed", e))?;
    match wrote {
        Ok(id) => saved(ctx, &id),
        Err(0) => Err(("no_such_rule", copy_text("beRotation.rule.gone", &[])).into()),
        Err(rev) => conflict(rev),
    }
}

/// `rotation-rule-rename`：`{id, name, ifRev}`；回同 `rotation-rule-save`。
pub(crate) fn answer_rule_rename(args: &Value) -> Answer {
    answer_rule_rename_with(&Ctx::here(), args, crate::accounts::quota::now_unix())
}

pub(crate) fn answer_rule_rename_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let id = rule_id_arg(args, "id")?.ok_or_else(|| bad("missing `id`"))?;
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("missing `name`"))?;
    let mut a = json!({"id": id, "name": name});
    if let Some(r) = args.get("ifRev") {
        a["ifRev"] = r.clone();
    }
    answer_rule_save_with(ctx, &a, now)
}

/// `rotation-rule-delete`：`{ids, then: "custom" | "follow"}`；用着它们的会话按 `then` 落（`custom` ＝ 照那条拷一份成本会话的，行为不变）。
/// 默认那条不许删（`is_default`）。回 `{moved: {sid: "custom" | "follow"}}`。
pub(crate) fn answer_rule_delete(args: &Value) -> Answer {
    answer_rule_delete_with(&Ctx::here(), args)
}

pub(crate) fn answer_rule_delete_with(ctx: &Ctx, args: &Value) -> Answer {
    let ids: Vec<String> = args
        .get("ids")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("`ids` must be an array of rule ids"))?
        .iter()
        .map(|v| match v {
            Value::String(s) => Ok(s.clone()),
            _ => Err(bad("`ids` must be an array of rule ids")),
        })
        .collect::<Result<_, _>>()?;
    let to_custom = match args.get("then").and_then(Value::as_str) {
        Some("custom") => true,
        Some("follow") => false,
        _ => return Err(bad("`then` must be \"custom\" or \"follow\"")),
    };
    store_path(ctx)?;
    let book = ctx.hop.store.now();
    if ids.iter().any(|i| *i == book.default_rule) {
        return Err(("is_default", copy_text("beRotation.rule.isDefault", &[])).into());
    }
    if let Some(i) = ids.iter().find(|i| !book.rules.contains_key(*i)) {
        return Err((
            "no_such_rule",
            copy_text("beRotation.rule.gone", &[]) + " " + i,
        )
            .into());
    }
    let moved = rotation::face_change(&ctx.hop.store, |b| {
        let mut moved = Map::new();
        for (sid, s) in b.sessions.iter_mut() {
            let Source::Rule(id) = &s.source else {
                continue;
            };
            let Some(r) = ids
                .iter()
                .find(|i| *i == id)
                .and_then(|i| book.rules.get(i))
            else {
                continue;
            };
            if to_custom {
                s.custom = Some(r.rotation.clone());
                s.source = Source::Custom;
            } else {
                s.source = Source::Follow;
            }
            moved.insert(
                sid.clone(),
                json!(if to_custom { "custom" } else { "follow" }),
            );
        }
        for i in &ids {
            b.rules.remove(i);
        }
        moved
    })
    .map_err(|e| ("io_failed", e))?;
    wire(&RuleDeleted { moved })
}

/// `rotation-rule-delete` 的应答：用着被删那几条的会话落到了哪。
#[derive(serde::Serialize)]
pub(crate) struct RuleDeleted {
    moved: Map<String, Value>,
}

/// `rotation-default-set`：`{rule}` 设为这台的默认；回 `{defaultRule, followers}`（跟随默认、此刻活着的会话有几个）。
pub(crate) fn answer_default_set(args: &Value) -> Answer {
    answer_default_set_with(&Ctx::here(), args)
}

pub(crate) fn answer_default_set_with(ctx: &Ctx, args: &Value) -> Answer {
    let id = rule_id_arg(args, "rule")?.ok_or_else(|| bad("missing `rule`"))?;
    store_path(ctx)?;
    if !ctx.hop.store.now().rules.contains_key(&id) {
        return Err(("no_such_rule", copy_text("beRotation.rule.gone", &[])).into());
    }
    rotation::face_change(&ctx.hop.store, |b| b.default_rule = id.clone())
        .map_err(|e| ("io_failed", e))?;
    let live = (ctx.live)();
    let followers = ctx
        .hop
        .store
        .now()
        .sessions
        .iter()
        .filter(|(sid, s)| s.source == Source::Follow && live.contains(*sid))
        .count();
    wire(&DefaultSet {
        default_rule: id,
        followers,
    })
}

/// `rotation-default-set` 的应答。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DefaultSet {
    default_rule: String,
    /// 跟随默认、此刻活着的会话有几个。
    followers: usize,
}

/// `session-new` 带 `rotation: {rule}`：起之前在这台的账本里给那个定好的 sid 记一条、来源 ＝ 那条规则（会话一报到就已经是它）。
/// 规则不在 ⇒ `no_such_rule`。`kind` ＝ 线上的那一家（记成它的适配器 id，与中转看见会话时同一格；对不上的那几格中转报到时照常改，来源不动）。
pub(crate) fn preset_with(
    ctx: &Ctx,
    sid: &str,
    kind: &str,
    rule: &str,
    now: u64,
) -> Result<(), (&'static str, String)> {
    store_path(ctx)?;
    if !ctx.hop.store.now().rules.contains_key(rule) {
        return Err(("no_such_rule", copy_text("beRotation.rule.gone", &[])));
    }
    let agent = crate::agents::pick_kind(Some(kind))
        .map(|(_, f)| f.adapter_id.to_string())
        .unwrap_or_else(|_| kind.to_string());
    let zero = crate::accounts::manage::model::ACCOUNT_ZERO;
    rotation::face_change(&ctx.hop.store, |b| {
        let mut e = SessionEntry::fresh(&agent, zero, now);
        e.source = Source::Rule(rule.to_string());
        b.sessions.insert(sid.to_string(), e);
    })
    .map_err(|e| ("io_failed", crate::common::said::IntoNote::into_note(e)))
}

/// 起不成：撤掉 [`preset_with`] 记的那一条（只撤还没换过号、来源还是规则的；写不成只出声）。
pub(crate) fn forget_preset_with(ctx: &Ctx, sid: &str) {
    if let Err(e) = rotation::face_change(&ctx.hop.store, |b| {
        if b.sessions
            .get(sid)
            .is_some_and(|s| s.history.is_empty() && matches!(s.source, Source::Rule(_)))
        {
            b.sessions.remove(sid);
        }
    }) {
        tracing::warn!("[rotation] 撤不掉起之前记的那一条 {sid}：{}", e.logged());
    }
}

/// `rotation-plan`：这份轮换接下来会怎么走 ＋ 草稿逐格校验（都不写）。问的是哪一份：`{rotation}`（草稿：先逐格校验，有错只回 `errors`）·
/// `{rule}`（这台的一条规则）· `{sid}`（这个会话此刻生效的那一份，从它此刻的号起）；`span`：`6h` · `12h`（缺省）· `24h` · `7d`。
/// 回 `{errors, now, plan, lanes, effective}`：用量只按此刻的算（以后涨多快没根据，不预测），结论只在重置 · 时段起止时变。
/// 形状不对（`rotation_from` 整份拒）⇒ `bad_args`；规则 / 会话不在 ⇒ `no_such_rule` / `bad_args`。
pub(crate) fn answer_plan(args: &Value) -> Answer {
    answer_plan_with(&Ctx::here(), args, crate::accounts::quota::now_unix())
}

/// 预览的视窗（秒）。
fn span_secs(args: &Value) -> Result<u64, Fail> {
    match args.get("span") {
        None | Some(Value::Null) => Ok(12 * 3600),
        Some(v) => match v.as_str() {
            Some("6h") => Ok(6 * 3600),
            Some("12h") => Ok(12 * 3600),
            Some("24h") => Ok(24 * 3600),
            Some("7d") => Ok(7 * 86_400),
            _ => Err(bad("`span` must be 6h / 12h / 24h / 7d")),
        },
    }
}

/// 规则 / 草稿没有会话：从池里排第一的号起（起始账号占位 ＝ 账号 0）。
fn first_of(rot: &Rotation, start: &str) -> String {
    rot.pool(start)
        .first()
        .cloned()
        .unwrap_or_else(|| start.to_string())
}

fn cap_at_wire(c: &crate::accounts::quota::decide::CapAt) -> Value {
    let mut v = json!({"v": c.v, "layer": c.layer});
    if let Some(w) = c.w {
        v["w"] = json!(w);
    }
    v
}

/// 全部窗口那一格的悬停：两窗各取多少（`5h ≤90 · 7d 不封顶`），一句由这里拼。
fn lines_text(each: &[(&'static str, crate::accounts::quota::decide::CapAt)]) -> String {
    each.iter()
        .map(|(w, c)| {
            let w = slot_label(w);
            match c.v {
                Some(n) => copy_text("beRotation.eff.line", &[("w", &w), ("n", &n.to_string())]),
                None => copy_text("beRotation.eff.none", &[("w", &w)]),
            }
        })
        .collect::<Vec<_>>()
        .join(&copy_text("kit.text.sep", &[]))
}

fn slot_label(w: &str) -> String {
    crate::accounts::quota::name_words::slot_text(w)
}

/// 时间轴的视窗（此刻之前, 之后，秒）：`6h` ＝ 前 2h · 后 4h；`24h` ＝ 前 6h · 后 18h；`7d` ＝ 前 1d · 后 6d。不给 ⇒ `None`（编辑器那一问：从此刻起）。
fn view_secs(args: &Value) -> Result<Option<(u64, u64)>, Fail> {
    match args.get("view") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_str() {
            Some("6h") => Ok(Some((2 * 3600, 4 * 3600))),
            Some("24h") => Ok(Some((6 * 3600, 18 * 3600))),
            Some("7d") => Ok(Some((86_400, 6 * 86_400))),
            _ => Err(bad("`view` must be 6h / 24h / 7d")),
        },
    }
}

/// 问的是哪一份。
enum Asked<'a> {
    /// 草稿 · 一条规则：没有会话。
    Rotation,
    /// 一个会话此刻那一份。
    Session(&'a str),
    /// 这台全部号（设置里的时间轴）：按默认规则判封顶。
    Machine,
}

/// 一个会话在 `[from, now]` 里走过哪几个号（照换号记录切段；换进那一段的原因带上，头一段没有）。
/// 没换成的那几种（`from == to`）不切；没有记录 ⇒ 从 `since`（不早于 `from`）起一整段此刻的号。
fn past_of(s: &SessionEntry, from: u64, now: u64) -> Vec<(u64, u64, String, Option<SwitchWhy>)> {
    let mut recs: Vec<&SwitchRecord> = s
        .history
        .iter()
        .filter(|h| h.from != h.to && h.at > from && h.at <= now)
        .collect();
    recs.sort_by_key(|h| h.at);
    let mut t = if recs.is_empty() {
        s.since.clamp(from, now)
    } else {
        from
    };
    let mut acct = recs
        .first()
        .map_or_else(|| s.current.clone(), |h| h.from.clone());
    let mut why: Option<SwitchWhy> = None;
    let mut out = Vec::new();
    for h in recs {
        if h.at > t {
            out.push((t, h.at, acct, why));
        }
        t = h.at;
        acct = h.to.clone();
        why = Some(h.why.clone());
    }
    if now > t {
        out.push((t, now, acct, why));
    }
    out
}

/// quota-warm 留下的状态文件（`quota-warm.json`，与 `rotation.json` 同一目录）：写它的进程还活着 ⇒ 号 → 下一次开窗的时刻；
/// 读不到 · 形状不对 · 进程没了 ⇒ 空（整类不画）。
fn warm_of(ctx: &Ctx) -> std::collections::BTreeMap<String, Vec<u64>> {
    let mut out = std::collections::BTreeMap::new();
    let Some(path) = ctx
        .hop
        .store
        .path()
        .and_then(Path::parent)
        .map(|d| d.join("quota-warm.json"))
    else {
        return out;
    };
    let Some(v) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return out;
    };
    let alive = v
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|p| u32::try_from(p).ok())
        .is_some_and(crate::platform::proc::pid_alive);
    if !alive {
        return out;
    }
    for n in v
        .get("next")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let (Some(a), Some(at)) = (
            n.get("account").and_then(Value::as_str),
            n.get("at").and_then(Value::as_u64),
        ) {
            out.entry(a.to_string()).or_insert_with(Vec::new).push(at);
        }
    }
    out
}

pub(crate) fn answer_plan_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let mut v = plan_with(ctx, args, now)?;
    crate::accounts::quota::name_words::with_names(&mut v);
    Ok(v)
}

/// [`answer_plan_with`] 的本体（号名 / 位名那一遍之前）。
fn plan_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let view = view_secs(args)?;
    let (from, until) = match view {
        Some((before, after)) => (now.saturating_sub(before), now + after),
        None => (now, now + span_secs(args)?),
    };
    let (state, why, book) = read_book(ctx);
    let (reason, detail) = crate::stream::detail::unreadable("rotation-plan", why.as_ref());
    let zero = crate::accounts::manage::model::ACCOUNT_ZERO.to_string();
    let (rot, agent, start, current, above, asked) = if let Some(v) = args.get("rotation") {
        let errors = rotation::cell_errors(v);
        if !errors.is_empty() {
            return wire(&PlanReply::Draft { errors });
        }
        let rot = rotation::rotation_from(
            v,
            0..=1,
            &account_ok,
            &|a| ctx.is_api(LIBRARY_AGENT, a),
            None,
        )
        .map_err(|e| bad(&e))?;
        let first = first_of(&rot, &zero);
        (
            rot,
            LIBRARY_AGENT.to_string(),
            zero,
            first,
            Vec::new(),
            Asked::Rotation,
        )
    } else if let Some(id) = rule_id_arg(args, "rule")? {
        let Some(r) = book.rules.get(&id) else {
            return Err(("no_such_rule", copy_text("beRotation.rule.gone", &[])).into());
        };
        let first = first_of(&r.rotation, &zero);
        (
            r.rotation.clone(),
            LIBRARY_AGENT.to_string(),
            zero,
            first,
            Vec::new(),
            Asked::Rotation,
        )
    } else if let Some(sid) = args.get("sid").and_then(Value::as_str) {
        let Some(s) = book.sessions.get(sid) else {
            return Err(bad("no such session"));
        };
        (
            book.rotation_of(s),
            s.agent.clone(),
            s.start.clone(),
            s.current.clone(),
            s.blocked_above.clone(),
            Asked::Session(sid),
        )
    } else if args.get("machine").and_then(Value::as_bool) == Some(true) {
        let lib = ctx.hop.library();
        let ids: Vec<String> = if lib.enabled {
            lib.accounts.iter().map(|a| a.id.clone()).collect()
        } else {
            vec![zero.clone()]
        };
        let mut rot = book.default_rotation();
        rot.order = ids
            .iter()
            .cloned()
            .map(rotation::RotationSlot::Named)
            .collect();
        rot.enabled = ids.clone();
        let first = ids.first().cloned().unwrap_or_else(|| zero.clone());
        (
            rot,
            LIBRARY_AGENT.to_string(),
            zero,
            first,
            Vec::new(),
            Asked::Machine,
        )
    } else {
        return Err(bad("missing `rotation` / `rule` / `sid` / `machine`"));
    };
    let view_obj = ctx.hop.plan_view(
        &rot,
        &agent,
        &start,
        &current,
        &above,
        &|a| (ctx.rows)(&agent, a),
        now,
        until,
    );
    let tz_min = crate::platform::local_tz::offset_secs(now).unwrap_or(0) / 60;
    let text = |t: u64| {
        crate::common::time::fmt_at(
            i64::try_from(t).unwrap_or(i64::MAX),
            i64::try_from(now).unwrap_or(i64::MAX),
            tz_min,
        )
    };
    let seg = |from: u64, to: u64, account: &Option<String>, why: &Option<SwitchWhy>| json!({"from": from, "fromText": text(from), "to": to, "toText": text(to), "account": account, "why": why});
    // 7d 的时间轴：将来那一截只画到 +1d（再往后预测没有根据），其后界面写 `—`。
    let plan_until = match view {
        Some((before, _)) if before >= 86_400 => (now + 86_400).min(until),
        _ => until,
    };
    let plan: Vec<Value> = view_obj
        .steps
        .iter()
        .filter(|p| p.from < plan_until)
        .map(|p| seg(p.from, p.to.min(plan_until), &p.account, &p.why))
        .collect();
    let machine = matches!(asked, Asked::Machine);
    let live = if machine {
        (ctx.live)()
    } else {
        Default::default()
    };
    let warm = if view.is_some() {
        warm_of(ctx)
    } else {
        Default::default()
    };
    let lanes: Vec<Value> = view_obj
        .lanes
        .iter()
        .map(|l| {
            let mut v = json!({
                "account": l.account,
                "spans": l.spans.iter().map(|x| json!({"from": x.from, "fromText": text(x.from), "to": x.to, "toText": text(x.to), "state": x.state, "n": x.n, "w": x.w})).collect::<Vec<_>>(),
                "resets": l.resets.iter().map(|(w, at)| json!({"w": w, "at": at, "atText": text(*at)})).collect::<Vec<_>>(),
                "pct": l.pinch.as_ref().map(|p| p.1),
            });
            if machine {
                v["usedBy"] = json!(book
                    .sessions
                    .iter()
                    .filter(|(sid, s)| s.current == l.account && live.contains(*sid))
                    .count());
            }
            let ats: Vec<Value> = warm
                .get(&l.account)
                .into_iter()
                .flatten()
                .filter(|t| **t > now && **t < until)
                .map(|t| json!({"at": t, "atText": text(*t)}))
                .collect();
            if !ats.is_empty() {
                v["warm"] = Value::Array(ats);
            }
            v
        })
        .collect();
    let effective: Map<String, Value> = view_obj
        .effective
        .iter()
        .map(|(a, cells)| {
            let mut m: Map<String, Value> = cells
                .iter()
                .map(|(k, (at, below))| {
                    let mut v = cap_at_wire(at);
                    v["below"] = cap_at_wire(below);
                    (k.clone(), v)
                })
                .collect();
            // 全部窗口那一格：两窗此刻各取多少 · 这一格不算时各取多少（后端拼好的一句）。
            if let (Some(Value::Object(all)), Some(ls)) =
                (m.get_mut(rotation::ALL_WINDOWS), view_obj.lines.get(a))
            {
                let now_: Vec<_> = ls.iter().map(|(w, at, _)| (*w, *at)).collect();
                let below: Vec<_> = ls.iter().map(|(w, _, b)| (*w, *b)).collect();
                all.insert("list".into(), json!(lines_text(&now_)));
                all.insert("belowList".into(), json!(lines_text(&below)));
            }
            (a.clone(), Value::Object(m))
        })
        .collect();
    let grid = view.map(|(before, _)| grid_of(before, from, until, tz_min * 60, &text));
    let head = (view.is_some() && !machine).then(|| head_of(ctx, &agent, &view_obj, now, &text));
    let past = match (view, &asked) {
        (Some(_), Asked::Session(sid)) => Some(
            past_of(&book.sessions[*sid], from, now)
                .into_iter()
                .map(|(a, b, acct, why)| seg(a, b, &Some(acct), &why))
                .collect(),
        ),
        _ => None,
    };
    wire(&PlanReply::Plan(Box::new(Plan {
        errors: Vec::new(),
        state,
        reason,
        detail,
        now,
        now_text: text(now),
        from,
        from_text: text(from),
        until,
        plan,
        lanes,
        effective,
        grid,
        head,
        past,
    })))
}

/// `rotation-plan` 的应答：草稿有错 ⇒ 只回逐格错；否则整份预览。
#[derive(serde::Serialize)]
#[serde(untagged)]
pub(crate) enum PlanReply {
    Draft { errors: Vec<CellError> },
    Plan(Box<Plan>),
}

/// `rotation-plan` 的整份预览（各格的意思见注册表那一条）。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Plan {
    errors: Vec<CellError>,
    state: &'static str,
    reason: Value,
    detail: Value,
    now: u64,
    now_text: String,
    from: u64,
    from_text: String,
    until: u64,
    plan: Vec<Value>,
    lanes: Vec<Value>,
    effective: Map<String, Value>,
    /// 只在带 `view` 时有。
    #[serde(skip_serializing_if = "Option::is_none")]
    grid: Option<Value>,
    /// 只在带 `view`、问的不是 `machine` 时有。
    #[serde(skip_serializing_if = "Option::is_none")]
    head: Option<Value>,
    /// 只在 `sid` ＋ `view` 时有。
    #[serde(skip_serializing_if = "Option::is_none")]
    past: Option<Vec<Value>>,
}

/// 时间轴的刻度：按这台本地钟对齐的格（6h 视窗一格 15m · 24h 1h · 7d 6h；悬停与键盘按格走，每格带写好的字），
/// 轴上写字的那几格另带 `label`（6h 每小时 · 24h 每 3h：`HH:MM`；7d 每天零点：`MM-DD`）。
fn grid_of(before: u64, from: u64, until: u64, off: i64, text: &dyn Fn(u64) -> String) -> Value {
    let (step, major): (i64, i64) = match before {
        b if b <= 2 * 3600 => (900, 3600),
        b if b <= 6 * 3600 => (3600, 3 * 3600),
        _ => (6 * 3600, 86_400),
    };
    let (from, until) = (
        i64::try_from(from).unwrap_or(i64::MAX),
        i64::try_from(until).unwrap_or(i64::MAX),
    );
    let mut t = (from + off).div_euclid(step) * step - off;
    if t < from {
        t += step;
    }
    let mut out = Vec::new();
    while t <= until {
        let at = u64::try_from(t).unwrap_or(0);
        let mut g = json!({"at": at, "atText": text(at)});
        if (t + off).rem_euclid(major) == 0 {
            let local = (t + off).rem_euclid(86_400);
            g["label"] = json!(if major == 86_400 {
                let (_, m, d) = crate::common::time::civil_from_days((t + off).div_euclid(86_400));
                format!("{m:02}-{d:02}")
            } else {
                format!("{:02}:{:02}", local / 3600, (local % 3600) / 60)
            });
        }
        out.push(g);
        t += step;
    }
    Value::Array(out)
}

/// 时间轴顶行：池里此刻都不能用（被拒 · 过封顶 · 时段停用）或预览说停发 ⇒ `{blocked: {account, at, w?}}`（最早回来的那个号、几点、哪个窗口重置）；
/// 否则 `{account, w, pct, toLine?: {w, n}}`（此刻用的号 · 卡人的窗口与用量 · 离线最近的那一窗还差几点：各窗按这号这窗此刻取的线
/// ——封顶 → 触发，同 `effective`——算，取差得最少的那一窗；都没线 ⇒ 缺）。
fn head_of(
    ctx: &Ctx,
    agent: &str,
    v: &crate::accounts::upstream_select::rotate::PlanView,
    now: u64,
    text: &dyn Fn(u64) -> String,
) -> Value {
    use crate::accounts::quota::decide::LaneState;
    let stuck = |l: &crate::accounts::upstream_select::rotate::PlanLane| {
        l.spans
            .iter()
            .find(|x| {
                x.from <= now
                    && now < x.to
                    && matches!(
                        x.state,
                        LaneState::Refused | LaneState::Capped | LaneState::Off
                    )
            })
            .map(|x| x.to)
    };
    let held = v.steps.first().is_some_and(|p| p.account.is_none());
    let all_stuck = !v.lanes.is_empty() && v.lanes.iter().all(|l| stuck(l).is_some());
    if held || all_stuck {
        let back = v
            .lanes
            .iter()
            .filter_map(|l| stuck(l).map(|t| (t, l)))
            .min_by_key(|(t, _)| *t);
        let Some((at, l)) = back else {
            return json!({"blocked": {}});
        };
        let mut b = json!({"account": l.account, "at": at, "atText": text(at)});
        if let Some(rel) = crate::common::time::fmt_rel(
            i64::try_from(at).unwrap_or(i64::MAX),
            i64::try_from(now).unwrap_or(i64::MAX),
        ) {
            b["atRelText"] = json!(rel);
        }
        if let Some((w, _)) = l.resets.iter().find(|(_, t)| *t == at) {
            b["w"] = json!(w);
        }
        return json!({ "blocked": b });
    }
    let Some(account) = v.steps.first().and_then(|p| p.account.clone()) else {
        return json!({});
    };
    let mut h = json!({ "account": account });
    if let Some((w, pct)) = ctx.hop.pinch(agent, &account, now) {
        h["w"] = json!(w);
        h["pct"] = json!(pct);
        if let Some((lw, n)) = v
            .lanes
            .iter()
            .find(|l| l.account == account)
            .and_then(|l| l.to_line.as_ref())
        {
            h["toLine"] = json!({"w": lw, "n": n});
        }
        // 「估」：到的是这号这窗口此刻取的上限（封顶 → 触发；都没有 ⇒ 满）；没根据（[`Observed::eta`] 那几条）就不给。
        let target = v
            .effective
            .get(&account)
            .and_then(|m| m.get(&w).or_else(|| m.get("*")))
            .and_then(|(at, _)| at.v)
            .map_or(100, u32::from);
        if let Some(at) = ctx.hop.eta(agent, &account, target, now) {
            h["est"] = json!({"at": at, "atText": text(at), "pct": target, "w": w});
        }
    }
    h
}

fn sids_of(args: &Value, key: &str) -> Result<Vec<String>, Fail> {
    let arr = args
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| bad(&format!("`{key}` must be an array of session ids")))?;
    arr.iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_str()
                .filter(|s| shell_quote_core::session_id_ok(s))
                .map(str::to_string)
                .ok_or_else(|| bad(&format!("`{key}[{i}]` must be a session id")))
        })
        .collect()
}

/// `rotation-session-read`：一批会话各自的那一份（`{sids}`）；这台没见过的照实标 `absent`，不整批失败。
/// 出口那一下给每个时刻添好显示的字（同 [`answer_quota_read`]）。
pub(crate) fn answer_session_read(args: &Value) -> Answer {
    let now = crate::accounts::quota::now_unix();
    let mut v = answer_session_read_with(&Ctx::here(), args, now)?;
    crate::common::time::with_texts_here(&mut v, now);
    Ok(v)
}

pub(crate) fn answer_session_read_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let sids = sids_of(args, "sids")?;
    let (state, why, book) = read_book(ctx);
    let (reason, detail) = crate::stream::detail::unreadable("rotation-session-read", why.as_ref());
    let live = (ctx.live)();
    let lineage = ctx.lineage.now();
    let mut sessions = Map::new();
    for sid in sids {
        let agent = book.sessions.get(&sid).map(|s| s.agent.clone());
        let row = |a: &str| agent.as_deref().and_then(|g| (ctx.rows)(g, a));
        let parent = lineage.parent_of(&sid);
        let one = ctx
            .hop
            .view(&book, &sid, parent, &row, &|x| live.contains(x), now);
        sessions.insert(
            sid,
            serde_json::to_value(one).map_err(|e| ("failed", e.to_string()))?,
        );
    }
    let mut v = wire::<_, Fail>(&SessionRead {
        state,
        reason,
        detail,
        now,
        sessions,
    })?;
    crate::accounts::quota::name_words::with_names(&mut v);
    Ok(v)
}

/// `rotation-session-read` 的应答。
#[derive(serde::Serialize)]
pub(crate) struct SessionRead {
    state: &'static str,
    reason: Value,
    detail: Value,
    now: u64,
    /// 每个 sid 一份。
    sessions: Map<String, Value>,
}

/// `rotation-session-set`：一批会话的轮换来源（`{sids, rotation}`；`rotation` ＝ `"follow"` · `{"rule": id}` ·
/// `"custom"`（恢复上一份自己的，没有就照此刻生效的那份拷）· `"detach"`（照此刻生效的那份拷成本会话的 ＝「转为本会话」）·
/// `{"custom": {order, enabled, cap?, …}}`）。这台没见过的会话要另给 `agent` 与 `start`（起它的号）才记得下。
/// 回每个会话的结果。指向的规则不在 ⇒ `no_such_rule`（整批不写）。
pub(crate) fn answer_session_set(args: &Value) -> Answer {
    answer_session_set_with(&Ctx::here(), args, crate::accounts::quota::now_unix())
}

/// 要写成什么样。
enum Want {
    Follow,
    /// 跟随父会话（父按血缘填）。
    Parent,
    Rule(String),
    /// 恢复自己那份（没有 ⇒ 照此刻生效的那份拷）。
    Restore,
    /// 照此刻生效的那份拷。
    Detach,
    Custom(Value),
}

pub(crate) fn answer_session_set_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let sids = sids_of(args, "sids")?;
    let want = match args.get("rotation") {
        Some(Value::String(s)) if s == "follow" => Want::Follow,
        Some(Value::String(s)) if s == "parent" => Want::Parent,
        Some(Value::String(s)) if s == "custom" => Want::Restore,
        Some(Value::String(s)) if s == "detach" => Want::Detach,
        Some(Value::Object(m)) if m.len() == 1 && m.contains_key("custom") => {
            Want::Custom(m["custom"].clone())
        }
        Some(Value::Object(m)) if m.len() == 1 && m.contains_key("rule") => {
            Want::Rule(rule_id_arg(args.get("rotation").unwrap_or(&Value::Null), "rule")?.unwrap_or_default())
        }
        _ => {
            return Err(bad(
                "`rotation` must be \"follow\", \"parent\", \"custom\", \"detach\", {\"rule\": id} or {\"custom\": {order, enabled, cap?}}",
            ))
        }
    };
    let named = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .filter(|s| account_ok(s) || *s == "_")
            .map(str::to_string)
    };
    let (agent, start) = (named("agent"), named("start"));
    store_path(ctx)?;
    let book = ctx.hop.store.now();
    if let Want::Rule(id) = &want {
        if !book.rules.contains_key(id) {
            return Err(("no_such_rule", copy_text("beRotation.rule.gone", &[])).into());
        }
    }
    // 跟随父会话：每个都得在血缘里有父，且父（在账本里的话）与它同一家；有一个不成立 ⇒ 整批 `no_parent`。
    let lineage = ctx.lineage.now();
    let parent_of = |sid: &str| -> Option<String> {
        let p = lineage.parent_of(sid)?;
        let mine = book
            .sessions
            .get(sid)
            .map(|s| s.agent.as_str())
            .or(agent.as_deref());
        let theirs = book.sessions.get(p).map(|s| s.agent.as_str());
        match (mine, theirs) {
            (Some(m), Some(t)) if m != t => None,
            _ => Some(p.to_string()),
        }
    };
    if matches!(want, Want::Parent) && sids.iter().any(|sid| parent_of(sid).is_none()) {
        return Err(("no_parent", copy_text("beRotation.parent.none", &[])).into());
    }
    // 先把要写的每一份都判完（不合法整批拒、一个字节不写），再一次写进去。
    let mut plan: Vec<(String, Source, Option<Rotation>)> = Vec::new();
    let mut outcomes = Map::new();
    for sid in &sids {
        let s = book.sessions.get(sid);
        let (g, effective) = match (s, &agent, &start) {
            (Some(s), _, _) => (s.agent.clone(), book.rotation_of(s)),
            (None, Some(g), Some(_)) => (g.clone(), book.default_rotation()),
            (None, _, _) => {
                outcomes.insert(
                    sid.clone(),
                    outcome(SwitchOutcome::Skipped {
                        code: "noRelay".into(),
                    })?,
                );
                continue;
            }
        };
        let (source, custom) = match &want {
            Want::Follow => (Source::Follow, None),
            Want::Parent => (Source::Parent(parent_of(sid).unwrap_or_default()), None),
            Want::Rule(id) => (Source::Rule(id.clone()), None),
            Want::Restore => (
                Source::Custom,
                Some(s.and_then(|s| s.custom.clone()).unwrap_or(effective)),
            ),
            Want::Detach => (Source::Custom, Some(effective)),
            Want::Custom(v) => (
                Source::Custom,
                Some(
                    rotation::rotation_from(
                        v,
                        0..=1,
                        &account_ok,
                        &|a| ctx.is_api(&g, a),
                        Some(&effective),
                    )
                    .map_err(|e| bad(&e))?,
                ),
            ),
        };
        plan.push((sid.clone(), source, custom));
    }
    rotation::face_change(&ctx.hop.store, |b| {
        for (sid, source, custom) in &plan {
            if !b.sessions.contains_key(sid) {
                if let (Some(g), Some(st)) = (&agent, &start) {
                    b.sessions
                        .insert(sid.clone(), SessionEntry::fresh(g, st, now));
                }
            }
            if let Some(s) = b.sessions.get_mut(sid) {
                s.source = source.clone();
                if custom.is_some() {
                    s.custom.clone_from(custom);
                }
            }
        }
    })
    .map_err(|e| ("io_failed", e))?;
    for (sid, _, _) in plan {
        outcomes.insert(sid, outcome(SwitchOutcome::Switched)?);
    }
    wire(&SessionSet { sessions: outcomes })
}

/// `rotation-session-set` 的应答：逐个结果。
#[derive(serde::Serialize)]
pub(crate) struct SessionSet {
    sessions: Map<String, Value>,
}

pub(crate) fn outcome(o: SwitchOutcome) -> Result<Value, (&'static str, String)> {
    serde_json::to_value(o).map_err(|e| ("failed", e.to_string()))
}

/// 不重启换一个会话：中转见过它 · 这一家有账号 · 这台建了账号库 · 目标号接得上 ⇒ 钉过去、记一条。
pub(crate) fn hot_one(ctx: &Ctx, sid: &str, target: &str, now: u64) -> SwitchOutcome {
    let book = ctx.hop.store.now();
    let skipped = |code: &str| SwitchOutcome::Skipped { code: code.into() };
    let Some(s) = book.sessions.get(sid) else {
        return skipped("noRelay");
    };
    if !(ctx.live)().contains(sid) {
        return skipped("ended");
    }
    if crate::agents::login_of(&s.agent).is_none() {
        return skipped("agentHasNoAccounts");
    }
    if !ctx.hop.library().enabled {
        return skipped("machineNotMulti");
    }
    if s.current == target {
        return SwitchOutcome::Switched;
    }
    let row = |a: &str| (ctx.rows)(&s.agent, a);
    if let Err(u) = ctx.hop.check_target(&s.agent, &s.start, target, &row, now) {
        let code = match u {
            Unready::NeedsKey => "targetNeedsKey",
            Unready::NeedsLogin | Unready::UnsureBody => "targetNeedsLogin",
        };
        return SwitchOutcome::NotSwitched { code: code.into() };
    }
    record(ctx, sid, &s.current, target, SwitchWhy::ManualHot, now)
}

/// 记一条手动换号（钉到 `to`）；写不进 ⇒ 没成。
pub(crate) fn record(
    ctx: &Ctx,
    sid: &str,
    from: &str,
    to: &str,
    why: SwitchWhy,
    now: u64,
) -> SwitchOutcome {
    let agent = ctx
        .hop
        .store
        .now()
        .sessions
        .get(sid)
        .map(|s| s.agent.clone())
        .unwrap_or_default();
    let resets = ctx
        .hop
        .quota
        .entry(&agent, from)
        .and_then(|o| o.reading.resets_at);
    let rec = SwitchRecord {
        at_text: None,
        from_resets_at_text: None,
        at: now,
        from: from.to_string(),
        to: to.to_string(),
        why,
        from_resets_at: resets,
    };
    let base = ctx.hop.baseline_of(&agent, to, now);
    let book = ctx.hop.store.now();
    let above = book
        .sessions
        .get(sid)
        .map(|s| {
            ctx.hop
                .above_at(&book, s, to, &|a| (ctx.rows)(&agent, a), now)
        })
        .unwrap_or_default();
    match rotation::face_change(&ctx.hop.store, |b| {
        b.pin(sid, rec, &[]);
        b.rebase(sid, &base);
        b.block_above(sid, &above);
    }) {
        Ok(()) => SwitchOutcome::Switched,
        Err(e) => {
            tracing::warn!("[rotate] {}", e.logged());
            SwitchOutcome::NotSwitched {
                code: "ioFailed".into(),
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_face_tests.rs"]
mod tests;
