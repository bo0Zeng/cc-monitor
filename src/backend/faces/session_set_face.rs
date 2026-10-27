//! `set-plan`（补足一组会话的预览）：判定住 `control::session_set`；这里交事实 —— 配置文件（经这台的文件管理面读）·
//! 这台此刻活着的会话（活会话表：活着的交互进程，后台任务不算）· 每条经哪条别名起的（启动台账 `aliases`）· 目录在不在。

use crate::control::session_set::{answer, Live, SetPlan, SetPlanErr, SetPlanReq};
use crate::wire::typed::{of_pair, Command};

/// 这台此刻活着的会话（交互进程、说得出会话 id 与目录的），各带台账里记的别名。
fn live_here() -> Vec<Live> {
    let table = crate::history::live_table::now(&crate::history::listing::agent_home());
    let aliases = crate::platform::paths::data_home()
        .and_then(|d| crate::history::launch_book::read_book(&d).ok())
        .map(|b| b.aliases)
        .unwrap_or_default();
    let mut out: Vec<Live> = Vec::new();
    for r in table
        .rows()
        .iter()
        .filter(|r| r.alive && !r.fact.background)
    {
        let (Some(sid), Some(cwd)) = (r.sid(), r.fact.cwd.as_deref()) else {
            continue;
        };
        // 同一条会话几个进程持着只算一个。
        if out.iter().any(|l| l.sid == sid) {
            continue;
        }
        out.push(Live {
            sid: sid.to_string(),
            cwd: cwd.to_string(),
            alias: aliases.get(sid).cloned(),
        });
    }
    out
}

/// `set-plan`：只读不写（读配置文件 · 活会话表 · 启动台账 ⇒ 阻塞档，带文件面）。
pub(crate) struct SetPlanCmd;

impl Command for SetPlanCmd {
    const NAME: &'static str = "set-plan";
    type Req = SetPlanReq;
    type Reply = SetPlan;
    type Err = SetPlanErr;
}

impl crate::wire::typed::Handle for SetPlanCmd {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: SetPlanReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<SetPlan, SetPlanErr>> + Send + 'static {
        let (_cx, files) = cx.split();
        async move {
            let store = crate::assets::aliases::load(files).map_err(SetPlanErr::Refused)?;
            let home = std::path::PathBuf::from(&store.home);
            let is_dir = |d: &str| {
                crate::platform::fs::Ro::metadata(d)
                    .map(|m| m.is_dir())
                    .unwrap_or(false)
            };
            answer(&req, &store.book, &live_here(), Some(&home), &is_dir)
                .map_err(of_pair(SetPlanErr::Refused))
        }
    }
}
