//! 用户配置 R/W —— `~/.cc-monitor/config.json`。
//!
//! Rust 端不解释配置内容（schema 在前端定义，顶层键的主人登记在 `src/frontend/ui/config.ts::CONFIG_KEY_OWNERS`），
//! 只负责读、**按键补丁**写 ＋ 文件缺失时给出最小骨架。
//!
//! 配置文件位置走 [`resolve_config_path`] —— monitor 自己的设置永远在
//! 默认 `~/.cc-monitor/` 下（一台机器一个家，与后端同一个），不跟随 `claudeDir` 字段变化。
//!
//! 🔴 **写 `config.json` 只有一个口：[`patch_config_at`]。** 整份替换那一形（旧 `save_config`）删了。 〔散文墓碑〕
//!
//! 从前每个前端模块「读整份 → 改自己的键 → 整份写回」，主窗（tab 栏的分组 / 固定 / 顺序）与设置窗
//! （行为 / 主题 / 快捷键 / 账号 / 远端）是同一进程里的两个 webview，两次读-改-写一交错，
//! 后写的整份就把先写的键盖掉（E §E1：拖放同拍发分组 ＋ 顺序，分组那次没落盘）。
//! 守的要求：「各自只写自己那个键」· 红线 ④「读不懂的 `config.json` 不写」。
//!
//! 现在前端只交「改哪几条路径」（[`ConfigEdit`]），这里在**一把进程级锁里现读盘、逐条应用、原子替换** ——
//! 主窗 / 设置窗 / `logging.rs` 的诊断写口都走这一个函数，谁写的键谁的值留在盘上。
//! 两个 monitor 进程同写（Linux / macOS 今天没有单实例）也串起来了：进程内那把锁里面、现读之前，
//! 再拿 `config.json` 所在目录的**跨进程**锁（`platform::fs::hold_dir_lock`：unix 对目录 `flock` · Windows 命名互斥量，与后端第四层同一种锁）。
//! 〔墓碑 —— CFG1 那一版这里写「射程：锁是进程内的。两个 monitor 进程同写仍在锁外；丢更新的窗口缩到读与 rename 之间」。〕
//!
//! # 〔「`config` ＋ `paths` → 一处」〕数据目录与配置文件路径（原 `paths.rs`，整份并进本文件、只挪住址）
//!
//! monitor 自己配置文件的路径解析 ＋ 设置里那条「Claude 目录」覆盖的原值。
//!
//! ### Claude 目录不在这里解析
//!
//! 「这台的 Claude 数据目录在哪」只由那台后端解析（`agents/claudecode/paths.rs::resolve_home`）。monitor 只读出用户在
//! 设置里填的覆盖（config.json 的 `claudeDir`，[`claude_dir_override`]），起本机后端时**显式**交过去
//! （`local_backend_host::backend_env`）；没填 ⇒ 不交，后端照它自己那条规矩解析。
//!
//! monitor 自己的 config.json 始终保存在**默认位置** `~/.cc-monitor/config.json`，**不**跟随 claudeDir 变化：
//! 用户切换 Claude 数据目录后，monitor 的 theme/字体设置不会丢。
//!
//! ### 🔴 那句「永远在默认位置」有一个出口
//!
//! `CCM_DATA_DIR`（[`DATA_DIR_ENV`]）**只为「把这个进程整体挪到别处跑」而存在** ——
//! 跑自动化测试、跑一次性复算。它**不是**给用户搬家用的设置面
//! （用户那一侧的「数据位置」是只读展示）。
//! 给了但不是绝对路径 ⇒ 回 `None`，**不退回用户真 profile**，
//! 逐条理由住 [`resolve_monitor_data_dir`]。

use crate::copy_table::copy_text;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[tauri::command]
pub fn load_config() -> Result<Value, String> {
    let path = resolve_config_path().ok_or_else(|| "no home dir".to_string())?;
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
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
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
    /// 〔第一问〕**按键认数组元素**：`path` 指到一个数组，在里面认出满足 `where` 每一条的**恰好一个**对象，
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
    /// **按键插一个元素**（设置页新增一台机器）：`path` 指到的数组里已有满足 `where` 的元素 ⇒ 不动、整批拒
    /// （[`ConfigWriteError::ElementExists`]）；否则追加到末尾。路上缺的段与数组本身照 `set` 的口径补出来（第一台机器）。
    #[serde(rename = "insertin", rename_all = "camelCase")]
    InsertIn {
        path: Vec<String>,
        r#where: Vec<ElemKey>,
        #[cfg_attr(test, ts(type = "unknown"))]
        value: Value,
    },
    /// **按键删一个元素**（设置页删一台机器）：认出恰好一个才删；认不出 / 认出多个 ⇒ 整批拒（同 `setin`）。
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
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
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
    /// 盘上那份读不懂 / 根不是对象 ⇒ **一个字节没写**（红线 ④）。
    Unreadable {
        path: PathBuf,
        detail: String,
    },
    /// 补丁本身不成形（空路径 —— 那就是「整份替换」换了个名字）⇒ 整批拒，盘上一个字节不动。
    BadEdit(String),
    /// `setin` 认不出那一个元素（`ambiguous` = 认出了不止一个）⇒ **整批拒**，盘上一个字节不动
    /// （设置页按格改一台，而那台在盘上已被改名 / 删掉：不许一半落盘、一半静默丢掉）。
    NoSuchElement {
        ambiguous: bool,
    },
    /// `insertin` 要插的那个键盘上已经有了 ⇒ **整批拒**，盘上一个字节不动（不静默变成改那一台）。
    ElementExists,
    /// 改完的机器表不成立（同名 · 端口越界 · 地址 / 用户空）⇒ **整批拒**，盘上一个字节不动。
    /// 单台改、添加一台、批量添加都走这一口（同一份校验，[`check_machine_table`]）。
    BadMachine(String),
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
            ConfigWriteError::BadEdit(m)
            | ConfigWriteError::BadMachine(m)
            | ConfigWriteError::Io(m) => f.write_str(m),
        }
    }
}

/// 进程里所有写 `config.json` 的人共用这一把锁（主窗 / 设置窗的 IPC、`logging.rs` 的诊断写口）。
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 前端写配置的唯一口：交一串 [`ConfigEdit`]，这里合并进盘上那份。
#[tauri::command]
pub fn patch_config(edits: Vec<ConfigEdit>) -> Result<(), String> {
    let path = resolve_config_path().ok_or_else(|| "no home dir".to_string())?;
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
    // ②b 跨进程：两个 monitor 进程同写也串起来 —— 先保证目录在（锁的就是它），再拿它的锁，锁住之后才现读。
    let dir = path
        .parent()
        .ok_or_else(|| ConfigWriteError::Io(format!("no parent dir for {}", path.display())))?;
    // 数据目录只给本人（缺的几层建成 0700，已在的不动）；下面那份临时件出生即 0600，换名上位后 `config.json` 就是它的权限。
    crate::platform::fs::ensure_private_dir(dir).map_err(ConfigWriteError::Io)?;
    let _cross = crate::platform::fs::hold_dir_lock(dir).map_err(ConfigWriteError::Io)?;

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
    // ④b 动了机器表里认人的那几格（增一台 · 改名称 / 地址 / 端口 / 用户 · 整段写）⇒ 改完的整张表要成立。
    //   固化指纹、改恢复命令这类格不查：别的格写不进去不该挡住它们。
    let identity = |e: &ConfigEdit| match e {
        ConfigEdit::InsertIn { path, .. } => path.first().map(String::as_str) == Some("remote"),
        ConfigEdit::SetIn { path, field, .. } => {
            path.first().map(String::as_str) == Some("remote")
                && ["label", "host", "port", "user"].contains(&field.as_str())
        }
        ConfigEdit::Set { path, .. } => path.first().map(String::as_str) == Some("remote"),
        ConfigEdit::Remove { .. } | ConfigEdit::RemoveIn { .. } => false,
    };
    if edits.iter().any(identity) {
        if let Some(why) = check_machine_table(&root) {
            return Err(ConfigWriteError::BadMachine(why));
        }
    }

    // ⑤ 临时件带 pid：两个 monitor 进程不互删对方的临时件。
    let pretty = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| ConfigWriteError::Io(e.to_string()))?;
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    crate::platform::fs::only_me_on_create(&mut opts);
    opts.open(&tmp)
        .and_then(|mut f| std::io::Write::write_all(&mut f, pretty.as_bytes()))
        .map_err(|e| ConfigWriteError::Io(format!("write {}: {e}", tmp.display())))?;
    crate::platform::fs::atomic_replace(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ConfigWriteError::Io(format!("replace → {}: {e}", path.display()))
    })?;
    Ok(applied)
}

/// 机器表（`remote.hosts`）成不成立：名字（`label` 非空取它、否则 `host`，去首尾空白、大小写敏感）不重 ·
/// 端口 1–65535（缺 ＝ 22）· 地址与用户非空。不成立 ⇒ 第一处的那一句（文案表）；成立 / 没有机器表 ⇒ `None`。**纯函数**。
pub(crate) fn check_machine_table(root: &Map<String, Value>) -> Option<String> {
    let hosts = root.get("remote")?.get("hosts")?.as_array()?;
    let mut seen = std::collections::HashSet::new();
    for h in hosts.iter().filter_map(Value::as_object) {
        let text = |k: &str| {
            h.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
        };
        let name = if text("label").is_empty() {
            text("host")
        } else {
            text("label")
        };
        if text("host").is_empty() {
            return Some(copy_text("rsConfig.machine.noHost", &[("name", name)]));
        }
        if text("user").is_empty() {
            return Some(copy_text("rsConfig.machine.noUser", &[("name", name)]));
        }
        let port_ok = match h.get("port") {
            None | Some(Value::Null) => true,
            Some(p) => p.as_u64().is_some_and(|p| (1..=65535).contains(&p)),
        };
        if !port_ok {
            return Some(copy_text("rsConfig.machine.port", &[("name", name)]));
        }
        if !seen.insert(name.to_string()) {
            return Some(copy_text("rsConfig.machine.nameTaken", &[("name", name)]));
        }
    }
    None
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

// `atomic_replace`（两个平台臂：`MoveFileExW` / `rename`）搬进 `platform/fs.rs`，本文件经 `crate::platform::fs::atomic_replace` 调。

/// 文件缺失时返回的最小骨架。仅做占位，前端读到没有 `theme` 字段会用 :root 默认值。
fn default_config() -> Value {
    serde_json::json!({})
}

/// F87b③ 起：**已配置且启用**的远端 origin（canonical `origin_label()`）。今天它是通用的「列远端配置标签」（历史清单 · 搜索 ·
/// cc-bus 两块 · MCP 推 / 拉面板都用它）：读的是 monitor 自己的配置 —— 从 `mcp.rs` 挪来（MCP 读写进了那台后端，`mcp.rs` 删了）。
#[tauri::command]
pub async fn list_remote_mcp_origins() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(|| {
        crate::load_remote_configs()
            .iter()
            .map(|c| c.origin_label())
            .collect()
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))
}

// ═══════════════════════════════════════════════════════════════════════
// 原 `paths.rs` 的正文：Claude 数据目录 · monitor 数据目录 · 配置文件路径（头注见本文件顶上那一节）。
// ═══════════════════════════════════════════════════════════════════════

/// 那个 env 出口的名字。
///
/// 🔴 它**只为「把这个进程整体挪到别处跑」而存在**（跑自动化测试、跑一次性复算），
/// **不是**给用户搬家用的设置面。用户那一侧的「数据位置」是只读展示
/// （`data_paths.rs` → 设置面板那一块）。
pub const DATA_DIR_ENV: &str = creds_core::store::DATA_DIR_ENV;

/// Monitor 自己的 user-data 目录。
///
/// 默认 `~/.cc-monitor/`—— **不跟随 `claudeDir` 变化**
/// （避免循环依赖、且保留用户设置在切换数据目录后仍存在）。
///
/// # 🔴 那个 env 出口为什么必须有
///
/// 这个目录是**用户手写的真相**的家：`config.json` · tab 集合名 · 固定了哪些 tab
/// · 凭据库 · 历史元数据。理由是
/// 「**集合名是用户手写的真相，不是能重算的缓存 ⇒ 它必须活过一次清缓存**」。
///
/// ⇒ 而在这之前它**没有任何出口** ⇒ 任何一趟「把 monitor 跑起来量点东西」
/// 都会**写进用户真 profile**。2026-09-21 在那台 Win11 虚拟机上跑 tier-2 时
/// 现打到这一形：`auto-launch.json` 从 87 字节被改成 133 字节，
/// 那一路只能靠**跑前备份、跑后还原**做到零残留 ——
/// 而「靠每次记得备份」不是一个机制，是一次运气。
///
/// # 🔴 给了但不合法 ⇒ 回 `None`，**不退回用户真 profile**
///
/// 这一条是本函数唯一有争议的地方，所以写清：
/// 退回真 profile 看起来「更稳」，实际是**这个出口存在的理由的反面** ——
/// 那一趟自动化会以为自己被隔离了，而它正在写用户的东西，**而且没有一句话**。
/// ⇒ 宁可让各个消费者**可见地降级**（凭据库拿不到、
/// 设置面板那一块显示拿不到路径），也不要静默写对家。
///
/// ⚠ 只认**绝对路径**：相对路径会按进程 cwd 解，而这个进程的 cwd 不是它自己定的。
///
/// ⚠ 它**不建目录、不判存在** —— 首次启动时它本来就不存在（建目录是各消费者的事）。
pub fn resolve_monitor_data_dir() -> Option<PathBuf> {
    monitor_data_dir_from(
        std::env::var(DATA_DIR_ENV).ok().as_deref(),
        creds_core::store::home_dir(),
    )
}

/// [`resolve_monitor_data_dir`] 里**有逻辑的那一段**，抽出来所以判得到。
///
/// 🔴 **抽出来的理由不是风格，是本仓踩过的一条**：`lib_env_scrub_tests` 那条判据
/// 头注逐字「**绝不能在测试里 set/remove 真实的 `CLAUDE_*` 变量
/// （会干扰并发测试与宿主环境）**」—— cargo test 多线程跑、进程级 env 共享，
/// 而 `resolve_monitor_data_dir` 有 8 处消费者。
/// ⇒ 判据**不许**去动那个 env；它把值当参数喂进来。
/// （同族先例：`filewin::download::judge_dest` 把「那儿有没有东西」注进来。）
pub fn monitor_data_dir_from(env_val: Option<&str>, home: Option<PathBuf>) -> Option<PathBuf> {
    // 规则本身住 `creds_core::store::monitor_data_dir`（远端常驻后端按同一份推默认路径）；这里只留两句日志。
    // 设成空串 == 没设（shell 里 `CCM_DATA_DIR=` 是最常见的「取消」写法）。
    let set = env_val.map(str::trim).filter(|t| !t.is_empty());
    let r = creds_core::store::monitor_data_dir(env_val, home);
    match (set, &r) {
        (Some(_), Some(p)) => {
            tracing::info!("monitor_data_dir from {}: {}", DATA_DIR_ENV, p.display())
        }
        // 🔴 **不退回真 profile** —— 逐条理由住上面那一节。
        (Some(t), None) => tracing::warn!(
            "{} 不是绝对路径（{}）—— 拒绝使用，也**不**退回 ~/.cc-monitor：                 那会让一趟以为自己被隔离了的自动化去写用户的东西",
            DATA_DIR_ENV,
            t
        ),
        _ => {}
    }
    r
}

/// Monitor 配置文件：`<monitor_data_dir>/config.json`
pub fn resolve_config_path() -> Option<PathBuf> {
    Some(resolve_monitor_data_dir()?.join("config.json"))
}

/// 读取 monitor config.json 的 `claudeDir` 字段（用户在设置面板里写入）——原值，不查在不在、不回退：
/// 怎么解析是那台后端的事，这里只负责把用户填的那一格交过去。
pub(crate) fn claude_dir_override() -> Option<PathBuf> {
    let cfg = resolve_config_path()?;
    if !cfg.exists() {
        return None;
    }
    // 〔E 吞错普查点名〕文件在而读不动 / 不是 JSON ⇒ 设置里那条「Claude 目录」覆盖这一次**不生效**、回到默认目录。
    // 原先两个 `.ok()?` 把它折成「没设」—— 用户以为改了目录，实际读的是默认那一份，一句话都没有。
    // 每次起 / 停本机后端都读一遍 ⇒ 同一个进程里只说一次。
    let value: serde_json::Value = match std::fs::read_to_string(&cfg)
        .map_err(|e| e.to_string())
        .and_then(|raw| serde_json::from_str(&raw).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => {
            static SAID: std::sync::Once = std::sync::Once::new();
            SAID.call_once(|| {
                tracing::warn!(
                    "设置里的 Claude 目录覆盖这一次没交给本机后端：{} 读不动或不是 JSON（{e}）—— 后端按它自己的规矩解析",
                    cfg.display()
                );
            });
            return None;
        }
    };
    let dir_str = value.get("claudeDir")?.as_str()?;
    if dir_str.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(dir_str))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/config_tests.rs"]
mod tests;

// 原 `paths.rs` 那份单测（文件不改名，挂在本模块下；`use super::*` 照旧够到这些项）。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/paths_tests.rs"]
mod paths_tests;
