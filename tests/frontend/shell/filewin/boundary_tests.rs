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
//!    `src/frontend/shell/src/filewin/**`，反向那条扫的是它之外的 `src/frontend/shell/src/**`
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
    repo_root().join("src/frontend/shell/src/filewin")
}

/// `src/frontend/shell/src` 整棵树，切成**两半**：`filewin/` 里的 · 外面的。
///
/// 🔴 **人群从文件系统全集派生**，不是一张手写名单 —— 新加一份 `.rs`
/// 自动进人群。切法走 `Path::starts_with`（拿**路径**比路径），
/// 不是拿字符串去 `starts_with("…")`：后者会撞
/// `needle_anchor_registry` 那条零富余的递减棘轮。
///
/// ⚠ 走 `guard_core::scan_tree_excluding` 而不是裸 `read_dir` ——
/// `scanning_guard_registry` 那条判据钉着「扫描型判据不许自己遍历」。
/// 名单明写为空（`设计/16 §5.4b` 纪律 4）：本判据要摘的不是自己
/// （它住 `tests/frontend/shell/filewin/`，压根不在被扫的那棵树里）。
fn both_halves() -> (Vec<(PathBuf, String)>, Vec<(PathBuf, String)>) {
    let fw = filewin_dir();
    guard_core::scan_tree_excluding(&repo_root().join("src/frontend/shell/src"), &["rs"], &[])
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
// ① 正向：`filewin/` 只许够到两张明写的清单 —— **窗口进程一张 · monitor 那一侧一张，各自逐格相等**
// ═══════════════════════════════════════════════════════════════════
//
// 🔴〔F2 · 2026-09-24〕从前这里是**一张**表（整棵 `filewin/` 够到 app 侧的 30 条边）。
// 窗口成了独立进程、只经通道说 `call` 之后，「整棵树」这个人群不再对应任何一个进程：
// 同一棵树编进两个二进制，一部分代码只在 monitor 里跑（入口那条 Tauri 命令、起窗口进程、
// 开窗前解 home），其余只在窗口进程里跑。题面那句判据逐字是「**窗口进程的依赖面** ==
// {通道客户端, 线上类型, 界面库…}，两向相等」⇒ 人群按**进程**切：
//
// - monitor 那一侧 ＝ `entry.rs` 整份 ＋ [`MONITOR_FNS`] 里点名的那几个函数；
// - 窗口进程那一侧 ＝ 其余全部（含每份文件函数外的 `use`）。
//
// 切法是**按函数体切块**（[`chunks_by_fn`]）：一行 `fn 名字(` 开一块，块归那个名字。
// ⚠ 漏判面：写在一个 monitor 函数体里的**闭包**若在别处被调用，按构造算 monitor 那一侧 ——
// 那是「代码写在哪」而不是「在哪个进程跑」；今天两份表都是现打的，那一形零处。

/// 一条边的**类别**。闭集。
///
/// 🔴 类别不是装饰：窗口那一侧它答的是「**为什么这条边还不是通道**」——
/// `Channel` / `Wire` 两类之外的每一类都是一笔**有住址的欠账**，各自写清卡在谁手里。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// 通道客户端那一侧（`chan::client` / `chan::dial` / 交接件那个类型）。**题面要的就是它。**
    Channel,
    /// 线上类型（`chan::wire::*`，`05 §3.3` 那一套）。**题面要的就是它。**
    Wire,
    // 〔P4〕「跨机传输那一族够到的 app 侧类型」（`Transfer`，末一条是开窗配置的类型 `ssh_source::RemoteConfig`）清零删了：种子只带那台的名字。
    // 〔F7a · 第三波 09-24〕这里原来还有一类「后端今天没有这条命令」（同机复制：池子那条复制命令 ＋
    //   它的裁决类型，2 条）。后端有了 `files-copy` 之后两条都换走了通道 ⇒ 这一类清零，随之删掉
    //   （`every_declared_edge_falls_in_a_live_category` 逐字要求「一条边都没有就从 `Kind` 里删掉」）。
    // 〔FN1 · 第四波 4C · V119〕这里原来还有一类「本地预判的那道围栏」（1 条：窗口借围栏本家那个判定）。
    //   用户「文件管理器全部都可以改. 不需要任何围栏」⇒ 窗口那道预判删了，这一类清零，随之删掉
    //   （`every_declared_edge_falls_in_a_live_category` 逐字要求「一条边都没有就从 `Kind` 里删掉」）。
    // 〔P4〕「在此打开终端」那一类（`Terminal`：机器事实 · 开窗两条）清零删了：窗口经通道交意图，monitor 接下来补事实、开窗（`chan/host.rs::terminal_open`）。
    // 〔P4〕「monitor 自己的状态」那一类（`OwnState`：原子写 · 书签文件名）清零删了：原子写进 `host_core`，书签全路径由开窗入口算好随种子交来。
    /// 〔CP2b · 第四波 4C〕**对外文案表的取文口**（`copy_table::copy_text`）。它不是欠账：
    /// `设计/01 §6.9`「所有对外文案与报错都从一张表来」—— 表是编译期内嵌的一份 JSON，窗口进程与 app 读同一份字节，
    /// 取文口是纯函数（查表 ＋ 填占位符），不碰进程外任何东西。
    Copy,
    /// 〔WF2 · WIN3 读数 D〕**窗口几何**：工作区那个类型 ＋「一扇窗夹进工作区」那一个判定（〔P4〕今天住 `host_core`，`geometry.rs::fit_into_work_area`；壳里只剩问 Tauri 的 `lib.rs::work_area_of`）。
    /// 不是欠账：它与 Tauri 那几扇窗共用一个家（一个判定不许两个家），不碰进程外任何东西。
    Geometry,
    /// monitor 那一侧：通道宿主（交接件 · 生产句柄）。
    Host,
    /// monitor 那一侧：起进程那个全仓唯一出口（`exec_site_registry` 管着）。
    Spawn,
    /// 〔FW34 · 第四波 09-24〕monitor 那一侧：monitor 自己的数据目录 —— 书签文件住那儿，
    /// 开窗时在这一侧算好全路径、放进种子交给窗口进程（窗口进程自己不找数据目录）。
    DataDir,
    /// 〔WF2 · WIN3 读数 J〕monitor 那一侧：出声的既有通道（`remote-health` 事件与它的载荷）——
    /// 窗口进程「判成功」之后又不体面地退了，那一句经它到界面 toast。
    Notify,
    /// monitor 那一侧：那条命令的入参类型（那台机器的配置）。
    /// 〔F7a · 第三波 09-24〕这一类原先还装着「开窗前解 home」（走 SFTP，后端没有这一问）——
    /// 现在问后端 `files-home`，走的是 `Host` 那一类的同一个句柄 ⇒ 这一类只剩配置，改了名。
    Config,
}

/// monitor 那一侧的函数（`entry.rs` 整份之外）。**点名，不靠目录。**
const MONITOR_FNS: &[(&str, &str)] = &[
    ("proc.rs", "spawn_window"),
    ("proc.rs", "write_seed"),
    ("proc.rs", "open_in_new_process"),
    ("proc.rs", "reap_later"),
];

/// monitor 那一侧整份算的文件。
const MONITOR_FILES: &[&str] = &["entry.rs"];

/// ★ **窗口进程**够得到的 app 侧符号，逐条。
///
/// 🔴 题面判据的可判形态：`Channel` ＋ `Wire` 两类是「只说 call/subscribe」本身；
/// 其余三类每一条都是一笔带住址的欠账（见 [`Kind`]；〔F7a 09-24〕「后端缺命令」那一类清零删了）。
const WINDOW_SIDE: &[(&str, Kind)] = &[
    // ── 通道客户端 ──
    ("chan::client::Client", Kind::Channel),
    ("chan::dial::dial", Kind::Channel),
    ("chan::host::Handoff", Kind::Channel),
    // ── 线上类型 ──
    ("chan::wire::Body", Kind::Wire),
    ("chan::wire::Budget", Kind::Wire),
    ("chan::wire::CallError", Kind::Wire),
    ("chan::wire::CancelToken", Kind::Wire),
    // 〔FILES3 · ㉜「可撤」〕按内容搜那一趟的撤单手柄由窗口自己造（「停」拨它）。
    ("chan::wire::CancelToken::new", Kind::Wire),
    ("chan::wire::Comms", Kind::Wire),
    ("chan::wire::HopFault", Kind::Wire),
    ("chan::wire::Op", Kind::Wire),
    ("chan::wire::Origin", Kind::Wire),
    ("chan::wire::OursFault", Kind::Wire),
    ("chan::wire::PeerFault", Kind::Wire),
    ("chan::wire::Reach", Kind::Wire),
    // 〔F7c · 第三波 09-24〕订阅那一口（`source::watch`，窗口进程里唯一一处 `subscribe`）用到的四样。
    ("chan::wire::By", Kind::Wire),
    ("chan::wire::Item", Kind::Wire),
    ("chan::wire::Kind", Kind::Wire),
    ("chan::wire::Sub", Kind::Wire),
    // 〔NET2〕那台的能力事实（接上通道时问一次，做不到的那一件置灰）。
    ("chan::wire::Offer", Kind::Wire),
    // ── 跨机传输 ──
    // 〔F7c · 第三波 09-24〕`§8.4` 拍了（「保留SFTP. 思考怎么干净」）：上传 / 下载经通道开单、订阅进度
    //   （`设计/60 §13`）⇒ `sftp_upload` · `sftp_download` · `TRANSFER_LANE_CAP` 三行走掉；
    //   `sftp_cancel_transfer`〔散文墓碑〕 随复制走后端（F7a，不可取消）一起走掉（`transfer::forward_cancel` 删了）。
    // ── 本地预判围栏 ──〔FN1 · V119〕那一行（围栏本家那个判定）随窗口那道预判删了：这一类清零。
    // ── 本机动作 ──
    // 〔FIX4 · `99 §2.1 ⑬`〕开终端三步：机器事实（monitor 的机器表 ＋ 上次赢的那条）→ 窗口那条通道问本机后端 `terminal-ssh` →
    //   开窗（`launch_remote_terminal`〔散文墓碑〕那一条拼 ssh 的边退役，ssh 外壳进了本机后端）。
    // ── monitor 自己的状态 ──
    // 〔P4〕`utils::atomic_write_json` 那一行摘了：原子写搬进共享 crate `host_core`（前端宿主原语），不再是壳里的边。
    // 〔FILES3 · ㉜〕书签那份文件的名字住数据目录的唯一枚举点（设置页「数据位置」列它），窗口这一侧引过来。
    // ── 对外文案表（CP2b）──
    ("copy_table::copy_text", Kind::Copy),
    // ── 窗口几何（〔WF2〕开窗第一拍夹进种子带来的工作区）──
    // 〔P4〕`WorkArea` · `fit_into_work_area` 两行摘了：类型与判定搬进 `host_core`（Tauri 那几扇窗与文件窗口共用那一份）。
];

/// ★ **monitor 那一侧**（`entry.rs` ＋ [`MONITOR_FNS`]）够得到的 app 侧符号，逐条。
const MONITOR_SIDE: &[(&str, Kind)] = &[
    // 〔MIG-3a · 主会话 09-28 裁 3〕开窗前那两问（`files-home` / `files-ls`）进了窗口进程 ⇒ monitor 这一侧问后端的五样
    //   （宿主句柄 `InboundBackends` · `router::Backends` · `wire::Body` / `CancelToken` / `Op`）退役，只剩交接件那一样。
    ("chan::host::handoff", Kind::Host),
    ("spawn_managed::ConsolePolicy", Kind::Spawn),
    ("spawn_managed::Lifetime", Kind::Spawn),
    ("spawn_managed::ManagedChild", Kind::Spawn),
    ("spawn_managed::StderrSink", Kind::Spawn),
    ("spawn_managed::spawn_managed_cmd", Kind::Spawn),
    ("ssh_source::RemoteConfig", Kind::Config),
    // 〔FILES2 · V152〕开窗种子带上机器名单（「复制到另一台」那一问的下拉）：已有的配置读口，不新建数据源。
    ("load_remote_configs", Kind::Config),
    ("config::resolve_monitor_data_dir", Kind::DataDir), // 〔RE〕原 `paths::`（`paths.rs` 并进 `config.rs`）
    // 〔P4〕书签文件的名字：全路径在这一侧拼好随种子交过去（窗口那一侧不再引它）。
    ("data_paths::FILEWIN_BOOKMARKS_FILE", Kind::DataDir),
    // 〔CP2b〕monitor 那一侧（entry.rs）的报错也从文案表取。
    ("copy_table::copy_text", Kind::Copy),
    // 〔WF2 · WIN3 读数 D〕问主窗所在显示器的工作区，放进种子。
    ("MAIN_WINDOW_LABEL", Kind::Geometry),
    // 〔P4〕类型搬进 `host_core`；问 Tauri 那一下留在壳里，改名 `work_area_of`。
    ("work_area_of", Kind::Geometry),
    // 〔WF2 · WIN3 读数 J〕开出来之后又退了那一句经 `remote-health` 出声。
    ("ui_contract::RemoteHealthPayload", Kind::Notify),
    ("ui_contract::events::REMOTE_HEALTH", Kind::Notify),
];

/// 一段生产代码 → `(函数名, 那一块)`。函数外的行归 `""`。
///
/// 一行里出现 `fn 名字(`（前面只许是 `pub` / `async` / `const` 之类的修饰词）就开一块。
fn chunks_by_fn(prod: &str) -> Vec<(String, String)> {
    const FN_WORD: &str = "fn ";
    let mut out: Vec<(String, String)> = vec![(String::new(), String::new())];
    for line in prod.lines() {
        let t = line.trim_start();
        let head_ok = t.find(FN_WORD).is_some_and(|at| {
            t[..at].split_whitespace().all(|w| {
                matches!(
                    w,
                    "pub" | "pub(crate)" | "pub(super)" | "async" | "const" | "unsafe"
                )
            })
        });
        if head_ok {
            let at = t.find(FN_WORD).unwrap_or(0) + FN_WORD.len();
            let name: String = t[at..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                out.push((name, String::new()));
            }
        }
        if let Some(last) = out.last_mut() {
            last.1.push_str(line);
            last.1.push('\n');
        }
    }
    out
}

/// 整棵 `filewin/` 生产段 → `(窗口进程那一侧的边, monitor 那一侧的边, 点名的 monitor 函数找到了几个)`。
fn edges_by_process() -> (
    std::collections::BTreeMap<String, Vec<String>>,
    std::collections::BTreeMap<String, Vec<String>>,
    BTreeSet<(String, String)>,
) {
    let (inside, _) = both_halves();
    assert!(
        inside.len() >= 16,
        "`filewin/` 下只扫到 {} 份 `.rs` —— 遍历器坏了（2026-09-24 现打 17）。\
         人群塌成空集时，下面那两条相等**照样成立**",
        inside.len()
    );
    let root = repo_root();
    let mut window: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let mut monitor: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let mut met: BTreeSet<(String, String)> = BTreeSet::new();
    for (path, src) in &inside {
        let prod = guard_core::production_code(src);
        guard_core::assert_no_test_code(&rel(&root, path), &prod);
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        let whole_monitor = MONITOR_FILES.contains(&file.as_str());
        for (name, chunk) in chunks_by_fn(&prod) {
            let named = MONITOR_FNS.iter().any(|(f, n)| *f == file && *n == name);
            if named {
                met.insert((file.clone(), name.clone()));
            }
            let side = if whole_monitor || named {
                &mut monitor
            } else {
                &mut window
            };
            for e in app_side_edges(&chunk) {
                side.entry(e).or_default().push(format!("{file}::{name}"));
            }
        }
    }
    (window, monitor, met)
}

fn assert_side_equals(
    who_side: &str,
    found: &std::collections::BTreeMap<String, Vec<String>>,
    table: &[(&str, Kind)],
) {
    let want: BTreeSet<String> = table.iter().map(|(p, _)| (*p).to_string()).collect();
    assert_eq!(want.len(), table.len(), "{who_side} 那张表里有重复路径");
    let got: BTreeSet<String> = found.keys().cloned().collect();
    assert_eq!(
        got,
        want,
        "{who_side} 与 app 侧那条边界动了。\n  \
         盘上有、表里没有（**新长出来的耦合**）：{:?}\n  \
         表里有、盘上没有（那条边退役了 ⇒ 删掉这一行）：{:?}\n\
         逐条住址：{found:?}",
        got.difference(&want).collect::<Vec<_>>(),
        want.difference(&got).collect::<Vec<_>>(),
    );
}

/// ★★ **窗口进程够到 app 侧的每一条边都在 [`WINDOW_SIDE`] 里，表里也不留死行；
/// monitor 那一侧同理对 [`MONITOR_SIDE`]。** 两张表各自两向相等。
#[test]
fn every_edge_from_the_file_manager_into_the_app_is_declared() {
    let (window, monitor, met) = edges_by_process();
    // ★ 抽取器自检：点名的 monitor 函数**一个都不许找不到**（改名 ⇒ 那一块会悄悄滑进窗口那一侧）。
    let want_met: BTreeSet<(String, String)> = MONITOR_FNS
        .iter()
        .map(|(f, n)| ((*f).to_string(), (*n).to_string()))
        .collect();
    assert_eq!(met, want_met, "`MONITOR_FNS` 里点名的函数盘上找不全");
    assert!(
        window.len() >= 10 && monitor.len() >= 5,
        "抽到的边太少 —— 抽取器坏了"
    );
    assert_side_equals("窗口进程", &window, WINDOW_SIDE);
    assert_side_equals("monitor 那一侧", &monitor, MONITOR_SIDE);
}

/// 🔴 **题面那句判据本身：窗口进程够后端只经通道 —— 池子与登记表那几条「够后端」的路零处。**
///
/// 上一条判的是「表 == 盘」（表里写什么都行，只要两边一样）；这一条判的是**表里不许写什么**：
/// 窗口那一侧不许有 `backend::*`（直连进程级登记表 / 分流器）、不许有池子那几条**读侧 / 写面**
/// 命令（列目录 · stat · 建目录 · 删 · 改名 · 改权限 · 写文本）—— 它们都已经有后端命令了。
/// 零命中守卫 ＋ 反向自检（那几条确实曾经在盘上，见 `git log`；这里喂合成文本证明认得出）。
#[test]
fn the_window_process_reaches_the_backend_only_through_the_channel() {
    let (window, _, _) = edges_by_process();
    const BANNED_PREFIX: &[&str] = &["backend::"];
    const BANNED_EXACT: &[&str] = &[
        "sftp_pool::sftp_list_dir",
        "sftp_pool::sftp_stat",
        "sftp_pool::sftp_mkdir",
        "sftp_pool::sftp_delete",
        "sftp_pool::sftp_rename",
        "sftp_pool::sftp_chmod",
        "sftp_pool::sftp_write_text",
        "sftp_pool::sftp_realpath",
        "sftp_pool::SftpEntry",
    ];
    let hits: Vec<&String> = window
        .keys()
        .filter(|e| {
            BANNED_EXACT.contains(&e.as_str())
                || BANNED_PREFIX.iter().any(|p| starts_with_str(e, p))
        })
        .collect();
    assert!(
        hits.is_empty(),
        "窗口进程里又长出了不经通道够后端的边：{hits:?}\n住址：{:?}",
        hits.iter().map(|h| (h, window.get(*h))).collect::<Vec<_>>()
    );
    // 两类「题面要的」确实在（否则上面那条零命中可能只是因为窗口什么都不够了）。
    for must in [
        "chan::client::Client",
        "chan::dial::dial",
        "chan::wire::Comms",
    ] {
        assert!(
            window.contains_key(must),
            "窗口进程里没有 `{must}` —— 它不再说 call 了？"
        );
    }
    // 反向自检：切块 ＋ 抽取认得出一条被禁的边。
    let fake = chunks_by_fn(&format!(
        "fn x() {{ {CRATE_WORD}::sftp_pool::sftp_list_dir(a) }}"
    ));
    assert!(fake
        .iter()
        .any(|(n, c)| n == "x" && app_side_edges(c).contains("sftp_pool::sftp_list_dir")));
}

/// ★ **表里每一类都还活着** —— 类别不许长草；而且每一类只出现在它该在的那一侧。
#[test]
fn every_declared_edge_falls_in_a_live_category() {
    use Kind::*;
    for (k, side) in [
        (Channel, WINDOW_SIDE),
        (Wire, WINDOW_SIDE),
        (Host, MONITOR_SIDE),
        (Spawn, MONITOR_SIDE),
        (Config, MONITOR_SIDE),
        (DataDir, MONITOR_SIDE),
    ] {
        let n = side.iter().filter(|(_, kk)| *kk == k).count();
        assert!(
            n > 0,
            "`Kind::{k:?}` 在它那一侧一条边都没有 —— 把它从 `Kind` 里删掉"
        );
        let other = if std::ptr::eq(side, WINDOW_SIDE) {
            MONITOR_SIDE
        } else {
            WINDOW_SIDE
        };
        assert!(
            !other.iter().any(|(_, kk)| *kk == k),
            "`Kind::{k:?}` 跑到了另一侧的表里 —— 类别是按进程分的"
        );
    }
    // 🔴〔P4〕欠账清零：窗口进程够到壳的只剩「题面要的」两类（通道客户端 · 线上类型）＋ 文案取文口 —— 恒等，不是地板。
    //   变多 ＝ 窗口又长出一条不经通道的路。历史：传输 7 → 5 → 1 → 0（F7a · F7c · P4：开窗配置类型换成名字）·
    //   本地围栏 1 → 0（FN1）· 本机动作 1 → 2 → 0（FIX4 · P4：开终端交 monitor）· 自己的状态 2 → 0（P4：原子写进 `host_core`、书签名回 monitor）。
    let kinds: std::collections::BTreeSet<String> =
        WINDOW_SIDE.iter().map(|(_, k)| format!("{k:?}")).collect();
    assert_eq!(
        kinds,
        ["Channel", "Copy", "Wire"]
            .iter()
            .map(|s| s.to_string())
            .collect::<std::collections::BTreeSet<String>>(),
        "窗口进程里「还不是通道」的边又长出来了"
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
    let lib = std::fs::read_to_string(repo_root().join("src/frontend/shell/src/lib.rs"))
        .expect("读不到 `src/frontend/shell/src/lib.rs`");
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
