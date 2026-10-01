//! 守的要求（用户裁决，逐字）：「产物（cc-monitor 仓）不能引用任何仓外的东西；产物文档只描述现状，不能引用任何开发文档。」
//!
//! 人群：`git ls-files` 的全部文本文件。跳过三类：含 NUL 的二进制（与 git 判二进制同一口径）；
//! 锁文件（`Cargo.lock` · `package-lock.json`：工具写的依赖清单，里面没有人写的文字）；
//! 上游原样拷来的源码（`src/panorama-engine/vendor/`：只照上游改，里面的字是上游写的）。
//! 判定：逐行过检测网，命中集 == ∅。点名仓里一份 `.md` 的（`INVARIANTS §48.1` · `IPC-PROTOCOL §10`）是仓内引用，放过。
//!
//! 本文件不排除：网眼与夹具都在运行期拼，它在人群里照扫。
//! 生成物（`src/frontend/ui/generated/`）也不跳：它随仓发出去；那里的命中来自 Rust 源码注释，源头清了、重新生成就没了。
//!
//! 第二张网（施工说法）：开发过程的阶段名「第 N 波」与施工记录名 `4d-…`。同一个人群，命中集 == ∅；
//! 测试声明行（`it(` · `test(` · `describe(` · `bench(` 打头）不扫 —— 测试名不改。
//!
//! 买不到：换了说法、不落在网眼上的指路；施工路代号、用户裁决编号这类还留在测试名与判据钉着的串里，没进网。

use regex::Regex;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 锁文件：工具生成的依赖清单。
const LOCK_FILES: &[&str] = &["Cargo.lock", "package-lock.json"];

/// 上游原样拷来的源码树（副本只照上游改）。
const UPSTREAM_COPIES: &[&str] = &["src/panorama-engine/vendor/"];

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
    ];
    let alts: Vec<String> = eyes.iter().map(|(a, b)| format!("{a}{b}")).collect();
    Regex::new(&alts.join("|")).expect("检测网拼不成正则")
}

/// 施工说法的网：「第 N 波」· 施工记录名 `4d-…`。拆两段拼 ⇒ 本文件里没有一处完整的网眼。
fn process_net() -> Regex {
    Regex::new(&format!("第[一二三四五六七八九十]{}|4d-{}", "波", "lanes"))
        .expect("施工说法网拼不成正则")
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
        if rel.is_empty()
            || LOCK_FILES.contains(&name)
            || UPSTREAM_COPIES.iter().any(|d| rel.starts_with(d))
        {
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
    ];
    let got: BTreeSet<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| first_outside_pointer(&net, &docs, l).is_some())
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(
        got,
        BTreeSet::from([1, 4, 5]),
        "检测网在合成夹具上抓到的行 ≠ 标定（该抓 1、4、5；该放过 2、3、6）—— 网坏了，全仓那条零命中不作数"
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
