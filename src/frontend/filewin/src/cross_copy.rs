//! **复制到另一台机器** —— 「跨机复制」· `§7` 第 9 条 Q2。
//!
//! 按通行做法：「保留『一窗一机』；在两个窗口之间拖 / 复制到另一台 ⇒ 一个任务『从 A 下到本机暂存 → 传到 B 的暂存 →
//! B 那台提交』（WinSCP / FileZilla 远端到远端的通行做法：经本机中转），一条进度、可撤、半路失败清暂存」。
//!
//! # 一个任务（[`run`]）
//!
//! 1. B 那台：问 `files-home`（连不上 / 没这台 ⇒ 当场说）；落点目录缺省 ＝ B 的 home；目标已在 ⇒ **问一次**盖不盖。
//! 2. 本机暂存：问本机后端（`<local>`）的 home，暂存区 `~/.cc-monitor/staging`（不在就经本机后端建、收 0700），落点 `<32hex>.part`。
//! 3. 从 A 下到本机暂存（`download::pull_one` —— 名字不是 UTF-8 走 `lossy_pull::pull_by_bytes`）。
//! 4. 传到 B（`transfer::upload_remote`：B 的暂存 → B 提交；B 若是非标准 SFTP 起始目录，照 Q5 自动改走块形）。
//! 5. 不论成败删掉本机暂存件（经本机后端 `files-delete`）；半路失败 / 撤 ⇒ 另把 B 那头开过单的暂存件删掉（`staged_keys`）。
//!
//! 一条进度：两腿各占一半（`(下了 ＋ 传了) / (2 × 总共)`）；「取消」同时撤两腿。
//! ⚠ 「在两个窗口之间拖」没做：两个窗口是两个进程，egui 里没有跨进程拖放 —— 入口是右键「复制到另一台…」，机器名照设置里那个名字填。

use std::sync::atomic::{AtomicU64, Ordering};
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

/// 暂存区（相对 home）。引契约那一份（`relay_route_core::STAGING_DIR_REL`，后端 `control/files_commit.rs::STAGING_DIR`
/// 与数据位置页引的同一个）；判据 `cross_copy_tests::the_staging_dir_is_the_backend_one` 钉两侧都引它。
pub const STAGING_DIR: &str = relay_route_core::STAGING_DIR_REL;

/// 暂存区自建时收成的权限位（与后端 `own_dir` 建自家目录同一个 0700）。
pub const STAGING_MODE: u32 = 0o700;

/// 那一问（UI 线程自己的）：要复制的那一行 ＋ 用户正在填的机器名与目标目录。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossPrompt {
    pub name: String,
    pub src: super::source::RemotePath,
    pub machine: String,
    pub dir: String,
}

/// 一趟的结局。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done {
        name: String,
        machine: String,
        path: String,
        bytes: u64,
    },
    /// 人答了「不盖」。
    Skipped { name: String, path: String },
    /// 没复制成：那一句 ＋ 复制详情（哪一步没成由那一步写）。
    Failed {
        name: String,
        why: super::source::Failed,
    },
}

/// 结局那一句。
pub fn outcome_text(o: &Outcome) -> String {
    match o {
        Outcome::Done {
            name,
            machine,
            path,
            bytes,
        } => copy_text(
            "rsFilewinCrossCopy.outcome.done",
            &[
                ("name", name),
                ("machine", machine),
                ("path", path),
                ("human", &copy_core::size_text(*bytes)),
            ],
        ),
        Outcome::Skipped { name, path } => copy_text(
            "rsFilewinCrossCopy.outcome.skipped",
            &[("name", name), ("path", path)],
        ),
        Outcome::Failed { name, why } => copy_text(
            "rsFilewinCrossCopy.outcome.failed",
            &[("name", name), ("why", &why.said)],
        ),
    }
}

/// 窗口上的样子：在复制哪一个 · 那一问（盖不盖）· 两腿的进度 · 上一趟的结局。**跨线程共享**。
#[derive(Clone, Default)]
pub struct CrossBoard {
    inner: Arc<Mutex<Desk>>,
    rounds: Arc<AtomicU64>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
    /// 第一腿（A ⇒ 本机暂存）。
    pub pull: super::download::DownloadBoard,
    /// 第二腿（本机暂存 ⇒ B）。
    pub push: super::transfer::DropBoard,
}

#[derive(Default)]
struct Desk {
    running: Option<String>,
    asking: Option<(String, tokio::sync::oneshot::Sender<bool>)>,
    last: Option<Outcome>,
}

impl CrossBoard {
    pub fn attach(&self, ctx: Option<egui::Context>) {
        *self.ctx.lock().unwrap() = ctx.clone();
        self.pull.attach(ctx.clone());
        self.push.attach(ctx);
    }

    fn poke(&self) {
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }

    pub fn running(&self) -> Option<String> {
        self.inner.lock().unwrap().running.clone()
    }

    /// 摆出「盖不盖」那一问；收端落地 ＝ 人答了（`true` ＝ 盖）。
    pub fn ask(&self, said: String) -> tokio::sync::oneshot::Receiver<bool> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.inner.lock().unwrap().asking = Some((said, tx));
        self.poke();
        rx
    }

    pub fn is_asking(&self) -> bool {
        self.inner.lock().unwrap().asking.is_some()
    }

    /// 人答了那一问。没在问 ⇒ `false`。
    pub fn settle(&self, overwrite: bool) -> bool {
        let Some((_, tx)) = self.inner.lock().unwrap().asking.take() else {
            return false;
        };
        tx.send(overwrite).ok();
        self.poke();
        true
    }

    /// 取消：两腿一起撤（下的那一腿停订、传的那一腿停订 ＋ 还没起的不起）。
    pub fn cancel(&self) {
        self.pull.cancels().request();
        self.push.cancels().request();
    }

    /// 一条进度：`(已走, 总共)`，两腿各占一半。
    pub fn seen(&self, name: &str) -> (u64, u64) {
        let (dg, dt) = self.pull.seen();
        let (ug, ut) = self.push.seen(name).unwrap_or((0, 0));
        let total = dt.max(ut);
        let up = if ut > 0 { ug } else { 0 };
        (dg.min(total) + up, total * 2)
    }

    fn begin(&self, name: &str) {
        self.inner.lock().unwrap().running = Some(name.to_string());
        self.pull.cancels().reset();
        self.push.cancels().reset();
        self.pull.begin(name);
        self.push.reset_target();
        self.poke();
    }

    pub fn finish(&self, o: Outcome) {
        {
            let mut d = self.inner.lock().unwrap();
            d.running = None;
            d.asking = None;
            d.last = Some(o);
        }
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    pub fn last(&self) -> Option<Outcome> {
        self.inner.lock().unwrap().last.clone()
    }

    /// 画盖不盖那一问（kit 的对话框：那一句 ＋［不复制］（焦点）［覆盖］（危险））。在复制那一行（进度 ＋ 停）与结局是「进度」表里的一行（`super::progress`）。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let asking = self
            .inner
            .lock()
            .unwrap()
            .asking
            .as_ref()
            .map(|(s, _)| s.clone());
        let Some(said) = asking else {
            return;
        };
        let hit = super::kit::dialog(
            ui.ctx(),
            "filewin-cross-overwrite",
            &said,
            |_| {},
            &[
                (
                    copy_text("rsFilewinCrossCopy.ask.skip", &[]),
                    super::kit::Btn::Plain,
                ),
                (
                    copy_text("rsFilewinCrossCopy.ask.overwrite", &[]),
                    super::kit::Btn::Danger,
                ),
            ],
            0,
            0,
        );
        if let Some(k) = hit {
            self.settle(k == 1);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 「放到」那一块小目录选择器（稿 13）：那台的面包屑 ＋ 只列文件夹，双击进去，缺省那台的主目录，记住上次
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
            Some(n) => super::writeops::join_remote(&self.path, n),
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
                match super::source::ask(
                    &line,
                    &to,
                    "files-home",
                    &serde_json::json!({}),
                    super::transfer::PROBE_BUDGET,
                )
                .await
                .and_then(|d| super::source::home_from_reply(&d))
                {
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
                path: super::writeops::join_remote(&now.path, &name),
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

/// 目标目录那一格 → 真落点目录：空 ⇒ B 的 home；不是 `/` 起头 ⇒ 拒（远端路径恒用 `/`）。
pub fn target_dir(typed: &str, home: &str) -> Result<String, String> {
    let t = typed.trim();
    if t.is_empty() {
        return Ok(home.to_string());
    }
    if !t.starts_with('/') {
        return Err(copy_text(
            "rsFilewinCrossCopy.dir.notAbsolute",
            &[("dir", t)],
        ));
    }
    let d = t.trim_end_matches('/');
    Ok(if d.is_empty() {
        "/".to_string()
    } else {
        d.to_string()
    })
}

/// 整趟（A 下到本机暂存 → 传到 B → B 提交），回结局。`ask` 是「目标已在，盖不盖」那一问。
pub async fn run<A, AFut>(
    line: &super::source::Line,
    from: &super::source::Origin,
    src: &super::source::RemotePath,
    name: &str,
    machine: &str,
    typed_dir: &str,
    board: &CrossBoard,
    ask: A,
) -> Outcome
where
    A: FnOnce(String) -> AFut,
    AFut: std::future::Future<Output = bool>,
{
    let failed = |why: super::source::Failed| Outcome::Failed {
        name: name.to_string(),
        why,
    };
    let to = comms_inward::chan::wire::Origin(machine.trim().to_string());
    // ① B 那台：home · 落点 · 目标已在就问一次。
    let bhome = match super::source::ask(
        line,
        &to,
        "files-home",
        &serde_json::json!({}),
        super::transfer::PROBE_BUDGET,
    )
    .await
    .and_then(|d| super::source::home_from_reply(&d))
    {
        Ok(h) => h,
        Err(e) => {
            return failed(super::source::Failed::from(copy_text(
                "rsFilewinCrossCopy.target.unreachable",
                &[("machine", machine), ("why", &e)],
            )))
        }
    };
    let dir = match target_dir(typed_dir, &bhome) {
        Ok(d) => d,
        Err(e) => return failed(e.into()),
    };
    let dest = format!("{}/{name}", dir.trim_end_matches('/'));
    let mut overwrite = false;
    if super::transfer::probe_remote(line, &to, &dest).await {
        let said = copy_text(
            "rsFilewinCrossCopy.ask.taken",
            &[("machine", machine), ("path", &dest)],
        );
        if !ask(said).await {
            return Outcome::Skipped {
                name: name.to_string(),
                path: dest,
            };
        }
        overwrite = true;
    }
    // 目标就是本机 ⇒ 没有第二腿：直接下到落点（有损名走按字节读回那一条）。
    if to.0 == LOCAL_ORIGIN {
        let got = if src.is_lossy() {
            super::lossy_pull::pull_by_bytes(
                line,
                from,
                &src.bytes(),
                serde_json::Value::String(dest.clone()),
                overwrite,
                &board.pull,
            )
            .await
            .map_err(super::source::Failed::from)
        } else {
            super::download::pull_one(line, from, &src.shown, &dest, &board.pull).await
        };
        return match got {
            Ok(()) => Outcome::Done {
                name: name.to_string(),
                machine: machine.trim().to_string(),
                path: dest,
                bytes: board.pull.seen().1,
            },
            Err(why) => failed(why),
        };
    }
    // ② 本机暂存（经本机后端：home · 暂存区 · 落点）。
    let local = comms_inward::chan::wire::Origin(LOCAL_ORIGIN.to_string());
    let lhome = match super::source::ask(
        line,
        &local,
        "files-home",
        &serde_json::json!({}),
        super::transfer::PROBE_BUDGET,
    )
    .await
    .and_then(|d| super::source::home_from_reply(&d))
    {
        Ok(h) => h,
        Err(e) => return failed(e.into()),
    };
    let own = format!("{}/.cc-monitor", lhome.trim_end_matches('/'));
    let staging = format!("{own}/staging");
    if super::source::ask(
        line,
        &local,
        "files-mkdir",
        &serde_json::json!({ "root": own, "rel": "staging" }),
        super::writeops::WRITE_BUDGET,
    )
    .await
    .is_ok()
    {
        if let Err(e) = super::source::ask(
            line,
            &local,
            "files-chmod",
            &serde_json::json!({ "root": own, "rel": "staging", "mode": STAGING_MODE }),
            super::writeops::WRITE_BUDGET,
        )
        .await
        {
            return failed(e.into());
        }
    }
    let key = uuid::Uuid::new_v4().simple().to_string();
    let kept = format!("{key}.part");
    let local_path = format!("{staging}/{kept}");
    // ③ A ⇒ 本机暂存。
    let pulled = if src.is_lossy() {
        super::lossy_pull::pull_by_bytes(
            line,
            from,
            &src.bytes(),
            serde_json::Value::String(local_path.clone()),
            false,
            &board.pull,
        )
        .await
        .map_err(super::source::Failed::from)
    } else {
        super::download::pull_one(line, from, &src.shown, &local_path, &board.pull).await
    };
    // ④ 本机暂存 ⇒ B（B 的暂存 → B 提交）。
    let pushed = match pulled {
        Ok(()) => {
            let p = super::transfer::Pending {
                local_path: local_path.clone(),
                remote_path: dest.clone(),
                name: name.to_string(),
                overwrite,
                remote_dir_raw: None,
            };
            super::transfer::upload_remote(line, &to, &p, &board.push).await
        }
        Err(e) => Err(e),
    };
    // ⑤ 清暂存：本机那一份不论成败都删（下载半路留下的 `.part` 一起）；B 那头半路失败 / 撤 ⇒ 开过单的暂存件删掉。
    for rel in [kept.clone(), format!("{kept}.part")] {
        let _ = super::source::ask(
            line,
            &local,
            "files-delete",
            &serde_json::json!({ "root": staging, "rel": rel }),
            super::writeops::WRITE_BUDGET,
        )
        .await;
    }
    match pushed {
        Ok(()) => Outcome::Done {
            name: name.to_string(),
            machine: machine.trim().to_string(),
            path: dest,
            bytes: board.pull.seen().1,
        },
        Err(why) => {
            let bstaging = format!("{}/{}", bhome.trim_end_matches('/'), STAGING_DIR);
            for k in board.push.take_staged() {
                let _ = super::source::ask(
                    line,
                    &to,
                    "files-delete",
                    &serde_json::json!({ "root": bstaging, "rel": format!("{k}.part") }),
                    super::writeops::WRITE_BUDGET,
                )
                .await;
            }
            failed(why)
        }
    }
}

/// 起一趟（UI 线程调）：摆「在复制」，扔给 tokio。
pub fn spawn(
    h: &tokio::runtime::Handle,
    line: super::source::Line,
    from: super::source::Origin,
    p: CrossPrompt,
    board: CrossBoard,
) {
    board.begin(&p.name);
    h.spawn(async move {
        let asker = board.clone();
        let o = run(
            &line,
            &from,
            &p.src,
            &p.name,
            &p.machine,
            &p.dir,
            &board,
            |said| async move { asker.ask(said).await.unwrap_or(false) },
        )
        .await;
        board.finish(o);
    });
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/cross_copy_tests.rs"]
mod tests;
