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
//! 期望值一律手写自上面三处原文（**不**取自被测的 `LINES`），两侧同源会恒真。
//! 〔P1〕表 B（承诺面）是判定、住后端：它的判据（判序 · 账本 == 代码）随 `promised` 搬进 `tests/backend/control/deploy_plan_tests.rs`。

use super::*;
use deploy_contract::{key_from_uname, key_of, Route};
use std::collections::BTreeSet;

fn k(os: Os, arch: Arch) -> Key {
    Key { os, arch }
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
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

/// 〔WIN1 · 第四波 4D · RT1 F4〕Windows 那台用本地代码页（GBK）回话 ⇒ 界面上**不照抄乱码**，拒绝照旧。
///
/// 要求住址：`设计/96 §7.1.4` 第 2 条逐字「**拒绝是一个会到达用户的结论，不是一行 `debug` 日志**；文案照 `01 §6.9`」
/// 与第 4 条「**OS 问不出 ＝ 拒绝，不是回落**」；读数出处 `第四波记录/RT1.md §8` F4 逐字
/// 「Windows 远端 `uname` 的回话在 toast / 日志里是**乱码**：PowerShell 按控制台代码页（GBK）吐错误，我们按 UTF-8 解」。
/// 异源：语料是**手写的 GBK 字节**（「无法将“uname”」），走与后端 `dial/uses.rs` 同一种有损解（`from_utf8_lossy`）；
/// 期望是「整句里零个 U+FFFD」＋「说得出不是 UTF-8」，不从被测常量里抠。正控：UTF-8 的回话照旧原样带出。
#[test]
fn an_answer_that_is_not_utf8_is_not_parroted_as_mojibake() {
    // 「uname : 无法将“uname”项识别为…」的 GBK 字节（前半段）。
    let gbk: &[u8] = b"uname : \xce\xde\xb7\xa8\xbd\xab\xa1\xb0uname\xa1\xb1";
    let lossy = String::from_utf8_lossy(gbk).into_owned();
    assert!(
        lossy.contains('\u{FFFD}'),
        "语料自检：GBK 字节按 UTF-8 解应当留下 U+FFFD"
    );
    for (exit, out, err) in [(Some(1), "", lossy.as_str()), (Some(0), lossy.as_str(), "")] {
        let r = key_from_uname(exit, out, err);
        let Err(refusal @ Refusal::OsUnknown { .. }) = &r else {
            panic!("不是 UTF-8 的回话照样该是「问不出 OS」：{r:?}");
        };
        for product in [Product::Backend, Product::Panorama] {
            let said = refusal.say(product, "vmself");
            assert!(!said.contains('\u{FFFD}'), "界面那句里照抄了乱码：{said}");
            assert!(
                said.contains("不是 UTF-8"),
                "那句话没说出「不是 UTF-8」：{said}"
            );
            assert!(said.contains("vmself"), "{said}");
        }
    }
    // 正控：UTF-8 的回话（英文 Windows / 真 POSIX 的报错）照旧原样带出 —— 本件不许把它们也吞掉。
    match key_from_uname(Some(1), "", "'uname' is not recognized") {
        Err(Refusal::OsUnknown { why }) => {
            assert!(why.contains("'uname' is not recognized"), "{why}");
            assert!(!why.contains("不是 UTF-8"), "{why}");
        }
        other => panic!("{other:?}"),
    }
}

// ═══ 拒绝点：六键 × 两路 × 两类字节的全表 ═══════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
enum Want {
    /// 给（这一版带着就 `Ok`，没带就 `NotCarried` —— 两者按 `pick` 现打分）。
    Give,
    Unsupported,
}

/// B4：`choose` 的判序（monitor 这一侧只剩查表 · 事实）：键问不出 ⇒ 原样交回；不在表 A 有产线的行里 ⇒ `unsupported_machine`；
/// 带没带 ⇒ 按 `pick`。期望手写自 `96 §7.1.1b` 表 A 的「有没有产线」一栏（〔P1〕表 B 那一维随判定进了后端）。
#[test]
fn choose_answers_every_cell_of_table_a() {
    use Arch::*;
    use Os::*;
    use Product::*;
    use Want::*;
    let expect: &[(Product, Key, Want)] = &[
        (Backend, k(Windows, X86_64), Give),
        // 用户 09-18「不含 arm64」⇒ 无产线。
        (Backend, k(Windows, Aarch64), Unsupported),
        (Backend, k(Linux, X86_64), Give),
        (Backend, k(Linux, Aarch64), Give),
        (Backend, k(Mac, X86_64), Unsupported),
        (Backend, k(Mac, Aarch64), Unsupported),
        // 全景：Linux 两格的 musl 产线 ＋ 〔RM1f〕Windows x86_64 的原生产线。
        (Panorama, k(Windows, X86_64), Give),
        (Panorama, k(Windows, Aarch64), Unsupported),
        (Panorama, k(Linux, X86_64), Give),
        (Panorama, k(Linux, Aarch64), Give),
        (Panorama, k(Mac, X86_64), Unsupported),
        (Panorama, k(Mac, Aarch64), Unsupported),
    ];
    for &(product, key, want) in expect {
        let got = choose(product, Ok(key));
        match (want, &got) {
            (Give, Ok(_)) => assert!(pick(product, key).is_some()),
            (Give, Err(Refusal::NotCarried { .. })) => assert!(
                pick(product, key).is_none(),
                "{product:?} {key:?}：表里有字节却说没带"
            ),
            (Unsupported, Err(Refusal::UnsupportedMachine { .. })) => {}
            _ => panic!("{product:?} {key:?}：期望 {want:?}，实得 {got:?}"),
        }
    }
    // 键问不出 ⇒ 原样交回（不走表）。
    let os_unknown = Refusal::OsUnknown { why: "x".into() };
    assert_eq!(
        choose(Backend, Err(os_unknown.clone())).unwrap_err(),
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
            arch: "x86_64".into(),
            route: Route::Remote,
        },
        // 〔V132〕本机那一句：同一形、另一句话（远端那句「只在本机用得上」对本机是假话）。
        Refusal::NotPromisedHere {
            os: "Linux".into(),
            arch: "arm64".into(),
            route: Route::Local,
        },
        Refusal::NotCarried {
            os: "Linux".into(),
            arch: "x86_64".into(),
        },
    ];
    let mut said = BTreeSet::new();
    // 〔TL1 · 4C〕两件产物各一组话：全景推字节也走 `choose` 之后，同一种拒绝对两件产物说两句（后果不同）。
    for (r, product) in cases
        .iter()
        .flat_map(|r| [(r, Product::Backend), (r, Product::Panorama)])
    {
        let s = r.say(product, "devbox");
        let noun = match product {
            Product::Backend => "后端",
            Product::Panorama => "代码全景组件",
        };
        assert!(s.contains(noun), "{r:?} / {product:?} 没说是哪件东西：{s}");
        if product == Product::Panorama {
            assert!(!s.contains("后端"), "全景那一句说成了后端：{s}");
        }
        assert!(!s.contains('〔'), "{r:?} 的 key 不在文案表里：{s}");
        assert!(s.contains("devbox"), "{r:?} 没说是哪台：{s}");
        assert!(!s.contains('{'), "{r:?} 还有没填的占位符：{s}");
        match r {
            Refusal::UnsupportedMachine { os, arch } | Refusal::NotCarried { os, arch } => {
                assert!(s.contains(os.as_str()) && s.contains(arch.as_str()), "{s}")
            }
            Refusal::NotPromisedHere { os, arch, route } => {
                assert!(s.contains(os.as_str()), "{s}");
                if *route == Route::Local {
                    assert!(s.contains(arch.as_str()), "本机那句要说出是哪种架构：{s}");
                }
            }
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
        // 〔V161〕每一形说的是**自己那一句** —— 按文案键比（`设计/91 §6`），不再按 `96 §7.1.4b` 那张表各取一个独有词
        //   （用户 09-29「所有文案…不能把开发过程混进去」之后那几句不再说「今天 / 承诺 / 没有验过」）。
        //   只比「五句互不相同」挡不住「一形借了另一形的句子、填了不同的参数」（死值验 K6 首刀没砍中，补这一向）。
        let family = match product {
            Product::Backend => "deploy",
            Product::Panorama => "panorama",
        };
        let (form, os, arch, why) = match r {
            Refusal::UnsupportedMachine { os, arch } => {
                ("unsupportedMachine", os.as_str(), arch.as_str(), "")
            }
            Refusal::OsUnknown { why } => ("osUnknown", "", "", why.as_str()),
            Refusal::ArchUnknown { why } => ("archUnknown", "", "", why.as_str()),
            Refusal::NotPromisedHere {
                os,
                arch,
                route: Route::Remote,
            } => ("notPromisedHere", os.as_str(), arch.as_str(), ""),
            // 〔V132〕本机那一形另有一句。
            Refusal::NotPromisedHere {
                os,
                arch,
                route: Route::Local,
            } => ("notPromisedLocal", os.as_str(), arch.as_str(), ""),
            Refusal::NotCarried { os, arch } => ("notCarried", os.as_str(), arch.as_str(), ""),
        };
        let own = crate::copy_table::copy_text(
            &format!("{family}.refused.{form}"),
            &[("machine", "devbox"), ("os", os), ("arch", arch), ("why", why)],
        );
        assert_eq!(s, own, "{r:?} / {product:?} 说的不是自己那一句");
        said.insert(s);
    }
    assert_eq!(
        said.len(),
        cases.len() * 2,
        "五形（不承诺那一形分本机 / 远端两句）× 两件产物里有两句说成了同一句"
    );
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
/// 本机原生那两份 ⇒ 这一份产物的 `TARGET` 那一格）。一个 (类, 键) 恰一个槽，**仅有的例外**是本机原生那两槽（后端 ·
/// 〔RM1f〕全景）在 Linux 构建上与 musl 同格（`96 §7.3` 第一条，没裁），例外名单两向相等。
#[test]
fn every_slot_hangs_on_exactly_one_key_and_one_key_has_one_slot_but_the_one_listed_exception() {
    let src = read("src/frontend/shell/src/byte_table.rs");
    let prod = guard_core::production_code(&src);
    let targets = include_targets(&prod);
    assert_eq!(
        targets.len(),
        6,
        "byte_table.rs 生产段里 `include_bytes!` 应当恰好 6 处（后端 musl 2 · 全景 musl 2 · 本机原生 2：后端 ＋ 〔RM1f〕全景）：{targets:?}"
    );
    let native_key = Key::this_machine().expect("这一份产物的 TARGET 不在表 A 的轴上");
    let mut slots: Vec<(Product, Key)> = Vec::new();
    for t in &targets {
        let slot = if t.contains("native-backend/cc-monitor-native") {
            (Product::Backend, native_key)
        } else if t.contains("native-backend/cc-monitor-panorama") {
            (Product::Panorama, native_key)
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
        [(true, native_key), (false, native_key)]
            .into_iter()
            .collect()
    } else {
        BTreeSet::new()
    };
    assert_eq!(
        doubled, expected_doubled,
        "一格两槽的名单变了 —— 只许本机原生那两槽在 Linux 构建上与 musl 同格（`96 §7.3` 第一条）"
    );
    // 每个槽的键都在表 A 的产线里（槽挂在一个没有产线的键上 = 那一格的字节永远没人选）。
    for (p, key) in &slots {
        assert!(
            LINES.contains(&(*p, *key)),
            "{p:?} {key:?} 有一槽字节，而 `LINES` 说那一格没有产线"
        );
    }
}

/// B1：`src/frontend/shell/src` 生产段里 `include_bytes!` 住哪几个文件 == {`byte_table.rs`（可执行字节）}，两向。
/// 〔MIG-3a · 子步 3〕`cc_bus_deploy.rs`（cc-bus skill 的文件）出列：装 cc-bus 进了本机后端，字节随它住 `src/backend/assets/cc_bus_install.rs`。
/// **别的文件长出一槽可执行字节 ⇒ 红**：它就是第二个取字节口（题面「今天两条内嵌路径各按一个错的轴选」的复发形）。
#[test]
fn byte_table_is_the_only_home_of_embedded_executables() {
    let root = repo_root().join("src/frontend/shell/src");
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
    // 〔MIG-3a · 09-28 预裁〕`acct_iso_deploy.rs`（vendored 脚本）出去了：账号库改由后端自己建，那份 vendored 脚本整棵删了。
    let want: BTreeSet<String> = ["byte_table.rs"].into_iter().map(String::from).collect();
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
    let host = guard_core::production_code(&read("src/frontend/shell/src/local_backend_host.rs"));
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
    let root = repo_root().join("src/frontend/shell/src");
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
    let prod = guard_core::production_code(&read("src/frontend/shell/src/byte_table.rs"));
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

// 〔V132 · TL2 · P1〕承诺面「账本 == 代码」那条随 `promised` 搬进后端：`deploy_plan_tests::the_promise_face_in_the_ledger_equals_the_code`。
