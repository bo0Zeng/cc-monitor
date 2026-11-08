//! 离线那台的上次值：本机后端替界面记下每台最近一次读成的那几份（账号清单 · 「文件与数据」那一份），断开时界面照它画「上次的」，跨重启还在。
//! 住 `~/.cc-monitor/cache/last-seen.json`（`relay_route_core::LAST_SEEN_REL`），别台那几份的副本，按缓存记（再连上一次就有）；存储在 `common/last_seen_store.rs`
//! （同一份里还记着组的上次所知，`history/groups.rs` 记、读，这两条命令不碰它，删机器时整台一起清）。
//! 界面那两样唯一的写者是 [`answer_write`]（记下一份 · 删机器时清掉那台）；同一份再交照记（时刻换成这一次）；读不懂 ⇒ 照没记过算，写的那一下整份重来（它只是缓存）。
//! 每一样按它那条命令的回包类型核：写时核过才收、连同形状指纹存，读时不是这一版 ⇒ 说读不懂（[`Seen`]）。
//! 种类闭集 [`SeenKind`]；每份封顶 [`MAX_VALUE_BYTES`]、台数封顶 [`store::MAX_ORIGINS`]（超了先丢最久没更新的那台）。

use crate::common::last_seen_store::{self as store, strict, MAX_VALUE_BYTES};
use crate::common::said::Said;
use crate::platform::fs::Own;
use cmd_wire::cells::SeenAt;
use copy_core::copy_text;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// 记哪一样。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub(crate) enum SeenKind {
    /// `accounts-list` 的应答。
    Accounts,
    /// `data-report` 的应答。
    Data,
}

impl SeenKind {
    fn key(self) -> &'static str {
        match self {
            SeenKind::Accounts => "accounts",
            SeenKind::Data => "data",
        }
    }

    /// 这一样是哪条命令的回包（存档记下它此刻的形状指纹，读回来对一下是不是这一版）。
    fn cmd(self) -> &'static str {
        match self {
            SeenKind::Accounts => "accounts-list",
            SeenKind::Data => "data-report",
        }
    }

    /// 一份应答是不是这条命令这一版的回包（严格收，见 [`strict`]）。
    fn fits(self, v: &Value) -> bool {
        match self {
            SeenKind::Accounts => strict::<cmd_wire::accounts::AccountsList>(v).is_some(),
            SeenKind::Data => strict::<crate::footprint::data::DataReport>(v).is_some(),
        }
    }
}

/// `last-seen-read` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct LastSeenReadReq {
    /// 哪台（机器名）。
    pub(crate) origin: cmd_wire::Machine,
}

/// `last-seen-write` 的入参：记一份 `{origin, kind, value}`，或清掉那台 `{origin, forget: true}`（不带 `kind` / `value`）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct LastSeenWriteReq {
    pub(crate) origin: cmd_wire::Machine,
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub(crate) kind: Option<SeenKind>,
    /// 那一份应答（对象，序列化后 ≤ 256 KiB）。
    #[serde(default)]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(optional, type = "Record<string, unknown>")
    )]
    pub(crate) value: Option<Map<String, Value>>,
    /// `true` ⇒ 清掉这台的上次值（删机器时）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub(crate) forget: Option<bool>,
}

/// 记下的一份，按那条命令的回包类型核过：对得上 ⇒ 原样（已是类型）；
/// 存档不是此刻这一版（形状指纹不同 · 旧版存的没带指纹 · 解不回回包类型）⇒ 照实说读不懂，原样不交出去。两样都带上次读数那一格。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "state", rename_all = "camelCase")]
pub(crate) enum Seen<T> {
    Kept { seen: SeenAt, value: T },
    Unreadable { seen: SeenAt },
}

/// `last-seen-read` 的回包：没记过的那一样 ⇒ `null`。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct LastSeen {
    pub(crate) accounts: Option<Seen<cmd_wire::accounts::AccountsList>>,
    pub(crate) data: Option<Seen<crate::footprint::data::DataReport>>,
}

/// `last-seen-write` 的回包。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct LastSeenWritten {
    /// 记下的时刻（毫秒）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub(crate) at_ms: u64,
}

/// 机器名最长多少字节。
const MAX_ORIGIN_BYTES: usize = 256;

type Answer<T> = Result<T, Said>;

fn bad(d: &str) -> Said {
    crate::wire::spec::fail("bad_args", crate::common::contract::malformed(d))
}

/// 这台机器上那份文件的路径；家目录解析不出来 ⇒ `None`。
pub(crate) fn store_path() -> Option<PathBuf> {
    store::store_path()
}

fn origin_arg(m: &cmd_wire::Machine) -> Result<String, Said> {
    Some(m.to_string())
        .filter(|s| !s.trim().is_empty() && s.len() <= MAX_ORIGIN_BYTES)
        .ok_or_else(|| bad("`origin` missing or not a short non-empty string"))
}

/// `{origin}` ⇒ `{accounts: … | null, data: … | null}`（盘上那一项没有时刻 ⇒ 照没记过）。
pub(crate) fn read_at(
    path: &Path,
    req: &LastSeenReadReq,
    clock: &crate::common::time::TextClock,
) -> Answer<LastSeen> {
    let origin = origin_arg(&req.origin)?;
    let all = store::load(path);
    let mine = all.get(&origin);
    Ok(LastSeen {
        accounts: seen_of(mine, SeenKind::Accounts, clock).map(stale_quota),
        data: seen_of(mine, SeenKind::Data, clock),
    })
}

/// 读回的账号清单是缓存 ⇒ 每号每一格额度写成「… · 上次所知」（`beQuota.cell.stale`），出口照摆。
fn stale_quota(
    s: Seen<cmd_wire::accounts::AccountsList>,
) -> Seen<cmd_wire::accounts::AccountsList> {
    match s {
        Seen::Kept { seen, mut value } => {
            for s in value.accounts.iter_mut().flat_map(|r| r.slots.iter_mut()) {
                s.text = crate::common::cells::Words(copy_text(
                    "beQuota.cell.stale",
                    &[("cell", &s.text.0)],
                ));
            }
            Seen::Kept { seen, value }
        }
        u @ Seen::Unreadable { .. } => u,
    }
}

/// 盘上那一项 ⇒ 是这一版（指纹对得上、解得回回包类型）就原样，否则读不懂。
fn seen_of<T: DeserializeOwned + Serialize>(
    mine: Option<&Value>,
    kind: SeenKind,
    clock: &crate::common::time::TextClock,
) -> Option<Seen<T>> {
    let e = store::entry(mine, kind.key())?;
    let seen = store::seen_at(e.at_ms, clock);
    Some(
        match store::this_version::<T>(&e, cmd_wire::shapes::shape_of(kind.cmd())) {
            Some(value) => Seen::Kept { seen, value },
            None => Seen::Unreadable { seen },
        },
    )
}

/// `{origin, kind, value}` ⇒ 记下（时刻由这里盖，同一份再交也换成这一次）；`{origin, forget: true}` ⇒ 清掉那台（删机器时）。回 `{atMs}`。
pub(crate) fn write_at(own: &Own, req: &LastSeenWriteReq, now_ms: u64) -> Answer<LastSeenWritten> {
    if req.forget.is_some() {
        return forget_at(own, req, now_ms);
    }
    let origin = origin_arg(&req.origin)?;
    let kind = req
        .kind
        .ok_or_else(|| bad("`kind` is not one of accounts / data"))?;
    let value = Value::Object(
        req.value
            .clone()
            .ok_or_else(|| bad("`value` must be an object"))?,
    );
    let size = value.to_string().len();
    if size > MAX_VALUE_BYTES.as_len() {
        return Err(crate::wire::spec::fail(
            "too_large",
            copy_text(
                "beLastSeen.value.tooLarge",
                &[
                    ("size", &size.to_string()),
                    ("max", &MAX_VALUE_BYTES.bytes.to_string()),
                ],
            ),
        ));
    }
    // 按回包类型核过才收，连同此刻的形状指纹一起存（读回来据此认是不是这一版）。
    if !kind.fits(&value) {
        return Err(bad("`value` is not a reply of this version to its command"));
    }
    let shape = cmd_wire::shapes::shape_of(kind.cmd());
    let (dir, file) = store::made_place(own).map_err(io)?;
    store::edit(own, &dir, &file, |all| {
        store::record(all, origin, kind.key(), &value, shape, now_ms);
        true
    })
    .map_err(io)?;
    Ok(LastSeenWritten { at_ms: now_ms })
}

/// 存储那一层的失败照 `io_failed` 交出去。
fn io(e: Said) -> Said {
    crate::wire::spec::fail("io_failed", e)
}

/// 清掉一台的上次值（每一样都清）。只认 `{origin, forget: true}`：带着 `kind` / `value` 不收。那台没记过 ⇒ 照样回成、盘上不动。
fn forget_at(own: &Own, req: &LastSeenWriteReq, now_ms: u64) -> Answer<LastSeenWritten> {
    let origin = origin_arg(&req.origin)?;
    if req.forget != Some(true) || req.kind.is_some() || req.value.is_some() {
        return Err(bad(
            "`forget` must be `true` and come without `kind` / `value`",
        ));
    }
    let (dir, file) = store::place(own).map_err(io)?;
    if !dir.as_path().is_dir() {
        return Ok(LastSeenWritten { at_ms: now_ms });
    }
    store::edit(own, &dir, &file, |all| all.remove(&origin).is_some()).map_err(io)?;
    Ok(LastSeenWritten { at_ms: now_ms })
}

/// `last-seen-read`：帧面入口（只读）；那一句按看的那一台的时区（请求信封的 `tz`）写。
pub fn answer_read(req: &LastSeenReadReq, tz: &crate::common::time::Tz) -> Answer<LastSeen> {
    let path = store_path()
        .ok_or_else(|| Said::new("io_failed", copy_text("beLastSeen.store.noHome", &[])))?;
    read_at(
        &path,
        req,
        &crate::common::time::TextClock::new(crate::common::time::now_secs(), tz),
    )
}

/// `last-seen-write`：帧面入口（**写口**，只从 `stream/inbound/` 进）。
pub fn answer_write(req: &LastSeenWriteReq, own: &Own) -> Answer<LastSeenWritten> {
    write_at(own, req, crate::common::time::now_ms())
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/last_seen_tests.rs"]
mod tests;
