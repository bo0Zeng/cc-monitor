//! 本进程里几条帧命令对后端其余部分开的门：资产域够用户文件那一扇（`stream/entry.rs` 起的那几样后台事也递它）· 两台之间枢纽问这台自己那一跳。

use super::spec::{invoke, Prepared};
use super::REGISTRY;
use crate::common::door::Door;
use crate::common::said::Said;
use crate::wire::Request;
use cmd_wire::files as wire;

/// 资产域（`assets/`）够用户文件的那一扇门：这台文件管理那一族的本体，每一问按入参类型直接问它那一条
/// （读的经 `files_face::in_process`，写的经 `files_write_face::in_process`：同那几条帧命令的本体、同一份线上类型）。
/// 住入方向是因为文件管理面的写句柄（`Face`）只有分派这一层造：写的那几问经 `spec::with_face` 领一份、只活这一问；
/// 资产模块只拿这个门，拿不到句柄，也不直呼 `files::write`。
#[derive(Clone, Copy)]
pub(crate) struct LocalFiles;

impl Door for LocalFiles {
    fn home(&self, req: wire::HomeReq) -> Result<wire::FilesPathReply, Said> {
        super::files_face::in_process(req)
    }
    fn stat(&self, req: wire::StatReq) -> Result<wire::StatReply, Said> {
        super::files_face::in_process(req)
    }
    fn ls(&self, req: wire::LsReq) -> Result<wire::LsReply, Said> {
        super::files_face::in_process(req)
    }
    fn peek(&self, req: wire::FilesPeekReq) -> Result<wire::FilesPeekReply, Said> {
        super::files_write_face::in_process(req)
    }
    fn put(&self, req: wire::FilesPutReq) -> Result<wire::FilesPutReply, Said> {
        super::files_write_face::in_process(req)
    }
    fn delete(&self, req: wire::FilesDeleteReq) -> Result<cmd_wire::batch::BatchReply, Said> {
        super::files_write_face::in_process(req)
    }
    fn copy(&self, req: wire::FilesCopyReq) -> Result<cmd_wire::batch::BatchReply, Said> {
        super::files_write_face::in_process(req)
    }
    fn mkdir(&self, req: wire::FilesMkdirReq) -> Result<wire::FilesPathReply, Said> {
        super::files_write_face::in_process(req)
    }
    fn link(&self, req: wire::FilesLinkReq) -> Result<wire::FilesPathReply, Said> {
        super::files_write_face::in_process(req)
    }
    fn rename_to(&self, req: wire::FilesRenameReq) -> Result<wire::FilesPathReply, Said> {
        super::files_write_face::in_process(req)
    }
    fn chmod(&self, req: wire::FilesChmodReq) -> Result<cmd_wire::batch::BatchReply, Said> {
        super::files_write_face::in_process(req)
    }
}

/// 两台之间那几件的枢纽问**这台自己**的那一跳：本进程那几条内层命令本身（阻塞档，原样调它们的 `run`）。
/// 限 `assets::hub_inner::NAMES` 那几条 —— 枢纽不是一扇通到任意命令的门。
pub(crate) struct LocalFrames {
    /// 引起这一趟的请求信封里的时区（内层命令照它写「几点」，问别台时转过去）。
    tz: crate::common::time::Tz,
    /// 写自家状态的句柄：内层那几条里写装记录的（`BlockingOwn`）要它。由引起这一趟的那条命令（它自己就是 `AsyncOwning`）递进来。
    own: crate::platform::Own,
}

impl crate::wire::typed::Here for LocalFrames {
    fn ask(&self, cmd: &str, args: serde_json::Value) -> Result<serde_json::Value, Said> {
        let spec = REGISTRY
            .iter()
            .find(|s| s.name == cmd && crate::assets::hub_inner::NAMES.contains(&s.name))
            .ok_or_else(|| Said::new("unknown_command", cmd.to_string()))?;
        let req = Request {
            id: "in-process".to_string(),
            cmd: cmd.to_string(),
            args,
            tz: self.tz.clone(),
            ..Default::default()
        };
        // 内层那几条都在阻塞档（读 · 带码的读 · 写装记录）；枢纽在阻塞线程上问，异步档不问。
        let Some(Prepared::Blocking(run)) = invoke(spec.run, &super::hands(Some(&self.own))) else {
            return Err(Said::new("unknown_command", cmd.to_string()));
        };
        run(req).map(|v| v.unwrap_or(serde_json::Value::Null))
    }

    fn viewer_tz(&self) -> crate::common::time::Tz {
        self.tz.clone()
    }
}

/// `tz` ＝ 引起这一趟的那条请求的信封 `tz`；`own` ＝ 分派递给那条命令的写句柄。
pub(super) fn hub_here(
    tz: &crate::common::time::Tz,
    own: &crate::platform::Own,
) -> std::sync::Arc<dyn crate::wire::typed::Here> {
    std::sync::Arc::new(LocalFrames {
        tz: tz.clone(),
        own: own.clone(),
    })
}
