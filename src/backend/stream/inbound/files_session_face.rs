//! 文件管理写面里删历史会话那一条（`files-delete-session`）：那一家怎么删照注册表 `SessionDelete` 那一格的形分流 ——
//! 跑那一家给的一条命令（[`run_delete`]），或删记录文件（落点那两问从原生那一侧的窄口取来递给文件管理写面）。不认哪一家。
//!
//! 单住一份：引用图按文件算，这一份够得到适配层，写面其余几条那一份不该被它带上（族文件也不再放这份窄口：它点着别族的名字，会把整族判成够得到 tmux）。

use crate::platform::child::{Child, Deadline};
use crate::wire::typed::{self, codes, Command};
use cmd_wire::files as wire;

/// 删历史会话那一条要问的两件事，从原生那一侧（适配层注册表）的窄口取来，
/// 由这一条递给文件管理写面 —— 写面自己一家 agent 的布局都不认（文件管理那一块是独立的包，引不到适配层）。
const SESSION_PORT: crate::files::write::SessionPort = crate::files::write::SessionPort {
    locate: crate::agents::locate_session_for_delete,
    is_record: crate::agents::is_session_record,
    file_name: crate::agents::session_file_name_of,
};

/// sid ⇒ 删它的那一条命令（那一家声明了跑命令那一形、又认得这个 sid）；没有 ⇒ 照删记录文件那一形走。
type ByCommand = fn(&str) -> Option<crate::agents::DeleteCommand>;

/// 那一家删一条会话的命令最多等这么久（它要改自己的状态库，不该久）；到点整组杀、照删不成回。
const DELETE_WITHIN: Deadline = Deadline::secs(30);

codes! {
    /// `files-delete-session` 的码。
    pub(crate) enum FilesDeleteSessionErr {
        /// 删不掉。
        IoFailed = "io_failed",
        /// 按 sid 找不到会话记录那一份。
        Refused = "refused",
    }
}

/// `files-delete-session`：删一份历史会话。
pub(crate) struct FilesDeleteSession;

impl Command for FilesDeleteSession {
    typed::by_call!(wire::FilesDeleteSessionReq);
    type Err = FilesDeleteSessionErr;
}

impl crate::wire::typed::Handle for FilesDeleteSession {
    type Needs = crate::wire::typed::needs::FaceOwn;
    fn run(
        req: wire::FilesDeleteSessionReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::FilesPathReply, FilesDeleteSessionErr>>
           + Send
           + 'static {
        let (_cx, (face, own)) = cx.split();
        async move {
            delete_then_forget(
                &face,
                &own,
                req,
                &SESSION_PORT,
                crate::agents::session_delete_command,
            )
        }
    }
}

/// 删那一份会话；删成了，这台那一条历史注解跟着删（删不掉只记一行：孤儿注解不害人，删会话照样算成）。
fn delete_then_forget(
    face: &crate::platform::fs::Face,
    own: &crate::platform::Own,
    req: wire::FilesDeleteSessionReq,
    port: &crate::files::write::SessionPort,
    by_command: ByCommand,
) -> Result<wire::FilesPathReply, FilesDeleteSessionErr> {
    let sid = req.sid.clone();
    let done = match by_command(&sid) {
        Some(cmd) => run_delete(&cmd)?,
        None => crate::files::write::delete_session_of(face, req, port).map_err(|f| {
            typed::of_fail(
                {
                    let code = cmd_wire::failed::Failed::code_or_fallback(f.code.clone());
                    crate::wire::spec::fail(&code, f)
                },
                FilesDeleteSessionErr::BadArgs,
            )
        })?,
    };
    if let Err(e) = crate::history::history_annotations::forget_at(own, &sid) {
        tracing::warn!("session {sid} deleted, its annotation was not: {e:?}");
    }
    Ok(done)
}

/// 跑那一家给的那一条删会话命令：退出码 0 ⇒ 删成（回那一份记录在哪）；没成 ⇒ 删不成（话里带它说的第一行，[`run_agent_command`]）。
/// 后端不碰记录文件 —— 那一家的状态跟着它自己的命令改。
fn run_delete(
    cmd: &crate::agents::DeleteCommand,
) -> Result<wire::FilesPathReply, FilesDeleteSessionErr> {
    run_agent_command(&cmd.program, &cmd.args, &cmd.env, DELETE_WITHIN).map_err(|why| {
        FilesDeleteSessionErr::IoFailed(copy_core::copy_text(
            "beFilesWrite.delete.failed",
            &[("path", &cmd.record.display().to_string()), ("why", &why)],
        ))
    })?;
    Ok(wire::FilesPathReply {
        path: crate::files::write::wire_of(&cmd.record),
    })
}

/// 起那一家给的一条命令（删会话 · 归档 · 取消归档）、等它、读退出码：0 ⇒ 成；起不来 · 到点 · 非 0 ⇒ 没成的那一句（它往 stderr 说的第一行，没说 ⇒ 退出状态）。
#[expect(
    clippy::disallowed_methods,
    reason = "起那一家自己的会话命令（程序 · 参数 · 环境都由注册表那一格给）：改的是那一家的状态，不是本后端写用户既有数据"
)]
fn run_agent_command(
    program: &str,
    args: &[String],
    env: &[(&'static str, std::ffi::OsString)],
    within: Deadline,
) -> Result<(), String> {
    let child = env
        .iter()
        .fold(Child::new(program).args(args), |c, (k, v)| c.env(k, v));
    let out = child.run(within).map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    Err(copy_core::first_line(&said).map_or_else(|| out.status.to_string(), str::to_string))
}

// ── 归档 · 取消归档 ─────────────────────────

/// 那一家归档 · 取消归档一条会话的命令最多等这么久；到点整组杀、照没成回。
const ARCHIVE_WITHIN: Deadline = Deadline::secs(30);

codes! {
    /// `files-archive-session` · `files-unarchive-session` 的码。
    pub(crate) enum FilesArchiveSessionErr {
        /// 那一家的命令起不来 · 到点 · 退出码非 0。
        IoFailed = "io_failed",
        /// 没有哪一家的归档面认得这个 sid（那一家不归档）。
        Refused = "refused",
    }
}

/// `files-archive-session`：归档一条会话。
pub(crate) struct FilesArchiveSession;

impl Command for FilesArchiveSession {
    typed::by_call!(wire::FilesArchiveSessionReq);
    type Err = FilesArchiveSessionErr;
}

impl crate::wire::typed::Handle for FilesArchiveSession {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: wire::FilesArchiveSessionReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<(), FilesArchiveSessionErr>> + Send + 'static
    {
        async move {
            archive_with(
                crate::agents::REGISTRY,
                &crate::agents::record_roots_of(crate::agents::REGISTRY),
                &req.sid,
                crate::agents::ArchiveSide::Archive,
            )
        }
    }
}

/// `files-unarchive-session`：取消归档一条会话。
pub(crate) struct FilesUnarchiveSession;

impl Command for FilesUnarchiveSession {
    typed::by_call!(wire::FilesUnarchiveSessionReq);
    type Err = FilesArchiveSessionErr;
}

impl crate::wire::typed::Handle for FilesUnarchiveSession {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: wire::FilesUnarchiveSessionReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<(), FilesArchiveSessionErr>> + Send + 'static
    {
        async move {
            archive_with(
                crate::agents::REGISTRY,
                &crate::agents::record_roots_of(crate::agents::REGISTRY),
                &req.sid,
                crate::agents::ArchiveSide::Unarchive,
            )
        }
    }
}

/// 照根表（`roots`）上找得到这个 sid 的那一家的归档面跑那一条命令（带会话所在那个号的家）：退出码 0 ⇒ 成；起不来 · 到点 · 非 0 ⇒ 没成；
/// 没有哪一家答得出 ⇒ 照实拒。后端不碰记录文件 —— 那一家的状态跟着它自己的命令改。
fn archive_with(
    registry: &[crate::agents::Adapter],
    roots: &[crate::agents::RecordRoot],
    sid: &str,
    side: crate::agents::ArchiveSide,
) -> Result<(), FilesArchiveSessionErr> {
    let Some(cmd) = crate::agents::session_archive_command_among(registry, roots, sid, side) else {
        return Err(FilesArchiveSessionErr::Refused(copy_core::copy_text(
            "beFilesWrite.archive.noFace",
            &[("id", sid)],
        )));
    };
    run_agent_command(&cmd.program, &cmd.args, &cmd.env, ARCHIVE_WITHIN).map_err(|why| {
        FilesArchiveSessionErr::IoFailed(copy_core::copy_text(
            "beFilesWrite.archive.failed",
            &[("id", sid), ("why", &why)],
        ))
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/stream/inbound/files_session_face_tests.rs"]
mod tests;
