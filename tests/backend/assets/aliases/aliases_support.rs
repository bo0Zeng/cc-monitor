//! 判据用的真盘门（本包与上层判据经 `test-support` 共用）：每一问直接交这台文件管理那一块的本体
//! （同生产那扇门 `stream/inbound/doors.rs::LocalFiles` 问的那一份：读的 `files::answer`，写的 `files::write::answer`，
//! 写句柄经分派那一层的 `wire::spec::with_face` 领、只活这一问）。写的规则不在这里判（那一份住 `tests/backend/files/write_tests.rs`）。

use crate::common::door::{said_of, Door};
use crate::common::said::Said;
use cmd_wire::files as wire;
use std::path::PathBuf;

/// 读的那几问：文件管理读族的本体，失败只交码与那一句（同生产那扇门）。
fn read<A: backend_files::Ask>(req: A) -> Result<A::Reply, Said> {
    backend_files::answer(req, &Default::default())
        .map_err(|f| said_of(f.code_or_fallback(), &f.said))
}

/// 写的那几问：写面的本体，写句柄只活这一问（同生产那扇门）。
fn write<A: backend_files::write::Act>(req: A) -> Result<A::Reply, Said> {
    crate::wire::spec::with_face(|face| backend_files::write::answer(req, face))
        .map_err(|f| said_of(f.code_or_fallback(), &f.said))
}

/// 这台文件管理的本体那扇门（`home` 也照本体答：测试档的家是沙箱）。根由调用方给 ⇒ 判据交临时目录。
pub struct FilesHere;

impl Door for FilesHere {
    fn home(&self, req: wire::HomeReq) -> Result<wire::FilesPathReply, Said> {
        read(req)
    }
    fn stat(&self, req: wire::StatReq) -> Result<wire::StatReply, Said> {
        read(req)
    }
    fn ls(&self, req: wire::LsReq) -> Result<wire::LsReply, Said> {
        read(req)
    }
    fn peek(&self, req: wire::FilesPeekReq) -> Result<wire::FilesPeekReply, Said> {
        write(req)
    }
    fn put(&self, req: wire::FilesPutReq) -> Result<wire::FilesPutReply, Said> {
        write(req)
    }
    fn delete(&self, req: wire::FilesDeleteReq) -> Result<cmd_wire::batch::BatchReply, Said> {
        write(req)
    }
    fn copy(&self, req: wire::FilesCopyReq) -> Result<cmd_wire::batch::BatchReply, Said> {
        write(req)
    }
    fn mkdir(&self, req: wire::FilesMkdirReq) -> Result<wire::FilesPathReply, Said> {
        write(req)
    }
    fn link(&self, req: wire::FilesLinkReq) -> Result<wire::FilesPathReply, Said> {
        write(req)
    }
    fn rename_to(&self, req: wire::FilesRenameReq) -> Result<wire::FilesPathReply, Said> {
        write(req)
    }
    fn chmod(&self, req: wire::FilesChmodReq) -> Result<cmd_wire::batch::BatchReply, Said> {
        write(req)
    }
}

/// 落在临时目录上的门：只有 `home` 答 `.0`，其余各问原样交 [`FilesHere`] —— **绝不碰真实家目录**。
pub struct HomeDoor(pub PathBuf);

impl Door for HomeDoor {
    fn home(&self, _req: wire::HomeReq) -> Result<wire::FilesPathReply, Said> {
        Ok(wire::FilesPathReply {
            path: self.0.display().to_string().as_str().into(),
        })
    }
    fn stat(&self, req: wire::StatReq) -> Result<wire::StatReply, Said> {
        FilesHere.stat(req)
    }
    fn ls(&self, req: wire::LsReq) -> Result<wire::LsReply, Said> {
        FilesHere.ls(req)
    }
    fn peek(&self, req: wire::FilesPeekReq) -> Result<wire::FilesPeekReply, Said> {
        FilesHere.peek(req)
    }
    fn put(&self, req: wire::FilesPutReq) -> Result<wire::FilesPutReply, Said> {
        FilesHere.put(req)
    }
    fn delete(&self, req: wire::FilesDeleteReq) -> Result<cmd_wire::batch::BatchReply, Said> {
        FilesHere.delete(req)
    }
    fn copy(&self, req: wire::FilesCopyReq) -> Result<cmd_wire::batch::BatchReply, Said> {
        FilesHere.copy(req)
    }
    fn mkdir(&self, req: wire::FilesMkdirReq) -> Result<wire::FilesPathReply, Said> {
        FilesHere.mkdir(req)
    }
    fn link(&self, req: wire::FilesLinkReq) -> Result<wire::FilesPathReply, Said> {
        FilesHere.link(req)
    }
    fn rename_to(&self, req: wire::FilesRenameReq) -> Result<wire::FilesPathReply, Said> {
        FilesHere.rename_to(req)
    }
    fn chmod(&self, req: wire::FilesChmodReq) -> Result<cmd_wire::batch::BatchReply, Said> {
        FilesHere.chmod(req)
    }
}
