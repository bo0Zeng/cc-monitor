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

use super::shell::FileWindow;
use super::source::{Line, Origin};
use super::writeops::WRITE_BUDGET;

/// 后端那条命令（`files-create`）。
pub const CMD_CREATE: &str = "files-create";

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

/// 就地新建：名字是用户敲的 `typed`，原样交后端、带 `single: true`（合不合法由那台判，不合 ⇒ 码 `bad_name`）。
pub async fn create_typed_coded(
    line: &Line,
    origin: &Origin,
    dir: &super::source::RemotePath,
    typed: &str,
) -> Result<(), super::source::Failed> {
    let args = serde_json::json!({ "root": dir.wire(), "rel": typed, "single": true });
    super::source::ask_coded(line, origin, CMD_CREATE, &args, WRITE_BUDGET)
        .await
        .map(|_| ())
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
    /// 正摆着的新建空文件那一格（就地；判据看同一个值）。
    pub fn new_file_prompt(&self) -> Option<&super::writeops::WritePrompt> {
        self.write_prompt()
            .filter(|p| matches!(p.kind, super::writeops::PromptKind::NewFile))
    }

    /// 那一格里正在编辑的字。
    pub fn new_file_text_mut(&mut self) -> Option<&mut String> {
        self.write_prompt_mut()
            .filter(|p| matches!(p.kind, super::writeops::PromptKind::NewFile))
            .map(|p| &mut p.text)
    }

    /// 「新建 ▾ → 空文件」⇒ 列表里冒出一行「新建文件」、名字选中（同新建文件夹那一格）。
    pub fn begin_new_file(&mut self) -> bool {
        self.begin_inline(super::writeops::WritePrompt::for_new(&self.cwd, true))
    }

    /// 收掉那一格，什么都不建。
    pub fn cancel_new_file(&mut self) {
        if self.new_file_prompt().is_some() {
            self.cancel_write();
        }
    }

    /// 那一格答完 ⇒ 发 `files-create`（填错 ⇒ 那一格下面说、留着；名字被占了 ⇒「x 已存在」）。
    pub fn confirm_new_file(&mut self, ctx: Option<egui::Context>) -> bool {
        if self.new_file_prompt().is_none() {
            return false;
        }
        self.commit_inline(ctx)
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/create_tests.rs"]
mod tests;
