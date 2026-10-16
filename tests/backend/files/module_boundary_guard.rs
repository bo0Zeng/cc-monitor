//! **文件管理后端模块**与原生后端那条边界的判据。
//!
//! 用户逐字：「甲, 窗口变成独立前端. 我说了后端要模块化, 即**原生后端＋文件管理后端**.
//! **现在先解耦清楚**. 然后 monitor 可以打开文件管理器的前端」。
//!
//! # 「解耦清楚」在这里被改述成了什么（三条，每条一个会红的判据）
//!
//! | # | 用户那句话落成的性质 | 判据 |
//! |---|---|---|
//! | ① | 这个模块**只许依赖** `platform` / `common` / 下面一层的基础设施（业务模块之间零依赖；适配层那一类为零） | [`every_edge_out_of_the_file_backend_is_declared`]：模块生产段够到外面的**每一条**符号路径，与 [`OUTWARD`] **逐格相等**；每一格的类别必须与它的路径首段对得上 |
//! | ② | 原生后端**零处**伸手进它**内部** | [`the_native_backend_reaches_the_file_backend_only_through_its_doors`]：模块之外的后端生产段够到模块的符号，集合 == [`DOORS`]（几个入口函数，零个内部符号） |
//! | ③ | 外界够到它**只有一扇门**（命令注册那一处） | 同上那一条的**文件**那一维：[`DOORS`] 里除了挂载／汇总那一格，住址全在入方向（命令表文件管理那一族 · 分派） |
//!
//! 形状照前端那一侧的先例 `tests/frontend/shell/filewin/boundary_tests.rs`（正向逐格相等 ＋
//! 反向一元素零命中守卫 ＋ 抽取器在合成语料上正反各喂一遍）。抽取器是**移植**过来的一份，
//! 不是共享的：两个 crate 之间没有共享落点（`src/backend` 刻意不在 monitor 那个 workspace 里），
//! 而把它搬进 `guard-core` 是另一件活。**两份会漂** —— 如实登记。
//!
//! # 模块的成员（人群）
//!
//! `src/backend/files/**` ＋ `src/backend/control/files_write.rs`。后者住在 `control/`
//! 是**层**的归属（「会改变世界」那一层，文件管理面的写都经它），
//! 不妨碍它是这个模块的成员 —— 本判据按**成员名单**切，不按目录切。
//! 🔴 **不挪文件**：协调方逐字「解耦清楚 ＝ 落成会红的判据（不是挪文件）」。
//!
//! # ⚠ 买不到什么
//!
//! - 判的是**编译期路径引用**：宏拼出来的路径、`use` 之后的短名多用几次都看不见
//!   （`use` 那一行本身带前缀，「引入一个新符号」照样看得见）。
//! - 判不了**运行期耦合**：两边共享同一个进程、同一个 tokio 运行时、同一个全局分配器。
//!   「模块化」在这里是**源码依赖面**的模块化，不是进程隔离。
//! - 抽取器两份（桥那一份与本份）会漂 —— 本份自己的合成语料判据钉得住本份，钉不住两份一致。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ═══════════════════════════════════════════════════════════════════
// 抽取器 —— 移植自 `tests/frontend/shell/filewin/boundary_tests.rs::paths_from`
// ═══════════════════════════════════════════════════════════════════

/// `crate` 这个关键字。
const CRATE_WORD: &str = "crate";

/// 一段源码里，从 `ident` 这个词出发的**全部路径引用**（`ident::a::b` ⇒ `a::b`）。
///
/// 认 · 认不出什么，与桥那一份逐条相同（整条路径不截断 · 花括号摊一层 ·
/// 词后面不是两个冒号不算 · 不剥注释 —— 调用方先过 `guard_core::production_code`）。
fn paths_from(prod: &str, ident: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b = prod.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
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
        let mut j = i;
        if !eat_colons(b, &mut j) {
            continue;
        }
        let mut segs: Vec<String> = Vec::new();
        loop {
            if j < b.len() && b[j] == b'{' {
                let Some(close) = (j..b.len()).find(|&k| b[k] == b'}') else {
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

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

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

fn split_group(inner: &str) -> Vec<String> {
    inner
        .split(',')
        .map(|s| s.trim().trim_start_matches("r#"))
        .filter(|s| !s.is_empty() && *s != "self" && *s != "*")
        .map(|s| s.to_string())
        .collect()
}

/// 前缀比对。**刻意是一个具名函数**：调用点写成语料上的裸前缀匹配会撞
/// `needle_anchor_registry` 那条零富余的递减棘轮（桥那一份逐字同一条理由）。
fn has_prefix(hay: &str, prefix: &str) -> bool {
    hay.len() >= prefix.len() && &hay[..prefix.len()] == prefix
}

// ═══════════════════════════════════════════════════════════════════
// 人群：模块成员 ↔ 原生后端
// ═══════════════════════════════════════════════════════════════════

/// 模块成员里**不住 `files/` 目录**的那几份（仓相对 `src/backend`）。
/// `control/files_commit.rs`（上传的提交）同一条理由住 `control/`：
/// 它会改变世界；而 `files/` 那一族的头注逐字「整族纯读」，放进去那句话就当场变假。
/// `control/transfer.rs`（传输台）**不是**成员：两台之间搬字节是拨号那一半的活（它引 `dial::sftp`），归原生那一块；
/// 它要的路径解析 · 暂存件命名 · 流式摘要 · 路径字节还原都住 `common/`，两块各引各的。
/// `control/files_extract.rs`（解压 ＋ 建链接）同一条理由住 `control/`。
const MEMBERS_ELSEWHERE: &[&str] = &[
    "control/files_batch.rs",
    "control/files_commit.rs",
    "control/files_extract.rs",
    "control/files_upload_chunks.rs",
    "control/files_write.rs",
];

/// 模块在 crate 内的路径前缀（**成员之间**的引用不算一条边）。
const MODULE_PATHS: &[&str] = &[
    "files::",
    "control::files_batch::",
    "control::files_commit::",
    "control::files_extract::",
    "control::files_upload_chunks::",
    "control::files_write::",
];

fn src_root() -> PathBuf {
    crate::guard_support::src_root()
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// `src/backend` 整棵树切两半：模块成员 · 原生后端。**人群从文件系统全集派生。**
///
/// 走 `guard_core::scan_tree_excluding`（扫描型判据不许自己遍历）；名单明写为空 ——
/// 本文件住 `tests/backend/files/`，压根不在被扫的那棵树里。
#[allow(clippy::type_complexity)]
fn both_halves() -> (Vec<(PathBuf, String)>, Vec<(PathBuf, String)>) {
    let root = src_root();
    let files_dir = root.join("files");
    let elsewhere: Vec<PathBuf> = MEMBERS_ELSEWHERE.iter().map(|m| root.join(m)).collect();
    guard_core::scan_tree_excluding(&root, &["rs"], &[])
        .into_iter()
        .partition(|(p, _)| p.starts_with(&files_dir) || elsewhere.iter().any(|e| e == p))
}

/// 一份成员源码里**够到模块之外**的全部符号路径（不带 `crate::`）。
fn outward_edges(prod: &str) -> BTreeSet<String> {
    paths_from(prod, CRATE_WORD)
        .into_iter()
        .filter(|p| !MODULE_PATHS.iter().any(|m| has_prefix(p, m)))
        .collect()
}

/// 一份原生后端源码里**够到模块**的全部符号路径（带模块前缀回，形如 `files::x` /
/// `control::files_write::y`）。
///
/// ⚠ 两种写法都要认：带 `crate::` 的整条路径，以及 `use` 进来之后的 `files::x`
/// （`files` 这个词本身出发）。后者会把**别的**名叫 `files` 的东西也捞进来 ——
/// 那是「出声」（一条读起来很怪的条目），不是「静默放过」。
fn inward_edges(prod: &str) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = paths_from(prod, CRATE_WORD)
        .into_iter()
        .filter(|p| MODULE_PATHS.iter().any(|m| has_prefix(p, m)))
        .collect();
    for p in paths_from(prod, "files") {
        out.insert(format!("files::{p}"));
    }
    for p in paths_from(prod, "files_write") {
        out.insert(format!("control::files_write::{p}"));
    }
    for p in paths_from(prod, "files_commit") {
        out.insert(format!("control::files_commit::{p}"));
    }
    for p in paths_from(prod, "files_extract") {
        out.insert(format!("control::files_extract::{p}"));
    }
    out
}

// ═══════════════════════════════════════════════════════════════════
// ① 正向：模块只许够到下面那几层（platform / common / 基础设施）—— 逐格相等
// ═══════════════════════════════════════════════════════════════════

/// 一条外向边的类别。**闭集三类**。
///
/// 「两块零互相依赖（共用 platform / common 除外）」读成
/// **业务模块之间**零依赖：文件模块往外只许够到**下面那几层** —— 没有一类装得下原生那一块的业务
/// （会话 · tmux · 账号 · 拨号与两台之间的传输 · 资产 · 历史），也没有一类装得下适配层（`agents::`）：
/// 删会话那两问（落点 · 形状）由门经 `control::files_write::SessionPort` 递进来，文件模块不认任何一家的记录布局。
/// 〔此前另有「围栏」「target 轴」「传输」「建自家目录」四类：围栏那两条改经门、建自家目录随 `own_dir` 进了 `common/`，
/// target 轴与传输并进 [`Kind::Infra`]。〕
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// `platform/` —— 唯一允许平台原语的层。
    Platform,
    /// `common/` —— 两块共用的判定。
    Common,
    /// 🔴 **汇总层的能力声明轴**：`lib.rs` 的 `TARGETS` / `Target`（住 crate 根，没有前缀）。
    /// 〔此前还许 `dial::sftp::` · `wire::` 两个前缀，都是传输台用的；传输台归原生那一块（layer11）之后
    /// 这一块一条都不引，前缀一并撤了 ⇒ 拨号那一半与帧面再引一条就当场红。〕
    Infra,
}

/// ★ **登记表**：模块生产段够到外面的符号，**逐条**，带类别与理由。
///
/// 数字与条目是**现打**出来的（先置空跑一趟，从诊断里读出来再钉，照先例），不是照 `use` 抄的。
/// ⚠ 它是 census 不是许可：加一行不等于那条边是对的，只等于它被看见了。
const OUTWARD: &[(&str, Kind, &str)] = &[
    (
        "platform::paths::device_of",
        Kind::Platform,
        "设备号：算目录大小与建索引要判这一层是不是挂着另一个文件系统",
    ),
    (
        "platform::paths::current_uid",
        Kind::Platform,
        "覆盖写「属主不是后端这个用户 ⇒ 退回就地写」要问这台进程的 uid",
    ),
    (
        "platform::paths::owner_of",
        Kind::Platform,
        "`files-stat` 的属主：uid → 用户名要问系统的用户库，只在平台层问",
    ),
    (
        "platform::watch_file::watch",
        Kind::Platform,
        "浏览目录保鲜：在名单上的目录挂监听，经盯盘的唯一原语（一阵动静并成一次回调、交出动过的路径）",
    ),
    (
        "platform::watch_file::Watching",
        Kind::Platform,
        "浏览目录保鲜：进程里那一份监听（名单跟着 `files-browse` 换）",
    ),
    (
        "platform::watch_file::Dir",
        Kind::Platform,
        "浏览目录保鲜：名单的一项（目录 · 不递归）",
    ),
    (
        "platform::paths::link_target_of",
        Kind::Platform,
        "`files-stat` 的链接指向：路径本身是不是链接、指向哪（`readlink`），读系统的口住平台层",
    ),
    (
        "platform::paths::home_dir",
        Kind::Platform,
        "这台后端的家目录只在平台层答（`HOME` → `USERPROFILE`）：`files-home` 与暂存区都拼在它底下",
    ),
    (
        "platform::paths::name_problem",
        Kind::Platform,
        "新建 / 建目录 / 改名的目标名在这台能不能用（Windows 与别处两套规则）：平台的事实",
    ),
    (
        "platform::paths::NameProblem::Reserved",
        Kind::Platform,
        "同上一格：为什么不能用的三档，各配一句",
    ),
    (
        "platform::paths::NameProblem::BadChar",
        Kind::Platform,
        "同上",
    ),
    (
        "platform::paths::NameProblem::Trailing",
        Kind::Platform,
        "同上",
    ),
    (
        "platform::fs::Face",
        Kind::Platform,
        "文件管理面的写句柄（分派那一层递进来）：写方法只收 `Resolved`；不覆盖改名 · 不跟链接的开法这些平台原语都在它的方法里",
    ),
    (
        "platform::fs::Resolved",
        Kind::Platform,
        "已过路径解析的路径：`files_write` 的路径解析交回它，`Face` 的写方法只收它",
    ),
    (
        "platform::fs::Perms",
        Kind::Platform,
        "权限位那一格的值（从元数据读出 · 照模式造），交给 `Face` 的改权限才落盘",
    ),
    (
        "platform::fs::RoFile",
        Kind::Platform,
        "只读打开交回的那一份（解压读包：只读、只挪位置）",
    ),
    (
        "platform::fs::noreplace_unsupported",
        Kind::Platform,
        "不覆盖改名那一格：那块盘不认这个旗时退回 `link ＋ unlink`（要问是不是「没有这件原语」）",
    ),
    (
        "platform::fs::Ro",
        Kind::Platform,
        "读盘的唯一命名空间（只读：`Ro::metadata` · `Ro::read_dir` · `Ro::open` …）：后端除 `platform/fs/` 外不许点名 `std::fs`",
    ),
    (
        "platform::fs::Ro::mode",
        Kind::Platform,
        "unix 权限位（`files-stat` 的 `mode` 那一格）：读元数据的平台扩展只住 `platform/fs/`，非 unix ⇒ 缺席",
    ),
    // 路径字节的线上两种形搬进 `common/path_wire.rs`（原生那一块的「在此打开终端」也读它），本族经 `raw` 的再导出照旧用。
    (
        "common::batch_book::Run",
        Kind::Common,
        "一组一条那几条共用的票 · 进度 · 撤：一组做的时候手里那一份（看撤的旗 · 报进度）",
    ),
    (
        "common::batch_book::MINT_TRIES",
        Kind::Common,
        "一组一条那几条共用的「保留两份」起名上界（复制 · 上传提交同一个）",
    ),
    (
        "common::batch_book::cancel_rest",
        Kind::Common,
        "一组一条那几条共用的票 · 进度 · 撤：撤了 ⇒ 剩下那几项都回 cancelled",
    ),
    (
        "common::batch_book::choice_of",
        Kind::Common,
        "一组一条那几条共用的票 · 进度 · 撤：一项同名时照 decide / on_exists 的答",
    ),
    (
        "common::batch_book::key_of",
        Kind::Common,
        "一组一条那几条共用的票 · 进度 · 撤：一项的键（序号）",
    ),
    (
        "common::batch_book::minted",
        Kind::Common,
        "一组一条那几条共用的票 · 进度 · 撤：保留两份另起的那个名",
    ),
    (
        "common::batch_book::moment_text",
        Kind::Common,
        "一组一条那几条共用的：同名清单里那两格时刻按看的那一台的钟写好（窗口照画，不换算）",
    ),
    (
        "common::batch_book::mtime_secs",
        Kind::Common,
        "一组一条那几条共用的：盘上那一格的修改时刻折成 unix 秒（同名清单写时刻之前）",
    ),
    (
        "common::batch_book::ticketed",
        Kind::Common,
        "一组一条那几条共用的票 · 进度 · 撤：带票做一组（断开照常做完、再问回同一份）",
    ),
    (
        "common::path_wire::from_json",
        Kind::Common,
        "入方向路径参数的两种形（字符串 / `{\"b16\"}`）→ 字节",
    ),
    (
        "common::path_wire::to_json",
        Kind::Common,
        "回送路径的两种形（字节 → 字符串 / `{\"b16\"}`，双向无损）",
    ),
    (
        "common::path_wire::HEX_KEY",
        Kind::Common,
        "线上那个键名 `b16`（本族判据按它对拍）",
    ),
    (
        "common::path_wire::to_path_buf",
        Kind::Common,
        "字节还原成 `PathBuf`（同一台取出去的字节原样回来），经 `raw` 的再导出用",
    ),
    // 下面这几格随传输台归原生那一块搬进 `common/`：两块都要，本族经再导出照旧用。
    (
        "common::digest::ContentDigest",
        Kind::Common,
        "SHA-256 的流式那一形：读交出去的摘要 · 写面比对 · 提交对暂存件逐块读着算",
    ),
    (
        "common::digest::SHA256_HEX_LEN",
        Kind::Common,
        "CAS 摘要形里十六进制串的长度（判 `expect` 的形）",
    ),
    (
        "common::root_path::lexical_in_root",
        Kind::Common,
        "路径解析①（词法不越根）：写面每一处改动先过它",
    ),
    (
        "common::root_path::resolve_parent_in_root",
        Kind::Common,
        "路径解析②（父目录解链接后仍在根下）：造 `Resolved` 的那一道",
    ),
    (
        "common::root_path::resolve_in_root",
        Kind::Common,
        "两道串起来跑一遍",
    ),
    (
        "common::staging::KEY_LEN",
        Kind::Common,
        "暂存件的键长：提交认键、孤儿扫认名（造键的一侧是传输台）",
    ),
    (
        "common::staging::PART_SUFFIX",
        Kind::Common,
        "暂存件的后缀：同上一格",
    ),
    (
        "common::contract::malformed",
        Kind::Common,
        "契约错只进表一句（「请求格式不对」，诊断记日志）",
    ),
    (
        "common::own_dir::DIR_NAME",
        Kind::Common,
        "新建落在这台机器自家目录（`~/.cc-monitor`）里的东西只给本人：认那个目录要它的名字",
    ),
    (
        "common::own_dir::PRIVATE_DIR_MODE",
        Kind::Common,
        "同上一格：自家目录里新建的目录给的那个权限位（与 `ensure_private_dir` 同一个值）",
    ),
    (
        "common::time::Tz",
        Kind::Common,
        "看的那一台的时区：修改时间那两格按请求带来的它写",
    ),
    (
        "common::time::Tz::default",
        Kind::Common,
        "写面那张分派表里没有请求信封的那一路（门 · 判据）：上传提交的同名清单时刻按缺省时区写",
    ),
    (
        "common::time::now",
        Kind::Common,
        "此刻（后端的「现在」只有一处，`one_clock_guard`）：索引建好的时刻 · 查找与状态按它算多久以前",
    ),
    (
        "common::time::mtime_texts_now",
        Kind::Common,
        "列目录 · 问元数据 · 查找回的修改时间旁边那两格（列里那一格 · 完整那一格）按看的那一台的时区写好，文件窗口照抄",
    ),
    (
        "TARGETS",
        Kind::Infra,
        "能力声明表（`files::CAPABILITIES`）逐条声明「在哪几个 target 上做得到」，那个轴住汇总层 `lib.rs`\
         （声明 target 的面不止一个）—— 声明契约，不是行为依赖",
    ),
    ("Target", Kind::Infra, "同上一格：target 轴的元素类型"),
];

/// ★★ 模块够到外面的每一条边都在表里，表里也不留死行；而且**只有**那三类。
#[test]
fn every_edge_out_of_the_file_backend_is_declared() {
    let (inside, _) = both_halves();
    assert!(
        inside.len() >= 5,
        "模块成员只扫到 {} 份 —— 遍历或切法坏了（2026-09-24 现打 5：files/ 四份 ＋ files_write.rs）。\
         人群塌成空集时下面那条相等照样成立",
        inside.len()
    );
    let root = src_root();
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut who: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (path, src) in &inside {
        let prod = guard_core::production_code(src);
        guard_core::assert_no_test_code(&rel(&root, path), &prod);
        for e in outward_edges(&prod) {
            who.entry(e.clone()).or_default().push(rel(&root, path));
            found.insert(e);
        }
    }
    let want: BTreeSet<String> = OUTWARD.iter().map(|(p, _, _)| (*p).to_string()).collect();
    assert_eq!(want.len(), OUTWARD.len(), "`OUTWARD` 里有重复行");
    assert_eq!(
        found,
        want,
        "文件管理后端够到外面的边界动了。\n  \
         盘上有、表里没有（**新长出来的依赖**）：{:?}\n  \
         表里有、盘上没有（那条边退役了 ⇒ 删行）：{:?}\n\n\
         用户逐字「后端要模块化, 即原生后端＋文件管理后端. 现在先解耦清楚」。\n\
         ⇒ 这个模块只许依赖 platform / common / 下面一层的基础设施；别的依赖先问能不能不要。\n\
         逐条住址：{:?}",
        found.difference(&want).collect::<Vec<_>>(),
        want.difference(&found).collect::<Vec<_>>(),
        who,
    );
}

// ═══════════════════════════════════════════════════════════════════
// ② ③ 反向：原生后端只经那几扇门够到它 —— 零命中守卫
// ═══════════════════════════════════════════════════════════════════

/// 一扇门的类别。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Door {
    /// **命令注册那一处** —— 用户那句「外界够到它只有一扇门」指的就是这一类。
    Command,
    /// 挂载与汇总：`lib.rs` 的能力清单（`CAPABILITY_FACES`）读这个模块**声明**了哪些能力名。
    /// 🔴 它是第二个**住址**，如实单列：读的是一张声明表，不是一次行为调用；
    /// 条数被钉死（[`the_doors_are_the_command_registry_plus_one_ledger_read`]）。
    Ledger,
}

/// ★ 门：`(住址, 符号, 类别)`。**每一个符号都是入口函数，零个内部符号**
/// （`files::index::*` / `files::raw::*` / 围栏那几个函数 —— 原生后端一个都够不到）。
const DOORS: &[(&str, &str, Door)] = &[
    // 读的那几条：入参的类型选中那条能力（[`crate::files::Ask`]），本体只经 `files::answer` 一个入口。
    (
        "stream/inbound/files_face.rs",
        "files::answer",
        Door::Command,
    ),
    ("stream/inbound/files_face.rs", "files::Ask", Door::Command),
    // 按内容搜要**可撤**（异步档：阻塞线程上那一趟看取消位）⇒ 同一族的第二个入口函数，
    //   不是内部符号：它自己再进 `answer_grep`（与唯一入口 `answer` 里那一臂同一个本体）。
    (
        "stream/inbound/files_face.rs",
        "files::answer_grep_cancellable",
        Door::Command,
    ),
    // 写的那几条（写面 · 上传提交 · 解压）：同读的那一半，一个入口 ＋ 选本体的那个 trait（入参类型挂在它上面）。
    (
        "stream/inbound/files_write_face.rs",
        "control::files_write::answer",
        Door::Command,
    ),
    (
        "stream/inbound/files_write_face.rs",
        "control::files_write::Act",
        Door::Command,
    ),
    // 删会话那两问的窄口：门把适配层那两个函数装进这个类型递给写面（`SESSION_PORT`）。
    (
        "stream/inbound/files_session_face.rs",
        "control::files_write::SessionPort",
        Door::Command,
    ),
    // 删会话那一条的本体（它要门递的那两问，不经 `Act`）。
    (
        "stream/inbound/files_session_face.rs",
        "control::files_write::delete_session_of",
        Door::Command,
    ),
    // 一组一条的复制（`control/files_batch.rs`）：同名要问那一形带同名清单（码表里 `exists` 带 `data`），不经 `Act` 那一扇（它只交码与那一句）。
    (
        "stream/inbound/files_write_face.rs",
        "control::files_batch::copy",
        Door::Command,
    ),
    (
        "stream/inbound/files_write_face.rs",
        "control::files_batch::CopyRefused",
        Door::Command,
    ),
    // 上传的提交：同名要问那一形同样带同名清单（`exists` 带 `data`），不经 `Act` 那一扇。
    (
        "stream/inbound/files_write_face.rs",
        "control::files_commit::commit_upload_of",
        Door::Command,
    ),
    (
        "stream/inbound/files_write_face.rs",
        "control::files_commit::CommitRefused",
        Door::Command,
    ),
    ("lib.rs", "files::capability_names", Door::Ledger),
];

#[test]
fn the_native_backend_reaches_the_file_backend_only_through_its_doors() {
    let (_, outside) = both_halves();
    assert!(
        outside.len() >= 60,
        "原生后端只扫到 {} 份 —— 遍历坏了；零命中守卫在空人群上恒绿",
        outside.len()
    );
    let root = src_root();
    let mut found: BTreeSet<(String, String)> = BTreeSet::new();
    for (path, src) in &outside {
        let prod = guard_core::production_code(src);
        for e in inward_edges(&prod) {
            found.insert((rel(&root, path), e));
        }
    }
    let want: BTreeSet<(String, String)> = DOORS
        .iter()
        .map(|(f, s, _)| ((*f).to_string(), (*s).to_string()))
        .collect();
    assert_eq!(
        found,
        want,
        "原生后端够到文件管理后端的口子与登记的门对不上。\n  \
         盘上有、表里没有（🔴 **有人伸手进了模块内部，或者开了第二扇门**）：{:?}\n  \
         表里有、盘上没有：{:?}",
        found.difference(&want).collect::<Vec<_>>(),
        want.difference(&found).collect::<Vec<_>>(),
    );
}

/// **文件模块与原生那一块的业务零依赖**：往外只够到下面那几层。
///
/// - 每一格的类别与路径首段对得上（没有这一条，`OUTWARD` 可以把一条 `observe::…` 登记成 `Kind::Common` 让上一条变绿）；
/// - 基础设施那一类只许 target 轴那两格，每一格都写得出理由，条数**相等**；
/// - 没有一类装得下 `agents::`（适配层）—— 删会话那两问经门递进来，这一向的边今天为零。
/// 反方向（原生那一块零处伸手进文件模块、只经门）由 [`the_native_backend_reaches_the_file_backend_only_through_its_doors`] 判。
#[test]
fn the_file_backend_depends_on_no_business_module_only_on_the_layers_below() {
    for (path, kind, why) in OUTWARD {
        let ok = match kind {
            Kind::Platform => has_prefix(path, "platform::"),
            Kind::Common => has_prefix(path, "common::"),
            Kind::Infra => *path == "Target" || *path == "TARGETS",
        };
        assert!(
            ok,
            "`{path}` 被登记成 `{kind:?}`，而它的路径不属于那一类 —— 登记表在替一条越界的边挡枪"
        );
        assert!(!why.trim().is_empty(), "`{path}` 没写为什么要这条边");
    }
    let infra = OUTWARD.iter().filter(|(_, k, _)| *k == Kind::Infra).count();
    assert_eq!(
        infra, 2,
        "基础设施那一类从 2 条变成了 {infra} 条 —— **相等，不是上限**：\
         变多 = 文件模块又往下面那一层多要了一样东西（先问能不能不要）；变少 = 同拍删行"
    );
}

/// ★ 门的**住址**：命令那一类全住入方向（命令表里文件管理那一族 · 分派的硬臂与读循环），汇总那一类恰好一格、住 `lib.rs`。
#[test]
fn the_doors_are_the_command_registry_plus_one_ledger_read() {
    for (file, sym, door) in DOORS {
        let homes: &[&str] = match door {
            Door::Command => &[
                "stream/inbound/registry/file_manager.rs",
                // 读的那几条 typed 命令（命令表那一行点它们的名）。
                "stream/inbound/files_face.rs",
                // 写的那几条 typed 命令（同上）。
                "stream/inbound/files_write_face.rs",
                "stream/inbound/files_session_face.rs",
                "stream/inbound/mod.rs",
            ],
            Door::Ledger => &["lib.rs"],
        };
        assert!(
            homes.contains(file),
            "`{sym}` 登记成 `{door:?}` 门，却住在 `{file}` —— 那不是那一类门的住址"
        );
    }
    let ledger = DOORS.iter().filter(|(_, _, d)| *d == Door::Ledger).count();
    assert_eq!(
        ledger, 1,
        "命令注册之外的门从 1 格变成了 {ledger} 格 —— 用户逐字「外界够到它只有一扇门」"
    );
    let command = DOORS.iter().filter(|(_, _, d)| *d == Door::Command).count();
    assert_eq!(
        // 6 → 7：多了解压面 `control::files_extract::answer_wire`。
        // 7 → 8：多了删会话那两问的窄口类型 `control::files_write::SessionPort`。
        // 8 → 9：读那一面多一个可撤的入口 `files::answer_grep_cancellable`（按内容搜，异步档）。
        // 9 → 6：传输台（`Desk` · `Desk::new` · `Desk::answer_wire`）归原生那一块，不再是这一块的门。
        // 6 → 7：读的那几条换成类型，`files::answer_wire` 换成 `files::answer` ＋ 选能力的 `files::Ask`。
        // 7 → 7：写的那几条换成类型，三面各一个 `answer_wire` 换成 `control::files_write::answer` ＋ 选本体的 `control::files_write::Act`
        //   ＋ 删会话那一条的本体 `control::files_write::delete_session_of`（它要门递的那两问）。
        // 7 → 9：一组一条（`control::files_batch`）：复制带同名清单的入口 `copy` ＋ 它的拒绝类型 `CopyRefused`（票 · 进度 · 撤住 `common/batch_book.rs`）。
        // 9 → 11：上传提交同名要问那一形带同名清单：`files_commit::commit_upload_of` ＋ 它的拒绝类型 `CommitRefused`。
        command, 11,
        "命令注册那一处够到的入口从 11 个变成了 {command} 个 —— \
         读（`files::answer` ＋ `files::Ask`）／ 写 · 上传提交 · 解压（`control::files_write::answer` ＋ `control::files_write::Act`）／ 删会话（`control::files_write::delete_session_of`），\
         〔SR1b 09-24 +3，layer11 −3〕传输台归原生那一块，三格出列。\
         〔MOD +1〕删会话那两问的窄口类型 `SessionPort`。〔FILES3 +1〕按内容搜那个可撤的入口 `files::answer_grep_cancellable`。\
         〔第 8 批 +1〕读的那几条换成类型：`files::answer_wire` 换成 `files::answer` ＋ 选能力的那个 trait `files::Ask`。\
         〔第 8 批 ±0〕写的那几条换成类型：三个 `answer_wire` 换成 `answer` ＋ `Act` ＋ 删会话那一条的 `delete_session_of`。\
         〔一组一条 +2〕`files_batch::copy` · `CopyRefused`。\
         多一个就说明有命令绕过了入口、直接调内部"
    );
}

/// ★ 挂载那三行**各恰好一处**（`use` 形与 `::` 形的针按构造看不见 `pub mod x;`）。〔F7c 09-24：两行 → 三行，多了 `files_commit`〕
#[test]
fn the_file_backend_is_mounted_from_exactly_its_three_declarations() {
    let root = src_root();
    for (file, line) in [
        ("lib.rs", "pub mod files;"),
        ("control/mod.rs", "pub mod files_write;"),
        // 上传的提交那一份。
        ("control/mod.rs", "pub mod files_commit;"),
        // 解压 ＋ 建链接那一份；上传块形那一份。
        ("control/mod.rs", "pub mod files_extract;"),
        ("control/mod.rs", "pub mod files_upload_chunks;"),
    ] {
        let src = std::fs::read_to_string(root.join(file))
            .unwrap_or_else(|e| panic!("读不到 `{file}`：{e}"));
        guard_core::pin_line(&guard_core::production_code(&src), line)
            .unwrap_or_else(|e| panic!("`{file}` 里那句模块声明不见了、或者不止一句：{e}"));
    }
}

/// 🔴 抽取器在**合成语料**上正反各喂一遍（照先例 ③）：「采到了且全对上」与
/// 「什么都没采到」在真树上输出一样。
#[test]
fn the_edge_extractors_see_what_they_should_and_nothing_else() {
    let want = |v: &[&str]| -> BTreeSet<String> { v.iter().map(|s| (*s).to_string()).collect() };
    let w = CRATE_WORD;
    assert_eq!(paths_from(&format!("{w}::a::b(1)"), w), want(&["a::b"]));
    assert_eq!(
        paths_from(&format!("use {w}::{{Target, TARGETS}};"), w),
        want(&["TARGETS", "Target"]),
        "顶层花括号组（`files/mod.rs` 那句再导出的形状）没摊开"
    );
    // 阴性：同名词没有两个冒号 · 是另一个标识符的一截。
    assert_eq!(paths_from(&format!("let {w} = 1;"), w), want(&[]));
    assert_eq!(paths_from(&format!("my_{w}::a"), w), want(&[]));
    // 外向：成员之间的引用不算边，别人的算。
    let src = format!(
        "{w}::files::raw::to_json(x); {w}::control::files_write::answer_wire(); {w}::observe::fs::x();"
    );
    assert_eq!(outward_edges(&src), want(&["observe::fs::x"]));
    // 内向：两种写法都认，带模块前缀回。
    let src = format!("{w}::files::index::find(); files_write::make_dir(); {w}::kill::x();");
    assert_eq!(
        inward_edges(&src),
        want(&["control::files_write::make_dir", "files::index::find"])
    );
    // 注释：抽取器自己不剥，判据先过 `production_code`。
    assert_eq!(
        paths_from(
            &guard_core::production_code(&format!("// {w}::sneaky::edge")),
            w
        ),
        want(&[]),
        "剥过之后还看得见注释里那一条"
    );
}
