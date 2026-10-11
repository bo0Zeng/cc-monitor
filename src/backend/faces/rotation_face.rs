//! 换号那一族的帧面宿主：把账号库（`accounts/manage`）读成上游选择换号要的那几格交进去 —— 上游选择与账号库管理互不认识，
//! 两边只在这里接上。命令（`轮换-积木.md` §5）：配置表（读 · 存 · 改名 · 删 · 设为本机默认）· 策略（读 · 装 · 删 · 试跑）·
//! 会话（读 · 打开一份配置 · 改副本 · 存 · 还原 · 手动调整）· 推演。都只照后端算好的回，界面不跑策略。

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

use crate::accounts::quota::decide::{self, Kind, Ranked, Verdict};
use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::rotation::doc::{self, Diff, Edit, Saved, Set};
use crate::accounts::quota::rotation::{
    self, Blocked, Book, CellError, Config, ConfigBody, Hot, Leave, Manual, Params, Place,
    RotationStore, SessionEntry, SwitchOutcome, SwitchRecord, Unready, Who,
};
use crate::accounts::quota::show;
use crate::accounts::quota::strategy::ctx::Facts;
use crate::accounts::quota::strategy::form::Shown;
use crate::accounts::quota::strategy::words::Speak;
use crate::accounts::quota::strategy::{self, Fail as StrategyFail, FailKind, Lang, Strategy};
use crate::accounts::upstream_select::rotate::{account_ok, Hop};
use crate::common::time::TextClock;
use crate::stream::inbound::spec::wire;
use copy_core::copy_text;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// 失败带码 ＋ 那一句 ＋ 原话。
type Answer = Result<Value, Fail>;
use crate::stream::inbound::spec::Fail;

/// `(agent, 号)` → 这台 key 表里那一行：没有 ⇒ `None`；有 ⇒ 接不接得上。
type Rows = Box<dyn Fn(&str, &str) -> Option<bool> + Send + Sync>;

/// 这台此刻活着的会话（sid 集合）。
type Live = Box<dyn Fn() -> std::collections::BTreeSet<String> + Send + Sync>;

/// 一条命令要的几样：轮换的账本（经它判、经它写）· 这台 key 表里有哪几行 · 活着的会话 · 看的那一台的时区。
pub(crate) struct Ctx {
    pub(crate) hop: Hop,
    pub(crate) rows: Rows,
    pub(crate) live: Live,
    /// 看的那一台的时区（请求信封的 `tz`）：写配置那一下盖进配置，字按它写。
    pub(crate) tz: crate::Tz,
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
            tz: crate::Tz::default(),
        }
    }

    /// 这一问看的那一台的时区。
    pub(crate) fn viewing(mut self, tz: &crate::Tz) -> Self {
        self.tz = tz.clone();
        self
    }

    fn kind(&self, agent: &str, a: &str) -> Kind {
        let api = (self.rows)(agent, a).is_some()
            || self
                .hop
                .library()
                .accounts
                .iter()
                .any(|x| x.id == a && x.api);
        if api {
            Kind::Api
        } else {
            Kind::Sub
        }
    }

    /// 此刻加载的全部策略。
    fn strategies(&self) -> Arc<Vec<Arc<Strategy>>> {
        strategy::all(self.hop.store.strategy_dir().as_deref())
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
        Some(rotation::Read::Present(_)) => ("present", None, ctx.hop.store.now()),
        // 读不懂 ⇒ 照上一份读懂的那一份回（引擎也照它挑号），盘上那份不动。
        Some(rotation::Read::Unreadable(e)) => ("unreadable", Some(e), ctx.hop.store.now()),
    }
}

/// 凭据文件那一家（账号库是它的）：没有会话的那几问（配置 · 草稿 · 推演）按它判种类与事实。
const LIBRARY_AGENT: &str = crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT;

fn lang_of(args: &Value) -> Lang {
    Lang::of(args.get("lang").and_then(Value::as_str))
}

fn str_arg(args: &Value, k: &str) -> Result<String, Fail> {
    args.get(k)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| bad(&format!("`{k}` must be a non-empty string")))
}

fn sid_arg(args: &Value) -> Result<String, Fail> {
    args.get("sid")
        .and_then(Value::as_str)
        .filter(|s| shell_quote_core::session_id_ok(s))
        .map(str::to_string)
        .ok_or_else(|| bad("`sid` must be a session id"))
}

fn if_rev(args: &Value) -> Result<Option<u64>, Fail> {
    match args.get("ifRev") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| bad("`ifRev` must be an integer")),
    }
}

fn flag(args: &Value, k: &str) -> Result<bool, Fail> {
    match args.get(k) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(bad(&format!("`{k}` must be true or false"))),
    }
}

fn now_secs(t: u64) -> i64 {
    i64::try_from(t).unwrap_or(i64::MAX)
}

// ── quota-read ───────────────────────────────────────────────────────────────────────

/// `quota-read`：这台的额度账，每条带上显示态（「快满」按本机默认那份配置里这个号那一行的线）；另给账号库里从没出过数的号、
/// 此刻发得出去的号、最早回来的那个。出口那一下给每个时刻添好显示的字、每号添好几行与开窗那一判。
pub(crate) fn answer_quota_read(tz: &crate::Tz) -> Answer {
    answer_quota_read_with(&Ctx::here(), crate::accounts::quota::now_unix(), tz)
}

/// [`answer_quota_read`] 的本体。
pub(crate) fn answer_quota_read_with(ctx: &Ctx, now: u64, tz: &crate::Tz) -> Answer {
    wire::<_, Fail>(&quota_read_with(ctx, now, tz))
}

pub(crate) use crate::faces::quota_read::QuotaRead;

/// 一份配置里一个号照哪一行的参数：点名的那一行；没点名的订阅号 ⇒ 「其余的订阅号」那一行；都没有 ⇒ `None`。
fn row_params_of<'a>(body: &'a ConfigBody, account: &str, kind: Kind) -> Option<&'a Params> {
    body.row(&Who::Id(account.to_string()))
        .or_else(|| (kind == Kind::Sub).then(|| body.row(&Who::Rest)).flatten())
        .map(|r| &r.params)
}

/// 读这台的额度账、给每个号判显示态，交出口写成成品。
pub(crate) fn quota_read_with(ctx: &Ctx, now: u64, tz: &crate::Tz) -> QuotaRead {
    use crate::faces::quota_read::{reply_of, Base, Seen, UnseenHead};
    let base = ledger::answer_of(ctx.hop.quota.path(), now);
    let seen = &base.accounts;
    let lib = ctx.hop.library();
    let book = ctx.hop.store.now();
    let default = book
        .configs
        .get(&book.default_config)
        .map(|c| c.body.clone());
    let shown = |agent: &str, account: &str, o: Option<&ledger::Observed>| {
        let slot = crate::agents::window_slot_of(agent);
        let key = crate::agents::window_key_of(agent);
        let slot = |w: &str| slot.and_then(|f| f(w));
        let key = |w: &str| key.and_then(|f| f(w));
        let params = default
            .as_ref()
            .and_then(|b| row_params_of(b, account, ctx.kind(agent, account)));
        let clock = default.as_ref().map_or(0, |b| b.clock_at(now));
        let mut sh = show::show(
            o.map(|o| (&o.reading, o.seen_at)),
            ctx.hop
                .show_facts(agent, &lib, account, &|a| (ctx.rows)(agent, a)),
            &show::lines_of(params, clock, &slot),
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
                usage: sh.usage.into(),
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
        &TextClock::new(now, tz),
    )
}

// ── 描述 · 写字 ─────────────────────────────────────────────────────────────────────────

/// 一份配置写成的那一句描述（策略的 `describe` 片段写成字）；策略读不懂 ⇒ 照内置策略 1 写、后面接「策略出错」。
fn describe(
    ctx: &Ctx,
    agent: &str,
    body: &ConfigBody,
    s: Option<&Strategy>,
    lang: Lang,
    now: u64,
) -> String {
    let (s, broken) = match s.filter(|s| s.usable()) {
        Some(s) => (Arc::new(s.clone()), false),
        None => match strategy::fallback() {
            Some(f) => (f, true),
            None => return String::new(),
        },
    };
    let b = doc::filled(body, &s);
    let accounts: Vec<Value> = b
        .accounts
        .iter()
        .map(|r| {
            let kind = match &r.who {
                Who::Id(a) => ctx.kind(agent, a),
                _ => Kind::Sub,
            };
            json!({"id": r.who.key(), "kind": kind, "params": r.params})
        })
        .collect();
    let Some(ast) = s.ast.as_deref() else {
        return String::new();
    };
    let (d, _) = strategy::call(
        ast,
        "describe",
        (
            rhai::serde::to_dynamic(Value::Object(b.params.clone())).unwrap_or_default(),
            rhai::serde::to_dynamic(Value::Array(accounts)).unwrap_or_default(),
        ),
    );
    let frags = d
        .ok()
        .and_then(|d| rhai::serde::from_dynamic::<Value>(&d).ok())
        .unwrap_or(Value::Null);
    let mut text = Speak::of(&s, lang, now, &ctx.tz).describe(&frags);
    if broken {
        text = [text, strategy::words::broken_text()]
            .into_iter()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(&copy_text("beRotation.desc.sep", &[]));
    }
    text
}

/// 策略出错那一句（面板红条照抄）。
fn fail_text(name: &str, f: &StrategyFail) -> String {
    match f.kind {
        FailKind::Broke => copy_text(
            "beRotation.strategy.failed",
            &[("name", name), ("line", &f.line.unwrap_or(0).to_string())],
        ),
        FailKind::Timeout => copy_text("beRotation.strategy.timeout", &[("name", name)]),
        FailKind::BadShape => copy_text(
            "beRotation.strategy.badShape",
            &[("name", name), ("what", &f.msg)],
        ),
        FailKind::Gone => copy_text("beRotation.strategy.gone", &[("name", name)]),
    }
}

/// 出错那一格的线上样子：哪一种 · 第几行 · 原话 · 写好的那一句。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct StrategyError {
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub fail: StrategyFail,
    pub text: String,
}

fn strategy_name(all: &[Arc<Strategy>], id: &str) -> String {
    strategy::find(all, id).map_or_else(|| id.to_string(), |s| s.name.clone())
}

/// 换号记录写成字：引擎那一半（`leave`）＋ 策略那一半（`pick`，照那份策略的字）。
fn record_text(
    r: &SwitchRecord,
    all: &[Arc<Strategy>],
    lang: Lang,
    now: u64,
    tz: &crate::Tz,
) -> String {
    let why_of = |s: &Strategy, w: &Value| Speak::of(s, lang, now, tz).why(w);
    let one = strategy::fallback();
    let leave = match &r.leave {
        Leave::Outranked => copy_text("beRotation.leave.top", &[]),
        Leave::Unusable { why } => {
            let w = one.as_ref().map(|s| why_of(s, why)).unwrap_or_default();
            copy_text("beRotation.leave.unusable", &[("why", &w)])
        }
        Leave::Manual => copy_text("beRotation.leave.manual", &[]),
        Leave::ManualRestart => copy_text("beRotation.leave.manualRestart", &[]),
        Leave::Refresh => copy_text("beRotation.leave.refresh", &[]),
        Leave::Held => copy_text("beRotation.leave.held", &[]),
        Leave::Overage => copy_text(
            "beRotation.leave.overage",
            &[(
                "acct",
                &crate::accounts::quota::name_words::account_text(&r.from),
            )],
        ),
    };
    let pick = r
        .pick
        .as_ref()
        .and_then(|p| strategy::find(all, &p.strategy).map(|s| why_of(&s, &p.why)))
        .filter(|s| !s.is_empty());
    match pick {
        Some(p) => format!("{leave}{}{p}", copy_text("beRotation.desc.sep", &[])),
        None => leave,
    }
}

// ── 配置表 ───────────────────────────────────────────────────────────────────────────

/// 配置列表里的一项：存下的那一份 ＋ 后端写好的描述 · 策略名 · 开着它的会话 · 是不是本机默认。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ConfigItem {
    pub id: String,
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub config: Config,
    pub text: String,
    pub strategy_name: String,
    pub in_use: Vec<String>,
    pub is_default: bool,
}

/// `rotation-configs-read` 的应答。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ConfigsRead {
    /// `present` · `absent`（没动过：内置三份）· `unreadable`。
    pub state: String,
    #[cfg_attr(test, ts(type = "string | null"))]
    pub reason: Value,
    #[cfg_attr(test, ts(type = "string | null"))]
    pub detail: Value,
    /// 默认那份在最前、其余按名字。
    pub configs: Vec<ConfigItem>,
    pub default_config: String,
    #[cfg_attr(test, ts(type = "string | null"))]
    pub path: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn config_item(
    ctx: &Ctx,
    book: &Book,
    all: &[Arc<Strategy>],
    id: &str,
    c: &Config,
    lang: Lang,
    now: u64,
) -> ConfigItem {
    let s = strategy::find(all, &c.body.strategy);
    ConfigItem {
        id: id.to_string(),
        config: c.clone(),
        text: describe(ctx, LIBRARY_AGENT, &c.body, s.as_deref(), lang, now),
        strategy_name: strategy_name(all, &c.body.strategy),
        in_use: book.in_use(id),
        is_default: book.default_config == id,
    }
}

/// `rotation-configs-read`：这台的配置表（默认那份在最前）。
pub(crate) fn answer_configs_read(args: &Value, tz: &crate::Tz) -> Answer {
    answer_configs_read_with(
        &Ctx::here().viewing(tz),
        args,
        crate::accounts::quota::now_unix(),
    )
}

pub(crate) fn answer_configs_read_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let (state, why, book) = read_book(ctx);
    let (reason, detail) = crate::stream::detail::unreadable("rotation-configs-read", why.as_ref());
    let all = ctx.strategies();
    let lang = lang_of(args);
    let mut items: Vec<ConfigItem> = book
        .configs
        .iter()
        .map(|(id, c)| config_item(ctx, &book, &all, id, c, lang, now))
        .collect();
    items.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| a.config.body.name.cmp(&b.config.body.name))
    });
    wire(&ConfigsRead {
        state: state.to_string(),
        reason,
        detail,
        configs: items,
        default_config: book.default_config.clone(),
        path: ctx.hop.store.path().map(|p| p.display().to_string()),
    })
}

/// 存一份配置的结局（设置窗的配置编辑器 · 改名）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum ConfigSaved {
    /// 写成了：id · 新的版本 · 写成那一刻的钟面 `HH:MM`。
    #[serde(rename_all = "camelCase")]
    Saved {
        id: String,
        #[cfg_attr(test, ts(type = "number"))]
        rev: u64,
        saved_at_text: String,
    },
    /// 逐格错，没写。
    Refused { errors: Vec<CellError> },
    /// 读到之后别处存过（没 `force`）：此刻的版本 · 别处那一版跟这一份差哪几格。
    Conflict { rev: u64, theirs: Vec<Diff> },
}

fn parse_body(v: &Value) -> Result<ConfigBody, Fail> {
    serde_json::from_value::<ConfigBody>(v.clone()).map_err(|e| bad(&format!("`config`: {e}")))
}

fn saved_at_text(ctx: &Ctx, now: u64) -> String {
    crate::common::time::hm(ctx.tz.local(now_secs(now)))
}

/// `rotation-config-save`：`{id?, ifRev?, config, force?}`；参数照声明逐格校验；`id` 缺 ＝ 新建。
pub(crate) fn answer_config_save(args: &Value, tz: &crate::Tz) -> Answer {
    answer_config_save_with(
        &Ctx::here().viewing(tz),
        args,
        crate::accounts::quota::now_unix(),
    )
}

pub(crate) fn answer_config_save_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let body = parse_body(args.get("config").unwrap_or(&Value::Null))?;
    let id = match args.get("id") {
        None | Some(Value::Null) => None,
        Some(_) => Some(str_arg(args, "id")?),
    };
    let (ifr, force) = (if_rev(args)?, flag(args, "force")?);
    store_path(ctx)?;
    let all = ctx.strategies();
    let s = strategy::find(&all, &body.strategy);
    let book = ctx.hop.store.now();
    let mut errors = doc::name_errors(&book, id.as_deref(), &body.name);
    let filled = match doc::validate(&body, s.as_deref(), &account_ok) {
        Ok(b) => Some(b),
        Err(e) => {
            errors.extend(e);
            None
        }
    };
    let (Some(filled), true) = (filled, errors.is_empty()) else {
        return wire(&ConfigSaved::Refused { errors });
    };
    let filled = ConfigBody {
        name: filled.name.trim().to_string(),
        ..filled
    };
    if let Some(id) = &id {
        let Some(cur) = book.configs.get(id) else {
            return Err(("no_such_config", copy_text("beRotation.config.gone", &[])).into());
        };
        if ifr.is_some_and(|r| r != cur.rev) && !force {
            let theirs = s
                .as_deref()
                .map(|s| doc::diff(&filled, &cur.body, s, lang_of(args)))
                .unwrap_or_default();
            return wire(&ConfigSaved::Conflict {
                rev: cur.rev,
                theirs,
            });
        }
    }
    let tz = ctx.tz.iana_name();
    let (id, rev) = rotation::face_change(&ctx.hop.store, |b| {
        let id = id
            .clone()
            .unwrap_or_else(|| rotation::new_config_id(&b.configs));
        let rev = doc::write_config(
            b,
            &id,
            filled.clone(),
            now,
            rotation::SAVED_BY_SETTINGS,
            tz.clone(),
        );
        (id, rev)
    })
    .map_err(|e| ("io_failed", e))?;
    wire(&ConfigSaved::Saved {
        id,
        rev,
        saved_at_text: saved_at_text(ctx, now),
    })
}

/// `rotation-config-rename`：`{id, name, ifRev?}`。
pub(crate) fn answer_config_rename(args: &Value, tz: &crate::Tz) -> Answer {
    answer_config_rename_with(
        &Ctx::here().viewing(tz),
        args,
        crate::accounts::quota::now_unix(),
    )
}

pub(crate) fn answer_config_rename_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let id = str_arg(args, "id")?;
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("`name` must be a string"))?
        .to_string();
    let ifr = if_rev(args)?;
    store_path(ctx)?;
    let book = ctx.hop.store.now();
    let Some(cur) = book.configs.get(&id).cloned() else {
        return Err(("no_such_config", copy_text("beRotation.config.gone", &[])).into());
    };
    let errors = doc::name_errors(&book, Some(&id), &name);
    if !errors.is_empty() {
        return wire(&ConfigSaved::Refused { errors });
    }
    if ifr.is_some_and(|r| r != cur.rev) {
        return wire(&ConfigSaved::Conflict {
            rev: cur.rev,
            theirs: Vec::new(),
        });
    }
    let body = ConfigBody {
        name: name.trim().to_string(),
        ..cur.body
    };
    let tz = ctx.tz.iana_name();
    let rev = rotation::face_change(&ctx.hop.store, |b| {
        doc::write_config(
            b,
            &id,
            body.clone(),
            now,
            rotation::SAVED_BY_SETTINGS,
            tz.clone(),
        )
    })
    .map_err(|e| ("io_failed", e))?;
    wire(&ConfigSaved::Saved {
        id,
        rev,
        saved_at_text: saved_at_text(ctx, now),
    })
}

/// 删一份配置的结局。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum ConfigDropped {
    Done,
    /// 有会话开着它（`inUse` 是哪几个）· 是本机默认（先设别的为默认）。
    #[serde(rename_all = "camelCase")]
    Refused {
        in_use: Vec<String>,
        is_default: bool,
        said: String,
    },
}

/// 只有一句「成了」的应答（设本机默认 · 还原）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "RotationDone"
    )
)]
pub struct Done {
    /// 恒为 `done`。
    pub state: String,
}

fn done() -> Done {
    Done {
        state: "done".into(),
    }
}

/// `rotation-config-delete`：`{id}`；有会话开着 · 是本机默认 ⇒ 不删。
pub(crate) fn answer_config_delete(args: &Value) -> Answer {
    answer_config_delete_with(&Ctx::here(), args)
}

pub(crate) fn answer_config_delete_with(ctx: &Ctx, args: &Value) -> Answer {
    let id = str_arg(args, "id")?;
    store_path(ctx)?;
    let book = ctx.hop.store.now();
    if !book.configs.contains_key(&id) {
        return Err(("no_such_config", copy_text("beRotation.config.gone", &[])).into());
    }
    let in_use = book.in_use(&id);
    let is_default = book.default_config == id;
    if !in_use.is_empty() || is_default {
        let said = if is_default {
            copy_text("beRotation.config.isDefault", &[])
        } else {
            copy_text(
                "beRotation.config.inUse",
                &[("n", &in_use.len().to_string())],
            )
        };
        return wire(&ConfigDropped::Refused {
            in_use,
            is_default,
            said,
        });
    }
    rotation::face_change(&ctx.hop.store, |b| {
        b.configs.remove(&id);
    })
    .map_err(|e| ("io_failed", e))?;
    wire(&ConfigDropped::Done)
}

/// `rotation-default-set`：`{id}` ⇒ 本机默认指向它（已开着的会话不跟着换）。
pub(crate) fn answer_default_set(args: &Value) -> Answer {
    answer_default_set_with(&Ctx::here(), args)
}

pub(crate) fn answer_default_set_with(ctx: &Ctx, args: &Value) -> Answer {
    let id = str_arg(args, "id")?;
    store_path(ctx)?;
    if !ctx.hop.store.now().configs.contains_key(&id) {
        return Err(("no_such_config", copy_text("beRotation.config.gone", &[])).into());
    }
    rotation::face_change(&ctx.hop.store, |b| b.default_config = id.clone())
        .map_err(|e| ("io_failed", e))?;
    wire(&done())
}

// ── 策略 ─────────────────────────────────────────────────────────────────────────────

/// 共用小函数库那一行（只读）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "StrategyLib"
    )
)]
pub struct LibShown {
    pub source: String,
}

/// 策略列表里的一项。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct StrategyItem {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub path: Option<String>,
    pub ordered: bool,
    /// 缺省参数下的描述（读不懂 ⇒ 空串）。
    pub text: String,
    pub params: Vec<Shown>,
    pub acct_params: Vec<Shown>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub error: Option<StrategyError>,
    /// 用它的配置（id）。
    pub used_by: Vec<String>,
}

/// `rotation-strategies-read` 的应答。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct StrategiesRead {
    pub lib: LibShown,
    pub strategies: Vec<StrategyItem>,
}

fn load_error(s: &Strategy, f: &StrategyFail) -> StrategyError {
    StrategyError {
        fail: f.clone(),
        text: if f.msg == strategy::DUP_ID {
            copy_text("beRotation.strategy.dupId", &[("id", &s.id)])
        } else {
            fail_text(&s.name, f)
        },
    }
}

fn strategy_item(ctx: &Ctx, book: &Book, s: &Strategy, lang: Lang, now: u64) -> StrategyItem {
    let shown = |d: &[strategy::form::Decl]| {
        d.iter()
            .map(|x| strategy::form::shown(x, &s.texts, lang))
            .collect()
    };
    let text = if s.usable() {
        let body = ConfigBody {
            name: String::new(),
            strategy: s.id.clone(),
            accounts: Vec::new(),
            params: s.param_init(),
            acct_init: s.acct_init(),
            parked: Default::default(),
            tz: None,
        };
        describe(ctx, LIBRARY_AGENT, &body, Some(s), lang, now)
    } else {
        String::new()
    };
    StrategyItem {
        id: s.id.clone(),
        name: s.name.clone(),
        builtin: s.builtin,
        path: s.path.as_ref().map(|p| p.display().to_string()),
        ordered: s.ordered,
        text,
        params: shown(&s.params),
        acct_params: shown(&s.acct_params),
        source: s.source.clone(),
        error: s.error.as_ref().map(|f| load_error(s, f)),
        used_by: book
            .configs
            .iter()
            .filter(|(_, c)| c.body.strategy == s.id)
            .map(|(id, _)| id.clone())
            .collect(),
    }
}

/// `rotation-strategies-read`：`{lang?}` ⇒ 共用小函数的源码 ＋ 每份策略（内置在前；读不懂的照实写哪一行）。
pub(crate) fn answer_strategies_read(args: &Value, tz: &crate::Tz) -> Answer {
    answer_strategies_read_with(
        &Ctx::here().viewing(tz),
        args,
        crate::accounts::quota::now_unix(),
    )
}

pub(crate) fn answer_strategies_read_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let book = ctx.hop.store.now();
    let lang = lang_of(args);
    let all = ctx.strategies();
    wire(&StrategiesRead {
        lib: LibShown {
            source: strategy::LIB_SOURCE.to_string(),
        },
        strategies: all
            .iter()
            .map(|s| strategy_item(ctx, &book, s, lang, now))
            .collect(),
    })
}

/// 装 · 删一份策略的结局。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum StrategyChanged {
    Done {
        id: String,
    },
    /// 装不进（读不懂 · 与内置同 id · 名字不合法）/ 删不了（内置的 · 有配置在用）：那一句 ＋ 读不懂时第几行。
    #[serde(rename_all = "camelCase")]
    Refused {
        said: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        error: Option<StrategyError>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(optional, as = "Option<Vec<String>>"))]
        used_by: Vec<String>,
    },
}

/// `rotation-strategy-add`：`{id, source}` ⇒ 读得懂才写进本机策略目录（`<id>.rhai`，已有同名 ⇒ 覆盖）。
pub(crate) fn answer_strategy_add(args: &Value) -> Answer {
    answer_strategy_add_with(&Ctx::here(), args)
}

pub(crate) fn answer_strategy_add_with(ctx: &Ctx, args: &Value) -> Answer {
    let id = str_arg(args, "id")?;
    let source = args
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("`source` must be the file text"))?
        .to_string();
    if ctx.hop.store.strategy_dir().is_none() {
        return Err(("io_failed", copy_text("beRotation.read.noHome", &[])).into());
    }
    let taken: Vec<String> = strategy::builtins().iter().map(|l| l.id.clone()).collect();
    let probe = strategy::load(
        vec![strategy::File {
            name: format!("{id}.rhai"),
            path: None,
            text: Ok(source.clone()),
        }],
        false,
        &taken,
    )
    .remove(0);
    if let Some(e) = &probe.error {
        let err = load_error(&probe, e);
        return wire(&StrategyChanged::Refused {
            said: err.text.clone(),
            error: Some(err),
            used_by: Vec::new(),
        });
    }
    rotation::strategy_file_write(&ctx.hop.store, &id, &source).map_err(|e| ("io_failed", e))?;
    wire(&StrategyChanged::Done { id })
}

/// `rotation-strategy-delete`：`{id}` ⇒ 删本机那一份（内置的 · 有配置在用 ⇒ 不删）。
pub(crate) fn answer_strategy_delete(args: &Value) -> Answer {
    answer_strategy_delete_with(&Ctx::here(), args)
}

pub(crate) fn answer_strategy_delete_with(ctx: &Ctx, args: &Value) -> Answer {
    let id = str_arg(args, "id")?;
    let all = ctx.strategies();
    let Some(s) = strategy::find(&all, &id) else {
        return Err((
            "no_such_strategy",
            copy_text("beRotation.strategy.gone", &[("name", &id)]),
        )
            .into());
    };
    if s.builtin {
        return wire(&StrategyChanged::Refused {
            said: copy_text("beRotation.strategy.builtinFixed", &[]),
            error: None,
            used_by: Vec::new(),
        });
    }
    let used_by: Vec<String> = ctx
        .hop
        .store
        .now()
        .configs
        .iter()
        .filter(|(_, c)| c.body.strategy == id)
        .map(|(cid, _)| cid.clone())
        .collect();
    if !used_by.is_empty() {
        return wire(&StrategyChanged::Refused {
            said: copy_text(
                "beRotation.strategy.inUse",
                &[("n", &used_by.len().to_string())],
            ),
            error: None,
            used_by,
        });
    }
    if let Some(p) = &s.path {
        rotation::strategy_file_remove(&ctx.hop.store, p).map_err(|e| ("io_failed", e))?;
    }
    wire(&StrategyChanged::Done { id })
}

/// 试跑那一行：号 · `p`（`null` ＝ 此刻不挑）· 写好的为什么 · 几点能再挑。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct TryRow {
    pub account: String,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub p: Option<f64>,
    pub why: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub back: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub back_text: Option<String>,
}

/// `rotation-strategy-try` 的应答：排出的先后 · 会挑谁（停发 ⇒ 缺）· 备忘 · 用了几次操作（上限）· 出错的话为什么。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct TryReply {
    pub rows: Vec<TryRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub pick: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "unknown"))]
    pub memo: Option<Value>,
    #[cfg_attr(test, ts(type = "number"))]
    pub ops: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub max_ops: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub strategy_error: Option<StrategyError>,
}

/// 推演 · 试跑问的是哪一份：会话此刻那份（可另给草稿）· 一份存下的配置 · 草稿。
struct Subject {
    agent: String,
    start: String,
    current: Option<String>,
    body: ConfigBody,
    entry: Option<SessionEntry>,
}

fn gone_session() -> Fail {
    Fail::new("no_such_session", copy_text("beRotation.session.gone", &[]))
}

fn gone_config() -> Fail {
    Fail::new("no_such_config", copy_text("beRotation.config.gone", &[]))
}

fn subject(book: &Book, args: &Value) -> Result<Subject, Fail> {
    let zero = crate::accounts::manage::model::ACCOUNT_ZERO.to_string();
    let draft = args.get("draft").filter(|v| !v.is_null());
    if args.get("sid").is_some_and(|v| !v.is_null()) {
        let sid = sid_arg(args)?;
        let s = book.sessions.get(&sid).ok_or_else(gone_session)?;
        let o = book.opened(s).ok_or_else(gone_config)?;
        let body = match draft {
            Some(d) => parse_body(d)?,
            None => o.body.clone(),
        };
        return Ok(Subject {
            agent: s.agent.clone(),
            start: s.start.clone(),
            current: Some(s.current.clone()),
            body,
            entry: Some(s.clone()),
        });
    }
    let body = match draft {
        Some(d) => parse_body(d)?,
        None => {
            let id = str_arg(args, "config")?;
            book.configs
                .get(&id)
                .map(|c| c.body.clone())
                .ok_or_else(gone_config)?
        }
    };
    Ok(Subject {
        agent: LIBRARY_AGENT.to_string(),
        start: zero,
        current: None,
        body,
        entry: None,
    })
}

/// 用哪份策略跑：`source` 有 ⇒ 现编那一份（不装）；`id` 有 ⇒ 那一份；都没有 ⇒ 配置写的那份。
fn trial_strategy(ctx: &Ctx, args: &Value, body: &ConfigBody) -> Option<Arc<Strategy>> {
    if let Some(src) = args.get("source").and_then(Value::as_str) {
        return Some(Arc::new(
            strategy::load(
                vec![strategy::File {
                    name: "try.rhai".into(),
                    path: None,
                    text: Ok(src.to_string()),
                }],
                false,
                &[],
            )
            .remove(0),
        ));
    }
    let all = ctx.strategies();
    let id = args
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or(&body.strategy);
    strategy::find(&all, id)
}

/// 一行的「为什么」写成字：策略没给 ⇒「策略没给」。
fn row_why(speak: Option<&Speak<'_>>, r: &Ranked) -> String {
    if r.missing {
        return copy_text("beRotation.strategy.missing", &[]);
    }
    speak.map(|s| s.why(&r.why)).unwrap_or_default()
}

fn strategy_error(name: &str, f: StrategyFail) -> StrategyError {
    StrategyError {
        text: fail_text(name, &f),
        fail: f,
    }
}

/// `rotation-strategy-try`：`{source? | id?, config | draft | sid, lang?}` ⇒ 按此刻的事实跑一次（不写盘不换号）。
pub(crate) fn answer_strategy_try(args: &Value, tz: &crate::Tz) -> Answer {
    answer_strategy_try_with(
        &Ctx::here().viewing(tz),
        args,
        crate::accounts::quota::now_unix(),
    )
}

pub(crate) fn answer_strategy_try_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let book = ctx.hop.store.now();
    let sub = subject(&book, args)?;
    let s = trial_strategy(ctx, args, &sub.body);
    let body = match &s {
        Some(st) if st.usable() => doc::filled(
            &ConfigBody {
                strategy: st.id.clone(),
                ..sub.body.clone()
            },
            st,
        ),
        _ => sub.body.clone(),
    };
    let row = |a: &str| (ctx.rows)(&sub.agent, a);
    let current = sub.current.clone().unwrap_or_else(|| sub.start.clone());
    let sc = ctx
        .hop
        .scene_of(&book, &sub.agent, &sub.start, &current, &body, &row, now);
    let memo = sub
        .entry
        .as_ref()
        .and_then(|e| e.memo.clone())
        .unwrap_or(Value::Null);
    let j = decide::judge(&decide::Judge {
        strategy: s.as_deref(),
        body: &body,
        accounts: &sc.accounts,
        now,
        current: &current,
        stint: sc.stint,
        rate: sc.rate,
        memo: &memo,
    });
    let manual = sub
        .entry
        .as_ref()
        .map(|e| e.manual.clone())
        .unwrap_or_default();
    let (eff, _) = decide::effective(&j.rows, &manual, &sc.accounts);
    let v = decide::decide(
        &eff,
        &decide::Pickable {
            facts: &sc.accounts,
            current: &current,
            tried: &[],
            can_hold: crate::agents::limit_reply_of(&sub.agent).is_some(),
        },
        &j.strategy,
        &mut |x| {
            if ctx.hop.ready(&sub.agent, &sub.start, x, &row, now) {
                Ok(())
            } else {
                Err(Unready::NeedsLogin)
            }
        },
    );
    let all = ctx.strategies();
    let used = if j.error.is_some() {
        strategy::fallback()
    } else {
        s.clone()
    };
    let lang = lang_of(args);
    let speak = used.as_deref().map(|u| Speak::of(u, lang, now, &ctx.tz));
    let clock = TextClock::new(now, &ctx.tz);
    let rows = eff
        .iter()
        .map(|r| TryRow {
            account: r.account.clone(),
            p: r.p,
            why: row_why(speak.as_ref(), r),
            back: r.back,
            back_text: r.back.map(|t| clock.text(t).0),
        })
        .collect();
    let pick = match v {
        Verdict::Stay { .. } => Some(current.clone()),
        Verdict::Switch { to, .. } => Some(to),
        Verdict::Hold { .. } => None,
    };
    let name = s
        .as_ref()
        .map_or_else(|| strategy_name(&all, &body.strategy), |x| x.name.clone());
    wire(&TryReply {
        rows,
        pick,
        memo: j.memo,
        ops: j.ops,
        max_ops: strategy::MAX_OPERATIONS,
        strategy_error: j.error.map(|f| strategy_error(&name, f)),
    })
}

// ── 会话 ─────────────────────────────────────────────────────────────────────────────

/// 会话打开的那一份：哪份 · 名字 · 版本 · 改没改过 · 别处存了新版（本会话改过才有）· 改了哪几格（改过才有）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct OpenShown {
    pub id: String,
    pub name: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub rev: u64,
    pub dirty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub newer: Option<Newer>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<Diff>>"))]
    pub diff: Vec<Diff>,
}

/// 别处存的新版：版本 · 时刻 · 谁存的（`settings` 或会话 id）· 写好的那一句。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ConfigNewer"
    )
)]
pub struct Newer {
    #[cfg_attr(test, ts(type = "number"))]
    pub rev: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
    pub by: String,
    pub text: String,
}

/// 会话面板列表的一行（有效优先级的先后，第一行是在用的号）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "RotationRow"
    )
)]
pub struct RowShown {
    pub account: String,
    /// 策略给的那一句写成的字（`skip` 时面板写「跳过 · {why}」）。
    pub why: String,
    /// 此刻不挑（`p` 是 `()`）。
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub skip: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub back: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub back_text: Option<String>,
    /// 此刻也钉在它上面的别的会话数。
    #[cfg_attr(test, ts(type = "number"))]
    pub also_used: u32,
    pub facts: Facts,
    /// 从「其余的订阅号」那一行来的。
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub from_rest: bool,
}

/// 一行的参数值（谁 ＋ 那一行的每号参数）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct WhoParams {
    /// `@start` · `@rest` · 号 id。
    pub who: String,
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub params: Params,
}

/// 副本此刻的参数值（照策略声明补齐）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ConfigValues"
    )
)]
pub struct Values {
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub params: Params,
    pub accounts: Vec<WhoParams>,
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub acct_init: Params,
}

/// 用的哪份策略。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct StrategyRef {
    pub id: String,
    pub name: String,
    pub ordered: bool,
}

/// `rotation-session-read` 里一个会话的那一份。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SessionRotation {
    pub agent: String,
    pub start: String,
    pub current: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub since: u64,
    pub history: Vec<SwitchRecord>,
    pub in_place: rotation::InPlace,
    pub config: OpenShown,
    /// 副本的描述（后端写好）。
    pub text: String,
    pub strategy: StrategyRef,
    pub values: Values,
    pub rows: Vec<RowShown>,
    /// 手动热换到的号（顶在最前）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub hot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub blocked: Option<Blocked>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub strategy_error: Option<StrategyError>,
    /// 此刻那个号的显示态。
    pub quota: show::QuotaShow,
    /// 轮换文件此刻读不懂（引擎照上一份读懂的接着挑号、不覆盖那份文件）：需手动；那一句 ＋ 复制详情。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub needs: Option<RotationNeeds>,
}

/// 会话这一侧需手动的那一格。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct RotationNeeds {
    /// `unreadable`（轮换文件读不懂）。
    pub kind: String,
    pub reason: String,
    pub detail: String,
}

/// 一个会话在这台查得到吗：查得到 ⇒ 那一份；中转没见过 ⇒ 只说能不能不重启换。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum SessionRotationState {
    Present(Box<SessionRotation>),
    #[serde(rename_all = "camelCase")]
    Absent {
        in_place: rotation::InPlace,
    },
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

fn newer_text(name: &str, at: u64, by: &str, now: u64, tz: &crate::Tz) -> String {
    let at_text = crate::common::time::fmt_at(now_secs(at), now_secs(now), tz);
    let place = if by == rotation::SAVED_BY_SETTINGS {
        copy_text("rot.doc.whereSettings", &[])
    } else {
        copy_text("rot.doc.whereSession", &[("session", by)])
    };
    [
        copy_text("rot.doc.newer", &[("cfg", name)]),
        copy_text("rot.doc.newerBy", &[("at", &at_text), ("where", &place)]),
    ]
    .join(&copy_text("beRotation.desc.sep", &[]))
}

/// ★ 一个会话此刻的那一份（只读）：有效优先级的每一行（第一行在用）· 打开的那份 · 描述 · 停发 · 策略出错 · 显示态。
pub(crate) fn session_state(
    ctx: &Ctx,
    book: &Book,
    sid: &str,
    live: &dyn Fn(&str) -> bool,
    lang: Lang,
    now: u64,
) -> SessionRotationState {
    let agent = book.sessions.get(sid).map(|s| s.agent.clone());
    let row = |a: &str| agent.as_deref().and_then(|g| (ctx.rows)(g, a));
    let v = match ctx.hop.view(book, sid, &row, live, now) {
        Ok(v) => v,
        Err(in_place) => return SessionRotationState::Absent { in_place },
    };
    let all = ctx.strategies();
    let tz = &ctx.tz;
    let used = strategy::find(&all, &v.judged.strategy);
    let speak = used.as_deref().map(|u| Speak::of(u, lang, now, tz));
    let clock = TextClock::new(now, tz);
    let fact = |a: &str| {
        v.scene
            .accounts
            .iter()
            .find(|(f, _)| f.id == a)
            .map(|(f, _)| f.clone())
    };
    // 第一行固定是在用的号，往下照有效优先级排。
    let mut eff = v.eff.clone();
    if let Some(i) = eff.iter().position(|r| r.account == v.entry.current) {
        let r = eff.remove(i);
        eff.insert(0, r);
    }
    let rows: Vec<RowShown> = eff
        .iter()
        .filter_map(|r| {
            let f = fact(&r.account)?;
            let back = r.back.or(f.back);
            Some(RowShown {
                account: r.account.clone(),
                why: row_why(speak.as_ref(), r),
                skip: r.p.is_none(),
                back,
                back_text: back.map(|t| clock.text(t).0),
                also_used: f.others,
                from_rest: v.scene.from_rest.contains(&r.account),
                facts: f,
            })
        })
        .collect();
    let blocked = match &v.verdict {
        Verdict::Hold { back, .. } => Some(Blocked {
            earliest: back.as_ref().map(|(a, at)| rotation::AccountAt {
                account: a.clone(),
                at: *at,
                at_text: None,
                at_rel_text: None,
            }),
            text: match back {
                Some((a, at)) => copy_text(
                    "beRotation.blocked.stop",
                    &[
                        ("at", &clock.text(*at).0),
                        ("acct", &crate::accounts::quota::name_words::account_text(a)),
                        ("rel", &clock.rel(*at).map(|w| w.0).unwrap_or_default()),
                    ],
                ),
                None => copy_text("beRotation.blocked.none", &[]),
            },
        }),
        _ => None,
    };
    let s = strategy::find(&all, &v.body.strategy);
    let body = match &s {
        Some(st) if st.usable() => doc::filled(&v.body, st),
        _ => v.body.clone(),
    };
    let name = book
        .configs
        .get(&v.config_id)
        .map_or_else(String::new, |c| c.body.name.clone());
    let newer = doc::newer_of(book, &v.entry).map(|(rev, at, by)| Newer {
        rev,
        at,
        text: newer_text(&name, at, &by, now, tz),
        by,
    });
    let diff = match (&s, book.configs.get(&v.config_id)) {
        (Some(st), Some(c)) if v.dirty && st.usable() => doc::diff(&c.body, &body, st, lang),
        _ => Vec::new(),
    };
    let mut history = v.entry.history.clone();
    for h in &mut history {
        h.text = Some(record_text(h, &all, lang, now, tz));
    }
    let mut quota = v.quota.clone();
    quota.stamp(&clock);
    let needs = ctx.hop.store.unreadable().map(|e| {
        let (reason, detail) = crate::stream::detail::unreadable("rotation-session-read", Some(&e));
        RotationNeeds {
            kind: "unreadable".into(),
            reason: reason.as_str().unwrap_or_default().to_string(),
            detail: detail.as_str().unwrap_or_default().to_string(),
        }
    });
    SessionRotationState::Present(Box::new(SessionRotation {
        agent: v.entry.agent.clone(),
        start: v.entry.start.clone(),
        current: v.entry.current.clone(),
        since: v.entry.since,
        history,
        in_place: v.in_place,
        config: OpenShown {
            id: v.config_id.clone(),
            name,
            rev: v.rev,
            dirty: v.dirty,
            newer,
            diff,
        },
        text: describe(ctx, &v.entry.agent, &v.body, s.as_deref(), lang, now),
        strategy: StrategyRef {
            id: v.body.strategy.clone(),
            name: strategy_name(&all, &v.body.strategy),
            ordered: s.as_ref().is_some_and(|x| x.ordered),
        },
        values: Values {
            params: body.params.clone(),
            accounts: body
                .accounts
                .iter()
                .map(|r| WhoParams {
                    who: r.who.key().to_string(),
                    params: r.params.clone(),
                })
                .collect(),
            acct_init: body.acct_init.clone(),
        },
        rows,
        hot: v
            .entry
            .manual
            .hot
            .as_ref()
            .filter(|_| !v.hot_stale)
            .map(|h| h.account.clone()),
        blocked,
        strategy_error: v
            .judged
            .error
            .clone()
            .map(|f| strategy_error(&strategy_name(&all, &v.body.strategy), f)),
        quota,
        needs,
    }))
}

/// `rotation-session-read` 的应答。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SessionRead {
    pub state: String,
    #[cfg_attr(test, ts(type = "string | null"))]
    pub reason: Value,
    #[cfg_attr(test, ts(type = "string | null"))]
    pub detail: Value,
    #[cfg_attr(test, ts(type = "number"))]
    pub now: u64,
    pub sessions: std::collections::BTreeMap<String, SessionRotationState>,
}

/// `rotation-session-read`：一批会话各自的那一份（`{sids, lang?}`）；这台没见过的照实标 `absent`，不整批失败。
pub(crate) fn answer_session_read(args: &Value, tz: &crate::Tz) -> Answer {
    let now = crate::accounts::quota::now_unix();
    let mut v = answer_session_read_with(&Ctx::here().viewing(tz), args, now)?;
    crate::common::time::with_texts_now(&mut v, now, tz);
    Ok(v)
}

pub(crate) fn answer_session_read_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let sids = sids_of(args, "sids")?;
    let (state, why, book) = read_book(ctx);
    let (reason, detail) = crate::stream::detail::unreadable("rotation-session-read", why.as_ref());
    let live = (ctx.live)();
    let lang = lang_of(args);
    let sessions = sids
        .into_iter()
        .map(|sid| {
            let one = session_state(ctx, &book, &sid, &|x| live.contains(x), lang, now);
            (sid, one)
        })
        .collect();
    let mut v = wire::<_, Fail>(&SessionRead {
        state: state.to_string(),
        reason,
        detail,
        now,
        sessions,
    })?;
    crate::accounts::quota::name_words::with_names(&mut v);
    Ok(v)
}

/// 打开一份配置的结局。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum SessionOpened {
    Opened,
    /// 本会话的副本有没存的改动（界面问保存 / 丢掉 / 取消）。
    Dirty {
        diff: Vec<Diff>,
    },
}

/// `rotation-session-open`：`{sid, config, discard?}`。
pub(crate) fn answer_session_open(args: &Value) -> Answer {
    answer_session_open_with(&Ctx::here(), args)
}

pub(crate) fn answer_session_open_with(ctx: &Ctx, args: &Value) -> Answer {
    let sid = sid_arg(args)?;
    let id = str_arg(args, "config")?;
    let discard = flag(args, "discard")?;
    let lang = lang_of(args);
    store_path(ctx)?;
    let all = ctx.strategies();
    let r = rotation::face_change(&ctx.hop.store, |b| {
        doc::open_session(b, &sid, &id, discard, &all, lang)
    })
    .map_err(|e| ("io_failed", e))?;
    match r {
        doc::Opened::Opened => wire(&SessionOpened::Opened),
        doc::Opened::Dirty(diff) => wire(&SessionOpened::Dirty { diff }),
        doc::Opened::Missing => Err(gone_config()),
    }
}

/// 改副本的结局。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SessionEdited {
    pub dirty: bool,
    pub diff: Vec<Diff>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<CellError>>"))]
    pub errors: Vec<CellError>,
}

fn edit_of(args: &Value) -> Result<Edit, Fail> {
    let list = |k: &str| -> Result<Vec<String>, Fail> {
        match args.get(k) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(a)) => a
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| bad(&format!("`{k}` must be an array of account ids")))
                })
                .collect(),
            Some(_) => Err(bad(&format!("`{k}` must be an array of account ids"))),
        }
    };
    let set = match args.get("set") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let k = x
                    .get("k")
                    .and_then(Value::as_str)
                    .ok_or_else(|| bad(&format!("`set[{i}].k` must be a parameter name")))?;
                Ok(Set {
                    acct: x.get("acct").and_then(Value::as_str).map(str::to_string),
                    k: k.to_string(),
                    v: x.get("v").cloned().unwrap_or(Value::Null),
                })
            })
            .collect::<Result<_, Fail>>()?,
        Some(_) => return Err(bad("`set` must be an array of {acct?, k, v}")),
    };
    Ok(Edit {
        set,
        add: list("add")?,
        drop: list("drop")?,
        strategy: args
            .get("strategy")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// `rotation-session-edit`：`{sid, set: [{acct?, k, v}], add?, drop?, strategy?, lang?}` ⇒ 改副本（手动调整不在这里）。
pub(crate) fn answer_session_edit(args: &Value) -> Answer {
    answer_session_edit_with(&Ctx::here(), args)
}

pub(crate) fn answer_session_edit_with(ctx: &Ctx, args: &Value) -> Answer {
    let sid = sid_arg(args)?;
    let e = edit_of(args)?;
    let lang = lang_of(args);
    store_path(ctx)?;
    let all = ctx.strategies();
    let r = rotation::face_change(&ctx.hop.store, |b| {
        doc::edit_session(b, &sid, &e, &all, &account_ok, lang)
    })
    .map_err(|e| ("io_failed", e))?
    .ok_or_else(gone_session)?;
    wire(&SessionEdited {
        dirty: r.dirty,
        diff: r.diff,
        errors: r.errors,
    })
}

/// 存会话副本的结局。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum SessionSaved {
    Saved {
        id: String,
        #[cfg_attr(test, ts(type = "number"))]
        rev: u64,
    },
    /// 那份配置读到之后别处存过（没 `force`）：此刻的版本 · 别处那一版 · 本会话的改动。
    Conflict {
        #[cfg_attr(test, ts(type = "number"))]
        rev: u64,
        theirs: Vec<Diff>,
        mine: Vec<Diff>,
    },
    Refused {
        errors: Vec<CellError>,
    },
}

/// `rotation-session-save`：`{sid, ifRev, as?: {name?}, force?}`。
pub(crate) fn answer_session_save(args: &Value, tz: &crate::Tz) -> Answer {
    answer_session_save_with(
        &Ctx::here().viewing(tz),
        args,
        crate::accounts::quota::now_unix(),
    )
}

pub(crate) fn answer_session_save_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let sid = sid_arg(args)?;
    let ifr = if_rev(args)?;
    let force = flag(args, "force")?;
    let as_name = match args.get("as") {
        None | Some(Value::Null) => None,
        Some(Value::Object(m)) => Some(m.get("name").and_then(Value::as_str).map(str::to_string)),
        Some(_) => return Err(bad("`as` must be {name?}")),
    };
    let lang = lang_of(args);
    store_path(ctx)?;
    let all = ctx.strategies();
    let tz = ctx.tz.iana_name();
    let r = rotation::face_change(&ctx.hop.store, |b| {
        doc::save_session(
            b,
            &sid,
            ifr,
            as_name.clone(),
            force,
            now,
            tz.clone(),
            &all,
            lang,
        )
    })
    .map_err(|e| ("io_failed", e))?;
    match r {
        Saved::Saved { id, rev } => wire(&SessionSaved::Saved { id, rev }),
        Saved::Clash { rev, theirs, mine } => wire(&SessionSaved::Conflict { rev, theirs, mine }),
        Saved::Refused(errors) => wire(&SessionSaved::Refused { errors }),
    }
}

/// `rotation-session-revert`：`{sid, toLatest?}` ⇒ 丢副本（`toLatest` ＝ 载入新版本：同一件事，`rev` 跟上）。
pub(crate) fn answer_session_revert(args: &Value) -> Answer {
    answer_session_revert_with(&Ctx::here(), args)
}

pub(crate) fn answer_session_revert_with(ctx: &Ctx, args: &Value) -> Answer {
    let sid = sid_arg(args)?;
    flag(args, "toLatest")?;
    store_path(ctx)?;
    let ok = rotation::face_change(&ctx.hop.store, |b| doc::revert_session(b, &sid))
        .map_err(|e| ("io_failed", e))?;
    if !ok {
        return Err(gone_session());
    }
    wire(&done())
}

/// 手动调整的结局：`done` · 没换成（`skipped` · `failed`，`why` 同 `rotation-switch` 那一格的码）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "RotationPlaced"
    )
)]
pub struct Placed {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub why: Option<String>,
}

impl From<SwitchOutcome> for Placed {
    fn from(o: SwitchOutcome) -> Self {
        match o {
            SwitchOutcome::Switched => Placed {
                state: "done".into(),
                why: None,
            },
            SwitchOutcome::Skipped { code } => Placed {
                state: "skipped".into(),
                why: Some(code),
            },
            SwitchOutcome::NotSwitched { code } => Placed {
                state: "failed".into(),
                why: Some(code),
            },
        }
    }
}

/// `rotation-session-set`：手动调整（不碰副本）。`{sid, place: {account, at}}` ⇒ 把那个号放到第 `at` 位（`at = 0` ＝ 现在就换过去，同
/// `rotation-switch` 热换）；`{sid, refresh: true}` ⇒ 清掉全部手动调整与备忘，照策略重算。
pub(crate) fn answer_session_set(args: &Value) -> Answer {
    answer_session_set_with(&Ctx::here(), args, crate::accounts::quota::now_unix())
}

pub(crate) fn answer_session_set_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let sid = sid_arg(args)?;
    store_path(ctx)?;
    if flag(args, "refresh")? {
        let ok = rotation::face_change(&ctx.hop.store, |b| match b.sessions.get_mut(&sid) {
            Some(s) => {
                s.manual = Manual::default();
                s.memo = None;
                true
            }
            None => false,
        })
        .map_err(|e| ("io_failed", e))?;
        if !ok {
            return Err(gone_session());
        }
        return wire(&Placed::from(SwitchOutcome::Switched));
    }
    let place = args
        .get("place")
        .and_then(Value::as_object)
        .ok_or_else(|| bad("`place` must be {account, at} (or `refresh: true`)"))?;
    let account = place
        .get("account")
        .and_then(Value::as_str)
        .filter(|a| account_ok(a) || *a == crate::accounts::quota::name_words::HOME_ACCOUNT)
        .ok_or_else(|| bad("`place.account` must name an account"))?
        .to_string();
    let at = place
        .get("at")
        .and_then(Value::as_u64)
        .ok_or_else(|| bad("`place.at` must be a position (0 = switch now)"))?;
    if at == 0 {
        return wire(&Placed::from(hot_one(ctx, &sid, &account, now)));
    }
    let ok = rotation::face_change(&ctx.hop.store, |b| match b.sessions.get_mut(&sid) {
        Some(s) => {
            s.manual.place.retain(|p| p.account != account);
            s.manual.place.push(Place {
                account: account.clone(),
                at: usize::try_from(at).unwrap_or(usize::MAX),
            });
            true
        }
        None => false,
    })
    .map_err(|e| ("io_failed", e))?;
    if !ok {
        return Err(gone_session());
    }
    wire(&Placed::from(SwitchOutcome::Switched))
}

/// `session-new` 带 `rotation: {config}`：起之前在这台的账本里给那个定好的 sid 记一条、打开那一份（会话一报到就已经是它）。
/// 配置不在 ⇒ `no_such_config`。`kind` ＝ 线上的那一家（记成它的适配器 id）。
pub(crate) fn preset_with(
    ctx: &Ctx,
    sid: &str,
    kind: &str,
    config: &str,
    now: u64,
) -> Result<(), (&'static str, String)> {
    store_path(ctx)?;
    let book = ctx.hop.store.now();
    let Some(c) = book.configs.get(config) else {
        return Err(("no_such_config", copy_text("beRotation.config.gone", &[])));
    };
    let open = rotation::OpenConfig {
        id: config.to_string(),
        rev: c.rev,
        copy: None,
    };
    let agent = crate::agents::pick_kind(Some(kind))
        .map(|(_, f)| f.adapter_id.to_string())
        .unwrap_or_else(|_| kind.to_string());
    let zero = crate::accounts::manage::model::ACCOUNT_ZERO;
    rotation::face_change(&ctx.hop.store, |b| {
        b.sessions.insert(
            sid.to_string(),
            SessionEntry::fresh(&agent, zero, now, open.clone()),
        );
    })
    .map_err(|e| ("io_failed", crate::common::said::IntoNote::into_note(e)))
}

/// 起不成：撤掉 [`preset_with`] 记的那一条（只撤还没换过号的；写不成只出声）。
pub(crate) fn forget_preset_with(ctx: &Ctx, sid: &str) {
    if let Err(e) = rotation::face_change(&ctx.hop.store, |b| {
        if b.sessions.get(sid).is_some_and(|s| s.history.is_empty()) {
            b.sessions.remove(sid);
        }
    }) {
        tracing::warn!("[rotation] 撤不掉起之前记的那一条 {sid}：{}", e.logged());
    }
}

pub(crate) fn outcome(o: SwitchOutcome) -> Result<Value, (&'static str, String)> {
    serde_json::to_value(o).map_err(|e| ("failed", e.to_string()))
}

/// 不重启换一个会话（手动热换）：中转见过它 · 这一家有账号 · 这台建了账号库 · 目标号接得上 ⇒ 钉过去、记 `manual.hot`、记一条。
/// 不出任何提示。
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
    let row = |a: &str| (ctx.rows)(&s.agent, a);
    if s.current != target {
        if let Err(u) = ctx.hop.check_target(&s.agent, &s.start, target, &row, now) {
            let code = match u {
                Unready::NeedsKey => "targetNeedsKey",
                Unready::NeedsLogin | Unready::UnsureBody => "targetNeedsLogin",
            };
            return SwitchOutcome::NotSwitched { code: code.into() };
        }
    }
    let from = s.current.clone();
    record(ctx, sid, &from, target, Leave::Manual, now)
}

/// 记一条手动换号（钉到 `to`；`Leave::Manual` ⇒ 另记 `manual.hot`）；写不进 ⇒ 没成。
pub(crate) fn record(
    ctx: &Ctx,
    sid: &str,
    from: &str,
    to: &str,
    leave: Leave,
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
    let hot = matches!(leave, Leave::Manual);
    let rec = SwitchRecord {
        at: now,
        at_text: None,
        from: from.to_string(),
        to: to.to_string(),
        leave,
        pick: None,
        from_resets_at: resets,
        from_resets_at_text: None,
        text: None,
    };
    let base = ctx.hop.baseline_of(&agent, to, now);
    match rotation::face_change(&ctx.hop.store, |b| {
        if hot {
            if let Some(s) = b.sessions.get_mut(sid) {
                s.manual.hot = Some(Hot {
                    account: to.to_string(),
                    since: now,
                });
                s.manual.place.retain(|p| p.account != to);
            }
        }
        if b.pin(sid, rec) {
            b.rebase(sid, &base);
        }
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

// ── 推演 ─────────────────────────────────────────────────────────────────────────────

/// 视窗：`12h`（前 3h · 后 9h）· `24h`（前 6h · 后 18h）。
fn view_secs(args: &Value) -> Result<(u64, u64, &'static str), Fail> {
    match args.get("view").and_then(Value::as_str) {
        None | Some("12h") => Ok((3 * 3600, 9 * 3600, "12h")),
        Some("24h") => Ok((6 * 3600, 18 * 3600, "24h")),
        Some(_) => Err(bad("`view` must be 12h / 24h")),
    }
}

/// 推演里的一段：`[from, to)` 用 `account`（`null` ＝ 停发）· 这一段开头为什么换过来（头一段 ⇒ 缺）· 估的。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct PlanSeg {
    #[cfg_attr(test, ts(type = "number"))]
    pub from: u64,
    pub from_text: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub to: u64,
    pub to_text: String,
    pub account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub why: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub est: bool,
}

/// 合图里一个号那一行的一段：`k` ＝ `use`（在用）· `line`（到线：策略给 `()` 且为什么是 `@line`）· `off`（时段不用）·
/// `refused` · `full` · `overage` · `login` · `skip`（策略此刻不挑，别的原因）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = "../../frontend/ui/generated/", rename = "PlanBar")
)]
pub struct Bar {
    #[cfg_attr(test, ts(type = "number"))]
    pub from: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub to: u64,
    pub k: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub est: bool,
}

/// 视窗里的一个重置刻度（语义位 · 时刻）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "PlanReset"
    )
)]
pub struct ResetMark {
    pub w: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
    pub at_text: String,
}

/// 合图里一个号那一行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct PlanRow {
    pub account: String,
    pub bars: Vec<Bar>,
    pub resets: Vec<ResetMark>,
}

/// 刻度。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct GridMark {
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
    pub at_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub label: Option<String>,
}

/// `rotation-plan` 的应答。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct PlanReply {
    #[cfg_attr(test, ts(type = "number"))]
    pub now: u64,
    pub now_text: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub from: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub until: u64,
    pub view: String,
    /// 此刻之前（只在问会话时有：照换号记录）＋ 此刻之后（推演）。
    pub plan: Vec<PlanSeg>,
    pub rows: Vec<PlanRow>,
    pub grid: Vec<GridMark>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub strategy_error: Option<StrategyError>,
}

/// 时间轴的刻度：按看的那一台的时区对齐（12h 一格 30m、每 2h 写字；24h 一格 1h、每 3h 写字）。
fn grid_of(
    view: &str,
    from: u64,
    until: u64,
    off: i64,
    text: &dyn Fn(u64) -> String,
) -> Vec<GridMark> {
    let (step, major): (i64, i64) = if view == "12h" {
        (1800, 7200)
    } else {
        (3600, 3 * 3600)
    };
    let (from, until) = (now_secs(from), now_secs(until));
    let mut t = (from + off).div_euclid(step) * step - off;
    if t < from {
        t += step;
    }
    let mut out = Vec::new();
    while t <= until {
        let at = u64::try_from(t).unwrap_or(0);
        let label = ((t + off).rem_euclid(major) == 0).then(|| {
            let local = (t + off).rem_euclid(86_400);
            format!("{:02}:{:02}", local / 3600, (local % 3600) / 60)
        });
        out.push(GridMark {
            at,
            at_text: text(at),
            label,
        });
        t += step;
    }
    out
}

/// 一个号在一个时刻的那一格是什么样子（`None` ＝ 能挑、没在用：那一格不画）。
fn bar_kind(snap: &crate::accounts::quota::plan::Snap, account: &str) -> Option<&'static str> {
    if snap.account.as_deref() == Some(account) {
        return Some("use");
    }
    let f = snap.facts.iter().find(|f| f.id == account)?;
    if let Some(w) = f.why_not {
        return Some(w.word());
    }
    let r = snap.rows.iter().find(|r| r.account == account)?;
    if r.p.is_some() {
        return None;
    }
    Some(match r.why.get("k").and_then(Value::as_str) {
        Some("@line") => "line",
        Some("@off") => "off",
        _ => "skip",
    })
}

/// `rotation-plan`：`{sid | config | draft, source?, view: 12h | 24h, lang?}` ⇒ 此刻往后怎么换（推演）＋ 合图每行（都不写）。
pub(crate) fn answer_plan(args: &Value, tz: &crate::Tz) -> Answer {
    let now = crate::accounts::quota::now_unix();
    let mut v = answer_plan_with(&Ctx::here().viewing(tz), args, now)?;
    crate::accounts::quota::name_words::with_names(&mut v);
    Ok(v)
}

pub(crate) fn answer_plan_with(ctx: &Ctx, args: &Value, now: u64) -> Answer {
    let (before, after, view) = view_secs(args)?;
    let (from, until) = (now.saturating_sub(before), now + after);
    let book = ctx.hop.store.now();
    let sub = subject(&book, args)?;
    let s = trial_strategy(ctx, args, &sub.body);
    let body = sub.body.clone();
    let row = |a: &str| (ctx.rows)(&sub.agent, a);
    let current0 = sub.current.clone().unwrap_or_else(|| sub.start.clone());
    let sc = ctx
        .hop
        .scene_of(&book, &sub.agent, &sub.start, &current0, &body, &row, now);
    let ready = |x: &str| ctx.hop.ready(&sub.agent, &sub.start, x, &row, now);
    let memo = sub
        .entry
        .as_ref()
        .and_then(|e| e.memo.clone())
        .unwrap_or(Value::Null);
    let snaps = crate::accounts::quota::plan::simulate(&crate::accounts::quota::plan::Input {
        strategy: s.as_deref(),
        body: &body,
        accounts: sc.accounts.clone(),
        now,
        until,
        current: current0.clone(),
        stint: sc.stint,
        rate: sc.rate,
        memo: memo.clone(),
        manual: sub
            .entry
            .as_ref()
            .map(|e| e.manual.clone())
            .unwrap_or_default(),
        can_hold: crate::agents::limit_reply_of(&sub.agent).is_some(),
        ready: &ready,
    });
    let clock = TextClock::new(now, &ctx.tz);
    let text = |t: u64| clock.text(t).0;
    let all = ctx.strategies();
    let lang = lang_of(args);
    let why_text = |strategy: &str, w: &Value| {
        strategy::find(&all, strategy)
            .map(|st| Speak::of(&st, lang, now, &ctx.tz).why(w))
            .unwrap_or_default()
    };
    let seg =
        |from: u64, to: u64, account: Option<String>, why: Option<String>, est: bool| PlanSeg {
            from,
            from_text: text(from),
            to,
            to_text: text(to),
            account,
            why,
            est,
        };
    let mut plan: Vec<PlanSeg> = Vec::new();
    // 此刻之前：问的是会话 ⇒ 照换号记录切段（没换成的那几种不切）。
    if let Some(e) = &sub.entry {
        let mut recs: Vec<&SwitchRecord> = e
            .history
            .iter()
            .filter(|h| h.from != h.to && h.at > from && h.at <= now)
            .collect();
        recs.sort_by_key(|h| h.at);
        let mut t = if recs.is_empty() {
            e.since.clamp(from, now)
        } else {
            from
        };
        let mut acct = recs
            .first()
            .map_or_else(|| e.current.clone(), |h| h.from.clone());
        let mut why: Option<String> = None;
        for h in recs {
            if h.at > t {
                plan.push(seg(t, h.at, Some(acct.clone()), why.clone(), false));
            }
            t = h.at;
            acct = h.to.clone();
            why = Some(record_text(h, &all, lang, now, &ctx.tz));
        }
        if now > t {
            plan.push(seg(t, now, Some(acct), why, false));
        }
    }
    // 此刻之后：推演的每一刻并成段。
    for (i, sn) in snaps.iter().enumerate() {
        let end = snaps.get(i + 1).map_or(until, |n| n.at);
        let why = sn.switched.as_ref().map(|w| why_text(&sn.strategy, w));
        match plan.last_mut() {
            Some(last)
                if last.account == sn.account
                    && why.is_none()
                    && last.to == sn.at
                    && last.est == sn.est =>
            {
                last.to = end;
                last.to_text = text(end);
            }
            _ => plan.push(seg(sn.at, end, sn.account.clone(), why, sn.est)),
        }
    }
    let mut rows: Vec<PlanRow> = sc
        .accounts
        .iter()
        .map(|(f, _)| PlanRow {
            account: f.id.clone(),
            bars: Vec::new(),
            resets: [("5h", f.w5h.resets), ("7d", f.w7d.resets)]
                .iter()
                .filter_map(|(w, t)| {
                    let at = t.filter(|t| *t > now && *t < until)?;
                    Some(ResetMark {
                        w: (*w).to_string(),
                        at,
                        at_text: text(at),
                    })
                })
                .collect(),
        })
        .collect();
    for (i, sn) in snaps.iter().enumerate() {
        let end = snaps.get(i + 1).map_or(until, |n| n.at);
        for r in &mut rows {
            let Some(k) = bar_kind(sn, &r.account) else {
                continue;
            };
            match r.bars.last_mut() {
                Some(b) if b.k == k && b.to == sn.at && b.est == sn.est => b.to = end,
                _ => r.bars.push(Bar {
                    from: sn.at,
                    to: end,
                    k: k.to_string(),
                    est: sn.est,
                }),
            }
        }
    }
    let name = s
        .as_ref()
        .map_or_else(|| strategy_name(&all, &body.strategy), |x| x.name.clone());
    let first_err = decide::judge(&decide::Judge {
        strategy: s.as_deref(),
        body: &body,
        accounts: &sc.accounts,
        now,
        current: &current0,
        stint: sc.stint,
        rate: sc.rate,
        memo: &memo,
    })
    .error
    .map(|f| strategy_error(&name, f));
    let off = ctx.tz.offset_secs(now_secs(now));
    wire(&PlanReply {
        now,
        now_text: text(now),
        from,
        until,
        view: view.to_string(),
        plan,
        rows,
        grid: grid_of(view, from, until, off, &text),
        strategy_error: first_err,
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_face_tests.rs"]
mod tests;
