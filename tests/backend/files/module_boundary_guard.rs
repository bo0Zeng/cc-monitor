//! 〔F1 · 波 5 · 2026-09-24〕**文件管理后端模块**与原生后端那条边界的判据。
//!
//! 用户逐字：「甲, 窗口变成独立前端. 我说了后端要模块化, 即**原生后端＋文件管理后端**.
//! **现在先解耦清楚**. 然后 monitor 可以打开文件管理器的前端」。
//!
//! # 「解耦清楚」在这里被改述成了什么（三条，每条一个会红的判据）
//!
//! | # | 用户那句话落成的性质 | 判据 |
//! |---|---|---|
//! | ① | 这个模块**只许依赖** `platform` / `common` / 围栏 | [`every_edge_out_of_the_file_backend_is_declared`]：模块生产段够到外面的**每一条**符号路径，与 [`OUTWARD`] **逐格相等**；每一格的类别必须与它的路径首段对得上 |
//! | ② | 原生后端**零处**伸手进它**内部** | [`the_native_backend_reaches_the_file_backend_only_through_its_doors`]：模块之外的后端生产段够到模块的符号，集合 == [`DOORS`]（几个入口函数，零个内部符号） |
//! | ③ | 外界够到它**只有一扇门**（命令注册那一处） | 同上那一条的**文件**那一维：[`DOORS`] 里除了挂载／汇总那一格，住址全是 `inbound.rs` |
//!
//! 形状照前端那一侧的先例 `tests/bridge/filewin/boundary_tests.rs`（正向逐格相等 ＋
//! 反向一元素零命中守卫 ＋ 抽取器在合成语料上正反各喂一遍）。抽取器是**移植**过来的一份，
//! 不是共享的：两个 crate 之间没有共享落点（`src/backend` 刻意不在 monitor 那个 workspace 里），
//! 而把它搬进 `guard-core` 是另一件活、不在本路写区。**两份会漂** —— 如实登记。
//!
//! # 模块的成员（人群）
//!
//! `src/backend/files/**` ＋ `src/backend/control/files_write.rs`。后者住在 `control/`
//! 是**层**的归属（「会改变世界」那一层，`readonly_guard` 第三层唯一登记的模块），
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
// 抽取器 —— 移植自 `tests/bridge/filewin/boundary_tests.rs::paths_from`
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
/// 〔F7c · 第三波 09-24〕`control/files_commit.rs`（上传的提交）同一条理由住 `control/`：
/// 它会改变世界；而 `files/` 那一族的头注逐字「整族纯读」，放进去那句话就当场变假。
/// 〔SR1b · 第四波 09-24〕`control/transfer.rs`（传输台住本机后端：下载的本机落点在这里写）同一条理由住 `control/`。
const MEMBERS_ELSEWHERE: &[&str] = &[
    "control/files_commit.rs",
    "control/files_write.rs",
    "control/transfer.rs",
];

/// 模块在 crate 内的路径前缀（**成员之间**的引用不算一条边）。
const MODULE_PATHS: &[&str] = &[
    "files::",
    "control::files_commit::",
    "control::files_write::",
    "control::transfer::",
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
    out
}

// ═══════════════════════════════════════════════════════════════════
// ① 正向：模块只许够到 platform / common / 围栏 —— 逐格相等
// ═══════════════════════════════════════════════════════════════════

/// 一条外向边的类别。**闭集**。
///
/// 前三类是用户那句「只许依赖 `platform` / `common` / 围栏」逐字列出的三样；
/// 第四类 [`Kind::LedgerAxis`] **在那三样之外**，如实单列（理由在它自己那一行）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// `platform/` —— 唯一允许平台原语的层。
    /// 〔W5-FILES · 第五波〕**第一条边**：设备号（`platform::paths::device_of`，`设计/60 §3.7`「设备号要走 `platform/`」）——
    /// 算目录大小与建索引都要判「这一层是不是挂着另一个文件系统」。此前 2026-09-24 现打是零条。
    Platform,
    /// `common/` —— 两边都要、不含平台原语的纯工具。⚠ 今天同样零条边。
    #[allow(dead_code)]
    Common,
    /// Claude 会话数据围栏（适配层里那一个判定，与桥那一侧函数体逐字相同）。
    /// 〔FN1 · V119〕文件管理写面不再有这道围栏；这一类今天只剩**删历史会话**那一条要的两样
    /// （「要删的必须**是**一份会话记录」· 「按 sid 找那一份」），名字沿用，因为它们仍是那个布局知识、仍住适配层。
    Fence,
    /// 🔴 **用户那三样之外的唯一一类**：`lib.rs` 顶层的 target 轴（`Target` / `TARGETS`）。
    ///
    /// `files/mod.rs` 再导出它，是因为本模块的能力声明表（`files::CAPABILITIES`）要逐条声明
    /// 「在哪几个 target 上做得到」，而那个轴住在汇总层（`lib.rs`，步 `8a` 的理由：
    /// 声明 target 的面不止一个）。⇒ 它是**声明契约**的一部分，不是行为依赖。
    /// 🔴 但它确实是一条「够到 platform / common / 围栏之外」的边 —— **不假装它不是**：
    /// 这一类的条数被钉死（[`the_only_edge_outside_the_three_allowed_kinds_is_the_target_axis`]），
    /// 多一条就红。搬走它要动 `lib.rs` 的模块层（A1 那一路的写区），本路不做。
    LedgerAxis,
    /// 🔴 〔SR1b · 第四波 09-24〕**用户那三样之外的第二类**：传输台（`control/transfer.rs`）的**传输**。
    ///
    /// 用户 V89「SFTP 进本机常驻后端」：传输台搬进本机后端之后，它要一条 sftp 会话（`dial::sftp`，
    /// 与其它 SSH 同一条连接）才搬得动字节，要出方向那一种帧（`wire`）才报得出进度 —— 这两样是
    /// 「传输」这件事本身，不是顺手借用。⇒ 只许 `dial::sftp::` 与 `wire::` 两个前缀，条数钉死
    /// （[`the_only_edge_outside_the_three_allowed_kinds_is_the_target_axis`]）；传输台**只经** `dial/sftp.rs`
    /// 够到拨号（手里不拿 `DialRequest`，拿的是那一份包出来的 `Dial`）。
    Transport,
    /// 🔴 〔HX1 · 4D · 主会话裁〕**用户那三样之外的第三类**：后端建自家目录的那一个函数（`own_dir::ensure_private_dir`）。
    ///
    /// 暂存区（`~/.cc-monitor/staging`）是后端自己的目录、不是用户的；主会话裁「建自家目录收成一个小函数（0700、已存在不动）」
    /// ⇒ 写面建暂存区那两层时调它，而不是自己按 umask 建。只许这一个符号，条数钉死 1。
    OwnHome,
}

/// ★ **登记表**：模块生产段够到外面的符号，**逐条**。
///
/// 数字与条目是**现打**出来的（先置空跑一趟，从诊断里读出来再钉，照先例），不是照 `use` 抄的。
/// ⚠ 它是 census 不是许可：加一行不等于那条边是对的，只等于它被看见了。
const OUTWARD: &[(&str, Kind)] = &[
    // ── 围栏 ────────────────────────────────────────────────────────
    (
        "agents::claudecode::paths::is_session_record_path",
        Kind::Fence,
    ),
    // 〔RW1 · 第四波 09-24〕同一道围栏的**另一面**：删历史会话那一条（〔FN1〕上一行今天也只剩它在用）
    //   的落点由适配层按 sid 找 —— 「哪一份算会话、它在哪」仍是适配层的知识，写面只调用。
    (
        "agents::claudecode::paths::session_file_for_delete",
        Kind::Fence,
    ),
    // ── platform ──────────────────────────────────────────────────────
    // 〔W5-FILES〕设备号（`设计/60 §3.7`）。
    ("platform::paths::device_of", Kind::Platform),
    // ── 〔HX1〕后端建自家目录（暂存区那两层）──────────────────────────
    ("own_dir::ensure_private_dir", Kind::OwnHome),
    // ── 平台 ────────────────────────────────────────────────────────
    // 〔HX1 · 主会话裁拍板项 2〕覆盖写「属主不是后端这个用户 ⇒ 退回就地写」要问这台进程的 uid。
    ("platform::paths::current_uid", Kind::Platform),
    // ── 汇总层的 target 轴（用户那三样之外，条数钉死）─────────────────
    ("TARGETS", Kind::LedgerAxis),
    ("Target", Kind::LedgerAxis),
    // ── 〔SR1b〕传输台的传输（用户那三样之外，条数钉死）─────────────────
    ("dial::sftp::Dial", Kind::Transport),
    ("dial::sftp::Session", Kind::Transport),
    ("wire::Frame", Kind::Transport),
    ("wire::TransferEnd", Kind::Transport),
];

/// 围栏那一类**只许**是这两个符号（不是「`agents::` 底下随便什么」）：
/// 写面「不许碰会话文件」那一问（〔FN1 · V119〕那一问拿掉了，今天是删会话那一条「要删的必须**是**会话」）＋
/// 〔RW1 · 第四波 09-24〕删历史会话那一条「要删的是哪一份」那一问。
/// 两个都是会话文件围栏的知识，住适配层；多出第三个 ⇒ 红。
const THE_FENCES: &[&str] = &[
    "agents::claudecode::paths::is_session_record_path",
    "agents::claudecode::paths::session_file_for_delete",
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
    let want: BTreeSet<String> = OUTWARD.iter().map(|(p, _)| (*p).to_string()).collect();
    assert_eq!(want.len(), OUTWARD.len(), "`OUTWARD` 里有重复行");
    assert_eq!(
        found,
        want,
        "文件管理后端够到外面的边界动了。\n  \
         盘上有、表里没有（**新长出来的依赖**）：{:?}\n  \
         表里有、盘上没有（那条边退役了 ⇒ 删行）：{:?}\n\n\
         用户逐字「后端要模块化, 即原生后端＋文件管理后端. 现在先解耦清楚」。\n\
         ⇒ 这个模块只许依赖 platform / common / 围栏；别的依赖先问能不能不要。\n\
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
    ("inbound.rs", "files::answer_wire", Door::Command),
    (
        "inbound.rs",
        "control::files_write::answer_wire",
        Door::Command,
    ),
    // 〔F7c · 第三波 09-24〕上传的提交（`设计/60 §13`）：同一扇门里的第三个入口函数。
    (
        "inbound.rs",
        "control::files_commit::answer_wire",
        Door::Command,
    ),
    // 〔SR1b · 第四波 09-24〕传输台（`control/transfer.rs`）：**每条流连接一张票表**（同 `dial::link::Table`）
    //   ⇒ 这一面的入口是「造表 ＋ 答口」两个函数，外加读循环 / 分派签名里点名的那个表类型。
    //   四条 `transfer-*` 硬臂**全**经 `answer_wire` 进来（它们是 `Run::Builtin`，要碰本连接的票表与应答通道）。
    ("inbound.rs", "control::transfer::Desk", Door::Command),
    ("inbound.rs", "control::transfer::Desk::new", Door::Command),
    (
        "inbound.rs",
        "control::transfer::Desk::answer_wire",
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

/// ★ 类别与路径对得上：「只许依赖 platform / common / 围栏」在**每一格**上都成立，
/// 而那三样之外的只有 target 轴那两格。
///
/// 🔴 没有这一条，`OUTWARD` 可以把一条 `observe::…` 登记成 `Kind::Common` 让上一条变绿。
#[test]
fn the_only_edge_outside_the_three_allowed_kinds_is_the_target_axis() {
    for (path, kind) in OUTWARD {
        let ok = match kind {
            Kind::Platform => has_prefix(path, "platform::"),
            Kind::Common => has_prefix(path, "common::"),
            Kind::Fence => THE_FENCES.contains(path),
            Kind::LedgerAxis => *path == "Target" || *path == "TARGETS",
            Kind::Transport => has_prefix(path, "dial::sftp::") || has_prefix(path, "wire::"),
            Kind::OwnHome => *path == "own_dir::ensure_private_dir",
        };
        assert!(
            ok,
            "`{path}` 被登记成 `{kind:?}`，而它的路径不属于那一类 —— 登记表在替一条越界的边挡枪"
        );
    }
    let axis = OUTWARD
        .iter()
        .filter(|(_, k)| *k == Kind::LedgerAxis)
        .count();
    assert_eq!(
        axis, 2,
        "用户那三样之外的边从 2 条变成了 {axis} 条 —— **相等，不是上限**。\
         变多 = 模块又长出一条 platform / common / 围栏之外的依赖；\
         变少 = target 轴被搬走了（好事，同拍把这一类删掉）"
    );
    // 〔SR1b〕传输那一类：**相等**。变多 = 传输台又伸手够了一样东西；变少 = 同拍删行。
    let transport = OUTWARD
        .iter()
        .filter(|(_, k)| *k == Kind::Transport)
        .count();
    // 〔HX1〕建自家目录那一类：**相等**，恰好那一个函数。
    let own_home = OUTWARD.iter().filter(|(_, k)| *k == Kind::OwnHome).count();
    assert_eq!(
        own_home, 1,
        "建自家目录那一类的外向边从 1 条变成了 {own_home} 条"
    );
    assert_eq!(
        transport, 4,
        "传输台的外向边从 4 条变成了 {transport} 条 —— 它只该要一条 sftp 会话（经 `dial/sftp.rs` 的 \
         `Dial` / `Session`）和出方向那一种帧（`Frame` / `TransferEnd`）"
    );
}

/// ★ 门的**住址**：命令那一类全住 `inbound.rs`，汇总那一类恰好一格、住 `lib.rs`。
#[test]
fn the_doors_are_the_command_registry_plus_one_ledger_read() {
    for (file, sym, door) in DOORS {
        let home = match door {
            Door::Command => "inbound.rs",
            Door::Ledger => "lib.rs",
        };
        assert_eq!(
            *file, home,
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
        command, 6,
        "命令注册那一处够到的入口从 6 个变成了 {command} 个 —— \
         三面（读 `files::answer_wire` ／ 写 `control::files_write::answer_wire` ／ \
         上传提交 `control::files_commit::answer_wire`〔F7c 09-24 +1〕）各一个入口，\
         〔SR1b 09-24 +3〕传输台每连接一张表：表类型 ＋ 造表 `Desk::new` ＋ 答口 `Desk::answer_wire`。\
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
        // 〔F7c · 第三波 09-24〕上传的提交那一份。
        ("control/mod.rs", "pub mod files_commit;"),
        // 〔SR1b · 第四波 09-24〕传输台那一份。
        ("control/mod.rs", "pub mod transfer;"),
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
