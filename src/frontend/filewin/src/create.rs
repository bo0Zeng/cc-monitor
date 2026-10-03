//! 窗口上的**「新建空文件」** —— 老面板 7 项里写侧那一项
//! （补齐那张表）。
//!
//! # 一、后端那一半早就在了，这一刀只接窗口这一半
//!
//! `files-create`（F1 · 波 5）：在一个目标根底下 `O_EXCL` 新建一份文件，**不给 `content`
//! 就是一份空文件**；先过写面那道路径解析；目标已经在（哪怕只是一条 symlink）就失败，
//! 绝不覆盖。契约住 `src/doc/IPC-PROTOCOL.md` 的 `files-create` 那一节。
//! ⇒ 本模块**一行写盘代码都没有**，它做三件事：要一个名字 · 切成 `(root, rel)` · 经
//! [`super::source::ask`] 说那一条命令（窗口进程够后端只有那一处 `call`）。
//!
//! # 二、为什么「存在就拒」而不是「问要不要覆盖」
//!
//! 同老面板 `newFile` 那段头注的裁定（issue #65）：**新建就是新建，不是「新建或覆盖」**。
//! 从前那一侧要先 `stat` 再写（中间有竞态、`stat` 失败还不严格等于「不存在」）；
//! 现在「不存在才写」由后端那一处 `O_EXCL` **原子地**答 ⇒ 两条边界都一起没了。
//! 真想改一份已有文件的人有现成的路：点开它、编辑、保存。
//!
//! # 三、为什么**不**走 [`super::writeops::run_writes`]（不加第五种 `WriteOp`）
//!
//! `WriteOp` 是刻意封闭的（那四条改动**既有**数据的操作）；它那条流水线的三段里，
//! 本操作只用得上第三段：
//! - ① 本地预判围栏〔连同后端那一道一起删了，这一条今天是历史〕：那道围栏的本地副本（`sftp_pool::is_protected_claude_data_path`）的去留
//!   正挂在 `boundary_tests::WINDOW_SIDE` 那一格上（「本地预判围栏」）。
//!   给它添一个新消费者是往**留**那一侧加码 ⇒ 本操作只靠**后端那一道**（它才是权威；
//!   被拒那句原话经 [`super::source::said`] 原样画到窗口上）。
//! - ② 一次问完：新建不毁任何东西（存在就拒）⇒ 不问（同 `writeops` 头注 §四「新建目录不问」）。
//! ⇒ 只借它的**结果板**（[`super::writeops::WriteBoard`]）：画结果、跑完重列目录，
//!   都是现成的那一条路，不另立一块板子。
//!
//! # ⚠ 买不到什么
//!
//! - 真远端上经这条命令建成过一份文件的读数（判据挂的是合成后端 ＋ 真回环口 ＋ 真钥匙）。
//! - 窗口真画在屏幕上（要图形会话）；判据跑的是生产那个 `frame_body`，读这一帧画出来的字。

use super::shell::{FileWindow, NO_LINE};
use super::source::{Line, Origin};
use super::writeops::{clean_name, join_remote, WriteOutcome, WRITE_BUDGET};
use copy_core::copy_text;

/// 工具栏上那颗按钮的字面。**唯一住址** —— 判据按同一个常量去找它画出来的字。
pub static NEW_FILE_LABEL: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinCreate.label.newFile", &[]));

/// 后端那条命令（`files-create`）。
pub const CMD_CREATE: &str = "files-create";

/// 「新建空文件叫什么」那个框 —— **UI 线程自己的草稿**
/// （同 [`super::writeops::WritePrompt`] 逐字的理由：正在输入的那几个字只有 UI 线程碰得到）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewFilePrompt {
    /// 建在哪个目录里（＝ 摆出这个框那一刻的当前目录）。
    pub dir: String,
    /// 正在编辑的那几个字。
    pub text: String,
}

impl NewFilePrompt {
    pub fn new(dir: &str) -> Self {
        Self {
            dir: dir.to_string(),
            text: String::new(),
        }
    }

    /// 框上那一行提示。
    /// 字从文案表取 ⇒ 回 `String`。
    pub fn heading() -> String {
        copy_text("rsFilewinCreate.heading.newFile", &[])
    }

    /// 框里那几个字 → 那份新文件的**完整路径**。
    ///
    /// 名字的规矩与「新建目录」**同一个函数**（[`clean_name`]：不许空、不许带 `/`、
    /// 不许是 `.` / `..`）—— 两颗并排的「新建」按钮，一个名字在这颗上合法、在那颗上不合法，
    /// 就是两套手感。
    ///
    /// # Errors
    ///
    /// 回一句给用户的话（调用方据此出声、把框留着）。
    pub fn to_path(&self) -> Result<String, String> {
        let name = clean_name(self.text.trim())?;
        Ok(join_remote(&self.dir, &name))
    }
}

/// 结果行 / 失败那一句里用的描述。
pub fn label(path: &str) -> String {
    format!("{} {path}", NEW_FILE_LABEL.as_str())
}

/// 真建一份 —— 经通道说 `files-create`，参数切成 `(root, rel)`，**不带 `content`**
/// （契约逐字：不给 `content` ⇒ 新建一份空文件）。
///
/// 切法走 [`super::source::parent_dir`] / [`super::source::remote_basename`] 那一对
/// （同 `writeops::apply_remote`，不另写一份切法）。
///
/// # Errors
///
/// 后端那句原话（围栏拒 `refused` · 已存在 / 父目录不在 `io_failed` · 通道哪一段断了），
/// 经 [`super::source::said`] 翻过，原样交出去。
pub async fn create_remote(line: &Line, origin: &Origin, path: &str) -> Result<(), String> {
    create_remote_at(line, origin, &super::source::RemotePath::plain(path)).await
}

/// 〔有损名全寻址〕同 [`create_remote`]，路径可以带字节（有损目录里新建：根发 `{"b16": …}`）。
pub async fn create_remote_at(
    line: &Line,
    origin: &Origin,
    at: &super::source::RemotePath,
) -> Result<(), String> {
    let args = serde_json::json!({
        "root": at.parent().wire(),
        "rel": at.tail_wire(),
    });
    super::source::ask(line, origin, CMD_CREATE, &args, WRITE_BUDGET)
        .await
        .map(|_| ())
}

impl FileWindow {
    /// 正摆着的那个框（判据与 [`Self::new_file_ui`] 用）。
    pub fn new_file_prompt(&self) -> Option<&NewFilePrompt> {
        self.new_file.as_ref()
    }

    /// 那个框里正在编辑的那几个字（`None` ＝ 没在问）。
    ///
    /// 它是**生产代码**：[`Self::new_file_ui`] 喂 `text_edit_singleline` 要的就是这个 `&mut`
    /// （同 `shell::FileWindow::pull_dest_mut` 那一条：界面与判据走同一条路）。
    pub fn new_file_text_mut(&mut self) -> Option<&mut String> {
        self.new_file.as_mut().map(|p| &mut p.text)
    }

    /// 摆出「新建空文件」那个框。回值 ＝ 真的摆出来了。
    pub fn begin_new_file(&mut self) -> bool {
        self.new_file = Some(NewFilePrompt::new(&self.cwd));
        true
    }

    /// 收掉那个框，什么都不做。
    pub fn cancel_new_file(&mut self) {
        self.new_file = None;
    }

    /// 框里那几个字 → 一次 `files-create`。回值 ＝ 真的发出去了。
    ///
    /// ⚠ 名字不合法 / 没有运行时 / 没连上后端 ⇒ **框留着、出声**，不静默收掉
    /// （收掉的话用户点了确定什么都没发生，与成功长得一模一样）。
    /// 跑完落在写操作那块结果板上（[`super::writeops::WriteBoard::finish`]）⇒
    /// 结果那一行照它的样子画，目录照它的节拍重列一次。
    pub fn confirm_new_file(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.new_file.clone() else {
            return false;
        };
        let path = match p.to_path() {
            Ok(path) => path,
            Err(why) => {
                *self.listing.error.lock().unwrap() = Some(why);
                return false;
            }
        };
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinCreate.confirm.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string());
            return false;
        };
        let origin = self.source.origin();
        let board = self.write_board.clone();
        board.attach(ctx);
        // 落点 ＝ 当前目录的字节 ＋ 新名字（合法 UTF-8 目录时与 `path` 逐字节同）。
        let at = super::shell::join_path(
            &self.cwd_path(),
            super::source::remote_basename(&path).as_bytes(),
        );
        h.spawn(async move {
            let out = match create_remote_at(&line, &origin, &at).await {
                Ok(()) => WriteOutcome {
                    ok: 1,
                    ..WriteOutcome::default()
                },
                Err(e) => WriteOutcome {
                    failed: vec![(label(&path), e)],
                    ..WriteOutcome::default()
                },
            };
            board.finish(out);
        });
        self.new_file = None;
        true
    }

    /// 画「新建空文件叫什么」那个框。**模态** —— 定下来之前不接别的。
    pub(super) fn new_file_ui(&mut self, ui: &mut egui::Ui) {
        if self.new_file.is_none() {
            return;
        }
        let (mut go, mut cancel) = (false, false);
        let (_, esc) = super::shell::modal(ui.ctx(), "filewin-new-file", |ui| {
            ui.heading(NewFilePrompt::heading());
            if let Some(text) = self.new_file_text_mut() {
                go |= super::shell::prompt_field(ui, text);
            }
            ui.label(&copy_text("rsFilewinCreate.ui.sameDirOnly", &[]));
            ui.horizontal(|ui| {
                if ui
                    .button(&copy_text("rsFilewinCreate.ui.ok", &[]))
                    .clicked()
                {
                    go = true;
                }
                if ui
                    .button(&copy_text("rsFilewinCreate.ui.cancel", &[]))
                    .clicked()
                {
                    cancel = true;
                }
            });
        });
        cancel |= esc;
        if cancel {
            self.cancel_new_file();
        } else if go {
            let ctx = ui.ctx().clone();
            self.confirm_new_file(Some(ctx));
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/create_tests.rs"]
mod tests;
