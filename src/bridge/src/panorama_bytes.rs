//! 〔RM1c · 第四波〕**全景小程序的字节从哪来** —— 按那台机器的 (OS, arch) 选内嵌的那一份。
//!
//! 只装代码全景引擎的独立小程序 `cc-monitor-panorama`（`src/panorama-engine`，用户 09-24 V108 选 B）
//! 随后端部署、**只传给开过远端全景的机器**。本模块只答「字节从哪来」：`build.rs::embed_panoramas`
//! 把两个 musl arch 的字节放进 `OUT_DIR`、置 `embedded_panoramas` cfg，这里 `include_bytes!` 它们。
//!
//! # 🔴 「推上去」不在这里
//!
//! 把字节推到远端 `~/.cc-monitor/bin/`（后端的 `control/panorama.rs` 找它的第二个候选）要走 F08
//! 那条部署路，而那条路正被搬进本机常驻后端（第四波 SR1b）。⇒ 本模块今天**零生产调用方**，
//! 接线归 SR1b 合进来之后的那一拍（要加什么写在 `调研/第四波记录/RM1c.md §4`）。
//! 下面那个 `allow` 只压编译器的死代码提示（`deadcode` 那一格数的就是它），不是压判据：
//! 判据（`tests::…`）逐条行使它。
//!
//! # 为什么按 (OS, arch) 而不是只按 arch
//!
//! 后端那两份按 arch 选（`sftp::backend_binary`），因为远端今天只有 Linux。全景小程序是**新**的一类字节，
//! 从第一天就把 OS 放进键里：远端是 Windows 的那天（`WN1` 那条线），这里答「没有」，
//! 而不是把一份 Linux ELF 推过去（`build.rs::embed_native_backend` 头注那条真机读数就是这个形状）。

/// 按那台机器的 `uname -s` / `uname -m`（小写、原样）选字节。**只认得出的组合才给**，其余 `None`。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn panorama_binary(os: &str, arch: &str) -> Option<&'static [u8]> {
    let arch = normalize_arch(os, arch)?;
    #[cfg(embedded_panoramas)]
    {
        static X86: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/panorama-x86_64"));
        static ARM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/panorama-aarch64"));
        match arch {
            "x86_64" => Some(X86),
            "aarch64" => Some(ARM),
            _ => None,
        }
    }
    #[cfg(not(embedded_panoramas))]
    {
        let _ = arch;
        None
    }
}

/// (OS, arch) → 内嵌表里的那个 arch 名。**纯函数**（与字节在不在无关，好测）。
///
/// 只认 Linux（musl 静态字节在任何 Linux 上都跑得起来）；arch 认 `uname -m` 的两种常见写法。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn normalize_arch(os: &str, arch: &str) -> Option<&'static str> {
    if !os.eq_ignore_ascii_case("linux") {
        return None;
    }
    match arch {
        "x86_64" | "amd64" => Some("x86_64"),
        "aarch64" | "arm64" => Some("aarch64"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/panorama_bytes_tests.rs"]
mod tests;
