//! 帧面 `first-run`（typed）：首次运行「开始用」三步各自打没打勾，本体在 `footprint/readiness.rs`；读启动文件经文件管理面。

use crate::footprint::readiness;
use crate::stream::inbound::typed::{codes, Command, Cx, Files, WithFiles};

codes! {
    /// `first-run` 自己的码（只有入参不对那一种）。
    pub(crate) enum FirstRunErr {}
}

/// `first-run`：首次运行「开始用」三步各自打没打勾。
pub(crate) struct FirstRunCmd;

impl Command for FirstRunCmd {
    const NAME: &'static str = "first-run";
    type Req = readiness::FirstRunReq;
    type Reply = readiness::FirstRun;
    type Err = FirstRunErr;
}

impl WithFiles for FirstRunCmd {
    fn run(
        req: readiness::FirstRunReq,
        _cx: &Cx,
        files: Files,
    ) -> Result<readiness::FirstRun, FirstRunErr> {
        Ok(readiness::answer(files, &req))
    }
}
