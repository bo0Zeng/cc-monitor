//! 历史注解（星标 / 改名 / 隐藏）—— **跟着会话住在那台**：每台的常驻后端读写自己这份，只记这台上的会话。注解是后端自有状态（写经 `Own`），不是用户文件；
//! 每台出自己的清单时并上自己这份（`history_list.rs::with_own_annotations`），本机后端代问远端清单时照那台带来的并、不代写别台的。「上次用哪个号起的」不在这里（会话所在那台的 `launch-accounts.json`）。
//!
//! # 文件在哪
//!
//! 住这台的家：`<家>/history-metadata.json`（家 = `creds_core::store::monitor_data_dir`，默认 `~/.cc-monitor`，隔离跑时跟 `CCM_DATA_DIR` 走）。
//! 推不出家 ⇒ 注解不可用：读回「不知道」（三个数不说成 0）、写拒 —— 不猜一个路径去写。
//!
//! # 形状
//!
//! `{"version": 1, "entries": {sid: {"starred", "customTitle", "hide"?, "resumeWith"?, "updatedAt"}}}`；两个键认蛇形别名（`custom_title` / `updated_at`），缺格取缺省。
//! `hide` 是单个那一格（`on` 隐藏 · `off` 不隐藏 · 缺 ＝ 照规则，判定住 `control/session_hidden.rs`）。
//!
//! # 升级链（一节，下一个大版本删）
//!
//! 第 0 版（没有 `version` 或 `0`）条目里是 `hidden: bool`：`true` ⇒ `hide: "on"`，`false` ⇒ 丢掉（照规则）。
//! 读的一侧在内存里照它换一遍来用（[`upgraded`]）；写回只一次：常驻后端起来时（[`migrate_here`]），或在那之前先来的那一次写，
//! 都在那个目录的锁里换整份、原文留 `history-metadata.json.v1`。
//!
//! # 写：一条不丢
//!
//! - 先严格读一遍：读不懂（坏 JSON / 类型不对 / 同一格驼峰与蛇形都在）⇒ 拒写，原文件一个字节不动（「读失败当空、只带这一条写回」会覆盖掉其余注解）。
//! - 再在原文 JSON 上只改那一条：其余条目、条目里认不出的键、顶层认不出的键原样留着；被改那一条按驼峰规范写回
//!   （它身上的蛇形别名摘掉 —— 不摘的话两个名字同时在，下一次严格读就读不懂了）。
//! - 原子写（`Own::write`）：`O_EXCL` 建临时文件 → 写满 → `sync` → 原子挪过去；目录不在只建那一层；失败删自己的临时文件。读—改—写在那个目录的跨进程锁里。
//!
//! 买不到：Windows 上「原子挪过去」是覆盖既有文件的那一下改名（`Own::write`），没在真 Windows 上跑过。

use crate::platform::fs::{Own, OwnLock, OwnPath};
use crate::wire::typed::{codes, of_fail, Command};
pub use cmd_wire::listing::{Entry, ResumeWith, Table};
use copy_core::copy_text;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::common::said::IntoNote as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// 读一份注解文件的上限。超了 ⇒ 当读不懂：读回「不知道」、拒写（一个字节不动）。
/// 一条注解约 150 字节 ⇒ 64 MiB 装得下四十万条；真撞上说明那份文件不对劲，不拿截断的当完整的用。
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// 单个那一格（[`Entry::hide`]；线上形住 `cmd-wire`，隐藏的判定在控制面也读它）。
pub use cmd_wire::session::Hide;

/// 线上 `patch.hide`：`on` · `off` 写单个那一格，`rule` 摘掉它、回到规则（`null` / 缺 ＝ 不改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum HideAsk {
    On,
    Off,
    Rule,
}

/// 这一版的号（第 0 版的条目带 `hidden: bool`，见头注「升级链」）。
pub(crate) const VERSION: u32 = 1;

/// 原文留底的那一份：`<文件名>.v1`。
const OLD_SUFFIX: &str = ".v1";

/// **升级链那一节**：第 0 版的原文 JSON ⇒ 这一版（`hidden: true` ⇒ `hide: "on"`，`false` ⇒ 丢掉；别的键不动）。
/// 已经是这一版 / 不是对象 ⇒ 原样、`false`。版本号不是非负整数的那一份原样交回，由严格读拒。
pub(crate) fn upgraded(mut raw: Value) -> (Value, bool) {
    let old = match raw.get("version") {
        None => raw.is_object(),
        Some(v) => v.as_u64().is_some_and(|n| n < u64::from(VERSION)),
    };
    if !old {
        return (raw, false);
    }
    if let Some(entries) = raw.get_mut("entries").and_then(Value::as_object_mut) {
        for e in entries.values_mut() {
            if let Some(o) = e.as_object_mut() {
                if o.remove("hidden").and_then(|h| h.as_bool()) == Some(true) {
                    o.insert("hide".into(), json!("on"));
                }
            }
        }
    }
    raw["version"] = json!(VERSION);
    (raw, true)
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

/// 这条会话「下次恢复用」那一格（现读这台那一份；读不到 ⇒ 没记）。还算不算数由读的那一方照记录长度判（`session_open_face::kept_for_resume`）。
pub(crate) fn resume_with_of(sid: &str) -> Option<ResumeWith> {
    path().and_then(|p| resume_with_at(&p, sid))
}

/// [`resume_with_of`]，文件是参数。
pub(crate) fn resume_with_at(path: &Path, sid: &str) -> Option<ResumeWith> {
    match load_at(path) {
        Loaded::Read(t) => t.get(sid).and_then(|e| e.resume_with.clone()),
        _ => None,
    }
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

/// 严格读（第 0 版先在内存里升一遍，[`upgraded`]）。
fn parse(bytes: &[u8], path: &Path) -> Result<Doc, String> {
    let unparsable = |e: serde_json::Error| {
        crate::common::said::Said::with_raw(
            copy_text(
                "beHistoryAnnotations.parse.unparsable",
                &[("path", &(path.display()).to_string())],
            ),
            e,
        )
        .into_note()
    };
    let raw: Value = serde_json::from_slice(bytes).map_err(unparsable)?;
    serde_json::from_value::<Doc>(upgraded(raw).0).map_err(unparsable)
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

/// 读—改—写的锁：先建那一层目录（数据目录，默认就是 `~/.cc-monitor`，建的那一下只给本人），再拿它的跨进程锁。
fn lock_for_write(own: &Own, dir: &OwnPath) -> Result<OwnLock, crate::wire::spec::Fail> {
    let d = dir.as_path();
    if let Err(e) = own.ensure_dir(dir) {
        return Err(crate::wire::spec::Fail::from((
            "io_failed",
            crate::common::said::Said::with_raw(
                copy_text(
                    "beHistoryAnnotations.writeAt.mkdirFailed",
                    &[
                        ("dir", &(d.display()).to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ),
        )));
    }
    own.lock(dir).map_err(|e| {
        crate::wire::spec::Fail::from(("io_failed", crate::common::said::Said::from(e)))
    })
}

/// 一次改动（线上 `patch`）：缺格 / `null` = 不改；标题给空白串 = 清空。
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct Patch {
    /// 星标。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    starred: Option<bool>,
    /// 自定义标题：`null` = 不改；空白串 = 清空（也认蛇形 `custom_title`）。
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    #[cfg_attr(test, ts(type = "string | null", optional))]
    custom_title: Option<Option<String>>,
    /// 单个那一格：`on` · `off` · `rule`（摘掉、回到规则）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    hide: Option<HideAsk>,
}

/// 被改那一条身上要摘掉的蛇形别名（写回时一律驼峰）。
const ALIASES: [&str; 2] = ["custom_title", "updated_at"];

fn now_ms() -> i64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 读原文成「严格读过的结构 ＋ 原文 JSON（已升到这一版）＋ 是不是刚升的」。文件不在 ⇒ 缺省形状（这一版）。
fn read_for_write(path: &Path) -> Result<(Doc, Value, bool), crate::wire::spec::Fail> {
    match read_raw(path)
        .map_err(|e| crate::wire::spec::Fail::from(("annotations_unreadable", e)))?
    {
        None => Ok((
            Doc::default(),
            json!({ "version": VERSION, "entries": {} }),
            false,
        )),
        Some(b) => {
            let doc = parse(&b, path).map_err(|e| ("annotations_unreadable", e))?;
            let raw: Value = serde_json::from_slice(&b)
                .map_err(|e| ("annotations_unreadable", format!("{e}")))?;
            if !raw.is_object() {
                return Err(crate::wire::spec::Fail::from((
                    "annotations_unreadable",
                    copy_text(
                        "beHistoryAnnotations.readForWrite.notObject",
                        &[("path", &(path.display()).to_string())],
                    ),
                )));
            }
            let (raw, moved) = upgraded(raw);
            Ok((doc, raw, moved))
        }
    }
}

/// 刚升的那一份：原文先留底（`<文件名>.v1`，已有就不动），再由调用方整份写回。锁由调用方拿着。
fn keep_old(
    own: &Own,
    dir: &OwnPath,
    file: &OwnPath,
    held: &OwnLock,
) -> Result<(), crate::common::said::Said> {
    let name = file
        .as_path()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let old = dir.join(&format!("{name}{OLD_SUFFIX}"))?;
    if crate::platform::fs::Ro::symlink_metadata(old.as_path()).is_ok() {
        return Ok(());
    }
    let bytes = read_raw(file.as_path())?.unwrap_or_default();
    own.write(&old, held, &bytes)
}

/// 常驻后端起来时调一次（升级链那一节的写回）：第 0 版 ⇒ 原文留 `.v1`、整份换成这一版。没有文件 / 已是这一版 / 读不懂 ⇒ 不写；写不成只出声。
pub fn migrate_here(own: &Own) {
    let run = || -> Result<bool, crate::wire::spec::Fail> {
        let (dir, file) = need_path(own)?;
        if crate::platform::fs::Ro::symlink_metadata(file.as_path()).is_err() {
            return Ok(false);
        }
        let held = lock_for_write(own, &dir)?;
        let (_, raw, moved) = read_for_write(file.as_path())?;
        if moved {
            keep_old(own, &dir, &file, &held)
                .and_then(|()| write_at(own, &file, &held, &raw))
                .map_err(|e| crate::wire::spec::Fail::from(("io_failed", e)))?;
        }
        Ok(moved)
    };
    match run() {
        Ok(true) => tracing::info!(
            "[annotations] {} 升到第 {VERSION} 版（原文留 {OLD_SUFFIX}）",
            creds_core::store::HISTORY_METADATA_FILE
        ),
        Ok(false) => {}
        Err(f) => tracing::warn!("[annotations] 升级写回不成：{} {}", f.code, f.message),
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
            // 可缺的那几格摘掉时序列化里没有它 ⇒ 原文里那一格也摘。
            obj.remove("hide");
            obj.remove("resumeWith");
            if let Value::Object(norm) = serde_json::to_value(e).expect("Entry 总能序列化") {
                for (k, v) in norm {
                    obj.insert(k, v);
                }
            }
            entries.insert(sid.to_string(), Value::Object(obj));
        }
    }
}

/// 唯一的写者（经 `Own` 原子写；目录由 [`lock_for_write`] 建）。序列化用 `to_string_pretty`。
fn write_at(
    own: &Own,
    path: &OwnPath,
    held: &OwnLock,
    raw: &Value,
) -> Result<(), crate::common::said::Said> {
    let body = serde_json::to_string_pretty(raw).map_err(|e| {
        // 契约错那一句后面跟着的诊断拆进原话（同失败信封那一处，`contract::parts`）：句子里不带它。
        crate::common::said::Said::from(crate::common::contract::parts(
            &crate::common::contract::malformed(&format!(
                "serializing the annotations failed: {e}"
            )),
            None,
        ))
    })?;
    own.write(path, held, body.as_bytes())
}

/// 那份文件所在的目录（数据目录）与它本身。推不出家 ⇒ `no_annotations`。
fn need_path(own: &Own) -> Result<(OwnPath, OwnPath), crate::wire::spec::Fail> {
    let dir = own.root().ok_or_else(|| {
        crate::wire::spec::Fail::from((
            "no_annotations",
            copy_text(
                "beHistoryAnnotations.needPath.unknown",
                &[("env", creds_core::store::DATA_DIR_ENV)],
            ),
        ))
    })?;
    let file = dir
        .join(creds_core::store::HISTORY_METADATA_FILE)
        .map_err(|e| crate::wire::spec::Fail::from(("io_failed", e)))?;
    Ok((dir, file))
}

// ───────────────────────── 帧命令（typed，`wire/typed.rs`）─────────────────────────

codes! {
    /// `history-annotate` · `history-forget` 的码。
    pub(crate) enum AnnotationErr {
        /// 这份文件此刻读不懂（坏 JSON · 类型不对 · 同一格驼峰与蛇形都在）⇒ 拒写，原文件一个字节不动。
        Unreadable = "annotations_unreadable",
        /// 写不下去（建目录 · 拿锁 · 原子写）。
        IoFailed = "io_failed",
        /// 推不出家 ⇒ 不知道往哪写。
        NoAnnotations = "no_annotations",
    }
}

/// 下层那一份 [`Fail`](crate::wire::spec::Fail) ⇒ 这两条的码（不在表里 ⇒ 落 `io_failed`）。
fn err(f: crate::wire::spec::Fail) -> AnnotationErr {
    of_fail(f, AnnotationErr::IoFailed)
}

/// 会话 id：非空、至多 256 字节、不含控制字符；不像 ⇒ `bad_args`（形状对、值不对，由处理器回）。
fn checked_sid(sid: &str) -> Result<&str, AnnotationErr> {
    if sid.is_empty() || sid.len() > 256 || sid.chars().any(char::is_control) {
        return Err(AnnotationErr::BadArgs(crate::common::contract::malformed(
            &format!("`sid` does not look like a session id: {sid:?}"),
        )));
    }
    Ok(sid)
}

/// `history-annotate` 的入参：改哪一条 ＋ 改哪几格。
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct AnnotateReq {
    /// 会话 id。
    pub(crate) sid: String,
    /// 要改的那几格。
    pub(crate) patch: Patch,
}

/// `history-forget` 的入参。
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ForgetReq {
    /// 会话 id。
    pub(crate) sid: String,
}

/// `history-annotate`：改一条历史注解（星标 / 改名 / 隐藏），回改完的那一条。
pub(crate) struct HistoryAnnotateCmd;

impl Command for HistoryAnnotateCmd {
    const NAME: &'static str = "history-annotate";
    type Req = AnnotateReq;
    type Reply = Annotated;
    type Err = AnnotationErr;
}

impl crate::wire::typed::Handle for HistoryAnnotateCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: AnnotateReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Annotated, AnnotationErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            annotate_at(own, req, now_ms())
        }
    }
}

/// `history-forget`：删一条历史注解（删会话时连带）。不在 ⇒ 不写。
pub(crate) struct HistoryForgetCmd;

impl Command for HistoryForgetCmd {
    const NAME: &'static str = "history-forget";
    type Req = ForgetReq;
    type Reply = Forgotten;
    type Err = AnnotationErr;
}

impl crate::wire::typed::Handle for HistoryForgetCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: ForgetReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Forgotten, AnnotationErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            forget_at(own, req)
        }
    }
}

/// [`HistoryAnnotateCmd`] 的本体：「现在」由调用方给（判据喂临时目录的写句柄与固定时刻）。
pub(crate) fn annotate_at(
    own: &Own,
    req: AnnotateReq,
    now: i64,
) -> Result<Annotated, AnnotationErr> {
    let sid = checked_sid(&req.sid)?;
    let patch = req.patch;
    let (dir, file) = need_path(own).map_err(err)?;
    let held = lock_for_write(own, &dir).map_err(err)?;
    let (doc, mut raw, moved) = read_for_write(file.as_path()).map_err(err)?;
    if moved {
        keep_old(own, &dir, &file, &held)
            .map_err(|e| err(crate::wire::spec::Fail::from(("io_failed", e))))?;
    }
    let mut entry = doc.entries.get(sid).cloned().unwrap_or_default();
    let prev = entry.hide;
    if let Some(s) = patch.starred {
        entry.starred = s;
    }
    if let Some(t) = patch.custom_title {
        entry.custom_title = t.filter(|s| !s.trim().is_empty());
    }
    match patch.hide {
        None => {}
        Some(HideAsk::On) => entry.hide = Some(Hide::On),
        Some(HideAsk::Off) => entry.hide = Some(Hide::Off),
        Some(HideAsk::Rule) => entry.hide = None,
    }
    entry.updated_at = now;
    put_entry(&mut raw, sid, Some(&entry));
    write_at(own, &file, &held, &raw)
        .map_err(|e| err(crate::wire::spec::Fail::from(("io_failed", e))))?;
    Ok(Annotated { entry, prev })
}

/// 「下次恢复用」那一格的唯一写者（`session-model` · `session-permission` 在终端没了时调）：同一个 `end` 上先后记的并成一份，
/// `end` 变了 ⇒ 整份换掉。别的格不动。
pub(crate) fn keep_resume_at(
    own: &Own,
    sid: &str,
    pick: ResumeWith,
    now: i64,
) -> Result<(), crate::wire::spec::Fail> {
    use crate::wire::typed::Codes as _;
    let sid = checked_sid(sid).map_err(|e| e.into_fail())?;
    let (dir, file) = need_path(own)?;
    let held = lock_for_write(own, &dir)?;
    let (doc, mut raw, moved) = read_for_write(file.as_path())?;
    let io = |e| crate::wire::spec::Fail::from(("io_failed", e));
    if moved {
        keep_old(own, &dir, &file, &held).map_err(io)?;
    }
    let mut entry = doc.entries.get(sid).cloned().unwrap_or_default();
    entry.resume_with = Some(match entry.resume_with.take() {
        Some(old) if old.end == pick.end => ResumeWith {
            model: pick.model.or(old.model),
            permission_mode: pick.permission_mode.or(old.permission_mode),
            end: pick.end,
        },
        _ => pick,
    });
    entry.updated_at = now;
    put_entry(&mut raw, sid, Some(&entry));
    write_at(own, &file, &held, &raw).map_err(io)
}

/// `history-annotate` 的回包：改完的那一条 ＋ 改之前单个隐藏那一格。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Annotated {
    pub(crate) entry: Entry,
    /// 改之前单个隐藏那一格（`on` · `off` · 没有 ⇒ `null`）：撤销「隐藏」照它交回去（`null` ⇒ `rule`），不把照规则显示的钉成单独显示。
    pub(crate) prev: Option<Hide>,
}

/// `history-forget` 的回包。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Forgotten {
    /// 真删了一条没有（`false` = 本来就没有这一条，文件没动）。
    pub(crate) removed: bool,
}

/// [`HistoryForgetCmd`] 的本体。
pub(crate) fn forget_at(own: &Own, req: ForgetReq) -> Result<Forgotten, AnnotationErr> {
    let sid = checked_sid(&req.sid)?;
    let (dir, file) = need_path(own).map_err(err)?;
    let held = lock_for_write(own, &dir).map_err(err)?;
    let (doc, mut raw, moved) = read_for_write(file.as_path()).map_err(err)?;
    if !doc.entries.contains_key(sid) {
        return Ok(Forgotten { removed: false });
    }
    if moved {
        keep_old(own, &dir, &file, &held)
            .map_err(|e| err(crate::wire::spec::Fail::from(("io_failed", e))))?;
    }
    put_entry(&mut raw, sid, None);
    write_at(own, &file, &held, &raw)
        .map_err(|e| err(crate::wire::spec::Fail::from(("io_failed", e))))?;
    Ok(Forgotten { removed: true })
}

#[cfg(test)]
#[path = "../../../tests/backend/history/history_annotations_tests.rs"]
mod tests;
