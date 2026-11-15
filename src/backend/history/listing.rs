//! **会话清单那一族**：项目清单 · 平铺会话清单（连那份「没变不重扫、变长只扫尾巴」的缓存）·
//! 后台热缓存 · 叫一条会话什么（[`label_facts_of`] · [`session_title_of`]）· 「这一行能做什么」（[`can_of`]）·
//! 藏起来的那个工作目录（[`hidden_cwd`]）· 各家记录树那道围栏（[`fenced`]）。家由调用方交（`agents::local_homes`，每一家各一份）。
//!
//! 历史清单（`history_list`）· 资产目录（`assets::asset_catalog`）· 观测侧（记录级的读 · 全文搜索 · watcher）都来这里拿；
//! 这一族不引 observe（历史清单那个包排在观测面之下，不依赖它）。
//!
//! 安全：按名字给的项目目录不许含路径分隔符；路径一律经 [`crate::common::fence::Fence`] 放行（`canonicalize` 后前缀校验，防 `../` 穿越）。
//! 读记录只 read_dir / read；写只有清单缓存落盘那一份（经启动时装的 [`Own`]）。

use super::listing_scan::{SessionRow, SessionScan};
use crate::agents::AgentHome;
use crate::common::append_read::{Blocks, Step, Tailed};
use crate::common::fence::Fence;
use crate::platform::fs::{Own, OwnPath, Ro};
use crate::wire::WireLife;
pub(crate) use cmd_wire::listing::{Can, CanDelete, CanResume};
use copy_core::copy_text;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

/// **这条会话能做什么**（`can`）——界面按它画，不另判：
/// - `resume`：`yes` 能恢复 · `switch` 在跑（切过去，不再起第二份）· `bg` 分身会话（要接着聊得恢复主会话）· `no` 认不出是哪一家的（没有那一家可起）；
/// - `resumeWhy`：恢复不给时那一句（`bg` · `no`）；
/// - `accounts`：恢复时能不能选号（那一家有没有账号这一维）· `fork`：能不能从某一轮分叉；
/// - `delete`：`yes` · `live` 在跑（先结束它）· `unsure` 说不清在不在跑（`life` 判不了 ⇒ 确认框里多说一句）。
///
/// 历史清单的行与全文搜索的命中同一个函数。`kind` 缺 ＝ 认不出是哪一家的 ⇒ 恢复是 `no`（在跑的照样切过去）· 选号 · 分叉都不给（没有那一家可问）。
pub fn can_of(kind: Option<&str>, life: &WireLife, bg: bool) -> Can {
    let live = matches!(life, WireLife::Live { .. });
    let resume = if live {
        CanResume::Switch
    } else if kind.is_none() {
        CanResume::No
    } else if bg {
        CanResume::Bg
    } else {
        CanResume::Yes
    };
    let why = match resume {
        CanResume::No => Some(copy_text("rsAgents.family.unknown", &[])),
        CanResume::Bg => Some(copy_text("history.row.bgHint", &[])),
        CanResume::Yes | CanResume::Switch => None,
    };
    Can {
        resume,
        resume_why: why.map(cmd_wire::cells::Words),
        accounts: kind.and_then(crate::agents::account_env_of).is_some(),
        fork: kind
            .and_then(crate::agents::record_face)
            .is_some_and(|r| r.branch.is_some())
            && !bg,
        delete: match life {
            WireLife::Live { .. } => CanDelete::Live,
            WireLife::Unclear => CanDelete::Unsure,
            _ => CanDelete::Yes,
        },
    }
}

/// **这份会话记录在菜单里的归档那一项**：归的那一家声明了归档面 ⇒ 此刻是不是已归档照它判，那一项的字与没成那一句照文案表写
/// （`label` ＝ 这一行显示的标题）；没声明 ⇒ `None`（界面不出那一项）。历史清单的行经它填。
pub fn archive_item(p: &Path, label: &str) -> Option<cmd_wire::listing::RowArchive> {
    archive_item_among(crate::agents::REGISTRY, p, label)
}

/// [`archive_item`] 的可喂夹具那一半。
pub fn archive_item_among(
    registry: &[crate::agents::Adapter],
    p: &Path,
    label: &str,
) -> Option<cmd_wire::listing::RowArchive> {
    let archived = crate::agents::archive_state_among(registry, p)?;
    let (item, failed) = if archived {
        (
            copy_text("history.menu.unarchive", &[]),
            copy_text("history.unarchive.failed", &[("label", label)]),
        )
    } else {
        (
            copy_text("history.menu.archive", &[]),
            copy_text("history.archive.failed", &[("label", label)]),
        )
    };
    Some(cmd_wire::listing::RowArchive {
        archived,
        label: cmd_wire::cells::Words(item),
        failed: cmd_wire::cells::Words(failed),
    })
}

/// 项目清单里的一项：一个真实目录（[`group_by_cwd`] 分的那一组）与它最近一次活动。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// 会话的真实工作目录（读不出 ⇒ 归最近那个读得出的；一个都读不出 ⇒ 空串）。
    pub project_path: String,
    /// 这一组里最近改过的那份会话的修改时刻（毫秒）。
    pub last_activity_ms: i64,
}

/// 项目清单（资产目录 `asset_catalog` · 起会话框的「最近目录」读它）。
///
/// `homes` 里住记录树的每一家各列一遍，并成一份。
/// 哪一家的记录树根都不在 ⇒ `Ok(None)`（这台还没起过会话 —— 判定只在这一处）；怎么说由调用方定：资产目录当零个项目。
/// 记录树根 · 认会话文件 · 读项目目录都照生产注册表（这一处取它、往下传）。
pub fn list_projects(homes: impl crate::agents::IntoHomes) -> Result<Option<Vec<Project>>, String> {
    let registry = crate::agents::REGISTRY;
    let homes_v = homes.into_homes_among(registry);
    let homes: &[crate::agents::AgentHome] = &homes_v;
    let mut out: Option<Vec<Project>> = None;
    for (home, root) in tree_roots_among(registry, homes) {
        let entries = match Ro::read_dir(&root) {
            Ok(it) => it,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(unreadable_dir(&root, &e)),
        };
        let got = out.get_or_insert_with(Vec::new);
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() {
                // 空目录（全删过/只剩本机后端）不展示；一个记录目录里会话的真实目录不止一个 ⇒ 一组一项。
                got.extend(project_rows(registry, home, &dir));
            }
        }
    }
    Ok(out)
}

/// `homes` 里住记录树的每一家与它家下那棵树的根（注册序 · `homes` 的次序；树根照 `registry`）。
fn tree_roots_among<'a>(
    registry: &[crate::agents::Adapter],
    homes: &'a [AgentHome],
) -> Vec<(&'a AgentHome, PathBuf)> {
    homes
        .iter()
        .filter_map(|h| {
            crate::agents::records_root_among(registry, h.kind, &h.home).map(|r| (h, r))
        })
        .collect()
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
fn project_rows(registry: &[crate::agents::Adapter], home: &AgentHome, dir: &Path) -> Vec<Project> {
    project_rows_hiding(registry, home, dir, &hidden_cwd)
}

/// 这台家里那个不出会话的工作目录（`~/.cc-monitor/autostart/`，名字住 [`crate::common::own_dir::HIDDEN_WORK_DIR`]）：
/// 在那里跑的 `claude`（例如替号开额度窗口的那一句、`quota-probe` 报用量的那一次）历史页与会话列表都不出。观测侧只按它藏。
const HIDDEN_DIR: &str = crate::common::own_dir::HIDDEN_WORK_DIR;

/// 工作目录是 [`HIDDEN_DIR`] 的会话不出。**判据只在这一处**（项目清单按它摘组、观测侧按 pidfile 的目录不宣告）。
pub fn hidden_cwd(cwd: &str) -> bool {
    crate::platform::paths::data_home().is_some_and(|home| hidden_cwd_in(&home, cwd))
}

/// [`hidden_cwd`] 的本体：`data_home` 由调用方给（判据喂夹具）。记录里的目录是那一家解析过链接的那一形 ⇒ 两形都认
/// （解开链接经 observe 唯一那道路径解析 [`crate::common::fence::Fence`]）。
pub(crate) fn hidden_cwd_in(data_home: &Path, cwd: &str) -> bool {
    let dir = data_home.join(HIDDEN_DIR);
    let cwd = Path::new(cwd);
    cwd == dir || crate::common::fence::Fence::at(&dir).is_ok_and(|f| cwd == f.root())
}

/// [`project_rows`] 的本体：`hide(目录)` ⇒ 那一组不出；认会话文件 · 读项目目录照 `registry`。
fn project_rows_hiding(
    registry: &[crate::agents::Adapter],
    home: &AgentHome,
    dir: &Path,
    hide: &dyn Fn(&str) -> bool,
) -> Vec<Project> {
    // 目录只读开头（`project_dir_among`，不整份扫）：与会话清单整行那一格（扫整份时取到的第一个）只在
    // 写着目录的那一行落在开头上限之后时不同，那种会话在这里归最近那一组（[`group_by_cwd`]）。
    let mut sessions: Vec<(Option<String>, i64)> = Vec::new();
    if let Ok(files) = Ro::read_dir(dir) {
        for f in files.flatten() {
            let p = f.path();
            if p.is_file() && crate::agents::is_session_file_among(registry, home.kind, &p) {
                sessions.push((
                    crate::agents::project_dir_among(registry, &p),
                    Ro::mtime_ms(&p),
                ));
            }
        }
    }
    let mut groups: std::collections::BTreeMap<String, i64> = Default::default();
    for ((_, mtime), path) in sessions.iter().zip(group_by_cwd(&sessions)) {
        let last = groups.entry(path).or_default();
        *last = (*last).max(*mtime);
    }
    groups
        .into_iter()
        .filter(|(project_path, _)| !hide(project_path))
        .map(|(project_path, last_activity_ms)| Project {
            project_path,
            last_activity_ms,
        })
        .collect()
}

/// 一个项目目录的会话清单（每份会话一行）。`only` ＝ 只要这一个会话的整行（按 sid 问清单那一条）：别的会话只出分组要的那几格
/// （sid · 读出的目录 · 修改时刻 —— 都不用扫整份），整行只扫叫这个名字的那一份。
/// 项目目录在 `homes` 里哪一家的记录树下：头一家围栏放行的那一家（目录名各家不相交时就是它那一家）。
/// 记录树根 · 认会话文件照 `registry`（生产传 [`crate::agents::REGISTRY`]）。
pub(crate) fn list_sessions_among(
    registry: &[crate::agents::Adapter],
    homes: &[AgentHome],
    project_dir: &str,
    only: Option<&str>,
) -> Result<Vec<SessionRow>, String> {
    // project_dir 是目录名而非路径：拒绝任何分隔符 / 上跳
    if project_dir.contains('/') || project_dir.contains('\\') || project_dir.contains("..") {
        return Err(format!("invalid project dir name: {project_dir}"));
    }
    // 与 read_session 对齐（也兑现本文件头部"canonicalize 后前缀校验"的承诺）：名字
    // 合法但 projects/ 下若有指向外部的 symlink 目录，read_dir 会跟随逃逸出 projects/
    // ——canonicalize 解析 symlink 后做前缀校验挡住。
    // **改调共享围栏**（E3）：此前这里是一份内联副本，
    // 注释写着「与 `read_session` 对齐」—— 靠手工对齐的两份迟早会漂。
    let (home, dir) = fenced_among(registry, homes, Path::new(project_dir))?;
    let entries = Ro::read_dir(&dir).map_err(|e| unreadable_dir(&dir, &e))?;
    let mut rows = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() || !crate::agents::is_session_file_among(registry, home.kind, &p) {
            continue;
        }
        let own = crate::agents::sid_of(&p).unwrap_or_default();
        rows.push(match only {
            Some(sid) if own != sid => SessionRow {
                session_id: own,
                cwd: crate::agents::project_dir_among(registry, &p),
                updated_at_ms: Ro::mtime_ms(&p),
                ..SessionRow::default()
            },
            _ => analyze_session_cached(&p),
        });
    }
    Ok(rows)
}

/// 平铺清单的每个记录目录：`(目录名, 那一目录的会话行 | 读不了 ⇒ [`Unread`])`。
pub(crate) type DirSessions = Vec<(String, Result<Vec<SessionRow>, Unread>)>;

/// 读不了的一个记录目录：扫它时手里是哪一家的记录根（那一家，不事后照目录名另认）· 为什么读不了。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unread {
    pub kind: &'static str,
    pub why: String,
}

/// 平铺清单（`history-list`）那一份：这台**每个**记录目录 ⇒ [`DirSessions`]。
/// 会话行同 [`list_sessions_among`]，`cwd` 换成按 [`group_by_cwd`] 归的那一组 ——
/// 与项目清单同一个分组（记录目录名会撞，按真实目录分）；归到藏起来的那个目录（[`hidden_cwd`]）的不出。
/// 记录树根不在 ⇒ `Ok(None)`（这台还没起过会话，同 [`list_projects`]）。
pub(crate) fn sessions_by_dir(
    homes: impl crate::agents::IntoHomes,
) -> Result<Option<DirSessions>, String> {
    let homes_v = crate::agents::IntoHomes::into_homes(homes);
    sessions_by_dir_among(crate::agents::REGISTRY, &homes_v, None)
}

/// [`sessions_by_dir`] 的本体（注册表可喂）；`only` 见 [`list_sessions_among`]（别的会话那几行只够分组用，调用方按 sid 滤掉）。
/// `homes` 里住记录树的每一家的目录一起交给几条线程扫，并成一份照目录名排（同名的照家的先后）；
/// 读不了的目录带上扫它的那一家（[`Unread`]）。
pub(crate) fn sessions_by_dir_among(
    registry: &[crate::agents::Adapter],
    homes: &[AgentHome],
    only: Option<&str>,
) -> Result<Option<DirSessions>, String> {
    let mut dirs: Vec<(AgentHome, String)> = Vec::new();
    let mut any = false;
    for (home, root) in tree_roots_among(registry, homes) {
        let entries = match Ro::read_dir(&root) {
            Ok(it) => it,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(unreadable_dir(&root, &e)),
        };
        any = true;
        dirs.extend(
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| (home.clone(), e.file_name().to_string_lossy().into_owned())),
        );
    }
    if !any {
        return Ok(None);
    }
    // 〔perfC〕按目录分给几条线程扫（冷的时候是整台每份会话从头扫一遍，单线程要几秒）；结果最后按目录名排，与单线程逐字相同。
    let mut out: Vec<_> = crate::common::par::par_in_order(dirs, |(home, dir_name)| {
        rows_of_dir(registry, &home, &dir_name, only).map(|got| (dir_name, got))
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
    registry: &[crate::agents::Adapter],
    home: &AgentHome,
    dir_name: &str,
    only: Option<&str>,
) -> Option<Result<Vec<SessionRow>, Unread>> {
    let rows = list_sessions_among(registry, std::slice::from_ref(home), dir_name, only);
    let rows = rows.map_err(|why| Unread {
        kind: home.kind,
        why,
    });
    let rows = rows.map(|mut rows| {
        let keys: Vec<(Option<String>, i64)> = rows
            .iter()
            .map(|r| (r.cwd.clone(), r.updated_at_ms))
            .collect();
        for (r, cwd) in rows.iter_mut().zip(group_by_cwd(&keys)) {
            r.cwd = Some(cwd);
        }
        rows.retain(|r| !hidden_cwd(r.cwd.as_deref().unwrap_or_default()));
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

/// 记一笔：清单扫描从 `p` 读了 `n` 字节（只在测试构建里记，账住判据那一侧，按记录分开记）。
fn tally_read(p: &Path, n: u64) {
    #[cfg(test)]
    tests::tally_read(p, n);
    let _ = (p, n);
}

/// 盘上那一份的格式：形状或读法变了就在表里（`LISTING_CACHE_FILE`）加一（对不上 ⇒ 当没有，照记录重扫、覆盖）。
const STORE_VERSION: u32 = STORE_DOC.version();

/// 启动时装一次（拿启动时造的写句柄）：之后重启照盘上那一份接着认 —— 没变的记录不读、变长的只读追加的那一截。
pub fn install(own: &Own) {
    if STORE.set(own.clone()).is_err() {
        tracing::debug!("listing cache home already installed; keeping the first one");
    }
    // 远端那几台的清单副本落在同一个数据目录底下（`remote_listing`）。
    super::remote_listing::install(own);
}

/// 盘上一份：版本（顶层 `version`，流式读写不经读写口的 `Value`，自己核）＋ 每份记录读到哪了、累计的那几格、成品行。
#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    version: u32,
    rows: Vec<SavedRow>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedRow {
    path: PathBuf,
    read: Tailed,
    done: SessionScan,
    tail: SessionScan,
    row: SessionRow,
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

/// 落盘那一份：缓存（读不出来 · 版本对不上 ⇒ 当没有，照记录重扫；落点 · 版本 · 读上限照表）。
const STORE_DOC: crate::common::own_state::Doc<Saved> =
    crate::common::own_state::Doc::of(&relay_route_core::LISTING_CACHE_FILE);

/// 落盘那一份的（目录, 文件）。
fn store_at(own: &Own) -> Option<(OwnPath, OwnPath)> {
    let rel = STORE_DOC.rel.strip_prefix(".cc-monitor/")?;
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
    let parse = |r: &mut dyn std::io::Read| {
        serde_json::from_reader(r).map_err(|e| crate::common::said::Said::from(e.to_string()))
    };
    let saved: Saved = match STORE_DOC.read_stream(file.as_path(), parse) {
        crate::common::own_state::Read::Present(s) => s,
        _ => return,
    };
    if saved.version != STORE_VERSION {
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
        version: STORE_VERSION,
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
    row: SessionRow,
    /// 上一次带到这一问时真读了多少字节（判据看它：变长只读尾巴）。
    last_bytes: u64,
}

impl ListingEntry {
    /// 从头逐行流式扫一份（不整份进内存）：完整行进 `done`、交给账本；末尾没写完的那截进 `tail`。
    /// `kind` ＝ 照哪一家的读法扫（缓存里的那几份照路径认的那一家；按 sid 找到的照找到它的那一家，[`row_as`]）。
    fn scan(p: &Path, kind: Option<&str>, mtime: Option<std::time::SystemTime>) -> Self {
        use std::io::BufRead;
        let mut e = Self {
            read: Tailed::default(),
            done: SessionScan::default(),
            tail: SessionScan::default(),
            row: SessionRow::default(),
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
                    e.done.absorb_bytes(kind, &line);
                    e.read.took(&line);
                } else {
                    e.tail.absorb_bytes(kind, &line);
                }
            }
        }
        e.read.saw((mtime, end));
        e.last_bytes = end;
        tally_read(p, end);
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
        let kind = crate::agents::record_kind_of(p);
        let base = self.read.consumed();
        loop {
            match new.next() {
                Ok(Some(whole)) => {
                    for line in whole.split_inclusive(|&b| b == b'\n') {
                        self.done.absorb_bytes(kind, line);
                    }
                    self.read.took(&whole);
                }
                Ok(None) => break,
                Err(_) => return false,
            }
        }
        self.read.saw((mtime, base + new.read()));
        self.tail = SessionScan::default();
        self.tail.absorb_bytes(kind, &new.rest());
        self.last_bytes = new.read();
        self.row = self.done.joined(&self.tail).row(p);
        true
    }
}

/// 后端一起来就后台把清单缓存热好（一条一次性线程，降到低优先级；不是定时器，扫完就退）：
/// 第一次打开历史页 / 按 sid 打开查看窗时不必再等整台从头扫一遍。远端流后端与本机常驻后端同一个调用点（`stream/entry.rs` 的 `run`）。
pub fn warm_listing_in_background(homes: Vec<AgentHome>) {
    let spawned = std::thread::Builder::new()
        .name("listing-warm".into())
        .spawn(move || {
            let lowered = crate::platform::proc::lower_this_thread();
            let n = warm_listing(&homes);
            tracing::info!("历史清单缓存：后台热好 {n} 份（低优先级：{lowered}）");
        });
    if let Err(e) = spawned {
        tracing::warn!("历史清单缓存：后台热缓存的线程起不来（{e}），第一次问清单会现扫");
    }
}

/// [`warm_listing_in_background`] 的本体（判据直接调它）：整台扫一遍清单，回缓存里有几份。
pub(crate) fn warm_listing(homes: impl crate::agents::IntoHomes) -> usize {
    let homes_v = crate::agents::IntoHomes::into_homes(homes);
    let homes: &[crate::agents::AgentHome] = &homes_v;
    // 读不了记录树 ⇒ 这一趟热不了，说一句；第一次问清单时照样现扫、照样把那一句交给界面。
    if let Err(e) = sessions_by_dir(homes) {
        tracing::warn!("历史清单缓存：后台热缓存读不了记录树（{e}）");
    }
    table().len()
}

/// 一份会话记录里叫它什么的那几样（标题 · 第一句 · 项目目录；注解那一格由调用方并），走清单缓存（没变不重扫、变长只扫尾巴）。
pub fn label_facts_of(p: &Path) -> super::search_rules::LabelFacts {
    label_facts_among(crate::agents::REGISTRY, p)
}

/// [`label_facts_of`] 的可喂夹具那一半（注册表是参数）：照路径认那一家；认不出 ⇒ 只数行的那一行（说不清）。
pub(crate) fn label_facts_among(
    registry: &[crate::agents::Adapter],
    p: &Path,
) -> super::search_rules::LabelFacts {
    match crate::agents::record_kind_among(registry, p) {
        Some(kind) => facts_as(registry, kind, p, || analyze_session_cached(p)),
        None => row_facts(analyze_session_cached(p)),
    }
}

/// 已知是哪一家的（按 sid 找到它的那一家，[`crate::agents::SessionAt::kind`]）：叫它的那几样照那一家取、不照路径另认；
/// 注册表里没有这一家 ⇒ 说不清（那几样都空，不退到注册序头一家去读）。
fn label_facts_as_among(
    registry: &[crate::agents::Adapter],
    kind: &'static str,
    p: &Path,
) -> super::search_rules::LabelFacts {
    facts_as(registry, kind, p, || row_as(p, kind))
}

/// 照 `kind` 那一家取叫它的那几样：有合成历史面 ⇒ 它的摘录法与项目目录；住记录树 ⇒ `row` 扫出的那一行。
fn facts_as(
    registry: &[crate::agents::Adapter],
    kind: &str,
    p: &Path,
    row: impl FnOnce() -> SessionRow,
) -> super::search_rules::LabelFacts {
    let Some(adapter) = crate::agents::adapter_among(registry, kind) else {
        return super::search_rules::LabelFacts::default();
    };
    // 有合成历史面的那一家（记录格式不是记录树那一家的）：第一句照它自己的读法（与历史清单那一行同一个摘录）；它没有标题记录。
    if let Some(h) = adapter.history {
        return super::search_rules::LabelFacts {
            first: super::search_rules::excerpt_of(&(h.excerpt)(p)),
            project_dir: crate::agents::project_dir_as(registry, kind, p),
            ..Default::default()
        };
    }
    row_facts(row())
}

/// 清单那一行里叫它的那几样。
fn row_facts(row: SessionRow) -> super::search_rules::LabelFacts {
    super::search_rules::LabelFacts {
        title: row.ai_title,
        first: row.first_user_excerpt,
        project_dir: row.cwd,
        forked: row.forked_from_session_id.is_some(),
        background: row.is_bg,
        ..Default::default()
    }
}

/// 一份会话记录叫什么（同历史清单那一格，[`super::search_rules::session_label`]）：`custom` ＝ 这台注解里给它改的名。
pub fn session_title_of(p: &Path, custom: Option<String>) -> String {
    super::search_rules::session_label(&super::search_rules::LabelFacts {
        custom,
        ..label_facts_of(p)
    })
    .0
}

/// [`session_title_of`] 的已知那一家一形（[`label_facts_as_among`]：照 `kind` 那一家取，不照路径另认）。
pub fn session_title_as(
    registry: &[crate::agents::Adapter],
    kind: &'static str,
    p: &Path,
    custom: Option<String>,
) -> String {
    super::search_rules::session_label(&super::search_rules::LabelFacts {
        custom,
        ..label_facts_as_among(registry, kind, p)
    })
    .0
}

/// 清单缓存里正在扫的那几份：同一份后到的等先到的扫完（[`Scanning::begin`]），再照它留下的那一行起步。
static SCANNING: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
static SCANNED: std::sync::Condvar = std::sync::Condvar::new();

/// 这一份正由本线程扫着（放手时摘掉、叫醒等它的）。
struct Scanning<'a>(&'a Path);

impl<'a> Scanning<'a> {
    /// 同一份正在扫 ⇒ 等它扫完（阻塞等结果，不定时醒），再记成本线程在扫。
    fn begin(p: &'a Path) -> Self {
        let mut g = SCANNING.lock().unwrap_or_else(|e| e.into_inner());
        while g.iter().any(|q| q == p) {
            g = SCANNED.wait(g).unwrap_or_else(|e| e.into_inner());
        }
        g.push(p.to_path_buf());
        Scanning(p)
    }
}

impl Drop for Scanning<'_> {
    fn drop(&mut self) {
        let mut g = SCANNING.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|q| q != self.0);
        SCANNED.notify_all();
    }
}

/// [`analyze_session`] 过一层 [`SESSION_META`]：没变 ⇒ 上次那一行；变长且前面没被改写 ⇒ 只扫新增的；其余整份重扫。
/// 同一份正在扫 ⇒ 等它扫完，照它留下的那一行起步（不并发扫两遍）。
fn analyze_session_cached(p: &Path) -> SessionRow {
    let _scanning = Scanning::begin(p);
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
            tally_read(p, e.last_bytes);
            e
        }
        _ => ListingEntry::scan(p, crate::agents::record_kind_of(p), seen.0),
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
// [`fenced`] 那道围栏（「全文件唯一的一处 `canonicalize` + 前缀校验」，audit-0805 08-06 定框 E3 从
// `list_sessions` 的内联副本收成一份）。E3 只收到了**本文件**，`search_query` 里还有一份内联的（头注逐字「复刻 history_query」）
// ⇒ 判定本体（解开根 · 解开目标 · 前缀比）搬去 observe 内部唯一的家 `common/fence.rs::Fence`；这里只剩「以 `projects/` 为根」
//   那一行（根是哪一个属 Claude 的目录布局，留在认得它的这一侧），三条按路径读的路照旧调它，报错原话逐字不变。
// LOC1b 在这里把本体提成了「根是参数」的一形（为各家合成历史面给的记录根）—— 那一形就是
// `common/fence.rs::Fence::at(根)?.admit(候选)`（报错取根目录名，与 LOC1b 那一版逐字同形），下面 [`validate_session_path_among`] 改调它。

/// 记录树那道围栏放行一个候选路径（本体在 [`Fence`]；`candidate` 相对按根拼、绝对直用）：`homes` 里住记录树的每一家，
/// 头一家放行的就是它（连那一家一起交回）；都不放行 ⇒ 头一家那一句；哪一家都不住记录树 ⇒ [`NO_TREE`]。
pub fn fenced<'a>(
    homes: &'a [AgentHome],
    candidate: &Path,
) -> Result<(&'a AgentHome, std::path::PathBuf), String> {
    fenced_among(crate::agents::REGISTRY, homes, candidate)
}

/// [`fenced`] 的可喂注册表那一半。
fn fenced_among<'a>(
    registry: &[crate::agents::Adapter],
    homes: &'a [AgentHome],
    candidate: &Path,
) -> Result<(&'a AgentHome, std::path::PathBuf), String> {
    let mut first_err = None;
    for (home, root) in tree_roots_among(registry, homes) {
        match Fence::at(&root).and_then(|f| f.admit(candidate)) {
            Ok(p) => return Ok((home, p)),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err.unwrap_or_else(|| NO_TREE.to_string()))
}

/// 注册表里没有记录树那一家时，按路径读的那几条拒的那一句。
pub const NO_TREE: &str = "no agent here keeps a record tree";

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
/// - aiTitle = 最后一条算数的标题记录（`search_rules::names_session`：只认 agent 起的，人在 agent 里改的名不算）
/// - cwd = 第一条写着工作目录的记录说的那个（扫整份时一并取到，适配层翻好的那一行；不另读开头）
/// - startedAtMs = 首条 user / assistant 记录的 `timestamp`；一条都解析不出 ⇒ 文件建立时刻（拿不到 ⇒ 修改时刻）
/// - forkedFromSessionId / forkedFromMessageUuid = 首条带 `forkedFrom` 的 user / assistant 记录（`/branch` 分叉来的）
///
pub(crate) fn analyze_session(p: &Path) -> SessionRow {
    // 〔audit-0805 F07 / 报告 B-6 第 5 环〕**流式**（[`ListingEntry::scan`] 逐行读）。原来是 `read_to_string(p)` 整读：
    // 会话清单对该项目**每个** jsonl 都调它一次，43 个项目 / 2.4 GB 的机器上
    // 一次列表就是把 2.4 GB 读进内存再逐行解析。扫描是必须的（要数行、要判 bg），**但不必先整份进内存**。
    let mtime = Ro::metadata(p).and_then(|m| m.modified()).ok();
    ListingEntry::scan(p, crate::agents::record_kind_of(p), mtime).row
}

/// 照 `kind` 那一家的读法扫这一份：路径认的也是这一家 ⇒ 走清单缓存；不是（路径认不出 · 认成别家）⇒ 现扫、不进缓存
/// （缓存里那几份一律照路径认的那一家扫，变长只扫尾巴也照它）。
fn row_as(p: &Path, kind: &'static str) -> SessionRow {
    if crate::agents::record_kind_of(p) == Some(kind) {
        return analyze_session_cached(p);
    }
    let mtime = Ro::metadata(p).and_then(|m| m.modified()).ok();
    ListingEntry::scan(p, Some(kind), mtime).row
}

#[cfg(test)]
#[path = "../../../tests/backend/history/listing_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/history/listing_kr83_tests.rs"]
mod kr83_tests;
