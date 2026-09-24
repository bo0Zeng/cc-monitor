//! 〔F7c · 第三波 · 2026-09-24〕**SFTP 那一族收到只剩传输** —— 命令面与写面的恒等登记（`设计/60 §13.4` · `§13.6` 判据 1）。
//!
//! 用户逐字「**保留SFTP. 思考怎么干净**」⇒ SFTP 只剩两个动作：上传、下载（断点续传 · 撤 · 进度）。
//!
//! # 今天的读数，与它为什么还不是「只剩两条」
//!
//! - **窗口经通道说得出的传输操作**恰好两条：`transfer-upload` · `transfer-download`（撤 = 停订，不是命令）。
//! - **写面**：传输核心只写暂存区（`ensure_staging_dir` · `upload_to_staging`；「暂存区之外零写」的行为读数住
//!   `sftp_staging_tests`）。
//! - 池子里那 13 条 `#[tauri::command]` **一条都不属于传输核心**（窗口不走 Tauri IPC、走通道）——
//!   它们今天全是**待收**：最后一个消费者在别人的写区里（老面板归 F7b 删；复制 / 读文本 / 问 home 归 F7a 换）。
//!   本路**不删**它们（删了 = 老面板那几处 `invoke` 当场悬空，红在别人那一侧），而是登记成 [`PENDING`]：
//!   每一格写清等哪一路、消费者住哪，**并要求那个消费者此刻真的在盘上**。
//!   ⇒ 那一路合进来、消费者一走，那一格当场红 ⇒ 删它是一件机械活（名字、住址都在表上）。
//!   这与 `sftp_pool.rs` 里 `is_protected_claude_data_path` 那行转出住址「递减棘轮 ＋ 最后一个消费者」同形。
//!
//! # 三条判据，都是两向相等
//!
//! 1. 池子的 Tauri 命令集合 == [`PENDING`] 里「命令」那几格（传输核心零条 Tauri 命令）。
//! 2. 池子里**碰远端写原语**的函数集合 == 暂存区那两个 ∪ [`PENDING_WRITERS`]（每一个挂在一条待收命令名下）。
//! 3. 窗口经通道说得出的传输操作 == `{transfer-upload, transfer-download}`，而且宿主的分流只认这两条。
//!
//! # 买不到什么
//!
//! - 「待收的消费者此刻在」是**文本命中**（文件里有那根针），判不了那一处是不是真的还会被执行到。
//! - 写原语按调用形认（`.create_dir(` · `.remove_file(` · `.rename(` · `.set_metadata(` · `.remove_dir(` ·
//!   带写意图的 `OpenFlags` · `upload_atomic(`）：换一种写法（经宏、经别的 crate）看不见 —— 与
//!   `remote_write_registry` 同一个洞，那一条按「持有 SFTP 会话的文件」另兜一层。

use std::collections::BTreeSet;

/// 🔴 **待收**：`(名字, 形态, 等哪一路, 消费者住址（仓相对）, 那根针)`。
///
/// **这张表只许变短** —— 变长 = SFTP 又长回一条非传输的命令。
pub(super) const PENDING: &[(&str, &str, &str, &str, &str)] = &[
    // 〔F7c 收尾 09-24〕F7a ＋ F7b 合进来之后这张表从 13 行收到 1 行 —— 删掉的十二条
    //   〔已删：`sftp_realpath` · `sftp_list_dir` · `sftp_stat` · `sftp_cancel_transfer`〔散文墓碑〕 · `sftp_download` ·
    //   `sftp_upload` · `sftp_read_text_for_edit`〔散文墓碑〕 · `sftp_write_text` · `sftp_mkdir` · `sftp_rename` ·
    //   `sftp_delete` · `sftp_chmod`〕连同它们的 Tauri 注册、`commands.ts` 包装、`parity_ledger` 行一起走了。
    //   ⚠ 剩下这一条的消费者**不是代码**：门禁 `f3-copy` 那一格（秤 F3）量的是它的核心 `copy_remote_path`；
    //   窗口的复制已经走后端 `files-copy`（F7a）。删它要连那一格一起退役（`gate.sh` 29 格 → 28），
    //   而那一处不在本路授权里 ⇒ 等主会话。
    (
        "sftp_copy",
        "命令",
        "主会话（门禁 f3-copy 那一格退役）",
        "tests/scripts/gate.sh",
        "run_gate f3-copy",
    ),
];

/// 碰远端写原语、而**不在**传输核心里的函数：`(函数, 它挂在哪条待收命令名下)`。
pub(super) const PENDING_WRITERS: &[(&str, &str)] = &[("copy_remote_path", "sftp_copy")];

/// 传输核心里**唯二**碰远端写原语的函数（都只写暂存区）。
const STAGING_WRITERS: &[&str] = &["ensure_staging_dir", "upload_to_staging"];

/// 远端写原语的调用形（见模块头注「买不到」第二条）。
const WRITE_FORMS: &[&str] = &[
    ".create_dir(",
    ".remove_file(",
    ".rename(",
    ".set_metadata(",
    ".remove_dir(",
    "OpenFlags::CREATE",
    "OpenFlags::WRITE",
    "OpenFlags::EXCLUDE",
    "upload_atomic(",
];

fn pool_production() -> String {
    let raw = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/bridge/src/sftp_pool.rs"),
    )
    .expect("读 sftp_pool.rs");
    guard_core::production_code(&raw)
}

/// 顶格函数名（`fn` / `pub fn` / `pub async fn` / `async fn`，不含 `impl` 里缩进的方法）⇒ 它那一块。
fn top_level_fns(prod: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in prod.lines() {
        let head = line
            .strip_prefix("pub async fn ")
            .or_else(|| line.strip_prefix("pub fn "))
            .or_else(|| line.strip_prefix("async fn "))
            .or_else(|| line.strip_prefix("fn "));
        if let Some(rest) = head {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            out.push((name, String::new()));
            continue;
        }
        if let Some((_, body)) = out.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    out
}

/// 带 `#[tauri::command]` 的函数名。
fn tauri_commands(prod: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut armed = false;
    for line in prod.lines() {
        let t = line.trim();
        if t == "#[tauri::command]" {
            armed = true;
            continue;
        }
        if armed {
            if let Some(rest) = t
                .strip_prefix("pub async fn ")
                .or_else(|| t.strip_prefix("pub fn "))
            {
                out.insert(
                    rest.chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect(),
                );
            }
            armed = false;
        }
    }
    out
}

/// 🔴 判据 1：池子的 Tauri 命令 == 待收表里的命令（两向）—— 传输核心零条 Tauri 命令。
#[test]
fn every_pool_tauri_command_is_pending_and_the_transfer_core_has_none() {
    let prod = pool_production();
    let got = tauri_commands(&prod);
    // 〔F7c 收尾 09-24〕地板 2 → 1：收尾之后池子只剩 `sftp_copy` 一条（下面那条相等才是判据，这个数只守抽取器）。
    //   量具自检另喂一段合成语料，证明抽取器认得出不止一条。
    assert!(!got.is_empty(), "一条 Tauri 命令都没抠到 —— 抽取器坏了");
    assert_eq!(
        tauri_commands(
            "#[tauri::command]\npub async fn a() {}\n#[tauri::command]\npub fn b() {}\nfn c() {}\n"
        )
        .len(),
        2,
        "抽取器认不出合成语料里那两条"
    );
    let want: BTreeSet<String> = PENDING
        .iter()
        .filter(|(_, k, ..)| *k == "命令")
        .map(|(n, ..)| (*n).to_string())
        .collect();
    assert_eq!(
        got,
        want,
        "池子的 Tauri 命令与待收表对不上。\n  盘上有、表里没有（🔴 SFTP 又长出一条命令）：{:?}\n  \
         表里有、盘上没有（那一条删掉了 ⇒ 同拍摘这一行，表只许变短）：{:?}",
        got.difference(&want).collect::<Vec<_>>(),
        want.difference(&got).collect::<Vec<_>>()
    );
}

/// 🔴 判据 1 的另一半：**待收的每一格，消费者此刻真的在**（没有「留着用不上的豁免」）。
#[test]
fn every_pending_entry_still_has_its_named_consumer_on_disk() {
    let root = crate::guard_support::repo_root();
    for (name, _, who, file, needle) in PENDING {
        let text = std::fs::read_to_string(root.join(file)).unwrap_or_default();
        assert!(
            text.contains(needle),
            "待收 `{name}` 的消费者 `{file}` 里已经没有 `{needle}` 了（等的是 {who}）。\n\
             ⇒ 它没人用了：删掉池子里那条（连 `lib.rs` 注册那一行、`PENDING_WRITERS` 里挂在它名下的写函数），\n\
             再摘掉这一行。别把针改宽让它继续绿 —— 那就是留了一个用不上的豁免。"
        );
    }
}

/// 🔴 判据 2：池子里碰远端写原语的函数 == 暂存区那两个 ∪ 待收写函数（两向）；
/// 每个待收写函数都挂在一条**还在表上**的待收命令名下。
#[test]
fn the_only_remote_writers_are_the_staging_pair_and_the_pending_ones() {
    let prod = pool_production();
    let got: BTreeSet<String> = top_level_fns(&prod)
        .into_iter()
        .filter(|(_, body)| WRITE_FORMS.iter().any(|f| body.contains(f)))
        .map(|(n, _)| n)
        .collect();
    let want: BTreeSet<String> = STAGING_WRITERS
        .iter()
        .map(|s| (*s).to_string())
        .chain(PENDING_WRITERS.iter().map(|(f, _)| (*f).to_string()))
        .collect();
    assert_eq!(
        got,
        want,
        "池子里碰远端写原语的函数与登记对不上。\n  盘上有、登记没有（🔴 暂存区之外又长出一处写）：{:?}\n  \
         登记有、盘上没有：{:?}",
        got.difference(&want).collect::<Vec<_>>(),
        want.difference(&got).collect::<Vec<_>>()
    );
    let pending: BTreeSet<&str> = PENDING.iter().map(|(n, ..)| *n).collect();
    for (f, owner) in PENDING_WRITERS {
        assert!(
            pending.contains(owner),
            "写函数 `{f}` 挂在 `{owner}` 名下，而 `{owner}` 已经不在待收表上 —— 它该跟着一起走"
        );
    }
    // 量具自检：一段合成的「暂存区外写」认得出来。
    let fake = "pub async fn sneaky(s: &S) {\n    s.rename(a, b).await;\n}\n";
    assert_eq!(top_level_fns(fake)[0].0, "sneaky");
    assert!(WRITE_FORMS
        .iter()
        .any(|f| top_level_fns(fake)[0].1.contains(f)));
}

/// 🔴 判据 3：窗口经通道说得出的传输操作恰好两条；宿主分流只认这两条（撤不是命令）。
#[test]
fn the_transfer_ops_are_exactly_upload_and_download() {
    let got: BTreeSet<&str> = super::TRANSFER_OPS.iter().copied().collect();
    let want: BTreeSet<&str> = ["transfer-upload", "transfer-download"]
        .into_iter()
        .collect();
    assert_eq!(got, want);
    for op in ["transfer-upload", "transfer-download"] {
        assert!(super::is_transfer_op(op));
    }
    for not in [
        "transfer-cancel",
        "transfer-copy",
        "files-copy",
        "files-ls",
        "sftp_upload",
        "",
    ] {
        assert!(
            !super::is_transfer_op(not),
            "`{not}` 被当成了传输台的命令 —— 撤是停订，不是命令；其余都归后端"
        );
    }
}
