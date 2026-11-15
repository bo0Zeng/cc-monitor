//! 资产面：`AssetFace` 一家的 skill · MCP · 插件住哪、怎么认。

use super::*;

/// 声明了资产面的每一家（注册序）：资产目录逐家扫一份（[`asset_sightings`] 照它走）。
pub fn asset_kinds_among(registry: &[Adapter]) -> Vec<&'static str> {
    registry
        .iter()
        .filter(|a| a.assets.is_some())
        .map(|a| a.kind)
        .collect()
}

/// 没说是哪一家的那几问（资产目录的缺省家 · 枢纽 MCP 那几跳 · 装 skill 不指名）：默认起的那一家（`LaunchFace.is_default`），
/// 它声明了资产面才算（照资产面问 [`kind_with_face_among`]）。
pub fn asset_default_among(registry: &[Adapter]) -> Option<&'static str> {
    kind_with_face_among(registry, None, |a| a.assets.is_some())
}

/// 「默认那一家不写 · 不画家」全仓只此一处判：`kind` 就是 `default` ⇒ `None`，否则 ⇒ `kind`。
/// 默认是哪一家由调用方交，有三种、各管各的：
/// - 资产那几问（资产目录的条目 · `mcp-read` 的每条 · 请求里的家规整，经 [`family_tag`]）交 [`asset_default_among`]：
///   起会话默认的那一家、它声明了资产面才算（没声明 ⇒ `None`，每一家都写家）；
/// - 历史清单的家名小牌（`history_list::agent_tag_among`）交 [`default_launch_among`] 那一家：起会话默认的那一家，不看资产面；
/// - 账号页 MCP 在共享集合里住哪一格（`mcp_share_exec::slots`）交 [`account_kind_among`]`(reg, None)` 那一家：账号默认的那一家住顶层。
pub fn unless_default<'a>(kind: &'a str, default: Option<&str>) -> Option<&'a str> {
    (Some(kind) != default).then_some(kind)
}

/// 线上那一格家：缺省那一家（`default`，即 [`asset_default_among`] 那一家）记成缺（同只有一家时那一形），
/// 别的家写上它（判在 [`unless_default`]）。资产目录的条目 · `mcp-read` 的每条 · 请求里的家规整都调它。
pub fn family_tag(kind: &str, default: Option<&str>) -> Option<cmd_wire::AgentKind> {
    unless_default(kind, default).map(cmd_wire::AgentKind::from)
}

/// 一家对人的叫法：它行头（[`Adapter::display_name`]）声明的那个名字；没声明 · 注册表里没有 ⇒ 照实说没声明（文案表那一句，不露 kind）。
pub fn family_said_among(registry: &[Adapter], kind: &str) -> String {
    adapter_among(registry, kind)
        .and_then(|a| a.display_name)
        .map_or_else(
            || copy_core::copy_text("rsAgents.family.unsaid", &[]),
            str::to_string,
        )
}

/// 一张单子（扩展页的行 · 账号页 MCP 的条目）每条写不写家的叫法，**全仓只此一处判**：每条的家缺 ⇒ 认 `default`
/// （打标签时省掉的那一家：资产那几问交 [`asset_default_among`]，账号页 MCP 交账号默认那一家；明写它的与缺家的算同一家）；
/// 认出来的家去重后不止一家 ⇒ 每条那一家的叫法（[`family_said_among`]）；只一家 ⇒ 都 `None`（界面不画家）。
/// `default` 是 `None`（没有默认那一家）却有缺家的条目 ⇒ 缺家的算另一家、那几条 `None`。
/// 回的与 `agents` 逐条对齐。
pub fn said_when_several(
    registry: &[Adapter],
    default: Option<&str>,
    agents: &[Option<&str>],
) -> Vec<Option<String>> {
    let fams: Vec<Option<&str>> = agents.iter().map(|a| a.or(default)).collect();
    let mut distinct = fams.clone();
    distinct.sort_unstable();
    distinct.dedup();
    let several = distinct.len() > 1;
    fams.into_iter()
        .map(|k| {
            k.filter(|_| several)
                .map(|k| family_said_among(registry, k))
        })
        .collect()
}

/// 一家的资产面：函数指针（同 [`Adapter::home`]，不立 trait）。几件布局知识：怎么扫 · skill 住哪 · 用户级 MCP 住哪。
#[derive(Clone, Copy)]
pub struct AssetFace {
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
    /// 项目级 MCP 配置的文件名（项目目录下；增 / 改 / 删 · 推 / 拉的落点）。
    pub project_mcp_file: &'static str,
    /// MCP 配置文件里装 server 表的那个顶层键。
    pub servers_key: &'static str,
    /// 一个配置根底下装着的插件：插件根 ＋ 它清单里的名字（用户级 skill 目录里放进来的 · 插件缓存里每个版本一个）。
    pub(crate) plugins: fn(config_root: &Path) -> Vec<(PathBuf, String)>,
    /// 插件装 · 删 · 列各跑哪条命令（[`PluginCells`]）。没声明 ⇒ 这一家不出插件装删那几项。
    pub plugin_cells: Option<PluginCells>,
    /// 一条会话能用的命令与 skill：那条会话的项目目录（说不出 ⇒ `None`）＋ 它用的那个号的配置根，现扫。
    pub session_scan: fn(project: Option<&Path>, config_root: &Path) -> SessionAssets,
    /// 这一家内置的斜杠命令（不住文件里的那几条）。
    pub builtin_commands: &'static [BuiltinCommand],
}

/// 一家的插件装删那一格（[`AssetFace::plugin_cells`]）：装 · 删 · 列各跑哪条命令，成败由读者读退出码。
/// 装 · 删交的是那一家自己的写法（[`PluginSeen::key`] · [`PluginListed::key`]，带不带市场是那一家的事），通用层不拆不拼。
#[derive(Clone, Copy)]
pub struct PluginCells {
    /// 装一个插件要敲的那一行（`key` = 那一家的写法）。
    pub install: fn(key: &str) -> Vec<String>,
    /// 删一个插件要敲的那一行（`key` 同上）。
    pub remove: fn(key: &str) -> Vec<String>,
    /// 列出装了哪些要敲的那一行。
    pub list: fn() -> Vec<String>,
    /// 列那一行的标准输出 ⇒ 装着的那几个。读不懂 ⇒ `None`（不猜）。纯函数。
    pub read_list: fn(stdout: &str) -> Option<Vec<PluginListed>>,
}

/// 列出来的一个插件（[`PluginCells::read_list`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginListed {
    /// 那一家自己的写法（装 · 删交回去的就是它；扫目录那一份的 [`PluginSeen::key`] 照它比）。
    pub key: String,
    /// 光名字（同 [`PluginSeen::name`]，由那一家给）：界面只交光名字，卸那一跳照它找出那一条、再交它的写法。
    pub name: String,
    /// 版本；那一家没说 ⇒ 缺。
    pub version: Option<String>,
    /// 开着。
    pub on: bool,
}

/// 一条内置的斜杠命令（[`AssetFace::builtin_commands`]）。
#[derive(Debug, Clone, Copy)]
pub struct BuiltinCommand {
    /// 命令名（不带 `/`）。
    pub name: &'static str,
    /// 写给人看的简介（经文案表取，键在适配层写成字面量）。
    pub text: fn() -> String,
    /// 要跟参数（填进输入框后停住等补）。
    pub needs_args: bool,
    /// 发之前先确认（会丢掉东西的那几条）。
    pub confirm: bool,
}

/// 一条会话能用的命令与 skill（[`AssetFace::session_scan`] 的原始事实，字由通用层写）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionAssets {
    /// 本项目的在前、号的在后，各按名字排。
    pub commands: Vec<CommandSeen>,
    pub skills: Vec<SkillSeen>,
    /// 读不出来的那几份（一句一份）。
    pub problems: Vec<String>,
}

/// 看到的一条住在文件里的斜杠命令。
#[derive(Debug, Clone, PartialEq)]
pub struct CommandSeen {
    pub name: String,
    /// 住在本项目里（否则住在号的配置根里）。
    pub project: bool,
    pub description: Option<String>,
    pub needs_args: bool,
}

/// 看到的一个 skill：`project` = `None` 是用户级，`Some(项目目录)` 是那个项目里的。
#[derive(Debug, Clone, PartialEq)]
pub struct SkillSeen {
    pub project: Option<String>,
    pub name: String,
    /// 那个 skill 的目录（可能是一条链接：通用层按它跟到底走一遍）。
    pub dir: PathBuf,
    pub description: Option<String>,
}

/// 看到的一条 MCP server：`project` 同 [`SkillSeen::project`]；`file` 是它住的那份配置文件。
#[derive(Debug, Clone, PartialEq)]
pub struct McpSeen {
    pub project: Option<String>,
    pub name: String,
    /// 那一条原样的定义。
    pub def: serde_json::Value,
    pub file: PathBuf,
}

/// 一家适配层看到的原始资产事实。**还没有摘要**（通用层 `asset_catalog.rs` 算）。
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Sightings {
    pub skills: Vec<SkillSeen>,
    pub mcp: Vec<McpSeen>,
    pub plugins: Vec<PluginSeen>,
    /// 读不出来的那几份（一句话一份）—— 「这台没有」与「这台那份读不出来」不许合成一句。
    pub problems: Vec<String>,
}

/// 看到的一个插件：`project` 同 [`SkillSeen::project`]（装在那个项目里，或那个项目的设置里写了它开不开）。
#[derive(Debug, Clone, PartialEq)]
pub struct PluginSeen {
    pub project: Option<String>,
    /// 清单里的名字。
    pub name: String,
    /// 那一家自己的写法（装 · 删交的、与它自己列出来的比的就是它；Claude 是安装记录的键）。扫出时由那一家给，通用层不拼。
    pub key: String,
    /// 它从哪个市场来（`enabledPlugins` 里 `<名>@<市场>` 的后一半）。
    pub market: String,
    /// 插件根（清单在它下面）。
    pub root: PathBuf,
    /// 清单里的 `version` / `description`（原话）。
    pub version: Option<String>,
    pub description: Option<String>,
    pub parts: PluginParts,
    /// 合并完那一家所有设置之后开着。
    pub on: bool,
    /// 那个开 / 关出自哪儿。
    pub from: PluginFrom,
}

/// 一个插件带了什么：每样列名字（排好序）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginParts {
    pub skills: Vec<String>,
    pub commands: Vec<String>,
    pub agents: Vec<String>,
    pub hooks: Vec<String>,
    pub mcp: Vec<String>,
}

/// 插件开不开那一句出自哪儿（码；字由通用层写）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginFrom {
    /// 托管设置（策略文件 · 远端下发的那一份）。
    Managed,
    /// 项目里只在这台的那份设置。
    Local,
    /// 项目设置。
    Project,
    /// 用户设置。
    User,
    /// 哪份设置都没写，照清单默认。
    Manifest,
}

/// 这几个配置根底下装着的插件（插件根 ＋ 名字）：逐家问（注册序）、配置根的次序。
pub fn plugins(config_roots: &[PathBuf]) -> Vec<(PathBuf, String)> {
    REGISTRY
        .iter()
        .filter_map(|a| a.assets)
        .flat_map(|f| config_roots.iter().flat_map(move |r| (f.plugins)(r)))
        .collect()
}

/// 注册表里每一家有资产面的，各扫一遍（注册序），每一份带上是哪一家。
/// `user_mcp`：那一家的用户级 MCP 读哪份（有账号库的机器上是那一家各号共用的那一份）；`None` ⇒ 交给那一家的扫描自己找。
pub fn asset_sightings(
    projects: &[String],
    user_mcp: &dyn Fn(&'static str) -> Option<PathBuf>,
) -> Vec<(&'static str, Sightings)> {
    asset_sightings_among(REGISTRY, projects, user_mcp)
}

/// [`asset_sightings`] 的可喂夹具那一半。
pub fn asset_sightings_among(
    registry: &[Adapter],
    projects: &[String],
    user_mcp: &dyn Fn(&'static str) -> Option<PathBuf>,
) -> Vec<(&'static str, Sightings)> {
    asset_kinds_among(registry)
        .into_iter()
        .filter_map(|kind| {
            let f = asset_face_among(registry, kind)?;
            let mcp = user_mcp(kind);
            Some((kind, (f.scan)(projects, mcp.as_deref())))
        })
        .collect()
}

/// `kind` 那一家的资产面。认不出 / 没有 ⇒ `None`。
pub fn asset_face(kind: &str) -> Option<AssetFace> {
    asset_face_among(REGISTRY, kind)
}

/// `kind` 那一家的资产面（注册表由调用方交）。
pub fn asset_face_among(registry: &[Adapter], kind: &str) -> Option<AssetFace> {
    adapter_among(registry, kind).and_then(|a| a.assets)
}

/// `kind` 那一家的 skill 根：`None` = 用户级，`Some(项目目录)` = 那个项目里的；注册表由调用方交。
/// 全仓只有 `assets::skill_install::skill_root_among` 调它（取根照家只在那一处拼）。
pub fn skill_root_among(
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

/// `kind` 那一家的插件装删那一格。认不出 · 没有资产面 · 没声明 ⇒ `None`（读者 `assets::plugin_exec` 照它跑、只判这一次）。
pub fn plugin_cells_among(registry: &[Adapter], kind: &str) -> Option<PluginCells> {
    asset_face_among(registry, kind).and_then(|f| f.plugin_cells)
}

/// `kind` 那一家的用户级 MCP 住的那份文件。
pub fn user_mcp_file(kind: &str) -> Option<PathBuf> {
    asset_face_among(REGISTRY, kind).and_then(|f| (f.user_mcp_file)())
}
