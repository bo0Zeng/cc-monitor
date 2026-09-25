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
        assert_eq!(
            u16::from_le_bytes([b[18], b[19]]),
            machine,
            "{arch} 那份的 e_machine 不对"
        );
    }
}

// ── 〔RM1e〕推上去 ─────────────────────────────────────────────────────────────────
//
// 要求住址：用户 09-24 **V108**（`设计/99 §1`）「随后端部署、只传给开过远端全景的机器，后端经插件通用调用口按需起它」；
// V89（`SR1b.md`）「只写暂存区」——远端写只许 `~/.cc-monitor/staging` 与 `~/.cc-monitor/bin`。

fn backend_prod(rel: &str) -> String {
    let p = crate::guard_support::repo_src_root().join(rel);
    let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
    guard_core::production_code(&s)
}

/// 一个 `const NAME: &str = "…";` 的值。
fn str_const(src: &str, name: &str) -> String {
    let key = format!("const {name}: &str = \"");
    let at = src
        .find(&key)
        .unwrap_or_else(|| panic!("找不到 `{key}` —— 改了写法，本条跟着改"));
    let rest = &src[at + key.len()..];
    rest[..rest.find('"').unwrap()].to_string()
}

/// ★ 推的落点 == 后端找它的第二个候选：`<家>/<exit_policy::DIR_NAME>/bin/<PLUGIN_NAME>`；
/// 而且那个目录是 SR1b 远端写根之一（推得上去）。异源：三份后端源码现读。
#[test]
fn the_push_lands_where_the_backend_looks_and_inside_a_remote_write_root() {
    let pano = backend_prod("backend/control/panorama.rs");
    assert_eq!(
        str_const(&pano, "PLUGIN_NAME"),
        PROGRAM_NAME,
        "名字两边对不上"
    );
    // 后端 `fixed_candidates` 的第二个候选是按 家 → DIR_NAME → "bin" → PLUGIN_NAME 拼的。
    let at = pano
        .find("fn fixed_candidates(")
        .expect("后端没有 fixed_candidates");
    let body = &pano[at..at + pano[at..].find("\n}\n").unwrap()];
    let chain: String = body.split_whitespace().collect();
    assert!(
        chain.contains(".join(super::exit_policy::DIR_NAME).join(\"bin\").join(PLUGIN_NAME)"),
        "后端第二候选不再是 <家>/DIR_NAME/bin/PLUGIN_NAME —— 推的落点跟着改：{body}"
    );
    let dir_name = str_const(&backend_prod("backend/control/exit_policy.rs"), "DIR_NAME");
    assert_eq!(format!("{dir_name}/bin"), PUSH_DIR);
    let sftp = backend_prod("backend/dial/sftp.rs");
    let roots_at = sftp
        .find("const REMOTE_WRITE_ROOTS")
        .expect("后端没有 REMOTE_WRITE_ROOTS");
    let roots_line = &sftp[roots_at..roots_at + sftp[roots_at..].find("];").unwrap()];
    assert!(
        roots_line.contains(&format!("\"{PUSH_DIR}\"")),
        "推的目录不在远端写根里：{roots_line}"
    );
    assert_eq!(
        push_target("/home/u/"),
        (
            "/home/u/.cc-monitor/bin".to_string(),
            "/home/u/.cc-monitor/bin/cc-monitor-panorama".to_string()
        )
    );
}

/// `uname -s -m` 恰两段才认。
#[test]
fn uname_must_answer_exactly_os_and_arch() {
    assert_eq!(
        parse_uname("Linux x86_64\n"),
        Ok(("Linux".to_string(), "x86_64".to_string()))
    );
    for bad in ["", "Linux", "Linux x86_64 extra", "   \n"] {
        assert!(parse_uname(bad).is_err(), "{bad:?}");
    }
}
