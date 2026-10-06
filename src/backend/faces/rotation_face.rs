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
use copy_core::copy_text;
use serde_json::{json, Map, Value};
use std::sync::Arc;

type Answer = Result<Value, (&'static str, String)>;

/// 一条命令要的几样：轮换的账本（经它判、经它写）· 这台 key 表里有哪几行（按量号接不接得上）。
/// `(agent, 号)` → 这台 key 表里那一行：没有 ⇒ `None`；有 ⇒ 接不接得上。
type Rows = Box<dyn Fn(&str, &str) -> Option<bool> + Send + Sync>;

/// 这台此刻活着的会话（sid 集合）。
type Live = Box<dyn Fn() -> std::collections::BTreeSet<String> + Send + Sync>;

pub(crate) struct Ctx {
    pub(crate) hop: Hop,
    pub(crate) rows: Rows,
    pub(crate) live: Live,
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

fn bad(detail: &str) -> (&'static str, String) {
    ("bad_args", crate::common::contract::malformed(detail))
}

fn store_path(ctx: &Ctx) -> Result<&Path, (&'static str, String)> {
    ctx.hop
        .store
        .path()
        .ok_or_else(|| ("io_failed", copy_text("beRotation.read.noHome", &[])))
}

/// 盘上那一份（三态，同额度账）。
fn read_book(ctx: &Ctx) -> (&'static str, Option<String>, Book) {
    match ctx.hop.store.path().map(rotation::read_at) {
        None => (
            "unreadable",
            Some(copy_text("beRotation.read.noHome", &[])),
            Book::default(),
        ),
        Some(rotation::Read::Absent) => ("absent", None, Book::default()),
        Some(rotation::Read::Present(b)) => ("present", None, b),
        Some(rotation::Read::Unreadable(e)) => ("unreadable", Some(e), Book::default()),
    }
}

/// 默认轮换的线上形状（读与写回同一形）。`followers` ＝ 跟随它的活会话有几个。
fn default_wire(ctx: &Ctx) -> Value {
    let (state, reason, book) = read_book(ctx);
    let live = (ctx.live)();
    let followers = book
        .sessions
        .iter()
        .filter(|(sid, s)| s.follow && live.contains(*sid))
        .count();
    json!({
        "state": state,
        "reason": reason,
        "path": ctx.hop.store.path().map(|p| p.display().to_string()),
        "rotation": book.default_rotation(),
        "followers": followers,
    })
}

/// 凭据文件那一家（账号库是它的）：默认轮换里新勾的按量号按它判。
const LIBRARY_AGENT: &str = crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT;

/// `quota-read`：这台的额度账，每条带上显示态（「快满」按这台默认轮换的 N）；另给账号库里从没出过数的号、
/// 此刻发得出去的号、最早回来的那个。
pub(crate) fn answer_quota_read() -> Value {
    quota_read_with(&Ctx::here(), crate::accounts::quota::now_unix())
}

pub(crate) fn quota_read_with(ctx: &Ctx, now: u64) -> Value {
    let mut v = ledger::answer_of(ctx.hop.quota.path(), now);
    let seen: Vec<ledger::Observed> =
        serde_json::from_value(v["accounts"].clone()).unwrap_or_default();
    let lib = ctx.hop.library();
    let n = show::near_of(ctx.hop.store.now().default_rotation().when);
    let shown = |agent: &str, account: &str, o: Option<&ledger::Observed>| {
        let slot = crate::agents::window_slot_of(agent);
        let key = crate::agents::window_key_of(agent);
        let mut sh = show::show(
            o.map(|o| (&o.reading, o.seen_at)),
            ctx.hop
                .show_facts(agent, &lib, account, &|a| (ctx.rows)(agent, a)),
            n,
            now,
            &|w| slot.and_then(|f| f(w)),
        );
        if let Some(o) = o {
            sh.windows = show::windows_of(o, &|w| key.and_then(|f| f(w)), now);
        }
        sh
    };
    let mut usable: Vec<String> = Vec::new();
    let mut earliest: Option<(u64, String)> = None;
    let mut rows: Vec<Value> = Vec::new();
    for o in &seen {
        let sh = shown(&o.agent, &o.account, Some(o));
        if show::usable(&sh) {
            usable.push(o.account.clone());
        }
        if let Some(at) = show::back_at(&sh, &o.reading, now) {
            if earliest.as_ref().is_none_or(|(t, _)| at < *t) {
                earliest = Some((at, o.account.clone()));
            }
        }
        let mut row = serde_json::to_value(o).unwrap_or_default();
        if let (Some(r), Value::Object(m)) = (
            row.as_object_mut(),
            serde_json::to_value(&sh).unwrap_or_default(),
        ) {
            r.extend(m);
        }
        rows.push(row);
    }
    let unseen: Vec<Value> = lib
        .accounts
        .iter()
        .filter(|a| !seen.iter().any(|o| o.agent == LIBRARY_AGENT && o.account == a.id))
        .map(|a| {
            let sh = shown(LIBRARY_AGENT, &a.id, None);
            if show::usable(&sh) {
                usable.push(a.id.clone());
            }
            let mut one = json!({"agent": LIBRARY_AGENT, "account": a.id, "kind": sh.kind, "login": sh.login});
            if let Some(id) = sh.sub_id {
                one["subId"] = json!(id);
            }
            one
        })
        .collect();
    v["accounts"] = Value::Array(rows);
    v["unseen"] = Value::Array(unseen);
    v["usableNow"] = json!(usable);
    v["earliestReturn"] = earliest.map_or(
        Value::Null,
        |(at, account)| json!({"account": account, "at": at}),
    );
    v
}

/// `rotation-read`：这台的默认轮换。
pub(crate) fn answer_read() -> Answer {
    Ok(answer_read_with(&Ctx::here()))
}

pub(crate) fn answer_read_with(ctx: &Ctx) -> Value {
    default_wire(ctx)
}

/// `rotation-set`：整份写回这台的默认轮换（`{rotation}`）；不合法整份拒、说哪一格。
pub(crate) fn answer_set(args: &Value) -> Answer {
    answer_set_with(&Ctx::here(), args)
}

pub(crate) fn answer_set_with(ctx: &Ctx, args: &Value) -> Answer {
    let v = args
        .get("rotation")
        .ok_or_else(|| bad("missing `rotation`"))?;
    store_path(ctx)?;
    let prior = ctx.hop.store.now().default_rotation();
    let r = rotation::rotation_from(
        v,
        1..=1,
        &account_ok,
        &|a| ctx.is_api(LIBRARY_AGENT, a),
        Some(&prior),
    )
    .map_err(|e| bad(&e))?;
    rotation::face_change(&ctx.hop.store, |b| b.default = Some(r)).map_err(|e| ("io_failed", e))?;
    Ok(default_wire(ctx))
}

fn sids_of(args: &Value, key: &str) -> Result<Vec<String>, (&'static str, String)> {
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
pub(crate) fn answer_session_read(args: &Value) -> Answer {
    answer_session_read_with(&Ctx::here(), args, crate::accounts::quota::now_unix())
}

pub(crate) fn answer_session_read_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let sids = sids_of(args, "sids")?;
    let (state, reason, book) = read_book(ctx);
    let live = (ctx.live)();
    let mut sessions = Map::new();
    for sid in sids {
        let agent = book.sessions.get(&sid).map(|s| s.agent.clone());
        let row = |a: &str| agent.as_deref().and_then(|g| (ctx.rows)(g, a));
        let one = ctx.hop.view(&book, &sid, &row, &|x| live.contains(x), now);
        sessions.insert(
            sid,
            serde_json::to_value(one).map_err(|e| ("failed", e.to_string()))?,
        );
    }
    Ok(json!({"state": state, "reason": reason, "now": now, "sessions": sessions}))
}

/// `rotation-session-set`：一批会话的轮换（`{sids, rotation}`；`rotation` ＝ `"follow"` · `"custom"`（恢复上一份自己的，
/// 没有就从默认起）· `{"custom": {order, enabled, when}}`）。这台没见过的会话要另给 `agent` 与 `start`（起它的号）才记得下。
/// 回每个会话的结果。
pub(crate) fn answer_session_set(args: &Value) -> Answer {
    answer_session_set_with(&Ctx::here(), args, crate::accounts::quota::now_unix())
}

/// 要写成什么样。
enum Want {
    Follow,
    Custom(Option<Value>),
}

pub(crate) fn answer_session_set_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let sids = sids_of(args, "sids")?;
    let want =
        match args.get("rotation") {
            Some(Value::String(s)) if s == "follow" => Want::Follow,
            Some(Value::String(s)) if s == "custom" => Want::Custom(None),
            Some(Value::Object(m)) if m.len() == 1 && m.contains_key("custom") => {
                Want::Custom(Some(m["custom"].clone()))
            }
            _ => return Err(bad(
                "`rotation` must be \"follow\", \"custom\" or {\"custom\": {order, enabled, when}}",
            )),
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
    // 先把要写的每一份都判完（不合法整批拒、一个字节不写），再一次写进去。
    let mut plan: Vec<(String, Option<Rotation>)> = Vec::new();
    let mut outcomes = Map::new();
    for sid in &sids {
        let s = book.sessions.get(sid);
        let (g, prior) = match (s, &agent, &start) {
            (Some(s), _, _) => (s.agent.clone(), Some(book.rotation_of(s))),
            (None, Some(g), Some(_)) => (g.clone(), None),
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
        let custom = match &want {
            Want::Follow => None,
            Want::Custom(None) => Some(
                s.and_then(|s| s.custom.clone())
                    .unwrap_or_else(|| book.default_rotation()),
            ),
            Want::Custom(Some(v)) => Some(
                rotation::rotation_from(
                    v,
                    0..=1,
                    &account_ok,
                    &|a| ctx.is_api(&g, a),
                    prior.as_ref(),
                )
                .map_err(|e| bad(&e))?,
            ),
        };
        plan.push((sid.clone(), custom));
    }
    let follow = matches!(want, Want::Follow);
    rotation::face_change(&ctx.hop.store, |b| {
        for (sid, custom) in &plan {
            if !b.sessions.contains_key(sid) {
                if let (Some(g), Some(st)) = (&agent, &start) {
                    b.sessions
                        .insert(sid.clone(), SessionEntry::fresh(g, st, now));
                }
            }
            if let Some(s) = b.sessions.get_mut(sid) {
                s.follow = follow;
                if custom.is_some() {
                    s.custom.clone_from(custom);
                }
            }
        }
    })
    .map_err(|e| ("io_failed", e))?;
    for (sid, _) in plan {
        outcomes.insert(sid, outcome(SwitchOutcome::Switched)?);
    }
    Ok(json!({ "sessions": outcomes }))
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
            tracing::warn!("[rotate] {e}");
            SwitchOutcome::NotSwitched {
                code: "ioFailed".into(),
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_face_tests.rs"]
mod tests;
