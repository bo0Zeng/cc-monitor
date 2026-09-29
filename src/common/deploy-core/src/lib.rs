//! 要求住址：`调研/第四波记录/_施工/4d-lanes.md` MIG-3b 第 1 条 ——「`sftp.rs` 部署决策（该不该换 · 换成什么 · 身份判定）进后端；monitor 只放字节」。
//!
//! # 本 crate 是部署决策的**唯一一份**
//!
//! 三问，各自的规矩出处照旧：
//!
//! | 问 | 规矩 | 这里的口 |
//! |---|---|---|
//! | 那台机器要哪一格字节（换成什么） | `设计/96 §7.1` 表 A（键是 (OS, arch)）· `设计/01 §6.7a` 表 B（这个 origin 承不承诺） | [`key_from_uname`] · [`judge`] |
//! | 那台落点上那一份是谁（身份判定） | `设计/96 §7.2`：读它字节里的身份戳，不跑它 | [`stamp_scan_cmd`] · [`interpret_stamp_scan`] · [`identity_of_bytes`] |
//! | 该不该换 | `96 §7.2.3` 对照物是手上那份字节自报的身份 · 〔HX2 · D-b〕只升不降 | [`identity_decision`] · [`landing_verdict`] · [`build_order`] |
//!
//! 用它的两侧：
//! - **本机常驻后端**（`src/backend/deploy/`）：帧命令 `deploy-plan` —— 问那台 `uname`、stat / 扫身份戳，出计划；
//! - **monitor**：按计划里的那一格取自己带着的字节、放上去（`sftp.rs`）；另有三个与部署路无关的用户要同一份判定：
//!   本机那一份的身份（`local_backend.rs`）· 远端 hello 的新旧（`ssh_source.rs` / `remote_resident.rs`）· 全景推字节的表 A/B（`byte_table.rs`）。
//!
//! # 不在本 crate 的
//!
//! - 字节本身（`include_bytes!` 那几槽）与「这一版带没带那一格」—— monitor `byte_table.rs`（放字节的一侧才知道）；
//! - 身份戳的两个界标 —— 唯一住址在后端源码（`lib.rs` 的 `BUILD_STAMP_OPEN` / `BUILD_STAMP_CLOSE`），这里只收参数（[`Marks`]）；
//! - 任何 IO（问那台、读盘、写）。

use copy_core::copy_text;

// ═══ 表 A / 表 B：那台机器要哪一格字节 ═══════════════════════════════════════════════════

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

/// 要哪一类字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    /// 后端本体（带身份戳）。
    Backend,
    /// 只装代码全景引擎的小程序（没有身份戳，按 `--probe` 的能力表认代）。
    Panorama,
}

impl Product {
    /// 〔THIN〕线上那个词（帧命令 `deploy-slot` 的 `product`）—— 两侧对上的契约，只此一份。
    pub fn wire(self) -> &'static str {
        match self {
            Product::Backend => "backend",
            Product::Panorama => "panorama",
        }
    }

    /// [`Product::wire`] 的逆；认不出 ⇒ `None`。
    pub fn of_wire(w: &str) -> Option<Product> {
        [Product::Backend, Product::Panorama]
            .into_iter()
            .find(|p| p.wire() == w)
    }
}

/// 表 B 的 origin 轴：目标机器是不是自己。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Local,
    Remote,
}

/// 为什么不给字节 —— **拒绝点在写第一个字节之前**（`96 §7.1.4b`）。五形互不合并：下一步各不相同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// 答得出是什么机器，但表 A 里那一格没有产线（或根本不在 6 行里）。
    UnsupportedMachine { os: String, arch: String },
    /// 问不出 OS。
    OsUnknown { why: String },
    /// 问不出 arch。
    ArchUnknown { why: String },
    /// 那一格有产线，但这个 origin 今天不承诺那种机器（表 B；例：远端 Windows · 〔V132〕本机 (Linux, aarch64)）。
    /// 带着 `route`：同一形对「推到远端」与「本机自己」要说两句话（远端那句「只在本机用得上」对本机是假话，`D7`）。
    NotPromisedHere {
        os: String,
        arch: String,
        route: Route,
    },
    /// 那一格有产线、也承诺，但**这一版产物没带**那份字节（开发构建 / 没铺字节）。
    /// `96 §7.1.4b` 的四个 key 里没有它 —— 并进前四个就把「换一版产物」说成了「不支持这台机器」（`D7`）。
    /// 只有放字节的一侧（monitor）判得出这一形；本 crate 只提供它的名字与那句话。
    NotCarried { os: String, arch: String },
}

impl Refusal {
    /// 对用户说的那一句（`machine` = 机器名，本机说「本机」）。**Rust 侧唯一的出口**；
    /// 每一形的 key 都是字面量（`copy-table.vitest.ts` 按调用形状读它们，与文案表两向相等）。
    ///
    /// 〔TL1 · 4C〕多收一个 `product`：同一种拒绝对两件产物要说两句话
    /// （后端那几句里「它的会话不会自动接上」之类的后果，换成全景就是假话）⇒ 各一组 key（`deploy.refused.*` · `panorama.refused.*`）。
    pub fn say(&self, product: Product, machine: &str) -> String {
        match (product, self) {
            (Product::Backend, Refusal::UnsupportedMachine { os, arch }) => copy_text(
                "deploy.refused.unsupportedMachine",
                &[("machine", machine), ("os", os), ("arch", arch)],
            ),
            (Product::Backend, Refusal::OsUnknown { why }) => copy_text(
                "deploy.refused.osUnknown",
                &[("machine", machine), ("why", why)],
            ),
            (Product::Backend, Refusal::ArchUnknown { why }) => copy_text(
                "deploy.refused.archUnknown",
                &[("machine", machine), ("why", why)],
            ),
            (
                Product::Backend,
                Refusal::NotPromisedHere {
                    os,
                    route: Route::Remote,
                    ..
                },
            ) => copy_text(
                "deploy.refused.notPromisedHere",
                &[("machine", machine), ("os", os)],
            ),
            (
                Product::Backend,
                Refusal::NotPromisedHere {
                    os,
                    arch,
                    route: Route::Local,
                },
            ) => copy_text(
                "deploy.refused.notPromisedLocal",
                &[("machine", machine), ("os", os), ("arch", arch)],
            ),
            (Product::Backend, Refusal::NotCarried { os, arch }) => copy_text(
                "deploy.refused.notCarried",
                &[("machine", machine), ("os", os), ("arch", arch)],
            ),
            (Product::Panorama, Refusal::UnsupportedMachine { os, arch }) => copy_text(
                "panorama.refused.unsupportedMachine",
                &[("machine", machine), ("os", os), ("arch", arch)],
            ),
            (Product::Panorama, Refusal::OsUnknown { why }) => copy_text(
                "panorama.refused.osUnknown",
                &[("machine", machine), ("why", why)],
            ),
            (Product::Panorama, Refusal::ArchUnknown { why }) => copy_text(
                "panorama.refused.archUnknown",
                &[("machine", machine), ("why", why)],
            ),
            (
                Product::Panorama,
                Refusal::NotPromisedHere {
                    os,
                    route: Route::Remote,
                    ..
                },
            ) => copy_text(
                "panorama.refused.notPromisedHere",
                &[("machine", machine), ("os", os)],
            ),
            (
                Product::Panorama,
                Refusal::NotPromisedHere {
                    os,
                    arch,
                    route: Route::Local,
                },
            ) => copy_text(
                "panorama.refused.notPromisedLocal",
                &[("machine", machine), ("os", os), ("arch", arch)],
            ),
            (Product::Panorama, Refusal::NotCarried { os, arch }) => copy_text(
                "panorama.refused.notCarried",
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
/// （原样带出它答的那两个词）。**问不出 OS 不许当成 Linux**（`96 §7.1.4` 第 4 条）。
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
pub const LINES: &[(Product, Key)] = &[
    (
        Product::Backend,
        Key {
            os: Os::Windows,
            arch: Arch::X86_64,
        },
    ),
    (
        Product::Backend,
        Key {
            os: Os::Linux,
            arch: Arch::X86_64,
        },
    ),
    (
        Product::Backend,
        Key {
            os: Os::Linux,
            arch: Arch::Aarch64,
        },
    ),
    (
        Product::Panorama,
        Key {
            os: Os::Windows,
            arch: Arch::X86_64,
        },
    ),
    (
        Product::Panorama,
        Key {
            os: Os::Linux,
            arch: Arch::X86_64,
        },
    ),
    (
        Product::Panorama,
        Key {
            os: Os::Linux,
            arch: Arch::Aarch64,
        },
    ),
];

/// 表 B：这个 origin 今天承诺哪几种机器（`01 §6.7a`：本机 Windows x86_64 · 本机 Linux〔用户 09-18「算」〕· 远端 Linux 两个 arch）。
///
/// 〔V132 · 09-25〕用户原话「不承诺. 适配部分, 即os适配部分后面单独写单独做.」⇒ **本机 (Linux, aarch64) 不承诺**
/// （`96 §7.1.5` 那句「建议本机 Linux 限定 x86_64、本机侧走「不承诺」那一形」，即 [`Refusal::NotPromisedHere`]）。于是本表不再只按 OS 分：
/// 本机那两行都钉到 x86_64（本机 Windows arm64 本来就不在产线里，`V31`），远端 Linux 两个 arch 照旧。
/// 承诺面的唯一住址是 `tests/evidence/K-G4-platform-ledger.py` 的 `PROMISE_FACE`；本函数与它两向相等
/// 由 `byte_table_tests.rs::the_promise_face_in_the_ledger_equals_the_code` 钉着。
pub fn promised(route: Route, key: Key) -> bool {
    matches!(
        (route, key.os, key.arch),
        (Route::Local, Os::Windows, Arch::X86_64)
            | (Route::Local, Os::Linux, Arch::X86_64)
            | (Route::Remote, Os::Linux, _)
    )
}

/// 拒绝点的前三步（在向目标机器写第一个字节之前）：键 → 产线 → 承诺。四形各在一步上，不合并；
/// 第五形「这一版带没带」只有放字节的一侧判得出（monitor `byte_table::choose` 接着判）。
pub fn judge(product: Product, route: Route, key: Result<Key, Refusal>) -> Result<Key, Refusal> {
    let key = key?;
    let (os, arch) = (key.os.label().to_string(), key.arch.label().to_string());
    if !LINES.contains(&(product, key)) {
        return Err(Refusal::UnsupportedMachine { os, arch });
    }
    if !promised(route, key) {
        return Err(Refusal::NotPromisedHere { os, arch, route });
    }
    Ok(key)
}

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

/// 〔WIN1 · RT1 F4〕那台机器的回话**不是 UTF-8** ⇒ 说 `rsByteTable.key.notUtf8` 那半句（接在「查了什么：问过它，」后面），
/// 不照抄原文。
///
/// 真 Win11 现打（`第四波记录/RT1.md §1.2` 第 3 跳）：Windows 默认 shell 是 PowerShell，它按控制台代码页
/// （中文系统是 GBK）报「无法将 uname 项识别为 cmdlet…」；回话在后端那一跳按 UTF-8 **有损**解
/// （`dial/uses.rs`，认不出的字节成了 U+FFFD）⇒ 原样照抄进界面就是一串乱码。
/// ⇒ 不照抄、也不猜代码页（GBK / Shift-JIS / 1252 都有可能，猜错了一样是乱码），只说「不是 UTF-8」
/// 与它多半是什么。拒绝本身不变（`96 §7.1.4` 第 4 条：问不出 OS ＝ 拒绝）。
/// 回话里有 U+FFFD ⇒ 那几个字节在后端按 UTF-8 解时就没解出来（有损解留下的记号）。
fn not_utf8(s: &str) -> bool {
    s.contains('\u{FFFD}')
}

// ═══ 身份：那台落点上那一份是谁 ═══════════════════════════════════════════════════════

/// 身份戳的两个界标。**不写字面量**：唯一住址在后端源码（`lib.rs` 的 `BUILD_STAMP_OPEN` / `BUILD_STAMP_CLOSE`），
/// 后端直接交那两个常量，monitor 交 `build.rs` 抠出来的 `BACKEND_STAMP_OPEN` / `BACKEND_STAMP_CLOSE`。
#[derive(Debug, Clone, Copy)]
pub struct Marks<'a> {
    pub open: &'a str,
    pub close: &'a str,
}

/// 部署落点那个文件**本身**的取样结论（落点身份的第一步：没有 / 0 字节就不必再问它是谁）。
///
/// 纪律：**「问不出来」不许读成上面任何一个确定答案**
/// ——把无权限/传输失败当成「不在」会变成每次连接都重传（把版本门控拆了），
/// 当成「在」则退回本枚举要治的那个静默。**所以它不是 `bool`。**
/// ⚠ 成员就在下面，别在散文里复述一份基数 —— 那份字面量会在加成员那天变成假话。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TargetBinary {
    /// stat 说它在，且有字节。
    Present,
    /// stat **明确说**它不在。
    Missing,
    /// stat 说它在，但是 **0 字节** —— 不是假想形态：原子上传那一段（今天住后端 `dial/sftp.rs::put_atomic`）里
    /// 「绝不 set_metadata」那条注释记的就是真机 e2e 实测把后端截成 0 字节、
    /// 不可 exec 的那次事故。`try_exists` 会把它算成「在」。
    Empty,
    /// 问不出来（无权限 / 传输失败 / 服务器不给属性）—— 不许读成上面任何一个。
    Unknown,
}

/// stat 那两次取样的**解释**（纯函数，可单测 —— K-W4b）。
///
/// 形状：**吃两次调用各自的结果，不吃会话**。拆出来的理由是一个具体缺陷，不是行数：解释这一半原先焊在 async 体里，
/// 四个状态的映射规则因此一条判据都没有（`tests/evidence/K-W4b-readings.md`）。
///
/// 入参就是两次调用**降解之后**的结果：
/// - `metadata_size`：`None` = `metadata` 那次调用失败；`Some(inner)` = 成功，
///   `inner` 是服务器给的 size —— ⚠ `Some(None)` 是**服务器没给 size**，不是 0 字节。
/// - `exists`：`metadata` 失败时补问 `try_exists` 的结果（`None` = 它也答不出来）。
///   `metadata` 成功那一路根本不问它，那时它恒为 `None` 而本函数在那一路也不看它。
pub fn interpret_target_probe(
    metadata_size: Option<Option<u64>>,
    exists: Option<bool>,
) -> TargetBinary {
    match metadata_size {
        Some(Some(0)) => TargetBinary::Empty,
        // 服务器不给 size（`Some(None)`）≠ 0 字节：存在是确定的，别把「没说」读成「空」。
        Some(_) => TargetBinary::Present,
        None => match exists {
            Some(false) => TargetBinary::Missing,
            Some(true) => TargetBinary::Present,
            None => TargetBinary::Unknown,
        },
    }
}

/// 〔DP1 · 第四波〕**那台机器上落点那一份后端是谁** —— `设计/96 §7.2.4` 那张四态表 ＋ 0 字节那一格。
///
/// 〔墓碑 —— 从前这一问读的是同目录一份旁挂的版本标记文件，标记是**标签不是指纹**（`96 §7.2.1`：「读它字节里那段身份戳，不跑它」）。〕
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

/// 〔E2 · `96 §7.2.2`〕**手上一份字节自报的身份**（本机那一跳：不经 shell、不跑它，直接扫字节）。
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

/// 在目标机器上扫身份戳的那一条命令（`96 §7.2.1` 档 A：目标机器**自己的**只读工具，一次 exec，常数字节回传）。
///
/// 正则与 `tests/scripts/re-embed.sh::bytes_id` 同一条（界标之间是 `[[:alnum:]_.-]`，这里要**至少一个字符** ——
/// 两个界标在 `.rodata` 里挨着就是空串那一形，`build.rs::bytes_build_id` 也不收它）。**纯函数**。
/// 〔E2〕`word` 是那个文件在远端 shell 里的**写法**（固定落点常量，自带 `"$HOME"`），不是一条要 quote 的外来路径。
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

// ═══ 该不该换 ═══════════════════════════════════════════════════════════════════════

/// 部署决策。
#[derive(Debug, PartialEq, Eq)]
pub enum DeployAction {
    /// 远端版本与期望一致 → 无需部署。
    Skip,
    /// 需要部署，附人读原因。
    Deploy(String),
    /// 〔HX2 · 主会话 D-b〕那台上是**另一版、但不比这一版旧**（更新 · 同序不同名 · 序解不出）⇒ **不动它**，
    /// 照旧连上那一份。`theirs` = 那台上那一份自报的身份；`why` = 人读原因（点名两边各是哪一版）。
    /// 只有 [`identity_decision`] 产这一格。
    Keep { theirs: String, why: String },
}

/// 〔HX2 · 主会话 D-b「部署只在『我的比盘上的新』时才换（BUILD_ID 可比序）」〕**`BUILD_ID` 的序键** —— 唯一实现。
///
/// 形状 `p<代号>` ＋ `<一个小写字母>` ＋ `-<名>`（`p1a-history` … `p3m-ssh-zlib`）⇒ 序键 `(代号, 字母)`。
/// 解不出 ⇒ `None`（**不可比**，不是「最旧」也不是「最新」）。下一次 bump 写出解不出的形状由
/// `lib_tests::hx2_every_build_id_ever_shipped_has_an_order_and_the_history_climbs` 当场红（它读后端源码里的历史表）。
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

/// 「手上这一版比那台上的新」—— 两边都解得出序键、且这一版的严格大。解不出任何一边 ⇒ `false`（不可比 ⇒ 不换）。
pub fn is_newer(mine: &str, theirs: &str) -> bool {
    matches!((build_order(mine), build_order(theirs)), (Some(m), Some(t)) if m > t)
}

/// 要不要（重）部署 —— **对照物是手上那份字节自报的身份**（`96 §7.2.3`），不是源码常量。**纯函数**。
///
/// `Err` = 显式失败、**一个字节都不写**（出路交给用户：机器页「卸载后端」删掉那个文件，就是明确授权覆盖）。
///
/// 〔HX2 · D-b〕「另一版」那一格按 [`build_order`] 拆两格：那台上的**比这一版旧** ⇒ 换；**不比这一版旧** ⇒ [`DeployAction::Keep`]
/// （两个不同版本的 monitor 连同一台远端，从此只会升不会降，不再每次连上互相换掉 —— 审计 E3）。
pub fn identity_decision(
    id: &RemoteIdentity,
    expected: &str,
    machine: &str,
    path: &str,
) -> Result<DeployAction, String> {
    let hands_off = copy_text("rsSftp.identity.handsOff", &[]);
    match id {
        RemoteIdentity::Missing => Ok(DeployAction::Deploy(copy_text(
            "rsSftp.identity.missing",
            &[],
        ))),
        RemoteIdentity::Empty => Ok(DeployAction::Deploy(copy_text(
            "rsSftp.identity.empty",
            &[],
        ))),
        RemoteIdentity::Stamp(s) if s == expected => Ok(DeployAction::Skip),
        RemoteIdentity::Stamp(s) if is_newer(expected, s) => Ok(DeployAction::Deploy(copy_text(
            "rsSftp.identity.other",
            &[("s", &s.to_string()), ("expected", &expected.to_string())],
        ))),
        RemoteIdentity::Stamp(s) => Ok(DeployAction::Keep {
            theirs: s.clone(),
            why: copy_text(
                "rsSftp.identity.notOlder",
                &[
                    ("machine", &machine.to_string()),
                    ("s", &s.to_string()),
                    ("expected", &expected.to_string()),
                ],
            ),
        }),
        RemoteIdentity::NoStamp => Err(copy_text(
            "rsSftp.identity.unstamped",
            &[
                ("machine", &machine.to_string()),
                ("path", &path.to_string()),
                ("handsOff", &hands_off.to_string()),
            ],
        )),
        RemoteIdentity::Ambiguous(ids) => Err(copy_text(
            "rsSftp.identity.multiple",
            &[
                ("machine", &machine.to_string()),
                ("path", &path.to_string()),
                ("ids", &ids.join(&copy_text("rsSftp.identity.listSep", &[]))),
                ("handsOff", &hands_off.to_string()),
            ],
        )),
        RemoteIdentity::Unreadable(why) => Err(copy_text(
            "rsSftp.identity.undecidable",
            &[
                ("machine", &machine.to_string()),
                ("path", &path.to_string()),
                ("why", &why.to_string()),
            ],
        )),
    }
}

/// 09-11 之前那份 bash 启动器第二行的开头（那份文件已删，记号只能是字面量；出处：`git show e8f9e08e^:shared/ccm`）。
const LAUNCHER_MARK: &str = "# ccm — cc-monitor 统一启动器";

/// 三行 shim（09-11 起历代）第二行的原文。〔E2〕它的生成器随「`ccm` 就是后端本体」删了（那一形只剩在已部署的机器上），
/// 记号从此只能是字面量（出处：`git show f32fba42:src/frontend/shell/src/local_backend.rs` 的 `ccm_entry_shim`〔散文墓碑〕）。
const SHIM_MARK: &str = "# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）";

/// 这份文本是不是我们从前放的那一形 `ccm` 入口（三行 shim / bash 启动器两形之一）。**纯函数**。
/// 用户：旧落点 [`LEGACY_ENTRY_REL`]（〔THIN〕后端 `deploy_plan::retired_verdict`，从前在 monitor `ccm_legacy`）· 今天的落点上从前那份三行入口（[`landing_verdict`]）·
/// monitor 本机探针认 PATH 上另一个 `ccm`（`ccm_probe::reach_of`，登记的残留）。
pub fn is_ours(text: &str) -> bool {
    let mut lines = text.lines();
    let (Some(first), Some(second)) = (lines.next(), lines.next()) else {
        return false;
    };
    if !first.starts_with("#!") {
        return false;
    }
    second == SHIM_MARK || second.starts_with(LAUNCHER_MARK)
}

/// 〔E2〕落点那一份怎么办：先按身份戳判（[`identity_decision`]）；「不说自己是谁」时再看它是不是我们从前放的
/// 三行入口（[`is_ours`]，`old_entry` = 读回来的那一份字节，读不到 ⇒ `None`）—— 是 ⇒ 换成后端本体（部署那一步是原子替换）；
/// 不是 ⇒ 照旧显式失败、不动。**纯函数**。
pub fn landing_verdict(
    id: &RemoteIdentity,
    old_entry: Option<&[u8]>,
    expected: &str,
    machine: &str,
    path: &str,
) -> Result<DeployAction, String> {
    if matches!(id, RemoteIdentity::NoStamp)
        && old_entry.is_some_and(|b| is_ours(&String::from_utf8_lossy(b)))
    {
        return Ok(DeployAction::Deploy(copy_text(
            "rsSftp.identity.oldEntry",
            &[],
        )));
    }
    identity_decision(id, expected, machine, path)
}

/// 〔GP1 · THIN〕旧版放在远端的 `ccm` 入口（三行 shim / 更早的 bash 启动器）：家目录相对。
/// 后端判它的去向（`control/deploy_plan.rs::retired_verdict`）· monitor 照删 · 足迹那一行，同一个常量。
pub const LEGACY_ENTRY_REL: &str = ".local/bin/ccm";

/// 〔E2 · E-c〕旧默认 `backendPath` 落下的那份后端字节（`backendPath` 那一格删了之后没人再用它）：SFTP 那一侧（家目录相对）。
/// 后端问它是谁、monitor 删它，同一个常量。
pub const LEGACY_BACKEND_REL: &str = ".cc-monitor/bin/cc-monitor-backend";

/// 同一个文件在远端 shell 里的写法（扫身份戳那一条命令用）。
pub const LEGACY_BACKEND_WORD: &str = "\"$HOME\"/.cc-monitor/bin/cc-monitor-backend";

/// 〔E2 · E-c〕旧落点那份后端字节怎么办（部署时与每次连上各判一次）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyVerdict {
    /// 不在 ⇒ 不说话。
    Absent,
    /// 身份戳恰一个（是我们编的）⇒ 删。
    Remove,
    /// 别的（0 字节 · 没戳 · 多个戳 · 扫不动）⇒ 不动、说一句。
    Keep,
    /// 连问都没问成（链路）⇒ 不动、带上原话。
    Unknown(String),
}

/// 旧落点那一份的身份 → 怎么办。**纯函数**。
pub fn legacy_verdict(id: Result<RemoteIdentity, String>) -> LegacyVerdict {
    match id {
        Ok(RemoteIdentity::Missing) => LegacyVerdict::Absent,
        Ok(RemoteIdentity::Stamp(_)) => LegacyVerdict::Remove,
        Ok(_) => LegacyVerdict::Keep,
        Err(e) => LegacyVerdict::Unknown(e),
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/deploy-core/lib_tests.rs"]
mod tests;
