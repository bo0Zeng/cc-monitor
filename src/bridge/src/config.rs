//! 用户配置 R/W —— `~/.claude/work/config.json`。
//!
//! Rust 端不解释配置内容（schema 在前端定义，顶层键的主人登记在 `src/config.ts::CONFIG_KEY_OWNERS`），
//! 只负责读、**按键补丁**写 ＋ 文件缺失时给出最小骨架。
//!
//! 配置文件位置走 `paths::resolve_config_path` —— monitor 自己的设置永远在
//! 默认 `~/.claude/work/` 下，不跟随 `claudeDir` 字段变化。
//!
//! 🔴 〔CFG1 · 4D〕**写 `config.json` 只有一个口：[`patch_config_at`]。** 整份替换那一形（旧 `save_config`）删了。 〔散文墓碑〕
//!
//! 从前每个前端模块「读整份 → 改自己的键 → 整份写回」，主窗（tab 栏的分组 / 固定 / 顺序）与设置窗
//! （行为 / 主题 / 快捷键 / 账号 / 远端）是同一进程里的两个 webview，两次读-改-写一交错，
//! 后写的整份就把先写的键盖掉（E §E1：拖放同拍发分组 ＋ 顺序，分组那次没落盘）。
//! 守的要求：`设计/30 §4`「各自只写自己那个键」· `设计/70 §6.3` 红线 ④「读不懂的 `config.json` 不写」。
//!
//! 现在前端只交「改哪几条路径」（[`ConfigEdit`]），这里在**一把进程级锁里现读盘、逐条应用、原子替换** ——
//! 主窗 / 设置窗 / `logging.rs` 的诊断写口都走这一个函数，谁写的键谁的值留在盘上。
//! 〔HX2 · 4D〕两个 monitor 进程同写（Linux / macOS 今天没有单实例）也串起来了：进程内那把锁里面、现读之前，
//! 再拿 `config.json` 所在目录的**跨进程**锁（`platform_fs::hold_dir_lock`：unix 对目录 `flock` · Windows 命名互斥量，与后端第四层同一种锁）。
//! 〔墓碑 —— CFG1 那一版这里写「射程：锁是进程内的。两个 monitor 进程同写仍在锁外；丢更新的窗口缩到读与 rename 之间」。〕

use crate::copy_table::copy_text;
use crate::paths;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[tauri::command]
pub fn load_config() -> Result<Value, String> {
    let path = paths::resolve_config_path().ok_or_else(|| "no home dir".to_string())?;
    if !path.exists() {
        return Ok(default_config());
    }
    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// 一条配置补丁。`path[0]` 是顶层键，其后是逐层子键。
///
/// - `set`：沿路径下行，中间段缺失或不是对象 ⇒ 换成 `{}`，末段整值替换（值里的 `null` 原样存 ——
///   快捷键的「解绑」就是 `null`）。
/// - `remove`：沿路径找，找不到 ⇒ 什么都不做。
///
/// 为什么不是 JSON Merge Patch（RFC 7396）：那个对子对象递归合并、`null` 表示删除；而 `keybindings` /
/// `theme` / `remote` 要的是**整键替换**，`keybindings` 的值里还有合法的 `null`。路径形状让写者明说替换到哪一层。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum ConfigEdit {
    Set {
        path: Vec<String>,
        #[cfg_attr(test, ts(type = "unknown"))]
        value: Value,
    },
    Remove {
        path: Vec<String>,
    },
    /// 〔FIX · `设计/99 §2 ㊶` 第一问〕**按键认数组元素**：`path` 指到一个数组，在里面认出满足 `where` 每一条的**恰好一个**对象，
    /// 只改它的 `field` 那一格（锁内现读现判 ⇒ 别的元素、别的格与并发写者的改动都不会被一份锁外算好的整列盖掉）。
    /// `ifEmpty` = CAS：那一格已有值（非空串 / 非 null）就不动。认不出 / 认出多个 ⇒ 不动（结局见 [`Applied`]）。
    #[serde(rename = "setin", rename_all = "camelCase")]
    SetIn {
        path: Vec<String>,
        r#where: Vec<ElemKey>,
        field: String,
        #[cfg_attr(test, ts(type = "unknown"))]
        value: Value,
        #[serde(default)]
        if_empty: bool,
    },
    /// 〔FIX2 续 · `99 §2 ㊶`〕**按键插一个元素**（设置页新增一台机器）：`path` 指到的数组里已有满足 `where` 的元素 ⇒ 不动、整批拒
    /// （[`ConfigWriteError::ElementExists`]）；否则追加到末尾。路上缺的段与数组本身照 `set` 的口径补出来（第一台机器）。
    #[serde(rename = "insertin", rename_all = "camelCase")]
    InsertIn {
        path: Vec<String>,
        r#where: Vec<ElemKey>,
        #[cfg_attr(test, ts(type = "unknown"))]
        value: Value,
    },
    /// 〔FIX2 续 · `99 §2 ㊶`〕**按键删一个元素**（设置页删一台机器）：认出恰好一个才删；认不出 / 认出多个 ⇒ 整批拒（同 `setin`）。
    #[serde(rename = "removein", rename_all = "camelCase")]
    RemoveIn {
        path: Vec<String>,
        r#where: Vec<ElemKey>,
    },
}

/// [`ConfigEdit::SetIn`] 认元素的一条：元素里 `fields` 按序取**第一个非空字符串**，它 == `equals`
/// （`remote.hosts` 的 origin 就是 `["label", "host"]`：`label` 非空取它、否则 `host`）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct ElemKey {
    pub fields: Vec<String>,
    pub equals: String,
}

/// 一条补丁落下去的结局（与交进来的补丁逐条对应）。`Set` / `Remove` 恒 `Done`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Applied {
    Done,
    /// `SetIn`：一个都认不出（路径不在 / 不是数组 / 没有满足的元素）⇒ 整批拒（[`ConfigWriteError::NoSuchElement`]）。
    NoMatch,
    /// `SetIn`：认出不止一个 ⇒ 不猜是哪个（同上，整批拒）。
    Ambiguous,
    /// `SetIn` 带 `ifEmpty`：那一格已有值。
    Kept,
    /// `InsertIn`：已有满足 `where` 的元素 ⇒ 整批拒（[`ConfigWriteError::ElementExists`]）。
    Exists,
}

impl ConfigEdit {
    fn path(&self) -> &[String] {
        match self {
            ConfigEdit::Set { path, .. }
            | ConfigEdit::Remove { path }
            | ConfigEdit::SetIn { path, .. }
            | ConfigEdit::InsertIn { path, .. }
            | ConfigEdit::RemoveIn { path, .. } => path,
        }
    }

    /// 按键认元素的那几种的 `where`（`None` = 不认元素）。空 `where` 会认中每个对象 ⇒ 在 ① 就拒。
    fn elem_where(&self) -> Option<&[ElemKey]> {
        match self {
            ConfigEdit::SetIn { r#where, .. }
            | ConfigEdit::InsertIn { r#where, .. }
            | ConfigEdit::RemoveIn { r#where, .. } => Some(r#where),
            ConfigEdit::Set { .. } | ConfigEdit::Remove { .. } => None,
        }
    }
}

/// [`patch_config_at`] 失败的三种。分开是为了让调用方各说各的话（`logging.rs` 那句「诊断设置没有存」原样保留）。
#[derive(Debug)]
pub(crate) enum ConfigWriteError {
    /// 盘上那份读不懂 / 根不是对象 ⇒ **一个字节没写**（`设计/70 §6.3` 红线 ④）。
    Unreadable {
        path: PathBuf,
        detail: String,
    },
    /// 补丁本身不成形（空路径 —— 那就是「整份替换」换了个名字）⇒ 整批拒，盘上一个字节不动。
    BadEdit(String),
    /// 〔FIX · `99 §2 ㊶`〕`setin` 认不出那一个元素（`ambiguous` = 认出了不止一个）⇒ **整批拒**，盘上一个字节不动
    /// （设置页按格改一台，而那台在盘上已被改名 / 删掉：不许一半落盘、一半静默丢掉）。
    NoSuchElement {
        ambiguous: bool,
    },
    /// 〔FIX2 续〕`insertin` 要插的那个键盘上已经有了 ⇒ **整批拒**，盘上一个字节不动（不静默变成改那一台）。
    ElementExists,
    Io(String),
}

impl std::fmt::Display for ConfigWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigWriteError::Unreadable { path, detail } => f.write_str(&copy_text(
                "rsConfig.write.unreadable",
                &[("path", &path.display().to_string()), ("e", detail)],
            )),
            ConfigWriteError::NoSuchElement { .. } => {
                f.write_str(&copy_text("rsConfig.write.elementGone", &[]))
            }
            ConfigWriteError::ElementExists => {
                f.write_str(&copy_text("rsConfig.write.elementExists", &[]))
            }
            ConfigWriteError::BadEdit(m) | ConfigWriteError::Io(m) => f.write_str(m),
        }
    }
}

/// 进程里所有写 `config.json` 的人共用这一把锁（主窗 / 设置窗的 IPC、`logging.rs` 的诊断写口）。
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 前端写配置的唯一口：交一串 [`ConfigEdit`]，这里合并进盘上那份。
#[tauri::command]
pub fn patch_config(edits: Vec<ConfigEdit>) -> Result<(), String> {
    let path = paths::resolve_config_path().ok_or_else(|| "no home dir".to_string())?;
    patch_config_at(&path, &edits)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// 🔴 **`config.json` 唯一的写函数。** 锁内现读 → 逐条应用 → 带 pid 的临时件 → 原子替换。回逐条结局；一条都没改动 ⇒ 不写。
pub(crate) fn patch_config_at(
    path: &Path,
    edits: &[ConfigEdit],
) -> Result<Vec<Applied>, ConfigWriteError> {
    // ① 先验全部：一条不成形 ⇒ 整批拒，连锁都不拿。
    if let Some(bad) = edits.iter().find(|e| e.path().is_empty()) {
        return Err(ConfigWriteError::BadEdit(format!(
            "config patch refused: empty path ({bad:?})"
        )));
    }
    if let Some(bad) = edits
        .iter()
        .find(|e| e.elem_where().is_some_and(<[ElemKey]>::is_empty))
    {
        return Err(ConfigWriteError::BadEdit(format!(
            "config patch refused: empty `where` ({bad:?})"
        )));
    }
    if edits.is_empty() {
        return Ok(Vec::new());
    }
    // ② 串行化。锁中毒（别的写者 panic 了）不妨碍这一次：锁只护「读-改-写」这一段，没有要恢复的内存状态。
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // ②b 〔HX2 · 4D〕跨进程：两个 monitor 进程同写也串起来 —— 先保证目录在（锁的就是它），再拿它的锁，锁住之后才现读。
    let dir = path
        .parent()
        .ok_or_else(|| ConfigWriteError::Io(format!("no parent dir for {}", path.display())))?;
    std::fs::create_dir_all(dir)
        .map_err(|e| ConfigWriteError::Io(format!("mkdir {}: {e}", dir.display())))?;
    let _cross = crate::platform_fs::hold_dir_lock(dir).map_err(ConfigWriteError::Io)?;

    // ③ 锁内现读。不存在 ⇒ 空对象；读不懂 / 根不是对象 ⇒ 不写。
    let mut root: Map<String, Value> = if path.exists() {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| ConfigWriteError::Io(format!("read {}: {e}", path.display())))?;
        match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(m)) => m,
            Ok(_) => {
                return Err(ConfigWriteError::Unreadable {
                    path: path.to_path_buf(),
                    detail: copy_text("rsConfig.write.notAnObject", &[]),
                })
            }
            Err(e) => {
                return Err(ConfigWriteError::Unreadable {
                    path: path.to_path_buf(),
                    detail: e.to_string(),
                })
            }
        }
    } else {
        // 目录已在 ②b 建过（拿锁之前）。
        Map::new()
    };

    // ④ 逐条应用。
    let applied: Vec<Applied> = edits.iter().map(|e| apply_edit(&mut root, e)).collect();
    if let Some(miss) = applied
        .iter()
        .find(|a| matches!(a, Applied::NoMatch | Applied::Ambiguous))
    {
        return Err(ConfigWriteError::NoSuchElement {
            ambiguous: *miss == Applied::Ambiguous,
        });
    }
    if applied.contains(&Applied::Exists) {
        return Err(ConfigWriteError::ElementExists);
    }
    if !applied.contains(&Applied::Done) {
        return Ok(applied);
    }

    // ⑤ 临时件带 pid：两个 monitor 进程不互删对方的临时件。
    let pretty = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| ConfigWriteError::Io(e.to_string()))?;
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, pretty)
        .map_err(|e| ConfigWriteError::Io(format!("write {}: {e}", tmp.display())))?;
    atomic_replace(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ConfigWriteError::Io(format!("replace → {}: {e}", path.display()))
    })?;
    Ok(applied)
}

/// 元素里 `fields` 按序第一个非空字符串（[`ElemKey`]）。
fn elem_key<'a>(elem: &'a Map<String, Value>, fields: &[String]) -> Option<&'a str> {
    fields.iter().find_map(|f| {
        elem.get(f)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    })
}

/// `where` 每一条都对得上。
fn elem_matches(o: &Map<String, Value>, r#where: &[ElemKey]) -> bool {
    r#where
        .iter()
        .all(|k| elem_key(o, &k.fields) == Some(k.equals.as_str()))
}

fn apply_edit(root: &mut Map<String, Value>, edit: &ConfigEdit) -> Applied {
    let (last, parents) = edit.path().split_last().expect("空路径在 ① 已拒");
    let mut cur = root;
    match edit {
        ConfigEdit::InsertIn { r#where, value, .. } => {
            for seg in parents {
                let slot = cur.entry(seg.clone()).or_insert(Value::Null);
                if !slot.is_object() {
                    *slot = Value::Object(Map::new());
                }
                cur = slot.as_object_mut().expect("上一行保证是对象");
            }
            let slot = cur.entry(last.clone()).or_insert(Value::Array(Vec::new()));
            let Some(arr) = slot.as_array_mut() else {
                return Applied::NoMatch;
            };
            if arr
                .iter()
                .filter_map(Value::as_object)
                .any(|o| elem_matches(o, r#where))
            {
                return Applied::Exists;
            }
            arr.push(value.clone());
            Applied::Done
        }
        ConfigEdit::RemoveIn { r#where, .. } => {
            for seg in parents {
                match cur.get_mut(seg).and_then(Value::as_object_mut) {
                    Some(next) => cur = next,
                    None => return Applied::NoMatch,
                }
            }
            let Some(arr) = cur.get_mut(last).and_then(Value::as_array_mut) else {
                return Applied::NoMatch;
            };
            let hits: Vec<usize> = arr
                .iter()
                .enumerate()
                .filter(|(_, v)| v.as_object().is_some_and(|o| elem_matches(o, r#where)))
                .map(|(i, _)| i)
                .collect();
            match hits.as_slice() {
                [i] => {
                    arr.remove(*i);
                    Applied::Done
                }
                [] => Applied::NoMatch,
                _ => Applied::Ambiguous,
            }
        }
        ConfigEdit::SetIn {
            r#where,
            field,
            value,
            if_empty,
            ..
        } => {
            for seg in parents {
                match cur.get_mut(seg).and_then(Value::as_object_mut) {
                    Some(next) => cur = next,
                    None => return Applied::NoMatch,
                }
            }
            let Some(arr) = cur.get_mut(last).and_then(Value::as_array_mut) else {
                return Applied::NoMatch;
            };
            let mut hits = arr
                .iter_mut()
                .filter_map(Value::as_object_mut)
                .filter(|o| elem_matches(o, r#where));
            let elem = match (hits.next(), hits.next()) {
                (Some(e), None) => e,
                (None, _) => return Applied::NoMatch,
                (Some(_), Some(_)) => return Applied::Ambiguous,
            };
            let taken = elem
                .get(field)
                .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| !s.trim().is_empty()));
            if *if_empty && taken {
                return Applied::Kept;
            }
            elem.insert(field.clone(), value.clone());
            Applied::Done
        }
        ConfigEdit::Set { value, .. } => {
            for seg in parents {
                let slot = cur.entry(seg.clone()).or_insert(Value::Null);
                if !slot.is_object() {
                    *slot = Value::Object(Map::new());
                }
                cur = slot.as_object_mut().expect("上一行保证是对象");
            }
            cur.insert(last.clone(), value.clone());
            Applied::Done
        }
        ConfigEdit::Remove { .. } => {
            for seg in parents {
                // 路上就没有（或不是对象）⇒ 要删的东西本来就不在；**不**顺手建出空段。
                match cur.get_mut(seg).and_then(Value::as_object_mut) {
                    Some(next) => cur = next,
                    None => return Applied::Done,
                }
            }
            cur.remove(last);
            Applied::Done
        }
    }
}

/// 把 src 原子替换到 dst。
///
/// ⚠ 〔`K-H2a` 08-27〕**从私有改成 `pub(crate)`，理由不是「顺手」**：
/// 〔GP1 · 第四波〕那个调用方（`creds_store::write_key`〔散文墓碑〕）随本机凭据文件的写者换成本机常驻后端一起删了；
/// 下面是它当年的理由，留作来历：它要一次原子替换，而它**不许自己写一个 `fs::rename`** ——
/// `atomic_replace_registry` 按「`rename` / `MoveFileExW` 的**出现次数**」逐文件登记，
/// 那张表不在 `K-H2a` 的写区。复用这一份 ⇒ 新文件里那两个字面量出现 **0** 次，
/// 既不动那张表，也不给它挖洞。
/// 选它（`MoveFileExW` 那套语义）而不是 `ReplaceFileW` 是**有理由的**：
/// `INVARIANTS §4` 那条 ACL 保留只限定在**用户的**文件，而凭据文件与 `config.json` 同类
/// ——**都是 monitor 自己的文件**（登记表里 `config.rs` 那两行逐字这么写的）。
/// std::fs::rename 在 Windows 上目标文件已存在时会失败（不像 POSIX 原子覆盖），
/// 所以这里走 MoveFileExW(MOVEFILE_REPLACE_EXISTING)；非 Windows 走 std::fs::rename。
#[cfg(windows)]
pub(crate) fn atomic_replace(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING};

    let to_wide = |p: &std::path::Path| -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let src_w = to_wide(src);
    let dst_w = to_wide(dst);
    unsafe {
        MoveFileExW(
            PCWSTR(src_w.as_ptr()),
            PCWSTR(dst_w.as_ptr()),
            MOVEFILE_REPLACE_EXISTING,
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.message().to_string()))
    }
}

#[cfg(not(windows))]
pub(crate) fn atomic_replace(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}

/// 文件缺失时返回的最小骨架。仅做占位，前端读到没有 `theme` 字段会用 :root 默认值。
fn default_config() -> Value {
    serde_json::json!({})
}

#[cfg(test)]
#[path = "../../../tests/bridge/config_tests.rs"]
mod tests;
