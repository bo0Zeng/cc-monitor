//! 计划读面的线上形状：`plan-list` · `plan-read` · `plan-command` 等的回包，与计划反查 `plan-files` 的回包
//! （这个目录落在哪一片的仓库里、每份文件归哪一格、哪几份无主）。
//! 几端同一份：后端成品（`plan::product` · `book` · `needs` · `review` · `owners`）直接造这里的类型、帧面照原样交出去；
//! 界面的类型由这里经 ts-rs 生成，文件窗口与手机嵌的核心按这里的类型解（不手解 JSON）。
//!
//! 类型名与字段名用片 · 块 · 格（＝ pb 的仓库 · 权力范围 · 节点）；给人看的字照 pb 的叫法，在文案表里。

use serde::{Deserialize, Serialize};

/// 一个节点算不算活、签收了没有（后端 `plan::product` 照领域表算：收的值 ＝ 表里 update 规则加 · 去的那个值，
/// 节点的类许记它 ⇒ 算活）：记着它 ⇒ `Done`，没记 ⇒ `Open`；类不许记它（测试 · 规矩 · 原话 …）⇒ 字段为 `null`。
/// TS 那一侧在字段上写成字面量并集（不另出一份生成文件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanStatusCode {
    Done,
    Open,
}

/// 一格画哪一种（图标 · 颜色），后端照状态码与待定判好：`Done` · `Open` 同 [`PlanStatusCode`]；
/// 待定还没人定 ⇒ `Undecided`；类不许记收的值 ⇒ `None`（不算活，画淡的）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum PlanLook {
    Done,
    Open,
    Undecided,
    None,
}

/// 一份文件的对账码（pb 的 `file_check`：present · missing · empty · invalid）。TS 那一侧同上，写在字段上。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanFileState {
    Ok,
    Missing,
    Empty,
    Broken,
}

/// `plan-files` 里一份文件（文件窗口「格」一列与预览头那一块）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanFileOwner {
    /// 文件名（目录里的那一项）。
    pub name: String,
    /// 作主的那一格（排期上先声明它的那一格）。
    pub id: String,
    /// 「编号 标题」（没有标题 ⇒ 只写编号），后端拼好。
    pub label: String,
    pub title: Option<String>,
    /// 类不许记收的值 ⇒ `null`（不算活）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"done\" | \"open\" | null"))]
    pub status_code: Option<PlanStatusCode>,
    /// 状态那一行字（后端照 pb 记着的值写好，如「已签收 2」）；什么都没记 ⇒ `null`。
    pub status: Option<String>,
    /// 那一格住的那一块（块根格的标题）；住顶块 ⇒ `null`。
    pub block: Option<String>,
    /// 对账码；认不出 ⇒ `null`。
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"ok\" | \"missing\" | \"empty\" | \"broken\" | null")
    )]
    pub file_state: Option<PlanFileState>,
    pub file_note: Option<String>,
    /// 也声明它的那几格（pb 判据红「两个节点声明同一个文件」）。
    pub dup: Vec<PlanFileDup>,
}

/// 也声明同一份文件的另一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanFileDup {
    pub id: String,
    /// 「编号 标题」，后端拼好。
    pub label: String,
    pub title: Option<String>,
}

/// `plan-files` 的入参：文件窗口在看的那个目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanFilesReq {
    /// 那台机器上的绝对路径。
    pub dir: String,
}

/// `plan-files` 的回包：这个目录落在哪一片的仓库里（不在 ⇒ `slice: null`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanFiles {
    pub workspace: Option<String>,
    pub slice: Option<String>,
    /// 那一片此刻读不成的那一句（条目空）。
    pub unreadable: Option<String>,
    pub entries: Vec<PlanFileOwner>,
    /// 这个目录里无主的那几份（文件名）；不在任何一片里 · 那一片读不成 · 仓库读不出 ⇒ `null`。
    pub unowned: Option<Vec<String>>,
    /// 仓库读不出、判不了无主的那一句（pb 原样）；判得了 ⇒ `null`。
    #[serde(rename = "unownedWhy")]
    pub unowned_why: Option<String>,
}

// ───────────────────────────── 计划读面（plan-list · plan-read · plan-command …）─────────────────────────────

/// 一个 id（权力范围的负责人 · 节点的负责人 · 签它的）对到的会话。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanWho {
    pub id: String,
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"session\" | \"subagent\" | \"unknown\"")
    )]
    pub kind: String,
    /// 主会话 ⇒ 它自己；子 agent ⇒ 它所在的主会话；认不出 ⇒ `null`。
    pub sid: Option<String>,
    /// 在等你时等的是哪一类（批准 · 回答 · 计划 · 说不清）。
    pub needs: Option<String>,
    /// 此刻那条会话的整份脸（认不出 ⇒ 状态不明）。
    pub face: crate::frame::SessionFace,
    /// 写好的那一个名字：子 agent ⇒「{主会话} 的 agent」；认不出 ⇒ 认不出那一句（id 截成前后几位）；
    /// 会话本身 ⇒ `null`（两端写那条会话的名字）。
    pub label: Option<String>,
}

/// 一片随数据变的那几句（后端照这台的文案表写好）：桌面整页 · 手机一屏一层摆的是同一份，两端不拼。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanSliceSaid {
    /// 读不成 ＋ 过期那一句：整份读不成在先，这一片读不成在后；都没有 ⇒ `null`。
    pub strip: Option<String>,
    /// 这一片读不成、只剩名字那一句；读得成 ⇒ `null`。
    pub bare: Option<String>,
    /// 读到的时刻那一句；时刻的字还没写 ⇒ `null`。
    pub read_at: Option<String>,
    /// 顶层签了几格（总览头）；没有收的值 ⇒ `null`。
    pub top: Option<String>,
    /// 大纲根那一行右端的「几/几」；没有收的值 ⇒ `null`。
    pub root: Option<String>,
}

/// 一块随数据变的那两句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanBlockSaid {
    /// 块名：最上面那一块 ⇒ 顶层那一句；否则根节点的标题（没有 ⇒ 根节点编号 ⇒ 块编号）。
    pub name: String,
    /// 根节点底下签了几格（块表「里面」那一格 · 手机此会话那一块）；`inside` 为 `null` ⇒ `null`。
    pub inside: Option<String>,
}

/// 一格随数据变的那两句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanCellSaid {
    /// 上面几层的「编号 标题」，从顶层往下连好；顶层 ⇒ `null`。
    pub trail: Option<String>,
    /// 它是一块的根节点 ⇒ 那一句（插它的「编号 标题」）；不是 ⇒ `null`。
    pub block_root: Option<String>,
}

/// 会话接手的那一块随数据变的那两句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanSessionSaid {
    /// 块名：同那一块的 [`PlanBlockSaid::name`]。
    pub name: String,
    /// 会话头那一枚标：片 · 块名 · 阶段。
    pub mark: String,
}

/// 一个节点管的一份文件（pb 的 `file_check`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanFile {
    pub path: String,
    /// 对账的码；pb 给了认不得的 ⇒ `null`。
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"ok\" | \"missing\" | \"empty\" | \"broken\" | null")
    )]
    pub state_code: Option<PlanFileState>,
    /// pb 的那一句（长度 · 坏在哪）。
    pub note: Option<String>,
    /// 文件核对那一格写的字（在 · 缺 · 空 · 坏，后端照码写好；认不出的码 ⇒ `null`）。
    pub check_text: Option<String>,
    /// 同一份文件还挂在哪几格（编号）。
    pub also_by: Vec<String>,
}

/// 话里提到的格：`[start, end)` 按 UTF-16 下标，`ids` 是那几格（简写已展开）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanRef {
    pub start: u32,
    pub end: u32,
    pub ids: Vec<String>,
}

/// 标题与正文里各提到哪几格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanRefs {
    pub title: Vec<PlanRef>,
    pub body: Vec<PlanRef>,
}

/// 状态里记着的一个值与它的笔数（pb 原样）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanState {
    pub value: String,
    pub count: u32,
}

/// 一格自己写的、还没执行的 sets。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanSetsTo {
    pub target: Option<String>,
    pub value: Option<String>,
    /// 加一笔 · 去一笔。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"add\" | \"remove\""))]
    pub op: String,
}

/// 一片里还没执行的一条 sets：来处 · 目标 · 值 · 加还是去 · 为什么没执行（查都过了 ⇒ `null`：下一趟那个权力范围 update 就执行）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanPendingSet {
    pub source: Option<String>,
    pub target: Option<String>,
    pub value: Option<String>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"add\" | \"remove\""))]
    pub op: String,
    /// pb 那一句。
    pub blocked_reason: Option<String>,
    /// pb 给程序的代号（`source_delegated` …）。
    pub blocked_code: Option<String>,
}

/// 划出了、还没建的权力范围（下一趟 update 建）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanUnbuilt {
    pub id: Option<String>,
    pub cells: Vec<String>,
}

/// 无主文件：`paths` 为 null ⇒ 仓库读不出、判不了（`why` 说为什么），不是一份都没有。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanUnowned {
    pub paths: Option<Vec<String>>,
    pub why: Option<String>,
}

/// 能选的一档领域（`pb repo` 认得的）：类 · 阶段；表读不成 ⇒ `error`、两样空。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanDomain {
    pub name: Option<String>,
    pub error: Option<String>,
    pub kinds: Vec<PlanKind>,
    pub phases: Vec<PlanPhase>,
}

/// 画线的四种边（指向 · 连着 · 排在后面 · 替换），各一列编号。covers 画圈、sets 画虚线、file 列在格上，不在这里。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanEdges {
    pub to: Vec<String>,
    #[serde(rename = "with")]
    #[cfg_attr(any(test, feature = "ts"), ts(rename = "with"))]
    pub with_: Vec<String>,
    pub after: Vec<String>,
    pub replaces: Vec<String>,
}

/// 没签收的原因（后端照 dump 数好）：叶子没签 · 底下 d/m · 等上一级执行交回的那一条 · 待定还没人定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum PlanWhyCode {
    Nosign,
    Inside { done: u32, of: u32 },
    Upper,
    Undecided,
}

/// 行尾那一个原因短词（写好的字）；`wait` ⇒ 等上一级（用正文色）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanWhyShort {
    pub text: String,
    pub wait: bool,
}

/// 一格底下算活的直接子里签收了几格 / 一共几格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanKids {
    pub done: u32,
    pub of: u32,
}

/// 退回落地时多出来的那一格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanReturnedChild {
    pub id: String,
    pub title: Option<String>,
}

/// 一格退回过的状态（后端记、后端判落地）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanReturned {
    /// 退回那一刻（epoch ms）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at_text: Option<String>,
    pub to: Option<PlanWho>,
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"returned\" | \"unsure\" | \"landed\"")
    )]
    pub state: String,
    /// 落地的那一种：底下多出新格 · 正文改了。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"child\" | \"body\" | null"))]
    pub by: Option<String>,
    pub child: Option<PlanReturnedChild>,
}

/// 一格（pb 的一个节点；只收 permanent 那一种图的）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanCell {
    pub id: String,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub body: Option<String>,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub edges: PlanEdges,
    pub pointed_by: PlanEdges,
    pub files: Vec<PlanFile>,
    /// 状态里记着的值，一个值一项带笔数（pb 原样，照类上登的先后）。
    pub state: Vec<PlanState>,
    /// 状态那一行字（「已签收 2 · decided」，同 `pb render`）；什么都没记 ⇒ `null`。
    pub state_line: Option<String>,
    /// 依据它的是哪几格（pb 给的）。
    pub evidenced_by: Vec<String>,
    /// 算活的：记着收的值 ⇒ `done`，没记 ⇒ `open`；类不许记收的值 ⇒ `null`（不进任何分母）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"done\" | \"open\" | null"))]
    pub status_code: Option<PlanStatusCode>,
    /// 类是待定、还没记 decided。
    pub undecided: bool,
    /// 画哪一种（图标 · 颜色）：照上面两格判好，两端照它挑。
    pub look: PlanLook,
    /// 行右端那一句：状态那一行 ＋ 原因短词，用分隔连好；都没有 ⇒ `null`。
    pub end: Option<String>,
    /// 状态那一节什么都没有（没记值、也没有指着它没执行的 sets）时那一句：算活、底下有格 ⇒ 跟着底下走；别的 ⇒ 没记状态。
    /// 有东西可摆 ⇒ `null`。
    pub quiet: Option<String>,
    /// 没签收（或待定没人定）的原因；签收了 · 不算活 ⇒ `null`。
    pub why_code: Option<PlanWhyCode>,
    /// 行尾那一个原因短词；没有原因 ⇒ `null`。
    pub why_short: Option<PlanWhyShort>,
    /// 它自己写的、还没执行的 sets。
    pub sets: Vec<PlanSetsTo>,
    /// 在它底下加节点要在哪个权力范围里做（pb 的 `children_in`；最上面那一个 ⇒ `project`）。
    pub block: Option<String>,
    /// 那个权力范围的负责人（要改它，该说给的那一位）。
    pub owner: Option<PlanWho>,
    /// 写得动它元数据的那一位（pb 的 `signer`）。
    pub signer: Option<PlanWho>,
    /// 它写的 covers（格的编号或 `project`）。
    pub covers: Vec<String>,
    /// covers 管着它的那几格（pb 算的）。
    pub covered_by: Vec<String>,
    pub refs: PlanRefs,
    /// 退回过 ⇒ 那一次的状态；没有 ⇒ `null`。
    #[serde(default)]
    pub returned: Option<PlanReturned>,
    pub said: PlanCellSaid,
}

/// 要你看的一条（后端判的四种）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanNeed {
    pub key: String,
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"top\" | \"red\" | \"ended\" | \"ask\"")
    )]
    pub kind: String,
    pub block: Option<String>,
    pub cell: Option<String>,
    /// 那一条牵着的会话（问人 · 停了的那一个）。
    pub sid: Option<String>,
    /// 不通过那一条在 `check.red` 里的位置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub red: Option<u32>,
    pub acked: bool,
    /// 那一条的标题（后端照种类写好）。
    pub title: String,
    /// 标题底下那几行（顶层签了几格 · pb 那一句与修法 · 底下签了几格 · 子 agent 由主会话收）；负责人那一枚由出口挂，不在这里。
    pub lines: Vec<String>,
    /// 「第几条 / 共几条」（照认可记录标过之后、没认可的那几条照先后写）；认可过的 ⇒ `null`。
    pub pos: Option<String>,
}

/// 建了的一个权力范围（pb 的 `scopes[]`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanBlock {
    pub id: String,
    /// 根节点（最上面那一个 ⇒ `project`）。
    pub cells: Vec<String>,
    /// 根节点底下算活的直接子签收了几格；根节点不在片里 · 底下没有算活的 ⇒ `null`。
    pub inside: Option<PlanKids>,
    pub dir: Option<String>,
    pub owner: Option<PlanWho>,
    pub phase: Option<String>,
    /// 站在哪一格（编号）；`null` ⇒ 起点。
    pub at: Option<String>,
    /// 划出它的那一项还在（false ⇒ 删了，下一趟 update 并回）。
    pub listed: bool,
    /// 交接说明（pb 原样，不带认领码：码的位置写着 `<认领码>`，句末说码在哪拿）；给不了 ⇒ `null`。
    pub handoff: Option<String>,
    /// 为什么给不了；给了 ⇒ `null`。
    pub handoff_error: Option<String>,
    /// 「新会话接手」那一格（后端判，界面照画；字在 `handoff` · `handoffError`）。
    pub takeover: PlanTakeover,
    /// 接手的会话此刻在哪几组（帧面答之前现标，见 后端 `plan::groups`；pb 不知道有组）。
    pub groups: PlanBlockGroups,
    pub said: PlanBlockSaid,
}

/// 一个权力范围的接手会话在哪几组（块表「组」那一格，字后端写好）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanBlockGroups {
    /// 组模块答了 ⇒ 那条会话在的几组（照组的先后；不在组里 ⇒ 空）；没有接手会话 · 组模块没答 ⇒ 空。
    pub groups: Vec<PlanGroup>,
    /// 那一格的字：组名照排；一组都没有（不在组里 · 没有接手会话 · 没答）⇒「—」。
    pub text: String,
    /// 组模块没答 ⇒ 那一句（「读取组失败 · …」，悬停看）；答了 · 不用问 ⇒ `null`。
    pub unread: Option<String>,
}

/// 一组：id（全局不重，界面按它筛）· 叫它什么（组模块写好的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanGroup {
    pub id: String,
    pub title: String,
}

/// 一个建了的权力范围此刻给不给「新会话接手」。还没划出（`unbuiltBlocks`）⇒ 灰：那一列本身就是这一判，不另带一格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum PlanTakeover {
    /// 出：划出那一项在、没有负责人或负责人那条会话已停；开口那句是 `handoff`。
    Open,
    /// 该出、交接说明拿不到：写 `handoffError`。
    NoLine,
    /// 不出：负责人那条会话活着。
    Running,
    /// 不出：负责人对不上这台的任何会话，说不清停没停（换掉负责人撤不回，判不了就不给）。
    Unsure,
    /// 不出：划出那一项删了，下一趟 update 并回。
    Merging,
}

/// 四种画线的边在这一类里各叫什么（pb 领域表的 `edge_labels`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanEdgeWords {
    pub to: Option<String>,
    #[serde(rename = "with")]
    #[cfg_attr(any(test, feature = "ts"), ts(rename = "with"))]
    pub with_: Option<String>,
    pub after: Option<String>,
    pub replaces: Option<String>,
}

/// 领域表里的一类。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanKind {
    pub name: Option<String>,
    pub edge_words: PlanEdgeWords,
    /// 这一类许记的状态值（照表里的先后）。
    pub values: Vec<String>,
}

/// 阶段表里的一步。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanPhase {
    pub name: Option<String>,
    pub does: Option<String>,
}

/// 校验不通过的一条（pb 的 `failures[]`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanRed {
    pub rule: Option<String>,
    pub what: Option<String>,
    /// 该哪个权力范围改。
    pub block: Option<String>,
    pub fix: Option<String>,
    /// `what` 里提到的格（UTF-16 下标）。
    pub refs: Vec<PlanRef>,
}

/// 判不了的一条。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanUndecidable {
    pub rule: Option<String>,
    pub why: Option<String>,
}

/// 读不成的一处：哪一份 · 第几行（说不出 ⇒ `null`）· 为什么。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanReadError {
    pub file: Option<String>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub line: Option<u64>,
    pub message: Option<String>,
}

/// 校验那一段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanCheck {
    pub unreadable: Vec<PlanReadError>,
    /// 元数据坏了、编号还认得出的节点：判不了，不在 `cells` 里。
    pub unparsed: Vec<String>,
    pub red: Vec<PlanRed>,
    pub undecidable: Vec<PlanUndecidable>,
}

/// 顶层进度：顶层算活的节点里签收了几个 / 一共几个。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanProgress {
    pub done: u32,
    pub of: u32,
}

/// 读不成、给的是上一次那一份：那一句 ＋ 原话 ＋ 那一份读到的时刻。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanStale {
    pub said: Option<String>,
    pub raw: Option<String>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub since: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub since_text: Option<String>,
}

/// 归档里的节点（被替换的）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanArchived {
    pub id: String,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub replaced_by: Option<String>,
}

/// 一片（pb 的一个仓库）。读不成又没读好过 ⇒ `bare`（只有头几格有字，其余是空的）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanSlice {
    pub name: String,
    pub domain: Option<String>,
    /// 仓库相对工作区根在哪（pb 给的，照用；pb 没给 ⇒ `null`）。
    pub path: Option<String>,
    pub current: bool,
    /// 只做计划：人还没 `pb adopt`（`plan-command adopt` 接）。
    pub docs_only: bool,
    /// 这一刻读不成的那一句（pb 原样）。
    pub error: Option<String>,
    pub stale: Option<PlanStale>,
    pub bare: bool,
    pub kinds: Vec<PlanKind>,
    pub phases: Vec<PlanPhase>,
    /// 收的值（领域表 update 规则加 · 去的那个，软件是「已签收」）；表里没有 ⇒ `null`（不出进度）。
    pub done_value: Option<String>,
    pub top: Vec<String>,
    /// 顶层进度；没有收的值 ⇒ `null`。
    pub progress: Option<PlanProgress>,
    /// 算活、还没签收的格数。
    pub open_cells: u32,
    pub blocks: Vec<PlanBlock>,
    /// 划出了、还没建的。
    pub unbuilt_blocks: Vec<PlanUnbuilt>,
    /// 无主文件（仓库读不出 ⇒ `paths` 为 null、`why` 说为什么）。
    pub unowned: PlanUnowned,
    /// 还没执行的 sets（画虚线，为什么写在目标那一格上）。
    pub pending_sets: Vec<PlanPendingSet>,
    pub check: PlanCheck,
    /// 编号 ⇒「编号 标题」（图上每个节点，连排期的 ＋ 归档里的；后端拼，界面凡出现编号照它写）。
    pub labels: std::collections::BTreeMap<String, String>,
    pub cells: Vec<PlanCell>,
    pub archived: Vec<PlanArchived>,
    pub needs: Vec<PlanNeed>,
    /// 这一片要你看的数（没认可的，不含问人）。
    pub need_count: u32,
    /// 最上面那个权力范围（`project`）退回过 ⇒ 那一次此刻的状态；没退回过 ⇒ `null`（缺 ⇒ 同）。
    #[serde(default)]
    pub returned: Option<PlanReturned>,
    pub said: PlanSliceSaid,
}

/// 会话 ⇒ 它负责的那个权力范围（会话头那一枚标）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanSessionBlock {
    pub slice: String,
    pub block: String,
    /// 根节点（编号；点会话头那一枚标选中它）。
    pub cell: Option<String>,
    /// 根节点的标题；最上面那一个 ⇒ `null`。
    pub title: Option<String>,
    pub top: bool,
    pub phase: Option<String>,
    pub at: Option<String>,
    pub at_title: Option<String>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"session\" | \"subagent\""))]
    pub via: String,
    pub said: PlanSessionSaid,
}

/// `plan-read` 的回包：一个工作区的成品。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanRead {
    pub pb: Option<String>,
    pub workspace: String,
    /// 当前仓库（pb 的 `current_repo`）。
    pub repo: Option<String>,
    /// 自动继续开没开（pb 的 `auto_continue`）。
    pub auto: bool,
    /// 能选的领域（`pb repo` 认得的每一档）。
    pub domains: Vec<PlanDomain>,
    pub slices: Vec<PlanSlice>,
    pub by_session: std::collections::BTreeMap<String, PlanSessionBlock>,
    /// 这一份输出的摘要：变了才算计划变了。
    pub rev: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub read_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub read_at_text: Option<String>,
    /// 上次读数那一格（与 `readAtText` 同一把钟写）：那台离线时计划条照它说这一份是什么时候读到的。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub seen: Option<crate::cells::SeenAt>,
    pub stale: Option<PlanStale>,
    pub need_count: u32,
}

/// `plan-list` 里一片的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanListSlice {
    pub name: String,
    pub domain: Option<String>,
    pub current: bool,
    pub progress: Option<PlanProgress>,
    pub need_count: u32,
    pub error: Option<String>,
    pub stale: Option<PlanStale>,
    /// 同那一片成品上的那一份（照抄）。
    pub said: PlanSliceSaid,
}

/// `plan-list` 里的一个工作区。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanListWorkspace {
    pub workspace: String,
    pub repo: Option<String>,
    pub auto: bool,
    pub rev: String,
    pub stale: Option<PlanStale>,
    pub need_count: u32,
    pub by_session: std::collections::BTreeMap<String, PlanSessionBlock>,
    pub slices: Vec<PlanListSlice>,
}

/// pb 装没装、认不认得它的输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanPb {
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"ok\" | \"missing\" | \"unsupported\"")
    )]
    pub state: String,
    pub said: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub version: Option<String>,
}

/// `plan-list` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanList {
    pub pb: PlanPb,
    pub workspaces: Vec<PlanListWorkspace>,
}

/// `plan-command` 的回包：pb 的退出码 · 那一句 · `render` 写的页面路径 · `adopt` 接过来的那个仓库。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanCmdReply {
    pub rc: i32,
    pub said: Option<String>,
    pub path: Option<String>,
    /// `adopt` 才有：接过来的是哪一个仓库。
    pub adopted: Option<PlanAdopted>,
}

/// `plan-command adopt` 接过来的那个仓库。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanAdopted {
    pub slice: String,
    /// 仓库相对工作区根在哪。
    pub path: String,
    /// `true` ⇒ 先把当前仓库切成了它（pb 的 `adopt` 只接当前仓库）。
    pub switched: bool,
}

/// `plan-new` 的回包：在那个目录开一片能选哪几档 · 落在哪 · 撞没撞上 · 要敲的那一行（后端 `plan::new_slice`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanNewForm {
    /// 找不到 pb ⇒ `missing`；pb 没有 `dump --domains` ⇒ `unsupported`（两样都带那一句）。
    pub pb: PlanPb,
    /// pb 往上找到的工作区根；找不到 ⇒ `null`（`root` 就成工作区根）。
    pub workspace: Option<String>,
    /// `pb repo` 在哪个目录敲（＝ `workspace`，没有 ⇒ 问的那个目录）。
    pub root: String,
    /// 登记之后那一片叫什么（路径最后一段 ＝ 仓库名；撞上 ⇒ 那一片）；路径空 ⇒ `null`。
    pub slice: Option<String>,
    /// 能选的几档（形状同 `PlanRead.domains`）。
    pub domains: Vec<PlanDomain>,
    /// 路径最后一段撞上的那一片（领域建了不改）。
    pub existing: Option<PlanNewExisting>,
    /// 仓库会落在哪（绝对路径）；路径空 ⇒ `null`。
    pub repo_dir: Option<String>,
    /// 路径底下那一句（「仓库 …」· 撞上 ⇒「已有此片 · 软件 · 只切换」）；路径空 ⇒ `null`。
    pub note: Option<String>,
    /// 要敲的那一行；路径空 ⇒ `null`。
    pub line: Option<String>,
    /// 起 pb 没成（超时 · 读不懂 · 别的退出码）：那一句。
    pub failed: Option<String>,
}

/// 撞上的那一片：名字 · 它的领域。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanNewExisting {
    pub slice: String,
    pub domain: Option<String>,
}

/// `plan-pb-check` 的回包：这个路径能不能当 pb 用 ＋ 给人看的那一句。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanPbCheck {
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"ok\" | \"missing\" | \"no_entry\" | \"not_runnable\"")
    )]
    pub state: String,
    pub said: String,
}

/// `plan-ack` / `plan-unack` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanAckReply {
    pub acked: bool,
    pub key: String,
    /// 这个工作区此刻要人过目的数。
    pub need_count: u32,
}

/// `plan-return` 的回包：拼好的那一行（后端拼）· 给谁 · 结局。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanReturnReply {
    pub line: String,
    pub to: Option<PlanWho>,
    /// `queued`（投进那条会话的输入队、那台已落盘；投递在它能收时送，已结束的先恢复它）· `copy`（投不了，只给这一行去复制）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "\"queued\" | \"copy\""))]
    pub result: String,
    /// `copy` 的原因：`unknown`（认不出接手）· `ended`（已结束、没点恢复）。
    pub why: Option<String>,
    pub said: Option<String>,
}

/// 会话消息里提到的一处：`[start, end)` 按 UTF-16 下标（编号原样留在正文里），`label` 是跟在它后面的那串淡色字
/// （`ids` 里每个的标题照先后接好；后端写好，界面照排）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanMsgRef {
    pub start: u32,
    pub end: u32,
    pub ids: Vec<String>,
    pub label: String,
}

/// `plan-refs` 的回包：照哪一片认的（认不出 ⇒ 两样都 `null`、每段空）；`refs[i]` 对 `texts[i]`（人说的话，原文在界面手上）；
/// `docs[i]` 对入参的 `docs[i]`（回复正文的树）：有引用 ⇒ 标好引用节点的那一棵（[`crate::md::InlNode::Ref`]），没有 ⇒ `null`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct PlanMsgRefs {
    pub workspace: Option<String>,
    pub slice: Option<String>,
    pub refs: Vec<Vec<PlanMsgRef>>,
    pub docs: Vec<Option<crate::md::Doc>>,
}

#[cfg(test)]
mod said_tests {
    use super::*;
    use serde_json::{json, Value};

    /// 一份 `plan-list` 回包：片那一行与会话接手的那一块都带写好的 `said`（值是可认的夹具字）。
    fn list_json() -> Value {
        json!({
            "pb": { "state": "ok", "said": null },
            "workspaces": [{
                "workspace": "/w", "repo": null, "auto": false, "rev": "r0", "stale": null, "needCount": 0,
                "bySession": { "s1": {
                    "slice": "ledger", "block": "B1", "cell": "A1", "title": "导出", "top": false,
                    "phase": "回看", "at": null, "atTitle": null, "via": "session",
                    "said": { "name": "后端·块名", "mark": "后端·标" } } },
                "slices": [{
                    "name": "ledger", "domain": null, "current": true, "progress": null, "needCount": 0,
                    "error": null, "stale": null,
                    "said": { "strip": "后端·读不成", "bare": null, "readAt": "后端·读到", "top": "后端·顶层", "root": "后端·几/几" } }],
            }],
        })
    }

    #[test]
    fn said_round_trips_word_for_word() {
        let v = list_json();
        let back: PlanList = serde_json::from_value(v.clone()).expect("照契约的形状");
        assert_eq!(serde_json::to_value(&back).expect("序列化"), v);
    }

    /// 必填：回包缺 `said` ⇒ 解不开，报的就是那一格（两端契约对不上，不悄悄当空）。
    #[test]
    fn a_reply_without_said_does_not_decode() {
        let mut v = list_json();
        v["workspaces"][0]["slices"][0]
            .as_object_mut()
            .expect("片")
            .remove("said");
        let e = serde_json::from_value::<PlanList>(v).expect_err("缺必填格");
        assert!(e.to_string().contains("said"), "{e}");
        let mut v = list_json();
        v["workspaces"][0]["bySession"]["s1"]
            .as_object_mut()
            .expect("会话块")
            .remove("said");
        let e = serde_json::from_value::<PlanList>(v).expect_err("缺必填格");
        assert!(e.to_string().contains("said"), "{e}");
    }
}
