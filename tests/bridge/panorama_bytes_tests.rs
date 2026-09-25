//! 〔RM1c · 第四波〕`panorama_bytes.rs` 的判据：(OS, arch) 选字节。

use super::*;

#[test]
fn only_linux_and_the_two_known_arches_get_bytes() {
    for (os, arch, want) in [
        ("Linux", "x86_64", Some("x86_64")),
        ("linux", "amd64", Some("x86_64")),
        ("Linux", "aarch64", Some("aarch64")),
        ("Linux", "arm64", Some("aarch64")),
        // 远端是别的 OS：答「没有」，不把一份 Linux ELF 推过去。
        ("Darwin", "arm64", None),
        ("MINGW64_NT-10.0", "x86_64", None),
        // 没登记的 arch：答「没有」。
        ("Linux", "riscv64", None),
        ("Linux", "", None),
    ] {
        assert_eq!(normalize_arch(os, arch), want, "{os} / {arch}");
        if want.is_none() {
            assert!(
                panorama_binary(os, arch).is_none(),
                "{os} / {arch} 不该有字节"
            );
        }
    }
}

/// ★ 选字节这一侧认的 arch == `build.rs::embed_panoramas` 放进 `OUT_DIR` 的那几个（两向集合相等，
/// **异源**：一侧是 `normalize_arch` 的产出值域，一侧读 `build.rs` 源码里那个 `for arch in [...]`）。
///
/// 多一个 ⇒ 选得出一个名字、却没有那份字节；少一个 ⇒ 放进来的字节永远没人选。
#[test]
fn the_arches_we_pick_are_exactly_the_ones_build_rs_embeds() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs");
    let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
    let at = src
        .find("fn embed_panoramas()")
        .expect("build.rs 里没有 embed_panoramas —— 改名了，本条跟着改");
    let body = &src[at..];
    let line = body
        .lines()
        .find(|l| l.trim_start().starts_with("for arch in ["))
        .expect("embed_panoramas 里找不到 `for arch in [...]`");
    let mut embedded: Vec<String> = line
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    embedded.sort();
    let mut picked: Vec<String> = ["x86_64", "amd64", "aarch64", "arm64"]
        .iter()
        .filter_map(|a| normalize_arch("linux", a))
        .map(str::to_string)
        .collect();
    picked.sort();
    picked.dedup();
    assert!(!embedded.is_empty(), "抠不到 arch —— 抽取坏了");
    assert_eq!(picked, embedded);
}

/// 字节在的那种构建（`embedded_panoramas`）：选出来的那一份真是那个 arch 的 ELF
/// （`e_machine`：x86_64 = 62，aarch64 = 183）—— 防「两个 arch 的字节铺反了」。
/// ⚠ 字节缺席的构建（开发树 / CI 的绝大多数）里本条不编译 —— 那一格判不了，不是绿。
#[cfg(embedded_panoramas)]
#[test]
fn the_embedded_bytes_are_the_right_arch() {
    for (arch, machine) in [("x86_64", 62u16), ("aarch64", 183u16)] {
        let b = panorama_binary("Linux", arch).expect("cfg 置了却选不出字节");
        assert_eq!(&b[..4], b"\x7fELF", "{arch} 那份不是 ELF");
        assert_eq!(u16::from_le_bytes([b[18], b[19]]), machine, "{arch} 那份的 e_machine 不对");
    }
}
