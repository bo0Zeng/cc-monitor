//! **解压到这里** —— 「解压」· `§7` 第 9 条 Q3 的窗口那一侧。
//!
//! 按通行做法：「右键『解压到这里』，由那台后端用 Rust 库解到同目录下以包名命名的新目录；……撞名就问；
//! 支持 zip · tar · tar.gz · tgz，其余格式说『不认这种包』」。
//!
//! 窗口只做三件：① 发 `files-extract {root, rel}`（落点名字、认不认这种包都由后端判，窗口不写第二份后缀表）；
//! ② 后端答 `exists`（那个目录已在）⇒ **问人**：「另起一个名字解」（再发一趟带 `fresh: true`，后端取第一个不在的 `名 (n)`）/「不解了」；
//! ③ 结局一句话，跑完重列目录。后端阻塞档一趟做完才回话 ⇒ 不可取消、没有进度，只画「正在那台机器上解压 …」。

use std::sync::{Arc, Mutex};

use copy_core::copy_text;

/// 菜单上那一项的字。**唯一住址**。
pub static EXTRACT_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinExtract.label.extract", &[]));

/// 那条线上命令。
pub const CMD_EXTRACT: &str = <cmd_wire::files::FilesExtractReq as cmd_wire::Call>::NAME;

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

/// `files-extract` 的回包（按类型解好的）→ [`Extracted`]。
pub fn extracted_from_reply(d: &cmd_wire::files::FilesTreeReply) -> Extracted {
    Extracted {
        path: String::from_utf8_lossy(d.path.bytes()).to_string(),
        files: d.files,
        dirs: d.dirs,
        links: d.links,
        bytes: d.bytes,
    }
}

/// 一趟的结局。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done(Extracted),
    /// 人答了「不解了」。
    Skipped(String),
    /// 没解成：那一句 ＋ 复制详情（[`super::source::Failed`]）。
    Failed(super::source::Failed),
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
                ("human", &copy_core::size_text(e.bytes)),
            ],
        ),
        Outcome::Skipped(said) => copy_text(
            "rsFilewinExtract.outcome.skipped",
            &[("name", name), ("said", said)],
        ),
        Outcome::Failed(why) => copy_text(
            "rsFilewinExtract.outcome.failed",
            &[("name", name), ("why", &why.said)],
        ),
    }
}

/// 结局那一句的［复制详情］整段（没解成、带详情时才有）。
pub fn outcome_copy(name: &str, o: &Outcome) -> Option<String> {
    match o {
        Outcome::Failed(f) => batch_core::copy_body(&outcome_text(name, o), &f.detail),
        _ => None,
    }
}

/// 真发一趟（`fresh` ＝ 人已经答了「另起一个名字」）。回 `Err((码, 原话))`。
pub async fn extract_remote(
    line: &super::source::Line,
    origin: &super::source::Origin,
    root: &cmd_wire::files::WirePath,
    rel: &cmd_wire::files::WirePath,
    fresh: bool,
) -> Result<Extracted, super::source::Failed> {
    let req = cmd_wire::files::FilesExtractReq {
        root: root.clone(),
        rel: rel.clone(),
        fresh: fresh.then_some(true),
    };
    let d = super::source::ask_typed(line, origin, &req, EXTRACT_BUDGET).await?;
    Ok(extracted_from_reply(&d))
}

/// 整趟：先发一次；后端说 `exists` ⇒ 把原话交给 `ask`（人答「另起一个名字」⇒ `true`）⇒ 带 `fresh` 再发一次。
pub async fn run<A, AFut>(
    line: &super::source::Line,
    origin: &super::source::Origin,
    root: &cmd_wire::files::WirePath,
    rel: &cmd_wire::files::WirePath,
    ask: A,
) -> Outcome
where
    A: FnOnce(String) -> AFut,
    AFut: std::future::Future<Output = bool>,
{
    match extract_remote(line, origin, root, rel, false).await {
        Ok(e) => Outcome::Done(e),
        Err(f) if f.code == "exists" => {
            let said = f.said;
            if !ask(said.clone()).await {
                return Outcome::Skipped(said);
            }
            match extract_remote(line, origin, root, rel, true).await {
                Ok(e) => Outcome::Done(e),
                Err(why) => Outcome::Failed(why),
            }
        }
        Err(why) => Outcome::Failed(why),
    }
}

/// 窗口上的样子：在解哪一个 · 上一趟的结局（通用的那一格 [`super::trip::Lane`]）＋ 解压独有的撞名那一问。
/// **跨线程共享**（UI 线程画，tokio 那条写）。
#[derive(Clone, Default)]
pub struct ExtractBoard {
    lane: super::trip::Lane<(String, Outcome)>,
    asking: Arc<Mutex<Option<(String, tokio::sync::oneshot::Sender<bool>)>>>,
}

impl ExtractBoard {
    pub fn attach(&self, ctx: Option<egui::Context>) {
        self.lane.attach(ctx);
    }

    pub fn begin(&self, name: &str) {
        self.lane.begin(name);
    }

    pub fn running(&self) -> Option<String> {
        self.lane.running()
    }

    /// 摆出撞名那一问；回的收端落地 ＝ 人答了（`true` ＝ 另起一个名字）。
    pub fn ask(&self, said: String) -> tokio::sync::oneshot::Receiver<bool> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        *self.asking.lock().unwrap() = Some((said, tx));
        self.lane.poke();
        rx
    }

    pub fn is_asking(&self) -> bool {
        self.asking.lock().unwrap().is_some()
    }

    /// 人答了那一问（判据与界面同一个口）。没在问 ⇒ `false`。
    pub fn settle(&self, fresh: bool) -> bool {
        let Some((_, tx)) = self.asking.lock().unwrap().take() else {
            return false;
        };
        tx.send(fresh).ok();
        self.lane.poke();
        true
    }

    pub fn finish(&self, name: &str, o: Outcome) {
        *self.asking.lock().unwrap() = None;
        self.lane.finish((name.to_string(), o));
    }

    pub fn rounds(&self) -> u64 {
        self.lane.rounds()
    }

    pub fn last(&self) -> Option<(String, Outcome)> {
        self.lane.last()
    }

    /// 画撞名那一问（模态）。「正在解」与结局是「进度」表里的一行（`super::progress`，那一句从 [`outcome_text`] 出来）。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let asking = self.asking.lock().unwrap().as_ref().map(|(s, _)| s.clone());
        let Some(said) = asking else {
            return;
        };
        // Esc / 点压暗的底 ＝「不解了」（第 0 颗）。
        let hit = super::kit::dialog(
            ui.ctx(),
            "filewin-extract-taken",
            EXTRACT_LABEL.as_str(),
            |ui| {
                ui.label(copy_text("rsFilewinExtract.ask.taken", &[("said", &said)]));
            },
            &[
                (
                    copy_text("rsFilewinExtract.ask.skip", &[]),
                    super::kit::Btn::Plain,
                ),
                (
                    copy_text("rsFilewinExtract.ask.fresh", &[]),
                    super::kit::Btn::Primary,
                ),
            ],
            1,
            0,
        );
        if let Some(i) = hit {
            self.settle(i == 1);
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/extract_tests.rs"]
mod tests;
