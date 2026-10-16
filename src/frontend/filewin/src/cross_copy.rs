//! **复制到另一台机器** —— 那一问（机器 ＋ 落点目录）与「放到」那一块小目录选择器。
//!
//! 做的那一半住本机后端（`transfer-copy`：经本机中转 A → 本机暂存 → B 暂存 → B 提交，目录整棵逐个文件，逐项结局，
//! 半路失败或撤清两头暂存）；窗口发一条（[`super::batch`]：带票 · 同名问一次 · 断了带票再问 · 撤），一步都不拼。
//! 本机不在机器那一栏里：落到本机是「下载」。
//! ⚠ 「在两个窗口之间拖」没做：两个窗口是两个进程，egui 里没有跨进程拖放 —— 入口是右键「复制到另一台…」。

use std::sync::{Arc, Mutex};

use copy_core::copy_text;

/// 菜单上那一项的字。**唯一住址**。
pub static CROSS_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinCrossCopy.label.cross", &[]));

/// 本机后端的地址（⚠ 与 app 侧 `inbound_client::LOCAL_ORIGIN` 是同一个值的两份 —— 窗口够不到 app 侧，
/// 判据 `cross_copy_tests::the_local_origin_is_the_app_one` 读两侧源码钉相等）。
pub const LOCAL_ORIGIN: &str = "<local>";

/// 机器下拉里给人看的名字：本机写「本机」，别的照名字。
pub fn shown_machine(origin: &str) -> String {
    if origin == LOCAL_ORIGIN {
        copy_text("rsFilewinCrossCopy.prompt.localMachine", &[])
    } else {
        origin.to_string()
    }
}

/// 框里那一格（给人看的名字，或手填的名字）→ 发出去的机器地址：「本机」⇒ 本机后端的地址。
pub fn origin_of(shown: &str) -> String {
    let t = shown.trim();
    if t == copy_text("rsFilewinCrossCopy.prompt.localMachine", &[]) {
        LOCAL_ORIGIN.to_string()
    } else {
        t.to_string()
    }
}

/// 两台之间复制一组那一条（本机后端；monitor 补上两台的拨号请求）。
pub const CMD_CROSS: &str = <cmd_wire::transfer::TransferCopyAsk as cmd_wire::files::Call>::NAME;

/// 那一问（UI 线程自己的）：要复制的那几项 ＋ 用户正在填的机器名与目标目录。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossPrompt {
    /// 标题里那一格（一项 ⇒ 它的名字；多项 ⇒ `n 项`）。
    pub name: String,
    /// 各项的名字（按序号，同后端的键）。
    pub names: Vec<String>,
    pub srcs: Vec<super::source::RemotePath>,
    pub machine: String,
    pub dir: String,
}

// ═══════════════════════════════════════════════════════════════════════
// 「放到」那一块小目录选择器：那台的面包屑 ＋ 只列文件夹，双击进去，缺省那台的主目录，记住上次
// ═══════════════════════════════════════════════════════════════════════

/// 那一块选择器的共享落点（UI 线程读，tokio 那条写）。带代数：换了机器 / 换了目录，晚到的上一趟不许落进来。
#[derive(Clone, Default)]
pub struct DirPick {
    inner: Arc<Mutex<Pick>>,
}

/// 选择器此刻摆的是什么。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pick {
    pub gen: u64,
    /// 哪台（地址，不是给人看的名字）。
    pub machine: String,
    /// 在看那台的哪个目录（还没问到主目录 ⇒ 空）。
    pub path: String,
    /// 那个目录底下的文件夹（`None` ＝ 在读；`Err` ＝ 读不出那一句）。
    pub dirs: Option<Result<Vec<String>, String>>,
    /// 单击选中的那一个子文件夹（落点 ＝ 它；没选 ⇒ 落点 ＝ 在看的目录）。
    pub picked: Option<String>,
}

impl Pick {
    /// 落点目录（[`Self::picked`] 选了 ⇒ 它；否则在看的那个目录）。
    pub fn target(&self) -> String {
        match &self.picked {
            Some(n) => super::source::child_of(&self.path, n),
            None => self.path.clone(),
        }
    }
}

/// 上一次在每一台上选到的落点（进程内记着；同一扇窗再开那一问就停在那儿）。
static LAST_DIR: std::sync::LazyLock<Mutex<std::collections::HashMap<String, String>>> =
    std::sync::LazyLock::new(Default::default);

/// 记下这一台上选到的落点。
pub fn remember_dir(machine: &str, dir: &str) {
    LAST_DIR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(machine.to_string(), dir.to_string());
}

/// 这一台上次选到的落点（没有 ⇒ `None`，那就去问它的主目录）。
pub fn last_dir(machine: &str) -> Option<String> {
    LAST_DIR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(machine)
        .cloned()
}

impl DirPick {
    /// 此刻摆的那一份（判据与界面看同一个值）。
    pub fn shown(&self) -> Pick {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 单击选中一个子文件夹（再点一次 ＝ 不选）。
    pub fn pick(&self, name: &str) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.picked = if g.picked.as_deref() == Some(name) {
            None
        } else {
            Some(name.to_string())
        };
    }

    /// 去那台的某个目录看（`path` 空 ⇒ 先问那台的主目录）：代数 +1、清掉旧的、起一趟。
    pub fn go(
        &self,
        h: &tokio::runtime::Handle,
        line: &super::source::Line,
        machine: &str,
        path: &str,
        ctx: Option<egui::Context>,
    ) {
        let gen = {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            g.gen += 1;
            g.machine = machine.to_string();
            g.path = path.to_string();
            g.dirs = None;
            g.picked = None;
            g.gen
        };
        let (me, line, machine, path) = (
            self.clone(),
            line.clone(),
            machine.to_string(),
            path.to_string(),
        );
        h.spawn(async move {
            let to = comms_inward::chan::wire::Origin(machine.clone());
            let path = if path.is_empty() {
                match super::source::home_of(&line, &to, super::transfer::PROBE_BUDGET).await {
                    Ok(h) => h,
                    Err(e) => {
                        me.land(gen, None, Err(e), &ctx);
                        return;
                    }
                }
            } else {
                path
            };
            let got = super::source::list_via_backend(
                &line,
                &to,
                &path,
                super::source::LS_LIMIT,
                super::source::SortBy::Name,
            )
            .await
            .map(|(rows, _)| {
                rows.into_iter()
                    .filter(|r| r.opens_as_dir() && !r.lossy_name)
                    .map(|r| r.name.clone())
                    .collect::<Vec<_>>()
            });
            me.land(gen, Some(path), got, &ctx);
        });
    }

    /// 在那台在看的目录里新建一个文件夹（名字缺省「新建文件夹」），建好就选中它。
    pub fn mkdir(
        &self,
        h: &tokio::runtime::Handle,
        line: &super::source::Line,
        ctx: Option<egui::Context>,
    ) {
        let now = self.shown();
        if now.path.is_empty() {
            return;
        }
        let name = copy_text("rsFilewinWriteops.inline.newDir", &[]);
        let (me, line) = (self.clone(), line.clone());
        h.spawn(async move {
            let to = comms_inward::chan::wire::Origin(now.machine.clone());
            let op = super::writeops::WriteOp::Mkdir {
                dir: now.path.clone(),
                name: name.clone(),
            };
            let made = super::writeops::apply_remote_coded(&line, &to, &op, None).await;
            let got = super::source::list_via_backend(
                &line,
                &to,
                &now.path,
                super::source::LS_LIMIT,
                super::source::SortBy::Name,
            )
            .await
            .map(|(rows, _)| {
                rows.into_iter()
                    .filter(|r| r.opens_as_dir() && !r.lossy_name)
                    .map(|r| r.name.clone())
                    .collect::<Vec<_>>()
            });
            me.land(now.gen, Some(now.path.clone()), got, &ctx);
            if made.is_ok() {
                me.inner.lock().unwrap_or_else(|e| e.into_inner()).picked = Some(name);
            }
        });
    }

    fn land(
        &self,
        gen: u64,
        path: Option<String>,
        got: Result<Vec<String>, String>,
        ctx: &Option<egui::Context>,
    ) {
        {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if g.gen != gen {
                return;
            }
            if let Some(p) = path {
                g.path = p;
            }
            g.dirs = Some(got);
        }
        if let Some(c) = ctx {
            c.request_repaint();
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/cross_copy_tests.rs"]
mod tests;
