//! ——「`deploy-core` 拆开：契约（键 · 戳格式 · 答话形状 · 路径常量）成 `deploy-contract`（契约类，monitor 可链）；判定部分进后端」·
//! 「共享 crate 只放两边必须对上的契约，不放判定；判定只在后端」。
//!
//! # 本 crate 是部署那一族的**契约**（两侧对上的形状），不含一条判定
//!
//! | 契约 | 规矩出处 | 这里的口 |
//! |---|---|---|
//! | 表 A 的键与行（(OS, arch) · 有产线的格） | | [`Key`] · [`LINES`] · [`key_of`] · [`key_from_uname`] |
//! | 拒绝的形状与对人说的话 | | [`Refusal`] · [`Refusal::say`] |
//! | 身份戳的格式（读它字节里那段，不跑它） | | [`STAMP_OPEN`] · [`STAMP_CLOSE`] · [`Marks`] · [`RemoteIdentity`] · [`identity_of_bytes`] · [`stamp_scan_cmd`] · [`interpret_stamp_scan`] · [`build_order`] |
//! | 流模式能力 token（hello 的 `capabilities`） | | [`STREAM_CAPABILITIES`] |
//! | 计划答话的形状 | `IPC-PROTOCOL.md` 的 `deploy-plan` | [`DeployAction`] |
//! | 从前那份三行入口两形的记号 | | [`SHIM_MARK`] · [`LAUNCHER_MARK`] |
//!
//! 判定（那台要哪一格 · 表 B 承诺 · 换不换 · 落点那一份认不认）住后端 `control/deploy_plan.rs` 一家；
//! monitor 自举那一刻（本机后端还没起）问的是手上那份字节自己（帧命令 `place-verdict`，CLI 面自动派生）。
//!
//! # 不在本 crate 的
//!
//! - 字节本身（`include_bytes!` 那几槽）与「这一版带没带那一格」—— monitor `byte_table.rs`（放字节的一侧才知道）；
//! - `BUILD_ID` —— 留在后端（进了这里就又成了 monitor 读得到的源码常量；monitor 的「我这一版」只认手上那份字节自报的）；
//! - 任何 IO（问那台、读盘、写）。

use copy_core::copy_text;

// ═══ 表 A：键与行（表 B 的承诺是判定，住后端 `control/deploy_plan.rs::promised`）═══════════════

/// 表 A 的 OS 轴。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Os {
    Linux,
    Windows,
    Mac,
}

/// 表 A 的 arch 轴。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Arch {
    X86_64,
    Aarch64,
}

/// 表 A 的键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub os: Os,
    pub arch: Arch,
}

/// 表 B 的 origin 轴：目标机器是不是自己。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Local,
    Remote,
}

/// 为什么不给字节 —— **拒绝点在写第一个字节之前**。五形互不合并：下一步各不相同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// 答得出是什么机器，但表 A 里那一格没有产线（或根本不在 6 行里）。
    UnsupportedMachine { os: String, arch: String },
    /// 问不出 OS。
    OsUnknown { why: String },
    /// 问不出 arch。
    ArchUnknown { why: String },
    /// 那一格有产线，但这个 origin 今天不承诺那种机器（表 B；例：远端 Windows · 本机 (Linux, aarch64)）。
    /// 带着 `route`：同一形对「推到远端」与「本机自己」要说两句话（远端那句「只在本机用得上」对本机是假话，`D7`）。
    NotPromisedHere {
        os: String,
        arch: String,
        route: Route,
    },
    /// 那一格有产线、也承诺，但**这一版产物没带**那份字节（开发构建 / 没铺字节）。
    /// 前四个 key 里没有它 —— 并进前四个就把「换一版产物」说成了「不支持这台机器」（`D7`）。
    /// 只有放字节的一侧（monitor）判得出这一形；本 crate 只提供它的名字与那句话。
    NotCarried { os: String, arch: String },
}

impl Refusal {
    /// 对用户说的那一句（`machine` = 机器名，本机说「本机」）。**Rust 侧唯一的出口**；
    /// 每一形的 key 都是字面量（`copy-table.vitest.ts` 按调用形状读它们，与文案表两向相等）。
    pub fn say(&self, machine: &str) -> String {
        match self {
            Refusal::UnsupportedMachine { os, arch } => copy_text(
                "deploy.refused.unsupportedMachine",
                &[("machine", machine), ("os", os), ("arch", arch)],
            ),
            Refusal::OsUnknown { why } => copy_text(
                "deploy.refused.osUnknown",
                &[("machine", machine), ("why", why)],
            ),
            Refusal::ArchUnknown { why } => copy_text(
                "deploy.refused.archUnknown",
                &[("machine", machine), ("why", why)],
            ),
            Refusal::NotPromisedHere {
                os,
                route: Route::Remote,
                ..
            } => copy_text(
                "deploy.refused.notPromisedHere",
                &[("machine", machine), ("os", os)],
            ),
            Refusal::NotPromisedHere {
                os,
                arch,
                route: Route::Local,
            } => copy_text(
                "deploy.refused.notPromisedLocal",
                &[("machine", machine), ("os", os), ("arch", arch)],
            ),
            Refusal::NotCarried { os, arch } => copy_text(
                "deploy.refused.notCarried",
                &[("machine", machine), ("os", os), ("arch", arch)],
            ),
        }
    }
}

impl Os {
    /// 说给人听的那个词（也是 [`key_of`] 认得回来的那个词）。
    pub fn label(self) -> &'static str {
        match self {
            Os::Linux => "Linux",
            Os::Windows => "Windows",
            Os::Mac => "macOS",
        }
    }
}

impl Arch {
    /// 说给人听的那个词（也是 [`key_of`] 认得回来的那个词）。
    pub fn label(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "arm64",
        }
    }
}

/// 那台机器答的 OS 名 → 表 A 的 OS 轴。认两种来源：远端 `uname -s` 的回话；本机 `std::env::consts::OS`。
fn os_of(s: &str) -> Option<Os> {
    let upper = s.to_ascii_uppercase();
    match upper.as_str() {
        "LINUX" => Some(Os::Linux),
        "DARWIN" | "MACOS" => Some(Os::Mac),
        "WINDOWS" | "WINDOWS_NT" => Some(Os::Windows),
        // Windows 上的 POSIX 层（Git Bash / MSYS2 / Cygwin）的 `uname -s` 形如 `MINGW64_NT-10.0-19045`。
        _ if ["MINGW", "MSYS", "CYGWIN"]
            .iter()
            .any(|p| upper.starts_with(p)) =>
        {
            Some(Os::Windows)
        }
        _ => None,
    }
}

/// 那台机器答的 arch 名 → 表 A 的 arch 轴（`uname -m` 与 `consts::ARCH` 的常见写法）。
fn arch_of(s: &str) -> Option<Arch> {
    match s {
        "x86_64" | "amd64" | "AMD64" => Some(Arch::X86_64),
        "aarch64" | "arm64" | "ARM64" => Some(Arch::Aarch64),
        _ => None,
    }
}

/// 两个答话 → 表 A 的键。**纯函数**。
///
/// 空 ⇒ 问不出（`os_unknown` / `arch_unknown`）；答得出但不是 3 × 2 里的值 ⇒ `unsupported_machine`
/// （原样带出它答的那两个词）。**问不出 OS 不许当成 Linux**。
pub fn key_of(os: &str, arch: &str) -> Result<Key, Refusal> {
    let (os, arch) = (os.trim(), arch.trim());
    if os.is_empty() {
        return Err(Refusal::OsUnknown {
            why: copy_text("rsByteTable.key.noAnswer", &[]),
        });
    }
    if arch.is_empty() {
        return Err(Refusal::ArchUnknown {
            why: copy_text("rsByteTable.key.noAnswer", &[]),
        });
    }
    match (os_of(os), arch_of(arch)) {
        (Some(os), Some(arch)) => Ok(Key { os, arch }),
        (o, a) => Err(Refusal::UnsupportedMachine {
            os: o.map_or_else(|| os.to_string(), |o| o.label().to_string()),
            arch: a.map_or_else(|| arch.to_string(), |a| a.label().to_string()),
        }),
    }
}

impl Key {
    /// 说给人听的那一格（「Linux / x86_64」）。
    pub fn label(self) -> String {
        format!("{} / {}", self.os.label(), self.arch.label())
    }

    /// 本机：目标机器恰好是自己（`consts` 在编译期就是这一份产物的 `TARGET`）。
    pub fn this_machine() -> Result<Key, Refusal> {
        key_of(std::env::consts::OS, std::env::consts::ARCH)
    }
}

/// 表 A 里「有产线」的格子（`release.yml` 真编得出字节的那几格；判据对着 `release.yml` 读）。
pub const LINES: &[Key] = &[
    Key {
        os: Os::Windows,
        arch: Arch::X86_64,
    },
    Key {
        os: Os::Linux,
        arch: Arch::X86_64,
    },
    Key {
        os: Os::Linux,
        arch: Arch::Aarch64,
    },
];

/// 问那台机器的 (OS, arch) 那条命令（一次性 exec，`exec_site_registry` 登记）。
pub const UNAME_CMD: &str = "uname -s -m";

/// `uname -s -m` 的收全结果 → 键。**纯函数**。
///
/// 退出码非 0 / 空 ⇒ 问不出 OS（Windows 默认 shell 没有 `uname` 就是这一形，那句 stderr 原样带回）；
/// 只答一段 ⇒ 问不出 arch；多于两段 ⇒ 问不出 OS（认不出哪段是什么）。
pub fn key_from_uname(exit: Option<u32>, stdout: &str, stderr: &str) -> Result<Key, Refusal> {
    if exit != Some(0) {
        let said = stderr.trim();
        return Err(Refusal::OsUnknown {
            why: if said.is_empty() {
                copy_text("rsByteTable.key.noAnswer", &[])
            } else if not_utf8(said) {
                copy_text("rsByteTable.key.notUtf8", &[])
            } else {
                copy_text("rsByteTable.key.said", &[("said", &said.to_string())])
            },
        });
    }
    let parts: Vec<&str> = stdout.split_whitespace().collect();
    match parts.as_slice() {
        [] => key_of("", ""),
        [os] => key_of(os, ""),
        [os, arch] => key_of(os, arch),
        _ if not_utf8(stdout) => Err(Refusal::OsUnknown {
            why: copy_text("rsByteTable.key.notUtf8", &[]),
        }),
        _ => Err(Refusal::OsUnknown {
            why: copy_text(
                "rsByteTable.key.unreadable",
                &[("reply", &(stdout.trim()).to_string())],
            ),
        }),
    }
}

/// 那台机器的回话**不是 UTF-8** ⇒ 说 `rsByteTable.key.notUtf8` 那半句（接在「查了什么：问过它，」后面），
/// 不照抄原文。
///
/// 真 Win11 现打（第 3 跳）：Windows 默认 shell 是 PowerShell，它按控制台代码页
/// （中文系统是 GBK）报「无法将 uname 项识别为 cmdlet…」；回话在后端那一跳按 UTF-8 **有损**解
/// （`dial/uses.rs`，认不出的字节成了 U+FFFD）⇒ 原样照抄进界面就是一串乱码。
/// ⇒ 不照抄、也不猜代码页（GBK / Shift-JIS / 1252 都有可能，猜错了一样是乱码），只说「不是 UTF-8」
/// 与它多半是什么。拒绝本身不变（问不出 OS ＝ 拒绝）。
/// 回话里有 U+FFFD ⇒ 那几个字节在后端按 UTF-8 解时就没解出来（有损解留下的记号）。
fn not_utf8(s: &str) -> bool {
    s.contains('\u{FFFD}')
}

// ═══ 身份：那台落点上那一份是谁 ═══════════════════════════════════════════════════════

/// 身份戳的开界标：后端二进制里那段 `<开><BUILD_ID><关>`（后端 `lib.rs::CC_MONITOR_BUILD_STAMP`）的头。
/// 两侧都 `use` 这一份：后端拼戳 · monitor 构建期扫内嵌字节 · 运行期扫推出去的字节 · 远端扫落点那一份。
pub const STAMP_OPEN: &str = "<<ccm-build-id:";
/// 身份戳的关界标（见 [`STAMP_OPEN`]）。
pub const STAMP_CLOSE: &str = ":ccm-build-id>>";

/// 后端流模式声明的能力 token（hello 帧的 `capabilities`，字典序）：后端 `lib.rs::CAPABILITIES` 取它，
/// monitor 拿它认 hello 里的 token、并在确认那台装的就是手上这一版时预知能力。
/// 每个 token 都要有后端 `split_stream_flags` 的剥离分支（后端 `every_capability_token_is_strippable` 钉着）。
pub const STREAM_CAPABILITIES: &[&str] = &["bg", "tail-only"];

/// 身份戳的两个界标（扫描函数的参数形）。生产里两侧都交 [`STAMP_OPEN`] / [`STAMP_CLOSE`]，判据可换一对去验扫描本身。
#[derive(Debug, Clone, Copy)]
pub struct Marks<'a> {
    pub open: &'a str,
    pub close: &'a str,
}

/// **那台机器上落点那一份后端是谁** —— 那张四态表 ＋ 0 字节那一格。
///
/// 〔墓碑 —— 从前这一问读的是同目录一份旁挂的版本标记文件，标记是**标签不是指纹**（「读它字节里那段身份戳，不跑它」）。〕
/// 四态**不许合并**：没装 ⇒ 装；问不出 / 问出多个 / 读不到 ⇒ **显式失败、不覆盖**（「判不了」要作为结论说出来）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteIdentity {
    /// 落点没有那个文件（没装）。
    Missing,
    /// 落点那个文件是 0 字节 —— 里面没有任何可保护的东西（真机截断事故就是这一形），按「没装」装。
    Empty,
    /// 那份字节里恰好一个身份戳。
    Stamp(String),
    /// 有文件、字节里一个身份戳都没有（不是我们编的 / 太旧 / 被改过）。
    NoStamp,
    /// 问出不止一个身份（身份不唯一）。
    Ambiguous(Vec<String>),
    /// 读不到（权限 / 链路 / 那台上没有能用的只读扫描手段）—— **判不了**。
    Unreadable(String),
}

/// 收齐的几个身份 → 结论（恰好一个才是身份）。
fn settle(mut ids: Vec<String>) -> RemoteIdentity {
    ids.sort();
    ids.dedup();
    match ids.len() {
        0 => RemoteIdentity::NoStamp,
        1 => RemoteIdentity::Stamp(ids.remove(0)),
        _ => RemoteIdentity::Ambiguous(ids),
    }
}

/// **手上一份字节自报的身份**（本机那一跳：不经 shell、不跑它，直接扫字节）。
/// 与远端那条扫描（[`stamp_scan_cmd`] ＋ [`interpret_stamp_scan`]）同一条规矩：界标之间 `[[:alnum:]_.-]+`，恰好一个才是身份。**纯函数**。
pub fn identity_of_bytes(bytes: &[u8], marks: Marks<'_>) -> RemoteIdentity {
    if bytes.is_empty() {
        return RemoteIdentity::Empty;
    }
    let (open, close) = (marks.open.as_bytes(), marks.close.as_bytes());
    let ok = |b: &u8| b.is_ascii_alphanumeric() || b"_.-".contains(b);
    let mut ids: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(k) = bytes[i..].windows(open.len()).position(|w| w == open) {
        let start = i + k + open.len();
        let n = bytes[start..].iter().take_while(|b| ok(b)).count();
        if n > 0 && bytes[start + n..].starts_with(close) {
            ids.push(String::from_utf8_lossy(&bytes[start..start + n]).into_owned());
        }
        i = start;
    }
    settle(ids)
}

/// 在目标机器上扫身份戳的那一条命令（档 A：目标机器**自己的**只读工具，一次 exec，常数字节回传）。
///
/// 正则与 `tests/scripts/re-embed.sh::bytes_id` 同一条（界标之间是 `[[:alnum:]_.-]`，这里要**至少一个字符** ——
/// 两个界标在 `.rodata` 里挨着就是空串那一形，[`identity_of_bytes`] 也不收它）。**纯函数**。
/// `word` 是那个文件在远端 shell 里的**写法**（固定落点常量，自带 `"$HOME"`），不是一条要 quote 的外来路径。
pub fn stamp_scan_cmd(word: &str, marks: Marks<'_>) -> String {
    let ere = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if "\\^$.|?*+()[]{}".contains(c) {
                    format!("\\{c}")
                } else {
                    c.to_string()
                }
            })
            .collect()
    };
    let pattern = format!("{}[[:alnum:]_.-]+{}", ere(marks.open), ere(marks.close));
    format!(
        "LC_ALL=C grep -aoE {} -- {word}",
        shell_quote_core::posix_quote(&pattern),
    )
}

/// 那一条扫描的收全结果 → [`RemoteIdentity`]（落点在、不是 0 字节时才问）。**纯函数**。
///
/// `grep` 的退出码：0 = 扫到了 · 1 = 一个都没有 · 其余（2 = 读不了 / 链路断了没给退出码）= 判不了。
pub fn interpret_stamp_scan(
    exit: Option<u32>,
    stdout: &str,
    stderr: &str,
    marks: Marks<'_>,
) -> RemoteIdentity {
    match exit {
        Some(0) => settle(
            stdout
                .lines()
                .filter_map(|l| l.trim().strip_prefix(marks.open)?.strip_suffix(marks.close))
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect(),
        ),
        Some(1) => RemoteIdentity::NoStamp,
        // 退出码不进这句话（`Some(..)` / `None` 是实现的形状）：没有错误输出时只说「没答完」。
        _ => RemoteIdentity::Unreadable(match stderr.trim() {
            "" => copy_text("rsSftp.stamp.unfinished", &[]),
            said => said.to_string(),
        }),
    }
}

// ═══ 计划答话的形状 · 戳的序键（换不换是判定，住后端 `control/deploy_plan.rs`）══════════════

/// 部署决策（答话的形状）。
#[derive(Debug, PartialEq, Eq)]
pub enum DeployAction {
    /// 远端版本与期望一致 → 无需部署。
    Skip,
    /// 需要部署，附人读原因。
    Deploy(String),
    /// 那台上是**另一版、但不比这一版旧**（更新 · 同序不同名 · 序解不出）⇒ **不动它**，
    /// 照旧连上那一份。`theirs` = 那台上那一份自报的身份；`why` = 人读原因（点名两边各是哪一版）。
    /// 只有后端 `control/deploy_plan.rs::identity_decision` 产这一格。
    Keep { theirs: String, why: String },
}

/// 〔部署只在「我的比盘上的新」时才换（BUILD_ID 可比序）〕**`BUILD_ID` 的序键** —— 唯一实现。
///
/// 形状 `p<代号>` ＋ `<一个小写字母>` ＋ `-<名>`（`p1a-history` … `p3m-ssh-zlib`）⇒ 序键 `(代号, 字母)`。
/// 解不出 ⇒ `None`（**不可比**，不是「最旧」也不是「最新」）。下一次 bump 写出解不出的形状由
/// 后端 `deploy_plan_tests::hx2_every_build_id_ever_shipped_has_an_order_and_the_history_climbs` 当场红（它读后端源码里的历史表）。
/// **纯函数**。
pub fn build_order(id: &str) -> Option<(u32, u8)> {
    let rest = id.strip_prefix('p')?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let generation: u32 = rest[..digits].parse().ok()?;
    let tail = rest[digits..].as_bytes();
    match tail {
        [letter, b'-', name @ ..] if letter.is_ascii_lowercase() && !name.is_empty() => {
            Some((generation, *letter))
        }
        _ => None,
    }
}

/// 09-11 之前那份 bash 启动器第二行的开头（那份文件已删，记号只能是字面量；出处：`git show e8f9e08e^:shared/ccm`）。
/// 从前那份入口两形的文件格式；认不认得出是后端判（`control/deploy_plan.rs::is_ours`，落点上那一份是不是它）。
pub const LAUNCHER_MARK: &str = "# ccm — cc-monitor 统一启动器";

/// 三行 shim（09-11 起历代）第二行的原文。它的生成器随「`ccm` 就是后端本体」删了（那一形只剩在已部署的机器上），
/// 记号从此只能是字面量（出处：`git show ef7baa63:src/frontend/shell/src/local_backend.rs` 的 `ccm_entry_shim`〔散文墓碑〕）。
pub const SHIM_MARK: &str = "# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）";

#[cfg(test)]
#[path = "../../../../tests/common/deploy-contract/lib_tests.rs"]
mod tests;
