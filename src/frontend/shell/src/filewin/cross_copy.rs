//! 〔FILES2 · 第四波 · 2026-09-27〕**复制到另一台机器** —— `设计/60 §6.2`「跨机复制」· `§7` 第 9 条 Q2。
//!
//! 主会话按通行做法裁：「保留『一窗一机』；在两个窗口之间拖 / 复制到另一台 ⇒ 一个任务『从 A 下到本机暂存 → 传到 B 的暂存 →
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

use crate::copy_table::copy_text;

/// 菜单上那一项的字。**唯一住址**。
pub static CROSS_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinCrossCopy.label.cross", &[]));

/// 本机后端的地址（⚠ 与 app 侧 `backend::control::inbound_client::LOCAL_ORIGIN` 是同一个值的两份 —— 窗口够不到 app 侧，
/// 判据 `cross_copy_tests::the_local_origin_is_the_app_one` 读两侧源码钉相等）。
pub const LOCAL_ORIGIN: &str = "<local>";

/// 暂存区（相对 home）。⚠ 与后端 `control/files_commit.rs::STAGING_DIR` 是同一个值的两份（两个 crate 互相引不到），
/// 判据 `cross_copy_tests::the_staging_dir_is_the_backend_one` 读两侧源码钉相等。
pub const STAGING_DIR: &str = ".cc-monitor/staging";

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
    Skipped {
        name: String,
        path: String,
    },
    Failed {
        name: String,
        why: String,
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
                ("human", &super::rows::human_size(*bytes)),
            ],
        ),
        Outcome::Skipped { name, path } => copy_text(
            "rsFilewinCrossCopy.outcome.skipped",
            &[("name", name), ("path", path)],
        ),
        Outcome::Failed { name, why } => copy_text(
            "rsFilewinCrossCopy.outcome.failed",
            &[("name", name), ("why", why)],
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

    /// 画：盖不盖那一问（模态）· 在复制那一行（进度 ＋ 取消）· 上一趟的结局。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let (running, asking, last) = {
            let d = self.inner.lock().unwrap();
            (
                d.running.clone(),
                d.asking.as_ref().map(|(s, _)| s.clone()),
                d.last.clone(),
            )
        };
        if let Some(said) = asking {
            let (mut yes, mut no) = (false, false);
            egui::Modal::new(egui::Id::new("filewin-cross-overwrite")).show(ui.ctx(), |ui| {
                ui.label(said);
                ui.horizontal(|ui| {
                    yes = ui
                        .button(copy_text("rsFilewinCrossCopy.ask.overwrite", &[]))
                        .clicked();
                    no = ui
                        .button(copy_text("rsFilewinCrossCopy.ask.skip", &[]))
                        .clicked();
                });
            });
            if yes || no {
                self.settle(yes);
            }
        }
        if let Some(name) = &running {
            let (got, total) = self.seen(name);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(copy_text(
                    "rsFilewinCrossCopy.ui.running",
                    &[
                        ("name", name),
                        ("got", &super::rows::human_size(got / 2)),
                        ("total", &super::rows::human_size(total / 2)),
                    ],
                ));
                if total > 0 {
                    ui.add(egui::ProgressBar::new(got as f32 / total as f32).desired_width(160.0));
                }
                if ui
                    .button(copy_text("rsFilewinCrossCopy.ui.cancel", &[]))
                    .clicked()
                {
                    self.cancel();
                }
            });
        }
        if let Some(o) = &last {
            let text = outcome_text(o);
            if matches!(o, Outcome::Failed { .. }) {
                ui.colored_label(egui::Color32::from_rgb(0xE0, 0x9A, 0x20), text);
            } else {
                ui.label(text);
            }
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
    let failed = |why: String| Outcome::Failed {
        name: name.to_string(),
        why,
    };
    let to = crate::chan::wire::Origin(machine.trim().to_string());
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
            return failed(copy_text(
                "rsFilewinCrossCopy.target.unreachable",
                &[("machine", machine), ("why", &e)],
            ))
        }
    };
    let dir = match target_dir(typed_dir, &bhome) {
        Ok(d) => d,
        Err(e) => return failed(e),
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
    // 〔FILES2 · V152〕目标就是本机 ⇒ 没有第二腿：直接下到落点（有损名走按字节读回那一条）。
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
    let local = crate::chan::wire::Origin(LOCAL_ORIGIN.to_string());
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
        Err(e) => return failed(e),
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
            return failed(e);
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
#[path = "../../../../../tests/frontend/shell/filewin/cross_copy_tests.rs"]
mod tests;
