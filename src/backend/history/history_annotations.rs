//! 历史注解（星标 / 改名 / 隐藏）—— **跟着会话住在那台**：每台的常驻后端读写自己这份，只记这台上的会话。注解是后端自有状态（`readonly_guard` 第四层），不是用户文件；
//! 每台出自己的清单时并上自己这份（`history_list.rs::with_own_annotations`），本机后端代问远端清单时照那台带来的并、不代写别台的。「上次用哪个号起的」不在这里（会话所在那台的 `launch-accounts.json`）。
//!
//! # 文件在哪
//!
//! 住这台的家：`<家>/history-metadata.json`（家 = `creds_core::store::monitor_data_dir`，默认 `~/.cc-monitor`，隔离跑时跟 `CCM_DATA_DIR` 走）。
//! 推不出家 ⇒ 注解不可用：读回「不知道」（三个数不说成 0）、写拒 —— 不猜一个路径去写。
//!
//! # 形状
//!
//! `{"version": u32, "entries": {sid: {"starred", "customTitle", "hidden", "updatedAt"}}}`；两个键认蛇形别名（`custom_title` / `updated_at`），缺格取缺省。
//!
//! # 写：一条不丢
//!
//! - 先严格读一遍：读不懂（坏 JSON / 类型不对 / 同一格驼峰与蛇形都在）⇒ 拒写，原文件一个字节不动（「读失败当空、只带这一条写回」会覆盖掉其余注解）。
//! - 再在原文 JSON 上只改那一条：其余条目、条目里认不出的键、顶层认不出的键原样留着；被改那一条按驼峰规范写回
//!   （它身上的蛇形别名摘掉 —— 不摘的话两个名字同时在，下一次严格读就读不懂了）。
//! - 第四层写法：`O_EXCL` 建临时文件 → 写满 → `sync` → 原子挪过去；目录不在只建那一层；失败删自己的临时文件。读—改—写在那个目录的跨进程锁里。
//!
//! 买不到：Windows 上「原子挪过去」是 `std::fs::rename`（覆盖既有文件），没在真 Windows 上跑过。

use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::common::said::IntoNote as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// 读一份注解文件的上限。超了 ⇒ 当读不懂：读回「不知道」、拒写（一个字节不动）。
/// 一条注解约 150 字节 ⇒ 64 MiB 装得下四十万条；真撞上说明那份文件不对劲，不拿截断的当完整的用。
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// 一条注解（别名与缺省见各字段）。
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    #[serde(default)]
    pub starred: bool,
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    pub custom_title: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, rename = "updatedAt", alias = "updated_at")]
    pub updated_at: i64,
}

/// 整份文件。
#[derive(Debug, Default, Deserialize)]
struct Doc {
    #[serde(default)]
    #[allow(dead_code)]
    // 读进来只为「类型对不对」那一道闸（同 monitor 那份）；值不参与任何判定
    version: u32,
    #[serde(default)]
    entries: BTreeMap<String, Entry>,
}

/// sid → 注解。
pub type Table = BTreeMap<String, Entry>;

/// 读的结局。「推不出家」「文件不在」「读不懂」三件事分开说 —— 后两件对界面的意思完全不同：
/// 文件不在 = 还没有任何注解（真的是 0）；读不懂 = 不知道（不许说成 0）。
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded {
    /// 推不出家（没有家目录 / `CCM_DATA_DIR` 设成了相对路径）—— 注解这一维不可用。
    NoPath,
    /// 读到了（文件不在 = 空表）。
    Read(Table),
    /// 读不懂 / 读不动 —— 带一句为什么。
    Unreadable(String),
}

/// 那份文件的路径：这台的家根上那一份；推不出家 ⇒ `None`。
pub fn path() -> Option<PathBuf> {
    path_from(&|k| std::env::var(k).ok())
}

/// [`path`] 有逻辑的那一半（环境取值器注入，判据不去动进程级环境变量）。
pub fn path_from(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    use creds_core::store::{monitor_data_dir, DATA_DIR_ENV, HISTORY_METADATA_FILE};
    let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into));
    monitor_data_dir(get(DATA_DIR_ENV).as_deref(), home).map(|d| d.join(HISTORY_METADATA_FILE))
}

/// 读原文（带上限）。`Ok(None)` = 文件不在。
fn read_raw(path: &Path) -> Result<Option<Vec<u8>>, crate::common::said::Said> {
    use crate::common::own_state::{read_bytes, Read};
    match read_bytes(path, MAX_BYTES) {
        Read::Absent => Ok(None),
        Read::Present(b) => Ok(Some(b)),
        Read::Unreadable(why) => Err(why),
    }
}

/// 严格读。
fn parse(bytes: &[u8], path: &Path) -> Result<Doc, String> {
    serde_json::from_slice::<Doc>(bytes).map_err(|e| {
        crate::common::said::Said::with_raw(
            copy_text(
                "beHistoryAnnotations.parse.unparsable",
                &[("path", &(path.display()).to_string())],
            ),
            e,
        )
        .into_note()
    })
}

/// 读一份（给 join 那一层）。
pub fn load_at(path: &Path) -> Loaded {
    match read_raw(path) {
        Ok(None) => Loaded::Read(Table::new()),
        Ok(Some(b)) => match parse(&b, path) {
            Ok(d) => Loaded::Read(d.entries),
            Err(e) => Loaded::Unreadable(e),
        },
        Err(e) => Loaded::Unreadable(e.into_note()),
    }
}

/// 读这台机器上那一份（生产入口）。
pub fn load() -> Loaded {
    match path() {
        Some(p) => load_at(&p),
        None => Loaded::NoPath,
    }
}

/// 读—改—写的锁：先建那一层目录，再拿它的跨进程锁（`platform/lock.rs`）。
fn lock_for_write(
    path: &Path,
) -> Result<crate::platform::lock::DirLock, crate::stream::inbound::spec::Fail> {
    let dir = path.parent().ok_or_else(|| {
        (
            "io_failed",
            copy_text(
                "beHistoryAnnotations.writeAt.noParent",
                &[("path", &(path.display()).to_string())],
            ),
        )
    })?;
    // 这一层是数据目录，默认就是 `~/.cc-monitor`（后端的家）⇒ 建的那一下只给本人。
    if let Err(e) = crate::common::own_dir::ensure_private_dir(dir) {
        return Err(crate::stream::inbound::spec::Fail::from((
            "io_failed",
            crate::common::said::Said::with_raw(
                copy_text(
                    "beHistoryAnnotations.writeAt.mkdirFailed",
                    &[
                        ("dir", &(dir.display()).to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ),
        )));
    }
    crate::platform::lock::hold(dir).map_err(|e| {
        crate::stream::inbound::spec::Fail::from(("io_failed", crate::common::said::Said::from(e)))
    })
}

/// 一次改动（线上 `patch`）：缺格 / `null` = 不改；标题给空白串 = 清空。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Patch {
    #[serde(default)]
    starred: Option<bool>,
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    custom_title: Option<Option<String>>,
    #[serde(default)]
    hidden: Option<bool>,
}

/// 被改那一条身上要摘掉的蛇形别名（写回时一律驼峰）。
const ALIASES: [&str; 2] = ["custom_title", "updated_at"];

fn sid_arg(args: &Value) -> Result<&str, crate::stream::inbound::spec::Fail> {
    let sid = args.get("sid").and_then(Value::as_str).ok_or_else(|| {
        (
            "bad_args",
            crate::common::contract::malformed("missing `sid` (a string)"),
        )
    })?;
    if sid.is_empty() || sid.len() > 256 || sid.chars().any(char::is_control) {
        return Err(crate::stream::inbound::spec::Fail::from((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`sid` does not look like a session id: {sid:?}"
            )),
        )));
    }
    Ok(sid)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 读原文成「严格读过的结构 ＋ 原文 JSON」。文件不在 ⇒ 缺省形状。
fn read_for_write(path: &Path) -> Result<(Doc, Value), crate::stream::inbound::spec::Fail> {
    match read_raw(path)
        .map_err(|e| crate::stream::inbound::spec::Fail::from(("annotations_unreadable", e)))?
    {
        None => Ok((Doc::default(), json!({ "version": 0, "entries": {} }))),
        Some(b) => {
            let doc = parse(&b, path).map_err(|e| ("annotations_unreadable", e))?;
            let raw: Value = serde_json::from_slice(&b)
                .map_err(|e| ("annotations_unreadable", format!("{e}")))?;
            if !raw.is_object() {
                return Err(crate::stream::inbound::spec::Fail::from((
                    "annotations_unreadable",
                    copy_text(
                        "beHistoryAnnotations.readForWrite.notObject",
                        &[("path", &(path.display()).to_string())],
                    ),
                )));
            }
            Ok((doc, raw))
        }
    }
}

/// 在原文 JSON 上把 `sid` 那一条换成 `entry`（驼峰规范；认不出的键留着）；`entry == None` ⇒ 删那一条。
fn put_entry(raw: &mut Value, sid: &str, entry: Option<&Entry>) {
    let root = raw.as_object_mut().expect("read_for_write 已核过是对象");
    let entries = root
        .entry("entries")
        .or_insert_with(|| Value::Object(Map::new()));
    if !entries.is_object() {
        *entries = Value::Object(Map::new());
    }
    let entries = entries.as_object_mut().expect("刚核过");
    match entry {
        None => {
            entries.remove(sid);
        }
        Some(e) => {
            let mut obj = match entries.remove(sid) {
                Some(Value::Object(o)) => o,
                _ => Map::new(),
            };
            for a in ALIASES {
                obj.remove(a);
            }
            if let Value::Object(norm) = serde_json::to_value(e).expect("Entry 总能序列化") {
                for (k, v) in norm {
                    obj.insert(k, v);
                }
            }
            entries.insert(sid.to_string(), Value::Object(obj));
        }
    }
}

/// 唯一的写者（经 `own_state` 原子写；目录由 [`lock_for_write`] 建）。序列化用 `to_string_pretty`。
fn write_at(path: &Path, raw: &Value) -> Result<(), crate::common::said::Said> {
    let body = serde_json::to_string_pretty(raw).map_err(|e| {
        crate::common::said::Said::from(crate::common::contract::malformed(&format!(
            "serializing the annotations failed: {e}"
        )))
    })?;
    crate::common::own_state::write(path, body.as_bytes())
}

fn need_path() -> Result<PathBuf, crate::stream::inbound::spec::Fail> {
    path().ok_or_else(|| {
        crate::stream::inbound::spec::Fail::from((
            "no_annotations",
            copy_text(
                "beHistoryAnnotations.needPath.unknown",
                &[("env", creds_core::store::DATA_DIR_ENV)],
            ),
        ))
    })
}

/// `history-annotate {sid, patch}`：改一条，回改完的那一条（`{entry}`）。
pub fn answer_annotate(args: &Value) -> Result<Value, crate::stream::inbound::spec::Fail> {
    answer_annotate_at(&need_path()?, args, now_ms())
}

/// [`answer_annotate`] 的本体：路径与「现在」由调用方给（判据喂临时目录与固定时刻）。
pub fn answer_annotate_at(
    path: &Path,
    args: &Value,
    now: i64,
) -> Result<Value, crate::stream::inbound::spec::Fail> {
    let sid = sid_arg(args)?;
    let patch: Patch = match args.get("patch") {
        Some(p @ Value::Object(_)) => serde_json::from_value(p.clone()).map_err(|e| {
            (
                "bad_args",
                crate::common::contract::malformed(&format!("`patch` unreadable: {e}")),
            )
        })?,
        _ => {
            return Err(crate::stream::inbound::spec::Fail::from((
                "bad_args",
                crate::common::contract::malformed("missing `patch` (an object)"),
            )))
        }
    };
    let _g = lock_for_write(path)?;
    let (doc, mut raw) = read_for_write(path)?;
    let mut entry = doc.entries.get(sid).cloned().unwrap_or_default();
    if let Some(s) = patch.starred {
        entry.starred = s;
    }
    if let Some(t) = patch.custom_title {
        entry.custom_title = t.filter(|s| !s.trim().is_empty());
    }
    if let Some(h) = patch.hidden {
        entry.hidden = h;
    }
    entry.updated_at = now;
    put_entry(&mut raw, sid, Some(&entry));
    write_at(path, &raw).map_err(|e| crate::stream::inbound::spec::Fail::from(("io_failed", e)))?;
    crate::stream::inbound::spec::wire(&Annotated { entry })
}

/// `history-annotate` 的应答：改完的那一条。
#[derive(Debug, Serialize)]
pub(crate) struct Annotated {
    pub(crate) entry: Entry,
}

/// `history-forget` 的应答：真删了一条没有（`false` = 本来就没有这一条，文件没动）。
#[derive(Debug, Serialize)]
pub(crate) struct Forgotten {
    pub(crate) removed: bool,
}

/// `history-forget {sid}`：删那一条（删会话时连带）。不在 ⇒ 不写。回 `{removed}`。
pub fn answer_forget(args: &Value) -> Result<Value, crate::stream::inbound::spec::Fail> {
    answer_forget_at(&need_path()?, args)
}

/// [`answer_forget`] 的本体。
pub fn answer_forget_at(
    path: &Path,
    args: &Value,
) -> Result<Value, crate::stream::inbound::spec::Fail> {
    let sid = sid_arg(args)?;
    let _g = lock_for_write(path)?;
    let (doc, mut raw) = read_for_write(path)?;
    if !doc.entries.contains_key(sid) {
        return crate::stream::inbound::spec::wire(&Forgotten { removed: false });
    }
    put_entry(&mut raw, sid, None);
    write_at(path, &raw).map_err(|e| crate::stream::inbound::spec::Fail::from(("io_failed", e)))?;
    crate::stream::inbound::spec::wire(&Forgotten { removed: true })
}

#[cfg(test)]
#[path = "../../../tests/backend/history/history_annotations_tests.rs"]
mod tests;
