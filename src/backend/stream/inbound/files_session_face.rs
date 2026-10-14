//! 文件管理写面里删历史会话那一条（`files-delete-session`）：落点那两问从原生那一侧（适配层注册表）的窄口取来递给本体。
//!
//! 单住一份：引用图按文件算，这一份够得到适配层，写面其余几条那一份不该被它带上（族文件也不再放这份窄口：它点着别族的名字，会把整族判成够得到 tmux）。

use crate::platform::fs::Face;
use crate::wire::spec::Fail;
use crate::wire::typed::{self, codes, Command, Cx, Facing};
use cmd_wire::files as wire;

/// 删历史会话那一条要问的两件事，从原生那一侧（适配层注册表）的窄口取来，
/// 由这一条递给文件管理写面 —— 写面自己一家 agent 的布局都不认（`files/module_boundary_guard.rs` 围栏那一类为零）。
const SESSION_PORT: crate::control::files_write::SessionPort =
    crate::control::files_write::SessionPort {
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
    const NAME: &'static str = "files-delete-session";
    type Req = wire::FilesDeleteSessionReq;
    type Reply = wire::FilesPathReply;
    type Err = FilesDeleteSessionErr;
}

impl Facing for FilesDeleteSession {
    fn run(
        req: wire::FilesDeleteSessionReq,
        _cx: &Cx,
        face: &Face,
    ) -> Result<wire::FilesPathReply, FilesDeleteSessionErr> {
        crate::control::files_write::delete_session_of(face, req, &SESSION_PORT).map_err(|f| {
            typed::of_fail(
                Fail::new(f.code, f.said).with_raw(f.raw.as_deref()),
                FilesDeleteSessionErr::BadArgs,
            )
        })
    }
}
