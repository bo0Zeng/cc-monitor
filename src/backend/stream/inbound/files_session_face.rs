//! 文件管理写面里删历史会话那一条（`files-delete-session`）：落点那两问从原生那一侧（适配层注册表）的窄口取来递给本体。
//!
//! 单住一份：引用图按文件算，这一份够得到适配层，写面其余几条那一份不该被它带上（族文件也不再放这份窄口：它点着别族的名字，会把整族判成够得到 tmux）。

use crate::wire::typed::{self, codes, Command};
use cmd_wire::files as wire;

/// 删历史会话那一条要问的两件事，从原生那一侧（适配层注册表）的窄口取来，
/// 由这一条递给文件管理写面 —— 写面自己一家 agent 的布局都不认（文件管理那一块是独立的包，引不到适配层）。
const SESSION_PORT: crate::files::write::SessionPort = crate::files::write::SessionPort {
    locate: crate::agents::locate_session_for_delete,
    is_record: crate::agents::is_session_record,
    file_name: crate::agents::session_file_name_of,
};

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
        async move { delete_then_forget(&face, &own, req, &SESSION_PORT) }
    }
}

/// 删那一份会话记录；删成了，这台那一条历史注解跟着删（删不掉只记一行：孤儿注解不害人，删会话照样算成）。
fn delete_then_forget(
    face: &crate::platform::fs::Face,
    own: &crate::platform::Own,
    req: wire::FilesDeleteSessionReq,
    port: &crate::files::write::SessionPort,
) -> Result<wire::FilesPathReply, FilesDeleteSessionErr> {
    let sid = req.sid.clone();
    let done = crate::files::write::delete_session_of(face, req, port).map_err(|f| {
        typed::of_fail(
            {
                let code = f.code_or_fallback().to_string();
                crate::wire::spec::fail(&code, f)
            },
            FilesDeleteSessionErr::BadArgs,
        )
    })?;
    if let Err(e) = crate::history::history_annotations::forget_at(own, &sid) {
        tracing::warn!("session {sid} deleted, its annotation was not: {e:?}");
    }
    Ok(done)
}

#[cfg(test)]
#[path = "../../../../tests/backend/stream/inbound/files_session_face_tests.rs"]
mod tests;
