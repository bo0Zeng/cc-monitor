//! 离线那台的上次值：本机后端替界面记下每台最近一次读成的那几份（账号清单 · 「文件与数据」那一份），断开时界面照它画「上次的」，跨重启还在。
//! 住 `~/.cc-monitor/last-seen.json`（`relay_route_core::LAST_SEEN_REL`），后端自己的状态；能重建（再连上一次就有），按缓存记。
//! 全仓唯一的写者是 [`answer_write`]（记下一份 · 删机器时清掉那台）；读不懂 ⇒ 照没记过算，写的那一下整份重来（它只是缓存）。
//! `data` 那一份按 `data-report` 的回包类型核：写时核过才收、连同形状指纹存，读时不是这一版 ⇒ 说读不懂（[`SeenData`]）。
//! 种类闭集 [`KINDS`]；每份封顶 [`MAX_VALUE_BYTES`]、台数封顶 [`MAX_ORIGINS`]（超了先丢最久没更新的那台）。

use crate::platform::fs::{Own, OwnPath};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// 记哪一样。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
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
}

/// `last-seen-read` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct LastSeenReadReq {
    /// 哪台（机器名）。
    #[cfg_attr(test, ts(as = "String"))]
    pub(crate) origin: cmd_wire::Machine,
}

/// `last-seen-write` 的入参：记一份 `{origin, kind, value}`，或清掉那台 `{origin, forget: true}`（不带 `kind` / `value`）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct LastSeenWriteReq {
    #[cfg_attr(test, ts(as = "String"))]
    pub(crate) origin: cmd_wire::Machine,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) kind: Option<SeenKind>,
    /// 那一份应答（对象，序列化后 ≤ 256 KiB）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional, type = "Record<string, unknown>"))]
    pub(crate) value: Option<Map<String, Value>>,
    /// `true` ⇒ 清掉这台的上次值（删机器时）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) forget: Option<bool>,
}

/// 记下的一份。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SeenEntry {
    /// 记下的时刻（毫秒）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) at_ms: u64,
    /// 那一份应答原样。
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub(crate) value: Map<String, Value>,
}

/// 记下的 `data-report` 那一份，按那条命令的回包类型核过：对得上 ⇒ 原样（已是类型）；
/// 存档不是此刻这一版（形状指纹不同 · 旧版存的没带指纹 · 解不回回包类型）⇒ 照实说读不懂，原样不交出去。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "state", rename_all = "camelCase")]
pub(crate) enum SeenData {
    Kept {
        /// 记下的时刻（毫秒）。
        #[serde(rename = "atMs")]
        #[cfg_attr(test, ts(rename = "atMs", type = "number"))]
        at_ms: u64,
        value: crate::footprint::data::DataReport,
    },
    Unreadable {
        #[serde(rename = "atMs")]
        #[cfg_attr(test, ts(rename = "atMs", type = "number"))]
        at_ms: u64,
    },
}

/// `last-seen-read` 的回包：没记过的那一样 ⇒ `null`。
/// `accounts` 那一份（`accounts-list` 还不是 typed）照旧原样交；它 typed 时同 `data` 一个办法。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LastSeen {
    pub(crate) accounts: Option<SeenEntry>,
    pub(crate) data: Option<SeenData>,
}

/// `last-seen-write` 的回包。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct LastSeenWritten {
    /// 记下的时刻（毫秒）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) at_ms: u64,
}

/// `data` 那一样是哪条命令的回包（存档记下它此刻的形状指纹，读回来对一下是不是这一版）。
const DATA_CMD: &str = "data-report";

/// 一份 `data-report` 应答按回包类型严格收：解成类型、再写回去跟原样逐值比（多一格 · 缺一格 · 类型不对都不收）。
fn data_report_of(v: &Value) -> Option<crate::footprint::data::DataReport> {
    let r: crate::footprint::data::DataReport = serde_json::from_value(v.clone()).ok()?;
    (serde_json::to_value(&r).ok()? == *v).then_some(r)
}

/// 记哪几样（闭集，同 [`SeenKind`]）：`accounts` ＝ `accounts-list` 的应答 · `data` ＝ `data-report` 的应答。
pub(crate) const KINDS: [&str; 2] = ["accounts", "data"];
/// 一份最大多少字节（序列化后）。
pub(crate) const MAX_VALUE_BYTES: usize = 256 * 1024;
/// 最多记几台。
pub(crate) const MAX_ORIGINS: usize = 64;
/// 整份文件最大多少字节（超了当读不懂）：每份封顶 × 两样 × 台数封顶 ＝ 256 KiB × 2 × 64。
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
/// 机器名最长多少字节。
const MAX_ORIGIN_BYTES: usize = 256;

type Answer<T> = Result<T, crate::stream::inbound::spec::Fail>;

fn bad(d: &str) -> crate::stream::inbound::spec::Fail {
    crate::stream::inbound::spec::Fail::new("bad_args", crate::common::contract::malformed(d))
}

/// 这台机器上那份文件的路径；家目录解析不出来 ⇒ `None`。
pub(crate) fn store_path() -> Option<PathBuf> {
    Some(crate::platform::paths::home_dir()?.join(relay_route_core::LAST_SEEN_REL))
}

fn origin_arg(m: &cmd_wire::Machine) -> Result<&str, crate::stream::inbound::spec::Fail> {
    Some(m.0.as_str())
        .filter(|s| !s.trim().is_empty() && s.len() <= MAX_ORIGIN_BYTES)
        .ok_or_else(|| bad("`origin` missing or not a short non-empty string"))
}

/// 整份（读不懂 / 不在 ⇒ 空）。形状：`{<origin>: {<kind>: {atMs, value}}}`。
fn load(path: &Path) -> Map<String, Value> {
    match crate::common::own_state::read_json::<Map<String, Value>>(path, MAX_FILE_BYTES) {
        crate::common::own_state::Read::Present(m) => m,
        crate::common::own_state::Read::Absent => Map::new(),
        crate::common::own_state::Read::Unreadable(why) => {
            tracing::warn!(
                "上次值 {} 读不懂（{why}）⇒ 照没记过算，下一次记的时候整份重来",
                path.display()
            );
            Map::new()
        }
    }
}

/// `{origin}` ⇒ `{accounts: {atMs, value} | null, data: … | null}`（盘上那一项形状不对 ⇒ 照没记过）。
pub(crate) fn read_at(path: &Path, req: &LastSeenReadReq) -> Answer<LastSeen> {
    let origin = origin_arg(&req.origin)?;
    let all = load(path);
    let mine = all.get(origin);
    let got = |k: SeenKind| {
        mine.and_then(|m| m.get(k.key()))
            .and_then(|e| serde_json::from_value::<SeenEntry>(e.clone()).ok())
    };
    let data = mine
        .and_then(|m| m.get(SeenKind::Data.key()))
        .and_then(|e| {
            let at_ms = e.get("atMs")?.as_u64()?;
            let this_version =
                e.get("shape").and_then(Value::as_str) == cmd_wire::shapes::shape_of(DATA_CMD);
            Some(
                match e
                    .get("value")
                    .filter(|_| this_version)
                    .and_then(data_report_of)
                {
                    Some(value) => SeenData::Kept { at_ms, value },
                    None => SeenData::Unreadable { at_ms },
                },
            )
        });
    Ok(LastSeen {
        accounts: got(SeenKind::Accounts),
        data,
    })
}

/// `{origin, kind, value}` ⇒ 记下（时刻由这里盖）；`{origin, forget: true}` ⇒ 清掉那台（删机器时）。回 `{atMs}`。
pub(crate) fn write_at(own: &Own, req: &LastSeenWriteReq, now_ms: u64) -> Answer<LastSeenWritten> {
    if req.forget.is_some() {
        return forget_at(own, req, now_ms);
    }
    let origin = origin_arg(&req.origin)?.to_string();
    let kind = req
        .kind
        .ok_or_else(|| bad("`kind` is not one of accounts / data"))?;
    let value = Value::Object(
        req.value
            .clone()
            .ok_or_else(|| bad("`value` must be an object"))?,
    );
    let size = value.to_string().len();
    if size > MAX_VALUE_BYTES {
        return Err(crate::stream::inbound::spec::Fail::from((
            "too_large",
            copy_text(
                "beLastSeen.value.tooLarge",
                &[
                    ("size", &size.to_string()),
                    ("max", &MAX_VALUE_BYTES.to_string()),
                ],
            ),
        )));
    }
    // `data` 那一份按回包类型核过才收，连同此刻的形状指纹一起存（读回来据此认是不是这一版）。
    let shape = match kind {
        SeenKind::Data => {
            data_report_of(&value)
                .ok_or_else(|| bad("`value` is not a `data-report` reply of this version"))?;
            cmd_wire::shapes::shape_of(DATA_CMD)
        }
        SeenKind::Accounts => None,
    };
    let kind = kind.key();
    let (dir, file) = place(own)?;
    own.ensure_dir(&dir).map_err(|e| {
        (
            "io_failed",
            crate::common::said::IntoNote::into_note(crate::common::said::Said::with_raw(
                copy_text(
                    "beLastSeen.store.mkdirFailed",
                    &[
                        ("dir", &dir.as_path().display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                e,
            )),
        )
    })?;
    locked_edit(own, &dir, &file, |all| {
        record(all, origin, kind, &value, shape, now_ms);
        true
    })?;
    Ok(LastSeenWritten { at_ms: now_ms })
}

/// 锁里读—改—写那一份（本文件唯一拿跨进程锁的地方）；`edit` 回 `false` ⇒ 没改、不写。
fn locked_edit(
    own: &Own,
    dir: &OwnPath,
    file: &OwnPath,
    edit: impl FnOnce(&mut Map<String, Value>) -> bool,
) -> Result<(), crate::stream::inbound::spec::Fail> {
    let io = |e| crate::stream::inbound::spec::Fail::from(("io_failed", e));
    let held = own
        .lock(dir)
        .map_err(|e| io(crate::common::said::Said::from(e)))?;
    let mut all = load(file.as_path());
    if edit(&mut all) {
        let body = crate::common::own_state::json_body(file.as_path(), &all).map_err(io)?;
        own.write(file, &held, &body).map_err(io)?;
    }
    Ok(())
}

/// 那份文件所在的那一层（`~/.cc-monitor`）与它本身。没有家目录 ⇒ `io_failed`。
fn place(own: &Own) -> Result<(OwnPath, OwnPath), crate::stream::inbound::spec::Fail> {
    let no_home = |_| {
        crate::stream::inbound::spec::Fail::from((
            "io_failed",
            copy_text("beLastSeen.store.noHome", &[]),
        ))
    };
    Ok((
        own.home_dir().map_err(no_home)?,
        own.at_home(relay_route_core::LAST_SEEN_REL)
            .map_err(no_home)?,
    ))
}

/// 记下一份；台数超了先丢最久没更新的那台。
fn record(
    all: &mut Map<String, Value>,
    origin: String,
    kind: &str,
    value: &Value,
    shape: Option<&str>,
    now_ms: u64,
) {
    let entry = all
        .entry(origin)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry[kind] = match shape {
        Some(shape) => json!({"atMs": now_ms, "shape": shape, "value": value}),
        None => json!({"atMs": now_ms, "value": value}),
    };
    while all.len() > MAX_ORIGINS {
        let newest = |m: &Value| {
            KINDS
                .iter()
                .filter_map(|k| m.get(*k)?.get("atMs")?.as_u64())
                .max()
                .unwrap_or(0)
        };
        let Some(oldest) = all
            .iter()
            .min_by_key(|(_, m)| newest(m))
            .map(|(k, _)| k.clone())
        else {
            break;
        };
        all.remove(&oldest);
    }
}

/// 清掉一台的上次值（两样都清）。只认 `{origin, forget: true}`：带着 `kind` / `value` 不收。那台没记过 ⇒ 照样回成、盘上不动。
fn forget_at(own: &Own, req: &LastSeenWriteReq, now_ms: u64) -> Answer<LastSeenWritten> {
    let origin = origin_arg(&req.origin)?;
    if req.forget != Some(true) || req.kind.is_some() || req.value.is_some() {
        return Err(bad(
            "`forget` must be `true` and come without `kind` / `value`",
        ));
    }
    let (dir, file) = place(own)?;
    if !dir.as_path().is_dir() {
        return Ok(LastSeenWritten { at_ms: now_ms });
    }
    locked_edit(own, &dir, &file, |all| all.remove(origin).is_some())?;
    Ok(LastSeenWritten { at_ms: now_ms })
}

fn now_ms() -> u64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// `last-seen-read`：帧面入口（只读）。
pub(crate) fn answer_read(req: &LastSeenReadReq) -> Answer<LastSeen> {
    let path = store_path().ok_or(("io_failed", copy_text("beLastSeen.store.noHome", &[])))?;
    read_at(&path, req)
}

/// `last-seen-write`：帧面入口（**写口**，只从 `stream/inbound/` 进）。
pub(crate) fn answer_write(req: &LastSeenWriteReq, own: &Own) -> Answer<LastSeenWritten> {
    write_at(own, req, now_ms())
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/last_seen_tests.rs"]
mod tests;
