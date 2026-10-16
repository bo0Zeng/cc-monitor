//! 用户配置 R/W —— `~/.cc-monitor/config.json`。
//!
//! Rust 端不解释配置内容（schema 在前端定义，顶层键的主人登记在 `src/frontend/ui/config.ts::CONFIG_KEY_OWNERS`），
//! 只负责读、**按键补丁**写 ＋ 文件缺失时给出最小骨架。
//!
//! 配置文件位置走 [`resolve_config_path`] —— monitor 自己的设置永远在
//! 默认 `~/.cc-monitor/` 下（一台机器一个家，与后端同一个），不跟随 Claude 数据目录变化。
//!
//! 从前每个前端模块「读整份 → 改自己的键 → 整份写回」，主窗（tab 栏的分组 / 固定 / 顺序）与设置窗
//! （行为 / 主题 / 快捷键 / 账号 / 远端）是同一进程里的两个 webview，两次读-改-写一交错，
//! 后写的整份就把先写的键盖掉（E §E1：拖放同拍发分组 ＋ 顺序，分组那次没落盘）。
//! 守的要求：「各自只写自己那个键」· 红线 ④「读不懂的 `config.json` 不写」。
//!
//! 现在前端只交「改哪几条路径」（[`ConfigEdit`]），这里在**一把进程级锁里现读盘、逐条应用、原子替换** ——
//! 主窗 / 设置窗 / `logging.rs` 的诊断写口都走这一个函数，谁写的键谁的值留在盘上。
//! 两个 monitor 进程同写（单实例拦不住的那几形：Linux 上没有会话总线 · 两个登录会话）也串起来了：进程内那把锁里面、现读之前，
//! 再拿 `config.json` 所在目录的**跨进程**锁（`platform::fs::hold_dir_lock`：unix 对目录 `flock` · Windows 命名互斥量，与后端第四层同一种锁）。
//!
//! # 〔「`config` ＋ `paths` → 一处」〕数据目录与配置文件路径（原 `paths.rs`，整份并进本文件、只挪住址）
//!
//! monitor 自己配置文件的路径解析。
//!
//! ### Claude 目录不在这里
//!
//! 「这台的 Claude 数据目录在哪」只由那台后端解析（`agents/claudecode/paths.rs::resolve_home`）；设置里那一格住本机后端的
//! `prefs.json`（`agentHomes`），后端起来时自己读，monitor 不读、不交。
//!
//! monitor 自己的 config.json 始终保存在**默认位置** `~/.cc-monitor/config.json`，**不**跟随 Claude 数据目录变化：
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
use crate::detail::Said;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[tauri::command]
pub fn load_config() -> Result<Value, Said> {
    let r: Result<Value, Said> = (move || -> Result<Value, Said> {
        let path =
            resolve_config_path().ok_or_else(|| Said::of("rsShellCmd.config.noHome", &[]))?;
        if !path.exists() {
            return Ok(default_config());
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| {
            Said::with_raw(
                copy_text(
                    "rsShellCmd.config.readFailed",
                    &[("why", &copy_core::reason::io_reason(e.kind()))],
                ),
                format!("{}\n{e}", path.display()),
            )
        })?;
        Ok(serde_json::from_str(&raw).map_err(|e| {
            Said::with_raw(
                copy_text("rsShellCmd.config.unparsable", &[]),
                format!("{}\n{e}", path.display()),
            )
        })?)
    })();
    r.map_err(|s| s.named("load_config"))
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
}

/// 一条补丁落下去的结局（与交进来的补丁逐条对应）。`Set` / `Remove` 恒 `Done`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Applied {
    Done,
}

impl ConfigEdit {
    fn path(&self) -> &[String] {
        match self {
            ConfigEdit::Set { path, .. } | ConfigEdit::Remove { path } => path,
        }
    }
}

/// [`patch_config_at`] 失败的三种。分开是为了让调用方各说各的话（`logging.rs` 那句「诊断设置没有存」原样保留）。
#[derive(Debug)]
pub(crate) enum ConfigWriteError {
    /// 盘上那份读不懂 / 根不是对象 ⇒ **一个字节没写**（红线 ④）。
    Unreadable { path: PathBuf, detail: String },
    /// 补丁本身不成形（空路径 —— 那就是「整份替换」换了个名字）⇒ 整批拒，盘上一个字节不动。
    BadEdit(String),
    /// 盘上那一步没成：那一句 ＋ 复制详情（原话在详情里）。
    Io(Said),
}

impl ConfigWriteError {
    /// 交给界面的那一形：读不懂那份的解析错与盘上那一步的原话进复制详情，其余只有那一句。
    pub(crate) fn into_said(self) -> Said {
        match self {
            ConfigWriteError::Io(s) => s,
            ConfigWriteError::Unreadable { ref detail, .. } => {
                let raw = detail.clone();
                Said::with_raw(self.to_string(), raw)
            }
            other => Said::written(other.to_string()),
        }
    }
}

impl std::fmt::Display for ConfigWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigWriteError::Unreadable { path, .. } => f.write_str(&copy_text(
                "rsConfig.write.unreadable",
                &[("path", &path.display().to_string())],
            )),
            ConfigWriteError::BadEdit(m) => f.write_str(m),
            ConfigWriteError::Io(s) => f.write_str(&s.said),
        }
    }
}

/// 进程里所有写 `config.json` 的人共用这一把锁（主窗 / 设置窗的 IPC、`logging.rs` 的诊断写口）。
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 前端写配置的唯一口：交一串 [`ConfigEdit`]，这里合并进盘上那份。
#[tauri::command]
pub fn patch_config(edits: Vec<ConfigEdit>) -> Result<(), Said> {
    let r: Result<(), Said> = (move || -> Result<(), Said> {
        let path = resolve_config_path().ok_or_else(|| Said::of("rsConfig.write.noHome", &[]))?;
        patch_config_at(&path, &edits).map_err(ConfigWriteError::into_said)?;
        Ok(())
    })();
    r.map_err(|s| s.named("patch_config"))
}

/// 🔴 **`config.json` 唯一的写函数。** 锁内现读 → 逐条应用 → 带 pid 的临时件 → 原子替换。回逐条结局；一条都没有 ⇒ 不写。
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
    if edits.is_empty() {
        return Ok(Vec::new());
    }
    // ② 串行化。锁中毒（别的写者 panic 了）不妨碍这一次：锁只护「读-改-写」这一段，没有要恢复的内存状态。
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // ②b 跨进程：两个 monitor 进程同写也串起来 —— 先保证目录在（锁的就是它），再拿它的锁，锁住之后才现读。
    let dir = path.parent().ok_or_else(|| {
        ConfigWriteError::Io(
            Said::of(
                "rsConfig.write.noParent",
                &[("path", &path.display().to_string())],
            )
            .into(),
        )
    })?;
    // 数据目录只给本人（缺的几层建成 0700，已在的不动）；下面那份临时件出生即 0600，换名上位后 `config.json` 就是它的权限。
    crate::platform::fs::ensure_private_dir(dir).map_err(ConfigWriteError::Io)?;
    let _cross = crate::platform::fs::hold_dir_lock(dir).map_err(ConfigWriteError::Io)?;

    // ③ 锁内现读。不存在 ⇒ 空对象；读不懂 / 根不是对象 ⇒ 不写。
    let mut root: Map<String, Value> = if path.exists() {
        let raw = std::fs::read_to_string(path).map_err(|e| {
            ConfigWriteError::Io(Said::with_raw(
                copy_text(
                    "rsConfig.write.readFailed",
                    &[
                        ("path", &path.display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ))
        })?;
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

    // ⑤ 临时件带 pid：两个 monitor 进程不互删对方的临时件。
    let pretty = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| {
        ConfigWriteError::Io(Said::with_raw(
            copy_text("rsConfig.write.encodeFailed", &[]),
            &e,
        ))
    })?;
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    crate::platform::fs::only_me_on_create(&mut opts);
    // 换名前先落盘：不然断电后换上去的可能是一份空文件。
    opts.open(&tmp)
        .and_then(|mut f| {
            std::io::Write::write_all(&mut f, pretty.as_bytes())?;
            f.sync_all()
        })
        .map_err(|e| {
            ConfigWriteError::Io(Said::with_raw(
                copy_text(
                    "rsConfig.write.tmpFailed",
                    &[
                        ("path", &tmp.display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ))
        })?;
    // 标准库的换名在 Windows 上就是覆盖已在的目标，长路径前缀它自己补。
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ConfigWriteError::Io(Said::with_raw(
            copy_text(
                "rsConfig.write.replaceFailed",
                &[
                    ("path", &path.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        ))
    })?;
    Ok(applied)
}

fn apply_edit(root: &mut Map<String, Value>, edit: &ConfigEdit) -> Applied {
    let (last, parents) = edit.path().split_last().expect("空路径在 ① 已拒");
    let mut cur = root;
    match edit {
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

/// 文件缺失时返回的最小骨架。仅做占位，前端读到没有 `theme` 字段会用 :root 默认值。
fn default_config() -> Value {
    serde_json::json!({})
}

/// F87b③ 起：**已配置且启用**的远端 origin（canonical `origin_label()`）。今天它是通用的「列远端配置标签」（历史清单 · 搜索 ·
/// cc-bus 两块 · MCP 推 / 拉面板都用它）：读的是 monitor 自己的配置 —— 从 `mcp.rs` 挪来（MCP 读写进了那台后端，`mcp.rs` 删了）。
#[tauri::command]
pub async fn list_remote_mcp_origins() -> Result<Vec<String>, Said> {
    let r: Result<Vec<String>, Said> = async move {
        Ok(tokio::task::spawn_blocking(|| {
            crate::load_remote_configs()
                .iter()
                .map(|c| c.origin_label())
                .collect()
        })
        .await
        .map_err(Said::crashed)?)
    }
    .await;
    r.map_err(|s| s.named("list_remote_mcp_origins"))
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
/// 默认 `~/.cc-monitor/`—— **不跟随 Claude 数据目录变化**
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

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/config_tests.rs"]
mod tests;

// 原 `paths.rs` 那份单测（文件不改名，挂在本模块下；`use super::*` 照旧够到这些项）。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/paths_tests.rs"]
mod paths_tests;
