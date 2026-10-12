//! `changed {plan}`：盯认过的工作区（计划仓 `.planned-build/` 整棵 ＋ 工作区根上那份 `.env` ＋ 各片仓库里声明过的文件，见 [`Spots`]），
//! 有动静就重跑一次 dump，**输出摘要或要你看的数变了**才推一帧 `{key: 工作区, rev, body: {needs}}`；界面收到再来 `plan-read`。
//! 认可 / 撤销认可不动盘上的计划，由那条命令自己经 [`changes`] 推一帧新的数。
//!
//! 合并不靠定时器（后端零定时器）：一次重读期间来的事件全攒着，重读完一并吞掉、再读一次。
//! 重读本身要 0.1–1 秒，正好把 agent 一口气写的几份并成一两次；写到一半读到的那一刻，
//! 那一片 pb 回 `error` ⇒ 本子给上一次好的、摘要多半不变 ⇒ 不推。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 同时盯着的工作区上限（一个工作区一棵小目录树；再多说明认错了目录）。
pub(crate) const MAX_WATCHED: usize = 32;

/// 计划仓的目录名（工作区根底下）。
pub(crate) const PLAN_DIR: &str = ".planned-build";
/// 工作区根上 pb 记当前片与 auto 的那份文件。
pub(crate) const WORKSPACE_ENV: &str = ".env";

/// 一个工作区此刻的样子：输出摘要 · 要你看的数（不含问人那一种）。
pub(crate) type Seen = (String, u64);

/// 重读一次、回新的样子（读不成 ⇒ `None`，不推）。生产由帧面宿主给（真 pb ＋ 会话表 ＋ 认可记录），判据喂假的。
pub(crate) type Reread = Arc<dyn Fn(&Path) -> Option<(Seen, Spots)> + Send + Sync>;

/// 各片仓库里要盯的：`dirs` 是工作区根往下、到每份声明过的文件所在目录一路上的每一层（都只盯直接子项）；
/// `hits` 是那几份文件本身。算数的事件：落在 `hits` 上（删了 · 清空 · 写了）或落在 `dirs` 上（一层整个挪走 / 删掉）。
/// 不整棵盯仓库：编译产物 · 依赖目录天天大批地动，又不在对账里，盯了只会白白重读、白占内核的监听名额。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Spots {
    pub(crate) dirs: Vec<PathBuf>,
    pub(crate) hits: Vec<PathBuf>,
}

/// 从一份成品算出 [`Spots`]：每片的仓库照 [`super::owners::repo_dir`]（与文件窗口反查同一处），每格的文件声明相对它。
pub(crate) fn spots(doc: &serde_json::Value) -> Spots {
    use serde_json::Value;
    let mut out = Spots::default();
    let Some(ws) = doc.get("workspace").and_then(Value::as_str) else {
        return out;
    };
    let root = PathBuf::from(ws);
    for sl in doc
        .get("slices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(repo) = super::owners::repo_dir(ws, sl).map(PathBuf::from) else {
            continue;
        };
        let mut want = vec![repo.clone()];
        for c in sl
            .get("cells")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for f in c
                .get("files")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(p) = f
                    .get("path")
                    .and_then(Value::as_str)
                    .filter(|p| !p.is_empty())
                else {
                    continue;
                };
                let file = repo.join(p);
                if !out.hits.contains(&file) {
                    out.hits.push(file.clone());
                }
                if let Some(parent) = file.parent() {
                    want.push(parent.to_path_buf());
                }
            }
        }
        // 每一处往上补到工作区根（不含根：根本层另外盯着）。
        for w in want {
            let mut at = Some(w.as_path());
            while let Some(d) = at.filter(|d| *d != root && d.starts_with(&root)) {
                if !out.dirs.iter().any(|x| x == d) {
                    out.dirs.push(d.to_path_buf());
                }
                at = d.parent();
            }
        }
    }
    out.dirs.sort();
    out.hits.sort();
    out
}

/// 一帧 `changed {plan}`：`(工作区, 摘要, 要你看的数)`。
pub(crate) type Change = (String, String, u64);

/// 进程里那条「某个工作区的计划变了」的通道（流连接订它推 `changed {plan}`）。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<Change> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<Change>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<Change>(64).0)
}

/// 一份活着的监听，重读那条线程换名单时要拿到它（线程里只握弱引用：外面一丢，监听连线程一起走）。
pub(crate) type Held = Arc<Mutex<Option<crate::platform::watch_file::Watching>>>;

/// 盯着的那几个：工作区 ⇒ 它的监听器（活着就一直盯）。
static HELD: Mutex<Vec<(PathBuf, Held)>> = Mutex::new(Vec::new());

/// 这个事件要不要重读：计划仓里的任何东西 · 工作区根上那份 `.env` · 各片仓库里声明过的文件与通往它们的目录（[`Spots`]）。
pub(crate) fn concerns(ws: &Path, spots: &Spots, p: &Path) -> bool {
    p.starts_with(ws.join(PLAN_DIR))
        || p == ws.join(WORKSPACE_ENV)
        || spots.hits.iter().any(|h| h == p)
        || spots.dirs.iter().any(|d| d == p)
}

/// 此刻真要挂的那几层：工作区根本层 ＋ 计划仓整棵 ＋ [`Spots::dirs`] 里在的那几层（还没有的等它出现：上一层看得见它被建出来）。
fn dirs_of(ws: &Path, spots: &Spots) -> Vec<crate::platform::watch_file::Dir> {
    let mut dirs = vec![(ws.to_path_buf(), false)];
    let plan = ws.join(PLAN_DIR);
    if plan.is_dir() {
        dirs.push((plan, true));
    }
    dirs.extend(
        spots
            .dirs
            .iter()
            .filter(|d| d.is_dir())
            .map(|d| (d.clone(), false)),
    );
    dirs
}

/// 开始盯一个工作区（已在盯 ⇒ 什么都不做；满了 ⇒ 只出声）。`last` 是此刻的样子（之后变了才推）。
pub(crate) fn arm(ws: &Path, spots: Spots, last: Option<Seen>, reread: Reread) {
    let mut g = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if g.iter().any(|(p, _)| p == ws) {
        return;
    }
    if g.len() >= MAX_WATCHED {
        tracing::warn!(
            "[plan] 盯着的工作区已满 {MAX_WATCHED} 个，{} 的变化要重开计划页才看得见",
            ws.display()
        );
        return;
    }
    match watch(ws, spots, last, reread) {
        Ok(w) => g.push((ws.to_path_buf(), w)),
        Err(e) => tracing::warn!(
            "[plan] 盯不上 {}：{e}（计划变了要重开计划页才看得见）",
            ws.display()
        ),
    }
}

/// 挂监听（[`dirs_of`]，经 [`crate::platform::watch_file`]）、起那条重读线程。每次重读完照新的成品换名单
/// （`pb repo` 搬了家 · 新声明了文件 ⇒ 盯的跟着换）。返回的那一份活着就一直盯。
pub(crate) fn watch(
    ws: &Path,
    spots: Spots,
    last: Option<Seen>,
    reread: Reread,
) -> Result<Held, String> {
    let held: Held = Arc::new(Mutex::new(None));
    let slot = Arc::downgrade(&held);
    let dirs = dirs_of(ws, &spots);
    let now_spots = Arc::new(Mutex::new(spots));
    let filter_spots = Arc::clone(&now_spots);
    let root = ws.to_path_buf();
    let target = ws.to_path_buf();
    let mut last = last;
    let w = crate::platform::watch_file::watch(
        &dirs,
        move |p| {
            let sp = filter_spots.lock().unwrap_or_else(|e| e.into_inner());
            concerns(&root, &sp, p)
        },
        "plan-watch",
        move |_| {
            let Some((now, sp)) = reread(&target) else {
                return;
            };
            // 名单每趟都跟上（不只是成品变了时）：声明过的文件所在的那一层刚建出来，也要这一趟挂上。
            let dirs = dirs_of(&target, &sp);
            *now_spots.lock().unwrap_or_else(|e| e.into_inner()) = sp;
            if let Some(h) = slot.upgrade() {
                if let Some(live) = h.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                    for e in live.rearm(&dirs) {
                        tracing::warn!("[plan] 盯不上 {e}（那里的文件变了要重开计划页才看得见）");
                    }
                }
            }
            if last.as_ref() != Some(&now) {
                last = Some(now.clone());
                let _ = changes().send((target.to_string_lossy().to_string(), now.0, now.1));
            }
        },
    )?;
    *held.lock().unwrap_or_else(|e| e.into_inner()) = Some(w);
    Ok(held)
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/watch_tests.rs"]
mod tests;
