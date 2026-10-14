//! 「足迹」那两条成品（typed）：`footprint-report` · `data-report`。本体在 `footprint/mod.rs`（申报表 · 判定 · 重排都在 `footprint/`），
//! 这里只接上名字 · 入参 · 回包 · 码表与档：都只读 ⇒ 阻塞档；`data-report` 读别名块的候选经文件管理面 ⇒ 带它那一档。
//! 单住这份文件：`data-report` 问这台的 tmux（`control::terminals::tmux_here`），引用图按文件算，与别的放一起会把它们也判成「够得着 tmux」。

use crate::footprint::data::DataReport;
use crate::footprint::rows::ConfigSurfaceReport;
use crate::footprint::FootprintReq;
use crate::wire::typed::{self, codes, Blocking, Command, Cx, Files, WithFiles};

codes! {
    /// 足迹那两条的码。
    pub(crate) enum FootprintErr {
        /// 这台的家目录解不出来（`~/…` 无从解）。
        Failed = "failed",
    }
}

/// `footprint-report`：「足迹」由这台后端出整份成品。
pub(crate) struct FootprintCmd;

impl Command for FootprintCmd {
    const NAME: &'static str = "footprint-report";
    type Req = FootprintReq;
    type Reply = ConfigSurfaceReport;
    type Err = FootprintErr;
}

impl Blocking for FootprintCmd {
    fn run(req: FootprintReq, _cx: &Cx) -> Result<ConfigSurfaceReport, FootprintErr> {
        crate::footprint::answer(&req).map_err(typed::of_pair(FootprintErr::Failed))
    }
}

/// `data-report`：「文件与数据」那一份成品。
pub(crate) struct DataReportCmd;

impl Command for DataReportCmd {
    const NAME: &'static str = "data-report";
    type Req = FootprintReq;
    type Reply = DataReport;
    type Err = FootprintErr;
}

impl WithFiles for DataReportCmd {
    fn run(req: FootprintReq, _cx: &Cx, files: Files) -> Result<DataReport, FootprintErr> {
        crate::footprint::data_answer(files, &req).map_err(typed::of_pair(FootprintErr::Failed))
    }
}
