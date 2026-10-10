//! 离线那台的上次值：本机后端替界面记下每台最近一次读成的那几份（账号清单 · 「文件与数据」那一份），连不上时界面照它画「上次的」，跨重启还在。
//! 住 `~/.cc-monitor/last-seen.json`（`relay_route_core::LAST_SEEN_REL`），后端自己的状态；能重建（再连上一次就有），按缓存记。
//! 全仓唯一的写者是 [`answer_write`]（记下一份 · 删机器时清掉那台）；读不懂 ⇒ 照没记过算，写的那一下整份重来（它只是缓存）。
//! 种类闭集 [`KINDS`]；每份封顶 [`MAX_VALUE_BYTES`]、台数封顶 [`MAX_ORIGINS`]（超了先丢最久没更新的那台）。

use copy_core::copy_text;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// 记哪几样（闭集）：`accounts` ＝ `accounts-list` 的应答 · `data` ＝ `data-report` 的应答。
pub(crate) const KINDS: [&str; 2] = ["accounts", "data"];
/// 一份最大多少字节（序列化后）。
pub(crate) const MAX_VALUE_BYTES: usize = 256 * 1024;
/// 最多记几台。
pub(crate) const MAX_ORIGINS: usize = 64;
/// 整份文件最大多少字节（超了当读不懂）：每份封顶 × 两样 × 台数封顶 ＝ 256 KiB × 2 × 64。
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
/// 机器名最长多少字节。
const MAX_ORIGIN_BYTES: usize = 256;

type Answer = Result<Value, crate::stream::inbound::spec::Fail>;

fn bad(d: &str) -> crate::stream::inbound::spec::Fail {
    crate::stream::inbound::spec::Fail::new("bad_args", crate::common::contract::malformed(d))
}

/// 这台机器上那份文件的路径；家目录解析不出来 ⇒ `None`。
pub(crate) fn store_path() -> Option<PathBuf> {
    Some(crate::platform::paths::home_dir()?.join(relay_route_core::LAST_SEEN_REL))
}

fn origin_arg(args: &Value) -> Result<&str, crate::stream::inbound::spec::Fail> {
    args.get("origin")
        .and_then(Value::as_str)
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

/// `{origin}` ⇒ `{accounts: {atMs, value} | null, data: … | null}`。
pub(crate) fn read_at(path: &Path, args: &Value) -> Answer {
    let origin = origin_arg(args)?;
    let all = load(path);
    let mine = all.get(origin);
    let mut out = Map::new();
    for k in KINDS {
        let got = mine
            .and_then(|m| m.get(k))
            .filter(|e| {
                e.get("atMs").is_some_and(Value::is_u64)
                    && e.get("value").is_some_and(Value::is_object)
            })
            .cloned()
            .unwrap_or(Value::Null);
        out.insert(k.to_string(), got);
    }
    Ok(Value::Object(out))
}

/// `{origin, kind, value}` ⇒ 记下（时刻由这里盖）；`{origin, forget: true}` ⇒ 清掉那台（删机器时）。回 `{atMs}`。
pub(crate) fn write_at(path: &Path, args: &Value, now_ms: u64) -> Answer {
    if args.get("forget").is_some() {
        return forget_at(path, args, now_ms);
    }
    let origin = origin_arg(args)?.to_string();
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .filter(|k| KINDS.contains(k))
        .ok_or_else(|| bad("`kind` is not one of accounts / data"))?;
    let value = args
        .get("value")
        .filter(|v| v.is_object())
        .ok_or_else(|| bad("`value` must be an object"))?;
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
    let dir = path
        .parent()
        .ok_or_else(|| bad("store path has no parent"))?;
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        (
            "io_failed",
            crate::common::said::IntoNote::into_note(crate::common::said::Said::with_raw(
                copy_text(
                    "beLastSeen.store.mkdirFailed",
                    &[
                        ("dir", &dir.display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                e,
            )),
        )
    })?;
    locked_edit(dir, path, |all| {
        record(all, origin, kind, value, now_ms);
        true
    })?;
    Ok(json!({ "atMs": now_ms }))
}

/// 锁里读—改—写那一份（本文件唯一拿跨进程锁的地方）；`edit` 回 `false` ⇒ 没改、不写。
fn locked_edit(
    dir: &Path,
    path: &Path,
    edit: impl FnOnce(&mut Map<String, Value>) -> bool,
) -> Result<(), crate::stream::inbound::spec::Fail> {
    let _g = crate::platform::lock::hold(dir).map_err(|e| {
        crate::stream::inbound::spec::Fail::from(("io_failed", crate::common::said::Said::from(e)))
    })?;
    let mut all = load(path);
    if edit(&mut all) {
        crate::common::own_state::write_json(path, &all)
            .map_err(|e| crate::stream::inbound::spec::Fail::from(("io_failed", e)))?;
    }
    Ok(())
}

/// 记下一份；台数超了先丢最久没更新的那台。
fn record(all: &mut Map<String, Value>, origin: String, kind: &str, value: &Value, now_ms: u64) {
    let entry = all
        .entry(origin)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry[kind] = json!({"atMs": now_ms, "value": value});
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
fn forget_at(path: &Path, args: &Value, now_ms: u64) -> Answer {
    let origin = origin_arg(args)?;
    if args.get("forget") != Some(&Value::Bool(true))
        || args.get("kind").is_some()
        || args.get("value").is_some()
    {
        return Err(bad(
            "`forget` must be `true` and come without `kind` / `value`",
        ));
    }
    let Some(dir) = path.parent().filter(|d| d.is_dir()) else {
        return Ok(json!({ "atMs": now_ms }));
    };
    locked_edit(dir, path, |all| all.remove(origin).is_some())?;
    Ok(json!({ "atMs": now_ms }))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// `last-seen-read`：帧面入口（只读）。
pub(crate) fn answer_read(args: &Value) -> Answer {
    let path = store_path().ok_or(("io_failed", copy_text("beLastSeen.store.noHome", &[])))?;
    read_at(&path, args)
}

/// `last-seen-write`：帧面入口（**写口**，只从 `stream/inbound/` 进）。
pub(crate) fn answer_write(args: &Value) -> Answer {
    let path = store_path().ok_or(("io_failed", copy_text("beLastSeen.store.noHome", &[])))?;
    write_at(&path, args, now_ms())
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/last_seen_tests.rs"]
mod tests;
