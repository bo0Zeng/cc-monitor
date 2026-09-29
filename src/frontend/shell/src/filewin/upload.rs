//! 〔F7c · 第三波 · 2026-09-24〕**上传那颗按钮** —— 工具栏上一颗「上传」，问一句「本机哪几个文件」，
//! 之后走的就是拖入那一条（[`super::transfer::run_drop`]：先一次问完覆盖，再并行传）。
//!
//! # 为什么它现在做得动了（`transfer.rs` 头注那条「做不动」的三件，逐条）
//!
//! ① **那道设计题拍了**：用户逐字「**保留SFTP. 思考怎么干净**」⇒ `设计/60 §13`：上传经通道开单、订阅进度，
//!    SFTP 只写暂存区，落进用户目录那一下是后端 `files-commit-upload`。按钮背后接的就是这一条。
//! ② **原生选文件框仍然没有**，理由不变（`rfd` 不是直接依赖、加依赖动 `src/frontend/shell/Cargo.toml` ——
//!    那几行归 C2；「能不能从 egui 那条线程弹出来」本机无图形会话验不了）⇒ 走窗口自己那套「问一句」
//!    （同 `download.rs` 的「存到哪儿」、`writeops.rs` 的「叫什么名字」）：全程可判，零新依赖。
//! ③ 按钮挂在 `shell.rs` 工具栏上**只有挂载那几行**（FW1 为主那份文件）；状态与判定全在本文件。
//!
//! # 它刻意**不**做的
//!
//! - 不自己起传输、不自己问覆盖：选完就交给 [`super::shell::FileWindow::start_drop`]
//!   （与拖入同一条路 ⇒ 两个入口在「一次问完」「并行度」「撤」上不可能分岔）。
//! - 不猜：框是空的 ⇒ 说出来，不拿一个缺省路径兜；某一行不是本机上的一份文件 ⇒ 整批不起、点名那一行
//!   （起一半、拒一半的话，用户分不清哪几件真的走了）。
//!
//! # 买不到什么
//!
//! - 原生选文件框（见 ②）；多选靠一行一个路径，不是鼠标框选。
//! - 真窗口里的点击：判据喂的是状态机与判定（纯函数），按钮挂在工具栏上那一跳由 `shell_tests` 另判。

use super::transfer::Pending;
use crate::copy_table::copy_text;

/// 工具栏上那颗按钮的字面。**唯一住址**（判据按同一个常量去找它画出来的字）。
pub static UPLOAD_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinUpload.label.upload", &[]));

/// 那一问正摆着的样子。`text` 是框里正在编辑的那几个字（一行一个本机路径）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UploadPrompt {
    /// `None` = 没在问。
    ask: Option<String>,
    /// 上一次「确定」被拒的那句话（框**留着**，不清空用户敲的东西）。
    refused: Option<String>,
    /// 〔W5-FILES · `设计/60 §6.2`〕这一帧点了「选择…」（窗口取走它去起原生选择框，[`super::picker`]）。
    browse: bool,
}

/// 判一遍「确定」时框里的东西。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UploadVerdict {
    /// 不合法 —— 带着原话回去，框留着。
    Rejected(String),
    /// 可以起了：这几件，全都放进 `remote_dir`。
    Go(Vec<Pending>),
}

/// 🔴 **判定**：框里的文字 → 这一摞要传的（或一句拒绝）。
///
/// - 按行切、去首尾空白、丢空行；一行都没有 ⇒ 拒。
/// - 每一行都必须是本机上的一份文件（`is_file` 是**注进来的**，判据不必在盘上摆真文件就走得到两支）；
///   有一行不是 ⇒ **整批**拒，点名那一行。
/// - 远端落点照拖入那一条拼（[`Pending::into_remote_dir`]，恒用 `/`）；拿不到名字 ⇒ 拒。
pub fn judge_upload(text: &str, remote_dir: &str, is_file: impl Fn(&str) -> bool) -> UploadVerdict {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return UploadVerdict::Rejected(copy_text("rsFilewinUpload.judge.empty", &[]));
    }
    let mut out = Vec::with_capacity(lines.len());
    for l in lines {
        if !is_file(l) {
            return UploadVerdict::Rejected(copy_text(
                "rsFilewinUpload.judge.notLocalFile",
                &[("path", &l.to_string())],
            ));
        }
        match Pending::into_remote_dir(l, remote_dir) {
            Some(p) => out.push(p),
            None => {
                return UploadVerdict::Rejected(copy_text(
                    "rsFilewinUpload.judge.noName",
                    &[("path", &l.to_string())],
                ))
            }
        }
    }
    UploadVerdict::Go(out)
}

/// 生产那一侧的「是不是本机上的一份文件」。**全树恰好一处**（与 `download::dest_exists` 同一条纪律）。
pub fn local_is_file(p: &str) -> bool {
    std::path::Path::new(p).is_file()
}

impl UploadPrompt {
    /// 摆出那一问（框是空的 —— 不预填：本机没有一个「最可能要传的文件」）。
    pub fn open(&mut self) {
        self.ask = Some(String::new());
        self.refused = None;
    }

    /// 正在问吗。
    pub fn is_open(&self) -> bool {
        self.ask.is_some()
    }

    /// 框里此刻的字（判据与界面看同一个值）。
    pub fn text(&self) -> Option<&str> {
        self.ask.as_deref()
    }

    /// 上一次被拒的那句话。
    pub fn refused(&self) -> Option<&str> {
        self.refused.as_deref()
    }

    /// 改框里的字（界面那一侧的 `TextEdit` 与判据走同一个口）。
    pub fn text_mut(&mut self) -> Option<&mut String> {
        self.ask.as_mut()
    }

    /// 「取消」：问题收掉，什么都不起。
    pub fn cancel(&mut self) {
        self.ask = None;
        self.refused = None;
        self.browse = false;
    }

    /// 〔W5-FILES〕这一帧点没点「选择…」（取走即清）。
    pub fn take_browse(&mut self) -> bool {
        std::mem::take(&mut self.browse)
    }

    /// 〔W5-FILES〕原生选择框选到的路径接进框里（一行一个，已有的不动）；没选到 ⇒ 框不动、说一句。
    /// 框已经收掉了（选的时候人点了取消）⇒ 什么都不做。
    pub fn take_picked(&mut self, picked: Option<Vec<std::path::PathBuf>>) {
        let Some(text) = self.ask.as_mut() else {
            return;
        };
        match picked {
            Some(p) if !p.is_empty() => {
                *text = super::picker::append_lines(text, &p);
                self.refused = None;
            }
            _ => self.refused = Some(copy_text("rsFilewinPicker.pick.none", &[])),
        }
    }

    /// 「确定」：判一遍。合法 ⇒ 问题收掉、交回那一摞；不合法 ⇒ 框留着、记下那句话、回 `None`。
    pub fn confirm(
        &mut self,
        remote_dir: &str,
        is_file: impl Fn(&str) -> bool,
    ) -> Option<Vec<Pending>> {
        let text = self.ask.as_deref()?;
        match judge_upload(text, remote_dir, is_file) {
            UploadVerdict::Go(items) => {
                self.cancel();
                Some(items)
            }
            UploadVerdict::Rejected(why) => {
                self.refused = Some(why);
                None
            }
        }
    }

    /// 画那一问。**模态**。回「确定之后可以起的那一摞」（没点确定 / 被拒 ⇒ `None`）。
    pub fn ui(&mut self, ui: &mut egui::Ui, remote_dir: &str) -> Option<Vec<Pending>> {
        if !self.is_open() {
            return None;
        }
        let (mut go, mut cancel) = (false, false);
        let refused = self.refused.clone();
        egui::Modal::new(egui::Id::new("filewin-upload-prompt")).show(ui.ctx(), |ui| {
            ui.heading(&copy_text("rsFilewinUpload.ui.title", &[]));
            ui.label(&copy_text("rsFilewinUpload.ui.paths", &[]));
            if let Some(text) = self.text_mut() {
                ui.text_edit_multiline(text);
            }
            // 〔W5-FILES · `设计/60 §6.2`〕原生选择框：选到的路径填进上面那个框，确定照旧走 `judge_upload`。
            if ui
                .button(&copy_text("rsFilewinPicker.ui.browse", &[]))
                .clicked()
            {
                self.browse = true;
            }
            if let Some(why) = &refused {
                ui.colored_label(egui::Color32::RED, why);
            }
            ui.horizontal(|ui| {
                if ui
                    .button(&copy_text("rsFilewinUpload.ui.ok", &[]))
                    .clicked()
                {
                    go = true;
                }
                if ui
                    .button(&copy_text("rsFilewinUpload.ui.cancel", &[]))
                    .clicked()
                {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.cancel();
            return None;
        }
        if go {
            return self.confirm(remote_dir, local_is_file);
        }
        None
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/filewin/upload_tests.rs"]
mod tests;
