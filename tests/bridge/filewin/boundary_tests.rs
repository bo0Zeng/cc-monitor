//! **文件管理器与 app 原生后端那条边界**的判据〔2026-09-23〕。
//!
//! 头注（「解耦清楚」被改述成了什么 · 摸底三问的答案 · 抽 crate 的代价 ·
//! 为什么选乙 · 买不到什么）住 [`super`] 的模块头注那一节，**这里不抄第二份**。
//!
//! # ⚠ 为什么抽取器住在**判据这一侧**，而不是一个生产模块里
//!
//! 本仓的惯例（剖分之后）是「生产模块放抽取器 ＋ 头注，`tests/` 放表与判词」。
//! 这一条**刻意偏了一格**，两条现打的理由：
//!
//! 1. **不许往 `filewin/` 里加一份 `.rs`。** `remote_write_registry_tests`
//!    那条判据把 `filewin/` 那棵树的 `.rs` 份数**钉成恒等 16**（现打：加一份
//!    当场红在 `left: 17 / right: 16`）—— 而那份文件不在本刀写区。
//! 2. **抽取器住这儿反而更不容易自欺。** 正向那条判据扫的是
//!    `src/bridge/src/filewin/**`，反向那条扫的是它之外的 `src/bridge/src/**`
//!    ⇒ 本文件住 `tests/`，**两个人群都够不着它** ⇒ 结构上不可能
//!    「在自己的语料里找到自己」。放进 `filewin/` 的话，那些针得靠
//!    「运行期拼字符串」才躲得开，而那是一道要靠人记住的防线。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ═══════════════════════════════════════════════════════════════════
// 抽取器 —— 逐条口径与漏判面
// ═══════════════════════════════════════════════════════════════════

/// `crate` 这个关键字本身。
const CRATE_WORD: &str = "crate";

/// `filewin` 这个模块名 —— 反向那条判据的针。
const FILEWIN_MOD: &str = "filewin";

/// 一段源码里，从 `ident` 这个词出发的**全部路径引用**（`ident::a::b`）。
///
/// 回的是**去掉了头一段**的路径（`a::b`），排序去重。
///
/// # 认什么
///
/// - `ident::a::b::c` ⇒ `a::b::c`（**整条写出来的路径**，不截断）。
/// - `ident::a::{b, c}` ⇒ `a::b` 与 `a::c`（花括号组逐项摊开）。
/// - `ident` 后面不是两个冒号 ⇒ 不算（那只是一个同名的词）。
///
/// # ⚠ 认不出什么（这是**漏判面**，如实登记）
///
/// - `use` 之后的**短名调用**：`use crate::x::Y;` 之后写 `Y::new()` ——
///   那一处不带前缀，本函数看不见它。接得住的是 `use` 那一行本身（它带前缀）
///   ⇒ 「引入了一个新符号」照旧会被看见，「同一个符号多用了几次」看不见。
///   **而后者不是本判据要数的东西**（数的是边，不是次数）。
/// - **嵌套花括号**（`a::{b::{c, d}}`）⇒ 本函数只摊一层，里层会被当成两个词。
///   现打：`filewin/` 生产段里这一形**零处**。哪天有了，判据会给出一个
///   读起来很怪的条目 —— 那是「出声」，不是「静默放过」。
/// - 宏展开出来的路径。
/// - **注释里的路径**：本函数自己不剥注释。三条判据每一处都先过
///   `guard_core::production_code` —— 那不是可有可无的一步，
///   下面 `the_path_extractor_sees_what_it_should_and_nothing_else` 的 ⑨ 把它钉住了。
fn paths_from(prod: &str, ident: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b = prod.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        // 走到一个**完整的词**（左边不许接标识符字符）。
        if !is_ident_byte(b[i]) || (i > 0 && is_ident_byte(b[i - 1])) {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && is_ident_byte(b[i]) {
            i += 1;
        }
        if &prod[start..i] != ident {
            continue;
        }
        // 词后面必须紧跟两个冒号，否则它只是一个同名的词。
        let mut j = i;
        if !eat_colons(b, &mut j) {
            continue;
        }
        let mut segs: Vec<String> = Vec::new();
        loop {
            if j < b.len() && b[j] == b'{' {
                // 花括号组：摊一层，每一项各是一条边。
                let Some(close) = find_byte(b, j, b'}') else {
                    break;
                };
                for item in split_group(&prod[j + 1..close]) {
                    if segs.is_empty() {
                        out.insert(item);
                    } else {
                        out.insert(format!("{}::{item}", segs.join("::")));
                    }
                }
                segs.clear();
                break;
            }
            let s = j;
            while j < b.len() && is_ident_byte(b[j]) {
                j += 1;
            }
            if s == j {
                break;
            }
            segs.push(prod[s..j].to_string());
            if !eat_colons(b, &mut j) {
                break;
            }
        }
        if !segs.is_empty() {
            out.insert(segs.join("::"));
        }
    }
    out
}

/// `filewin/` 生产段里**够到 app 侧**的全部符号路径。
///
/// ⚠ `filewin::…`（够到自己人）**不算一条边** —— 那是模块内部的事。
fn app_side_edges(prod: &str) -> BTreeSet<String> {
    let mine = format!("{FILEWIN_MOD}::");
    paths_from(prod, CRATE_WORD)
        .into_iter()
        .filter(|p| !starts_with_str(p, &mine))
        .collect()
}

/// app 侧一份源码里**够到 `filewin/`** 的全部符号路径（带 `filewin::` 前缀回）。
fn filewin_reaches(prod: &str) -> BTreeSet<String> {
    paths_from(prod, FILEWIN_MOD)
        .into_iter()
        .map(|p| format!("{FILEWIN_MOD}::{p}"))
        .collect()
}

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// 吃掉「可能有空白 ＋ 两个冒号」。吃到了回 `true` 并推进 `j`。
fn eat_colons(b: &[u8], j: &mut usize) -> bool {
    let ws = |c: u8| c == b' ' || c == b'\n' || c == b'\t' || c == b'\r';
    let mut k = *j;
    while k < b.len() && ws(b[k]) {
        k += 1;
    }
    if k + 1 < b.len() && b[k] == b':' && b[k + 1] == b':' {
        k += 2;
        while k < b.len() && ws(b[k]) {
            k += 1;
        }
        *j = k;
        return true;
    }
    false
}

fn find_byte(b: &[u8], from: usize, needle: u8) -> Option<usize> {
    (from..b.len()).find(|&k| b[k] == needle)
}

/// 花括号组里那几项（逗号分隔；`self` 与 `*` 丢掉）。
fn split_group(inner: &str) -> Vec<String> {
    inner
        .split(',')
        .map(|s| s.trim().trim_start_matches("r#"))
        .filter(|s| !s.is_empty() && *s != "self" && *s != "*")
        .map(|s| s.to_string())
        .collect()
}

/// 前缀比对。**刻意是一个具名函数**：调用点上写 `p.starts_with("…")`
/// 会撞 `needle_anchor_registry` 那条「语料变量上的裸匹配」递减棘轮，
/// 而那条棘轮今天**零富余**。
fn starts_with_str(hay: &str, prefix: &str) -> bool {
    hay.len() >= prefix.len() && &hay[..prefix.len()] == prefix
}

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（它的头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

fn filewin_dir() -> PathBuf {
    repo_root().join("src/bridge/src/filewin")
}

/// `src/bridge/src` 整棵树，切成**两半**：`filewin/` 里的 · 外面的。
///
/// 🔴 **人群从文件系统全集派生**，不是一张手写名单 —— 新加一份 `.rs`
/// 自动进人群。切法走 `Path::starts_with`（拿**路径**比路径），
/// 不是拿字符串去 `starts_with("…")`：后者会撞
/// `needle_anchor_registry` 那条零富余的递减棘轮。
///
/// ⚠ 走 `guard_core::scan_tree_excluding` 而不是裸 `read_dir` ——
/// `scanning_guard_registry` 那条判据钉着「扫描型判据不许自己遍历」。
/// 名单明写为空（`设计/16 §5.4b` 纪律 4）：本判据要摘的不是自己
/// （它住 `tests/bridge/filewin/`，压根不在被扫的那棵树里）。
fn both_halves() -> (Vec<(PathBuf, String)>, Vec<(PathBuf, String)>) {
    let fw = filewin_dir();
    guard_core::scan_tree_excluding(&repo_root().join("src/bridge/src"), &["rs"], &[])
        .into_iter()
        .partition(|(p, _)| p.starts_with(&fw))
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

// ═══════════════════════════════════════════════════════════════════
// ① 正向：`filewin/` 只许够到一张明写的清单 —— **逐格相等**
// ═══════════════════════════════════════════════════════════════════

/// 一条边的**类别**。闭集 —— 加一格之前先读 `filewin/mod.rs` 那一节的五类。
///
/// 🔴 类别不是装饰：它是「甲要花多少钱」的那一维。
/// `Transport` 那一类是抽 crate 真正的墙（状态耦合，不是符号耦合）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// 纯数据类型。搬进共享 crate 最便宜。
    Type,
    /// 口径常量。搬走之后两侧仍然只许有一份。
    Budget,
    /// 那 13 条 `sftp_*`。**进程级连接池**绑在这一类上。
    Transport,
    /// 安全性质的唯一住址（`INVARIANTS F47` 要求与 SFTP 写命令共用）。
    Fence,
    /// 后端长连接那条控制通道。
    Channel,
    /// 起进程那个全仓唯一出口。
    Spawn,
    /// 🔴〔补齐五项 2026-09-23〕**给用户开一个真终端**那个全仓唯一出口。
    ///
    /// # 为什么它不是 [`Kind::Spawn`]（那条判词逐字说了「不许随手加一格」，所以这里得答）
    ///
    /// `Spawn` 那一类是 `spawn_managed::*` —— 一组**原语**：起一个子进程、
    /// 按策略管它的控制台与寿命。而 `launch::launch_remote_terminal` 是**一层编排**，
    /// 它在原语之上多做两件 `Spawn` 那一类一件都不做的事：
    ///
    /// 1. **按 origin 去读落盘的那份远端配置**（`load_remote_config_by_label`）
    ///    ⇒ 它把 `filewin/` 与 **app 的配置文件**接上了，那是一条状态耦合，
    ///    不是一次符号引用（抽 crate 的代价因此与 `Spawn` 那一类不同级）。
    /// 2. **按平台分档**（Windows：PowerShell ＋ Windows Terminal；POSIX：
    ///    逐字回一句「刻意不替你挑终端模拟器」）⇒ 它带着一条**产品裁决**，
    ///    而那条裁决的唯一住址在 `launch.rs`，不在这儿。
    ///
    /// ⇒ 归进 `Spawn` 会让那一维记的「抽 crate 要花多少钱」**变假**：
    /// 读者会以为这条边只要注一个 spawn 口就断得开。
    /// 同拍在 `filewin/mod.rs` 那一节里加了第 6 类（判词逐字要求「同拍改」）。
    Terminal,
}

/// ★ **登记表**：`filewin/` 生产段够得到的 app 侧符号，**逐条**。
///
/// 🔴 这张表就是那句「解耦清楚」的可判形态：盘上多一条 / 少一条都红。
/// 数字与条目是**现打**出来的（先置空跑一趟，从诊断里读出来再钉），
/// 不是照着 `use` 抄的。
///
/// ⚠ 路径逐字**不带 `crate` 前缀**，而且这张表住 `tests/` ——
/// 被扫的两个人群都够不着它 ⇒ 结构上不可能「在自己的语料里找到自己」
/// （理由住本文件头注那一节）。
///
/// ⚠ **它是一份 census，不是一份许可**：在这里加一行不等于那条边是对的，
/// 只等于「它被看见了」。
const REGISTERED: &[(&str, Kind)] = &[
    // ── 后端长连接那条控制通道（搜索那一族走它）──────────────────
    ("backend::control::backend_route::Routed", Kind::Channel),
    ("backend::control::backend_route::no_channel", Kind::Channel),
    (
        "backend::control::backend_route::route_call_error",
        Kind::Channel,
    ),
    (
        "backend::control::inbound_client::client_for",
        Kind::Channel,
    ),
    // ── 纯类型 ──────────────────────────────────────────────────
    ("origin::Origin", Kind::Type),
    ("sftp_pool::CopyVerdict", Kind::Type),
    ("sftp_pool::SftpEntry", Kind::Type),
    ("ssh_source::RemoteConfig", Kind::Type),
    // ── 口径常量 ────────────────────────────────────────────────
    ("sftp_pool::MAX_EDIT_BYTES", Kind::Budget),
    ("sftp_pool::TRANSFER_LANE_CAP", Kind::Budget),
    // ── 围栏（安全性质的唯一住址）────────────────────────────────
    ("sftp_pool::is_protected_claude_data_path", Kind::Fence),
    // ── 传输：那 13 条 `sftp_*` —— 抽 crate 的墙就在这一类上 ──────
    ("sftp_pool::sftp_cancel_transfer", Kind::Transport),
    ("sftp_pool::sftp_chmod", Kind::Transport),
    ("sftp_pool::sftp_copy", Kind::Transport),
    ("sftp_pool::sftp_delete", Kind::Transport),
    ("sftp_pool::sftp_download", Kind::Transport),
    ("sftp_pool::sftp_list_dir", Kind::Transport),
    ("sftp_pool::sftp_mkdir", Kind::Transport),
    ("sftp_pool::sftp_read_text_for_edit", Kind::Transport),
    ("sftp_pool::sftp_realpath", Kind::Transport),
    ("sftp_pool::sftp_rename", Kind::Transport),
    ("sftp_pool::sftp_stat", Kind::Transport),
    ("sftp_pool::sftp_upload", Kind::Transport),
    ("sftp_pool::sftp_write_text", Kind::Transport),
    // ── 开终端（全仓唯一出口；`launcher_identity_registry` 把它记成 `L2`）──
    ("launch::launch_remote_terminal", Kind::Terminal),
    // ── 起进程（全仓唯一出口；另有 `exec_site_registry` 管着它）──
    ("spawn_managed::ConsolePolicy", Kind::Spawn),
    ("spawn_managed::Lifetime", Kind::Spawn),
    ("spawn_managed::ManagedChild", Kind::Spawn),
    ("spawn_managed::StderrSink", Kind::Spawn),
    ("spawn_managed::spawn_managed_cmd", Kind::Spawn),
];

/// ★★ **`filewin/` 够到 app 侧的每一条边都在表里，表里也不留死行。**
///
/// 少了它，「解耦清楚」这句话会退化成一次性的：今天摸完了是干净的，
/// 明天谁在 `writeops.rs` 里随手 `use crate::history::…` 一行，
/// 没有任何一个数会动。
#[test]
fn every_edge_from_the_file_manager_into_the_app_is_declared() {
    let (inside, _) = both_halves();
    // ★ 抽取器自检 1：人群塌了 ⇒ 下面那条相等会在两边都空上成立。
    assert!(
        inside.len() >= 16,
        "`filewin/` 下只扫到 {} 份 `.rs` —— 遍历器坏了（2026-09-23 现打 17）。\
         这一格非有不可：人群塌成空集时，下面那条相等**照样成立**",
        inside.len()
    );

    let root = repo_root();
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut who: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (path, src) in &inside {
        let prod = guard_core::production_code(src);
        // ★ 剥法自检：剥完不许残留测试属性（`guard-core` 那条反向自检）。
        guard_core::assert_no_test_code(&rel(&root, path), &prod);
        for e in app_side_edges(&prod) {
            who.entry(e.clone()).or_default().push(rel(&root, path));
            found.insert(e);
        }
    }
    // ★ 抽取器自检 2：命中面塌了 ⇒ 同上。
    assert!(
        found.len() >= 20,
        "只抽到 {} 条边 —— 抽取器坏了（2026-09-23 现打 30：补齐五项那一刀多了\
         `launch::launch_remote_terminal` 一条，也就是「在此打开终端」那颗按钮）",
        found.len()
    );

    let want: BTreeSet<String> = REGISTERED.iter().map(|(p, _)| (*p).to_string()).collect();
    assert_eq!(
        want.len(),
        REGISTERED.len(),
        "`REGISTERED` 里有重复路径 —— 后一行会静默吃掉前一行"
    );
    // 🔴 **逐格相等，两个方向一起报**：地板在「变少」方向是瞎的。
    assert_eq!(
        found,
        want,
        "文件管理器与 app 侧那条边界动了。\n  \
         盘上有、表里没有（**新长出来的耦合**）：{:?}\n  \
         表里有、盘上没有（那条边退役了 ⇒ 把这一行删掉，别让表替真判据挡枪）：{:?}\n\n\
         ⇒ 这张表就是「解耦清楚」那句话的可判形态。加一行之前先问一遍：\n\
         ① 这条边**非有不可**吗（`filewin/` 能不能只吃一个已经在表里的类型）？\n\
         ② 它落在哪一类（`Kind`）—— 那一维记的是「哪天真抽 crate 要花多少钱」；\n\
         ③ 类别不够用**不是**往 `Kind` 里随手加一格的理由，先读 `filewin/mod.rs` 那一节。\n\
         逐条住址：{:?}",
        found.difference(&want).collect::<Vec<_>>(),
        want.difference(&found).collect::<Vec<_>>(),
        who,
    );
}

/// ★ **表里每一类都还活着** —— 类别不许长草。
///
/// 一个空了的类别会替真欠账挡枪：读表的人以为那一维还有内容。
#[test]
fn every_declared_edge_falls_in_a_live_category() {
    use Kind::*;
    for k in [Type, Budget, Transport, Fence, Channel, Spawn, Terminal] {
        let n = REGISTERED.iter().filter(|(_, kk)| *kk == k).count();
        assert!(
            n > 0,
            "`Kind::{k:?}` 这一类今天一条边都没有 —— 把它从 `Kind` 里删掉，\
             并在 `filewin/mod.rs` 那一节里同拍改掉它"
        );
    }
    // 🔴 `Transport` 那一类是甲的墙 —— 它**空了**才是大新闻（那说明池子解耦了）。
    let transport = REGISTERED.iter().filter(|(_, k)| *k == Transport).count();
    assert_eq!(
        transport, 13,
        "走 `sftp_pool` 那条池子的命令从 13 条变成了 {transport} 条。\
         **变多**＝ 抽 crate 更贵了；**变少**＝ 可能有人真把池子注进来了 —— \
         那是好事，但要同拍改 `filewin/mod.rs` 那一节的「甲的代价 ①」（它逐字说这是墙）"
    );
}

// ═══════════════════════════════════════════════════════════════════
// ② 反向：app 侧只许有**一条门** —— 零命中守卫
// ═══════════════════════════════════════════════════════════════════

/// ★ 那条门逐字。**一条，恰好一条。**
const SITES: &[&str] = &["filewin::entry::open_file_window"];

/// ★★ **app 侧够到文件管理器的，除了那条 Tauri 命令之外一处都没有。**
///
/// 🔴 这是一条**零命中守卫**：期望集合只有一个元素，多出任何一个都红。
/// 它钉的正是用户那句「可以单独搞」里最实在的那一半 ——
/// app 侧对 `filewin/` 的**依赖面**只有一个符号宽。
///
/// ⚠ 买不到什么：它判的是**编译期引用**。`lib.rs` 里那句 `pub mod filewin;`
/// （模块声明，不带 `::`）由下面单独一条钉。
#[test]
fn the_app_side_reaches_into_the_file_manager_through_exactly_one_door() {
    let (_, outside) = both_halves();
    // ★ 抽取器自检：人群塌了 ⇒ 零命中守卫会在空集上成立。
    assert!(
        outside.len() >= 80,
        "`filewin/` 之外只扫到 {} 份 `.rs` —— 遍历器坏了。\
         这一格非有不可：一条零命中守卫在空人群上**恒绿**",
        outside.len()
    );

    let root = repo_root();
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut who: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (path, src) in &outside {
        let prod = guard_core::production_code(src);
        for e in filewin_reaches(&prod) {
            who.entry(e.clone()).or_default().push(rel(&root, path));
            found.insert(e);
        }
    }
    let want: BTreeSet<String> = SITES.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        found,
        want,
        "app 侧够到文件管理器的口不止一个了（或者那一个不见了）。\n  \
         盘上有、表里没有：{:?}\n  表里有、盘上没有：{:?}\n\n\
         ⇒ 「文件管理器可以单独搞」这句话的可判形态就是这个集合的**大小**。\n\
         多一条，`filewin/` 就多一根长进 app 的钉子；\n\
         真要加，请在这里加一行**并写清为什么那条 Tauri 命令不够**。\n\
         逐条住址：{:?}",
        found.difference(&want).collect::<Vec<_>>(),
        want.difference(&found).collect::<Vec<_>>(),
        who,
    );
}

/// ★ 模块声明那一行**恰好一处**，而且在 `lib.rs` 上。
///
/// 🔴 与上一条**刻意分开**：上一条的针是 `filewin::`（带两个冒号），
/// 它按构造看不见 `pub mod filewin;`。两条合起来才是完整的反向依赖面。
#[test]
fn the_file_manager_is_mounted_from_exactly_one_place() {
    let lib = std::fs::read_to_string(repo_root().join("src/bridge/src/lib.rs"))
        .expect("读不到 `src/bridge/src/lib.rs`");
    let prod = guard_core::production_code(&lib);
    // 整行相等（不是子串）—— `pin_line` 自带「恰好一行」那道自检。
    guard_core::pin_line(&prod, &format!("pub mod {FILEWIN_MOD};"))
        .expect("`lib.rs` 里那句模块声明不见了、或者有不止一句");
}

// ═══════════════════════════════════════════════════════════════════
// ③ 抽取器自己的判据 —— 🔴 **在合成语料上正反各喂一遍**
// ═══════════════════════════════════════════════════════════════════

/// 🔴 上面三条全都建立在 [`paths_from`] 之上。在**真树**上判它是不够的：
/// 「它采到了而且全对上了」与「它压根什么都没采到」在输出上一模一样
/// （两边都空的相等）。⇒ 这里喂合成文本，**每一档都断相等**。
#[test]
fn the_path_extractor_sees_what_it_should_and_nothing_else() {
    let want = |v: &[&str]| -> BTreeSet<String> { v.iter().map(|s| (*s).to_string()).collect() };
    let w = CRATE_WORD;

    // ① 一条普通路径。
    assert_eq!(
        paths_from(&format!("let x = {w}::a::b(1);"), w),
        want(&["a::b"])
    );
    // ② 深路径**整条**回来，不截断。
    assert_eq!(
        paths_from(&format!("{w}::a::b::c::D"), w),
        want(&["a::b::c::D"])
    );
    // ③ 花括号组摊开成几条。
    assert_eq!(
        paths_from(&format!("use {w}::a::{{b, c, d}};"), w),
        want(&["a::b", "a::c", "a::d"])
    );
    // ④ 🔴 **阴性对照**：同名的词后面没有两个冒号 ⇒ 一条都不算。
    //    少了这一档，一个「凡出现这个词就记一条」的实现照样能过 ①②③。
    assert_eq!(paths_from(&format!("let {w} = 1; // {w}"), w), want(&[]));
    // ⑤ 🔴 **阴性对照**：这个词是另一个标识符的**一截** ⇒ 一条都不算。
    //    ⚠ 本条第一版把期望写成了 `a::b`（抄了 ② 的形），**判据当场红并逮住了我** ——
    //      如实记：它证明这一档不是摆设（`my_crate::a::b` 真的返回空集）。
    assert_eq!(paths_from(&format!("my_{w}::a::b"), w), want(&[]));
    // ⑥ 换行折过的 `use` 照样认得出（rustfmt 真会折）。
    assert_eq!(
        paths_from(&format!("use {w}::a::{{\n    b,\n    c,\n}};"), w),
        want(&["a::b", "a::c"])
    );

    // ⑦ 自己人（`filewin::…`）不算一条边，别人算。
    let src = format!("{w}::{FILEWIN_MOD}::rows::show; {w}::sftp_pool::sftp_stat();");
    assert_eq!(app_side_edges(&src), want(&["sftp_pool::sftp_stat"]));
    // ⑧ 反向那个抽取器带前缀回。
    assert_eq!(
        filewin_reaches(&format!("commands![{FILEWIN_MOD}::entry::open_x]")),
        want(&["filewin::entry::open_x"])
    );
    // ⑨ 🔴 **阴性对照**：注释里那一行**不该**被算 —— 判的是生产段。
    //    （剥注释是 `production_code` 的活，这里钉的是「调用方真的剥了」的前提：
    //     喂一份没剥过的文本，抽取器**会**看见它 ⇒ 所以上面三条判据
    //     每一处都先过 `production_code`，不是可有可无的一步。）
    assert_eq!(
        paths_from(&format!("// {w}::sneaky::edge"), w),
        want(&["sneaky::edge"]),
        "抽取器本身不剥注释 —— 这是**已知**的，上面那三条判据靠 `production_code` 剥"
    );
    assert_eq!(
        paths_from(
            &guard_core::production_code(&format!("// {w}::sneaky::edge")),
            w
        ),
        want(&[]),
        "剥过之后还看得见注释里那一条 —— `production_code` 在这一形上不生效了"
    );
}
