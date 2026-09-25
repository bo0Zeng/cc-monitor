//! 〔DP1 · 第四波〕`byte_table.rs` 的判据：全仓唯一的取字节口 · 表 A（键 (OS, arch)）· 表 B（承诺面）· 拒绝点。
//!
//! # 要求住址
//!
//! - `设计/01 §6.7a` 规矩 4，逐字：「**选哪份字节，按目标机器的 (OS, arch) 选** —— 不按『本机 / 远端』选」；
//!   同节表 B：「本机 Windows ✅ 承诺 · 远端 Linux（x86_64 · aarch64）✅ 承诺 · 本机 macOS · 远端 Windows · 远端 macOS ⬜ 现在不做 ⇒ 显式拒绝」。
//! - `设计/96 §7.1.1b`，逐字：「**`pick(os, arch)` 是全仓唯一的取字节入口**，两条投递路都从它取；判据：每个 `include_bytes!`
//!   内嵌槽恰好挂在表 A 的一个键上，一个键恰好一个槽」。
//! - `设计/96 §7.1.4`，逐字：「**OS 问不出来 ＝ 拒绝，不是回落**：不许『问不出 OS 就当它是 Linux』」。
//!
//! 期望值一律手写自上面三处原文（**不**取自被测的 `LINES` / `promised`），两侧同源会恒真。

use super::*;
use std::collections::BTreeSet;

fn k(os: Os, arch: Arch) -> Key {
    Key { os, arch }
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"))
}

// ═══ 键的解析 ═══════════════════════════════════════════════════════════════════════════

/// B5：两个答话 → 键。问不出 ⇒ `os_unknown` / `arch_unknown`；答得出但不在 3 × 2 里 ⇒ `unsupported_machine`。
#[test]
fn the_key_is_read_from_what_the_machine_answers() {
    use Arch::*;
    use Os::*;
    for (os, arch, want) in [
        ("Linux", "x86_64", Ok(k(Linux, X86_64))),
        ("Linux", "aarch64", Ok(k(Linux, Aarch64))),
        ("linux", "amd64", Ok(k(Linux, X86_64))),
        ("Darwin", "arm64", Ok(k(Mac, Aarch64))),
        ("macos", "x86_64", Ok(k(Mac, X86_64))),
        ("MINGW64_NT-10.0-19045", "x86_64", Ok(k(Windows, X86_64))),
        ("MSYS_NT-10.0", "x86_64", Ok(k(Windows, X86_64))),
        ("CYGWIN_NT-10.0", "x86_64", Ok(k(Windows, X86_64))),
        ("windows", "aarch64", Ok(k(Windows, Aarch64))),
    ] {
        assert_eq!(key_of(os, arch), want, "{os} / {arch}");
    }
    // 问不出：不许回落成任何一格。
    assert!(matches!(
        key_of("", "x86_64"),
        Err(Refusal::OsUnknown { .. })
    ));
    assert!(matches!(key_of("  ", ""), Err(Refusal::OsUnknown { .. })));
    assert!(matches!(
        key_of("Linux", ""),
        Err(Refusal::ArchUnknown { .. })
    ));
    // 答得出、但不是表里的值：原样带出它答的词。
    assert_eq!(
        key_of("FreeBSD", "amd64"),
        Err(Refusal::UnsupportedMachine {
            os: "FreeBSD".into(),
            arch: "x86_64".into()
        })
    );
    assert_eq!(
        key_of("Linux", "riscv64"),
        Err(Refusal::UnsupportedMachine {
            os: "Linux".into(),
            arch: "riscv64".into()
        })
    );
}

/// B5b：`uname -s -m` 收全的三样 → 键。退出码不是 0（Windows 默认 shell 没有 `uname`）/ 空 ⇒ 问不出 OS；只一段 ⇒ 问不出 arch。
#[test]
fn uname_answers_map_to_a_key_or_a_named_refusal() {
    assert_eq!(
        key_from_uname(Some(0), "Linux x86_64\n", ""),
        Ok(k(Os::Linux, Arch::X86_64))
    );
    match key_from_uname(
        Some(1),
        "",
        "'uname' is not recognized as an internal command",
    ) {
        Err(Refusal::OsUnknown { why }) => assert!(why.contains("is not recognized"), "{why}"),
        other => panic!("退出码 1 该是问不出 OS：{other:?}"),
    }
    // 没送退出码（连接被掐）≠ 0：不许读成「跑成了」。
    assert!(matches!(
        key_from_uname(None, "Linux x86_64", ""),
        Err(Refusal::OsUnknown { .. })
    ));
    assert!(matches!(
        key_from_uname(Some(0), "", ""),
        Err(Refusal::OsUnknown { .. })
    ));
    assert!(matches!(
        key_from_uname(Some(0), "Linux\n", ""),
        Err(Refusal::ArchUnknown { .. })
    ));
    assert!(matches!(
        key_from_uname(Some(0), "Linux x86_64 extra", ""),
        Err(Refusal::OsUnknown { .. })
    ));
}

// ═══ 拒绝点：六键 × 两路 × 两类字节的全表 ═══════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
enum Want {
    /// 给（这一版带着就 `Ok`，没带就 `NotCarried` —— 两者按 `pick` 现打分）。
    Give,
    Unsupported,
    NotPromised,
}

/// B4：`choose` 的判序。期望手写自 `01 §6.7a` 表 B 与 `96 §7.1.1b` 表 A 的「今天有没有字节」一栏。
#[test]
fn choose_answers_every_cell_of_both_tables() {
    use Arch::*;
    use Os::*;
    use Product::*;
    use Route::*;
    use Want::*;
    let expect: &[(Product, Route, Key, Want)] = &[
        // 后端：表 A 行 1（Windows, x86_64）有产线 —— 本机承诺、远端现在不做。
        (Backend, Local, k(Windows, X86_64), Give),
        (Backend, Remote, k(Windows, X86_64), NotPromised),
        // 行 2（Windows, aarch64）：用户 09-18「不含 arm64」⇒ 无产线。
        (Backend, Local, k(Windows, Aarch64), Unsupported),
        (Backend, Remote, k(Windows, Aarch64), Unsupported),
        // 行 3 / 4（Linux）：远端承诺；本机 Linux 用户 09-18「算」。
        (Backend, Local, k(Linux, X86_64), Give),
        (Backend, Remote, k(Linux, X86_64), Give),
        (Backend, Local, k(Linux, Aarch64), Give),
        (Backend, Remote, k(Linux, Aarch64), Give),
        // 行 5 / 6（macOS）：无产线。
        (Backend, Local, k(Mac, X86_64), Unsupported),
        (Backend, Remote, k(Mac, X86_64), Unsupported),
        (Backend, Local, k(Mac, Aarch64), Unsupported),
        (Backend, Remote, k(Mac, Aarch64), Unsupported),
        // 全景小程序：只有 Linux 两格的 musl 产线（`release.yml` 的 `Cross-compile panorama for both musl targets`）。
        (Panorama, Remote, k(Linux, X86_64), Give),
        (Panorama, Remote, k(Linux, Aarch64), Give),
        (Panorama, Local, k(Linux, X86_64), Give),
        (Panorama, Remote, k(Windows, X86_64), Unsupported),
        (Panorama, Local, k(Windows, X86_64), Unsupported),
        (Panorama, Remote, k(Mac, Aarch64), Unsupported),
    ];
    for &(product, route, key, want) in expect {
        let got = choose(product, route, Ok(key));
        match (want, &got) {
            (Give, Ok(_)) => assert!(pick(product, key).is_some()),
            (Give, Err(Refusal::NotCarried { .. })) => assert!(
                pick(product, key).is_none(),
                "{product:?} {route:?} {key:?}：表里有字节却说没带"
            ),
            (Unsupported, Err(Refusal::UnsupportedMachine { .. })) => {}
            (NotPromised, Err(Refusal::NotPromisedHere { .. })) => {}
            _ => panic!("{product:?} {route:?} {key:?}：期望 {want:?}，实得 {got:?}"),
        }
    }
    // 键问不出 ⇒ 原样交回（不走表）。
    let os_unknown = Refusal::OsUnknown { why: "x".into() };
    assert_eq!(
        choose(Backend, Remote, Err(os_unknown.clone())).unwrap_err(),
        os_unknown
    );
}

/// B4b：每一形拒绝都说得出「哪台 · 什么机器」，且 key 在文案表里（取不到时 `copy_text` 回 `〔key〕`）。
#[test]
fn every_refusal_names_the_machine_and_what_it_is() {
    let cases = [
        Refusal::UnsupportedMachine {
            os: "macOS".into(),
            arch: "arm64".into(),
        },
        Refusal::OsUnknown {
            why: "它没有答".into(),
        },
        Refusal::ArchUnknown {
            why: "它没有答".into(),
        },
        Refusal::NotPromisedHere {
            os: "Windows".into(),
        },
        Refusal::NotCarried {
            os: "Linux".into(),
            arch: "x86_64".into(),
        },
    ];
    let mut said = BTreeSet::new();
    for r in &cases {
        let s = r.say("aya");
        assert!(!s.contains('〔'), "{r:?} 的 key 不在文案表里：{s}");
        assert!(s.contains("aya"), "{r:?} 没说是哪台：{s}");
        assert!(!s.contains('{'), "{r:?} 还有没填的占位符：{s}");
        match r {
            Refusal::UnsupportedMachine { os, arch } | Refusal::NotCarried { os, arch } => {
                assert!(s.contains(os.as_str()) && s.contains(arch.as_str()), "{s}")
            }
            Refusal::NotPromisedHere { os } => assert!(s.contains(os.as_str()), "{s}"),
            Refusal::OsUnknown { why } | Refusal::ArchUnknown { why } => {
                assert!(s.contains(why.as_str()), "{s}")
            }
        }
        // `96 §7.1.4b` 的禁词表。
        for banned in [
            "musl",
            "glibc",
            "triple",
            "BUILD_ID",
            "daemon",
            "sidecar",
            "externalBin",
            "include_bytes",
            "uname",
        ] {
            assert!(!s.contains(banned), "{r:?} 说了禁词 {banned}：{s}");
        }
        said.insert(s);
    }
    assert_eq!(said.len(), cases.len(), "五形里有两形说成了同一句");
}

// ═══ 槽 ↔ 键 ════════════════════════════════════════════════════════════════════════════

/// 生产段里 `include_bytes!` 那几处的宏参数（按出现顺序）。
fn include_targets(prod: &str) -> Vec<String> {
    let needle = format!("{}!(", "include_bytes");
    prod.match_indices(&needle)
        .map(|(at, _)| {
            // 宏参数可能带括号（`concat!(env!("OUT_DIR"), "/x")`）⇒ 按括号配对取到收口那一个。
            let rest = &prod[at + needle.len()..];
            let mut depth = 1usize;
            let mut end = rest.len();
            for (i, c) in rest.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            rest[..end].to_string()
        })
        .collect()
}

/// B2：`byte_table.rs` 的每个槽恰挂表 A 的一个键（从 `include_bytes!` 的目标名读：`<类>-<arch>` ⇒ (类, Linux, arch)；
/// 本机原生那一份 ⇒ 这一份产物的 `TARGET` 那一格）。一个 (类, 键) 恰一个槽，**唯一的例外**是本机原生那一槽在
/// Linux 构建上落在 (Backend, Linux, x86_64 或 aarch64)（`96 §7.3` 第一条，没裁），例外名单两向相等。
#[test]
fn every_slot_hangs_on_exactly_one_key_and_one_key_has_one_slot_but_the_one_listed_exception() {
    let src = read("src/bridge/src/byte_table.rs");
    let prod = guard_core::production_code(&src);
    let targets = include_targets(&prod);
    assert_eq!(
        targets.len(),
        5,
        "byte_table.rs 生产段里 `include_bytes!` 应当恰好 5 处（后端 musl 2 · 全景 musl 2 · 本机原生 1）：{targets:?}"
    );
    let native_key = Key::this_machine().expect("这一份产物的 TARGET 不在表 A 的轴上");
    let mut slots: Vec<(Product, Key)> = Vec::new();
    for t in &targets {
        let slot = if t.contains("native-backend/cc-monitor-native") {
            (Product::Backend, native_key)
        } else {
            let name = t
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .trim_end_matches(['"', ')']);
            let (kind, arch) = name
                .split_once('-')
                .unwrap_or_else(|| panic!("认不出这一槽的名字：{t}"));
            let product = match kind {
                "backend" => Product::Backend,
                "panorama" => Product::Panorama,
                other => panic!("认不出这一槽是哪一类字节：{other}（{t}）"),
            };
            let key = key_of("Linux", arch).unwrap_or_else(|r| panic!("{t}：{r:?}"));
            (product, key)
        };
        slots.push(slot);
    }
    let mut seen = BTreeSet::new();
    let mut doubled = BTreeSet::new();
    for (p, key) in &slots {
        if !seen.insert((*p == Product::Backend, *key)) {
            doubled.insert((*p == Product::Backend, *key));
        }
    }
    let expected_doubled: BTreeSet<(bool, Key)> = if native_key.os == Os::Linux {
        [(true, native_key)].into_iter().collect()
    } else {
        BTreeSet::new()
    };
    assert_eq!(
        doubled, expected_doubled,
        "一格两槽的名单变了 —— 唯一许的是本机原生那一槽在 Linux 构建上与 musl 同格（`96 §7.3` 第一条）"
    );
    // 每个槽的键都在表 A 的产线里（槽挂在一个没有产线的键上 = 那一格的字节永远没人选）。
    for (p, key) in &slots {
        assert!(
            LINES.contains(&(*p, *key)),
            "{p:?} {key:?} 有一槽字节，而 `LINES` 说那一格没有产线"
        );
    }
}

/// B1：`src/bridge/src` 生产段里 `include_bytes!` 住哪几个文件 == {`byte_table.rs`（可执行字节）· `acct_iso_deploy.rs`（vendored 的
/// cc-acct-iso 脚本）· `cc_bus_deploy.rs`（cc-bus skill 的文件）}，两向。后两处嵌的是随部署带过去的文本 / 脚本，不是表 A 的字节。
/// **别的文件长出一槽可执行字节 ⇒ 红**：它就是第二个取字节口（题面「今天两条内嵌路径各按一个错的轴选」的复发形）。
#[test]
fn byte_table_is_the_only_home_of_embedded_executables() {
    let root = repo_root().join("src/bridge/src");
    let mut homes = BTreeSet::new();
    let mut scanned = 0usize;
    for (p, text) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        let prod = guard_core::production_code(&text);
        if !include_targets(&prod).is_empty() {
            homes.insert(
                p.strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    assert!(scanned > 50, "只扫到 {scanned} 份 .rs —— 扫描口坏了");
    let want: BTreeSet<String> = ["byte_table.rs", "acct_iso_deploy.rs", "cc_bus_deploy.rs"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(homes, want);
    // 正控：一段带 `include_bytes!` 的合成生产代码认得出来。
    let sample = format!("static X: &[u8] = {}!(\"../x\");", "include_bytes");
    assert_eq!(include_targets(&sample), vec!["\"../x\"".to_string()]);
}

/// B3：`LINES`（表 A 里有产线的格子）== `release.yml` 真编得出字节的那几格（**异源**：读发版流水线）。
///
/// 读法：`Cross-compile backend|panorama for both musl targets` 那两步里的 `--target <arch>-unknown-linux-musl` ⇒ (类, Linux, arch)；
/// `runs-on: windows-latest` 那个 job 里名为 `Stage native <类> for self-extract` 的步 ⇒ (类, Windows, x86_64)。
#[test]
fn lines_are_exactly_what_the_release_pipeline_builds() {
    let yml = read(".github/workflows/release.yml");
    let mut built: BTreeSet<(bool, Key)> = BTreeSet::new();
    // 按 job 切：顶层 job 以两格缩进的 `<名>:` 起头。
    let mut jobs: Vec<String> = Vec::new();
    let mut in_jobs = false;
    for line in yml.lines() {
        if line.starts_with("jobs:") {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        let is_job_head = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if is_job_head || jobs.is_empty() {
            jobs.push(String::new());
        }
        let last = jobs.last_mut().expect("刚推过");
        last.push_str(line);
        last.push('\n');
    }
    assert!(jobs.len() >= 3, "release.yml 只切出 {} 个 job", jobs.len());
    for job in &jobs {
        let windows = job.contains("runs-on: windows-latest");
        for step in job.split("- name:").skip(1) {
            let name = step.lines().next().unwrap_or_default().trim();
            for (label, backend) in [("backend", true), ("panorama", false)] {
                if name == format!("Cross-compile {label} for both musl targets") {
                    for l in step.lines() {
                        if let Some(rest) = l.split("--target ").nth(1) {
                            let triple = rest.split_whitespace().next().unwrap_or_default();
                            let arch = triple.split('-').next().unwrap_or_default();
                            built.insert((backend, key_of("Linux", arch).expect(triple)));
                        }
                    }
                }
                if windows && name == format!("Stage native {label} for self-extract") {
                    built.insert((backend, k(Os::Windows, Arch::X86_64)));
                }
            }
        }
    }
    let lines: BTreeSet<(bool, Key)> = LINES
        .iter()
        .map(|(p, key)| (*p == Product::Backend, *key))
        .collect();
    assert_eq!(
        lines, built,
        "`byte_table::LINES` 与 `release.yml` 的产线对不上（左：表；右：流水线）"
    );
}

/// B6：旧的三个取字节口与那道 OS 闸在生产段零命中（带正控）。
#[test]
fn the_old_byte_doors_and_the_linux_gate_are_gone() {
    let host = guard_core::production_code(&read("src/bridge/src/local_backend_host.rs"));
    let body_at = host
        .find("pub fn start_local_backend()")
        .expect("找不到 start_local_backend");
    let body = &host[body_at..];
    let body = &body[..body.find("\n}\n").unwrap_or(body.len())];
    let gate = format!("cfg!(target_os = \"{}\")", "linux");
    assert!(
        !body.contains(&gate),
        "start_local_backend 里又长回了那道 OS 闸 —— 本机也按 (OS, arch) 从表里取"
    );
    assert!(
        body.contains("byte_table::choose("),
        "start_local_backend 不经 `byte_table::choose` 取字节了"
    );
    let root = repo_root().join("src/bridge/src");
    let doors = [
        format!("fn {}_binary(", "backend"),
        format!("fn native_embedded_{}(", "backend"),
        format!("fn probe_remote_{}(", "arch"),
    ];
    for (p, text) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let prod = guard_core::production_code(&text);
        for d in &doors {
            assert!(!prod.contains(d.as_str()), "{p:?} 里又有了 `{d}`");
        }
    }
    // 正控：闸的写法认得出来。
    assert!(format!("if {gate} {{").contains(&gate));
}

/// 表的行为：**musl 那几份字节只会落在 Linux 格**；非 Linux 格要么没有字节，要么恰是这一份产物按 `TARGET` 内嵌的那份
/// （它就是给这台机器编的）。补审阻塞 C1（Windows 上释放一个 Linux ELF 再报「已起」）在表里没有写法可以复发。
#[test]
fn musl_bytes_only_ever_land_on_linux_cells() {
    let native = Key::this_machine().ok();
    for os in [Os::Linux, Os::Windows, Os::Mac] {
        for arch in [Arch::X86_64, Arch::Aarch64] {
            let key = k(os, arch);
            for product in [Product::Backend, Product::Panorama] {
                if os != Os::Linux && Some(key) != native {
                    assert!(
                        pick(product, key).is_none(),
                        "{product:?} {key:?}：非 Linux、也不是这一份产物自己那一格，却给出了字节"
                    );
                }
            }
        }
    }
    // 源码那一侧：musl 槽只在 `Os::Linux` 那两条臂里被取。
    let prod = guard_core::production_code(&read("src/bridge/src/byte_table.rs"));
    let at = prod.find("pub(crate) fn pick(").expect("找不到 pick");
    let body = &prod[at..];
    let body = &body[..body.find("\n}\n").unwrap_or(body.len())];
    for line in body.lines().filter(|l| l.contains("musl_")) {
        assert!(
            line.contains("Os::Linux"),
            "musl 那几槽在一条不是 Linux 的臂里被取了：{line}"
        );
    }
    assert!(
        body.lines().filter(|l| l.contains("musl_")).count() >= 2,
        "pick 里一处 musl 都没找到 —— 切歪了"
    );
}
