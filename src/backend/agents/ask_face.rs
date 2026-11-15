//! 答题口：`AskFace` 经那一家的 hook 把待答请求交常驻后端、把答案交回。

use super::*;

/// 一家的**答题口**（[`Adapter::asks`]）：那一家的 hook 输入长什么样、答案写成什么、起会话时怎么挂上 hook，只住适配层；
/// 待答登记表、选项怎么写字、谁先答算谁的住通用层（`control::asks`）。
#[derive(Debug, Clone, Copy)]
pub struct AskFace {
    /// 起会话时垫在启动器后面的那几个词（只给这一个会话挂上答题 hook，不碰用户自己的设置）；入参 ＝ 这台 `ccm` 的绝对路径。
    pub launch_args: fn(&str) -> Vec<String>,
    /// 会话级设置那个旗标（[`AskFace::merge`] 有 ⇒ 透传里已经有它就不另垫、并进用户那一份）。
    pub launch_flag: &'static str,
    /// 用户自己给了那个旗标 ⇒ 把答题 hook 并进他那一份（原样留着他别的一切，只往那一种 hook 的列表末尾加一项）；
    /// 入参 ＝ 他那一份解析出来的值 · ccm 入口。形状不是那一家认的 ⇒ `None`（不挂，这条会话只能在终端里答批准）。
    /// 整格 `None` ＝ 那个旗标可重复、各写各的键（`-c key=value` 那一形）：用户写的那几条不读不并，[`AskFace::launch_args`] 照垫。
    pub merge: Option<fn(&serde_json::Value, &str) -> Option<serde_json::Value>>,
    /// 版本门（[`HookGate`]）：这一家挂 hook 的写法只在某一版起核过 ⇒ 起会话前看装的那一版，够不着 ⇒ 不挂、标版本。`None` ＝ 不看版本。
    pub gate: Option<HookGate>,
    /// 透传参数里有没有这一家拿来**整格盖掉**我们垫的钩子表的写法（[`AskFace::merge`] 为空的那一家，用户自己写了整张钩子表）：
    /// 认出 ⇒ 照旧起、不动用户的参数，这一趟当没挂钩子、标「被盖掉」（`HookOff::Overridden`）。`None` ＝ 不诊断。
    pub overridden: Option<fn(&[String]) -> bool>,
    /// hook 交来的那一份 ⇒ 通用的待答请求；不是要人答的那一种（别的事件 · 形状认不出）⇒ `None`（hook 空手退）。
    pub read: fn(&serde_json::Value) -> Option<AskIn>,
    /// 人的答案 ⇒ hook 要打到 stdout 的那一整份（允许 · 拒绝 · 答了的提问各是那一家要的那一形，hook 原样打、不自己拼）；
    /// `None` ＝ 这一家写不出这个决定（不给决定，框照常在终端里问）。
    pub decide: fn(&AskIn, &Verdict) -> Option<serde_json::Value>,
    /// 这一家**不给经钩子答的口**的那几种请求（比如提问）：认了 ⇒ hook 即刻退（那一家照常在终端里问）、登记表不挂，
    /// 这条会话记一条需手动，原因码 `HookOff::NoAnswer`。哪一种都答得了 ⇒ [`answers_every_kind`]。
    pub cannot: fn(&AskIn) -> bool,
    /// 这一家拒绝时写得出「并停下这一轮」（[`Verdict::Deny`] 的 `stop`）：假 ⇒ 待答成品里就不带「拒绝并停下」那一项
    /// （[`super::asks::product_of`]），两端照成品摆、不另判。
    pub stop_on_deny: bool,
    /// 同一组 hook 交来的别的那几种 ⇒（会话 id, 统一事件）：这一家有「钩子」这一档（会话在干什么先照它，pidfile 只兜底）。
    /// 是要人答的那一种 · 认不出 ⇒ `None`。
    pub heard: fn(&serde_json::Value) -> Option<(String, AgentEvent)>,
    /// 换了启动器那一支：答题 hook 做成一个小插件、经环境交下去（启动器后面的词不动）；`None` ＝ 这一家没有这一路（照「自定义启动器」标挂不上）。
    pub plugin: Option<PluginFace>,
    /// 挡着终端的那一种（[`AskIn::blocks_terminal`]）在终端里那个框怎么认、按哪个键答（app 里答 ⇒ 读屏认准才按）；
    /// `None` ＝ 这一家没有认框的那一份（这一种在 app 里只写「在终端里答」、一项都不给选）。
    pub on_screen: Option<OnScreen>,
}

impl AskFace {
    /// 这一条挡着终端，而它在终端里的那个框这一家认得出 ⇒ app 里经读屏答（成品给「允许 · 拒绝」，[`super::asks::product_of`] 照它）。
    pub fn answers_on_screen(&self, a: &AskIn) -> bool {
        a.blocks_terminal && self.on_screen.is_some_and(|s| (s.covers)(a))
    }
}

/// 挡着终端的那一条在终端画面上的框（[`AskFace::on_screen`]）：认法与按键只住适配层，每家一份；
/// 读屏 · 按 · 按完复核住通用层（读屏驱动）。
#[derive(Debug, Clone, Copy)]
pub struct OnScreen {
    /// 这一条请求的框认得出来（种类 · 工具这一家认过排法的那几种）；假 ⇒ 不给选、只写「在终端里答」。
    pub covers: fn(&AskIn) -> bool,
    /// 此刻那一屏（纯文本行）上**正显示着的**就是这一条的框（排着的不画、不算）；认不准 ⇒ 假（一个键不按）。
    pub seen: fn(&[String], &AskIn) -> bool,
    /// 此刻那一屏上说还排着几条（连正显示的这一条）；屏上不说（只一条 · 窗太窄不画）⇒ `None`。
    /// 比登记表认得的多 ⇒ 正显示的可能是认不得的那一条（后端重启前挂上的），一个键不按。
    pub queued: fn(&[String]) -> Option<u32>,
    /// 允许按的那一下。
    pub allow: Keystroke,
    /// 拒绝（只拒这一条）按的那一下。
    pub deny: Keystroke,
}

/// 往终端里按的一下：键表里的一个键（`terminal-input` 键表的名字），或一个字（字面送、不带回车）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keystroke {
    Key(&'static str),
    Char(&'static str),
}

/// 一家挂答题 hook 的**版本门**（唯一一形：[`AskFace::gate`] 与 [`PluginFace::gate`] 都是它）：
/// 认得这一写法的最低版本 ＋ 读装着的那一版要的两样声明（读法只一处：[`installed_version`]）。
/// 不过门的原因只一种：[`HookGate::admits`] 交 `HookOff::Version`。
#[derive(Debug, Clone, Copy)]
pub struct HookGate {
    /// 照它核过的最低版本。
    pub min_version: [u64; 3],
    /// `PATH` 上找的那个命令名。
    pub command: &'static str,
    /// 那一家的 npm 包名（npm 安装那一形往上找它的 `package.json`，名字对得上才算）。
    pub package: &'static str,
}

impl HookGate {
    /// 装着的那一版（认不出 ⇒ `None`）过不过这道门：不过 ⇒ 挂不上的原因（版本）。
    pub fn admits(&self, installed: Option<[u64; 3]>) -> Result<(), super::asks::HookOff> {
        match installed {
            Some(v) if v >= self.min_version => Ok(()),
            _ => Err(super::asks::HookOff::Version),
        }
    }
}

/// ★ 「这台 `PATH` 上装的那一家是哪一版」（唯一一处，哪一家都走它）：照 [`HookGate::command`] 找到那个命令、顺着链接找到真身；
/// 真身文件名本身是版本号（原生安装 `…/versions/<版本>`）⇒ 它；否则往上四层找名字是 [`HookGate::package`] 的 `package.json`。
/// 找不到 · 包名对不上 · 版本不是三段整数 ⇒ `None`。
pub fn installed_version(gate: &HookGate, path: Option<&str>) -> Option<[u64; 3]> {
    let crate::platform::shell::SessionCommand::Found { file: found, .. } =
        crate::platform::shell::find_session_command(gate.command, path, cfg!(windows), &|p| {
            crate::platform::fs::Ro::metadata(p).is_ok_and(|m| m.is_file())
        })
    else {
        return None;
    };
    let real = crate::platform::fs::Ro::canonicalize(&found).ok()?;
    if let Some(v) = real.file_name().and_then(|n| semver3(&n.to_string_lossy())) {
        return Some(v);
    }
    for dir in real.ancestors().skip(1).take(4) {
        let manifest = dir.join("package.json");
        if let Ok(text) = crate::platform::fs::Ro::read_string_capped(manifest, PACKAGE_JSON_CAP) {
            if let Some(v) = version_in_package(&text, gate.package) {
                return Some(v);
            }
        }
    }
    None
}

/// 读那一家 `package.json` 的上限（几 KB 的清单）：超了那一份跳过（读的那一口 `warn!` 带路径），这一层当没有、接着往上找。
const PACKAGE_JSON_CAP: cap_core::Cap = cap_core::Cap {
    bytes: 1024 * 1024,
    over: cap_core::Over::SkipAndSay,
};

/// 一份 `package.json`（名字是 `package`）⇒ 它的版本。
fn version_in_package(text: &str, package: &str) -> Option<[u64; 3]> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("name").and_then(serde_json::Value::as_str) != Some(package) {
        return None;
    }
    semver3(v.get("version")?.as_str()?)
}

/// `1.2.3` ⇒ `[1, 2, 3]`；少一段 · 多一段 · 带尾巴 ⇒ `None`。
fn semver3(s: &str) -> Option<[u64; 3]> {
    let mut it = s.split('.');
    let mut out = [0u64; 3];
    for slot in &mut out {
        *slot = it.next()?.parse().ok()?;
    }
    it.next().is_none().then_some(out)
}

/// 一家的**会话插件**那一路（[`AskFace::plugin`]）：插件目录经哪个环境变量交下去、里面放什么、哪些情形挂不上。
#[derive(Debug, Clone, Copy)]
pub struct PluginFace {
    /// 那一家认的插件目录清单变量（多个目录用这台的路径分隔符连）。
    pub env: &'static str,
    /// 认这个变量的版本门（与 [`AskFace::gate`] 同一形，计划那一侧同一处判）。
    pub gate: HookGate,
    /// 插件目录里的每一份文件：（相对路径, 内容）；入参 ＝ ccm 绝对路径。
    pub files: fn(&str) -> Vec<(&'static str, String)>,
    /// 这台的 managed 策略文件（起会话前读；服务端下发的那一份事先读不到）。
    pub policy_files: fn() -> Vec<std::path::PathBuf>,
    /// 一份 managed 策略不许这一路（设了变量起不来 · 插件里的 hook 被拦）。
    pub policy_blocks: fn(&serde_json::Value) -> bool,
    /// 那一家导给它起的每个子进程的嵌套标记（ccm 起会话前清掉它 ⇒ 发问的进程起来时带着它 ＝ 会话里 Bash 起的子进程）。
    pub child_marker: &'static str,
}

/// **统一事件**：那一家经钩子交来的、落成「此刻在干什么」的那两样（[`AskFace::heard`] 翻好）。
/// 「在等批准 · 在问」不在这里：它们是待答请求（[`AskIn`]），挂着就是等人。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEvent {
    /// 用户提交了一句 ⇒ 一轮起来了。
    PromptSubmitted,
    /// 一轮结束（那一家说它闲着、等下一句）。
    TurnEnded,
}

impl AgentEvent {
    /// 这件事落成的那一态。
    pub fn activity(self) -> SessionActivity {
        match self {
            AgentEvent::PromptSubmitted => SessionActivity::Working,
            AgentEvent::TurnEnded => SessionActivity::Idle,
        }
    }
}

/// 一次待答请求（适配层翻好的通用形；那一家的字段名不出适配层）。
#[derive(Debug, Clone, PartialEq)]
pub struct AskIn {
    /// 哪条会话（那一家的会话 id）。
    pub sid: String,
    /// 种类：批准 · 回答 · 批计划（只这三种；别的框 hook 管不着）。
    pub kind: AskKind,
    /// 工具名原样。
    pub tool: String,
    /// 一行摘要（钉条里那一段：命令 · 路径 · 问题头一行）；说不出 ⇒ `None`。
    pub what: Option<String>,
    /// 要批的东西的全文（命令全文 · 文件路径与改动 · 计划原文），不截。
    pub detail: String,
    /// 那一家建议的「以后不再问」那几条：写好的字 ＋ 原样（答的时候原样交回）。
    pub suggestions: Vec<AskSuggestion>,
    /// 提问那一种的题目（与过程里那张提问卡同一形）；别的种类为空。
    pub questions: Vec<AskQuestion>,
    /// 是子 agent 发的 ⇒ 它的 id 与种类（那一家原样）。
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    /// 这个请求**挡着终端**：hook 没答完之前终端里不出框（那一家先跑 hook 再弹框的那几种，如后台子 agent）。
    /// 为真 ⇒ hook 交给常驻就放手（框当场在终端里出），常驻记成「终端里有框」：那一家认得出那个框 ⇒ app 里给「允许 · 拒绝」、经读屏答（[`AskFace::on_screen`]），
    /// 认不得 ⇒ 一项都不给选、在终端里答；终端那边答没答由常驻那一侧判。
    pub blocks_terminal: bool,
    /// 那一家交来的工具参数原样（答的时候适配层要它）。
    pub input: serde_json::Value,
}

/// 「允许，且以后不再问」那一条建议：适配层认出的码 ＋ 那一家的原样（答的时候原样交回）。写成人话只在 [`AskSuggestion::said`]。
#[derive(Debug, Clone, PartialEq)]
pub struct AskSuggestion {
    /// 记在哪一层。
    pub scope: SuggestScope,
    /// 放宽的是什么。
    pub rule: SuggestRule,
    pub raw: serde_json::Value,
}

/// 「以后不再问」记在哪一层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestScope {
    /// 只这一次会话。
    Session,
    /// 本机这个项目（不进仓的那一份设置）。
    Local,
    /// 这个项目。
    Project,
    /// 这个用户。
    User,
}

/// 「以后不再问」放宽的是什么（只这几种放宽的；删规则 · 删目录那种不给）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestRule {
    /// 允许访问这几个目录（原样）。
    Dirs(Vec<String>),
    /// 切到自动接受编辑。
    AcceptEdits,
    /// 切到别的那一档（档名原样）。
    Mode(String),
    /// 允许这几条工具规则（`工具(内容)` 原样）。
    Allow(Vec<String>),
}

impl AskSuggestion {
    /// ★ 写成人话的那一段（唯一一处，「本次会话里允许访问 /a/b」）：答题块「以后不再问」那一颗照它写。
    pub fn said(&self) -> String {
        use copy_core::copy_text;
        let scope = match self.scope {
            SuggestScope::Session => copy_text("beAsk.scope.session", &[]),
            SuggestScope::Local => copy_text("beAsk.scope.local", &[]),
            SuggestScope::Project => copy_text("beSessionCommands.scope.project", &[]),
            SuggestScope::User => copy_text("beAsk.scope.user", &[]),
        };
        match &self.rule {
            SuggestRule::Dirs(dirs) => copy_text(
                "beAsk.rule.dirs",
                &[("dirs", &dirs.join(", ")), ("scope", &scope)],
            ),
            SuggestRule::AcceptEdits => copy_text("beAsk.rule.acceptEdits", &[("scope", &scope)]),
            SuggestRule::Mode(m) => copy_text("beAsk.rule.mode", &[("mode", m), ("scope", &scope)]),
            SuggestRule::Allow(rules) => copy_text(
                "beAsk.rule.rules",
                &[("rules", &rules.join(", ")), ("scope", &scope)],
            ),
        }
    }
}

/// 人的答案（通用形，适配层翻成那一家的决定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 允许。`rule` ＝ 带上第几条建议（以后不再问）；`mode` ＝ 批计划时切到哪一档；`answers` ＝ 提问那一种每题的答案（题目原文, 答案）。
    Allow {
        rule: Option<usize>,
        mode: Option<PlanMode>,
        answers: Vec<(String, String)>,
    },
    /// 拒绝。`note` ＝ 给它看的那句话；`stop` ＝ 拒绝并停下这一轮。
    Deny { note: Option<String>, stop: bool },
}

/// 批计划之后切到哪一档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanMode {
    /// 自动模式（那一家替人判要不要问）。
    Auto,
    /// 自动接受编辑。
    AcceptEdits,
    /// 逐个确认。
    Manual,
}

/// [`AskFace::cannot`] 那一格的「哪一种都答得了」。
pub fn answers_every_kind(_: &AskIn) -> bool {
    false
}

/// `kind` 那一家的答题口（[`Adapter::asks`]）；认不出 / 没有 ⇒ `None`。
pub fn ask_face_among(registry: &[Adapter], kind: &str) -> Option<AskFace> {
    adapter_among(registry, kind).and_then(|a| a.asks)
}

/// 同上，生产注册表。
pub fn ask_face(kind: &str) -> Option<AskFace> {
    ask_face_among(REGISTRY, kind)
}

/// 各家经环境交会话插件的那个变量（[`PluginFace::env`]）：会话事实判「插件到没到那个进程里」按它们读。
/// 去重照集合去（几家报同一个、不挨着也只留一个），顺序按注册表、留第一次出现的那个位置。
pub fn plugin_envs() -> Vec<&'static str> {
    plugin_envs_among(REGISTRY)
}

/// [`plugin_envs`] 在给定注册表上（判据喂替身家拼的那一张）。
pub(crate) fn plugin_envs_among(registry: &[Adapter]) -> Vec<&'static str> {
    let mut seen = std::collections::HashSet::new();
    registry
        .iter()
        .filter_map(|a| a.asks.and_then(|f| f.plugin).map(|p| p.env))
        .filter(|env| seen.insert(*env))
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/ask_face_tests.rs"]
mod tests;
