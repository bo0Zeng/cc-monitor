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
//! - 〔MIG-3b〕表 A / 表 B 本身（键 · 产线 · 承诺 · 拒绝五形与它们的话 · `uname` 的解读）：共享 crate `deploy-core`
//!   （本机常驻后端出部署计划要同一份）；本文件再导出那几个名字，自己只留**槽**与取字节口。
//! - 字节落到哪（条 62 一个常量，与来源无关）；推上去怎么推（`sftp.rs` 部署 · `panorama_bytes::push_to`）。
//! - 那台机器上已有的那一份是谁（〔MIG-3b〕本机常驻后端出计划时读它字节里的身份戳，`96 §7.2`）。

// 〔MIG-3b〕表 A / 表 B 本身（键 · 产线 · 承诺 · 拒绝那五形与它们的话 · `uname` 的解读）搬进了共享的 `deploy-core`：
//   本机常驻后端出部署计划（`deploy-plan`）要同一份判定。本文件留下的是**槽**（这一版带着哪几份字节）与取字节口。
// 〔THIN〕表 B 的承诺是裁决（`设计/00 §1.2`「判定只在后端」）⇒ 生产段只剩本机后端引导那一处经 [`choose`] 用它（报备，见头注）；
//   全景推字节「那台要哪一格」改问本机常驻后端（帧命令 `deploy-slot`），`uname` 那一问与它的解读不再住 monitor。
//   判据那几份（`key_of` · `key_from_uname` · `promised`）在测试档里直接 `use deploy_core::…`，本文件生产段不再导出判定名。
pub(crate) use deploy_core::{Arch, Key, Os, Product, Refusal, Route, LINES};

/// 表里取到的一份字节。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Picked {
    pub(crate) bytes: &'static [u8],
    /// 后端：`build.rs` 从**这份字节**里扫出的身份戳（`K-R70`）。全景小程序没有戳 ⇒ `None`。
    pub(crate) build_id: Option<&'static str>,
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
/// （RM1f 那一版本机取全景字节就是「原生 → 否则 musl」）。两份哪份该留是 `96 §7.3` 第一条，本表不替它裁。
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

/// 拒绝点（在向目标机器写第一个字节之前）：键 → 产线 → 承诺（`deploy_core::judge`）→ 这一版带没带（本文件）。五形各在一步上，不合并。
///
/// 〔THIN〕生产段只剩**本机后端引导**一个调用方（`local_backend_host::start_local_backend`）：那一刻本机后端还没起、问不了它 ——
/// 这一处判定留在 monitor 是已登记的残留（`调研/第四波记录/THIN.md §四`，待主会话拍板）。全景两条路改问本机常驻后端 `deploy-slot`。
pub(crate) fn choose(
    product: Product,
    route: Route,
    key: Result<Key, Refusal>,
) -> Result<Picked, Refusal> {
    let key = deploy_core::judge(product, route, key)?;
    pick(product, key).ok_or(Refusal::NotCarried {
        os: key.os.label().to_string(),
        arch: key.arch.label().to_string(),
    })
}

/// 〔THIN〕这一版为某件产物**真带着字节**的那几格（表 A 有产线的格里 [`pick`] 取得到的）—— 交给本机常驻后端判「那台要哪一格」的事实
/// （「这一版带没带」只有放字节的一侧知道）。**不按承诺筛**：承不承诺是后端判（`deploy-plan` · `deploy-slot`）。
pub(crate) fn carried(product: Product) -> Vec<Key> {
    LINES
        .iter()
        .filter(|(p, k)| *p == product && pick(product, *k).is_some())
        .map(|(_, k)| *k)
        .collect()
}

/// 〔MIG-3b〕这一版为远端带着哪几格后端字节、各自自报的身份 —— 交给本机常驻后端出部署计划的那一份事实。
/// 〔THIN〕不再按表 B 筛（从前 `promised(Remote, …)`）：承诺是后端判的。
pub(crate) fn carried_backends() -> Vec<(Key, &'static str)> {
    carried(Product::Backend)
        .into_iter()
        .filter_map(|k| Some((k, pick(Product::Backend, k)?.build_id?)))
        .collect()
}

// 〔THIN〕`probe_key`〔散文墓碑〕（问远端 `uname -s -m` 再解读）删了：全景推字节「那台要哪一格」整问进了本机常驻后端（`deploy-slot`），
//   monitor 里再没有一处跑 `uname`。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/byte_table_tests.rs"]
mod tests;
