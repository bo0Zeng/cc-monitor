//! `S2`（2026-08-14）：**agent 适配层** —— 每个 agent 一份，装它**专属**的知识。
//!
//! # 它为什么存在
//!
//! 〔用 08-14〕逐字：「**现在的后端几乎都是兼容claudecode, 那就标清楚, 分离清楚,
//! 后面搞兼容其他agent的时候才方便**」。
//!
//! 病灶不是"代码散"，是**「加一个 agent 要改哪几处」这份清单只住在人的脑子里**。
//! `S2` 开工前实测：codex 的知识在三处，而三处**长得完全不一样** ——
//! `usage_query.rs` 里是 `aggregate(claude_dir).and_then(|()| aggregate_codex())`，
//! `resolve_query.rs` 里是 `spec.agent_kind.trim() == "codex"`，
//! grep 出其中一处**找不到另一处**。接第三个 agent 的人得先把它们找出来，而没有东西会告诉他有几处。
//!
//! # 分界：**格式知识**进来，**值判别**留在通用层
//!
//! `D3` 逐字：「agent 维度只许出现在**值**里，不许出现在**字段名**里」。照它推：
//!
//! | 归这里 | 留通用层 |
//! |---|---|
//! | 会话文件长什么样（信封 / 事件名 / 用量字段在哪一层） | `agent_kind == "codex"` 这种**值**上的派发 |
//! | 会话住哪个目录、文件怎么命名 | 派发之后两边**共用**的形状（校验、错误出口、CommandPlan 骨架） |
//! | 起会话/resume 的**命令形状**与默认命令名 | |
//!
//! 两条判据钉住这个分界，都住 [`crate::agent_locality_guard`]：
//! ① 专有的格式针**只许**在 `agents/<名>/` 下出现；
//! ② 通用层里**一个 agent 名字面量都没有** —— 按哪一家做什么，只问适配层那一格（如 [`LaunchFace`]）。
//!
//! # ⚠ 两个 agent 都在了，但**故意还没有 trait**
//!
//! `D4` 逐字要求「接口由**现有能力反推**，不凭空设计」。`S3` 把 Claude 那半也搬进来之后，
//! 两个实现终于摆在一起了 —— 而**它们的形状差得比预想大**：
//! `codex/` 是 `parse`+`usage`+`resume` 三块（自带一整套记录抽取器），
//! `claudecode/` 是 `paths`+`records`+`liveness`+`accounts`+`resume` 五块（记录抽取住在通用机器里）。
//! 唯一严格对称的只有 `resume`。⇒ 现在就立 trait 会得到一个"两边都别扭"的抽象。
//! 立接口的动作留给 `L2`，由 `S6`（最小假 agent）**反过来**逼出真正需要的那几个方法。
//!
//! ⚠ 另一半如实说：那个 `claude_dir` **参数名已经清了**
//!（8 个文件；生产段 `claude_dir` 64 行 → 3 行，剩下的 3 处全是冻结的 wire 字段名；
//! `agent_locality_guard` 判据③钉住不许长回来）。
//! **但通用层仍然叫得出 agent 的名字** —— 换了个形状：8 个文件 / 27 处
//! `agents::<名>::…` 的**调用点**（`agent_locality_guard::ADAPTER_CALL_SITES` 逐条登记）。
//! 「知识收进适配层」与「通用层不再叫得出 agent 名字」是两件事，本层只做到了第一件；
//! 第二件卡在**还没有接口**（`L2`），归 `S6`。
//!
//! # **注册表**：本文件从"目录索引"变成了"这台机器认得哪几个 agent"
//!
//! [`REGISTRY`] + [`visible_homes`] 让 backend **有能力**声明它看得见哪些 agent
//! （`G1` 成功标准③）。⚠ **能力先建、生产路径今天不接** —— `main.rs` 仍硬写
//! `homes: Vec::new()`，一个线上字节都没变。理由不是保守：填 `homes` 是一次**跨仓契约变更**
//! （仓外 aterm 的 hello fixture 按精确字节对），而本机没有 aterm 仓、验不了它的运行时
//! （前提 `P3`）。⇒ 把"何时真填"留成**一次纯发布决策**：改 `main.rs` 那一行，
//! `wire::tests::production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen`
//! 当场变红，逼那一天的人重新裁一次。**那条判据是提醒，不是障碍，别删它。**
//!
//! ## 为什么注册表住这里，而不是通用层的某个新文件
//!
//! 「加一个 agent 要改哪几处」这份清单本来就**必然**包含本文件 ——
//! 下面那两行 `pub(crate) mod …` 不加，新的适配层根本编不进来。
//! 把注册表放在别处只会让必改的文件从 1 个变成 2 个。
//! ⚠ 代价如实写，以及它**为什么不能**记进 `ADAPTER_CALL_SITES`：
//! 那张表里每一条的含义是「该被压到零的耦合」，而注册表这几行**方向相反** ——
//! 它该随 agent 数增长。混进去，`S6` 就没法拿那个数当成绩。
//! ⇒ 拆成 `agent_locality_guard::AGENT_REGISTRY_SITES` 单独一张；
//! 扣出人群的**对价**是判据⑦把本文件的处数钉死成 `REGISTRY.len()`（**一家一行**），
//! 谁想把别处的直呼挪进来刷数，当场红。
//! （同轮还补了判据④的针：此前只认全路径 `agents::<名>::`，本文件写的相对路径 `codex::`
//! 一处都数不到 —— 那是本区第二次「量具的作用域比事实**小**」。）

use crate::stream::wire::AgentHome;
use std::path::{Path, PathBuf};

pub mod claudecode;
// 上游协议的流面（按协议分，不按 agent 分）。
pub(crate) mod codex;
pub(crate) mod sse_anthropic;

/// **夹具家** —— 本区验收件的最小假 agent。
///
/// ⚠ **那行 `#[cfg(test)]` 就是它与一个真 agent 的全部差别**（外加它不进 [`REGISTRY`]）：
/// 生产二进制里一个字节都没有它，真 `hello.homes` 永远不会声明它。
/// 两件事都由 `fake::tests::the_fixture_agent_never_ships` 双向钉住，
/// 登记住 `agent_locality_guard::tests::FIXTURE_HOMES`（**带天花板**——
/// 没有天花板的话「夹具家」就成了往 `agents/` 里塞东西躲判据①的逃生舱）。
#[cfg(test)]
pub(crate) mod fake;

/// 一个 agent 适配层在注册表里的样子。
///
/// ⚠ **刻意不是 trait**（`D4` 逐字「接口由现有能力反推，不凭空设计」；`S4b §2` 复述过一遍）。
/// 今天两家能对称答出来的只有"我叫什么"与"我的 home 在哪"这两问 ——
/// 那就先把这两问定成数据，剩下的等 `S6` 用最小假 agent **反过来**逼。
/// 用函数指针而不是方法，正是为了让这一步**不需要**先决定 trait 长什么样。
pub(crate) struct Adapter {
    /// wire 上的 `agent_kind` 值。**由适配层自己提供** —— 通用层里一个 agent 名的
    /// 字面量都不该有（`D3`）。
    pub(crate) kind: &'static str,
    /// **账号维度在这一家上的载体** —— 切账号靠改哪个环境变量。`None` = 这一家没有账号维度。
    ///
    /// 加这一格的理由：终端 `ccm` 面（`control/ccm/`）要设 / 清 / 读它，
    /// 而那个名字**是某一家的知识**。让通用层直接 `use agents::claudecode::paths::…`
    /// 会在 `agent_locality_guard` 的「④ 通用层直呼适配层」上凭空多出 5 处 ——
    /// 而那个数是 `G1` 成功标准②的头条数字、**只许降**。
    /// ⇒ 把它收进注册表：**加一个 agent 本来就该改的那一行**，正是这一格该在的地方。
    pub(crate) account_env: Option<&'static str>,
    /// 这个 agent 在本机的 home 目录候选。`None` = 连候选都说不出（⇒ 一定看不见）。
    ///
    /// ⚠ 它**只答"该在哪"**。"在不在"由 [`home_is_visible`] 统一判 ——
    /// 让每家自己定判准的话，「看得见一个 agent」就成了两套语义，
    /// 而 `S6` 的最小假 agent 得先猜自己该伪造哪一套。
    pub(crate) home: fn() -> Option<PathBuf>,
    /// 这一家的**资产面**（skill · 项目级 MCP 住哪、怎么认出来）。
    /// `None` = 这一家今天没有可记进资产目录的东西。
    ///
    /// ⚠ 收进注册表而不是让 `asset_catalog.rs` 直呼 `agents::<名>::` —— 理由与 [`Adapter::account_env`] 同一条：
    /// 后者会让 `agent_locality_guard` 判据④的读数（只许降）凭空上涨。
    pub(crate) assets: Option<AssetFace>,
    /// 这一家的**合成历史**面：它的会话不在「按项目目录分」的记录树里（Codex 按日期分），
    /// 历史清单由通用层把它的会话并进来。`None` = 这一家的历史走记录树那一条（Claude）或没有历史。
    ///
    /// ⚠ 收进注册表而不是让 `history_list.rs` 直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]。
    pub(crate) history: Option<HistoryFace>,
    /// 这一家的**默认上游**（中转 `/t/` 直通、表里没有那一行时发到哪）。`None` = 未登记 ⇒ 上游选择拒（404 ＋ 原因头，FIX3 之前是 502），不回落。
    ///
    /// 用户 2026-09-18 逐字「**写死, 跟着适配层**」：先前这一格住上游选择自己那张表
    /// （`accounts::upstream_select` 的每 agent 一行表），与「跟着适配层」不是同一格（自陈待对齐）。
    /// 今天上游选择**只从这里读**（`default_upstreams`）—— 加一家的默认上游，改的就是注册表里那一行。
    pub(crate) upstream: Option<DefaultUpstream>,
    /// 这一家的 **MCP 读**（server 表住哪几层、项目表住哪 —— 那一家的格式知识）。`None` = 这一家今天没有。
    /// 收进注册表而不是让帧面宿主直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub(crate) mcp: Option<McpFace>,
    /// 这一家的**足迹面**：落在它自己布局里的那几条申报 ＋ 申报路径里 `~/<它的家>/…` 怎么认 ＋ 用户级 settings 住哪。
    /// 收进注册表而不是让 `footprint/` 直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub(crate) footprint: Option<FootprintFace>,
    /// 这一家的**账号库布局**：一个身份由哪几份文件组成 · 没设账号时的配置根 · 登录邮箱在哪读。`None` = 这一家今天没有多账号。
    /// 收进注册表而不是让 `accounts/manage/` 直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub(crate) accounts: Option<AccountsFace>,
    /// 这一家的**记录解释**：一行原文在渲染模型里是什么 · sid 怎么从文件名来 · 轮次边沿 · 漂移账。
    /// 收进注册表而不是让通用层直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]（判据④的读数只许降）。
    pub(crate) records: Option<RecordFace>,
    /// tmux 前台命令（`#{pane_current_command}`）是这几个之一 ⇒ 那个 pane 跑的是这一家（从前界面按画像表自己判）。
    /// `None` ＝ 今天没人考据过。收进注册表而不是让调用方直呼 `agents::<名>::` —— 理由同 [`Adapter::assets`]。
    pub(crate) processes: Option<&'static [&'static str]>,
    /// 这一家的**起会话事实**（默认启动器 · shell wrapper · resume 字面量 · 嵌套标记）；`None` ＝ 这一家不由我们起。
    pub(crate) launch: Option<LaunchFace>,
    /// 请求压缩上下文用的那一句（送进会话所在的终端，换号重启「先压缩」那一步）；`None` ＝ 这一家不支持。
    pub(crate) compact_request: Option<&'static str>,
}

/// 〔加一个 agent 只改 `agents/`〕一家的起会话事实 —— **唯一的家**。
/// 从前住两处（monitor `adapter.rs` · 后端 `control/ccm/`，靠金样 `agent-profile-golden.tsv` 对着）；今天 `ccm` 按注册表读
/// （[`launch_face_among`]），界面与 monitor 读从这里生成的 `src/frontend/ui/generated/agent-profile-table.ts`。
// 不派生 `PartialEq`：带着一个函数指针（resume 命令形），函数地址相等不是一个有意义的比较。
#[derive(Debug, Clone, Copy)]
pub(crate) struct LaunchFace {
    /// 适配器 id（界面起会话那一发交回，上游选择按它挑那一行）。
    pub(crate) adapter_id: &'static str,
    /// 这一家对用户的叫法（「{名} 会话还不能选账号」这类话里用）。
    pub(crate) display_name: &'static str,
    /// 默认启动器（无候选时的命令基底）。
    pub(crate) default_launcher: &'static str,
    /// shell 集成 wrapper：探得到先用它，探不到回退默认启动器；没有 ⇒ `None`。
    pub(crate) launcher_alias: Option<&'static str>,
    /// resume 那个字面量：`--` 开头 ＝ flag 形（`claude --resume <sid>`），否则 ＝ 子命令形（`codex resume <sid>`）。
    pub(crate) resume_token: &'static str,
    /// `ccm` 起这一家（新起与 resume）时垫在交给它的那一串最前面的参数。
    pub(crate) launch_args: &'static [&'static str],
    /// 起之前要清掉的嵌套会话标记（顺序决定载荷字节）。
    pub(crate) nested_env: &'static [&'static str],
    /// 不说是哪一家时就起这一家（`ccm` 不给 `--agent` · resume 规格里没写或写了认不出的 `agentKind` · 载荷内核）。
    /// 注册表里恰一家为真（`agents_tests.rs` 钉着）。
    pub(crate) is_default: bool,
    /// resume 命令怎么拼：`(启动器, sid) → 整条命令`。
    pub(crate) resume_command: fn(&str, &str) -> String,
    /// resume 那个 tmux 会话名的前缀（会话名 ＝ `<前缀>-<sid 前 8 个字符>`）。
    pub(crate) session_name_prefix: &'static str,
    /// 这一家自己够不着 tmux socket ⇒ 起它时要把 cc-bus 身份（tmux 会话名）经 `CC_BUS_ID` 交进去。
    ///
    /// ⚠ 它答的是「**要不要**」，不是「**能不能**」：值恒来自 tmux 的会话名（`control::ccm::BUS_ID_RECIPE`），
    /// 没有 tmux 就没有这个值 —— 那是载体没了，不是这一家做不到。要不要请一个 `sh` 进来另由 `ccm` 的 `needs_shell` 判。
    pub(crate) needs_bus_id: bool,
    /// 这一家的会话有身份面（`@ccm_sid` 回填）。
    pub(crate) has_identity: bool,
    /// 这一家的会话留 pidfile、入口注入的那份「此刻在跑」扫描认得出来 ⇒ resume 之前先问它是不是已经在别处跑着。
    pub(crate) has_pidfiles: bool,
    /// 这一家认得的模型名（账号页「默认模型」下拉的选项；不选 ＝ 跟着这一家自己的默认）。没考据过 ⇒ `None`。
    pub(crate) models: Option<&'static [&'static str]>,
}

/// 某一家（wire 上的 kind）在给定注册表里的起会话事实。认不出 ⇒ `None`。
/// 生产走 [`REGISTRY`]；判据喂合成注册表（含夹具家），通用层因此只看这一格、不看名字。
pub(crate) fn launch_face_among(registry: &[Adapter], kind: &str) -> Option<LaunchFace> {
    registry
        .iter()
        .find(|a| a.kind == kind)
        .and_then(|a| a.launch)
}

/// 给定注册表里声明「不说是哪一家时就是我」（[`LaunchFace::is_default`]）的那一家：`(kind, 起会话事实)`。没有 ⇒ `None`。
pub(crate) fn default_launch_among(registry: &[Adapter]) -> Option<(&'static str, LaunchFace)> {
    registry
        .iter()
        .find_map(|a| a.launch.filter(|f| f.is_default).map(|f| (a.kind, f)))
}

/// 生产注册表里默认那一家的 kind。注册表恰有一家声明默认（判据钉着）；没有 ⇒ 空串（下游按认不出处理）。
pub(crate) fn default_kind() -> &'static str {
    default_launch_among(REGISTRY).map_or("", |(kind, _)| kind)
}

/// 收 agent 名字的入口用哪一家：**全仓只有这一种认法**（[`pick_kind_among`] 认 wire 上的 kind，[`pick_adapter_among`] 认适配器 id）。
///
/// - 没写 / 写空（两侧空白不算字）⇒ 注册表里声明默认（[`LaunchFace::is_default`]）的那一家；
/// - 写了注册表里由我们起的一家 ⇒ 就是它（大小写敏感）；
/// - 写了注册表里没有的名字 ⇒ 报错，那句话列出认得的那几家（从注册表取）。**不落默认**：拼错了就跑成另一家，比起不来更坏。
///
/// 回 `(kind, 起会话事实)`；`Err` 是给用户看的那一句。
fn pick_among(
    registry: &[Adapter],
    name: Option<&str>,
    name_of: fn(&'static str, &LaunchFace) -> &'static str,
) -> Result<(&'static str, LaunchFace), String> {
    let launchable = || {
        registry
            .iter()
            .filter_map(|a| a.launch.map(|f| (a.kind, f)))
    };
    let name = name.map(str::trim).unwrap_or_default();
    let hit = if name.is_empty() {
        launchable().find(|(_, f)| f.is_default)
    } else {
        launchable().find(|(k, f)| name_of(k, f) == name)
    };
    hit.ok_or_else(|| {
        let known: Vec<&str> = launchable().map(|(k, f)| name_of(k, &f)).collect();
        copy_core::copy_text(
            "beAgents.pick.unknown",
            &[("agent", name), ("known", &known.join(" / "))],
        )
    })
}

/// 按 wire 上的 kind 认（`ccm --ccm-agent` · resume 规格的 `agentKind` · cc-bus 派生的 `tool` · 起会话那一发的 `agent`）。规则见 [`pick_among`]。
pub(crate) fn pick_kind_among(
    registry: &[Adapter],
    name: Option<&str>,
) -> Result<(&'static str, LaunchFace), String> {
    pick_among(registry, name, |kind, _| kind)
}

/// 按适配器 id 认（上游选择那几问的 `agent`：`apikey-routing` · `accounts-list` · `launch-local`）。规则见 [`pick_among`]。
pub(crate) fn pick_adapter_among(
    registry: &[Adapter],
    id: Option<&str>,
) -> Result<(&'static str, LaunchFace), String> {
    pick_among(registry, id, |_, f| f.adapter_id)
}

/// [`pick_kind_among`] 在生产注册表上。
pub(crate) fn pick_kind(name: Option<&str>) -> Result<(&'static str, LaunchFace), String> {
    pick_kind_among(REGISTRY, name)
}

/// [`pick_adapter_among`] 在生产注册表上；回那一家的适配器 id。
pub(crate) fn pick_adapter(id: Option<&str>) -> Result<&'static str, String> {
    pick_adapter_among(REGISTRY, id).map(|(_, f)| f.adapter_id)
}

/// 由我们起的那几家（带 [`LaunchFace`] 的，注册表序）—— `ccm --agent` 的闭集就是它，不另写一份。
pub(crate) fn launchable_kinds() -> Vec<&'static str> {
    REGISTRY
        .iter()
        .filter(|a| a.launch.is_some())
        .map(|a| a.kind)
        .collect()
}

/// 一个 `tool_use` 在界面上画成哪一种卡 —— **通用的值域**；哪个工具名算哪一种是各家的格式知识（`agents/<名>/`，
/// 注册表 [`RecordFace::tool_card`]）。没有这一格 ＝ 普通工具卡。随记录成品带出（`JsonlRecord::Assistant` 的 `toolCards`），
/// 界面只按它画、不认工具名（判定只在后端 · `§2.1` 加一个 agent 只改 `agents/`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub enum ToolCard {
    /// 展开 ＝ 子会话（折叠卡里嵌着渲染子 agent 的记录）。
    Agent,
    /// agent 在等用户决定（默认展开，不进工具组折叠）。
    Interactive,
    /// 写类工具（参数按行级 diff 画）。
    Diff,
    /// 结果默认按 Markdown 画。
    Md,
    /// 跑一行命令的工具：入参里 `command` 是那一行、`description` 是可缺的说明 —— 展开画命令本身，不画入参 JSON。
    Command,
}

/// [`RecordFace::branch`] 的形状。
pub(crate) type BranchFn =
    fn(&[serde_json::Value], &str, &str, &str) -> Result<Vec<serde_json::Value>, String>;

/// 按 sid 找会话文件 —— 通用层（分叉 · 「记录还在不在」）按 sid 找文件的唯一入口：逐家问，谁认得算谁。
/// 没有哪一家答得了 ⇒ 照实拒（都答不出 ⇒ 第一家说的那一句）。
pub(crate) fn find_session_file(records_root: &Path, sid: &str) -> Result<PathBuf, String> {
    find_session_file_among(REGISTRY, records_root, sid)
}

/// [`find_session_file`] 的可喂夹具那一半。
pub(crate) fn find_session_file_among(
    registry: &[Adapter],
    records_root: &Path,
    sid: &str,
) -> Result<PathBuf, String> {
    let mut first_err = None;
    for find in registry.iter().filter_map(|a| a.records?.find_session) {
        match find(records_root, sid) {
            Ok(p) => return Ok(p),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err.unwrap_or_else(|| format!("no agent adapter can locate session {sid:?}")))
}

/// `kind` 那一家的分叉记录变换 —— 通用层（`control/fork_write.rs`）分叉的唯一入口。那一家答不了 ⇒ 照实拒。
pub(crate) fn build_branch_records(
    kind: &str,
    lines: &[serde_json::Value],
    message_uuid: &str,
    src_sid: &str,
    new_sid: &str,
) -> Result<Vec<serde_json::Value>, String> {
    match record_face(kind).and_then(|r| r.branch) {
        Some(f) => f(lines, message_uuid, src_sid, new_sid),
        None => Err("no agent adapter can fork a session".to_string()),
    }
}

/// 这个 tmux 前台命令是不是注册表里某一家的进程（[`Adapter::processes`]）—— 批量停 / 起认窗格时每一行的 `agent`。
pub(crate) fn is_agent_process(command: &str) -> bool {
    REGISTRY
        .iter()
        .filter_map(|a| a.processes)
        .any(|names| names.contains(&command))
}

/// 一家的记录解释面：函数指针（同 [`Adapter::home`]，不立 trait）。
#[derive(Clone, Copy)]
pub(crate) struct RecordFace {
    /// 一行原文 ⇒ 渲染模型那一条（空行 / 纯 BOM ⇒ `Ok(None)`；连 JSON 都不是 ⇒ `Err`，调用方照占号、不出成品）。
    pub(crate) parse: fn(&str) -> Result<Option<ParsedLine>, String>,
    /// 会话文件 ⇒ 它的 sid（这一家的文件命名）。
    pub(crate) sid: fn(&Path) -> Option<String>,
    /// 这一行是不是一轮的结束 ⇒ 那条记录的 uuid（`turn_end` 帧）。`None` ＝ 这一家今天不报轮次边沿。
    pub(crate) turn_end: Option<fn(&str) -> Option<String>>,
    /// 在这一家的记录树（`records_root`）下按 sid 找那份会话文件（原共享 crate `branch-core`）。`None` ＝ 这一家不按 sid 找。
    pub(crate) find_session: Option<fn(&Path, &str) -> Result<PathBuf, String>>,
    /// 分叉的记录变换：`(记录, 分叉点 uuid, 源 sid, 新 sid)` ⇒ 新会话的记录（原共享 crate `branch-core`）。`None` ＝ 这一家不分叉。
    pub(crate) branch: Option<BranchFn>,
    /// 这一家的漂移账（看不懂的记录类型记在哪）⇒ 成品；`None` ＝ 这一家不记。
    pub(crate) drift: Option<fn() -> serde_json::Value>,
    /// 这一家的记录文本面（正文 / 工具内容怎么抽 · CLI 注入怎么剥）；`None` ＝ 这一家不进搜索 / 摘录那几条通用路。
    pub(crate) text: Option<TextFace>,
    /// 删历史会话那一条的两问（按 sid 找那一份 · 它是不是一份会话记录）；`None` ＝ 这一家不删。
    pub(crate) delete: Option<SessionDelete>,
    /// 一条记录与流对账用的键（同一次上游应答写出的记录都带它）。`None` ＝ 这一家的流对不上记录（活卡不定稿，只靠收尾撤）。
    pub(crate) response_id: Option<fn(&serde_json::Value) -> Option<String>>,
    /// 这条记录属于哪个子运行（主运行的记录 ⇒ `None`）。`None` 这一格 ＝ 这一家没有子运行。
    pub(crate) run_of: Option<fn(&serde_json::Value) -> Option<RunMark>>,
    /// 父记录里说到子运行的那几条：派出（父侧工具调用 id、标签，知道时也给出子运行是哪个）· 收场（派出那一方说它完了 / 败了 / 被叫停）。
    pub(crate) child_link: Option<fn(&serde_json::Value) -> Vec<ChildLink>>,
    /// 子运行的记录住哪（由父记录路径推出）。`None` ＝ 这一家没有单独存放的子运行记录。
    pub(crate) children: Option<ChildFace>,
    /// 会话的项目目录（会话起在哪个目录）：只读记录开头（[`first_in_head`]，有上界）。`None` 这一格 ＝ 这一家的记录里没有这件事。
    pub(crate) project_dir: Option<fn(&Path) -> Option<String>>,
}

/// 一条子运行记录说了什么：属于哪个运行 · 是不是它的终局 · 它做的那件事（行上「最近：…」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunMark {
    pub(crate) run: String,
    pub(crate) end: Option<RunEnd>,
    pub(crate) did: Option<RunDid>,
}

/// 子运行怎么收场的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunEnd {
    Done,
    Failed,
    /// 被叫停（用户 / 派出它的那一方让它停的）。
    Stopped,
}

/// 一个运行最近做的那件事（通用的值域；某一家的哪种记录算哪一件由那一家判）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum RunDid {
    /// 在写回复。
    Say,
    /// 在想。
    Think,
    /// 调用了一个工具。
    Tool { name: String },
}

/// 父记录里说到子运行的一条。三形：
/// - 派出：`tool` 有、`run` 无 ⇒ 这次工具调用派出了一个子运行（标签 · 类别）；
/// - 对上：`tool` 与 `run` 都有 ⇒ 这次调用派出的就是这个子运行（可带收场：前台跑完的那次结果）；
/// - 收场：只有 `run` 与 `end` ⇒ 派出那一方说这个子运行完了 / 败了 / 被叫停（不认识的运行不立新行）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ChildLink {
    pub(crate) tool: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) run: Option<String>,
    /// 派出那一方说这个子运行已经收场了；没说 ⇒ `None`。
    pub(crate) end: Option<RunEnd>,
}

/// 记录成品里「这次工具调用派出了一个子运行」的那一格（父侧工具调用 id ⇒ 它）：界面按它给那张工具卡起名，不认工具名与入参。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ChildRunTag {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub kind: Option<String>,
}

/// 过程里一步的「一行人话」：assistant 记录成品的 `toolSteps`（`tool_use.id` ⇒ 它）。
/// 怎么从入参里挑主参数与说明是各家的格式知识（`agents/<名>/steps.rs`）；界面只排版，不认入参结构。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ToolStep {
    /// 工具名（原样）。
    pub tool: String,
    /// 主参数（命令 · 路径 · 搜索词 · 网址 · 任务说明）：一行（换行压成空格）。认不出主参数 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub arg: Option<String>,
    /// 主参数是不是路径（界面「中间省略」与「打开文件」按它）。
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub path: bool,
    /// 说明（Bash 的 `description` 那一类）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub note: Option<String>,
    /// 这一家认不认得这个工具（`false` ⇒ 界面画问号、给原文）。
    pub known: bool,
}

/// 过程里一步的结果一句（B5 后一半 ＋ B7）：user 记录成品的 `toolResults`（`tool_result.tool_use_id` ⇒ 它）。
/// 数是从结果里读的（各家的格式知识），界面只按这几格拼字；读不出的格缺。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct StepResult {
    /// 成功（`false` ＝ 失败或被拒）。
    pub ok: bool,
    /// 人没批准这一步（批准框里选了「不」/ 计划没批）。只在 `ok == false` 时可能为真。
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub rejected: bool,
    /// 读了几行（读文件）/ 输出几行（命令）/ 命中几行（搜索）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub lines: Option<u32>,
    /// 改动：加了几行 · 删了几行。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub added: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub removed: Option<u32>,
    /// 命中几个文件（搜索 / 列文件）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub files: Option<u32>,
    /// 提问 / 计划的结果（B7）：批准了 · 选了哪几项。没批准走 `rejected`。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub answer: Option<Answer>,
    /// 命令失败时的退出码（结果里写着才有）。
    #[serde(rename = "exitCode", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub exit_code: Option<i32>,
}

/// 提问 / 计划答了什么（B7）。界面写「已批准」/「已选「{option}」」，不显示 Claude Code 的英文原句。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Answer {
    /// 计划批准了。
    Approved,
    /// 提问选了这几项（选项原文，按问题顺序；一题多选的那题各项都列）。
    Picked { options: Vec<String> },
}

/// 报错 / 重试的原因种类（B6）：界面按它出一词（服务器过载 · 额度满 · 网络中断 · 需登录 · 上下文超长 · 原因不明），不认状态码与报错原文。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub enum ApiReason {
    Overloaded,
    Quota,
    Network,
    Auth,
    Context,
    Unknown,
}

/// 一条用户角色的记录（或一条排队消息）是**谁说的** —— 通用的值域；怎么认是各家的格式知识（`agents/<名>/`，
/// 注册表 [`TextFace::user`]）。随记录成品带出（`userText.speaker`），界面按它决定画不画、画成哪种，不认正文里的标记。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Speaker {
    /// 人打的字（含粘贴、采纳的建议、忙时排队打的）。
    Human,
    /// 人敲的斜杠命令。
    SlashCommand { name: String, args: String },
    /// 人敲的 `!` 命令。
    BashInput { command: String },
    /// `!` 命令的输出回显。
    BashOutput { stdout: String, stderr: String },
    /// 本地命令（斜杠命令）的输出回显。
    CommandOutput,
    /// 后台任务（子 agent / 后台命令）的收场通知。
    TaskNotification {
        #[serde(rename = "taskId", skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        task_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        summary: Option<String>,
        #[serde(rename = "toolUseId", skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        tool_use_id: Option<String>,
    },
    /// 同一会话里的子 agent 发来的话；`handback` ＝ 它交回的报告。`from` 是子 agent 的 id。
    AgentMessage {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        from: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        name: Option<String>,
        handback: bool,
        /// 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        body: Option<String>,
    },
    /// 另一个会话（另一个实例）发来的话。
    PeerSession {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        from: Option<String>,
        /// 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        body: Option<String>,
    },
    /// （子 agent 那一侧）主会话后来发给它的话。
    Coordinator {
        /// 那段话的正文（记录级 `origin.body`，没有就取框里那段；剥过两头空白）。没有正文 ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        body: Option<String>,
    },
    /// （子 agent 那一侧）主会话派给它的活。
    AgentTask,
    /// 系统注入：提醒、技能展开、续跑样板、定时触发、额度恢复后的续跑等。
    /// 默认不露；会话头「⋯」里开了「显示系统注入」才画（一条一行、收着）。
    System {
        /// 注入的原文（剥过两头空白）；这一家给不出 ⇒ 缺。只随记录成品给界面，不进日志。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        body: Option<String>,
    },
    /// 上下文压缩后续接用的摘要。
    CompactSummary,
    /// 中断标记（人按了 Esc）。
    Interrupt,
    /// 工具结果回灌。
    ToolResult,
}

impl Speaker {
    /// 线上 `kind` 那一格（骨架索引的 `sp` 也用它）。
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::SlashCommand { .. } => "slashCommand",
            Self::BashInput { .. } => "bashInput",
            Self::BashOutput { .. } => "bashOutput",
            Self::CommandOutput => "commandOutput",
            Self::TaskNotification { .. } => "taskNotification",
            Self::AgentMessage { .. } => "agentMessage",
            Self::PeerSession { .. } => "peerSession",
            Self::Coordinator { .. } => "coordinator",
            Self::AgentTask => "agentTask",
            Self::System { .. } => "system",
            Self::CompactSummary => "compactSummary",
            Self::Interrupt => "interrupt",
            Self::ToolResult => "toolResult",
        }
    }
}

/// 一条用户角色记录判过「谁说的」的成品（user 记录与排队消息的 `userText`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct UserText {
    pub speaker: Speaker,
    /// 要显示的正文（剥过注入、trim 过）：人说的话 · 压缩摘要 · 派给子 agent 的活；别的来源是空串。
    pub text: String,
    /// `text` 里人粘贴进来的块（UTF-16 下标，含两头的标记）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<Pasted>>"))]
    pub pasted: Vec<Pasted>,
}

/// 人粘贴进来的一块在 `text` 里的位置（`[start, end)`，UTF-16 下标，含两头的标记）＋ 正文那一截（`[bodyStart, bodyEnd)`）与它的行数。
/// 界面据此把超过 12 行的折起来、不超过的只露正文（不认标记的写法）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Pasted {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub id: Option<String>,
    pub start: u32,
    pub end: u32,
    pub body_start: u32,
    pub body_end: u32,
    /// 正文的行数（两头的空行不算）。
    pub lines: u32,
}

impl Default for UserText {
    fn default() -> Self {
        Self::of(Speaker::System { body: None })
    }
}

impl UserText {
    /// 没有正文要显示的那几种来源。
    pub(crate) fn of(speaker: Speaker) -> Self {
        Self {
            speaker,
            text: String::new(),
            pasted: Vec::new(),
        }
    }

    /// 人在这条里说的话（大纲 · 搜索 · 历史摘录都用它）；不是人说的、或说了个空 ⇒ `None`。
    pub(crate) fn speech(&self) -> Option<String> {
        let s = match &self.speaker {
            Speaker::Human => self.text.clone(),
            Speaker::SlashCommand { name, args } => format!("{name} {args}").trim().to_string(),
            Speaker::BashInput { command } => format!("!{command}"),
            _ => return None,
        };
        (!s.is_empty()).then_some(s)
    }
}

/// 子运行的记录住哪：父记录路径 ⇒ 此刻在盘上的子运行记录路径们（不在 ⇒ 空）。通用 watcher 拿这些路径走与主记录同一条事件管线。
#[derive(Clone, Copy)]
pub(crate) struct ChildFace {
    pub(crate) sources: fn(&Path) -> Vec<PathBuf>,
    /// 一个路径**若是**某份父记录的子运行记录（形状对得上 `sources` 会收的那种）⇒ 那份父记录的路径；否则 `None`。
    /// 文件事件来了只对它答得出的才去找，且只在那一份父记录底下找。
    pub(crate) owner: fn(&Path) -> Option<PathBuf>,
    /// 父记录的一行原文可能说到子运行（[`RecordFace::child_link`] 会答出东西）—— 便宜的预筛：漏判不许，多判无妨。
    /// 只读尾巴的那条流接上会话时，靠它从父记录已有的那一截里只挑这几行解析。
    pub(crate) hint: fn(&str) -> bool,
}

/// 一个上游协议的流面：把一个原始流事件（SSE `data:` 后面那段原文）折成归一事件。认不出 ⇒ 空。
/// 按上游协议分，不按 agent 分：哪家走哪个协议由那一家的 [`DefaultUpstream::stream`] 说。
#[derive(Clone, Copy)]
pub(crate) struct StreamFace {
    pub(crate) fold: fn(&str) -> Vec<StreamEv>,
}

/// 归一流事件（界面只收这个，不收任何一家的原始事件）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum StreamEv {
    /// 一次应答开始：对账键（与记录的 [`RecordFace::response_id`] 同一个值域）。
    Start { rid: String },
    /// 第 `i` 块开始了。
    Block {
        #[cfg_attr(test, ts(type = "number"))]
        i: u64,
        kind: BlockKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        tool: Option<String>,
    },
    /// 第 `i` 块的一段文字。
    Text {
        #[cfg_attr(test, ts(type = "number"))]
        i: u64,
        s: String,
    },
    /// 上游说完了（`ok` ＝ 正常说完；`false` ＝ 上游报错收尾）。
    Stop { ok: bool },
}

/// 归一流里一块是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub enum BlockKind {
    Text,
    Thinking,
    Tool,
    Other,
}

/// 通用层要的那一组面（记录 ＋ 子运行住址）：流式 watcher 跟的那一家给；判据喂别的一组（假适配层）。
#[derive(Clone, Copy)]
pub(crate) struct RunFaces {
    pub(crate) response_id: Option<fn(&serde_json::Value) -> Option<String>>,
    pub(crate) run_of: Option<fn(&serde_json::Value) -> Option<RunMark>>,
    pub(crate) child_link: Option<fn(&serde_json::Value) -> Vec<ChildLink>>,
    pub(crate) children: Option<ChildFace>,
}

impl RunFaces {
    /// 一家的记录面 ⇒ 它的运行面。
    pub(crate) fn of(r: &RecordFace) -> Self {
        Self {
            response_id: r.response_id,
            run_of: r.run_of,
            child_link: r.child_link,
            children: r.children,
        }
    }

    /// 什么都不填 ⇒ 没有子运行、流对不上记录。
    pub(crate) const NONE: Self = Self {
        response_id: None,
        run_of: None,
        child_link: None,
        children: None,
    };

    pub(crate) fn response_id(&self, v: &serde_json::Value) -> Option<String> {
        self.response_id.and_then(|f| f(v))
    }

    pub(crate) fn run_of(&self, v: &serde_json::Value) -> Option<RunMark> {
        self.run_of.and_then(|f| f(v))
    }

    pub(crate) fn child_links(&self, v: &serde_json::Value) -> Vec<ChildLink> {
        self.child_link.map(|f| f(v)).unwrap_or_default()
    }

    pub(crate) fn sources(&self, parent: &Path) -> Vec<PathBuf> {
        self.children
            .map(|c| (c.sources)(parent))
            .unwrap_or_default()
    }

    pub(crate) fn owner(&self, p: &Path) -> Option<PathBuf> {
        self.children.and_then(|c| (c.owner)(p))
    }

    pub(crate) fn hint(&self, line: &str) -> bool {
        self.children.is_some_and(|c| (c.hint)(line))
    }
}

/// `kind` 那一家的运行面（没有 ⇒ [`RunFaces::NONE`]）。
pub(crate) fn run_faces(kind: &str) -> RunFaces {
    record_face(kind).map_or(RunFaces::NONE, |r| RunFaces::of(&r))
}

/// `kind` 那一家的一条已解析记录在会话里属于哪个运行（主运行 ⇒ `None`）—— 通用层（大纲 · 骨架索引）判「这条是不是子运行的」的唯一入口。
pub(crate) fn run_of_record(kind: &str, v: &serde_json::Value) -> Option<RunMark> {
    run_faces(kind).run_of(v)
}

/// 一家的记录文本面（原共享 crate `search-core` 里 Claude 记录文本那一半）：函数指针（同 [`Adapter::home`]，不立 trait）。
/// 通用层（全局搜索 · 会话内查找 · 历史摘录 · 用户输入列表 · 骨架索引）经 [`main_text`] · [`tool_text`] · [`user_text_of`] · [`human_speech`] 够它，不按名字够。
#[derive(Clone, Copy)]
pub(crate) struct TextFace {
    /// 一条记录的 `message.content` ⇒ 正文文本块。
    pub(crate) main: fn(&serde_json::Value) -> String,
    /// 同上 ⇒ 工具内容（`is_assistant`：工具入参 · 思考；否则：工具结果）。
    pub(crate) tool: fn(&serde_json::Value, bool) -> String,
    /// 一条已解析的记录 ⇒ 它是谁说的（[`UserText`]）；不是用户角色的记录 ⇒ `None`。
    pub(crate) user: fn(&serde_json::Value) -> Option<UserText>,
    /// 一个工具结果块（＋ 记录级的结构化结果）⇒ 结果一句（[`StepResult`]：失败 · 被拒 · 数 · 提问与计划答了什么）。
    pub(crate) result: fn(&serde_json::Value, Option<&serde_json::Value>) -> StepResult,
}

/// `kind` 那一家的文本面。
fn text_face(kind: &str) -> Option<TextFace> {
    record_face(kind).and_then(|r| r.text)
}

/// `kind` 那一家怎么抽正文 —— 通用层够它的唯一入口。那一家答不了 ⇒ 空串（不搜、不摘）。
pub(crate) fn main_text(kind: &str, content: &serde_json::Value) -> String {
    text_face(kind).map_or_else(String::new, |t| (t.main)(content))
}

/// `kind` 那一家怎么抽工具内容。那一家答不了 ⇒ 空串。
pub(crate) fn tool_text(kind: &str, content: &serde_json::Value, is_assistant: bool) -> String {
    text_face(kind).map_or_else(String::new, |t| (t.tool)(content, is_assistant))
}

/// `kind` 那一家判这条记录是谁说的。那一家答不了 ⇒ `None`。
pub(crate) fn user_text_of(kind: &str, v: &serde_json::Value) -> Option<UserText> {
    text_face(kind).and_then(|t| (t.user)(v))
}

/// `kind` 那一家怎么读一个工具结果块（一轮的摘要数失败用：被拒的不算失败）。那一家答不了 ⇒ 只看 `is_error`。
pub(crate) fn step_result_of(
    kind: &str,
    block: &serde_json::Value,
    tur: Option<&serde_json::Value>,
) -> StepResult {
    text_face(kind).map_or_else(
        || StepResult {
            ok: block.get("is_error").and_then(serde_json::Value::as_bool) != Some(true),
            ..StepResult::default()
        },
        |t| (t.result)(block, tur),
    )
}

/// 人在这条记录里说的话（[`UserText::speech`]）；不是人说的 ⇒ `None`。
pub(crate) fn human_speech(kind: &str, v: &serde_json::Value) -> Option<String> {
    user_text_of(kind, v)?.speech()
}

/// 删历史会话那一条要问适配层的两件事 —— 那是记录布局的知识，文件管理写面不认；
/// 由门（命令注册那一处）经 [`locate_session_for_delete`] · [`is_session_record`] 这一个窄口递给它。
#[derive(Clone, Copy)]
pub(crate) struct SessionDelete {
    /// sid ⇒ 本机要删的那一份（找 → 解到底 → 恰是那一形）。
    pub(crate) locate: fn(&str) -> Result<PathBuf, String>,
    /// 这一份是不是会话记录的形状（写面删之前再问一次）。
    pub(crate) is_record: fn(&Path) -> bool,
}

/// 窄口之一：sid ⇒ 要删的那一份。逐家问（声明了删会话两问的那几家），谁认得算谁；
/// 都答不出 ⇒ 第一家说的那一句；没有哪一家答得了 ⇒ 照实拒。
pub(crate) fn locate_session_for_delete(sid: &str) -> Result<PathBuf, String> {
    let mut first_err = None;
    for d in REGISTRY.iter().filter_map(|a| a.records?.delete) {
        match (d.locate)(sid) {
            Ok(p) => return Ok(p),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err
        .unwrap_or_else(|| copy_core::copy_text("beFilesWrite.session.noReader", &[("id", sid)])))
}

/// 窄口之二：这一份是不是会话记录 —— 这份记录归哪一家（[`record_face_of`]）就问哪一家；那一家答不了 ⇒ 不是。
pub(crate) fn is_session_record(p: &Path) -> bool {
    record_face_of(p)
        .and_then(|r| r.delete)
        .is_some_and(|d| (d.is_record)(p))
}

/// 一行原文在渲染模型里的样子 —— 适配层给，通用层只搬（`message` 的字段通用层一个都不读）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedLine {
    /// 渲染模型那一条（界面收到的就是它）。
    pub(crate) message: serde_json::Value,
    /// 进不进界面：`false` ＝ 照占号、不出成品（没有读者的元数据记录）。
    pub(crate) displayable: bool,
    /// 这条记录自己的 `cwd`（带它的那一类才有）。
    pub(crate) cwd: Option<String>,
}

/// 注册表里 `kind` 那一家。认不出 ⇒ `None`。
fn adapter_among<'a>(registry: &'a [Adapter], kind: &str) -> Option<&'a Adapter> {
    registry.iter().find(|a| a.kind == kind)
}

/// 给定注册表里**恰一家**满足 `has` ⇒ 它的 kind；一家都没有、或不止一家 ⇒ `None`（显式失败，不按注册序挑）。
/// 请求里没说是哪一家、而那一格今天只有一家声明的那几处用它（调用行写明是哪一格）。
pub(crate) fn sole_kind_among(
    registry: &[Adapter],
    has: impl Fn(&Adapter) -> bool,
) -> Option<&'static str> {
    let mut hit = registry.iter().filter(|a| has(a));
    match (hit.next(), hit.next()) {
        (Some(a), None) => Some(a.kind),
        _ => None,
    }
}

/// [`sole_kind_among`] 在生产注册表上。
pub(crate) fn sole_kind(has: impl Fn(&Adapter) -> bool) -> Option<&'static str> {
    sole_kind_among(REGISTRY, has)
}

/// `kind` 那一家的记录解释面。认不出 ⇒ `None`。
pub(crate) fn record_face(kind: &str) -> Option<RecordFace> {
    adapter_among(REGISTRY, kind).and_then(|a| a.records)
}

/// 一份已过围栏的会话记录是哪一家的：落在某一家合成历史的根下 ⇒ 那一家；否则 ⇒ 家目录记录树那一家（[`record_tree_kind`]）。
pub(crate) fn record_kind_of(path: &Path) -> Option<&'static str> {
    record_kind_among(REGISTRY, path)
}

/// [`record_kind_of`] 的可喂夹具那一半。
pub(crate) fn record_kind_among(registry: &[Adapter], path: &Path) -> Option<&'static str> {
    let under = |root: PathBuf| {
        let root = std::fs::canonicalize(&root).unwrap_or(root);
        path.starts_with(root)
    };
    registry
        .iter()
        .find(|a| a.history.and_then(|h| (h.root)()).is_some_and(under))
        .map(|a| a.kind)
        .or_else(|| record_tree_kind_among(registry))
}

/// 这份会话记录归的那一家的记录解释面（[`record_kind_of`]）。
pub(crate) fn record_face_of(path: &Path) -> Option<RecordFace> {
    record_face_among(REGISTRY, path)
}

/// [`record_face_of`] 的可喂夹具那一半。
pub(crate) fn record_face_among(registry: &[Adapter], path: &Path) -> Option<RecordFace> {
    record_kind_among(registry, path)
        .and_then(|k| adapter_among(registry, k))
        .and_then(|a| a.records)
}

/// 家目录记录树那一家的 kind：**恰一家**有记录解释面、却没有合成历史面（它的会话住在按项目分的记录树里）。
/// 历史清单的记录树那一支、全文搜索、流式 watcher 跟的都是它。不止一家 ⇒ `None`（说不清是哪一家，不猜）。
pub(crate) fn record_tree_kind() -> Option<&'static str> {
    record_tree_kind_among(REGISTRY)
}

fn record_tree_kind_among(registry: &[Adapter]) -> Option<&'static str> {
    sole_kind_among(registry, |a| a.history.is_none() && a.records.is_some())
}

/// 找项目目录时记录开头至多读多少字节。带它的那一条实测在前一 KiB 内；上界是给几百 MB 的大会话的：不整读。
pub(crate) const HEAD_CAP: u64 = 1 << 20;

/// 记录开头逐条交给 `pick`，第一个给出值的胜；读满 [`HEAD_CAP`] 字节就停（截在半截的那一行解析不出、不算）。
/// 读满了还没找到 ⇒ `None`，并在日志里说是哪份文件（调用方退到「没有这一格」）。
pub(crate) fn first_in_head(
    p: &Path,
    pick: impl Fn(&serde_json::Value) -> Option<String>,
) -> Option<String> {
    use std::io::{BufRead, Read};
    let file = std::fs::File::open(p).ok()?;
    let mut r = std::io::BufReader::new(file.take(HEAD_CAP));
    let mut line = Vec::new();
    let mut read = 0u64;
    loop {
        line.clear();
        let n = r.read_until(b'\n', &mut line).ok()?;
        if n == 0 {
            if read >= HEAD_CAP {
                tracing::warn!(
                    "{}: nothing found within the head cap; giving up",
                    p.display()
                );
            }
            return None;
        }
        read += n as u64;
        let one = line.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&line);
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(one) {
            if let Some(got) = pick(&v) {
                return Some(got);
            }
        }
    }
}

/// 这份会话记录的项目目录：记录归哪一家就问哪一家（[`RecordFace::project_dir`]）；那一家不声明 / 开头里没有 ⇒ `None`。
pub(crate) fn project_dir_of(p: &Path) -> Option<String> {
    record_face_of(p)
        .and_then(|f| f.project_dir)
        .and_then(|f| f(p))
}

/// 注册表里每一家的漂移账（注册序；不记的跳过）—— 帧命令 `drift-report` 的读法入口。
pub(crate) fn drift_reports() -> Vec<serde_json::Value> {
    REGISTRY
        .iter()
        .filter_map(|a| a.records.and_then(|r| r.drift).map(|f| f()))
        .collect()
}

/// 见 [`Adapter::footprint`]。
#[derive(Clone, Copy)]
pub(crate) struct FootprintFace {
    pub(crate) tools: &'static [crate::footprint::registry::ToolSpec],
    pub(crate) env: &'static [crate::footprint::registry::UnmanagedEnv],
    pub(crate) under_agent_home: fn(&str) -> Option<&str>,
    pub(crate) user_settings: fn(&Path) -> [PathBuf; 2],
}

/// 一份身份文件的原生根：没设账号环境变量时它住哪。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityRoot {
    /// 住 `<配置根>/<名>`（设了账号环境变量就跟着走）。
    ConfigDir,
    /// 住 `$HOME/<名>`（不跟账号环境变量走）。
    Home,
}

/// 一份身份文件是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityClass {
    /// 身份本体：必须 600、必须每个号一份、绝不从别的号复制。
    Secret,
    /// 本机状态：每个号一份（共享会串号，或高频写互相覆盖）。
    State,
    /// 跟着别的项走的附属物（备份之类）。
    Derived,
}

/// 见 [`Adapter::accounts`]。
#[derive(Clone, Copy)]
pub(crate) struct AccountsFace {
    /// 每个号各有一份的那几项：`(名, 原生根, 类别)`；共享库顶层其余每一项都链回共享库。
    pub(crate) identity: &'static [(&'static str, IdentityRoot, IdentityClass)],
    /// 账号级配置文件的名字（原生根是家目录的那一份，账号 0 的住 `$HOME`）。
    pub(crate) config_file: &'static str,
    /// 那份配置文件里装用户级 MCP 的顶层键（账号之间同步的就是它，别的键不碰）。
    pub(crate) user_mcp_key: &'static str,
    /// 没设账号时的配置根（= 各号链回去的共享库）。
    pub(crate) shared_root: fn(&Path) -> PathBuf,
    /// 一个配置根下登录的邮箱（读不到 ⇒ `None`）。
    pub(crate) email_in: fn(&Path) -> Option<String>,
    /// 后端看会话用的那几项（会话起停 · 会话记录）：常驻后端只看共享库里的这一份 ⇒ 各号必须链回去，不许隔离。
    pub(crate) watched: &'static [&'static str],
}

/// `kind` 那一家的账号库布局。认不出 / 那一家没有 ⇒ `None`。
pub(crate) fn accounts_face(kind: &str) -> Option<AccountsFace> {
    accounts_face_among(REGISTRY, kind)
}

/// [`accounts_face`] 的可喂夹具那一半。
pub(crate) fn accounts_face_among(registry: &[Adapter], kind: &str) -> Option<AccountsFace> {
    adapter_among(registry, kind).and_then(|a| a.accounts)
}

/// 注册表里带足迹面的那几家（按注册表顺序）。
pub(crate) fn footprint_faces() -> impl Iterator<Item = FootprintFace> {
    REGISTRY.iter().filter_map(|a| a.footprint)
}

/// 一家的 MCP 读面：函数指针（同 [`Adapter::home`]，不立 trait）。入参是项目目录（可缺）。
#[derive(Clone, Copy)]
pub(crate) struct McpFace {
    pub(crate) read: fn(Option<&Path>) -> McpRead,
}

/// 一家读出来的 MCP 事实：条目（user / local / project）· 用过的项目目录（`dirs`）· 读不出来的那几份（说出来，不当成空）。
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct McpRead {
    pub entries: Vec<McpEntry>,
    pub dirs: Vec<String>,
    pub problems: Vec<String>,
}

/// 一条 MCP server：`server` 原样（宽容，未知字段不丢）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct McpEntry {
    pub scope: &'static str,
    pub name: String,
    pub server: serde_json::Value,
    pub source: String,
}

/// `kind` 那一家的 MCP 读面，读一遍。`None` = 那一家不认得 MCP。
pub(crate) fn mcp_read_among(
    registry: &[Adapter],
    kind: &str,
    project_dir: Option<&Path>,
) -> Option<McpRead> {
    adapter_among(registry, kind)
        .and_then(|a| a.mcp)
        .map(|f| (f.read)(project_dir))
}

/// [`mcp_read_among`] 对本机注册表 —— 帧命令 `mcp-read` 的读法入口。
pub(crate) fn mcp_read(kind: &str, project_dir: Option<&Path>) -> Option<McpRead> {
    mcp_read_among(REGISTRY, kind, project_dir)
}

/// 一家的默认上游：路由里叫它什么 · 盖掉内置默认的那个旋钮 · 内置默认。三格焊在一起
/// （分开取就写得出「A 家的旋钮配 B 家的默认值」—— 与上游选择那张表的 `Row` 焊住上游与 key 同一条理由）。
pub(crate) struct DefaultUpstream {
    /// 中转路由键第 1 段里这一家的名字（monitor 起会话拼 `/t/<它>/…`）。⚠ 与 [`Adapter::kind`]（wire 上的 `agent_kind`）
    /// 是两个值域：Claude 在 wire 上叫 `claude`、在路由里叫 `claude-code` —— 各有各的既有契约，不在这里对齐。
    pub(crate) route_id: &'static str,
    /// 盖掉内置默认的环境变量名。**每家一个**（「每家一个」：不留「覆盖哪一家说不清」的全局旋钮）。
    pub(crate) env: &'static str,
    /// 没配 `env` 时这一家发到哪儿。
    pub(crate) fallback: &'static str,
    /// 这一家的请求里**它自己带着会话标识**的那个头（中转拿它给流打标签）。`None` = 说不出 ⇒ 流不带标签。
    pub(crate) session_header: Option<&'static str>,
    /// 这一家的上游说哪种流协议（归一流的折法）。`None` ＝ 它的流不折（活卡认不得）。
    pub(crate) stream: Option<StreamFace>,
    /// 请求本身就说出「我是哪个子运行」的那个头（值 ＝ 子运行的标识，与 [`RecordFace::run_of`] 同一个值域）。
    /// 声明了 ⇒ 带头的请求归那个子运行、不带头的就是主运行（当场定）；`None` ＝ 请求认不出运行 ⇒ 通用层按记录对账归位。
    pub(crate) owner_header: Option<&'static str>,
    /// API key 凭据文件（界面给账号配第三方 key 时写的那一份）里的行挂在这一家名下。
    /// 那份文件没有 agent 这一维 ⇒ 至多一家为真；没有一家为真 ⇒ 上游选择起不来（fail-closed）。
    pub(crate) owns_credentials_file: bool,
    /// 直接敲的这一家也走中转（可选，用户自己贴、后端只读）：它的用户级设置文件里写进程环境的那一块。
    /// `None` ＝ 这一家没有这一形，只能经起会话注入。
    pub(crate) settings_env: Option<SettingsEnvFace>,
    /// 请求里说明「这一轮用的是扩展上下文」的那一项：（头名, 列表里那一项的前缀）。中转只记它在不在，会话事实据此定上限。
    /// `None` ＝ 这一家的请求说不出。
    pub(crate) context_mark: Option<(&'static str, &'static str)>,
    /// 回包状态 ＋ 回包头（＋ 此刻，unix 秒）→ 通用的额度快照：这一家的读法（头名只住那一家）。
    /// `None` ＝ 这一家的回包说不出额度；`Some` 答 `None` ＝ 这一个回包里没有额度信息。
    pub(crate) quota: Option<QuotaRead>,
    /// 额度窗口名 → 语义位（`5h` / `7d`；别的窗口 ⇒ `None`）。界面与换号记录只认语义位，不认各家的窗口名。
    pub(crate) window_slot: Option<fn(&str) -> Option<&'static str>>,
    /// 额度窗口名 → 窗口键（轮换的上限 · 单段预算按它配；通用层当不透明的键，只认 `5h` · `7d` 两个缺省键）。
    /// 不算用量窗口的那几档（超额）⇒ `None`。
    pub(crate) window_key: Option<fn(&str) -> Option<String>>,
    /// 让这一家的官方客户端报一次某个号的用量（不花模型额度）：怎么起 · 怎么把输出读成窗口。`None` ＝ 这一家没有这一形。
    pub(crate) usage: Option<UsageFace>,
    /// 订阅号登录那一格：令牌住哪、什么格式、怎么续、锁叫什么。`None` ＝ 这一家没有可换的订阅号登录。
    pub(crate) login: Option<LoginFace>,
    /// 这一家自己认得的「用满」回包（轮换的硬上限用它：这一发不发上游、回这一份）。`None` ＝ 给不出 ⇒ 对它硬上限不成立、按软阈值办。
    pub(crate) limit_reply: Option<LimitReplyOf>,
}

/// 一份「用满」回包：状态行里状态码那一截 · 头 · 体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LimitReply {
    pub(crate) status: &'static str,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

/// （几点重置 unix 秒, 此刻 unix 秒, 卡着的那个窗口的窗口键 `5h` · `7d` · `7d:<模型>`，说不出 ⇒ `None`）→ 这一家的「用满」回包。
pub(crate) type LimitReplyOf = fn(u64, u64, Option<&str>) -> LimitReply;

/// 一家「让官方客户端报用量」那一形：起哪个程序 · 带什么参数 · 号的配置目录交给哪一格环境 · 起它时摘 / 设哪几格 · 输出怎么读。
/// 通用层（帧命令 `quota-probe`）只照它起、照它读，读成的窗口记进额度账、标来源 `usage`。
#[derive(Debug, Clone, Copy)]
pub(crate) struct UsageFace {
    pub(crate) program: &'static str,
    pub(crate) args: &'static [&'static str],
    /// 号的配置目录交给这一格；账号 0（默认配置目录）⇒ 摘掉它。
    pub(crate) dir_env: &'static str,
    /// 起它时摘掉的（会盖掉那个号自己的登录的那几格）。
    pub(crate) env_remove: &'static [&'static str],
    /// 起它时设上的（如让输出里的时刻按 UTC 写）。
    pub(crate) env_set: &'static [(&'static str, &'static str)],
    /// （标准输出, 此刻 unix 秒）→ 窗口；读不懂 ⇒ `Err(哪一处读不懂)`（不猜）。
    pub(crate) read: fn(&str, u64) -> Result<Vec<QuotaWindow>, String>,
}

/// 路由第 1 段 → 那一家的「报用量」那一形。
pub(crate) fn usage_of(route_id: &str) -> Option<UsageFace> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.usage)
}

/// 路由第 1 段 → 那一家的「用满」回包（见 [`DefaultUpstream::limit_reply`]）。
pub(crate) fn limit_reply_of(route_id: &str) -> Option<LimitReplyOf> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.limit_reply)
}

/// 一家的订阅号登录的格式知识（读写与续期在账号域 `accounts/oauth/`，这里只有这一家的名字与地址）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoginFace {
    /// 配置目录里那份凭据文件的名字。
    pub(crate) creds_file: &'static str,
    /// 凭据文件里放令牌的那一节，与节里的几格。
    pub(crate) section: &'static str,
    pub(crate) access: &'static str,
    pub(crate) refresh: &'static str,
    /// 访问令牌几点过期（毫秒时间戳）。
    pub(crate) expires_ms: &'static str,
    pub(crate) scopes: &'static str,
    /// 这个号登录时用的客户端 id（缺 ⇒ [`Self::client_id`]）。
    pub(crate) client_field: &'static str,
    /// 令牌端点（续期发到这里）与缺省的客户端 id。
    pub(crate) token_url: &'static str,
    pub(crate) client_id: &'static str,
    /// 还剩这么多毫秒就算快过期（同那一家自己的余量）。
    pub(crate) margin_ms: u64,
    /// 续期锁：配置目录里那一把的名字 · 配置目录旁边那一把的后缀（那一家自己续期时拿的同一套）。
    pub(crate) lock_inside: &'static str,
    pub(crate) lock_beside: Option<&'static str>,
    /// 一个号的账号身份住哪份文件：（配置目录, 账号 0 时的家目录）→ 那份文件。
    pub(crate) identity_file: fn(&Path, Option<&Path>) -> PathBuf,
    /// 那份文件里的账号身份（请求里跟着鉴权一起换的那一格的值）；读不到 ⇒ `None`。
    pub(crate) identity_in: fn(&Path) -> Option<String>,
    /// 请求体里那一格账号身份换成给的值（按量号给空串）；只动那几个字节。
    pub(crate) rewrite_identity: fn(&[u8], &str) -> IdentityCell,
    /// 账号 0（没设配置目录的那个号）的配置目录：家目录 → 它。
    pub(crate) base_dir: fn(&Path) -> PathBuf,
}

/// 请求体里账号身份那一格换没换成。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IdentityCell {
    /// 请求体里没有这一格 ⇒ 原样发。
    Absent,
    /// 换好了的整份请求体（只差那一格）。
    Rewritten(Vec<u8>),
    /// 有这一格，但认不准是哪几个字节 ⇒ 不能拿这份请求体换号。
    Unsure,
}

/// 路由第 1 段 → 那一家的额度窗口语义位读法。
pub(crate) fn window_slot_of(route_id: &str) -> Option<fn(&str) -> Option<&'static str>> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.window_slot)
}

/// 路由第 1 段 → 那一家的额度窗口键读法。
pub(crate) fn window_key_of(route_id: &str) -> Option<fn(&str) -> Option<String>> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.window_key)
}

/// 路由第 1 段 → 那一家的订阅号登录格式。
pub(crate) fn login_of(route_id: &str) -> Option<LoginFace> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.login)
}

/// 一家的额度读法（见 [`DefaultUpstream::quota`]）。
pub(crate) type QuotaRead = fn(u16, &[(String, String)], u64) -> Option<QuotaReading>;

/// 路由第 1 段 → 那一家的额度读法。
pub(crate) fn quota_read_of(route_id: &str) -> Option<QuotaRead> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .and_then(|u| u.quota)
}

/// 限流器对一发的结论（各家的值翻成这三档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum QuotaStatus {
    Allowed,
    /// 还能用，但越过了某条预警线。
    Warning,
    Rejected,
}

/// 一个额度窗口此刻的样子。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaWindow {
    /// 窗口名（那一家读法表里的值，如 5 小时 · 7 天）。
    pub(crate) name: String,
    /// 已用比例（通常 0–1，可以超过 1）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) used: Option<f64>,
    /// 几点重置（unix 秒）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) resets_at: Option<u64>,
    /// 越过了哪条预警线（比例）；没越过就没有。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) warned_at: Option<f64>,
}

/// 超额（付费额外用量）那一档。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaOverage {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) status: Option<QuotaStatus>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) resets_at: Option<u64>,
    /// 为什么不可用（那一家的原值）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) disabled: Option<String>,
    /// 这一发正在用超额。
    pub(crate) in_use: bool,
}

/// 一个回包读出来的额度快照（通用形状，账号域只认它）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaReading {
    /// 限流器的结论；回包里没说 ⇒ `None`。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) status: Option<QuotaStatus>,
    /// 这一发被上游拒了（没被服务）。
    pub(crate) refused: bool,
    /// 此刻卡着的那个窗口名。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) limiting: Option<String>,
    /// 卡着的那个窗口几点重置（被拒时：到这一刻之前这个号用不了）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) resets_at: Option<u64>,
    pub(crate) windows: Vec<QuotaWindow>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) overage: Option<QuotaOverage>,
}

impl QuotaReading {
    /// 各窗口里最高的已用比例；一个都说不出 ⇒ `None`。
    pub(crate) fn peak_used(&self) -> Option<f64> {
        self.windows
            .iter()
            .filter_map(|w| w.used)
            .fold(None, |m, u| Some(m.map_or(u, |m: f64| m.max(u))))
    }
}

/// 一家的用户级设置文件里「上游地址」那一格：住哪 · 怎么读出来 · 要贴的那一段长什么样（格式知识与那一次只读都在这一家）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SettingsEnvFace {
    /// 家目录（`$HOME`）→ 那份文件（各号共用的那一份）＋ 里面写的上游地址（**只读**）。
    pub(crate) read: fn(&Path) -> (PathBuf, SettingsBaseUrl),
    /// 地址 → 要合并进那份文件的那一段。
    pub(crate) snippet: fn(&str) -> String,
}

/// 设置文件里上游地址那一格读出来的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SettingsBaseUrl {
    /// 文件不在，或没写（或写了空串）。
    Unset,
    /// 写了这个地址。
    Set(String),
    /// 读不了 / 读不懂 ⇒ 装没装说不清。
    Unreadable(SettingsUnreadable),
}

/// 为什么读不了。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SettingsUnreadable {
    /// 读的时候出错（权限 · I/O），带系统的原话。
    Io(String),
    /// 那不是一个普通文件。
    NotFile,
    /// 超过这个字节数。
    TooLarge(u64),
    /// 读得到但读不懂（坏 JSON · 顶层或那一块不是对象 · 值不是串）。
    BadShape,
}

/// 声明拥有 API key 凭据文件（[`DefaultUpstream::owns_credentials_file`]）的第一家的路由名；没有一家 ⇒ 空串（上游选择据此起不来）。
pub(crate) const fn credentials_file_agent() -> &'static str {
    let mut i = 0;
    while i < REGISTRY.len() {
        if let Some(u) = &REGISTRY[i].upstream {
            if u.owns_credentials_file {
                return u.route_id;
            }
        }
        i += 1;
    }
    ""
}

/// 登记了默认上游的每一家：`(路由名, 那一格)`。**上游选择读默认上游的唯一入口**。
pub(crate) fn default_upstreams() -> impl Iterator<Item = &'static DefaultUpstream> {
    REGISTRY.iter().filter_map(|a| a.upstream.as_ref())
}

/// 注册表里第一家声明了「直接敲的也走中转」那一格的：`(路由名, 那一格)`。没有 ⇒ `None`。
pub(crate) fn settings_env_face() -> Option<(&'static str, SettingsEnvFace)> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find_map(|u| u.settings_env.map(|f| (u.route_id, f)))
}

/// 各家登记的会话标识头（注册序、去重）：中转经上游选择拿到这份名单，按它从请求里认会话 —— 会话 id 归 agent 自己。
pub(crate) fn session_headers() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for h in REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref()?.session_header)
    {
        if !v.contains(&h) {
            v.push(h);
        }
    }
    v
}

/// 各家登记的「请求说明用的是扩展上下文」的那一项（注册序、去重）：中转按它记每个会话的请求带没带。
pub(crate) fn context_marks() -> Vec<(&'static str, &'static str)> {
    let mut v: Vec<(&'static str, &'static str)> = Vec::new();
    for m in REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref()?.context_mark)
    {
        if !v.contains(&m) {
            v.push(m);
        }
    }
    v
}

/// 各家登记的「请求说出子运行」的头（注册序、去重）：中转按它从请求里认运行。
pub(crate) fn owner_headers() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for h in REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref()?.owner_header)
    {
        if !v.contains(&h) {
            v.push(h);
        }
    }
    v
}

/// 一家上游在流归位那里的样子：它的流协议面 ＋ 它的请求是否自报运行（声明了 [`DefaultUpstream::owner_header`]）。
#[derive(Clone, Copy)]
pub(crate) struct StreamFamily {
    pub(crate) face: StreamFace,
    /// 声明了自报运行的头 ⇒ 带头的归那个子运行、不带头的就是主运行（当场定，不挂起）。
    pub(crate) owns: bool,
}

/// 各家上游的流协议面（注册序）。一条应答用哪一个，由通用层按头一件事认（认得出「开始」的那一个）。
pub(crate) fn stream_families() -> Vec<StreamFamily> {
    REGISTRY
        .iter()
        .filter_map(|a| {
            let u = a.upstream.as_ref()?;
            Some(StreamFamily {
                face: u.stream?,
                owns: u.owner_header.is_some(),
            })
        })
        .collect()
}

/// 一家的合成历史面：函数指针（同 [`Adapter::home`]，不立 trait）。
#[derive(Clone, Copy)]
pub(crate) struct HistoryFace {
    /// 这台机器上这一家的会话（没装 ⇒ 空）。
    pub(crate) sessions: fn() -> Vec<SynthSession>,
    /// 一份会话的首条真用户话（列表摘要）。
    pub(crate) excerpt: fn(&Path) -> String,
    /// 这一家会话记录的根（没装 / 说不出 ⇒ `None`）。按路径读会话那几条命令（`history-read` 等）的
    /// 围栏认它 —— 历史清单列出来的这一家的会话，要能按同一条路打开（本机冷读也走后端之后，本机 Codex 会话靠它）。
    pub(crate) root: fn() -> Option<PathBuf>,
}

/// 合成历史里的一个会话（通用层按 `cwd` 分组成项目）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SynthSession {
    pub sid: String,
    pub path: PathBuf,
    /// 分组键；缺 ⇒ 空串（归「(<kind>)」组）。
    pub cwd: String,
    pub mtime_ms: i64,
}

/// 注册表里有合成历史面的每一家：`(kind, 面)`（注册序）。
pub(crate) fn history_faces() -> Vec<(&'static str, HistoryFace)> {
    REGISTRY
        .iter()
        .filter_map(|a| a.history.map(|h| (a.kind, h)))
        .collect()
}

/// 内容搜索（只扫记录树）不覆盖、而这台上又有它的会话记录的那几家：它们对用户的叫法（注册序）。
pub(crate) fn content_search_skips() -> Vec<&'static str> {
    REGISTRY
        .iter()
        .filter(|a| {
            a.history
                .is_some_and(|h| (h.root)().is_some_and(|r| r.is_dir()))
        })
        .map(|a| a.launch.map_or(a.kind, |l| l.display_name))
        .collect()
}

/// 注册表里每一家合成历史面给的记录根（注册序；说不出的跳过）。按路径读会话的围栏在 Claude 的
/// `projects/` 之外还认这几个（`observe/history_query.rs::validate_session_path`）。
pub(crate) fn history_roots() -> Vec<PathBuf> {
    history_faces()
        .into_iter()
        .filter_map(|(_, h)| (h.root)())
        .collect()
}

/// 一家的资产面：函数指针（同 [`Adapter::home`]，不立 trait）。几件布局知识：怎么扫 · skill 住哪 · 用户级 MCP 住哪。
#[derive(Clone, Copy)]
pub(crate) struct AssetFace {
    /// 按这台机器的环境现解根、现扫：用户级那一份 ＋ 交进来的那几个项目目录（这台上开过会话的项目）。
    /// 用户级 MCP 读哪份文件由调用方交（有账号库的机器上是各号共用的那一份，格式与本家那份同一种）。
    /// 交回原始事实（还没有摘要 —— 摘要是通用层的机器）。
    pub(crate) scan: fn(projects: &[String], user_mcp: Option<&Path>) -> Sightings,
    /// 这一家用户级 skill 的根（「装到这台」读 / 写 skill 的落点从这里来；与扫描同一个家）。
    pub(crate) skills_root: fn() -> Option<PathBuf>,
    /// 一个项目目录里 skill 的根（项目级 skill 的落点，与扫描同一个家）。
    pub(crate) project_skills_root: fn(project: &Path) -> PathBuf,
    /// 这一家自己存用户级 MCP 的那份文件（没有账号库的机器上，用户级 MCP 就是它）。
    pub(crate) user_mcp_file: fn() -> Option<PathBuf>,
}

/// 看到的一个 skill：`project` = `None` 是用户级，`Some(项目目录)` 是那个项目里的。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SkillSeen {
    pub project: Option<String>,
    pub name: String,
    /// 那个 skill 的目录（可能是一条链接：通用层按它跟到底走一遍）。
    pub dir: PathBuf,
    pub description: Option<String>,
}

/// 看到的一条 MCP server：`project` 同 [`SkillSeen::project`]；`file` 是它住的那份配置文件。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct McpSeen {
    pub project: Option<String>,
    pub name: String,
    /// 那一条原样的定义。
    pub def: serde_json::Value,
    pub file: PathBuf,
}

/// 一家适配层看到的原始资产事实。**还没有摘要**（通用层 `asset_catalog.rs` 算）。
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Sightings {
    pub skills: Vec<SkillSeen>,
    pub mcp: Vec<McpSeen>,
    /// 读不出来的那几份（一句话一份）—— 「这台没有」与「这台那份读不出来」不许合成一句。
    pub problems: Vec<String>,
}

/// 注册表里每一家有资产面的，各扫一遍（注册序）。
pub(crate) fn asset_sightings(projects: &[String], user_mcp: Option<&Path>) -> Vec<Sightings> {
    REGISTRY
        .iter()
        .filter_map(|a| a.assets.map(|f| (f.scan)(projects, user_mcp)))
        .collect()
}

/// `kind` 那一家的资产面。
fn asset_face_among(registry: &[Adapter], kind: &str) -> Option<AssetFace> {
    adapter_among(registry, kind).and_then(|a| a.assets)
}

/// `kind` 那一家的用户级 skill 根。那一家没有资产面 ⇒ `None`。
pub(crate) fn skills_root(kind: &str) -> Option<PathBuf> {
    skill_root_among(REGISTRY, kind, None)
}

/// `kind` 那一家的 skill 根：`None` = 用户级（同 [`skills_root`]），`Some(项目目录)` = 那个项目里的。
pub(crate) fn skill_root_at(kind: &str, project: Option<&Path>) -> Option<PathBuf> {
    skill_root_among(REGISTRY, kind, project)
}

/// [`skill_root_at`] 的可喂夹具那一半。
pub(crate) fn skill_root_among(
    registry: &[Adapter],
    kind: &str,
    project: Option<&Path>,
) -> Option<PathBuf> {
    let f = asset_face_among(registry, kind)?;
    match project {
        None => (f.skills_root)(),
        Some(p) => Some((f.project_skills_root)(p)),
    }
}

/// `kind` 那一家的用户级 MCP 住的那份文件。
pub(crate) fn user_mcp_file(kind: &str) -> Option<PathBuf> {
    asset_face_among(REGISTRY, kind).and_then(|f| (f.user_mcp_file)())
}

/// **这个后端认得哪几个 agent**。加一个 agent = 加一行（+ 上面加一行 `mod`）。
///
/// ⚠ 它与 `agent_locality_guard::tests::HOMES`（判据用的"agent 家"清单）**必须一样长**，
/// 由 `every_agent_adapter_has_exactly_one_registry_entry` 双向钉住：
/// 建了 `agents/<名>/` 却不登记 ⇒ 那家永远"看不见"，而没有任何东西会说。
/// ⚠ **一家一行**（不是每字段一行）：`ADAPTER_CALL_SITES` 数的是**行**，
/// 而这张表要回答的是「加一个 agent 要回来改**几处**」——
/// 一家拆成四行会让那个读数变成排版的函数。
#[rustfmt::skip]
pub(crate) const REGISTRY: &[Adapter] = &[
    Adapter { kind: claudecode::AGENT_KIND, home: claudecode::home, account_env: Some(claudecode::paths::CONFIG_DIR_ENV), assets: Some(claudecode::ASSETS), history: None, upstream: Some(claudecode::UPSTREAM), mcp: Some(claudecode::MCP), footprint: Some(claudecode::footprint::FACE), accounts: Some(claudecode::accounts::FACE), records: Some(claudecode::RECORDS), processes: Some(claudecode::cards::PROCESS_NAMES), launch: Some(claudecode::LAUNCH), compact_request: Some(claudecode::COMPACT_REQUEST) },
    // codex 今天没有账号维度（`account_env: None`）：选号对它说不出，起法与 `ccm` 都明说不行。
    // codex **刻意不登记**默认上游：它的默认上游是哪一个、认不认 base URL 覆盖，本仓零证据（`C7`）⇒ 未登记即拒（fail-closed）。
    Adapter { kind: codex::AGENT_KIND,      home: codex::home,      account_env: None, assets: None, history: Some(codex::HISTORY), upstream: None, mcp: None, footprint: None, accounts: None, records: Some(codex::RECORDS), processes: None, launch: Some(codex::LAUNCH), compact_request: None },
];

/// 某一家的账号载体（环境变量名）。认不出这家 ⇒ `None`。
///
/// ⚠ **它是通用层拿这个名字的唯一入口** —— 直接 `use agents::<名>::…` 会让
/// `agent_locality_guard` 判据④的读数凭空上涨，而那个数只许降（见 [`Adapter::account_env`]）。
pub(crate) fn account_env_of(kind: &str) -> Option<&'static str> {
    REGISTRY
        .iter()
        .find(|a| a.kind == kind)
        .and_then(|a| a.account_env)
}

/// 某一家请求压缩上下文用的那一句（[`Adapter::compact_request`]）。认不出这家 / 这一家不支持 ⇒ `None`。
pub(crate) fn compact_request_of(kind: &str) -> Option<&'static str> {
    REGISTRY
        .iter()
        .find(|a| a.kind == kind)
        .and_then(|a| a.compact_request)
}

/// `kind` 那一家的这条记录是不是压缩之后续接用的摘要（同渲染模型里 `compactSummary` 那一种来源）。
pub(crate) fn is_compact_summary(kind: &str, v: &serde_json::Value) -> bool {
    user_text_of(kind, v).is_some_and(|t| t.speaker == Speaker::CompactSummary)
}

/// **这台机器上看得见哪些 agent** —— 直接产出 `hello.homes` 的那张表〔`S5`，`G1` 成功标准③〕。
///
/// # 判准是「**home 目录存在**」，三选一，理由写在这里
///
/// 候选三条语义不同：home 目录存在（**装了**）· 目录下有会话记录（**用过**）·
/// 可执行文件在 `PATH` 里（**能起**）。取第一条：
///
/// 1. **字段语义只允许这一条**。`homes` 的每一项是 `{agent_kind, path}` —— 一个**路径**。
///    按 `PATH` 判的话，claude 那家 [`claudecode::home`] 恒 `Some`（有默认值），
///    我们就得为一个**不存在的目录**报一个路径 —— 那是在说谎，消费方拿它去列会话只会得到空。
/// 2. **它与后端真正能干的事对齐**。四类能力里的三类（会话发现与判活 · 会话内容读 ·
///    用量）**全部** root 在 home 之下；home 不在，这三类一律零输出。
///    ⇒「home 在」就是「backend 对这个 agent 的观测面能干活」。
///    ⚠ 第四类（起会话/resume）确实靠 `PATH`，本判准**覆盖不到** —— 如实登记，见 `§4`。
/// 3. **排除「目录下有会话记录」**：那是"用过"不是"装了"。三条具体害处 ——
///    ① 刚装好还没开过会话的 agent 会被判成看不见，而后端的 watcher 正是要
///    inotify 那个目录**等第一条会话出现**（自相矛盾）；② `hello` 一次连接只发一次，
///    而"有没有会话记录"在连接期内会变（清理/轮转）—— 会中途变旧的值不该进握手帧；
///    ③ 它要扫盘，而 hello 是 flush 前的**第一件事**，给握手加一次全树扫描是加了一段不确定的延迟。
/// 4. **排除「可执行文件在 `PATH` 里」**：① 它答不出 `path` 该填什么（见 1）；
///    ② backend 自己的 `PATH` 与用户在 tmux 里的 `PATH` 常常不是一回事（非交互 SSH 登录）
///    —— 拿后端的 `PATH` 判"用户能不能起 claude"是**问错了人群**；
///    ③ 它答的是"能不能起"，而 `homes` 声明的是"数据在哪"。
///
/// ⇒ 对 `S6` 的含义（判准决定假 agent 怎么伪造自己）：**`mkdir` 一个 home 目录就够了**。
/// 这是三条里最容易伪造的一条，而这正合适 —— `S6` 要证的是"通用层零改动"，
/// 不是"假 agent 起得来"。
///
/// # ⚠ 生产路径今天**不调它**
///
/// 见本模块头注：`main.rs` 仍硬写 `homes: Vec::new()`。摘掉下面这个 `allow` 的那天，
/// 就是把那一行换成 `agents::visible_homes()` 的那天。
#[allow(dead_code)] // `S5`：能填不真填 —— 接线是一次纯发布决策，不是忘了。
pub(crate) fn visible_homes() -> Vec<AgentHome> {
    visible_among(REGISTRY)
}

/// [`visible_homes`] 的**可喂夹具**那一半：注册表进、看得见的那些出，**顺序保持注册序**。
///
/// # 为什么要多这一层（而不是让 [`visible_homes`] 直接读 `REGISTRY`）
///
/// 真 home 由**环境变量**解析（`CLAUDE_CONFIG_DIR` / `HOME` / `CODEX_HOME`），
/// 而在测试里改进程环境既不可靠（并行跑的别的测试也在读）又不安全。
/// 传注册表进来之后，判据可以喂一张**合成注册表**（`Adapter.home` 是 `fn` 指针 ⇒
/// 非捕获闭包/普通 fn 就够），于是**整条链**（`None` 候选 · 缺席的 home · 同名文件 ·
/// 顺序）全部可以在夹具里验，不依赖跑测试这台机器上装了什么。
///
/// ⚠ 它**仍然真的碰文件系统** —— 判准就是文件系统事实，把那一步也抽成参数就只剩一个
/// 恒真的壳，那种"能力"验的是它自己。
///
/// ★ `S6` 的入口就是这里：最小假 agent = 一条 `Adapter` + 一个 `mkdir` 出来的 home。
#[allow(dead_code)] // 同上：唯一的生产调用点是 `visible_homes`，而它今天不接线。
pub(crate) fn visible_among(registry: &[Adapter]) -> Vec<AgentHome> {
    registry
        .iter()
        // `None` = 这家连候选路径都说不出（如 codex 在没有 `HOME`/`CODEX_HOME` 的环境里）
        // ⇒ 直接出局，不要拿一个空路径去 stat。
        .filter_map(|a| (a.home)().map(|home| (a.kind, home)))
        .filter(|(_, home)| home_is_visible(home))
        .map(|(kind, home)| AgentHome {
            agent_kind: kind.to_string(),
            path: home.to_string_lossy().into_owned(),
        })
        .collect()
}

/// 判准本身：**是一个存在的目录**。
///
/// ⚠ `is_dir()` 而不是 `exists()`：同名的**文件**不是一个 agent 的家。
/// 这条区分不是洁癖 —— 报出去的 `path` 会被消费方当目录去拼子路径。
#[allow(dead_code)] // 同上。
fn home_is_visible(home: &Path) -> bool {
    home.is_dir()
}

#[cfg(test)]
#[path = "../../../tests/backend/agents_tests.rs"]
mod tests;
