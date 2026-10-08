//! **解压到这里** —— 「解压」· `§7` 第 9 条 Q3 的窗口那一侧。
//!
//! 按通行做法：「右键『解压到这里』，由那台后端用 Rust 库解到同目录下以包名命名的新目录；……撞名就问；
//! 支持 zip · tar · tar.gz · tgz，其余格式说『不认这种包』」。
//!
//! 窗口只做三件：① 发 `files-extract {root, rel}`（落点名字、认不认这种包都由后端判，窗口不写第二份后缀表）；
//! ② 后端答 `exists`（那个目录已在）⇒ **问人**：「另起一个名字解」（再发一趟带 `fresh: true`，后端取第一个不在的 `名 (n)`）/「不解了」；
//! ③ 结局一句话，跑完重列目录。后端阻塞档一趟做完才回话 ⇒ 不可取消、没有进度，只画「正在那台机器上解压 …」。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use copy_core::copy_text;

/// 菜单上那一项的字。**唯一住址**。
pub static EXTRACT_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinExtract.label.extract", &[]));

/// 那条线上命令。
pub const CMD_EXTRACT: &str = "files-extract";

/// 一趟的往返上限：与递归删 / 复制目录同一个上界（取消不掉 ⇒ 期限是它唯一的上界）。
pub const EXTRACT_BUDGET: std::time::Duration = super::writeops::TREE_BUDGET;

/// 解完了什么（后端报的，原样）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extracted {
    pub path: String,
    pub files: u64,
    pub dirs: u64,
    pub links: u64,
    pub bytes: u64,
}

/// `files-extract` 的 `data` → [`Extracted`]。缺字段 ⇒ 失败（不补 0）。
pub fn extracted_from_reply(d: &serde_json::Value) -> Result<Extracted, String> {
    let n = |k: &str| -> Result<u64, String> {
        d.get(k)
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| copy_text("rsFilewinExtract.reply.missingField", &[("field", k)]))
    };
    let path = d
        .get("path")
        .and_then(super::find::decode_path)
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .ok_or_else(|| copy_text("rsFilewinExtract.reply.missingField", &[("field", "path")]))?;
    Ok(Extracted {
        path,
        files: n("files")?,
        dirs: n("dirs")?,
        links: n("links")?,
        bytes: n("bytes")?,
    })
}

/// 一趟的结局。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done(Extracted),
    /// 人答了「不解了」。
    Skipped(String),
    Failed(String),
}

/// 结局那一句。
pub fn outcome_text(name: &str, o: &Outcome) -> String {
    match o {
        Outcome::Done(e) => copy_text(
            "rsFilewinExtract.outcome.done",
            &[
                ("name", name),
                ("path", &e.path),
                ("files", &e.files.to_string()),
                ("dirs", &e.dirs.to_string()),
                ("links", &e.links.to_string()),
                ("human", &super::rows::human_size(e.bytes)),
            ],
        ),
        Outcome::Skipped(said) => copy_text(
            "rsFilewinExtract.outcome.skipped",
            &[("name", name), ("said", said)],
        ),
        Outcome::Failed(why) => copy_text(
            "rsFilewinExtract.outcome.failed",
            &[("name", name), ("why", why)],
        ),
    }
}

/// 真发一趟（`fresh` ＝ 人已经答了「另起一个名字」）。回 `Err((码, 原话))`。
pub async fn extract_remote(
    line: &super::source::Line,
    origin: &super::source::Origin,
    root: &serde_json::Value,
    rel: &serde_json::Value,
    fresh: bool,
) -> Result<Extracted, (Option<String>, String)> {
    let mut args = serde_json::json!({ "root": root, "rel": rel });
    if fresh {
        args["fresh"] = serde_json::Value::Bool(true);
    }
    let d = super::source::ask_coded(line, origin, CMD_EXTRACT, &args, EXTRACT_BUDGET)
        .await
        .map_err(|f| (f.code, f.said))?;
    extracted_from_reply(&d).map_err(|e| (None, e))
}

/// 整趟：先发一次；后端说 `exists` ⇒ 把原话交给 `ask`（人答「另起一个名字」⇒ `true`）⇒ 带 `fresh` 再发一次。
pub async fn run<A, AFut>(
    line: &super::source::Line,
    origin: &super::source::Origin,
    root: &serde_json::Value,
    rel: &serde_json::Value,
    ask: A,
) -> Outcome
where
    A: FnOnce(String) -> AFut,
    AFut: std::future::Future<Output = bool>,
{
    match extract_remote(line, origin, root, rel, false).await {
        Ok(e) => Outcome::Done(e),
        Err((Some(code), said)) if code == "exists" => {
            if !ask(said.clone()).await {
                return Outcome::Skipped(said);
            }
            match extract_remote(line, origin, root, rel, true).await {
                Ok(e) => Outcome::Done(e),
                Err((_, why)) => Outcome::Failed(why),
            }
        }
        Err((_, why)) => Outcome::Failed(why),
    }
}

/// 窗口上的样子：在解哪一个 · 撞名那一问 · 上一趟的结局。**跨线程共享**（UI 线程画，tokio 那条写）。
#[derive(Clone, Default)]
pub struct ExtractBoard {
    inner: Arc<Mutex<Desk>>,
    rounds: Arc<AtomicU64>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

#[derive(Default)]
struct Desk {
    running: Option<String>,
    asking: Option<(String, tokio::sync::oneshot::Sender<bool>)>,
    last: Option<(String, Outcome)>,
}

impl ExtractBoard {
    pub fn attach(&self, ctx: Option<egui::Context>) {
        *self.ctx.lock().unwrap() = ctx;
    }

    fn poke(&self) {
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }

    pub fn begin(&self, name: &str) {
        self.inner.lock().unwrap().running = Some(name.to_string());
        self.poke();
    }

    pub fn running(&self) -> Option<String> {
        self.inner.lock().unwrap().running.clone()
    }

    /// 摆出撞名那一问；回的收端落地 ＝ 人答了（`true` ＝ 另起一个名字）。
    pub fn ask(&self, said: String) -> tokio::sync::oneshot::Receiver<bool> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.inner.lock().unwrap().asking = Some((said, tx));
        self.poke();
        rx
    }

    pub fn is_asking(&self) -> bool {
        self.inner.lock().unwrap().asking.is_some()
    }

    /// 人答了那一问（判据与界面同一个口）。没在问 ⇒ `false`。
    pub fn settle(&self, fresh: bool) -> bool {
        let Some((_, tx)) = self.inner.lock().unwrap().asking.take() else {
            return false;
        };
        tx.send(fresh).ok();
        self.poke();
        true
    }

    pub fn finish(&self, name: &str, o: Outcome) {
        {
            let mut d = self.inner.lock().unwrap();
            d.running = None;
            d.asking = None;
            d.last = Some((name.to_string(), o));
        }
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    pub fn last(&self) -> Option<(String, Outcome)> {
        self.inner.lock().unwrap().last.clone()
    }

    /// 画撞名那一问（模态）。「正在解」与结局是「进度」表里的一行（`super::progress`，那一句从 [`outcome_text`] 出来）。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let asking = self
            .inner
            .lock()
            .unwrap()
            .asking
            .as_ref()
            .map(|(s, _)| s.clone());
        if let Some(said) = asking {
            let (mut fresh, mut skip) = (false, false);
            let (_, esc) = super::shell::modal(ui.ctx(), "filewin-extract-taken", |ui| {
                ui.label(copy_text("rsFilewinExtract.ask.taken", &[("said", &said)]));
                ui.horizontal(|ui| {
                    fresh = ui
                        .button(copy_text("rsFilewinExtract.ask.fresh", &[]))
                        .clicked();
                    skip = ui
                        .button(copy_text("rsFilewinExtract.ask.skip", &[]))
                        .clicked();
                });
            });
            skip |= esc;
            if fresh || skip {
                self.settle(fresh);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/extract_tests.rs"]
mod tests;
