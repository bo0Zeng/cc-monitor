//! 命令表 · 计划：读面 `plan-list` · `plan-read` · `plan-cell-view`（planned-build 的工作区 · 仓库 · 一个节点的 agent 视角；只读）；
//! 反查 `plan-files`（文件窗口：目录里每份文件归哪一格）；代敲 `plan-command`（以人的身份跑 pb 的 `continue` · `pause` · `adopt` · `render` · `repo`；写盘的是 pb）；
//! 新计划框 `plan-new`（能选的领域 · 落在哪 · 要敲的那一行；只起 `pb dump --domains`）；
//! 校验 `plan-pb-check`（一个路径能不能当 pb 用）；认消息里的编号 `plan-refs`（认编号交 pb 的 `dump --refs`）；
//! 审面 `plan-ack` · `plan-unack` · `plan-return`（认可与退回记在后端自己的 `~/.cc-monitor/plan-review.json`，计划仓一个字节不写）。

use crate::wire::spec::CommandSpec;
use crate::wire::typed;

pub(super) const SPECS: &[CommandSpec] = &[
    // 读面三条都在阻塞档：起一次 pb（研究盘 252 格 1.1 秒），开跑之后打不断 ⇒ `cancel` 命中时回 `not_cancellable`。
    typed::blocking::<crate::faces::plan_face::PlanListCmd>(
        "这台的 pb 工作区与片（目录 ＝ 活会话的工作目录 ∪ `dirs`，pb 自己往上找工作区）",
    ),
    typed::blocking::<crate::faces::plan_face::PlanReadCmd>(
        "一个工作区的成品（几个仓库的图 · 状态 · 权力范围 · 校验；负责人与 signer 对到会话；不带 agent_view）",
    ),
    typed::blocking::<crate::faces::plan_face::PlanCellViewCmd>(
        "一个节点的 agent 视角（agent 站在那个节点时 pb 印给它的那一段，原样；点到才起 `pb dump --node`）",
    ),
    // 代敲也在阻塞档：起一次 pb（`render` 要画整张图）。写盘的是 pb（工作区配置 · 系统临时目录里那一页），本族不写。
    typed::blocking::<crate::faces::plan_face::PlanCommandCmd>(
        "以人的身份代敲 pb 的命令：`continue` · `pause`（开关自动继续，管整个工作区）· `adopt`（接过 `slice` 那个仓库，不是当前仓库先切过去）· `render`（pb 画整张图写进系统临时目录，回页面路径）· `repo`（在 `workspace` 那个目录登记 `path` 那一片，带 `domain`；撞上已有的一片 ⇒ 只带名字切过去）；这一侧带着 agent 身份 ⇒ 用户命令不代敲",
    ),
    // 新计划框：起一次 `pb dump --domains`（只读、不写盘）⇒ 阻塞档。
    typed::blocking::<crate::faces::plan_face::PlanNewCmd>(
        "新计划框问的那一份：那个目录能选的领域（`pb dump --domains`）· pb 往上找到的工作区 · 仓库落在哪 · 路径最后一段撞上的那一片 · 要敲的那一行 `pb repo …`；pb 不在 · 太旧写在回包里",
    ),
    typed::blocking::<crate::faces::plan_face::PlanPbCheckCmd>(
        "一个路径能不能当 pb 用（入口在 · 起得来），回给人看的那一句；设置里存 pb 路径之前问",
    ),
    // 反查只读本子里那一份（不起 pb），照样放阻塞档（读认可那份小文件）。
    typed::blocking::<crate::faces::plan_face::PlanFilesCmd>(
        "文件窗口反查：这个目录落在哪一片的仓库里、每份文件归哪一格（只看读好过的工作区，不起 pb）",
    ),
    // 认消息里的编号：认片只读本子（不起 pb），认出了起一次 `pb dump --refs`（正文经标准输入）⇒ 阻塞档。
    typed::blocking::<crate::faces::plan_face::PlanRefsCmd>(
        "会话消息里提到的格：照会话接手的那块（否则工作目录所在的那片）认片，几段正文一次交 pb 的 `dump --refs`，回每段的几处（UTF-16 下标 ＋ 跟在编号后面的标题）；认不出片 · pb 不在 ⇒ 每段空",
    ),
    // 审面三条也在阻塞档：认可读—改—写后端自己那份小文件（跨进程锁）；退回现读一次计划（起 pb）再投进那条会话的输入队（写自家状态）。
    typed::blocking::<crate::faces::plan_review_face::PlanAckCmd>(
        "认可一条要你看（只记在 cc-monitor；键带条目版本，版本换了那一条再出）；推一帧 `changed {plan}` 带新的数",
    ),
    typed::blocking::<crate::faces::plan_review_face::PlanUnackCmd>(
        "撤掉一条认可（没有也不算错）；推一帧 `changed {plan}` 带新的数",
    ),
    typed::blocking::<crate::faces::plan_return_face::PlanReturnCmd>(
        "把人的话投给负责那一格的会话：后端拼「人 · {编号} {标题}：{原话}」，认不出 ⇒ 只给复制，已结束 ⇒ 只给复制或（`resume`）照投（投递时在这台恢复它），能投就投进它的输入队；排进去了记一条已退回",
    ),
];
