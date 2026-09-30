//! 守的要求（用户裁决，逐字）：「产物（cc-monitor 仓）不能引用任何仓外的东西；产物文档只描述现状，不能引用任何开发文档。」
//!
//! 人群：`git ls-files` 的全部文本文件。跳过两类：含 NUL 的二进制（与 git 判二进制同一口径）；
//! 锁文件（`Cargo.lock` · `package-lock.json`：工具写的依赖清单，里面没有人写的文字）。
//! 判定：逐行过检测网，命中集 == ∅。点名仓里一份 `.md` 的（`INVARIANTS §48.1` · `IPC-PROTOCOL §10`）是仓内引用，放过。
//!
//! 本文件不排除：网眼与夹具都在运行期拼，它在人群里照扫。
//! 生成物（`src/frontend/ui/generated/`）也不跳：它随仓发出去；那里的命中来自 Rust 源码注释，源头清了、重新生成就没了。
//!
//! 买不到：换了说法、不落在网眼上的指路。

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
fn net() -> Regex {
    let eyes = [
        ("调", "研/"),
        ("第四波", "记录"),
        ("_施", "工/"),
        ("真相", "源"),
        ("设计", "篇"),
        ("路线", "图"),
        ("主计", "划"),
        ("MASTER", "PLAN"),
        ("ROAD", "MAP"),
        ("设计", "/[0-9]"),
        ("(^|[^0-9A-Za-z.§])[0-9]{2} ?", "§"),
        ("(?P<doc>[A-Z][A-Za-z0-9]*-[A-Za-z0-9]+) ", "§"),
        ("记录", " §"),
    ];
    let alts: Vec<String> = eyes.iter().map(|(a, b)| format!("{a}{b}")).collect();
    Regex::new(&alts.join("|")).expect("检测网拼不成正则")
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
    ];
    let got: BTreeSet<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| first_outside_pointer(&net, &docs, l).is_some())
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(
        got,
        BTreeSet::from([1, 4]),
        "检测网在合成夹具上抓到的行 ≠ 标定（该抓 1、4；该放过 2、3）—— 网坏了，全仓那条零命中不作数"
    );
}
