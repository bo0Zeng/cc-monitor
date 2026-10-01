//! **全仓唯一的取字节口**：一台机器要哪一份可执行字节，按那台机器的 (OS, arch) 查一张表
//! （表 A）。这一个 origin 承不承诺那种机器（表 B）是判定，住后端。
//!
//! # 要求住址
//!
//! 规矩 4，逐字：「**选哪份字节，按目标机器的 (OS, arch) 选** —— 不按『本机 / 远端』选。
//! 本机只是『目标机器恰好是自己』的情形」。：「**`pick(os, arch)` 是全仓唯一的取字节入口**，
//! 两条投递路都从它取；判据：每个 `include_bytes!` 内嵌槽恰好挂在表 A 的一个键上，一个键恰好一个槽」。
//!
//! # 从前是什么样（墓碑）
//!
//! 〔墓碑 —— 本拍之前字节从三处各自取：远端部署在 `sftp.rs` 里只 `match arch`、不认 OS；
//!  本机 `local_backend_host.rs::start_local_backend` 压一道 `cfg!(target_os = "linux")` 的闸再按 `consts::ARCH` 取那两份
//!  musl；宿主交白卷时 `local_backend.rs` 再给一份按编译期 `TARGET` 定死的。〕 ⇒ 后端那三个槽今天都住本文件
//! （另有文件窗口那一槽，不在表 A 上，见 [`native_filewin`]）。
//!
//! # 表 A：键是 (OS, arch)，6 行
//!
//! 键从**探测得出来的**轴来：OS 3 值 × arch 2 值。键之外的组合（FreeBSD · riscv64 …）不在表里，
//! 与表里「那一格没有产线」的几行落在同一个拒绝形上（[`Refusal::UnsupportedMachine`]）。
//!
//! # 同一格两份来源 —— 只有「本机原生 × Linux」这一形，而且没有裁
//!
//! Linux 构建上，这一份产物按 `TARGET` 内嵌的本机原生后端（开发树 `re-embed.sh --native` 铺的）也落在
//! (Linux, 这台的 arch) 这一格，与远端那份 musl 同格。第一条逐字「本机 Linux 那两份字节哪份留 ——
//! 要先有条 62 那次迁移才谈得上」⇒ **本表不替它裁**：那一格按今天的次序给一个值（见 [`pick`]），判据把它
//! 登记成仅有的双来源格，多出别的就红。
//!
//! # 不在本文件的
//!
//! - 表 A 的键与行 · 拒绝五形与它们的话 · `uname` 的解读：契约 crate `deploy-contract`（两侧同一份）；
//!   表 B 的承诺是判定，住后端 `control/deploy_plan.rs`。本文件再导出那几个名字，自己只留**槽**与取字节口。
//! - 字节落到哪（条 62 一个常量，与来源无关）；推上去怎么推（`sftp.rs` 部署）。
//! - 那台机器上已有的那一份是谁（本机常驻后端出计划时读它字节里的身份戳）。

// 表 B 的承诺是裁决（「判定只在后端」）⇒ 住后端 `control/deploy_plan.rs::promised`；本文件只剩表 A 的行与槽。
//   本机后端引导那一刻问手上那份字节自己（`place-verdict`）。
pub(crate) use deploy_contract::{Arch, Key, Os, Refusal, LINES};

/// 表里取到的一份字节。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Picked {
    pub(crate) bytes: &'static [u8],
    /// `build.rs` 从**这份字节**里扫出的身份戳（`K-R70`）。
    pub(crate) build_id: &'static str,
}

// ═══ 槽：本文件是全仓唯一 `include_bytes!` 可执行字节的地方 ═══════════════════════════

/// 远端那两份 musl 后端（`build.rs::embed_backends` 放进 `OUT_DIR`）。
/// 🔴 `K-R70`：`build_id` 只许是 `build.rs` 从**这份字节**里扫出的身份戳。
#[cfg(embedded_backends)]
mod musl_backend {
    use super::Picked;
    pub(super) static X86: Picked = Picked {
        bytes: include_bytes!(concat!(env!("OUT_DIR"), "/backend-x86_64")),
        build_id: env!("BACKEND_EMBEDDED_ID_X86_64"),
    };
    pub(super) static ARM: Picked = Picked {
        bytes: include_bytes!(concat!(env!("OUT_DIR"), "/backend-aarch64")),
        build_id: env!("BACKEND_EMBEDDED_ID_AARCH64"),
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
        build_id: id,
    })
}

/// 没内嵌那一份时的同名壳。
#[cfg(not(embedded_native_backend))]
fn native_backend() -> Option<Picked> {
    None
}

/// 文件窗口那份程序（`build.rs::embed_native_filewin`，按这一份产物的 `TARGET` 编）。没有身份戳。
///
/// ⚠ **不挂表 A、不经 [`pick`]**：它是 monitor 自己的窗口进程，只在跑着这个 monitor 的这台机器上起，
/// 从不部署到别的机器 ⇒ 没有「目标机器是哪一格」这一问。唯一的消费者是 `filewin::proc::resolve_window_bin`（旁边没有时放下来再起）。
/// 🔴 路径必须是字面量（同上）⇒ 名字定死在两处：这一行与 `build.rs` 的 `NATIVE_BACKEND_DIR` ＋ `NATIVE_FILEWIN_FILE`。
#[cfg(embedded_native_filewin)]
pub(crate) fn native_filewin() -> Option<&'static [u8]> {
    Some(include_bytes!("../native-backend/cc-monitor-filewin"))
}

/// 没内嵌那一份时的同名壳。
#[cfg(not(embedded_native_filewin))]
pub(crate) fn native_filewin() -> Option<&'static [u8]> {
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

/// 本机原生那一槽挂在哪一格：这一份产物的 `TARGET`。
fn native_key() -> Option<Key> {
    Key::this_machine().ok()
}

/// 🔴 **全仓唯一的取字节入口**：只查槽，不判承诺。那一格没有这一版带着的字节 ⇒ `None`。
///
/// 双来源格（头注「同一格两份来源」）：Linux 构建上，本机原生那一槽也落在 (Linux, 这台的 arch)。次序照今天的：
/// musl 先（`start_local_backend` 从前就是「宿主那份 musl 优先、产物自带的兜底」）。两份哪份该留是第一条，本表不替它裁。
pub(crate) fn pick(key: Key) -> Option<Picked> {
    let native = || {
        if native_key() == Some(key) {
            native_backend()
        } else {
            None
        }
    };
    match key.os {
        Os::Linux => musl_backend(key.arch).or_else(native),
        Os::Windows => native(),
        Os::Mac => None,
    }
}

/// 取字节口的拒绝链里 monitor 这一侧的两步（都是查表 · 事实，不是判定）：键在不在表 A 有产线的行里（[`LINES`]，契约）→ 这一版带没带（本文件）。
///
/// 从前这里先过表 A / 表 B 那条判序（表 B 承诺是裁决，今天住后端 `control/deploy_plan.rs::judge`）。生产段唯一的调用方是本机后端引导（`local_backend_host::start_local_backend`），
/// 那一刻本机后端还没起 ⇒ 表 B 本机那一行改由手上那份字节自己判（帧命令 `place-verdict`，`local_backend::extract_embedded_to` 问）。
pub(crate) fn choose(key: Result<Key, Refusal>) -> Result<Picked, Refusal> {
    let key = key?;
    let (os, arch) = (key.os.label().to_string(), key.arch.label().to_string());
    if !LINES.contains(&key) {
        return Err(Refusal::UnsupportedMachine { os, arch });
    }
    pick(key).ok_or(Refusal::NotCarried { os, arch })
}

/// 这一版为远端**真带着**哪几格后端字节（表 A 有产线的格里 [`pick`] 取得到的）、各自自报的身份 ——
/// 交给本机常驻后端出部署计划的那一份事实（「这一版带没带」只有放字节的一侧知道）。
/// **不按承诺筛**：承不承诺是后端判（`deploy-plan`）。
pub(crate) fn carried_backends() -> Vec<(Key, &'static str)> {
    LINES
        .iter()
        .filter_map(|k| Some((*k, pick(*k)?.build_id)))
        .collect()
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/byte_table_tests.rs"]
mod tests;
