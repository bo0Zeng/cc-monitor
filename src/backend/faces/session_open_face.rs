//! 帧面 `session-open` 的宿主壳：恢复那一条的记录（哪一家 · 目录 · 模型 · 权限档）· 分叉源会话此刻在不在 tmux 里 · 写分支记录 ·
//! 起之前定 sid 写轮换那几下都住观测层与这台的家，由这层读好交给 `control/session_open`；起会话要的那几样同批量那一份（`session_batch_face`）。

use crate::control::session_new::{ForkWrite, PreRotation};
use crate::control::session_open::{self as open, Facts, ResumeOf};
use cmd_wire::session::{ForkSlot, ForkTerminal};

/// `session-open`：起新的 · 恢复某条 · 接回，一个请求。阻塞档；写自家状态（起之前写好打开哪一份轮换配置）⇒ 写句柄由分派那一层递。
pub(crate) struct SessionOpen;

impl crate::wire::typed::Command for SessionOpen {
    const NAME: &'static str = "session-open";
    type Req = open::SessionOpenReq;
    type Reply = open::Opened;
    type Err = open::SessionOpenErr;
}

impl crate::wire::typed::Handle for SessionOpen {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: open::SessionOpenReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<open::Opened, open::SessionOpenErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);
            answer(req, files, own, &crate::platform::fs::NewFile::issue())
                .map_err(open::SessionOpenErr)
        }
    }
}

fn answer(
    req: open::SessionOpenReq,
    files: super::launch_face::Files,
    own: &crate::platform::fs::Own,
    nf: &crate::platform::fs::NewFile,
) -> Result<open::Opened, open::Failed> {
    let client = crate::control::gate::requester(req.client.as_deref())
        .map_err(|(c, m)| open::fail(c, m, None))?;
    let (agent, facts) = match &req.what {
        open::Open::New(n) => {
            let fork_tmux = n
                .fork
                .as_ref()
                .filter(|_| req.run.is_none())
                .and_then(|f| fork_in_tmux(&f.sid));
            (
                n.agent.as_str().to_string(),
                Facts {
                    resume: None,
                    fork_tmux,
                },
            )
        }
        open::Open::Resume { sid } => {
            let resume = resume_of(sid);
            let agent = resume
                .as_ref()
                .map_or_else(String::new, |r| r.agent.clone());
            (
                agent,
                Facts {
                    resume,
                    fork_tmux: None,
                },
            )
        }
        open::Open::Look { .. } | open::Open::Shell { .. } => (
            String::new(),
            Facts {
                resume: None,
                fork_tmux: None,
            },
        ),
    };
    with_launch_ctx(own, nf, |fork, pre, home| {
        super::session_batch_face::with_deps_as(client.as_deref(), &agent, Some(files), |d| {
            open::answer(req, facts, d, fork, pre, home)
        })
    })
}

/// 起新会话要的那几下（写分支记录 · 先定 sid · 写 / 撤轮换）＋ 这台的家目录，交给 `f`。`session-new` 同一份。
pub(crate) fn with_launch_ctx<T>(
    own: &crate::platform::fs::Own,
    nf: &crate::platform::fs::NewFile,
    f: impl FnOnce(ForkWrite, &PreRotation, Option<&std::path::Path>) -> T,
) -> T {
    let home = crate::history::listing::agent_home();
    let fork =
        |sid: &str, uuid: &str| crate::control::fork_write::fork_for_launch(nf, &home, sid, uuid);
    let user_home = crate::platform::paths::home_dir();
    let ctx = super::rotation_face::Ctx::here();
    let sid = crate::accounts::quota::rotation::new_session_id;
    let write = |sid: &str, kind: &str, rule: &str| {
        super::rotation_face::preset_with(
            &ctx,
            own,
            sid,
            kind,
            rule,
            crate::accounts::quota::now_unix(),
        )
    };
    let forget = |sid: &str| super::rotation_face::forget_preset_with(&ctx, own, sid);
    let pre = PreRotation {
        sid: &sid,
        write: &write,
        forget: &forget,
    };
    f(&fork, &pre, user_home.as_deref())
}

/// 恢复那一条要从记录里读的：记录在这台记录树里找；哪一家 · 工作目录 · 模型 · 权限档从会话事实那一处扫描读（上限表用不着，不读）。
/// 任一找不到 ⇒ `None`（不恢复）。`session-say` 恢复那一支同一份。
pub(crate) fn resume_of(sid: &str) -> Option<ResumeOf> {
    use crate::observe::query::locate as hq;
    let home = crate::history::listing::agent_home();
    // 记录在不在交这台的会话账本（同 `history-record`：说过已结束的那条随之在「已结束 ⇄ 记录已不在」之间翻）。
    let path = hq::session_record(&home, sid);
    crate::observe::record_hook::seen(sid, path.is_ok());
    let path = path.ok()?;
    let path = path.to_str()?;
    let agent = hq::facts_agent(&home, path)?;
    let cwd = hq::facts_project_dir(&home, path)?;
    let facts = hq::facts(&home, path, &Vec::new(), None).ok()?;
    Some(ResumeOf {
        agent,
        cwd,
        model: facts.usage.and_then(|u| u.model),
        permission_mode: facts.permission_mode,
    })
}

/// 分叉的源会话此刻在不在 tmux 里（同 `session-new-facts` 那一格 `fork.launch.terminal`）；说不出 ⇒ `None`。
fn fork_in_tmux(sid: &str) -> Option<bool> {
    let home = crate::history::listing::agent_home();
    let root = crate::agents::records_root(&home)?;
    let source = crate::agents::find_session_file(&root, sid).ok()?;
    match super::fork_face::launch_of(&home, &source, sid).terminal {
        ForkSlot::Known { value, .. } => Some(matches!(value, ForkTerminal::Hosted { .. })),
        ForkSlot::Unknown { .. } => None,
    }
}
