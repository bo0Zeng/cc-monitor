//! 〔DP1 · 第四波〕**全仓唯一的取字节口**：一台机器要哪一份可执行字节，按那台机器的 (OS, arch) 查一张表
//! （`设计/96 §7.1` 的表 A）；这一个 origin 今天承不承诺那种机器，查另一张表（`设计/01 §6.7a` 的表 B）。
//!
//! # 要求住址
//!
//! `设计/01 §6.7a` 规矩 4，逐字：「**选哪份字节，按目标机器的 (OS, arch) 选** —— 不按『本机 / 远端』选。
//! 本机只是『目标机器恰好是自己』的情形」。`设计/96 §7.1.1b`：「**`pick(os, arch)` 是全仓唯一的取字节入口**，
//! 两条投递路都从它取；判据：每个 `include_bytes!` 内嵌槽恰好挂在表 A 的一个键上，一个键恰好一个槽」。
//!
//! # 从前是什么样（墓碑）
//!
//! 〔墓碑 —— 本拍之前字节从三处各自取：远端部署在 `sftp.rs` 里只 `match arch`、不认 OS；
//!  本机 `local_backend_host.rs::start_local_backend` 压一道 `cfg!(target_os = "linux")` 的闸再按 `consts::ARCH` 取那两份
//!  musl；宿主交白卷时 `local_backend.rs` 再给一份按编译期 `TARGET` 定死的。
//!  全景小程序那两份住 `panorama_bytes::panorama_binary` 的函数体里。〕 ⇒ 五个槽今天都住本文件（合主线时 RM1f 的原生全景小程序那一槽也收了进来，共六个）。
//!
//! # 表 A：键是 (OS, arch)，6 行
//!
//! 键从**探测得出来的**轴来：OS 3 值 × arch 2 值（`96 §7.1.1b`）。键之外的组合（FreeBSD · riscv64 …）不在表里，
//! 与表里「那一格没有产线」的几行落在同一个拒绝形上（[`Refusal::UnsupportedMachine`]）。
//!
//! # 同一格两份来源 —— 只有「本机原生 × Linux」这一形，而且没有裁
//!
//! Linux 构建上，这一份产物按 `TARGET` 内嵌的本机原生那两份（后端 · 〔RM1f〕全景小程序；开发树 `re-embed.sh --native`
//! 铺的）也落在 (Linux, 这台的 arch) 这一格，与远端那份 musl 同格。`96 §7.3` 第一条逐字「本机 Linux 那两份字节哪份留 ——
//! 要先有条 62 那次迁移才谈得上」⇒ **本表不替它裁**：那一格按各自今天的次序给一个值（见 [`pick`]），判据把这两格
//! 登记成仅有的双来源格，多出别的就红。
//!
//! # 不在本文件的
//!
//! - 字节落到哪（条 62 一个常量，与来源无关）；推上去怎么推（`sftp.rs` 部署 · `panorama_bytes::push_to`）。
//! - 那台机器上已有的那一份是谁（`sftp.rs` 读它字节里的身份戳，`96 §7.2`）。

use crate::copy_table::copy_text;

/// 表 A 的 OS 轴。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Os {
    Linux,
    Windows,
    Mac,
}

/// 表 A 的 arch 轴。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Arch {
    X86_64,
    Aarch64,
}

/// 表 A 的键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Key {
    pub(crate) os: Os,
    pub(crate) arch: Arch,
}

/// 要哪一类字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Product {
    /// 后端本体（带身份戳）。
    Backend,
    /// 只装代码全景引擎的小程序（没有身份戳，按 `--probe` 的能力表认代）。
    Panorama,
}

/// 表 B 的 origin 轴：目标机器是不是自己。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    Local,
    Remote,
}

/// 表里取到的一份字节。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Picked {
    pub(crate) bytes: &'static [u8],
    /// 后端：`build.rs` 从**这份字节**里扫出的身份戳（`K-R70`）。全景小程序没有戳 ⇒ `None`。
    pub(crate) build_id: Option<&'static str>,
}

/// 为什么不给字节 —— **拒绝点在写第一个字节之前**（`96 §7.1.4b`）。五形互不合并：下一步各不相同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// 答得出是什么机器，但表 A 里那一格没有产线（或根本不在 6 行里）。
    UnsupportedMachine { os: String, arch: String },
    /// 问不出 OS。
    OsUnknown { why: String },
    /// 问不出 arch。
    ArchUnknown { why: String },
    /// 那一格有产线，但这个 origin 今天不承诺那种机器（表 B；例：远端 Windows）。
    NotPromisedHere { os: String },
    /// 那一格有产线、也承诺，但**这一版产物没带**那份字节（开发构建 / 没铺字节）。
    /// `96 §7.1.4b` 的四个 key 里没有它 —— 并进前四个就把「换一版产物」说成了「不支持这台机器」（`D7`）。
    NotCarried { os: String, arch: String },
}

impl Refusal {
    /// 对用户说的那一句（`machine` = 机器名，本机说「本机」）。**Rust 侧唯一的出口**；
    /// 每一形的 key 都是字面量（`copy-table.vitest.ts` 按调用形状读它们，与文案表两向相等）。
    ///
    /// 〔TL1 · 4C〕多收一个 `product`：全景推字节也走 [`choose`] 之后，同一种拒绝对两件产物要说两句话
    /// （后端那几句里「它的会话不会自动接上」之类的后果，换成全景就是假话）⇒ 各一组 key（`deploy.refused.*` · `panorama.refused.*`）。
    pub(crate) fn say(&self, product: Product, machine: &str) -> String {
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
            (Product::Backend, Refusal::NotPromisedHere { os }) => copy_text(
                "deploy.refused.notPromisedHere",
                &[("machine", machine), ("os", os)],
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
            (Product::Panorama, Refusal::NotPromisedHere { os }) => copy_text(
                "panorama.refused.notPromisedHere",
                &[("machine", machine), ("os", os)],
            ),
            (Product::Panorama, Refusal::NotCarried { os, arch }) => copy_text(
                "panorama.refused.notCarried",
                &[("machine", machine), ("os", os), ("arch", arch)],
            ),
        }
    }
}

impl Os {
    fn label(self) -> &'static str {
        match self {
            Os::Linux => "Linux",
            Os::Windows => "Windows",
            Os::Mac => "macOS",
        }
    }
}

impl Arch {
    fn label(self) -> &'static str {
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
pub(crate) fn key_of(os: &str, arch: &str) -> Result<Key, Refusal> {
    let (os, arch) = (os.trim(), arch.trim());
    if os.is_empty() {
        return Err(Refusal::OsUnknown {
            why: "它没有答".to_string(),
        });
    }
    if arch.is_empty() {
        return Err(Refusal::ArchUnknown {
            why: "它没有答".to_string(),
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
    pub(crate) fn label(self) -> String {
        format!("{} / {}", self.os.label(), self.arch.label())
    }

    /// 本机：目标机器恰好是自己（`consts` 在编译期就是这一份产物的 `TARGET`）。
    pub(crate) fn this_machine() -> Result<Key, Refusal> {
        key_of(std::env::consts::OS, std::env::consts::ARCH)
    }
}

/// 表 A 里「有产线」的格子（`release.yml` 真编得出字节的那几格；判据对着 `release.yml` 读）。
pub(crate) const LINES: &[(Product, Key)] = &[
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

/// 表 B：这个 origin 今天承诺哪几种 OS（`01 §6.7a`：本机 Windows · 本机 Linux〔用户 09-18「算」〕· 远端 Linux）。
pub(crate) fn promised(route: Route, os: Os) -> bool {
    matches!(
        (route, os),
        (Route::Local, Os::Windows) | (Route::Local, Os::Linux) | (Route::Remote, Os::Linux)
    )
}

// ═══ 槽：本文件是全仓唯一 `include_bytes!` 可执行字节的地方 ═══════════════════════════

/// 远端那两份 musl 后端（`build.rs::embed_backends` 放进 `OUT_DIR`）。
/// 🔴 `K-R70`：`build_id` 只许是 `build.rs` 从**这份字节**里扫出的身份戳。
#[cfg(embedded_backends)]
mod musl_backend {
    use super::Picked;
    pub(super) static X86: Picked = Picked {
        bytes: include_bytes!(concat!(env!("OUT_DIR"), "/backend-x86_64")),
        build_id: Some(env!("BACKEND_EMBEDDED_ID_X86_64")),
    };
    pub(super) static ARM: Picked = Picked {
        bytes: include_bytes!(concat!(env!("OUT_DIR"), "/backend-aarch64")),
        build_id: Some(env!("BACKEND_EMBEDDED_ID_AARCH64")),
    };
}

/// 全景小程序那两份 musl（`build.rs::embed_panoramas`）。没有身份戳。
#[cfg(embedded_panoramas)]
mod musl_panorama {
    use super::Picked;
    pub(super) static X86: Picked = Picked {
        bytes: include_bytes!(concat!(env!("OUT_DIR"), "/panorama-x86_64")),
        build_id: None,
    };
    pub(super) static ARM: Picked = Picked {
        bytes: include_bytes!(concat!(env!("OUT_DIR"), "/panorama-aarch64")),
        build_id: None,
    };
}

/// 这一份产物按 `TARGET` 内嵌的本机原生后端（`build.rs::embed_native_backend`）。
///
/// 🔴 路径必须是字面量（`cross_half_edge_registry` 不认拼出来的 `include_*!`）⇒ 名字定死在两处：这一行与
/// `build.rs` 的 `NATIVE_BACKEND_DIR` / `NATIVE_BACKEND_FILE`（`the_native_backend_path_is_spelled_the_same_on_both_sides` 对拍）。
#[cfg(embedded_native_backend)]
fn native_backend() -> Option<Picked> {
    let id = env!("BACKEND_NATIVE_ID");
    if id.is_empty() {
        // 走不到（`build.rs` 缺身份当场 panic），但不假设走不到：空身份拼不出释放名。
        return None;
    }
    Some(Picked {
        bytes: include_bytes!("../native-backend/cc-monitor-native"),
        build_id: Some(id),
    })
}

/// 没内嵌那一份时的同名壳。
#[cfg(not(embedded_native_backend))]
fn native_backend() -> Option<Picked> {
    None
}

/// 〔RM1f → DP1〕这一份产物按 `TARGET` 内嵌的原生全景小程序（`build.rs::embed_native_panorama`）。没有身份戳。
///
/// 🔴 路径必须是字面量（同上）⇒ 名字定死在两处：这一行与 `build.rs` 的 `NATIVE_BACKEND_DIR` ＋ `NATIVE_PANORAMA_FILE`。
#[cfg(embedded_native_panorama)]
fn native_panorama() -> Option<Picked> {
    Some(Picked {
        bytes: include_bytes!("../native-backend/cc-monitor-panorama"),
        build_id: None,
    })
}

/// 没内嵌那一份时的同名壳。
#[cfg(not(embedded_native_panorama))]
fn native_panorama() -> Option<Picked> {
    None
}

fn musl_backend(arch: Arch) -> Option<Picked> {
    #[cfg(embedded_backends)]
    {
        Some(match arch {
            Arch::X86_64 => musl_backend::X86,
            Arch::Aarch64 => musl_backend::ARM,
        })
    }
    #[cfg(not(embedded_backends))]
    {
        let _ = arch;
        None
    }
}

fn musl_panorama(arch: Arch) -> Option<Picked> {
    #[cfg(embedded_panoramas)]
    {
        Some(match arch {
            Arch::X86_64 => musl_panorama::X86,
            Arch::Aarch64 => musl_panorama::ARM,
        })
    }
    #[cfg(not(embedded_panoramas))]
    {
        let _ = arch;
        None
    }
}

/// 本机原生那一槽挂在哪一格：这一份产物的 `TARGET`。
fn native_key() -> Option<Key> {
    Key::this_machine().ok()
}

/// 🔴 **全仓唯一的取字节入口**：只查槽，不判承诺（承诺在 [`choose`]）。那一格没有这一版带着的字节 ⇒ `None`。
///
/// 双来源格（头注「同一格两份来源」）：Linux 构建上，本机原生那两槽也落在 (Linux, 这台的 arch)。**两类字节的次序
/// 各照各自今天的**：后端 musl 先（`start_local_backend` 从前就是「宿主那份 musl 优先、产物自带的兜底」）；全景原生先
/// （RM1f 的 `local_panorama_binary` 就是「原生 → 否则 musl」）。两份哪份该留是 `96 §7.3` 第一条，本表不替它裁。
pub(crate) fn pick(product: Product, key: Key) -> Option<Picked> {
    let mine = native_key() == Some(key);
    let native_b = || if mine { native_backend() } else { None };
    let native_p = || if mine { native_panorama() } else { None };
    match (product, key.os) {
        (Product::Backend, Os::Linux) => musl_backend(key.arch).or_else(native_b),
        (Product::Backend, Os::Windows) => native_b(),
        (Product::Panorama, Os::Linux) => native_p().or_else(|| musl_panorama(key.arch)),
        (Product::Panorama, Os::Windows) => native_p(),
        (Product::Backend | Product::Panorama, Os::Mac) => None,
    }
}

/// 拒绝点（在向目标机器写第一个字节之前）：键 → 产线 → 承诺 → 这一版带没带。五形各在一步上，不合并。
pub(crate) fn choose(
    product: Product,
    route: Route,
    key: Result<Key, Refusal>,
) -> Result<Picked, Refusal> {
    let key = key?;
    let (os, arch) = (key.os.label().to_string(), key.arch.label().to_string());
    if !LINES.contains(&(product, key)) {
        return Err(Refusal::UnsupportedMachine { os, arch });
    }
    if !promised(route, key.os) {
        return Err(Refusal::NotPromisedHere { os });
    }
    pick(product, key).ok_or(Refusal::NotCarried { os, arch })
}

/// 问那台机器的 (OS, arch) 那条命令（一次性 exec，`exec_site_registry` 登记）。
const UNAME_CMD: &str = "uname -s -m";

/// `uname -s -m` 的收全结果 → 键。**纯函数**。
///
/// 退出码非 0 / 空 ⇒ 问不出 OS（Windows 默认 shell 没有 `uname` 就是这一形，那句 stderr 原样带回）；
/// 只答一段 ⇒ 问不出 arch；多于两段 ⇒ 问不出 OS（认不出哪段是什么）。
pub(crate) fn key_from_uname(
    exit: Option<u32>,
    stdout: &str,
    stderr: &str,
) -> Result<Key, Refusal> {
    if exit != Some(0) {
        let said = stderr.trim();
        return Err(Refusal::OsUnknown {
            why: if said.is_empty() {
                "它没有答".to_string()
            } else if not_utf8(said) {
                NOT_UTF8_ANSWER.to_string()
            } else {
                format!("它答「{said}」")
            },
        });
    }
    let parts: Vec<&str> = stdout.split_whitespace().collect();
    match parts.as_slice() {
        [] => key_of("", ""),
        [os] => key_of(os, ""),
        [os, arch] => key_of(os, arch),
        _ if not_utf8(stdout) => Err(Refusal::OsUnknown {
            why: NOT_UTF8_ANSWER.to_string(),
        }),
        _ => Err(Refusal::OsUnknown {
            why: format!("它的回答认不出：「{}」", stdout.trim()),
        }),
    }
}

/// 〔WIN1 · RT1 F4〕那台机器的回话**不是 UTF-8** 时说的那半句（接在「查了什么：问过它，」后面）。
///
/// 真 Win11 现打（`第四波记录/RT1.md §1.2` 第 3 跳）：Windows 默认 shell 是 PowerShell，它按控制台代码页
/// （中文系统是 GBK）报「无法将 uname 项识别为 cmdlet…」；回话在后端那一跳按 UTF-8 **有损**解
/// （`dial/uses.rs`，认不出的字节成了 U+FFFD）⇒ 原样照抄进界面就是一串乱码。
/// ⇒ 不照抄、也不猜代码页（GBK / Shift-JIS / 1252 都有可能，猜错了一样是乱码），只说「不是 UTF-8」
/// 与它多半是什么。拒绝本身不变（`96 §7.1.4` 第 4 条：问不出 OS ＝ 拒绝）。
const NOT_UTF8_ANSWER: &str =
    "它的回答不是 UTF-8 编码，多半是 Windows 的 PowerShell 或 cmd 在说没有 uname 这个命令";

/// 回话里有 U+FFFD ⇒ 那几个字节在后端按 UTF-8 解时就没解出来（有损解留下的记号）。
fn not_utf8(s: &str) -> bool {
    s.contains('\u{FFFD}')
}

/// 问远端那台的键。链路本身没通 ⇒ `Err`（普通失败：连都连不上，后面的流也起不来）；问得出答案 ⇒ `Ok(键或拒绝)`。
pub(crate) async fn probe_key(
    cfg: &crate::ssh_source::RemoteConfig,
) -> Result<Result<Key, Refusal>, String> {
    let got = crate::ssh_source::connect_and_exec_capture(cfg, UNAME_CMD, None).await?;
    Ok(key_from_uname(got.exit_status, &got.stdout, &got.stderr))
}

#[cfg(test)]
#[path = "../../../tests/bridge/byte_table_tests.rs"]
mod tests;
