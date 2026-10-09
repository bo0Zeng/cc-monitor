//! 守的要求（用户原话）：「产物（cc-monitor 仓）不能引用任何仓外的东西；产物文档只描述现状，不能引用任何开发文档。」
//!
//! 人群：`git ls-files` 的全部文本文件。跳过两类：含 NUL 的二进制（与 git 判二进制同一口径）；
//! 锁文件（`Cargo.lock` · `package-lock.json`：工具写的依赖清单，里面没有人写的文字）。
//! 判定：逐行过检测网，命中集 == ∅。点名仓里一份 `.md` 的（`INVARIANTS §48.1` · `IPC-PROTOCOL §10`）是仓内引用，放过。
//!
//! 本文件不排除：网眼与夹具都在运行期拼，它在人群里照扫。
//! 生成物（`src/frontend/ui/generated/`）也不跳：它随仓发出去；那里的命中来自 Rust 源码注释，源头清了、重新生成就没了。
//!
//! 第二张网（施工说法）：开发过程的阶段名「第 N 波」与施工记录名 `4d-…`。同一个人群，命中集 == ∅；
//! 测试声明行（`it(` · `test(` · `describe(` · `bench(` 打头）不扫 —— 测试名不改。
//!
//! 第三张网（只报不拦）：仓里没有的那份 `.md` · 工单 / 裁决编号 · 指向已摘掉的代码全景目录。
//! 今天存量太大，它只把命中清单与数打出来（`cargo test -p guard-core --lib unblocked -- --nocapture`），
//! 不让门禁红；它自己的合成夹具照样判网眼有没有坏。
//! 什么时候改成拦：产品代码（`src/`）与 `src/doc` 里这三形清到零的那个提交里，把三眼并进 `net()`，
//! 本条改成命中集 == ∅（测试与 `tests/evidence/` 同拍清，或那时按目录分两步收）。
//!
//! 买不到：换了说法、不落在网眼上的指路（施工路代号不在网眼上；编号网只认几种常见写法）。

use regex::Regex;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 锁文件：工具生成的依赖清单。
const LOCK_FILES: &[&str] = &["Cargo.lock", "package-lock.json"];

/// 仓根（本 crate 住 `src/common/guard-core`）。
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("guard-core 的上三级 = 仓根")
}

/// 检测网。每个网眼拆两段、运行期拼 ⇒ 本文件里没有一处完整的网眼。
/// `doc` 组是「大写打头、带连字符的名字 ＋ §」那一眼：名字是仓里一份 `.md` 的就放过。
/// 「两位数 ＋ §」那一眼不认斜杠后的两位数：带目录的写法各有自己的网眼，
/// 而合成语料（`tests/__fixtures__/scale2-height-records.jsonl`）逐字打码后留下的 `四五六/78 §1` 不是指路。
fn net() -> Regex {
    let eyes = [
        ("调", "研/"),
        ("第四", "波记录"),
        ("_施", "工/"),
        ("真相", "源"),
        ("设计", "篇"),
        ("路线", "图"),
        ("主计", "划"),
        ("MASTER", "PLAN"),
        ("ROAD", "MAP"),
        ("设计", "/[0-9]"),
        ("(^|[^0-9A-Za-z.§/])[0-9]{2} ?", "§"),
        ("(?P<doc>[A-Z][A-Za-z0-9]*-[A-Za-z0-9]+) ", "§"),
        ("记录", " §"),
        ("稿 ?", "§"),
    ];
    let alts: Vec<String> = eyes.iter().map(|(a, b)| format!("{a}{b}")).collect();
    Regex::new(&alts.join("|")).expect("检测网拼不成正则")
}

/// 施工说法的网：「第 N 波」· 施工记录名 `4d-…`。拆两段拼 ⇒ 本文件里没有一处完整的网眼。
fn process_net() -> Regex {
    Regex::new(&format!("第[一二三四五六七八九十]{}|4d-{}", "波", "lanes"))
        .expect("施工说法网拼不成正则")
}

/// 第三张网的三形，各一个名字（报告按它分组）。每个网眼拆两段、运行期拼。
fn unblocked_eyes() -> [(&'static str, Regex); 3] {
    let md = format!(
        r"(?:^|[^A-Za-z0-9_./-])(?P<md>[A-Z][A-Z0-9_]*(?:-[A-Z0-9_]+)*)\.{}\b",
        "md"
    );
    let ticket = [
        (r"\bK-", r"R\d+"),
        (r"\bKR\d+", r"D\d+\b"),
        (r"\bKS", r"\d+\b"),
        (r"\bK\d{1,3}", r"\b"),
        (r"\bR\d{2,3}", r"\b"),
        (r"\bF\d{2,3}", r"[a-z]?\b"),
        (r"\b[A-Z]\d+[a-z]?", r"(?:-[A-Z]?\d+[a-z]?)+\b"),
    ]
    .map(|(a, b)| format!("{a}{b}"))
    .join("|");
    [
        ("仓里没有的 .md", Regex::new(&md).expect("md 网眼")),
        ("工单 / 裁决编号", Regex::new(&ticket).expect("编号网眼")),
        (
            "已摘掉的代码全景目录",
            Regex::new(&format!("{}{}", "code-", "picture/")).expect("全景网眼"),
        ),
    ]
}

/// 别的程序的文件名与本仓生成的文件名：形状像「仓里没有的 `.md`」，但不是指路。
const NOT_A_POINTER_MD: &[&str] = &[
    "CLAUDE",       // Claude Code 读的项目说明
    "AGENTS",       // Codex 读的项目说明
    "RELEASE_BODY", // 发版流水线现生成的 Release 正文
];

/// 功能键名：形状像编号，是键。
const FUNCTION_KEYS: &[&str] = &["F10", "F11", "F12"];

/// 一行里第三张网第 `eye` 形的第一处命中（字节偏移）。
fn first_unblocked_hit(
    eye: usize,
    rx: &Regex,
    in_repo_docs: &BTreeSet<String>,
    line: &str,
) -> Option<usize> {
    rx.captures_iter(line).find_map(|c| {
        let all = c.get(0).expect("整段匹配");
        let spared = match eye {
            0 => c.name("md").is_some_and(|d| {
                in_repo_docs.contains(d.as_str()) || NOT_A_POINTER_MD.contains(&d.as_str())
            }),
            1 => FUNCTION_KEYS.contains(&all.as_str()),
            _ => false,
        };
        (!spared).then(|| all.start())
    })
}

/// 测试声明行：测试名不改，第二张网不扫它。
fn test_declaration() -> Regex {
    Regex::new(r"^\s*(?:it|test|describe|bench)(?:\.\w+)*\(").expect("测试声明正则")
}

/// 一行里第一处越界指路的字节偏移。
fn first_outside_pointer(
    net: &Regex,
    in_repo_docs: &BTreeSet<String>,
    line: &str,
) -> Option<usize> {
    net.captures_iter(line).find_map(|c| {
        let spared = c
            .name("doc")
            .is_some_and(|d| in_repo_docs.contains(d.as_str()));
        (!spared).then(|| c.get(0).expect("整段匹配").start())
    })
}

/// 人群：`(仓根相对路径, 正文)`。跟踪着的符号链接按 git 存的那样取链接目标的文字。
fn tracked_texts(root: &Path) -> Vec<(String, String)> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .expect("跑不动 `git ls-files` —— 人群口径就是它");
    assert!(
        out.status.success(),
        "`git ls-files` 非零退出：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut texts = Vec::new();
    for rel in String::from_utf8_lossy(&out.stdout).split('\0') {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if rel.is_empty() || LOCK_FILES.contains(&name) {
            continue;
        }
        let p = root.join(rel);
        let meta = match std::fs::symlink_metadata(&p) {
            // 工作树里删掉了还没进索引：没有正文可扫。
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            other => other.unwrap_or_else(|e| panic!("跟踪着的 {rel} 读不动：{e}")),
        };
        let bytes = if meta.file_type().is_symlink() {
            let t = std::fs::read_link(&p).unwrap_or_else(|e| panic!("{rel}：{e}"));
            t.to_string_lossy().into_owned().into_bytes()
        } else {
            std::fs::read(&p).unwrap_or_else(|e| panic!("跟踪着的 {rel} 读不动：{e}"))
        };
        if bytes.iter().take(8000).any(|&b| b == 0) {
            continue;
        }
        texts.push((
            rel.to_string(),
            String::from_utf8_lossy(&bytes).into_owned(),
        ));
    }
    texts
}

/// 仓里的文档名：跟踪着的每份 `.md` 的文件名去掉后缀。
fn in_repo_docs(texts: &[(String, String)]) -> BTreeSet<String> {
    texts
        .iter()
        .filter_map(|(rel, _)| rel.rsplit('/').next()?.strip_suffix(".md"))
        .map(str::to_string)
        .collect()
}

/// 命中处前后的一小段（按字符截，不切坏 UTF-8）。
fn snippet(line: &str, at: usize) -> String {
    let before: Vec<char> = line[..at].chars().rev().take(20).collect();
    let before: String = before.into_iter().rev().collect();
    let after: String = line[at..].chars().take(40).collect();
    format!("{before}{after}").trim().to_string()
}

#[test]
fn no_tracked_text_points_at_the_outside_dev_docs() {
    let texts = tracked_texts(&repo());
    let exts: BTreeSet<&str> = texts
        .iter()
        .filter_map(|(rel, _)| rel.rsplit_once('.').map(|(_, e)| e))
        .collect();
    assert!(
        texts.len() > 500 && ["rs", "ts", "md"].iter().all(|e| exts.contains(e)),
        "人群只有 {} 份、后缀 {exts:?} —— `git ls-files` 口径坏了，零命中不作数",
        texts.len()
    );
    let net = net();
    let docs = in_repo_docs(&texts);
    let mut hits = Vec::new();
    let mut files = BTreeSet::new();
    for (rel, text) in &texts {
        for (i, line) in text.lines().enumerate() {
            if let Some(at) = first_outside_pointer(&net, &docs, line) {
                hits.push(format!("{rel}:{}: {}", i + 1, snippet(line, at)));
                files.insert(rel.as_str());
            }
        }
    }
    assert!(
        hits.is_empty(),
        "仓里有 {} 行指向仓外的开发文档（{} 份文件；人群 {} 份）。\
         只是出处的删掉；承载理由的换成就地一句话。前 50 处：\n{}",
        hits.len(),
        files.len(),
        texts.len(),
        hits.iter().take(50).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// 合成夹具：越界的两行必须抓到，仓内引用的两行必须放过（行号集相等）。
#[test]
fn the_net_catches_outside_pointers_and_spares_in_repo_references() {
    let net = net();
    let docs: BTreeSet<String> = ["INVARIANTS", "IPC-PROTOCOL"].map(String::from).into();
    let lines = [
        format!("// 出处〔`{}9 §2.2 ⑫`〕", "9"),   // 1 ✔ 两位数 ＋ §
        "// 见 `INVARIANTS §48.1`".to_string(),    // 2 ✘ 仓内文档
        format!("// 见 `IPC-PROTOCOL {}10`", "§"), // 3 ✘ 带连字符，但名字是仓里的文档
        format!("// 守的要求：B-decouple {}2.1", "§"), // 4 ✔ 带连字符，名字不是仓里的文档
        format!("// 见 `{}/78 {}1`", "设计", "§"), // 5 ✔ 带目录的写法归目录那一眼
        format!("正文 `四五六/78 {}1`。", "§"),    // 6 ✘ 合成语料打码后的形状
        format!("/* 会话头（主窗口稿 {}5.13）：40 高 */", "§"), // 7 ✔ 设计稿的节号
        format!("// 按钮上的字就是结果（额度稿{}5.2）", "§"), // 8 ✔ 稿与 § 之间不空格
        "// 存成草稿再发".to_string(),             // 9 ✘ 不带 §
    ];
    let got: BTreeSet<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| first_outside_pointer(&net, &docs, l).is_some())
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(
        got,
        BTreeSet::from([1, 4, 5, 7, 8]),
        "检测网在合成夹具上抓到的行 ≠ 标定（该抓 1、4、5、7、8；该放过 2、3、6、9）—— 网坏了，全仓那条零命中不作数"
    );
}

#[test]
fn no_tracked_text_names_a_dev_process_wave() {
    let texts = tracked_texts(&repo());
    assert!(
        texts.len() > 500,
        "人群只有 {} 份 —— `git ls-files` 口径坏了，零命中不作数",
        texts.len()
    );
    let (net, decl) = (process_net(), test_declaration());
    let mut hits = Vec::new();
    for (rel, text) in &texts {
        for (i, line) in text.lines().enumerate() {
            if decl.is_match(line) {
                continue;
            }
            if let Some(m) = net.find(line) {
                hits.push(format!("{rel}:{}: {}", i + 1, snippet(line, m.start())));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "仓里有 {} 行写着开发过程的阶段名或施工记录名：只写现状，删掉。前 50 处：\n{}",
        hits.len(),
        hits.iter().take(50).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// 合成夹具：注释与报文里的阶段名必须抓到，测试声明行与不相干的「波」必须放过（行号集相等）。
#[test]
fn the_process_net_catches_wave_names_and_spares_test_declarations() {
    let (net, decl) = (process_net(), test_declaration());
    let lines = [
        format!("// 〔C4d · 第四{}〕", "波"), // 1 ✔
        format!("/// 子命令 ＋1（2026-09-26，第五{} W5-ALIAS 合并）", "波"), // 2 ✔
        format!("describe(\"〔第四{} ST2〕指路\", () => {{", "波"), // 3 ✘ 测试名不改
        "// 波形 · 第 4 波段 · 第四版".to_string(), // 4 ✘
        format!("//! 要求住址：`4d-{}.md ### P1` 第 2 件", "lanes"), // 5 ✔
    ];
    let got: BTreeSet<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !decl.is_match(l) && net.is_match(l))
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(
        got,
        BTreeSet::from([1, 2, 5]),
        "施工说法网在合成夹具上抓到的行 ≠ 标定（该抓 1、2、5；该放过 3、4）—— 网坏了，全仓那条零命中不作数"
    );
}

/// 第三张网：只报不拦。命中清单与数打到标准输出（`-- --nocapture` 看得见），门禁不因它红。
#[test]
fn unblocked_outside_pointers_are_reported_not_blocked() {
    let texts = tracked_texts(&repo());
    assert!(
        texts.len() > 500,
        "人群只有 {} 份 —— `git ls-files` 口径坏了，报出来的数不作数",
        texts.len()
    );
    let docs = in_repo_docs(&texts);
    let area = |rel: &str| -> &'static str {
        if rel.starts_with("src/doc/") {
            "src/doc"
        } else if rel.starts_with("src/") {
            "src（产品）"
        } else if rel.starts_with("tests/evidence/") {
            "tests/evidence"
        } else if rel.starts_with("tests/") {
            "tests"
        } else {
            "其余"
        }
    };
    let mut summary = Vec::new();
    for (i, (name, rx)) in unblocked_eyes().iter().enumerate() {
        let mut lines = Vec::new();
        let mut by_area: std::collections::BTreeMap<&str, usize> = Default::default();
        let mut files = BTreeSet::new();
        for (rel, text) in &texts {
            for (n, line) in text.lines().enumerate() {
                if let Some(at) = first_unblocked_hit(i, rx, &docs, line) {
                    lines.push(format!("{rel}:{}: {}", n + 1, snippet(line, at)));
                    *by_area.entry(area(rel)).or_default() += 1;
                    files.insert(rel.as_str());
                }
            }
        }
        println!(
            "== 第三张网 · {name}：{} 行 / {} 份文件",
            lines.len(),
            files.len()
        );
        for l in &lines {
            println!("{l}");
        }
        summary.push(format!(
            "{name}：{} 行 / {} 份（{}）",
            lines.len(),
            files.len(),
            by_area
                .iter()
                .map(|(a, c)| format!("{a} {c}"))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }
    println!("== 第三张网合计（只报不拦）\n{}", summary.join("\n"));
}

/// 合成夹具：三形各该抓的抓到、该放过的放过（行号集相等）。只报不拦那条的数靠它才作数。
#[test]
fn the_unblocked_net_catches_its_three_shapes_and_spares_the_rest() {
    let eyes = unblocked_eyes();
    let docs: BTreeSet<String> = ["INVARIANTS", "README"].map(String::from).into();
    let cases: [(usize, Vec<String>, BTreeSet<usize>); 3] = [
        (
            0,
            vec![
                format!("// 住址 `{}.md#R{}`", "DECISIONS", "28"), // 1 ✔ 仓里没有
                format!("// 见 `src/doc/{}.md`", "INVARIANTS"),    // 2 ✘ 仓里有
                format!("// 读数在 `{}.md`", "PR-S6"),             // 3 ✔ 带连字符，仓里没有
                format!("// 项目说明 `{}.md`", "CLAUDE"),          // 4 ✘ 别的程序的文件
                format!("// 小写的 `{}.md` 不算", "notes"),        // 5 ✘ 不是大写名
            ],
            BTreeSet::from([1, 3]),
        ),
        (
            1,
            vec![
                format!("// 🔴 `K-R{}`：先让它自己说一遍", "70"), // 1 ✔
                format!("// `KR{}D2` 那条", "120"),               // 2 ✔
                format!("// 用户 `K{}` 逐字", "33"),              // 3 ✔
                format!("// F{}（#39）：可打开文件窗口", "83"),   // 4 ✔
                format!("// U8c-{} 只需改例子", "3"),             // 5 ✔
                format!("| `F{}` | 全屏 |", "11"),                // 6 ✘ 功能键
                format!("// {}-2023-0071 与 SHA256SUMS", "RUSTSEC"), // 7 ✘
                format!("// x86_64 · K8s · {}x", "4K"),           // 8 ✘
                format!("// {} 号的凭据", "KS"),                  // 9 ✘ 没有数字
            ],
            BTreeSet::from([1, 2, 3, 4, 5]),
        ),
        (
            2,
            vec![
                format!("规格住 `{}-picture/doc/agents/claude-code.md`", "code"), // 1 ✔
                "代码全景已经摘掉了".to_string(),                                 // 2 ✘
            ],
            BTreeSet::from([1]),
        ),
    ];
    for (eye, lines, want) in cases {
        let (name, rx) = &eyes[eye];
        let got: BTreeSet<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| first_unblocked_hit(eye, rx, &docs, l).is_some())
            .map(|(i, _)| i + 1)
            .collect();
        assert_eq!(
            got, want,
            "第三张网「{name}」在合成夹具上抓到的行 ≠ 标定 —— 网坏了，报出来的数不作数"
        );
    }
}

/// 量具目录（仓根相对）。
const EVIDENCE_DIR: &str = "tests/evidence/";

/// 第四张网：点名量具的地方。两形，网眼拆两段、运行期拼。
/// - 带目录：`tests/evidence/<文件>`，全仓都扫；
/// - 不带目录：量具目录里的文件互相点名用的短名（大写打头、带连字符、带后缀；前面带半截目录也算），只在量具目录里扫。
fn evidence_pointer_eyes() -> (Regex, Regex) {
    let full = format!(r"{}{}(?P<name>[A-Za-z0-9_.-]+)", "tests/evi", "dence/");
    let bare = format!(
        r"(?:^|[^A-Za-z0-9_.-])(?P<name>[A-Z][A-Za-z0-9]*(?:-[\p{{Han}}A-Za-z0-9]+)+\.(?:py|sh|ps1|mjs|ts|rs|json|tsv|{}))\b",
        "md"
    );
    (
        Regex::new(&full).expect("带目录网眼"),
        Regex::new(&bare).expect("短名网眼"),
    )
}

/// 一行里点名、却不在仓里的量具（名字列表）。`evidence` = 量具目录里跟踪着的文件名；
/// `basenames` = 全仓跟踪着的文件名（短名点到仓里别处的文档也算在）；`in_evidence` = 这一行是否住量具目录。
fn dangling_evidence_names(
    eyes: &(Regex, Regex),
    evidence: &BTreeSet<String>,
    basenames: &BTreeSet<String>,
    in_evidence: bool,
    line: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    for c in eyes.0.captures_iter(line) {
        let name = c["name"].trim_end_matches('.');
        if !name.is_empty() && !evidence.contains(name) {
            out.push(format!("{}{name}", EVIDENCE_DIR));
        }
    }
    if in_evidence {
        for c in eyes.1.captures_iter(line) {
            let name = &c["name"];
            let full = format!("{}{name}", EVIDENCE_DIR);
            if !basenames.contains(name) && !out.contains(&full) {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// 量具挪出仓之后，仓里不许还指着已经不在的量具：点名的都得在（命中集 == ∅）。
#[test]
fn every_named_evidence_file_is_in_the_repo() {
    let texts = tracked_texts(&repo());
    let evidence: BTreeSet<String> = texts
        .iter()
        .filter_map(|(rel, _)| rel.strip_prefix(EVIDENCE_DIR))
        .map(str::to_string)
        .collect();
    assert!(
        texts.len() > 500 && evidence.len() > 10,
        "人群 {} 份、量具 {} 份 —— `git ls-files` 口径坏了，零命中不作数",
        texts.len(),
        evidence.len()
    );
    let basenames: BTreeSet<String> = texts
        .iter()
        .map(|(rel, _)| rel.rsplit('/').next().unwrap_or(rel).to_string())
        .collect();
    let eyes = evidence_pointer_eyes();
    let mut hits = Vec::new();
    for (rel, text) in &texts {
        let in_evidence = rel.starts_with(EVIDENCE_DIR);
        for (i, line) in text.lines().enumerate() {
            for name in dangling_evidence_names(&eyes, &evidence, &basenames, in_evidence, line) {
                hits.push(format!("{rel}:{}: {name}", i + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "仓里有 {} 处点名了不在仓里的量具：只是出处的删掉，承载理由的换成就地一句话。\n{}",
        hits.len(),
        hits.join("\n")
    );
}

/// 合成夹具：点到不在的量具必须抓到，点到在的、目录本身、通配、别处的文档必须放过（行号集相等）。
#[test]
fn the_evidence_net_catches_missing_files_and_spares_present_ones() {
    let eyes = evidence_pointer_eyes();
    let evidence: BTreeSet<String> = ["CP-copy-judges.py", "README.md"].map(String::from).into();
    let basenames: BTreeSet<String> = ["CP-copy-judges.py", "README.md", "IPC-COMMANDS.md"]
        .map(String::from)
        .into();
    let dir = format!("tests/evi{}", "dence/");
    let cases: [(bool, String); 10] = [
        (false, format!("// 量具 `{dir}K-R1-gone.py`")), // 1 ✔ 不在
        (false, format!("// 量具 `{dir}CP-copy-judges.py`。")), // 2 ✘ 在（句末标点不算名字）
        (false, format!("// 本目录 `{dir}` 被排除")),    // 3 ✘ 目录本身
        (false, format!("// `{dir}*.sh` 纳进 shellcheck")), // 4 ✘ 通配
        (true, format!("口径同 `{}-ruler.py`", "K-R9")), // 5 ✔ 量具目录里的短名，不在
        (true, format!("命令住 `{}-readings.md §1`", "S9")), // 6 ✔ 短名点 .md，仓里没有
        (true, format!("见 `{}-COMMANDS.md`", "IPC")),   // 7 ✘ 仓里别处有
        (false, format!("口径同 `{}-ruler.py`", "K-R9")), // 8 ✘ 量具目录外不扫短名
        (true, format!("口径同 `evidence/{}-ruler.py`", "K-R9")), // 9 ✔ 带半截目录的短名
        (true, format!("读数在 `{}-发版读数.md § 1`", "K-R9")), // 10 ✔ 名字里带汉字
    ];
    let got: BTreeSet<usize> = cases
        .iter()
        .enumerate()
        .filter(|(_, (inside, l))| {
            !dangling_evidence_names(&eyes, &evidence, &basenames, *inside, l).is_empty()
        })
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(
        got,
        BTreeSet::from([1, 5, 6, 9, 10]),
        "量具网在合成夹具上抓到的行 ≠ 标定（该抓 1、5、6、9、10）—— 网坏了，全仓那条零命中不作数"
    );
}
