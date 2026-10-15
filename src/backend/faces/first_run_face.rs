//! 帧面 `first-run`（typed）：首次运行「开始用」三步各自打没打勾，本体在 `footprint/readiness.rs`；读启动文件经文件管理面。

use crate::footprint::readiness;
use crate::wire::typed::{codes, Command};

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

impl crate::wire::typed::Handle for FirstRunCmd {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: readiness::FirstRunReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<readiness::FirstRun, FirstRunErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            Ok(readiness::answer(files, &req))
        }
    }
}
