//! **历史注解**（星标 / 改名 / 隐藏 / 上次用哪个号起）—— 读写者是本机常驻后端。
//!
//! # 裁决与出处
//!
//! 主会话 09-25 裁（「主会话裁」第 2 条，逐字）：「本机注解（`history-metadata.json`：星标 / 改名 / 隐藏）
//! 的**读写者**换成本机常驻后端 —— **文件留在原处、同一路径，不迁移、一条不丢**」。
//! C4c 的设计（`C4c.md §3.2`）：注解是 monitor 自有状态搬去后端自有状态（`readonly_guard` 第四层），不是用户文件；
//! `D1`：join 只有一个家 —— 历史清单并注解这件事从此住本机后端（`history_join.rs`）。
//!
//! # 文件在哪（同一路径是**构造**出来的，不是算法对齐出来的）
//!
//! monitor 起本机后端时用 [`ENV_PATH`] 把那份文件的绝对路径显式交过来（`local_backend_host::relay_host_envs`，
//! 值由 monitor 从前读写它的那一个函数算：`history·rs::metadata_path`）。同 RL1 交凭据路径那一条理由：
//! 由知道那份文件在哪的那一侧把路径说出来，`CCM_DATA_DIR` 隔离跑时跟着走。
//! **没交 ⇒ 注解不可用**：读回「不知道」（三个数不说成 0）、写拒 —— 不猜一个路径去写。
//!
//! # 形状（与 monitor 从前那份**同形同注解**，逐格照搬）
//!
//! `{"version": u32, "entries": {sid: {"starred", "customTitle", "hidden", "updatedAt", "lastAccount"}}}`；
//! 三个键认蛇形别名（`custom_title` / `updated_at` / `last_account`），缺格取缺省。
//!
//! # 写：**一条不丢**
//!
//! - 先按上面那份结构**严格**读一遍：读不懂（坏 JSON / 类型不对 / 同一格驼峰与蛇形都在）⇒ **拒写，原文件一个字节不动**。
//!   ⚠ monitor 从前那份写法是「读失败当空 ⇒ 只带这一条改动整份写回」—— 一份写坏的文件会让其余注解全部被覆盖掉。这里不照搬。
//! - 再在**原文 JSON** 上只改那一条：其余条目、条目里认不出的键、顶层认不出的键原样留着；被改那一条按驼峰规范写回
//!   （它身上的蛇形别名摘掉 —— 不摘的话两个名字同时在，下一次严格读就读不懂了）。
//! - 第四层写法：`O_EXCL` 建临时文件 → 写满 → `sync` → 原子挪过去；目录不在只建那一层；失败删自己的临时文件。
//!
//! # 买不到
//!
//! - 两个后端**进程**同时写（常驻那一个 ＋ 一次性 CLI `--history-annotate`）：进程内那把锁挡不住，后写的整份盖掉先写的那一次改动。
//! - Windows 上「原子挪过去」是 `std::fs::rename`（覆盖既有文件）；monitor 从前用 `ReplaceFileW`。没在真 Windows 上跑过。

use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// monitor 起本机后端时交那份文件绝对路径的环境变量名（monitor 那一侧 `local_backend_host::relay_host_envs` 同名，判据对拍）。
pub(crate) const ENV_PATH: &str = "CCM_HISTORY_METADATA";

/// 读一份注解文件的上限。超了 ⇒ 当读不懂：读回「不知道」、拒写（一个字节不动）。
/// 一条注解约 150 字节 ⇒ 64 MiB 装得下四十万条；真撞上说明那份文件不对劲，不拿截断的当完整的用。
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// 一条注解 —— 与 monitor 从前那份 `EntryMetadata` **同形同注解**（别名与缺省逐格照搬）。
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
    #[serde(default, rename = "lastAccount", alias = "last_account")]
    pub last_account: Option<String>,
}

/// 整份文件 —— 与 monitor 从前那份 `HistoryMetadata` 同形。
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

/// 读的结局。「没交路径」「文件不在」「读不懂」三件事分开说 —— 后两件对界面的意思完全不同：
/// 文件不在 = 还没有任何注解（真的是 0）；读不懂 = 不知道（不许说成 0）。
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded {
    /// 没交路径（不是本机常驻后端 / monitor 没交）—— 注解这一维不可用。
    NoPath,
    /// 读到了（文件不在 = 空表，同 monitor 从前那份）。
    Read(Table),
    /// 读不懂 / 读不动 —— 带一句为什么。
    Unreadable(String),
}

/// 那份文件的路径（环境变量给的；空串 / 相对路径都不认）。
pub fn path() -> Option<PathBuf> {
    path_from(std::env::var(ENV_PATH).ok().as_deref())
}

/// [`path`] 有逻辑的那一半（判据喂值，不去动进程级环境变量）。
pub fn path_from(raw: Option<&str>) -> Option<PathBuf> {
    let t = raw?.trim();
    let p = PathBuf::from(t);
    (!t.is_empty() && p.is_absolute()).then_some(p)
}

/// 读原文（带上限）。`Ok(None)` = 文件不在。
fn read_raw(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        _ => {}
    }
    crate::common::fs::read_regular_capped(path, MAX_BYTES)
        .map(Some)
        .map_err(|e| {
            copy_text(
                "beHistoryAnnotations.readRaw.unreadable",
                &[
                    ("path", &(path.display()).to_string()),
                    ("e", &e.to_string()),
                ],
            )
        })
}

/// 严格读（与 monitor 从前那份 `serde_json::from_str::<HistoryMetadata>` 同一个口径）。
fn parse(bytes: &[u8], path: &Path) -> Result<Doc, String> {
    serde_json::from_slice::<Doc>(bytes).map_err(|e| {
        copy_text(
            "beHistoryAnnotations.parse.unparsable",
            &[
                ("path", &(path.display()).to_string()),
                ("e", &e.to_string()),
            ],
        )
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
        Err(e) => Loaded::Unreadable(e),
    }
}

/// 读这台机器上那一份（生产入口）。
pub fn load() -> Loaded {
    match path() {
        Some(p) => load_at(&p),
        None => Loaded::NoPath,
    }
}

/// 读—改—写的锁：先建那一层目录，再拿它的**跨进程**锁（`platform/lock.rs`）。
/// 〔墓碑 —— 从前是一把进程内 `Mutex`：只挡同一进程，两个后端进程（常驻 ＋ 一次性 CLI）同时改注解，后写的整份盖掉先写的。〕
fn lock_for_write(path: &Path) -> Result<crate::platform::lock::DirLock, (&'static str, String)> {
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
        return Err((
            "io_failed",
            copy_text(
                "beHistoryAnnotations.writeAt.mkdirFailed",
                &[("dir", &(dir.display()).to_string()), ("e", &e.to_string())],
            ),
        ));
    }
    crate::platform::lock::hold(dir).map_err(|e| ("io_failed", e))
}

/// 一次改动（线上 `patch`）—— 语义逐格照搬 monitor 从前那份 `MetadataPatch` ＋ `update_history_metadata`：
/// 缺格 / `null` = 不改；标题 / 账号名给空白串 = 清空。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Patch {
    #[serde(default)]
    starred: Option<bool>,
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    custom_title: Option<Option<String>>,
    #[serde(default)]
    hidden: Option<bool>,
    #[serde(default, rename = "lastAccount", alias = "last_account")]
    last_account: Option<Option<String>>,
}

/// 被改那一条身上要摘掉的蛇形别名（写回时一律驼峰）。
const ALIASES: [&str; 3] = ["custom_title", "updated_at", "last_account"];

fn sid_arg(args: &Value) -> Result<&str, (&'static str, String)> {
    let sid = args.get("sid").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `sid` (a string)"),
    ))?;
    if sid.is_empty() || sid.len() > 256 || sid.chars().any(char::is_control) {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`sid` does not look like a session id: {sid:?}"
            )),
        ));
    }
    Ok(sid)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 读原文成「严格读过的结构 ＋ 原文 JSON」。文件不在 ⇒ monitor 从前那份缺省形状。
fn read_for_write(path: &Path) -> Result<(Doc, Value), (&'static str, String)> {
    match read_raw(path).map_err(|e| ("annotations_unreadable", e))? {
        None => Ok((Doc::default(), json!({ "version": 0, "entries": {} }))),
        Some(b) => {
            let doc = parse(&b, path).map_err(|e| ("annotations_unreadable", e))?;
            let raw: Value = serde_json::from_slice(&b)
                .map_err(|e| ("annotations_unreadable", format!("{e}")))?;
            if !raw.is_object() {
                return Err((
                    "annotations_unreadable",
                    copy_text(
                        "beHistoryAnnotations.readForWrite.notObject",
                        &[("path", &(path.display()).to_string())],
                    ),
                ));
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

/// **唯一的写者**：`O_EXCL` 新建临时文件 → 写满 → `sync` → 原子挪过去；失败删临时文件（目录由 [`lock_for_write`] 建）。
/// 序列化口径同 monitor 从前那份（`to_string_pretty`）。
fn write_at(path: &Path, raw: &Value) -> Result<(), String> {
    use std::io::Write as _;
    let dir = path.parent().ok_or_else(|| {
        copy_text(
            "beHistoryAnnotations.writeAt.noParent",
            &[("path", &(path.display()).to_string())],
        )
    })?;
    let body = serde_json::to_string_pretty(raw).map_err(|e| {
        crate::common::contract::malformed(&format!("serializing the annotations failed: {e}"))
    })?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "annotations".into());
    let tmp = dir.join(format!("{name}.{}.ccm-tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| {
                copy_text(
                    "beHistoryAnnotations.writeAt.tmpCreateFailed",
                    &[("tmp", &(tmp.display()).to_string()), ("e", &e.to_string())],
                )
            })?;
        f.write_all(body.as_bytes())
            .and_then(|()| f.sync_all())
            .map_err(|e| {
                copy_text(
                    "beHistoryAnnotations.writeAt.tmpWriteFailed",
                    &[("tmp", &(tmp.display()).to_string()), ("e", &e.to_string())],
                )
            })?;
        drop(f);
        std::fs::rename(&tmp, path).map_err(|e| {
            copy_text(
                "beHistoryAnnotations.writeAt.renameFailed",
                &[
                    ("tmp", &(tmp.display()).to_string()),
                    ("path", &(path.display()).to_string()),
                    ("e", &e.to_string()),
                ],
            )
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn need_path() -> Result<PathBuf, (&'static str, String)> {
    path().ok_or((
        "no_annotations",
        copy_text("beHistoryAnnotations.needPath.unknown", &[]),
    ))
}

/// `history-annotate {sid, patch}`：改一条，回改完的那一条（`{entry}`）。
pub fn answer_annotate(args: &Value) -> Result<Value, (&'static str, String)> {
    answer_annotate_at(&need_path()?, args, now_ms())
}

/// [`answer_annotate`] 的本体：路径与「现在」由调用方给（判据喂临时目录与固定时刻）。
pub fn answer_annotate_at(
    path: &Path,
    args: &Value,
    now: i64,
) -> Result<Value, (&'static str, String)> {
    let sid = sid_arg(args)?;
    let patch: Patch = match args.get("patch") {
        Some(p @ Value::Object(_)) => serde_json::from_value(p.clone()).map_err(|e| {
            (
                "bad_args",
                crate::common::contract::malformed(&format!("`patch` unreadable: {e}")),
            )
        })?,
        _ => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("missing `patch` (an object)"),
            ))
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
    if let Some(a) = patch.last_account {
        entry.last_account = a.filter(|s| !s.trim().is_empty());
    }
    entry.updated_at = now;
    put_entry(&mut raw, sid, Some(&entry));
    write_at(path, &raw).map_err(|e| ("io_failed", e))?;
    Ok(json!({ "entry": entry }))
}

/// `history-forget {sid}`：删那一条（删会话时连带）。不在 ⇒ 不写。回 `{removed}`。
pub fn answer_forget(args: &Value) -> Result<Value, (&'static str, String)> {
    answer_forget_at(&need_path()?, args)
}

/// [`answer_forget`] 的本体。
pub fn answer_forget_at(path: &Path, args: &Value) -> Result<Value, (&'static str, String)> {
    let sid = sid_arg(args)?;
    let _g = lock_for_write(path)?;
    let (doc, mut raw) = read_for_write(path)?;
    if !doc.entries.contains_key(sid) {
        return Ok(json!({ "removed": false }));
    }
    put_entry(&mut raw, sid, None);
    write_at(path, &raw).map_err(|e| ("io_failed", e))?;
    Ok(json!({ "removed": true }))
}

/// `history-last-accounts {}`：sid → 上次用哪个号起（只含真有的那几条）。读不懂 ⇒ 明拒（不说成「一条都没有」）。
pub fn last_accounts(_args: &Value) -> Result<Value, (&'static str, String)> {
    last_accounts_of(load())
}

/// [`last_accounts`] 的本体（读的结局由调用方给）。
pub fn last_accounts_of(loaded: Loaded) -> Result<Value, (&'static str, String)> {
    match loaded {
        Loaded::NoPath => Err((
            "no_annotations",
            copy_text("beHistoryAnnotations.lastAccountsOf.unknown", &[]),
        )),
        Loaded::Unreadable(why) => Err(("annotations_unreadable", why)),
        Loaded::Read(t) => {
            let m: BTreeMap<String, String> = t
                .into_iter()
                .filter_map(|(sid, e)| e.last_account.map(|a| (sid, a)))
                .collect();
            Ok(json!({ "accounts": m }))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/history/history_annotations_tests.rs"]
mod tests;
