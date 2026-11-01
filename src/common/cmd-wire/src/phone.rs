//! **手机嵌的 Rust 核心与 Kotlin 之间那一层**的线上形状（JNI：核心交上来的几样事 · 几个口的回值）—— 两头同一份：
//! 核心照这里的类型写，Kotlin 读从这里生成的类（`tests/scripts/gen-kotlin-wire.mjs`）。缺一格就是两端契约对不上，Kotlin 不补默认值。
//! 纯数据：判定 · 拼字都在核心里，这里只有形状。

use serde::{Deserialize, Serialize};

use crate::batch::Decide;
use crate::machine::MachineState;

/// `state` 那一件：那条流通没通（`connecting` · `up` 带第几条流 · `down`）＋ 那台的状态成品。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LinkLine {
    /// 正在连（还没有哪一条流）。
    Connecting { machine: MachineState },
    /// 通着：第几条流（换了一条 ⇒ 手上那几份要重问）。
    Up {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        epoch: u64,
        machine: MachineState,
    },
    /// 断着。
    Down { machine: MachineState },
}

/// `reply` 那一件：一问一答的结局（`call` ＝ 问的时候领的那个号）。成了的 `data` 是那台回包原样（Kotlin 照那条命令的回包类型解那一次；
/// 记录那一格核心先判过认不认得，`record::mend_records`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "end", rename_all = "snake_case")]
pub enum CallEnd {
    /// 成了。
    Ok {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        call: i64,
        #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
        data: serde_json::Value,
    },
    /// 那台回了失败（或事前就拒：不认 · 形状不同 · 这台做不到 · 在等的太多），码 · 那一句 · 复制详情 · 带的 `data`。
    Failed {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        call: i64,
        code: String,
        said: String,
        detail: String,
        #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
        data: Option<serde_json::Value>,
    },
    /// 到期限没回。
    Timeout {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        call: i64,
    },
    /// 这边撤了。
    Cancelled {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        call: i64,
    },
    /// 流断了。
    Down {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        call: i64,
    },
    /// 传文件那一趟的回包照类型解不开（两端版本对不上）：serde 那一句。
    Unreadable {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        call: i64,
        detail: String,
    },
}

/// `moved` 那一件：传文件那一趟走到哪（`total` 不知道 ⇒ `null`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Moved {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub call: i64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub done: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub total: Option<u64>,
}

/// 装 / 换后端那一趟做的是哪一样（[`DeployEnd::Done`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum DeployAct {
    /// 那台已是这一版，不用装。
    Skip,
    /// 装了。
    Deploy,
    /// 留着那台的（新于这一版）。
    Keep,
}

/// `deploy` 那一件：装 / 换后端那一趟的结局（那一句由部署那一份写好）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "end", rename_all = "snake_case")]
pub enum DeployEnd {
    /// 做完了。
    Done { act: DeployAct, text: String },
    /// 没做成：交出去的那一形失败（码 · 那一句 · 复制详情）。
    Failed(crate::failed::Failed),
}

/// 同名那一问的一行（抽屉照画）：键 · 名字 · 那一行的说明（核心写好）· 这一项能怎么答（带字）· 撞的那一份叫什么（本机同名才有）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct ClashRow {
    pub key: String,
    pub name: String,
    pub meta: String,
    /// 这一项能怎么答，先后由核心排好（`batch_core::choices`）。
    pub choices: Vec<ClashChoice>,
    pub there: Option<String>,
}

/// 同名那一问的一种答：答法 · 整组按钮那一句（`跳过同名`）· 逐项那一格的字（`跳过`）。字由核心写好，两端照摆。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct ClashChoice {
    pub decide: Decide,
    pub label: String,
    pub pick: String,
}

/// `batch` 那一件：一组走到哪（删 · 改权限 · 复制 · 下到本机同一形）。字全由核心写好；`kept` ＝ 落过盘、进程没了再开时接回来的那一组。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum BatchLine {
    /// 在做：手上那一项 · 几成。
    Running {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        id: i64,
        title: String,
        kept: bool,
        current: Option<String>,
        /// 几成（一项都还没数 ⇒ `null`）。
        frac: Option<f32>,
    },
    /// 同名那一问：那几行 · 逐项选完那一颗的字（那个动作）。
    Asking {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        id: i64,
        title: String,
        kept: bool,
        conflicts: Vec<ClashRow>,
        go: String,
    },
    /// 断开：那一行（连回来核心同一张票接着做）。
    Cut {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        id: i64,
        title: String,
        kept: bool,
        line: String,
    },
    /// 收场：结局句 · 没成的那几行（看它们那一屏的标题）· 复制详情 · 给不给［重试］［撤销］。
    Ended {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        id: i64,
        title: String,
        kept: bool,
        said: String,
        failed_title: String,
        failed: Vec<String>,
        /// 复制详情（没什么可说 ⇒ `null`）。
        details: Option<String>,
        retry: bool,
        undo: bool,
    },
    /// 整组没做成：那一句 · 复制详情。
    Failed {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        id: i64,
        title: String,
        kept: bool,
        said: String,
        details: Option<String>,
    },
    /// 取消了。
    Dropped {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        id: i64,
        title: String,
        kept: bool,
    },
}

/// `batch_keep` 那一件：那一组的落盘串（原样存、原样交回再开那一口）；收场 ⇒ `plan: null`（删）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct BatchKeep {
    pub ticket: String,
    pub plan: Option<String>,
}

/// `batch_local` 那一件：下到本机那一组要 Kotlin 在本机那一侧做的（`id` ＝ 那一组的编号，`key` ＝ 那一项的键）。
/// `folder` 是起这一组时 Kotlin 交的那一棵（原样一串，核心不认里面的格）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum LocalAsk {
    /// 开一份写进去：`over` ＝ 盖那棵里叫这个名的那一份（同名答了覆盖）；`null` ⇒ 新建（同名时系统另起名）。
    /// 答经 `batchAnswer`（[`LocalOpened`]）。
    Open {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        id: i64,
        key: String,
        folder: String,
        name: String,
        over: Option<String>,
    },
    /// 那一项没下成（失败 · 撤了 · 断了）：删掉开给它的那一份。
    Drop {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        id: i64,
        key: String,
    },
}

/// [`LocalAsk::Open`] 的答：开好的描述符（所有权交给核心）· 开不了（系统原话；那一句核心写）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "opened", rename_all = "snake_case")]
pub enum LocalOpened {
    Fd {
        key: String,
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        fd: i64,
    },
    Refused {
        key: String,
        detail: String,
    },
}

/// 现生成的一把钥匙：私钥 PEM（openssh-key-v1，无口令）· 公钥那一行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct KeyPair {
    pub pem: String,
    pub public: String,
}

/// 手机导入的那一段 `~/.ssh/config` 里一个具体主机（通配那几块滤掉；解析住拨号核心）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct ConfigHost {
    /// `Host` 别名（作标签）。
    pub alias: String,
    /// `HostName`（缺 ⇒ 别名）。
    pub host_name: String,
    pub port: u16,
    pub user: Option<String>,
    /// `IdentityFile` 原值（手机读不到那个文件，只按文件名去配已导入的钥匙）。
    pub identity_file: Option<String>,
}

/// 手机文件页选中这几项时交回的：顶栏一组 · ⋯ 一组 · 单选时点一下那一行做什么（`open` · `preview` · `download`；什么都不做 ⇒ `null`）。
/// 判法住 `pick-core`（与桌面文件窗口同一张表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct PhoneMenu {
    pub bar: Vec<MenuEntry>,
    pub more: Vec<MenuEntry>,
    pub tap: Option<&'static str>,
}

/// 手机那一项：交回的词 · 字 · 能不能点 · 灰着的为什么（手机没有悬停，点了在底部说这一句）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct MenuEntry {
    pub id: &'static str,
    pub text: String,
    pub enabled: bool,
    pub why: Option<String>,
}

/// 同名那一问的答（Kotlin 交进来；`null` ＝ 取消）：整组一个答 · 逐项（键 ⇒ 答，没点的那几项照跳过，`batch_core::answer_of` 定）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum BatchAnswer {
    All(Decide),
    Each(std::collections::BTreeMap<String, Decide>),
}

/// `Core.dialPlan` 的结局：照手机那张机器表、按名字组拨号（跳板一路照表找，多跳；组法是拨号核心那一份）。
/// 成了 ⇒ 组好的那一份请求（每一跳的 `key_path` 是那一行交进来的原值：手机写那台的编号，Kotlin 照它解认证、换成钥匙编号）；
/// 拒了（成环 · 跳板找不到 · 那一台不成立 · 表读不懂）⇒ 那一句。不回落直连。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DialPlan {
    Ready { request: crate::dial::DialRequest },
    Refused { said: String },
}

/// `Core.machineCheck` 的回值：手机编辑页上那张表（连正在编的那一台）判得怎样 ＋ 那一台展开的拨号地址（预览）。
/// 毛病的判法 · 说法与桌面 `machines-try` 同一处（`ssh_dial::machine::table_fault`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct MachineCheck {
    pub fault: Option<crate::machine::MachineFault>,
    pub endpoints: Vec<crate::dial::Endpoint>,
}
