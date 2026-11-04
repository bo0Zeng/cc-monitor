//! 离线那台的上次值那一份的存储：`~/.cc-monitor/cache/last-seen.json`（`relay_route_core::LAST_SEEN_FILE`，缓存）。
//! 盘上形状 `{version, origins: {<台>: {<样>: {atMs, shape, value}}}}`；一样是一条命令的应答，连同它此刻的形状指纹存。
//! 这里只管存（落点 · 锁里读改写 · 记一样 · 台数封顶 · 按形状指纹认一样），记哪几样、每样怎么核归记它的那一处：
//! 界面那两样（账号清单 · 「文件与数据」）在 `footprint/last_seen.rs`，组的上次所知在 `history/groups.rs`。
//! 读不懂 / 不在 / 旧版 ⇒ 照没记过（它只是缓存，下一次记的时候整份重来）。

use crate::common::said::Said;
use crate::platform::fs::{Own, OwnPath};
use copy_core::copy_text;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// 一样最大多少字节（序列化后）。
pub const MAX_VALUE_BYTES: relay_route_core::Cap = relay_route_core::Cap {
    bytes: 256 * 1024,
    over: relay_route_core::Over::Reject,
};
/// 最多记几台。
pub const MAX_ORIGINS: usize = 64;
/// 那一份：缓存（落点 · 版本 · 读上限照表）。
const DOC: crate::common::own_state::Doc<Store> =
    crate::common::own_state::Doc::of(&relay_route_core::LAST_SEEN_FILE);

/// 盘上那一份：各台住 `origins` 底下（顶层还有读写口带的 `version`，台名不和它同一层）。
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Store {
    #[serde(default)]
    origins: Map<String, Value>,
}

/// 记下的一样：那一刻（毫秒）· 记下时的形状指纹 · 应答原样。
pub struct Entry<'a> {
    pub at_ms: u64,
    pub shape: Option<&'a str>,
    pub value: Option<&'a Value>,
}

/// 这台机器上那份文件的路径；家目录解析不出来 ⇒ `None`。
pub fn store_path() -> Option<PathBuf> {
    Some(crate::platform::paths::home_dir()?.join(DOC.rel))
}

/// 各台那一层（读不懂 / 不在 / 旧版 ⇒ 空）。
pub fn load(path: &Path) -> Map<String, Value> {
    match DOC.read(path) {
        crate::common::own_state::Read::Present(s) => s.origins,
        _ => Map::new(),
    }
}

/// 一台那一层里的一样（没有时刻 ⇒ 照没记过）。
pub fn entry<'a>(mine: Option<&'a Value>, kind: &str) -> Option<Entry<'a>> {
    let e = mine?.get(kind)?;
    Some(Entry {
        at_ms: e.get("atMs")?.as_u64()?,
        shape: e.get("shape").and_then(Value::as_str),
        value: e.get("value"),
    })
}

/// 一样是这一版（记下时的指纹与 `shape` 相同、严格解得回 `T`）⇒ 解出来；否则 `None`。
pub fn this_version<T: DeserializeOwned + Serialize>(e: &Entry<'_>, shape: Option<&str>) -> Option<T> {
    e.value.filter(|_| e.shape == shape).and_then(strict::<T>)
}

/// 一份应答按回包类型严格收：解成类型、再写回去跟原样逐值比（多一格 · 缺一格 · 类型不对都不收）。
pub fn strict<T: DeserializeOwned + Serialize>(v: &Value) -> Option<T> {
    let r: T = serde_json::from_value(v.clone()).ok()?;
    (serde_json::to_value(&r).ok()? == *v).then_some(r)
}

/// 那份文件所在的那一层（`~/.cc-monitor/cache`）与它本身。没有家目录 ⇒ `io_failed`。
pub fn place(own: &Own) -> Result<(OwnPath, OwnPath), Said> {
    let no_home = |_| Said::new("io_failed", copy_text("beLastSeen.store.noHome", &[]));
    let dir = DOC.rel.rsplit_once('/').map_or(DOC.rel, |(d, _)| d);
    Ok((
        own.at_home(dir).map_err(no_home)?,
        own.at_home(DOC.rel).map_err(no_home)?,
    ))
}

/// [`place`] 那两层，目录建好：先 `~/.cc-monitor` 再它底下的缓存目录（建目录只建一层）。
pub fn made_place(own: &Own) -> Result<(OwnPath, OwnPath), Said> {
    let (dir, file) = place(own)?;
    let home = own.home_dir().ok();
    for d in home.iter().chain(std::iter::once(&dir)) {
        own.ensure_dir(d).map_err(|e| {
            Said::new(
                "io_failed",
                crate::common::said::IntoNote::into_note(Said::with_raw(
                    copy_text(
                        "beChore.marks.mkdirFailed",
                        &[
                            ("dir", &d.as_path().display().to_string()),
                            ("why", &copy_core::io_reason(e.kind())),
                        ],
                    ),
                    e,
                )),
            )
        })?;
    }
    Ok((dir, file))
}

/// 锁里读—改—写那一份（全仓唯一拿这一份跨进程锁的地方）；`edit` 回 `false` ⇒ 没改、不写。
/// 拿锁 · 写盘失败照原样交回（记它的那一处定码）。
pub fn edit(
    own: &Own,
    dir: &OwnPath,
    file: &OwnPath,
    edit: impl FnOnce(&mut Map<String, Value>) -> bool,
) -> Result<(), Said> {
    let held = own.lock(dir)?;
    let mut origins = load(file.as_path());
    if edit(&mut origins) {
        DOC.write(own, dir, &held, &Store { origins })?;
    }
    Ok(())
}

/// 记下一样；台数超了先丢最久没更新的那台（按那台各样里最新的时刻）。
pub fn record(
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
    entry[kind] = json!({"atMs": now_ms, "shape": shape, "value": value});
    while all.len() > MAX_ORIGINS {
        let newest = |m: &Value| {
            m.as_object()
                .into_iter()
                .flat_map(|o| o.values())
                .filter_map(|e| e.get("atMs")?.as_u64())
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
