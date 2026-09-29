//! 〔RM1c · 第四波〕`panorama_bytes.rs` 的判据：(OS, arch) 选字节。

use super::*;

/// 〔DP1〕键的解析与字节住 `byte_table`：本条判的是「全景这一类字节认哪几格」（Linux 两格 ＋ 〔RM1f〕Windows x86_64
/// 那一格的原生产线），与「别的 OS / arch 答没有、不把一份 Linux ELF 推过去」。
#[test]
fn only_the_panorama_cells_with_a_production_line_get_bytes() {
    use crate::byte_table::{key_of, Arch, Key, Os, Product, LINES};
    let linux = |arch| {
        Some(Key {
            os: Os::Linux,
            arch,
        })
    };
    for (os, arch, want) in [
        ("Linux", "x86_64", linux(Arch::X86_64)),
        ("linux", "amd64", linux(Arch::X86_64)),
        ("Linux", "aarch64", linux(Arch::Aarch64)),
        ("Linux", "arm64", linux(Arch::Aarch64)),
        // 〔RM1f〕Windows x86_64 那一格有原生产线（`release.yml` 的 `Stage native panorama for self-extract`）。
        (
            "MINGW64_NT-10.0",
            "x86_64",
            Some(Key {
                os: Os::Windows,
                arch: Arch::X86_64,
            }),
        ),
        // 别的 OS / arch：答「没有」，不把一份 Linux ELF 推过去。
        ("Darwin", "arm64", None),
        ("MINGW64_NT-10.0", "arm64", None),
        // 没登记的 arch：答「没有」。
        ("Linux", "riscv64", None),
        ("Linux", "", None),
    ] {
        let got = key_of(os, arch)
            .ok()
            .filter(|k| LINES.contains(&(Product::Panorama, *k)));
        assert_eq!(got, want, "{os} / {arch}");
        if want.is_none() {
            // 〔TL1 · 4C〕从前判的是一个按两个词直接取字节的函数（远端推字节改走 `choose` 之后它删了）；
            //   今天判那个口本身：两条路都拒。
            for route in [
                crate::byte_table::Route::Remote,
                crate::byte_table::Route::Local,
            ] {
                assert!(
                    crate::byte_table::choose(Product::Panorama, route, key_of(os, arch)).is_err(),
                    "{os} / {arch} 不该有字节（{route:?}）"
                );
            }
        }
    }
}

/// ★ 表里全景的 **Linux** 那几格 == `build.rs::embed_panoramas` 放进 `OUT_DIR` 的那几个 arch（两向集合相等，
/// **异源**：一侧是 `byte_table::LINES` 里全景的 Linux 格子，一侧读 `build.rs` 源码里那个 `for arch in [...]`）。
/// Windows 那一格是原生产线（`embed_native_panorama`，按 `TARGET`），不在 musl 那一步里 —— 它由 `byte_table_tests` 对 `release.yml` 判。
///
/// 多一格 ⇒ 表里有一格、却没有那份字节；少一格 ⇒ 放进来的字节永远没人选。
#[test]
fn the_arches_we_pick_are_exactly_the_ones_build_rs_embeds() {
    use crate::byte_table::{key_of, Product, LINES};
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
    let embedded: std::collections::BTreeSet<_> = line
        .split('"')
        .skip(1)
        .step_by(2)
        .map(|a| {
            key_of("Linux", a)
                .unwrap_or_else(|r| panic!("build.rs 的 arch `{a}` 表里认不出：{r:?}"))
        })
        .collect();
    let lines: std::collections::BTreeSet<_> = LINES
        .iter()
        .filter(|(p, k)| *p == Product::Panorama && k.os == crate::byte_table::Os::Linux)
        .map(|(_, k)| *k)
        .collect();
    assert!(!embedded.is_empty(), "抠不到 arch —— 抽取坏了");
    assert_eq!(lines, embedded);
}

/// 字节在的那种构建（`embedded_panoramas`）：选出来的那一份真是那个 arch 的 ELF
/// （`e_machine`：x86_64 = 62，aarch64 = 183）—— 防「两个 arch 的字节铺反了」。
/// ⚠ 字节缺席的构建（开发树 / CI 的绝大多数）里本条不编译 —— 那一格判不了，不是绿。
#[cfg(embedded_panoramas)]
#[test]
fn the_embedded_bytes_are_the_right_arch() {
    for (arch, machine) in [("x86_64", 62u16), ("aarch64", 183u16)] {
        let key = crate::byte_table::key_of("Linux", arch).expect("认得出");
        let b = crate::byte_table::pick(crate::byte_table::Product::Panorama, key)
            .expect("cfg 置了却选不出字节")
            .bytes;
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
    // 后端 `fixed_candidates` 的第二个候选是按 家 → DIR_NAME → "bin" → 文件名 拼的；
    // 〔RM1f〕文件名 = `program_file_name()` = PLUGIN_NAME ＋ 那台机器的可执行后缀（Windows 本机 `.exe`）。
    let at = pano
        .find("fn fixed_candidates(")
        .expect("后端没有 fixed_candidates");
    let body = &pano[at..at + pano[at..].find("\n}\n").unwrap()];
    let chain: String = body.split_whitespace().collect();
    assert!(
        chain.contains("letfile=program_file_name();")
            && chain.contains(".join(super::exit_policy::DIR_NAME).join(\"bin\").join(&file)"),
        "后端第二候选不再是 <家>/DIR_NAME/bin/<program_file_name()> —— 推的落点跟着改：{body}"
    );
    let at = pano
        .find("fn program_file_name(")
        .expect("后端没有 program_file_name");
    let body: String = pano[at..at + pano[at..].find("\n}\n").unwrap()]
        .split_whitespace()
        .collect();
    assert!(
        body.contains("format!(\"{PLUGIN_NAME}{}\",std::env::consts::EXE_SUFFIX)"),
        "后端认的文件名不再是 PLUGIN_NAME ＋ 本机可执行后缀：{body}"
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

/// ★〔TL1 · 4C〕**问那台是什么机器，monitor 生产段只有一处**（`DP1.md` 报备 7「两份 `uname -s -m`」收成一份）。
///
/// 要求住址：`设计/96 §7.1.1b`「全仓唯一的取字节口」（`byte_table.rs` 头注逐字）＋ `设计/01 §6.7a` 规矩 4
/// （本机只是「目标机器恰好是自己」—— 两件产物、两条路走同一张表）。
/// 两向：`uname -s -m` 这个命令串在 monitor 生产段的住址集合 == {`byte_table.rs`}（多一处 = 又长出第二份；
/// 零处 = 尺子瞎了）；全景推字节那一臂真经 `choose(Panorama, Remote, probe_key(..))` 取、拒绝经 `say(Panorama, ..)` 说
/// （读本模块生产段，异源于 `byte_table` 自己的判据）。
#[test]
fn asking_what_the_machine_is_lives_in_one_place_and_the_push_goes_through_choose() {
    // 〔MIG-3b〕命令串随表 A / 表 B 搬进共享的 `deploy-core`（本机常驻后端出部署计划也问这一条）⇒ 射程是 monitor 生产段 ∪ 共享 crate，
    //   住址集合 == {`src/common/deploy-core/src/lib.rs`}；全景这一臂经 `byte_table::probe_key` 用它（下面那几格锚）。
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let needle = format!("uname -s {}", "-m");
    let mut at: Vec<String> = Vec::new();
    let repo = crate::guard_support::repo_root();
    for root in [
        base.join("src"),
        crate::guard_support::repo_src_root().join("common"),
    ] {
        for (p, src) in guard_core::scan_tree!(&root, &["rs"]) {
            if guard_core::production_code(&src).contains(&needle) {
                at.push(
                    p.strip_prefix(&repo)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    at.sort();
    assert_eq!(
        at,
        vec!["src/common/deploy-core/src/lib.rs".to_string()],
        "问机器的那条命令在 monitor 生产段 ∪ 共享 crate 的住址不是只有 deploy-core"
    );
    // 正控：同一把尺子在一段合成源码上数得出（不然上面的「只有一处」可能是空真 —— 它的非空由 deploy-core 那一处担着）。
    assert!(
        guard_core::production_code(&format!("const X: &str = \"{needle}\";")).contains(&needle)
    );
    let me: String = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/panorama_bytes.rs"
    ))
    .split_whitespace()
    .collect();
    for want in [
        "choose(Product::Panorama,Route::Remote,key)",
        "letkey=probe_key(&cfg).await?;",
        ".say(Product::Panorama,label)",
        "choose(Product::Panorama,Route::Local,Key::this_machine())",
        // 〔CP2b〕「本机」这个机器名进了文案表（取文口），锚跟着换。
        ".say(Product::Panorama,&copy_text(\"rsPanoramaBytes.local.machine\",&[]),)",
    ] {
        assert!(
            me.contains(want),
            "panorama_bytes.rs 生产段里找不到 `{want}`"
        );
    }
}

// ── 〔RM1f〕本机那一份 ───────────────────────────────────────────────────────────
//
// 要求住址：用户 09-24 **V108**（`设计/99 §1`）「之后本机也走这条路、monitor 摘内嵌引擎」·
// `INVARIANTS §40`（本机 ＝ 不走 ssh 的远端：本机后端也经插件口起那个小程序）。

/// ★〔RM1f · L1〕本机放下来的那一份，名字 == 本机后端去找的那个名字：`PROGRAM_NAME` ＋ **这台**的可执行后缀
/// （monitor 这一侧取 `build.rs` 按 `TARGET` 算的后缀，后端那一侧取它自己的 `EXE_SUFFIX` —— 同一台机器上必须相等；
/// 本判据跑在的就是 `TARGET` 那台）；落点目录与推到远端同一个 [`PUSH_DIR`]（上一条钉它 == 后端第二候选的目录）。
#[test]
fn the_local_copy_is_named_the_way_the_local_backend_looks_for_it() {
    assert_eq!(
        local_file_name(),
        format!("{PROGRAM_NAME}{}", std::env::consts::EXE_SUFFIX)
    );
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/panorama_bytes.rs"),
    )
    .expect("读 panorama_bytes.rs");
    let prod = guard_core::production_code(&src);
    let at = prod.find("fn place_local()").expect("没有 place_local");
    let body: String = prod[at..at + prod[at..].find("\n}\n").unwrap()]
        .split_whitespace()
        .collect();
    assert!(
        body.contains("PUSH_DIR.split('/')") && body.contains("&local_file_name(),"),
        "本机那一份的落点不再是 <家>/PUSH_DIR/<local_file_name()>：{body}"
    );
}

/// ★〔RM1f · L2〕放那一份：第一次写（且置可执行位）；**逐字节相等**的第二次零写；字节变了就重写；
/// 置可执行位失败 ⇒ 报、盘上没有半截的正式文件。
#[test]
fn placing_the_local_copy_writes_once_and_only_rewrites_when_the_bytes_differ() {
    use crate::backend::control::local_backend::place_local_panorama;
    use std::cell::Cell;
    let dir = std::env::temp_dir().join(format!("ccm-rm1f-place-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let made = Cell::new(0usize);
    let mk = |_: &std::path::Path| -> Result<(), String> {
        made.set(made.get() + 1);
        Ok(())
    };
    let p = place_local_panorama(
        &dir,
        "cc-monitor-panorama",
        b"v1-bytes",
        &mk,
        &crate::platform::fs::ensure_private_dir,
    )
    .unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"v1-bytes");
    assert_eq!(made.get(), 1, "第一次没置可执行位");
    // 同字节：零写（可执行位那一跳是「写了」的见证）。
    place_local_panorama(
        &dir,
        "cc-monitor-panorama",
        b"v1-bytes",
        &mk,
        &crate::platform::fs::ensure_private_dir,
    )
    .unwrap();
    assert_eq!(made.get(), 1, "逐字节相等还重写了一次");
    // 同长不同字节：要重写（只比长度会把旧版当新版留着）。
    place_local_panorama(
        &dir,
        "cc-monitor-panorama",
        b"v2-bytes",
        &mk,
        &crate::platform::fs::ensure_private_dir,
    )
    .unwrap();
    assert_eq!(made.get(), 2, "字节变了却没重写");
    assert_eq!(std::fs::read(&p).unwrap(), b"v2-bytes");
    // 置可执行位失败：报出来，正式文件还是上一份（没有半截的新版顶替它）。
    let bad = |_: &std::path::Path| -> Result<(), String> { Err("不许".to_string()) };
    assert!(place_local_panorama(
        &dir,
        "cc-monitor-panorama",
        b"v3-bytes!",
        &bad,
        &crate::platform::fs::ensure_private_dir
    )
    .is_err());
    assert_eq!(std::fs::read(&p).unwrap(), b"v2-bytes");
    let _ = std::fs::remove_dir_all(&dir);
}

/// ★〔RM1f · L3〕本机原生小程序的落点名字**四处同一个串**：`build.rs` 的 `NATIVE_BACKEND_DIR` ＋
/// `NATIVE_PANORAMA_FILE` == 消费侧 `include_bytes!` 那个字面量 == `re-embed.sh --native` 铺的 ==
/// `release.yml` Windows 那一格铺的。名字是定死的（字面量 `include_bytes!` 逼的），漂了就是「编进去一个空 cfg」
/// 或「铺了没人吃」。异源：四份文件现读。
#[test]
fn the_native_panorama_landing_is_spelled_the_same_in_all_four_places() {
    let root = crate::guard_support::repo_root();
    let read = |rel: &str| {
        std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读不到 {rel}：{e}"))
    };
    let build = read("src/frontend/shell/build.rs");
    let dir = str_const(&build, "NATIVE_BACKEND_DIR");
    let file = str_const(&build, "NATIVE_PANORAMA_FILE");
    let landing = format!("{dir}/{file}");
    assert_eq!(
        landing, "native-backend/cc-monitor-panorama",
        "正控：抠得到"
    );
    // 〔DP1 · 第四波〕消费侧那个 `include_bytes!` 搬进了 `byte_table.rs`（全仓唯一的取字节口）。
    let bytes_src = guard_core::production_code(&read("src/frontend/shell/src/byte_table.rs"));
    // 针在运行时拼（整串写死在本文件里，`cross_half_edge_registry` 会把它当成一处解析不出路径的内嵌）。
    let needle = format!("{}!(\"../{landing}\")", "include_bytes");
    assert!(
        bytes_src.contains(&needle),
        "消费侧的内嵌字面量不是 `../{landing}`"
    );
    let reembed = read("tests/scripts/re-embed.sh");
    assert!(
        reembed.contains(&format!("\"$NATIVE_DIR/{file}\""))
            && reembed.contains(&format!("\"$NATIVE_DIR/{file}.target\"")),
        "`re-embed.sh --native` 铺的不是 `{landing}` ＋ `.target`"
    );
    assert!(
        reembed.contains(&format!("NATIVE_DIR=\"$ROOT/src/frontend/shell/{dir}\"")),
        "`re-embed.sh` 的 NATIVE_DIR 不是 src/frontend/shell/{dir}"
    );
    let yml = read(".github/workflows/release.yml");
    assert!(
        yml.contains(&format!("$dst = \"src/frontend/shell/{landing}\"")),
        "`release.yml` Windows 那一格铺的不是 src/frontend/shell/{landing}"
    );
}
