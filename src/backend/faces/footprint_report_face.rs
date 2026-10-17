//! 「足迹」那三条成品（typed）：`footprint-report` · `data-report` · `chores-merge`（问到各台的 `data-report`、件数合成一份）。本体在 `footprint/mod.rs`（申报表 · 判定 · 重排都在 `footprint/`），
//! 这里只接上名字 · 入参 · 回包 · 码表与档：都只读 ⇒ 阻塞档；`data-report` 读别名块的候选经文件管理面 ⇒ 带它那一档。
//! 单住这份文件：`data-report`（`chores-merge` 答本机那一台也是它）问这台的 tmux（`control::terminals::tmux_here`），引用图按文件算，与别的放一起会把它们也判成「够得着 tmux」。

use crate::footprint::data::DataReport;
use crate::footprint::rows::ConfigSurfaceReport;
use crate::footprint::FootprintReq;
use crate::wire::typed::{self, codes, Command};

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

impl crate::wire::typed::Handle for FootprintCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: FootprintReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ConfigSurfaceReport, FootprintErr>> + Send + 'static
    {
        async move { crate::footprint::answer(&req).map_err(typed::of_pair(FootprintErr::Failed)) }
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

impl crate::wire::typed::Handle for DataReportCmd {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: FootprintReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<DataReport, FootprintErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            crate::footprint::data_answer(files, &req).map_err(typed::of_pair(FootprintErr::Failed))
        }
    }
}

/// `chores-merge` 的入参：问哪几台（本机逐字 `"<local>"`）· 本机那一台带的 monitor 那几条事实（同 `data-report` 的 `client`）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ChoresMergeReq {
    pub(crate) machines: Vec<cmd_wire::Machine>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) client: Option<crate::footprint::FootprintClient>,
}

/// `chores-merge` 的回包：各台的件数合成的那一份（设置窗角标 · 「文件与数据」顶上 · 主窗口状态栏读它）· 每台那一份（照问的先后）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct ChoresMerged {
    pub(crate) total: crate::footprint::chores::ChoreCounts,
    pub(crate) machines: Vec<ChoresAt>,
}

/// 一台：读成了 ⇒ 那一份 `data-report`；没问到 ⇒ 那一句（够不着 · 那台常驻不在 · 那台答了失败 …，这台的件数不计）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct ChoresAt {
    pub(crate) origin: cmd_wire::Machine,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) report: Option<DataReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) said: Option<String>,
}

codes! {
    /// `chores-merge` 自己没有别的码（哪台没问到只进那一台的 `said`）。
    pub(crate) enum ChoresMergeErr {}
}

/// `chores-merge`：本机后端自己问到各台的「文件与数据」（本机同进程答、别的台经 `--resident-call` 问它的常驻 `data-report`），
/// 件数逐格相加（`chores::merge_counts`）再答；每台那一份原样带回（「文件与数据」按台画），界面不再逐台问、不当信差。
pub(crate) struct ChoresMergeCmd;

impl Command for ChoresMergeCmd {
    const NAME: &'static str = "chores-merge";
    type Req = ChoresMergeReq;
    type Reply = ChoresMerged;
    type Err = ChoresMergeErr;
}

impl crate::wire::typed::Handle for ChoresMergeCmd {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: ChoresMergeReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ChoresMerged, ChoresMergeErr>> + Send + 'static
    {
        let (cx, files) = cx.split();
        async move {
            let here = FootprintReq {
                client: req.client.clone(),
            };
            let local = async move {
                tokio::task::spawn_blocking(move || crate::footprint::data_answer(files, &here))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|(_, said)| said))
                    .map_err(|message| crate::dial::remote_ask::Said {
                        code: None,
                        message,
                    })
            };
            chores_merge_with(
                &req.machines,
                local,
                &cx.tz,
                cx.until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
            )
            .await
        }
    }
}

/// [`ChoresMergeCmd`] 的本体（本机那一台怎么答 · 可达表 · 对面由调用方给）。
pub(crate) async fn chores_merge_with(
    machines: &[cmd_wire::Machine],
    local: impl std::future::Future<Output = Result<DataReport, crate::dial::remote_ask::Said>> + Send,
    tz: &crate::common::time::Tz,
    until: Option<crate::platform::child::Until>,
    table: &crate::dial::remote_ask::Table,
    remote: &dyn crate::dial::remote_ask::Remote,
) -> Result<ChoresMerged, ChoresMergeErr> {
    let got = crate::dial::remote_ask::ask_each::<DataReportCmd, _>(
        machines,
        &FootprintReq::default(),
        local,
        tz,
        until,
        table,
        remote,
    )
    .await
    .map_err(ChoresMergeErr::BadArgs)?;
    let total = crate::footprint::chores::merge_counts(
        got.iter()
            .filter_map(|(_, r)| r.as_ref().ok().map(|d| d.counts))
            .collect(),
    );
    let machines = got
        .into_iter()
        .map(|(origin, r)| match r {
            Ok(report) => ChoresAt {
                origin,
                report: Some(report),
                said: None,
            },
            Err(s) => ChoresAt {
                origin,
                report: None,
                said: Some(s.message),
            },
        })
        .collect();
    Ok(ChoresMerged { total, machines })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/chores_merge_tests.rs"]
mod chores_merge_tests;
