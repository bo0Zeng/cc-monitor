//! 起会话面：`LaunchFace` 一家的起会话事实（权限档 · 模型 · 回退），收 agent 名字的唯一认法。

use super::*;

/// 〔加一个 agent 只改 `agents/`〕一家的起会话事实 —— **唯一的家**。
/// `ccm` 按注册表读（[`launch_face_among`]），界面与 monitor 读从这里生成的 `src/frontend/ui/generated/agent-profile-table.ts`。
// 不派生 `PartialEq`：带着一个函数指针（resume 命令形），函数地址相等不是一个有意义的比较。
#[derive(Debug, Clone, Copy)]
pub struct LaunchFace {
    /// 适配器 id（界面起会话那一发交回，上游选择按它挑那一行）。
    pub adapter_id: &'static str,
    /// 消息流里说话的那一方叫什么（卡头 · 刻度悬停「{名}：…」）—— 短名，不是产品全称（家名在行头 `Adapter.display_name`）。
    /// 生成器那一格见 `models`。
    // 后端里的读者只有 [`speaker_said_among`]（`observe::search_query::who_of_among` 经它写「谁说的」）· 生成器（`agents_tests.rs::render_agent_profile_table`，测试档，写进 `generated/agent-profile-table.ts`，界面读那一份）。
    pub speaker_name: &'static str,
    /// 默认启动器（无候选时的命令基底）。
    pub default_launcher: &'static str,
    /// shell 集成 wrapper：探得到先用它，探不到回退默认启动器；没有 ⇒ `None`。
    pub launcher_alias: Option<&'static str>,
    /// resume 那个字面量：`--` 开头 ＝ flag 形（`claude --resume <sid>`），否则 ＝ 子命令形（`codex resume <sid>`）。
    pub resume_token: &'static str,
    /// flag 形 resume 的另一种写法（`claude -r <sid>`）：透传里认 resume 的那条会话时与 [`Self::resume_token`] 同等看；没有 ⇒ `None`。
    pub resume_short: Option<&'static str>,
    /// 起**新**会话时先定好 sid 的那个旗标（`claude --session-id <uuid>`）：起会话框选了规则 ⇒ 后端起之前按这个 sid 写好来源。
    /// 这一家不认 ⇒ `None`（那就不许起的时候带规则）。
    pub preset_sid: Option<&'static str>,
    /// 起会话带第一句时，能不能把它作为启动参数交过去（交给那一家的那一半末尾、`--` 之后的位置参数：那一家自己排着，启动画面 · 信任框过了才交进会话）。
    /// 不能 ⇒ 带第一句的起会话回 `bad_args`。
    pub first_prompt_arg: bool,
    /// 权限档：「档名 ⇒ 一组参数」那张表，起会话与会话中途同出它（起会话框 · 手机起会话带它；不给越过批准的那种档）。这一家没有 ⇒ `None`。
    pub permission: Option<PermissionFace>,
    /// 这一家把「我是哪个会话」导给它起的子进程的那个环境变量（会话血缘：`ccm` 在一个会话的 shell 里被调用时读它当父）。
    /// 这一家不导 ⇒ `None`。
    pub self_sid_env: Option<&'static str>,
    /// `ccm` 起这一家（新起与 resume）时垫在交给它的那一串最前面的参数。
    pub launch_args: &'static [&'static str],
    /// 起之前要清掉的嵌套会话标记（顺序决定载荷字节）。
    pub nested_env: &'static [&'static str],
    /// 不说是哪一家时就起这一家（`ccm` 不给 `--agent` · 载荷内核）。
    /// 注册表里恰一家为真（`agents_tests.rs` 钉着）。
    pub is_default: bool,
    /// 这一家认得的模型名与写给人看的字（起会话框 · 账号页「默认模型」的选项；不选 ＝ 跟着这一家自己的默认）。没考据过 ⇒ `None`。
    /// 声明了 [`Self::model_list`] 且读得成 ⇒ 照清单，这一张是读不成时的那一份。
    pub models: Option<&'static [Choice]>,
    /// 起会话选了模型（＋ 那个模型 `efforts` 里的一档推理，不选 ⇒ `None`）⇒ 交给那一家的那几个参数（`claude --model <模型>`）。
    /// 这一家起会话选不了模型 ⇒ `None`。
    pub model_args: Option<fn(&str, Option<&str>) -> Vec<String>>,
    /// 这台装的那一家自己列得出的模型清单（跑哪条子命令 · 输出怎么读）。没有不联网 · 不用登录就列得出的那一形 ⇒ `None`。
    pub model_list: Option<ModelList>,
    /// 回到这一家缺省模型的那一项（原值 ＋ 字）：会话开着时换模型交它；起会话不交（不选就是默认）。没有 ⇒ `None`。
    pub model_default: Option<Choice>,
    /// 记录里写的模型原值 ⇒ 写给人看的名字（会话事实 `modelText`）；认不出 ⇒ `None`。这一家没考据过 ⇒ `None`。
    pub model_text: Option<fn(&str) -> Option<String>>,
    /// 记录里写的模型原值 ⇒ 换模型清单里**唯一**对得上的那一项的原值（会话事实 `modelPick`，抽屉勾它）；对不上 · 不止一项 ⇒ `None`。没考据过 ⇒ `None`。
    pub model_pick: Option<fn(&str) -> Option<&'static str>>,
    /// 会话开着时怎么换模型；这一家换不了 ⇒ `None`。
    pub model_switch: Option<ModelSwitch>,
    /// 会话开着时怎么回退到人说过的某一句之前（编辑已发的一句）；这一家没申报 ⇒ `None`（照实回「不支持」）。
    pub rewind: Option<RewindFace>,
    /// 一次性跑一件活那一形（不开交互界面、跑完就退）：派活只经它。这一家没有 ⇒ `None`（不能派活给它）。
    pub task: Option<TaskFace>,
}

/// 一家**一次性跑一件活**的说法（[`LaunchFace::task`]）：交给它的那几个参数 · 结果文本从哪读。
/// 起 · 等 · 收住通用层（`control::launch::run_task`），这里只说拼什么、读哪儿。
#[derive(Debug, Clone, Copy)]
pub struct TaskFace {
    /// （说明, 结果文件）⇒ 接在启动器后面交给那一家的那几个参数（不开交互界面、跑完就退的那一形）。
    pub args: fn(&str, &str) -> Vec<String>,
    /// 结果文本从哪读。
    pub text: TaskText,
}

/// 一件活的结果文本从哪读（[`TaskFace::text`]）。
#[derive(Debug, Clone, Copy)]
pub enum TaskText {
    /// 那一家把结果写进交给它的结果文件（原样就是结果文本）。
    File,
    /// 那一家把结果印在标准输出：照这一家给的读法读；读不成 ⇒ `None`。
    Stdout(fn(&str) -> Option<String>),
}

impl TaskText {
    /// 结果文本：`stdout` ＝ 那个进程印的（超了上限 ⇒ `None`）；`file` ＝ 结果文件读到的（没写出 · 读不成 · 超了上限 ⇒ `None`）。
    /// 读不到 ⇒ `None`。
    pub fn read(&self, stdout: Option<&str>, file: Option<String>) -> Option<String> {
        match self {
            TaskText::File => file,
            TaskText::Stdout(read) => stdout.and_then(read),
        }
    }
}

/// 跑完一件活交回的那一形（各家同一形）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDone {
    /// 结果文本；没读到（结果文件没写出 · 输出读不成 · 超过 4 MiB）⇒ `None`，照实说没有，不拿别的顶。
    pub text: Option<String>,
    /// 退出码；被信号杀掉、没有退出码 ⇒ `None`。
    pub exit: Option<i32>,
    /// 起到退用了多少毫秒（起止现算）。
    pub took_ms: u64,
}

/// `kind` 那一家（在给定注册表里）一次性跑一件活的说法。认不出这一家 · 它没声明那一形 ⇒ `None`。
pub fn task_face_among(registry: &[Adapter], kind: &str) -> Option<TaskFace> {
    launch_face_among(registry, kind)?.task
}

/// 一家的**回退菜单**（[`LaunchFace::rewind`]）：怎么调出来 · 每一屏怎么认 · 各按哪个键 · 「怎么退」那几项在确认页上叫什么。
/// 键都是 `terminal-input` 键表里的名字。按法 · 认法只住适配层；按的先后 · 每按一键前读屏核对住通用层（`control/rewind.rs`）。
#[derive(Debug, Clone, Copy)]
pub struct RewindFace {
    /// 在输入行（空着）上调出回退列表的那几下。
    pub open: &'static [&'static str],
    /// [`Self::open`] 两下之间隔多少毫秒（tmux 的 escape-time 会把挨得太近的两下 Esc 并成一键）。
    pub open_gap_ms: u64,
    /// 列表 · 确认页里往上 · 往下移一项。
    pub up: &'static str,
    pub down: &'static str,
    /// 列表上进确认页 · 确认页上按下选中那一项。
    pub enter: &'static str,
    /// 退一层（确认页 ⇒ 列表 ⇒ 输入行）。
    pub back: &'static str,
    /// 主 agent 在干活时停下这一轮的那一下；`None` ＝ 不支持「现在停」。
    pub interrupt: Option<&'static str>,
    /// 删输入行里一个字的那一下（被打断的那一句 · 那一家填回来的原句要先删掉）；`None` ＝ 输入行有字就不退。
    pub erase: Option<&'static str>,
    /// 此刻那一屏（纯文本行）是哪一屏。
    pub read: fn(&[String]) -> RewindScreen,
    /// 「怎么退」那几项（只列这一家确认页上有的；缺省那一项恰一个）。
    pub choices: &'static [RewindChoice],
}

/// 「怎么退」的一项在这一家那里的样子。
#[derive(Debug, Clone, Copy)]
pub struct RewindChoice {
    pub how: cmd_wire::rewind::RewindHow,
    /// 确认页上那一项的字（去掉序号与选中标之后逐字比；屏上没有这一项 ⇒ 不按）。
    pub label: &'static str,
    /// 写给人看的字（发送那一下的选项）。
    pub text: fn() -> String,
    /// 缺省选它。
    pub default: bool,
}

/// 回退那一串认得出的几屏（[`RewindFace::read`] 交出；认不出 ⇒ [`RewindScreen::Unknown`]，一个键不按）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewindScreen {
    /// 输入行，主 agent 空着；`typed` ＝ 输入行里此刻的字（空着 ⇒ 空串）。
    Prompt {
        typed: String,
    },
    /// 主 agent 正在这一轮里（出字 · 调工具 · 等工具结果）。
    Busy,
    /// 回退列表：人说过的每一句一行（先说的在上），末行是「此刻」。
    List {
        rows: Vec<RewindRow>,
    },
    /// 选中一句之后的确认页：几项、哪项选中。
    Confirm {
        items: Vec<RewindItem>,
    },
    Unknown,
}

/// 回退列表上的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewindRow {
    /// 那一句在屏上的字（可能被截短）。
    pub text: String,
    pub selected: bool,
    /// 「此刻」那一行（不是人说过的一句）。
    pub current: bool,
}

/// 确认页上的一项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewindItem {
    /// 去掉序号与选中标之后的字。
    pub label: String,
    pub selected: bool,
}

impl LaunchFace {
    /// 一个模型值写给人看的字 —— **只这一处**（会话事实 · 起会话框 · 换模型的回包都问它）：
    /// 那一家列的清单（`listed`，读不成 ⇒ `None`）里有 ⇒ 清单的字；没有 ⇒ 静态那一张（[`Self::models`]）；「默认」那一项另写；都没有 ⇒ `None`。
    pub fn model_word(&self, model: &str, listed: Option<&[ListedModel]>) -> Option<String> {
        listed
            .into_iter()
            .flatten()
            .find(|m| m.value == model)
            .map(|m| m.text.clone())
            .or_else(|| {
                self.models
                    .into_iter()
                    .flatten()
                    .chain(self.model_default.as_ref())
                    .find(|c| c.value == model)
                    .map(|c| (c.text)())
            })
    }
}

/// 一家自己列模型清单的说法（[`LaunchFace::model_list`]）：照那一家的启动器跑一条子命令（不联网 · 不用登录的那一形），把它的输出读成模型。
/// 跑在通用层（`faces::session_open_face::listed_models`）；这里只说跑哪条、怎么读。
#[derive(Debug, Clone, Copy)]
pub struct ModelList {
    /// 接在启动器后面的那几个词（`codex debug models --bundled` 的 `debug models --bundled`）。
    pub args: &'static [&'static str],
    /// 标准输出 ⇒ 列出的模型（那一家排的先后）；读不成 ⇒ `None`。
    pub read: fn(&str) -> Option<Vec<ListedModel>>,
}

/// 清单里的一个模型：原值 · 写给人看的名字 · 这个模型能选的推理档（那一家排的先后）· 缺省那一档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedModel {
    /// 原值（交给那一家的就是它）。
    pub value: String,
    /// 那一家写的显示名（原样）。
    pub text: String,
    /// 能选的推理档；那一家不分档 ⇒ 空。
    pub efforts: Vec<String>,
    /// 不选时用哪一档；那一家没说 ⇒ `None`。
    pub default_effort: Option<String>,
}

impl ListedModel {
    /// 静态那一张（[`LaunchFace::models`]）里的一项：不分推理档。
    pub fn of_choice(c: &Choice) -> ListedModel {
        ListedModel {
            value: c.value.to_string(),
            text: (c.text)(),
            efforts: Vec::new(),
            default_effort: None,
        }
    }
}

/// 会话开着时换模型的说法（[`LaunchFace::model_switch`]）：往会话里送一行，再从记录里此后长出的那一条读回换成了什么（那一家写读回才读）。
#[derive(Debug, Clone, Copy)]
pub struct ModelSwitch {
    /// 送进会话的那一行：交的是可选的模型里的原值 ＋ 推理档（不选 ⇒ `None`，那一家照它自己的缺省）。
    pub command: fn(&str, Option<&str>) -> String,
    /// 从记录里的一条认出读回的那一句；不是 ⇒ `None`。那一家换模型当场不写记录、读不回 ⇒ 声明 `None`（送出那一句就回「已发出」，不白等）。
    pub read: Option<fn(&serde_json::Value) -> Option<ModelSaid>>,
    /// 换模型顺带写的那份设置（配置目录下的相对路径）：读回那一句照这条会话用的号的那一份跟不跟别的号共用说；不写 ⇒ `None`。
    pub saves: Option<&'static str>,
    /// 换模型顺带改了什么（换模型抽屉底那一行）；没有 ⇒ `None`。
    pub note: Option<fn() -> String>,
    /// 送出那一行之后那一家可能弹的确认框（换模型本来就是人点的 ⇒ 认出来就替人确认）；不弹 ⇒ `None`。认不出的框不按。
    pub confirm: Option<ScreenBox>,
}

/// 终端画面上一个认得出、该替人按掉的框（适配层声明它的样子与按哪个键）。
#[derive(Debug, Clone, Copy)]
pub struct ScreenBox {
    /// 此刻那一屏（纯文本行）上是不是这个框、而且按 [`Self::key`] 就是确认。
    pub seen: fn(&[String]) -> bool,
    /// 确认按哪个键（`terminal-input` 键表里的名字）。
    pub key: &'static str,
}

/// 换模型读回的那一句（[`ModelSwitch::read`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSaid {
    /// 那一家写的显示名（原样）。
    pub name: String,
    /// 那一家说「没变」（本来就是它）。
    pub kept: bool,
}

/// 一个可选值：交给那一家的原值 ＋ 写给人看的那一句（模型 · 权限档）。
#[derive(Debug, Clone, Copy)]
pub struct Choice {
    /// 原值（交给那一家的就是它，也是界面交回的那一格）。
    pub value: &'static str,
    /// 写给人看的那一句（经文案表取，键在适配层那一处写成字面量）。
    pub text: fn() -> String,
}

/// 一家的权限档（[`LaunchFace::permission`]）：一张「档名 ⇒ 一组参数」的表，起会话与会话中途同出这一张。
/// 不认得的档 ⇒ 不出参数、不出那一句；认得却不给选的档（[`Self::shown`]）只认不选。
#[derive(Debug, Clone, Copy)]
pub struct PermissionFace {
    /// 可选的档：原值 · 字 · 起在这一档交给那一家的那一组参数。
    pub modes: &'static [PermissionMode],
    /// 会话会处在、却不给选的档（原值 ＋ 字：越过批准的那几档 · 别名）。会话事实 · 屏底认出来的档都要有字，字从这里与 [`Self::modes`] 取。
    pub shown: &'static [Choice],
    /// 会话开着时怎么切档；这一家切不了 ⇒ `None`。
    pub switch: Option<ModeSwitch>,
}

/// 权限档表里的一行：档名 ⇒ 一组参数。
#[derive(Debug, Clone, Copy)]
pub struct PermissionMode {
    /// 原值（界面交回的那一格，也是会话事实里的那一档）。
    pub value: &'static str,
    /// 写给人看的那一句（经文案表取，键在适配层那一处写成字面量）。
    pub text: fn() -> String,
    /// 起在这一档交给那一家的那一组参数（`claude --permission-mode <档>` · `codex -a <策略> -s <沙箱>`）。
    pub args: &'static [&'static str],
}

impl PermissionMode {
    /// 原值 ＋ 字（起会话框的那一项）。
    pub fn choice(&self) -> Choice {
        Choice {
            value: self.value,
            text: self.text,
        }
    }
}

/// 会话开着时切权限档的说法（[`PermissionFace::switch`]），两形。
#[derive(Debug, Clone, Copy)]
pub enum ModeSwitch {
    /// 那一家只有「按一下换到下一档」的循环：按、读屏，到了为止。
    Cycle(ModeCycle),
    /// 往会话里投一句命令直接切到那一档：交的是可选的档的原值（只认不选的 · 不认得的通用层不问它，见 [`PermissionFace::switch_line`]）。
    Command(fn(&str) -> String),
}

/// 按键轮换那一形（[`ModeSwitch::Cycle`]）。
#[derive(Debug, Clone, Copy)]
pub struct ModeCycle {
    /// 换到下一档按哪个键（`terminal-input` 键表里的名字）。
    pub key: &'static str,
    /// 循环里最多几档（按满这么多下还没到 ⇒ 不在循环里）。
    pub cycle_max: usize,
    /// 从此刻那一屏（纯文本行）认出此刻的档（原值）；认不出 ⇒ `None`。
    pub read: fn(&[String]) -> Option<&'static str>,
}

impl PermissionFace {
    /// 可选的档里的那一行；只认不选的 · 不认得的 ⇒ `None`。
    pub fn offered(&self, mode: &str) -> Option<&'static PermissionMode> {
        self.modes.iter().find(|m| m.value == mode)
    }

    /// 起在 `mode` 这一档交给那一家的那一组参数；只认不选的 · 不认得的 ⇒ `None`（一个参数都不出）。
    pub fn args_of(&self, mode: &str) -> Option<Vec<String>> {
        self.offered(mode)
            .map(|m| m.args.iter().map(|a| a.to_string()).collect())
    }

    /// 会话开着时切到 `mode` 投的那一句（投命令那一形）；按键那一形 · 切不了 · 只认不选的 · 不认得的 ⇒ `None`。
    pub fn switch_line(&self, mode: &str) -> Option<String> {
        match self.switch? {
            ModeSwitch::Command(line) => self.offered(mode).map(|m| line(m.value)),
            ModeSwitch::Cycle(_) => None,
        }
    }

    /// 这一档写给人看的那一句（会话事实 · 切档的回包都读它）：可选的档 ＋ 只认不选的档（[`PermissionFace::shown`]）里找；都没有 ⇒ `None`（不猜）。
    pub fn text_of(&self, mode: &str) -> Option<String> {
        self.modes
            .iter()
            .map(PermissionMode::choice)
            .chain(self.shown.iter().copied())
            .find(|c| c.value == mode)
            .map(|c| (c.text)())
    }
}

/// 某一家（wire 上的 kind）在给定注册表里的起会话事实。认不出 ⇒ `None`。
/// 生产走 [`REGISTRY`]；判据喂合成注册表（含夹具家），通用层因此只看这一格、不看名字。
pub fn launch_face_among(registry: &[Adapter], kind: &str) -> Option<LaunchFace> {
    adapter_among(registry, kind).and_then(|a| a.launch)
}

/// 消息流里说话一方的短名（全文搜索与会话内查找的「谁」）：后端读 [`LaunchFace::speaker_name`] **只经这里**。
/// 这一家没有起会话面 · 注册表里认不出 ⇒ 那一家对人的叫法（[`family_said_among`]，说不出就是照实那一句），不露 kind。
pub fn speaker_said_among(registry: &[Adapter], kind: &str) -> String {
    launch_face_among(registry, kind).map_or_else(
        || family_said_among(registry, kind),
        |l| l.speaker_name.to_string(),
    )
}

/// 给定注册表里声明「不说是哪一家时就是我」（[`LaunchFace::is_default`]）的那一家：`(kind, 起会话事实)`。没有 ⇒ `None`。
pub fn default_launch_among(registry: &[Adapter]) -> Option<(&'static str, LaunchFace)> {
    registry
        .iter()
        .find_map(|a| a.launch.filter(|f| f.is_default).map(|f| (a.kind, f)))
}

/// 生产注册表里默认那一家的 kind。注册表恰有一家声明默认（判据钉着）；没有 ⇒ 空串（下游按认不出处理）。
pub fn default_kind() -> &'static str {
    default_launch_among(REGISTRY).map_or("", |(kind, _)| kind)
}

/// 收 agent 名字的入口用哪一家：**全仓只有这一种认法**，按 wire 上的 kind 认（`ccm --ccm-agent` · cc-bus 派生的 `tool` ·
/// 起会话那一发的 `agent`；要路由名的 [`pick_route`] 也先认 kind）。
///
/// - 没写 / 写空（两侧空白不算字）⇒ 注册表里声明默认（[`LaunchFace::is_default`]）的那一家；
/// - 写了注册表里由我们起的一家 ⇒ 就是它（大小写敏感）；
/// - 写了注册表里没有的名字 ⇒ 报错，那句话列出认得的那几家（[`unknown_agent_said`]，同一份注册表）。**不落默认**：拼错了就跑成另一家，比起不来更坏。
///
/// 回 `(kind, 起会话事实)`；`Err` 是给用户看的那一句。
pub fn pick_kind_among(
    registry: &[Adapter],
    name: Option<&str>,
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
        launchable().find(|(k, _)| *k == name)
    };
    hit.ok_or_else(|| unknown_agent_said(registry, name))
}

/// 「不认识这个 agent」那一句：列出 `registry` 里由我们起的那几家（[`launchable_kinds_among`]）。
/// [`pick_kind_among`] 与要求名字原样说出的入口（`ccm` 那一行的渲染）共用这一句。
pub fn unknown_agent_said(registry: &[Adapter], name: &str) -> String {
    copy_core::copy_text(
        "beAgents.pick.unknown",
        &[
            ("agent", name),
            ("known", &launchable_kinds_among(registry).join(" / ")),
        ],
    )
}

/// [`pick_kind_among`] 在生产注册表上。
pub fn pick_kind(name: Option<&str>) -> Result<(&'static str, LaunchFace), String> {
    pick_kind_among(REGISTRY, name)
}

/// 各家导给子进程的「我是哪个会话」变量（注册序、去重；[`LaunchFace::self_sid_env`]）。`ccm` 按它认父；
/// 后端自己起的子进程不该带着它们（常驻后端若是在某个会话里起的，环境里就有那个会话的编号）。
pub fn self_sid_envs() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for e in REGISTRY
        .iter()
        .filter_map(|a| a.launch.as_ref()?.self_sid_env)
    {
        if !v.contains(&e) {
            v.push(e);
        }
    }
    v
}

/// 把各家「我是哪个会话」的变量登记进起子进程原语的不往下传名单（`platform::child_env::also_internal`）。
/// 进程入口（主 crate 的 `stream/entry.rs`）起第一个子进程之前调一次：之后这个进程起的每个子进程都看不见它们。
pub fn install_child_env_filter() {
    crate::platform::child_env::also_internal(self_sid_envs());
}

/// 由我们起的那几家（带 [`LaunchFace`] 的，注册表序）—— `ccm --agent` 的闭集就是它，不另写一份。
pub fn launchable_kinds() -> Vec<&'static str> {
    launchable_kinds_among(REGISTRY)
}

/// [`launchable_kinds`] 照传进来的注册表（判据喂替身注册表时，拒的那句列的就是这一份）。
pub fn launchable_kinds_among(registry: &[Adapter]) -> Vec<&'static str> {
    registry
        .iter()
        .filter(|a| a.launch.is_some())
        .map(|a| a.kind)
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/launch_face_tests.rs"]
mod tests;
