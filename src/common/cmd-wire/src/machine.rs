//! `Machine` —— **「这一趟问的是哪台机器」的唯一类型**。
//!
//! # 它为什么存在（「origin 归一 —— **这是地基**」）
//!
//! 在它之前，同一个概念在盘上有**五种表达**（现打）：
//!
//! | 侧 | 现打 |
//! |---|---|
//! | Rust | `origin: &str`（46 处）· `origin: String`（33）· `origin: Option<String>`（8） |
//! | TS | `origin: string` · `origin: string \| null` |
//!
//! 而「本机」那个哨兵值是**两份手抄的字面量** —— Rust 一份、TS 一份，
//! 靠一条跨语言对拍判据钉着（`inbound_client_tests.rs`）。
//! 那条判据是对的，但它买的是「两个字面量相等」，**不是**「只有一个类型」。
//!
//! # 🔴 **线上只剩两个值** —— `null` 退役了
//!
//! 原先逐字写着三个线上值 `null | "<local>" | host`。
//! 步 2 逐字「**`origin` 去 `null` 化 —— 本机也带 origin**」⇒ 线上从此**只有字符串**：
//!
//! | 线上值 | 意思 |
//! |---|---|
//! | `"<local>"` | 本机那台。**它是一个具名的 origin**，不是「缺省」 |
//! | `"devbox"`（任意机器名）| 那台远端（机器表里那一台的名字） |
//! | ~~`null`~~ | **拒收** —— 见下一节 |
//!
//! ⚠ `INVARIANTS §40` 逐字「本地 ＝ 不走 ssh 的远端」⇒ 本机本来就该有名字。
//! 「省掉 origin」与「本机」长得一样，正是这一层要治的东西。
//!
//! # 🔴 `Unspecified(())` 那个变体：**退役**（处置与理由，逐条写明）
//!
//! 上一拍这个类型有两个变体，`Unspecified(())` 承载线上的 `null`（「调用方没说」）。
//! 已**把它整个删掉**，理由三条：
//!
//! 1. **要求的就是结构上的「不容忍」** —— 那一节的警告逐字：
//!    「⚠ **2 在 5 之前**（否则 `call` 的第一个参数还得容忍 `null`）」。
//!    留着变体就是**留着容忍**：`call(origin: Origin, …)` 在类型上仍然装得下「没说」，
//!    于是「不许把它当本机」这条纪律要在**每一个**新消费者身上重新打一遍
//!    （今天唯一被机器守着的漏斗是 [`Machine::route`]，而它只盖得住
//!    `#[tauri::command]` 那一族 —— `call` 不会是其中之一）。
//! 2. **「留着当拒收的类型位」在这一层是个空位。** 变体只能从线上来；
//!    把它排除出线上形状（`serde(skip)`）之后它永远构造不出来 ⇒ 纯死重。
//!    而不排除出线上形状，`Machine.ts` 就还是 `null | string`，
//!    `§8` 承诺的「**`tsc` 就能验**」当场落空。两条路只能选一条。
//! 3. 本仓纪律逐字「**不为旧配置留兼容** —— 旧形直接退役」。`null` 是旧形。
//!
//! ## ⚠ 而它守的那条性质**一个字都没丢** —— 岗位挪到了两处
//!
//! 它守的是「**「没说」不许被悄悄当成本机**」。退役之后这条性质由两道闸接着守：
//!
//! | 闸 | 拦的是 | 在哪 |
//! |---|---|---|
//! | ① **反序列化边界** | 线上 `null`（以及数字 / 数组 / 对象 / 布尔）| 本文件手写的 `Deserialize`：它**构造不出**一个「没说」的 `Machine` ⇒ 命令的代码**结构上**见不到那一档 |
//! | ② [`Machine::route`] | **空白名**（`""` / 全空白）—— `null` 消失之后，线上唯一还能表达「没说」的值 | 拒成 [`Blank`]（带是哪条命令）；给人看的那一句由连接层 `comms_inward::chan::said::blank_origin` 写，点名命令、并**逐字**给出本机该送什么 |
//!
//! 🔴 ② 不是凑数的：`subagent.rs` 的 `Backend::for_origin` 上一拍**逐字**把
//! 「`origin` 缺省 / **空串** = 本机」写在一起 —— 也就是说空串在盘上**真的**被当过本机。
//! 删掉 `null` 而不管空串，等于把同一个洞从一个值搬到另一个值。
//!
//! ⚠ ① 那道闸的**话说得清不清楚**是靠 `expecting()` 买的，不是白送的：
//! serde 的 stock 报错是「invalid type: null, expected a string」，它不会告诉调用方
//! 「本机该送 `"<local>"`」。那句期望串是本文件自己写的技术字面量，由 `origin_tests.rs` 的 `A` 组钉着。
//!
//! # ⚠ 射程（写死，别读宽）
//!
//! · 它买的是「**这个概念只有一个类型**」。**买不到**「所有调用点都用上了它」——
//!   替换 93 处签名是分批的活，进度由
//!   `tests/comms/inward/origin_tests.rs::ORIGIN_MIGRATION_CEILING`
//!   那张**递减棘轮**现打（数还在用裸 `&str`/`String` 的处数，只许变少）。
//! · 它**不判**某个 origin 今天连不连得上。那是 `inbound_client` 的事。
//! · 🔴 它**只管入方向**（前端 → 命令）。**出方向那一半今天还有 `null`** ——
//!   `ui_contract::JsonlLinePayload.origin` 一族仍是 `Option<String>`（`None` = 本机），
//!   而那几份这里没改。**逐份住址与为什么没动，写在交回件里，别读成「已经没有 `null` 了」。**
//!
//! # 线上形状：**本机与远端那两个值与今天逐字节相同**
//!
//! `serde(transparent)` ⇒ `"<local>"` / `"devbox"` 两种线上值与今天**一个字节不差**；
//! 变的只有 `null` 那一档（从「解出一个变体」变成「**报错**」）。
//! 两侧都有金标准钉着（`the_two_surviving_wire_values_are_byte_identical`
//! ＋ `an_unknown_shape_is_refused_not_guessed`）。

use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::cells::Tone;

/// 本机那台机器的哨兵值。
///
/// 这个字面量的家。界面经生成物 `src/frontend/ui/generated/judgment-rules.ts` 的 `LOCAL` 拿它
/// （生成器 `payload_judgment_rules.rs`，`ipc/origin.ts` 转出）。
/// 还有两份待删的抄写：`inbound_client::LOCAL_ORIGIN`（Rust）与 `src/frontend/ui/backend-policy.ts`（TS）；
/// 删之前由 `the_sentinel_agrees_with_the_two_existing_homes` 钉着与本常量逐字节相同。
pub const LOCAL: &str = "<local>";

/// 机器键里不能当名字给人看的那几个（哨兵）。别的键就是那台在机器表里的名字。
/// 读者：界面 `chan-caller.ts::machineName`——那台的成品还没到时，键在这里 ⇒ 说机器不明，不在 ⇒ 键本身
/// （经 `npm run gen:types` 生成进 `src/frontend/ui/generated/judgment-rules.ts`，生成器 `payload_judgment_rules.rs`）。
pub const KEYS_NOT_NAMES: &[&str] = &[LOCAL];

/// 「这一趟问的是哪台机器」。线上就是**一个字符串**（本机逐字 `"<local>"`）。
///
/// ⚠ 本机与远端**同一个线上形状**（都是字符串）⇒ `serde` 分不开它们，
/// 只能在**读进来之后**用 [`Machine::is_local`] 分。这是刻意的：
/// 那两个值的线上形状必须与今天逐字节相同，不许为了类型好看而改协议。
///
/// 字段私有、不借出 `&str`：按机器记的表以 `Machine` 当键（`Hash` · `Ord`），比本机远端用
/// [`Machine::is_local`] · [`Machine::host_name`]，要远端那台的名字只有 `host_name` 一个口；
/// 日志与拼路径 · URL 要线上那串经 `Display`。造一台走 [`Machine::named`]（不判名字，
/// 「名字不许是空白」由 [`Machine::route`] 那个漏斗执行 —— 装在构造器上会拆成两处、会漂）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
// TS 那一侧照本名导出（`Machine.ts`）。
//   别的 crate 的测试档里有生成类型嵌着它 ⇒ 那边经 `ts` feature 拿到同一个派生，也会把它当依赖导出一份；
//   落点因此写成本包根起算的绝对路径（相对的会跟着那边的目录走，曾经落到仓根 `frontend/`；判据 `ts_export_guard`）。
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(transparent)]
pub struct Machine(String);

/// 手写而不是 `derive` —— 为的就是 `expecting()` 那句话。
///
/// 🔴 线上 `null` 在这里**变成一次反序列化失败**，而失败那句话必须自带出路：
/// 「本机逐字送 `"<local>"`」。`derive` 出来的版本只会说
/// 「invalid type: null, expected a string」——那句话是对的，但它把调用方留在原地。
/// 期望串是 serde 的技术字面量（同 `files.rs` 的 `WirePath`），进 `bad_args` 的细节，不是给人看的成品句。
///
/// ⚠ 它**不判**名字合不合法（空白名走 [`Machine::route`] 那道闸）：
/// 两道闸各只有一个住址，合在一处的话「构造时拒」与「路由时拒」会各说各的。
impl<'de> Deserialize<'de> for Machine {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct MachineVisitor;
        impl Visitor<'_> for MachineVisitor {
            type Value = Machine;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a machine name; this machine is \"{LOCAL}\"")
            }

            fn visit_str<E: DeError>(self, s: &str) -> Result<Machine, E> {
                Ok(Machine(s.to_string()))
            }
        }
        d.deserialize_str(MachineVisitor)
    }
}

impl Machine {
    /// 名字叫 `name` 的那一台（`"<local>"` 即本机）。不判名字（空白名由 [`Machine::route`] 拒）。
    pub fn named(name: impl Into<String>) -> Self {
        Machine(name.into())
    }

    /// 本机那一个。
    pub fn local() -> Self {
        Machine(LOCAL.to_string())
    }

    /// 这一趟问的是不是本机。
    pub fn is_local(&self) -> bool {
        self.0 == LOCAL
    }

    /// 这一趟问的是不是某台远端。**空白名 ⇒ `false`**（「没给名字」不是远端，也不是本机）。
    pub fn is_remote(&self) -> bool {
        !self.0.trim().is_empty() && self.0 != LOCAL
    }

    /// 远端那台的机器名。本机与空白名都回 `None`。
    ///
    /// ⚠ 名字是 `host_name` 不是 `name`：本机也有名字（`"<local>"`），
    /// 而这个方法**只答远端**。叫 `name` 会让调用方以为本机也能取到。
    pub fn host_name(&self) -> Option<&str> {
        self.is_remote().then_some(self.0.as_str())
    }

    /// **合并后的命令唯一的入口分派**（步 12·C）。
    ///
    /// # 为什么它是一个方法，而不是每条命令自己写一个 `if origin.is_local()`
    ///
    /// 合并 N 对命令就会有 N 处「先分本机」。那 N 处里只要有一处把「没给名字」
    /// 顺手归进本机，`INVARIANTS §40`（「本地 ＝ 不走 ssh 的远端」）在那一条命令上
    /// 就悄悄破了 —— 而**那种破法不报错**：调用方送一个空串，命令去动了本机的文件。
    /// ⇒ 两态在这里一次分完，调用方拿到的是一个**只有两个可能**的值，
    /// 「没给名字」那一支在类型上就到不了它手里。
    ///
    /// # ⚠ 这道闸拦的**换了**：从 `null` 换成**空白名**
    ///
    /// ⚠ 它**不判**那台远端今天连不连得上（那是 `inbound_client` 的事），
    /// 也**不判**本机那条路该不该做这件事（那是各命令自己的守卫）。
    /// ⚠ 它**不 trim**：非空白的名字**原样**交出去（`" devbox "` 仍然是 `" devbox "`）——
    /// trim 会让今天找不到配置的 label 突然找得到，那是行为变更，不归本步。
    pub fn route(&self, command: &str) -> Result<Route<'_>, Blank> {
        if self.0.trim().is_empty() {
            return Err(Blank {
                command: command.to_string(),
            });
        }
        if self.0 == LOCAL {
            Ok(Route::Local)
        } else {
            Ok(Route::Remote(&self.0))
        }
    }
}

/// 线上那一串（本机 `"<local>"`）：给日志与拼路径 · URL 用。按机器分什么不走它（用 `is_local` · `host_name`）。
impl std::fmt::Display for Machine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// [`Machine::route`] 的产出：**分过本机之后，只剩两种可能**。
///
/// 🔴 刻意没有第三个变体。「没给名字」在 [`Machine::route`] 那一步就变成了 `Err`，
/// 线上的 `null` 更是连一个 `Machine` 都构造不出来，
/// 所以拿到 `Route` 的代码**没有办法**把「没说」误当本机或误当某台远端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route<'a> {
    /// 本机那一条路（不走 ssh）。
    Local,
    /// 某台远端，带着它的机器名（机器表里那一台的名字）。
    Remote(&'a str),
}

/// [`Machine::route`] 拒的那一档：名字空白（「没给名字」）。只带是哪条命令；给人看的那一句由连接层写
/// （`comms_inward::chan::said::blank_origin`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blank {
    pub command: String,
}

// 判据 `tests/comms/inward/origin_tests.rs` 挂在 monitor 的 `lib.rs`（它的棘轮扫的是 monitor 的命令面）；那边经 `comms_inward::origin` 的再导出用它。

/// 拨一台没拨成的原因（闭集）：拨号核心判、写进拨号应答的 `reason`；接这台的宿主随断开那一句带着它，后端按它定那一态与修法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "DialWhy", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// 地址解析不出来（DNS）。
    Resolve,
    /// 那个口拨不通（拒绝 / 主机或网络不可达）。
    Unreachable,
    /// 拨号 / 握手超时。
    Timeout,
    /// 主机指纹与记下的不一样。
    HostKey,
    /// 密钥被拒（公钥 · agent 都没过）。
    Auth,
    /// 那台只收密码（服务端剩下的方法里没有公钥）。
    Password,
    /// 私钥文件读不出来。
    KeyUnreadable,
    /// 跳板那一台断开（拨号 · 鉴权 · 经它开隧道任一步）。
    Jump,
    /// 登录过了、开会话被那台回拒（exec · shell · 子系统回 CHANNEL_FAILURE：共用登录机进程数到上限那一形）。
    SessionDenied,
    /// 别的（看那一句）。
    Other,
}

/// 那台此刻是什么样（闭集）：`up` 连着 · `connecting` 正在连（`stage` 说到哪一步）· `installing` 正在把 cc-monitor 装上去 ·
/// `updating` 正在换成这一版 · `down` 这一轮没连上（`reason` 说为什么）· `host_key_changed` 主机指纹与记下的不一样 ·
/// `needs_update` 连着但那台旧（或帧形状对不上）· `newer` 连着但那台比这一版新 · `disabled` 「连接这台」关着 ·
/// `unsupported` 那台做不了 · `unknown` 还没有任何一轮的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum MachineStateKind {
    Up,
    Connecting,
    Installing,
    Updating,
    Down,
    HostKeyChanged,
    NeedsUpdate,
    Newer,
    Disabled,
    Unsupported,
    Unknown,
}

/// 正在连时到哪一步：部署预检（核对版本）· 接那台的常驻后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "MachineStage", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Deploy,
    Attach,
}

/// 那一行上给的修法（闭集）。`install` 只给自己不装后端的宿主（手机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum MachineFix {
    Retry,
    ConnSettings,
    PushKey,
    CompareFingerprint,
    Update,
    Connect,
    Install,
}

/// 一颗修法按钮：做什么 ＋ 按钮上的字。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "MachineFixButton", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FixButton {
    pub fix: MachineFix,
    pub label: String,
}

/// 机器条上那一台那一行怎么画（进条才有）：语气 · 带不带进度条。字照成品的 `said` · `fixes`。
/// 语气用共享的 [`Tone`]：正在连 · 装 · 更新 ⇒ `Busy`；没连上 ⇒ `Offline`；那台旧了 ⇒ `Warn`；指纹不对 · 做不了 · 缺后端 ⇒ `Fail`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct MachineStripLine {
    pub tone: Tone,
    pub progress: bool,
}

/// 一台的状态成品（判定在后端 `deploy::machine`，本机远端同一份）。界面照画：字取 `word` · `said` · `fixes[].label`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MachineState {
    /// 给人看的那台的名字（问题行那一句里说的就是它）。
    pub name: String,
    pub state: MachineStateKind,
    /// 算不算离线（页头「N 台离线」数的就是这一格）：没连上 · 指纹不对 · 做不了 ⇒ 是；登录过了、只是拒开会话 ⇒ 不是。
    pub offline: bool,
    /// 没连上的原因码（`down` · `host_key_changed` 才有）。
    pub reason: Option<Why>,
    /// 正在连时到哪一步。
    pub stage: Option<Stage>,
    /// 那台 cc-monitor 的产品版本号：与这一版同一份字节时就是这一版的号；别的报不出 ⇒ 空。
    pub version: Option<String>,
    /// 与这一版的版本关系（握手过才有）。
    pub version_relation: Option<crate::attach::Relation>,
    /// 那台的系统（部署问到过才有）。
    pub os: Option<String>,
    /// 名字旁那个词（连着时空）。
    pub word: String,
    /// 问题行那一句（空 ⇒ 不出问题行）。
    pub said: String,
    /// 那台离线那一句（恒有，连接层写的；哪一态都带着）。出口要说那台离线时照摆这一句，不各自取字。
    pub offline_said: String,
    pub fixes: Vec<FixButton>,
    /// 指纹不对时那台这一次出示的主机指纹（`host_key_changed` 才有）。
    pub seen_host_key: Option<String>,
    /// 指纹不对时库里记下的那一枚（`host_key_changed` 才有；库里没有同类型的 ⇒ 空串）。拨号核心说的，不在这里算。
    pub expected_host_key: Option<String>,
    /// 复制详情：这一轮没连上 / 做不了时那一份（界面照原样跟在问题行那句后面）；连着时末尾接每条连接那一段（卡头 ⋯ 里复制）。
    pub detail: Option<String>,
    /// 到这台怎么连过去的那一句（「经跳板 devbox · 经代理」；多跳外到内「经跳板 a → b」）：跳板照配置，经代理连上了才知道。
    /// 直连又不经代理 ⇒ 省略。界面有就摆一行，不拼字；每条连接的读数不上卡头，在 `detail` 末尾「连接：」那一段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub route: Option<String>,
    /// 机器条上那一行（缺 ⇒ 不进条：连着的 · 比这一版新的 · 停用的 · 还不知道的）。桌面与手机的条都只照这一格画。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub strip: Option<MachineStripLine>,
}

/// **首屏那一句**（一条会话都没有时正文区中央那一块）：拨号那一端照各台的成品与报完了没有合一次写好（`deploy::machine::empty`），
/// 随机器状态一起推；桌面主窗口与手机会话列表的空态照摆，不数台。各台都报完了才说「无会话」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct EmptyFace {
    /// 大字那一句（「无会话」· 「本机无会话」· 一台都没报完那一句）。
    pub line: String,
    /// 下面那一行（怎么让它有 · 哪几台还在连 · 哪几台未计入）。
    pub hint: String,
}

/// 接常驻那一段的一轮结论（宿主把通信层交出的那一态原样转给后端，只有事实）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "MachineRound", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Round {
    Connecting,
    /// 通着：那台相对手上这一版 · 握手那一刻比出形状对不上的那几种帧。
    Up {
        relation: crate::attach::Relation,
        differs: Vec<String>,
    },
    /// 断着：那一句 ＋ 详情 ＋ 原因码与那台出示的指纹；`blocker` ⇒ 断在那台本身（不是路上），是哪一种。
    Down {
        problem: RoundProblem,
        blocker: Option<Blocker>,
    },
}

/// 一轮断在那台本身的哪一处（闭集）：不是 Unix（怎样都不会好）· 没有我们的后端（没装 · 起不了常驻 · 不是我们的）· 后端旧了（不认这一版的子命令）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "MachineBlocker", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum Blocker {
    NotUnix,
    NoBackend,
    OldBackend,
}

/// [`Round::Down`] 那一句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, rename = "MachineRoundProblem", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct RoundProblem {
    pub said: String,
    pub detail: String,
    pub why: Option<Why>,
    pub seen_key: Option<String>,
    /// 因主机指纹被拒时库里记下的那一枚（库里没有同类型的 ⇒ 空串；别的原因 ⇒ `None`）。
    pub expected_key: Option<String>,
}

// ───────────────────────────── 机器表那一道 ─────────────────────────────

// 机器表那一道没过的是哪一种（码与界面那一侧同一份；生成的 TS 不带说明）。判法在拨号核心（`ssh_dial::machine`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum MachineFaultCode {
    NoHost,
    NoUser,
    Port,
    NameTaken,
    LeadingDash,
}

/// 机器表那一道没过的那一处（闭集码 ＋ 那台的名字 ＋ 那一格下面的那一句）；界面照 `code` 挑落在哪一格、照抄 `said`。
/// 只有字段：句子由拨号核心写（`ssh_dial::machine::fault` · `refused_said`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase")]
pub struct MachineFault {
    /// `no_host` 地址空 · `no_user` 用户空 · `port` 端口不在 1–65535 · `name_taken` 与另一台同名 ·
    /// `leading_dash` 地址 / 用户以 `-` 打头（开终端时会被 ssh 当成选项）。
    pub code: MachineFaultCode,
    /// 出问题的那台的名字（`label` 非空取它、否则 `host`）。
    pub name: String,
    /// 那一格下面的那一句（添加机器框 · 机器卡同读）。
    pub said: String,
}

/// `machine-read` 的入参：不收参数（缺 · `null` · `{}` 都认，多一个键 ⇒ `bad_args`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct MachineReadReq {}

/// `machine-read` 的回包：机器表里每台 → 成品；连同此刻的首屏那一句（主题 `machine` 的重问）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct MachineRead {
    pub machines: std::collections::BTreeMap<String, MachineState>,
    pub empty: EmptyFace,
}

/// `machine-wake` 的入参：那台的名字（空白名由后端拒）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct MachineWakeReq {
    pub machine: String,
}

/// `machine-stop` 的入参：那台的名字（空白名由后端拒）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct MachineStopReq {
    pub machine: String,
}

/// `machine-stop` 的回包：那台常驻怎么停的（`--resident-stop` 那一行原样的词：`graceful` · `killed` · `not_running`）· 被停的 pid。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct MachineStopped {
    pub stopped: String,
    pub pid: Option<u32>,
}
