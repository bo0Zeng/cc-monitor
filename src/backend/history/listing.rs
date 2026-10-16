//! **会话清单那一族**：项目清单 · 平铺会话清单（连那份「没变不重扫、变长只扫尾巴」的缓存）·
//! 后台热缓存 · 叫一条会话什么（[`label_facts_of`] · [`session_title_of`]）· 「这一行能做什么」（[`can_of`]）· 分叉来源（[`fork_origin`]）·
//! 藏起来的那个工作目录（[`hidden_cwd`]）· `<agent_home>/projects/` 那道围栏（[`fence_under_projects`]）· 这台的 agent 家（[`agent_home`]）。
//!
//! 历史清单（`history_list`）· 资产目录（`assets::asset_catalog`）· 观测侧（记录级的读 · 全文搜索 · watcher）都来这里拿；
//! 这一族不引 observe（`layering_guard::ONE_WAY`）。
//!
//! 安全：按名字给的项目目录不许含路径分隔符；路径一律经 [`crate::common::fence::Fence`] 放行（`canonicalize` 后前缀校验，防 `../` 穿越）。
//! 读记录只 read_dir / read；写只有清单缓存落盘那一份（经启动时装的 [`Own`]）。

use super::listing_scan::SessionScan;
use crate::common::append_read::{Blocks, Step, Tailed};
use crate::common::fence::Fence;
use crate::platform::fs::{Own, OwnPath, Ro};
pub(crate) use cmd_wire::listing::{Can, CanDelete, CanResume};
use copy_core::copy_text;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

/// 一条会话的状态：`live` 在跑 · `reconnectable` 不在跑、tmux 里还挂着它（会话账本说的）· `ended` 已结束 ·
/// `unknown` 这条路上答不了（合成历史没有 pidfile）。历史清单的行与全文搜索的命中同一个函数。
pub(crate) fn status_of(is_live: Option<bool>, reconnectable: bool) -> &'static str {
    match is_live {
        Some(true) => "live",
        Some(false) if reconnectable => "reconnectable",
        Some(false) => "ended",
        None => "unknown",
    }
}

/// **这条会话能做什么**（`can`）——界面按它画，不另判：
/// - `resume`：`yes` 能恢复 · `switch` 在跑（切过去，不再起第二份）· `bg` 分身会话（要接着聊得恢复主会话）；
/// - `accounts`：恢复时能不能选号（那一家有没有账号这一维）· `fork`：能不能从某一轮分叉；
/// - `delete`：`yes` · `live` 在跑（先结束它）· `unsure` 说不清在不在跑（确认框里多说一句）。
pub(crate) fn can_of(kind: &str, status: &str, bg: bool) -> Can {
    Can {
        resume: if status == "live" {
            CanResume::Switch
        } else if bg {
            CanResume::Bg
        } else {
            CanResume::Yes
        },
        accounts: crate::agents::account_env_of(kind).is_some(),
        fork: crate::agents::record_face(kind).is_some_and(|r| r.branch.is_some()) && !bg,
        delete: match status {
            "live" => CanDelete::Live,
            "unknown" => CanDelete::Unsure,
            _ => CanDelete::Yes,
        },
    }
}

/// 项目清单的本体，出口是参数（资产目录 `asset_catalog` 写内存）。
///
/// 回「记录树根在不在」：不在 ⇒ `Ok(false)`、一行不写（这台还没起过会话 —— 判定只在这一处）；
/// 怎么说由调用方定：资产目录当零个项目。
pub(crate) fn list_projects_into(agent_home: &Path, out: &mut dyn Write) -> Result<bool, String> {
    let Some(root) = records_root(agent_home) else {
        return Ok(false);
    };
    let entries = match Ro::read_dir(&root) {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(unreadable_dir(&root, &e)),
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        // 空目录（全删过/只剩本机后端）不展示；一个记录目录里会话的真实目录不止一个 ⇒ 一组一行。
        for line in project_rows(&dir, &dir_name) {
            writeln!(out, "{line}").map_err(|e| format!("stdout write failed: {e}"))?;
        }
    }
    Ok(true)
}

/// 一个记录目录里的会话按**真实目录**分组 —— 记录目录名把非 ASCII 字符、`.`、`/` 都折成 `-`，
/// 不同的目录会撞成同一个名字。输入每个会话的 `(读出的目录, 修改时刻)`，回每个会话归哪一组（目录）。
/// 读不出目录的归最近修改的那个读得出目录的会话那一组；一个都读不出 ⇒ 空串一组。
/// 项目清单（[`project_rows`]）与平铺清单（[`sessions_by_dir`]）用的是这同一个函数。
pub(crate) fn group_by_cwd(items: &[(Option<String>, i64)]) -> Vec<String> {
    let fallback = items
        .iter()
        .filter_map(|(c, m)| c.as_ref().map(|c| (*m, c)))
        .max_by_key(|(m, _)| *m)
        .map(|(_, c)| c.clone())
        .unwrap_or_default();
    items
        .iter()
        .map(|(c, _)| c.clone().unwrap_or_else(|| fallback.clone()))
        .collect()
}

/// 会话记录目录读不了 ⇒ 给人看的那一句（按错误的**种类**说；系统原话只进日志 —— 不露实现词）。
pub(crate) fn unreadable_dir(dir: &Path, e: &std::io::Error) -> String {
    tracing::warn!("history listing: 读不了 {}：{e}", dir.display());
    let path = dir.display().to_string();
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        copy_text("beHistory.dir.denied", &[("path", &path)])
    } else {
        copy_text("beHistory.dir.unreadable", &[("path", &path)])
    }
}

/// 一个项目目录 → 项目清单的那几行（按会话的真实目录分组，一组一行，[`group_by_cwd`]）；
/// 目录下没有会话记录 ⇒ 一行都没有（不展示）。
///
/// # 为什么它是**一个函数**而不是 `list_projects` 里的一段
///
/// 判据要判的是**这一行带了什么**。
/// 同样的分法在本文件里已有先例：`analyze_session` 也是把「算出那一行」与「把它印出去」分开的。
fn project_rows(dir: &Path, dir_name: &str) -> Vec<serde_json::Value> {
    project_rows_hiding(dir, dir_name, &hidden_cwd)
}

/// 这台家里那个不出会话的工作目录（`~/.cc-monitor/autostart/`，名字住 [`crate::common::own_dir::HIDDEN_WORK_DIR`]）：
/// 在那里跑的 `claude`（例如替号开额度窗口的那一句、`quota-probe` 报用量的那一次）历史页与会话列表都不出。观测侧只按它藏。
const HIDDEN_DIR: &str = crate::common::own_dir::HIDDEN_WORK_DIR;

/// 工作目录是 [`HIDDEN_DIR`] 的会话不出。**判据只在这一处**（项目清单按它摘组、观测侧按 pidfile 的目录不宣告）。
pub(crate) fn hidden_cwd(cwd: &str) -> bool {
    crate::platform::paths::data_home().is_some_and(|home| hidden_cwd_in(&home, cwd))
}

/// [`hidden_cwd`] 的本体：`data_home` 由调用方给（判据喂夹具）。记录里的目录是那一家解析过链接的那一形 ⇒ 两形都认
/// （解开链接经 observe 唯一那道路径解析 [`crate::common::fence::Fence`]）。
pub(crate) fn hidden_cwd_in(data_home: &Path, cwd: &str) -> bool {
    let dir = data_home.join(HIDDEN_DIR);
    let cwd = Path::new(cwd);
    cwd == dir || crate::common::fence::Fence::at(&dir).is_ok_and(|f| cwd == f.root())
}

/// [`project_rows`] 的本体：`hide(目录)` ⇒ 那一组不出。
fn project_rows_hiding(
    dir: &Path,
    dir_name: &str,
    hide: &dyn Fn(&str) -> bool,
) -> Vec<serde_json::Value> {
    // 目录与修改时刻与会话清单那条同源（`project_dir_of` · `mtime_ms`），两边分出来的组才一样。
    let mut sessions: Vec<(Option<String>, i64)> = Vec::new();
    if let Ok(files) = Ro::read_dir(dir) {
        for f in files.flatten() {
            let p = f.path();
            if p.is_file() && crate::agents::is_tree_session_file(&p) {
                sessions.push((crate::agents::project_dir_of(&p), Ro::mtime_ms(&p)));
            }
        }
    }
    let mut groups: std::collections::BTreeMap<String, (u32, i64)> = Default::default();
    for ((_, mtime), path) in sessions.iter().zip(group_by_cwd(&sessions)) {
        let g = groups.entry(path).or_default();
        g.0 += 1;
        g.1 = g.1.max(*mtime);
    }
    groups
        .into_iter()
        .filter(|(project_path, _)| !hide(project_path))
        .map(|(project_path, (count, last_activity_ms))| {
            serde_json::json!({
                "dirName": dir_name,
                "projectPath": project_path,
                "sessionCount": count,
                "lastActivityMs": last_activity_ms,
            })
        })
        .collect()
}

/// 一个项目目录的会话清单（每份会话一行）。`only` ＝ 只要这一个会话的整行（按 sid 问清单那一条）：别的会话只出分组要的那几格
/// （sid · 读出的目录 · 修改时刻 —— 都不用扫整份），整行只扫叫这个名字的那一份。
pub(crate) fn list_sessions_for(
    agent_home: &Path,
    project_dir: &str,
    only: Option<&str>,
    out: &mut dyn Write,
) -> Result<(), String> {
    // project_dir 是目录名而非路径：拒绝任何分隔符 / 上跳
    if project_dir.contains('/') || project_dir.contains('\\') || project_dir.contains("..") {
        return Err(format!("invalid project dir name: {project_dir}"));
    }
    // 与 read_session 对齐（也兑现本文件头部"canonicalize 后前缀校验"的承诺）：名字
    // 合法但 projects/ 下若有指向外部的 symlink 目录，read_dir 会跟随逃逸出 projects/
    // ——canonicalize 解析 symlink 后做前缀校验挡住。
    // **改调共享围栏**（E3）：此前这里是一份内联副本，
    // 注释写着「与 `read_session` 对齐」—— 靠手工对齐的两份迟早会漂。
    let dir = fence_under_projects(agent_home, Path::new(project_dir))?;
    let entries = Ro::read_dir(&dir).map_err(|e| unreadable_dir(&dir, &e))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() || !crate::agents::is_tree_session_file(&p) {
            continue;
        }
        let meta = match only {
            Some(sid) if p.file_stem().is_none_or(|s| s != sid) => serde_json::json!({
                "sessionId": p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                "cwd": crate::agents::project_dir_of(&p),
                "updatedAtMs": Ro::mtime_ms(&p),
            }),
            _ => analyze_session_cached(&p),
        };
        writeln!(out, "{meta}").map_err(|e| format!("stdout write failed: {e}"))?;
    }
    Ok(())
}

/// 平铺清单（`history-list`）那一份：这台**每个**记录目录 ⇒ `(目录名, 那一目录的会话行 | 读不了的那一句)`。
/// 会话行同 [`list_sessions_for`]，`cwd` 换成按 [`group_by_cwd`] 归的那一组 ——
/// 与项目清单同一个分组（记录目录名会撞，按真实目录分）；归到藏起来的那个目录（[`hidden_cwd`]）的不出。
/// 记录树根不在 ⇒ `Ok(None)`（这台还没起过会话，同 [`list_projects_into`]）。
pub(crate) fn sessions_by_dir(
    agent_home: &Path,
) -> Result<Option<Vec<(String, Result<Vec<serde_json::Value>, String>)>>, String> {
    sessions_by_dir_for(agent_home, None)
}

/// [`sessions_by_dir`] 的本体；`only` 见 [`list_sessions_for`]（别的会话那几行只够分组用，调用方按 sid 滤掉）。
pub(crate) fn sessions_by_dir_for(
    agent_home: &Path,
    only: Option<&str>,
) -> Result<Option<Vec<(String, Result<Vec<serde_json::Value>, String>)>>, String> {
    let Some(root) = records_root(agent_home) else {
        return Ok(None);
    };
    let entries = match Ro::read_dir(&root) {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(unreadable_dir(&root, &e)),
    };
    let dirs: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    // 〔perfC〕按目录分给几条线程扫（冷的时候是整台每份会话从头扫一遍，单线程要几秒）；结果最后按目录名排，与单线程逐字相同。
    let mut out: Vec<_> = crate::common::par::par_in_order(dirs, |dir_name| {
        rows_of_dir(agent_home, &dir_name, only).map(|got| (dir_name, got))
    })
    .into_iter()
    .flatten()
    .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    if only.is_none() {
        save();
    }
    Ok(Some(out))
}

/// 一个记录目录的会话行（`None` ＝ 这一目录一行都没有，不出）。
fn rows_of_dir(
    agent_home: &Path,
    dir_name: &str,
    only: Option<&str>,
) -> Option<Result<Vec<serde_json::Value>, String>> {
    let mut buf = Vec::new();
    let rows = list_sessions_for(agent_home, dir_name, only, &mut buf).map(|()| {
        let mut rows: Vec<serde_json::Value> = String::from_utf8_lossy(&buf)
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        let keys: Vec<(Option<String>, i64)> = rows
            .iter()
            .map(|v| {
                (
                    v["cwd"].as_str().map(str::to_string),
                    v["updatedAtMs"].as_i64().unwrap_or(0),
                )
            })
            .collect();
        for (v, cwd) in rows.iter_mut().zip(group_by_cwd(&keys)) {
            v["cwd"] = serde_json::json!(cwd);
        }
        rows.retain(|v| !hidden_cwd(v["cwd"].as_str().unwrap_or_default()));
        rows
    });
    if matches!(&rows, Ok(r) if r.is_empty()) {
        return None;
    }
    Some(rows)
}

/// 一份记录扫出的那一行，连同扫到哪了（[`ListingEntry`]）：没变不再扫，变长只扫新增的那一段。
/// 常驻进程里平铺清单（`history-list`）每次都要过这台**全部**会话；一次性进程里这张表只活一趟（等于没有）。
static SESSION_META: std::sync::Mutex<std::collections::BTreeMap<PathBuf, ListingEntry>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// [`SESSION_META`] 的上限（条）：超了整张清掉重来（不做淘汰次序 —— 一台上会话数到这个量级之前它都不会触发）。
const SESSION_META_CAP: usize = 50_000;

/// 清单缓存落盘的家（启动入口装：[`install`]）：数据目录底下一份 [`relay_route_core::LISTING_CACHE_REL`]。
/// 没装（判据 · 一次性进程）⇒ 只在内存。
static STORE: std::sync::OnceLock<Own> = std::sync::OnceLock::new();

/// 盘上那一份装回过没有（头一次碰清单缓存时装，谁先碰谁装）。
static LOADED: std::sync::Once = std::sync::Once::new();

/// 内存那一份自上次落盘之后变过没有（没变不重写）。
static DIRTY: AtomicBool = AtomicBool::new(false);

/// 盘上那一份的格式：形状变了就加一（对不上 ⇒ 当没有，照记录重扫、覆盖）。
const STORE_VERSION: u32 = 1;

/// 读盘上那一份的上限。
const STORE_MAX_READ: u64 = 256 << 20;

/// 启动时装一次（拿启动时造的写句柄）：之后重启照盘上那一份接着认 —— 没变的记录不读、变长的只读追加的那一截。
pub fn install(own: &Own) {
    if STORE.set(own.clone()).is_err() {
        tracing::debug!("listing cache home already installed; keeping the first one");
    }
}

/// 盘上一份：版本 ＋ 每份记录读到哪了、累计的那几格、成品行。
#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    v: u32,
    rows: Vec<SavedRow>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedRow {
    path: PathBuf,
    read: Tailed,
    done: SessionScan,
    tail: SessionScan,
    row: serde_json::Value,
}

/// 清单缓存那张表（头一次碰时先把盘上那一份装回）。
fn table() -> std::sync::MutexGuard<'static, std::collections::BTreeMap<PathBuf, ListingEntry>> {
    LOADED.call_once(|| {
        if let Some(own) = STORE.get() {
            load_from(own);
        }
    });
    SESSION_META.lock().unwrap_or_else(|e| e.into_inner())
}

/// 落盘那一份的（目录, 文件）。
fn store_at(own: &Own) -> Option<(OwnPath, OwnPath)> {
    let rel = relay_route_core::LISTING_CACHE_REL.strip_prefix(".cc-monitor/")?;
    let (dir, name) = rel.rsplit_once('/')?;
    let dir = own.path(dir).ok()?;
    let file = dir.join(name).ok()?;
    Some((dir, file))
}

/// 把盘上那一份装进内存（内存里已有的那几份不动）。没有 · 读不出 · 版本对不上 ⇒ 什么都不装。
pub(crate) fn load_from(own: &Own) {
    let Some((_, file)) = store_at(own) else {
        return;
    };
    let Ok(meta) = Ro::metadata(file.as_path()) else {
        return;
    };
    if !meta.is_file() || meta.len() > STORE_MAX_READ {
        tracing::warn!("listing cache skipped: not a file or over the cap");
        return;
    }
    let saved: Saved = match Ro::open(file.as_path())
        .map_err(|e| e.to_string())
        .and_then(|f| {
            serde_json::from_reader(std::io::BufReader::new(f)).map_err(|e| e.to_string())
        }) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("listing cache unreadable: {e}");
            return;
        }
    };
    if saved.v != STORE_VERSION {
        return;
    }
    let mut t = SESSION_META.lock().unwrap_or_else(|e| e.into_inner());
    for r in saved.rows {
        t.entry(r.path).or_insert_with(|| ListingEntry {
            read: r.read,
            done: r.done,
            tail: r.tail,
            row: r.row,
            last_bytes: 0,
        });
    }
}

/// 内存那一份变过 ⇒ 整份落盘（已经不在的记录不落）。没装家 ⇒ 不落。
fn save() {
    if let Some(own) = STORE.get() {
        save_to(own);
    }
}

/// [`save`] 的本体（家是参数）。写不成只记一笔调试日志：缓存，下一次照记录重扫。
pub(crate) fn save_to(own: &Own) {
    if !DIRTY.swap(false, Ordering::Relaxed) {
        return;
    }
    let rows: Vec<SavedRow> = table()
        .iter()
        .filter(|(p, _)| p.to_str().is_some())
        .map(|(p, e)| SavedRow {
            path: p.clone(),
            read: e.read.clone(),
            done: e.done.clone(),
            tail: e.tail.clone(),
            row: e.row.clone(),
        })
        .collect();
    let rows: Vec<SavedRow> = rows.into_iter().filter(|r| r.path.is_file()).collect();
    let saved = Saved {
        v: STORE_VERSION,
        rows,
    };
    let written = (|| -> Result<(), String> {
        let (dir, file) = store_at(own).ok_or("no data dir")?;
        let root = own.root().ok_or("no data dir")?;
        own.ensure_dir(&root).map_err(|e| e.to_string())?;
        own.ensure_dir(&dir).map_err(|e| e.to_string())?;
        let held = own.lock(&dir).map_err(|e| format!("{e:?}"))?;
        let body = serde_json::to_vec(&saved).map_err(|e| e.to_string())?;
        own.write(&file, &held, &body).map_err(|e| e.to_string())
    })();
    if let Err(e) = written {
        tracing::debug!("listing cache not stored: {e}");
    }
}

/// 清单缓存里的一份：读到哪了（与全文搜索索引共用的 [`Tailed`]）· 完整行累计的那几格 · 末尾半行的那几格 · 成品行。
struct ListingEntry {
    read: Tailed,
    done: SessionScan,
    tail: SessionScan,
    row: serde_json::Value,
    /// 上一次带到这一问时真读了多少字节（判据看它：变长只读尾巴）。
    last_bytes: u64,
}

impl ListingEntry {
    /// 从头逐行流式扫一份（不整份进内存）：完整行进 `done`、交给账本；末尾没写完的那截进 `tail`。
    fn scan(p: &Path, mtime: Option<std::time::SystemTime>) -> Self {
        use std::io::BufRead;
        let mut e = Self {
            read: Tailed::default(),
            done: SessionScan::default(),
            tail: SessionScan::default(),
            row: serde_json::Value::Null,
            last_bytes: 0,
        };
        let mut end = 0u64;
        if let Ok(file) = Ro::open(p) {
            let mut r = std::io::BufReader::new(file);
            let mut line = Vec::new();
            loop {
                line.clear();
                match r.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => end += n as u64,
                }
                if line.ends_with(b"\n") {
                    e.done.absorb_bytes(&line);
                    e.read.took(&line);
                } else {
                    e.tail.absorb_bytes(&line);
                }
            }
        }
        e.read.saw((mtime, end));
        e.last_bytes = end;
        e.row = e.done.joined(&e.tail).row(p);
        e
    }

    /// 尾巴上新长的那一截按块读进来（`new` 停在上次的完整行之后，读到尾；末尾可能是半行）。读出错 ⇒ `false`（这一份调用方整份重扫）。
    fn extend<R: std::io::Read>(
        &mut self,
        p: &Path,
        mtime: Option<std::time::SystemTime>,
        new: &mut Blocks<R>,
    ) -> bool {
        let base = self.read.consumed();
        loop {
            match new.next() {
                Ok(Some(whole)) => {
                    for line in whole.split_inclusive(|&b| b == b'\n') {
                        self.done.absorb_bytes(line);
                    }
                    self.read.took(&whole);
                }
                Ok(None) => break,
                Err(_) => return false,
            }
        }
        self.read.saw((mtime, base + new.read()));
        self.tail = SessionScan::default();
        self.tail.absorb_bytes(&new.rest());
        self.last_bytes = new.read();
        self.row = self.done.joined(&self.tail).row(p);
        true
    }
}

/// 后端一起来就后台把清单缓存热好（一条一次性线程，降到低优先级；不是定时器，扫完就退）：
/// 第一次打开历史页 / 按 sid 打开查看窗时不必再等整台从头扫一遍。远端流后端与本机常驻后端同一个调用点（`main.rs`）。
pub fn warm_listing_in_background(agent_home: PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("listing-warm".into())
        .spawn(move || {
            let lowered = crate::platform::proc::lower_this_thread();
            let n = warm_listing(&agent_home);
            tracing::info!("历史清单缓存：后台热好 {n} 份（低优先级：{lowered}）");
        });
    if let Err(e) = spawned {
        tracing::warn!("历史清单缓存：后台热缓存的线程起不来（{e}），第一次问清单会现扫");
    }
}

/// [`warm_listing_in_background`] 的本体（判据直接调它）：整台扫一遍清单，回缓存里有几份。
pub(crate) fn warm_listing(agent_home: &Path) -> usize {
    // 读不了记录树 ⇒ 这一趟热不了，说一句；第一次问清单时照样现扫、照样把那一句交给界面。
    if let Err(e) = sessions_by_dir(agent_home) {
        tracing::warn!("历史清单缓存：后台热缓存读不了记录树（{e}）");
    }
    table().len()
}

/// 一份会话记录里叫它什么的那几样（标题 · 第一句 · 项目目录；注解那一格由调用方并），走清单缓存（没变不重扫、变长只扫尾巴）。
pub(crate) fn label_facts_of(p: &Path) -> super::search_rules::LabelFacts {
    label_facts_among(crate::agents::REGISTRY, p)
}

/// [`label_facts_of`] 的可喂夹具那一半（注册表是参数）。
pub(crate) fn label_facts_among(
    registry: &[crate::agents::Adapter],
    p: &Path,
) -> super::search_rules::LabelFacts {
    // 有合成历史面的那一家（记录格式不是记录树那一家的）：第一句照它自己的读法（与历史清单那一行同一个摘录）；它没有标题记录。
    if let Some(excerpt) = crate::agents::synth_excerpt_among(registry, p) {
        return super::search_rules::LabelFacts {
            custom: None,
            title: None,
            first: excerpt(p),
            project_dir: crate::agents::project_dir_among(registry, p),
        };
    }
    let row = analyze_session_cached(p);
    super::search_rules::LabelFacts {
        custom: None,
        title: row["aiTitle"].as_str().map(str::to_string),
        first: row["firstUserExcerpt"].as_str().unwrap_or("").to_string(),
        project_dir: row["cwd"].as_str().map(str::to_string),
    }
}

/// 一份会话记录的标题（同历史清单那一格，[`super::search_rules::session_label`]，不并注解）。
pub(crate) fn session_title_of(p: &Path) -> String {
    let sid = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    super::search_rules::session_label(&label_facts_of(p)).0
}

/// [`analyze_session`] 过一层 [`SESSION_META`]：没变 ⇒ 上次那一行；变长且前面没被改写 ⇒ 只扫新增的；其余整份重扫。
fn analyze_session_cached(p: &Path) -> serde_json::Value {
    let prev = table().remove(p);
    // 读到过读不动的一行（之后的都不算）⇒ 不追加读：那一行之后的内容要整份重扫才对得上。
    let may_append = prev.as_ref().is_some_and(|e| !e.done.stopped);
    let mut bytes = 0u64;
    let Ok((step, seen)) = Tailed::step(prev.as_ref().map(|e| &e.read), p, may_append, &mut bytes)
    else {
        return analyze_session(p);
    };
    let changed = !matches!((&step, &prev), (Step::Same, Some(_)));
    let entry = match (step, prev) {
        (Step::Same, Some(mut e)) => {
            e.last_bytes = 0;
            e
        }
        (Step::Appended(mut new), Some(mut e)) => {
            // 读到一半读不动 ⇒ 与见证那一下读不动同一形：这一次现扫、不留。
            if !e.extend(p, seen.0, &mut new) {
                return analyze_session(p);
            }
            e.last_bytes = bytes + new.read();
            e
        }
        _ => ListingEntry::scan(p, seen.0),
    };
    let row = entry.row.clone();
    // 拿不到修改时刻 ⇒ 不留（下次认不出它变没变）。
    if seen.0.is_some() {
        let mut t = table();
        if t.len() >= SESSION_META_CAP {
            t.clear();
        }
        t.insert(p.to_path_buf(), entry);
        if changed {
            DIRTY.store(true, Ordering::Relaxed);
        }
    }
    row
}

// **围栏住 `common/fence.rs`**〔审计 F 🔴-6〕：这里原来是具名围栏
// `fence_under_projects` 的本体（「全文件唯一的一处 `canonicalize` + 前缀校验」，audit-0805 08-06 定框 E3 从
// `list_sessions` 的内联副本收成一份）。E3 只收到了**本文件**，`search_query` 里还有一份内联的（头注逐字「复刻 history_query」）
// ⇒ 判定本体（解开根 · 解开目标 · 前缀比）搬去 observe 内部唯一的家 `common/fence.rs::Fence`；这里只剩「以 `projects/` 为根」
//   那一行（根是哪一个属 Claude 的目录布局，留在认得它的这一侧），三条按路径读的路照旧调它，报错原话逐字不变。
// LOC1b 在这里把本体提成了「根是参数」的一形（为各家合成历史面给的记录根）—— 那一形就是
// `common/fence.rs::Fence::at(根)?.admit(候选)`（报错取根目录名，与 LOC1b 那一版逐字同形），下面 [`validate_session_path_among`] 改调它。

/// `<agent_home>/projects/` 这道围栏放行一个候选路径（本体在 [`Fence`]；`candidate` 相对按根拼、绝对直用）。
pub(crate) fn fence_under_projects(
    agent_home: &Path,
    candidate: &Path,
) -> Result<std::path::PathBuf, String> {
    Fence::at(&records_root(agent_home).ok_or(NO_TREE)?)?.admit(candidate)
}

/// 这台记录树的根（记录树那一家的布局，注册表 `RecordFace.tree`）。没有记录树那一家 ⇒ `None`。
pub(crate) fn records_root(agent_home: &Path) -> Option<PathBuf> {
    crate::agents::records_root(agent_home)
}

/// 注册表里没有记录树那一家时，按路径读的那几条拒的那一句。
pub(crate) const NO_TREE: &str = "no agent here keeps a record tree";

/// 帧面那几条要的「这台的 agent 家目录」—— 与 `main.rs` 那一句**同一个出处**。
///
/// 问注册表里后端盯着的那一家（`agents::home_at`），照进程环境解。
pub(crate) fn agent_home() -> std::path::PathBuf {
    crate::agents::home_at(None, true)
}

pub(crate) fn created_ms_or_mtime(p: &Path) -> i64 {
    let meta = match Ro::metadata(p) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    let t = meta.created().or_else(|_| meta.modified());
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 单个会话的元数据提取（整文件扫描，跑在那台机器的 CPU 上）：
/// - messageCountApprox = 非空行数
/// - firstUserExcerpt = 首条人说的话（适配层判「谁说的」，经注册表 `agents::human_speech`）的前 120 字符
///   （换行折成空格、超了加 `…`）
/// - aiTitle = 最后一条标题记录（`ai-title` 的 `aiTitle` 与 CC v2.1.x 起的 `custom-title` 的 `customTitle`，取最新）
/// - cwd = 会话的项目目录（适配层 `RecordFace.project_dir`：只读开头，与活 tab 的标题同一个函数）
/// - startedAtMs = 首条 user / assistant 记录的 `timestamp`；一条都解析不出 ⇒ 文件建立时刻（拿不到 ⇒ 修改时刻）
/// - forkedFromSessionId / forkedFromMessageUuid = 首条带 `forkedFrom` 的 user / assistant 记录（`/branch` 分叉来的）
///
pub(crate) fn analyze_session(p: &Path) -> serde_json::Value {
    // 〔audit-0805 F07 / 报告 B-6 第 5 环〕**流式**（[`ListingEntry::scan`] 逐行读）。原来是 `read_to_string(p)` 整读：
    // 会话清单对该项目**每个** jsonl 都调它一次，43 个项目 / 2.4 GB 的机器上
    // 一次列表就是把 2.4 GB 读进内存再逐行解析。扫描是必须的（要数行、要判 bg），**但不必先整份进内存**。
    let mtime = Ro::metadata(p).and_then(|m| m.modified()).ok();
    ListingEntry::scan(p, mtime).row
}

/// **「这条记录说明本会话是从哪个会话分叉来的」的唯一住址**：
/// `user` / `assistant` 记录上的 `forkedFrom`（两个键都得是串）⇒ (源会话 id, 分叉处的消息 uuid)。
///
/// 两个读者调同一个函数：历史会话行（[`analyze_session`] 的 `forkedFromSessionId`）与活 tab 的会话事实
/// （`recognize/facts.rs` 的 `forkedFrom`）—— 搬之前活 tab 那一份住前端、口径更松（任何记录、只看 `sessionId`），
/// 同一个会话会在历史树与 tab 栏上一个认是分叉、一个不认。
pub(crate) fn fork_origin(v: &serde_json::Value) -> Option<(String, String)> {
    match v.get("type").and_then(|t| t.as_str()) {
        Some("user") | Some("assistant") => forked_from(v),
        _ => None,
    }
}

/// 一条记录的 `forkedFrom`（`/branch` 分叉出来的会话，每条复制过来的记录都带着）→ (源会话 id, 分叉处的消息 uuid)。
fn forked_from(v: &serde_json::Value) -> Option<(String, String)> {
    forked_pair(v.get("forkedFrom")?)
}

/// `forkedFrom` 那一格本身 → (源会话 id, 分叉处的消息 uuid)（两格都得是串）。
pub(crate) fn forked_pair(f: &serde_json::Value) -> Option<(String, String)> {
    Some((
        f.get("sessionId")?.as_str()?.to_string(),
        f.get("messageUuid")?.as_str()?.to_string(),
    ))
}

#[cfg(test)]
#[path = "../../../tests/backend/history/listing_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/history/listing_kr83_tests.rs"]
mod kr83_tests;
