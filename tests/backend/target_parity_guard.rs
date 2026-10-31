//! **四个 target 横向对等**：帧面命令集与 CLI 子命令集在四个编译 target 上两向相等，不等的每一格在 [`TARGET_GAPS`] 有一行理由。
//!
//! 每个 target 上那一份从实现侧的声明现推：命令自己声明的码（`no_tmux` · `no_unix_mode`）× 那个 target 的平台档，
//! 与生产里填 `hello.unavailable` 的是同一对函数（[`crate::unavailable_from`] · [`crate::unix_mode_unavailable`]）。
//! 现推不读 [`TARGET_GAPS`]，两侧异源。另两条从实现那一侧反过来对声明：真够得着 tmux 的命令都声明了 `no_tmux`，
//! 真靠 tmux 的 ccm 能力都登记进了 `CCM_TMUX_CARRIED`（本文件后半）。
//!
//! # 买不到
//!
//! - 只看得见声明说得出来的差异：CLI 独有的子命令（如 `--tmux-notify`）没有逐 target 声明，判成处处都在。
//! - 判的是「做得到」这根轴的声明，不是「编得过」（门禁的 `winchk-backend` / `muslbuild`），更不是真机上跑得起来；
//!   macOS 一列是纯声明（没有产线、没有编译门禁）。

use crate::control::ccm::CCM_TMUX_CARRIED;
use crate::platform::tmux::{TmuxPlatform, TMUX_PLATFORM};
use std::collections::BTreeSet;

/// 四个编译 target（Linux musl / gnu · Windows · macOS）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    LinuxGnu,
    LinuxMusl,
    Windows,
    MacOs,
}

const TARGETS: [Target; 4] = [
    Target::LinuxGnu,
    Target::LinuxMusl,
    Target::Windows,
    Target::MacOs,
];

/// 那个 target 上 tmux 那一维的平台档（本机一格由正题对 `TMUX_PLATFORM` 钉，Windows 一格由下面的编译期断言钉）。
const fn tmux_platform_of(t: Target) -> TmuxPlatform {
    match t {
        Target::Windows => TmuxPlatform::AbsentUnlessExeOnPath,
        Target::LinuxGnu | Target::LinuxMusl | Target::MacOs => TmuxPlatform::AskThePath,
    }
}

/// 那个 target 上有没有 unix 权限位（`files-chmod` 的 `no_unix_mode` 看它）。
const fn unix_mode_bits_on(t: Target) -> bool {
    !matches!(t, Target::Windows)
}

#[cfg(windows)]
const _: () = assert!(
    tmux_platform_of(Target::Windows) as u8 == TMUX_PLATFORM as u8
        && !unix_mode_bits_on(Target::Windows),
    "Target 轴给 Windows 的档与这份 Windows 二进制编进去的对不上"
);

/// 等 Windows 后台机制（未定）的那一形：命令声明了 `no_tmux`，Windows 上没有 tmux。
const NO_TMUX_YET: &str =
    "这一版宿主只有 tmux（命令声明了 `no_tmux`）；Windows 后台机制选定后换实现、不换形状";
/// unix 权限位：Windows 上没有这套位（那边是 ACL ＋ 只读属性），这一条本身不跨过去。
const NO_MODE_BITS: &str =
    "改的是 unix 权限位（命令声明了 `no_unix_mode`）；Windows 上没有这套位，不跨过去";

/// 四个 target 之间的差异：`(面, 名, 哪个 target 上没有, 理由)`。
const TARGET_GAPS: &[(&str, &str, Target, &str)] = &[
    (
        "wire-commands",
        "terminal-preview",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "wire-commands",
        "terminal-follow",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "wire-commands",
        "terminal-input",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "wire-commands",
        "session-interrupt",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "cli-subcommands",
        "--session-interrupt",
        Target::Windows,
        NO_TMUX_YET,
    ),
    ("wire-commands", "set-plan", Target::Windows, NO_TMUX_YET),
    ("wire-commands", "set-launch", Target::Windows, NO_TMUX_YET),
    ("wire-commands", "plan-ask", Target::Windows, NO_TMUX_YET),
    (
        "cli-subcommands",
        "--plan-ask",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "cli-subcommands",
        "--set-plan",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "cli-subcommands",
        "--set-launch",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "wire-commands",
        "session-permission",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "wire-commands",
        "session-model",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "wire-commands",
        "files-chmod",
        Target::Windows,
        NO_MODE_BITS,
    ),
    (
        "cli-subcommands",
        "--terminal-preview",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "cli-subcommands",
        "--terminal-input",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "cli-subcommands",
        "--session-permission",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "cli-subcommands",
        "--session-model",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "cli-subcommands",
        "--files-chmod",
        Target::Windows,
        NO_MODE_BITS,
    ),
];

/// 那个 target 上做不到的帧命令名（生产里填 `hello.unavailable` 的同一对函数）。
fn unavailable_on(t: Target) -> Vec<String> {
    crate::unavailable_from(crate::platform::tmux::tmux_present(
        tmux_platform_of(t),
        None,
    ))
    .into_iter()
    .chain(crate::unix_mode_unavailable(unix_mode_bits_on(t)))
    .map(|u| u.command)
    .collect()
}

/// 那个 target 上的 `(面, 名)`：帧面 `inbound::REGISTRY` ＋ CLI 面 `SUBCOMMANDS`（经 `spec_for` 派生的跟着帧面那一条走）。
fn faces_on(t: Target) -> BTreeSet<(&'static str, &'static str)> {
    let gone = unavailable_on(t);
    let off = |name: &str| gone.iter().any(|g| g == name);
    let wire = crate::stream::inbound::REGISTRY
        .iter()
        .map(|s| s.name)
        .filter(|n| !off(n))
        .map(|n| ("wire-commands", n));
    let cli = crate::stream::cli::tests::subcommands()
        .iter()
        .copied()
        .filter(|f| crate::stream::cli::spec_for(f).is_none_or(|s| !off(s.name)))
        .map(|f| ("cli-subcommands", f));
    wire.chain(cli).collect()
}

/// 🔴 四个 target 的帧面命令集与 CLI 子命令集两向相等，差异 == [`TARGET_GAPS`]。
///
/// - 现推有、登记没有 ⇒ 有个 target 少了一条命令，没人说过为什么：补一行，或者回去改命令的 `codes`；
/// - 登记有、现推没有 ⇒ 已经做到了（删那一行），或者声明侧把这件事丢了。
#[test]
fn every_target_has_the_same_commands_except_the_registered_gaps() {
    // 本机一格：Target 轴给本机的档 == 本机真编进去的那一档（不然 Windows 那一列只是抄上去的）。
    let host = if cfg!(windows) {
        Target::Windows
    } else if cfg!(target_os = "macos") {
        Target::MacOs
    } else if cfg!(target_env = "musl") {
        Target::LinuxMusl
    } else {
        Target::LinuxGnu
    };
    assert_eq!(
        tmux_platform_of(host),
        TMUX_PLATFORM,
        "本机 {host:?}：Target 轴与 `TMUX_PLATFORM` 分叉了"
    );
    assert_eq!(
        unix_mode_bits_on(host),
        cfg!(unix),
        "本机 {host:?}：Target 轴与 `cfg(unix)` 分叉了"
    );
    assert_eq!(
        crate::control::files_write::WriteRefusal::Unsupported(String::new()).code(),
        crate::NO_UNIX_MODE,
        "写面回的那个码与现推读的那个码不是同一个串"
    );

    let per: Vec<(Target, BTreeSet<(&str, &str)>)> =
        TARGETS.iter().map(|&t| (t, faces_on(t))).collect();
    let union: BTreeSet<(&str, &str)> = per.iter().flat_map(|(_, s)| s.iter().copied()).collect();
    for face in ["wire-commands", "cli-subcommands"] {
        assert!(
            union.iter().any(|(f, _)| *f == face),
            "并集里没有 `{face}` 这一面 —— 人群缺了一块"
        );
    }
    let derived: BTreeSet<(&str, &str, Target)> = per
        .iter()
        .flat_map(|(t, here)| union.difference(here).map(move |&(f, n)| (f, n, *t)))
        .collect();
    let registered: BTreeSet<(&str, &str, Target)> =
        TARGET_GAPS.iter().map(|&(f, n, t, _)| (f, n, t)).collect();
    assert_eq!(
        registered.len(),
        TARGET_GAPS.len(),
        "`TARGET_GAPS` 里有重复的行"
    );
    let unexplained: Vec<_> = derived.difference(&registered).collect();
    let unwitnessed: Vec<_> = registered.difference(&derived).collect();
    assert!(
        unexplained.is_empty() && unwitnessed.is_empty(),
        "四个 target 的命令集对不上差异登记表。\n\
         现推有、登记没有（有个 target 少了一条，没人说过为什么）：{unexplained:?}\n\
         登记有、现推没有（登记说做不到，声明侧看不出差异）：{unwitnessed:?}"
    );
}

// ══════════════ 机制声明的反方向：从实现那一侧对声明 ══════════════
//
// 上面那条现推只读声明 —— 声明漏一格，那条命令在 Windows 上就被现推成「做得到」，而两向相等照样绿。下面两条补这一向：
// - **帧面**：`inbound::REGISTRY` 每条命令的处理器函数（`impl Handle for <类型>` 那一块），沿**函数级调用图**（[`fn_graph`]）
//   能不能走到一个**真起 `tmux`** 的函数（`Command::new("tmux")`，或经 `platform::shell::posix_shell` 起 shell 送 tmux 脚本的那一个）。
//   够得着的 == 声明 `no_tmux` 的 ∪ 登记的「够得着但 tmux 只是可选的」。
// - **ccm 面**：`control::ccm::CAPABILITIES` 每一条配一个最小探针 argv，经**真解析器 ＋ 真计划**（`argv::parse` → `plan::build`，纯、不起进程）
//   看它落在哪条路上：接回 / 容器 ⇒ 靠 tmux；直路 ⇒ 它在直路上必须**有效果**（计划与基线不同），否则它离了 tmux 什么都不做。
//   靠 tmux 的 ∪ 登记的例外 == `CCM_TMUX_CARRIED`。
//
// ⚠ 买不到（如实）：
// - 帧面的调用图按名字认、不认类型：方法 `.f(` 连本文件与它引用到的文件里每个 `…::f`，点名一个类型连它全部方法 ⇒ 多判不少判，
//   多出来的进登记表逐条写理由；值经别的线程的通道送过去（watcher 那一类）、经文件级引用图之外的路注入的调用看不见。
// - 帧面硬臂那一档（`typed::frame::<…>`）本体在分派的硬臂里，起点退成那个类型住的那份文件里的全部函数。
// - ccm 面判的是「落在哪条路上」与「直路上有没有效果」，**不是**「在 Windows 上真跑得起来」（真机维一格没有）；
//   平台原语那一层（读别的进程的环境）计划里看不见 —— `ccm-sid` 正是这一形，登记在例外表里。

/// 一棵源码树：相对路径 → 生产段（剥注释与测试段）。纯数据，判据与合成正控共用下面那几个函数。
type Tree = std::collections::BTreeMap<String, String>;

/// 后端 crate 的库那棵树（`main.rs` 是二进制根，不在库的模块树里 ⇒ 不收）。
fn backend_tree() -> Tree {
    // 走仓里唯一的遍历口径（`scanning_guard_registry` 那条纪律）；本文件住 `tests/backend/`，不在被扫的树里。
    let root = guard_core::addr::backend_src_root();
    guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .map(|(p, raw)| {
            // 人群声明带进来的中转 crate（`src/comms/outward/`）按模块住址认（`comms-outward/…`）。
            let rel = guard_core::module_address(&root, &p);
            (rel, guard_core::production_code(&raw))
        })
        .filter(|(rel, _)| rel != "main.rs")
        .collect()
}

/// `control/gate.rs` → `["control", "gate"]`；`files/mod.rs` → `["files"]`；`lib.rs` → `[]`。
fn module_of(rel: &str) -> Vec<String> {
    let mut segs: Vec<String> = rel
        .trim_end_matches(".rs")
        .split('/')
        .map(String::from)
        .collect();
    if matches!(segs.last().map(String::as_str), Some("mod") | Some("lib")) {
        segs.pop();
    }
    segs
}

/// 一条路径的最长模块前缀落在哪份文件。
fn file_of(
    mods: &std::collections::BTreeMap<Vec<String>, String>,
    path: &[String],
) -> Option<String> {
    (1..=path.len())
        .rev()
        .find_map(|n| mods.get(&path[..n]).cloned())
}

/// `use a::{b, c::{d, e}};` 展开成 `a::b` · `a::c::d` · `a::c::e`（输入已去空白）。
fn expand_use(u: &str) -> Vec<String> {
    let Some(open) = u.find('{') else {
        return vec![u.to_string()];
    };
    let Some(close) = u.rfind('}').filter(|c| *c > open) else {
        return vec![u.to_string()];
    };
    let (pre, inner) = (&u[..open], &u[open + 1..close]);
    let mut parts = Vec::new();
    let (mut depth, mut cur) = (0i32, String::new());
    for c in inner.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
        .iter()
        .flat_map(|p| expand_use(&format!("{pre}{p}")))
        .collect()
}

/// `use` 那一段里每个 ` as 名字`（前后是空白 · 逗号 · 花括号）摘掉：`crate::g::calm as c` → `crate::g::calm`。
fn drop_use_aliases(u: &str) -> String {
    let toks: Vec<&str> = u
        .split_inclusive(|c: char| c.is_whitespace() || c == ',' || c == '{' || c == '}')
        .collect();
    let mut out = String::new();
    let mut i = 0;
    while i < toks.len() {
        let t = toks[i];
        if t.trim_end() == "as" && t.ends_with(char::is_whitespace) {
            // 跳过 `as` 与它后面那个名字（空白 token 一并跳过），保留名字后面的分隔符。
            i += 1;
            while i < toks.len() && toks[i].trim().is_empty() {
                i += 1;
            }
            if let Some(name) = toks.get(i) {
                let sep: String = name
                    .chars()
                    .rev()
                    .take_while(|c| *c == ',' || *c == '{' || *c == '}')
                    .collect();
                out.push_str(&sep);
            }
            i += 1;
            continue;
        }
        out.push_str(t);
        i += 1;
    }
    out
}

/// 一份文件的生产段里提到的路径（`crate::…` / `super::…` / `self::…` / 本文件 `mod x;` 声明的子模块 `x::…`），
/// 以及 `use` 语句展开后的每一条。返回解析成绝对模块路径之后的段列表。
fn paths_in(me: &[String], src: &str) -> Vec<Vec<String>> {
    let children: std::collections::BTreeSet<String> = src
        .split("mod ")
        .skip(1)
        .filter_map(|t| {
            let name: String = t
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            t[name.len()..]
                .trim_start()
                .starts_with(';')
                .then_some(name)
        })
        .collect();
    let absolutize = |segs: Vec<String>| -> Option<Vec<String>> {
        let first = segs.first()?.as_str();
        match first {
            "crate" => Some(segs[1..].to_vec()),
            "super" | "self" => {
                let mut base = me.to_vec();
                let mut i = 0;
                while i < segs.len() && (segs[i] == "super" || segs[i] == "self") {
                    if segs[i] == "super" {
                        base.pop();
                    }
                    i += 1;
                }
                base.extend(segs[i..].iter().cloned());
                Some(base)
            }
            c if children.contains(c) => {
                let mut base = me.to_vec();
                base.extend(segs.iter().cloned());
                Some(base)
            }
            _ => None,
        }
    };
    let mut out = Vec::new();
    // ① `use …;` 展开（花括号分组）。只认语句开头的 `use`（前一个字符是空白 / `;` / `{` / `}` / `)`），
    //    而且 `;` 之前只许是路径字符 —— 字符串里一句 `… use …` 的散文不算。
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(off) = src[from..].find("use ") {
        let at = from + off;
        from = at + 4;
        if at > 0
            && !matches!(
                bytes[at - 1],
                b' ' | b'\n' | b'\t' | b';' | b'{' | b'}' | b')'
            )
        {
            continue;
        }
        let Some(end) = src[at + 4..].find(';') else {
            break;
        };
        // `a::b as c`：别名不改指向 ⇒ 先把 ` as 名字` 摘掉再去空白（不然拼成 `bas c`，指到父模块或指不到）。
        let u: String = drop_use_aliases(&src[at + 4..at + 4 + end])
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if u.is_empty()
            || !u.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            || !u
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_:{},*".contains(c))
        {
            continue;
        }
        for e in expand_use(&u) {
            let segs: Vec<String> = e
                .split("::")
                .filter(|s| !s.is_empty() && *s != "*")
                .map(String::from)
                .collect();
            if let Some(p) = absolutize(segs) {
                out.push(p);
            }
        }
    }
    // ② 代码里的 `a::b::c` 链。
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let start_ok = c.is_ascii_alphabetic()
            && (i == 0
                || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b':'));
        if !start_ok {
            i += 1;
            continue;
        }
        let mut segs = Vec::new();
        let mut j = i;
        loop {
            let s = j;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            segs.push(src[s..j].to_string());
            if j + 2 < b.len()
                && b[j] == b':'
                && b[j + 1] == b':'
                && (b[j + 2].is_ascii_alphabetic() || b[j + 2] == b'_')
            {
                j += 2;
                continue;
            }
            break;
        }
        // 紧跟 `(` 的最后一段是函数名，不是模块：摘掉再找文件（不然文件内联模块里的 `super::f()` 会被读成顶层模块 `f`）。
        let chain = segs.len() > 1;
        if chain && b.get(j) == Some(&b'(') {
            segs.pop();
        }
        if chain {
            if let Some(p) = absolutize(segs) {
                out.push(p);
            }
        }
        i = j.max(i + 1);
    }
    out
}

/// 文件级引用图：文件 → 它引用的别的文件。
fn file_edges(
    tree: &Tree,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    let mods: std::collections::BTreeMap<Vec<String>, String> =
        tree.keys().map(|r| (module_of(r), r.clone())).collect();
    tree.iter()
        .map(|(rel, src)| {
            let me = module_of(rel);
            let to = paths_in(&me, src)
                .into_iter()
                .filter_map(|p| file_of(&mods, &p))
                .filter(|f| f != rel)
                .collect();
            (rel.clone(), to)
        })
        .collect()
}

/// 真起 `tmux` 的生产文件：直呼 `Command::new("tmux")`，或经 shell 适配口送一段带 `tmux ` 的脚本。
fn tmux_spawning_files(tree: &Tree) -> std::collections::BTreeSet<String> {
    tree.iter()
        .filter(|(_, src)| {
            let squeezed: String = src.chars().filter(|c| !c.is_whitespace()).collect();
            squeezed.contains("Child::new(\"tmux\")")
                || (src.contains("posix_shell(") && src.contains("tmux "))
        })
        .map(|(r, _)| r.clone())
        .collect()
}

/// 从一组起点文件沿引用图走完，交回够得着的文件（含起点）。
fn reachable(
    edges: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    from: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeSet<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut todo: Vec<String> = from.iter().cloned().collect();
    while let Some(f) = todo.pop() {
        if seen.insert(f.clone()) {
            todo.extend(edges.get(&f).into_iter().flatten().cloned());
        }
    }
    seen
}

/// 一个函数的住址：`(文件, 名)`；`impl` / `trait` 块里的写成 `类型::名`。同一份文件里同一个住址的几个并成一个（多判不少判）。
type FnAt = (String, String);

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 字符串 · 字符字面量的内容抹成空白（引号留着，长度不变）：找花括号与路径时不被字面量里的 `{` · `::` 骗。
fn masked(src: &[char]) -> Vec<char> {
    let mut out = src.to_vec();
    let n = src.len();
    let mut i = 0;
    while i < n {
        let c = src[i];
        let fresh = i == 0 || !is_ident(src[i - 1]);
        let raw_at = match c {
            'r' if fresh => Some(i + 1),
            'b' if fresh && src.get(i + 1) == Some(&'r') => Some(i + 2),
            _ => None,
        };
        if let Some(mut j) = raw_at {
            let mut hashes = 0;
            while j < n && src[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && src[j] == '"' {
                let mut k = j + 1;
                while k < n && !(src[k] == '"' && (1..=hashes).all(|h| src.get(k + h) == Some(&'#'))) {
                    k += 1;
                }
                out[j + 1..k.min(n)].fill(' ');
                i = k + 1 + hashes;
                continue;
            }
        }
        if c == '"' {
            let mut k = i + 1;
            while k < n && src[k] != '"' {
                k += if src[k] == '\\' { 2 } else { 1 };
            }
            out[i + 1..k.min(n)].fill(' ');
            i = k + 1;
            continue;
        }
        if c == '\'' {
            if src.get(i + 1) == Some(&'\\') {
                let mut k = i + 3;
                while k < n && src[k] != '\'' {
                    k += 1;
                }
                out[i + 1..k.min(n)].fill(' ');
                i = k + 1;
                continue;
            }
            if src.get(i + 2) == Some(&'\'') {
                out[i + 1] = ' ';
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// `m[i..]` 以整词 `w` 打头。
fn word_at(m: &[char], i: usize, w: &str) -> bool {
    let w: Vec<char> = w.chars().collect();
    m.get(i..i + w.len()) == Some(&w[..])
        && (i == 0 || !is_ident(m[i - 1]))
        && !m.get(i + w.len()).copied().is_some_and(is_ident)
}

/// `m[open]` 是 `{`：交回与它配对的 `}`（没收尾 ⇒ 末尾）。
fn match_brace(m: &[char], open: usize) -> usize {
    let mut depth = 0i32;
    for (k, c) in m.iter().enumerate().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    m.len().saturating_sub(1)
}

/// `impl` / `trait` 头（关键字之后到 `{` 之前）里那个类型的名字：尖括号里的去掉，有 ` for ` 取它后面那一个，路径取末段。
fn owner_of(header: &str, is_impl: bool) -> String {
    let h: Vec<char> = header.chars().collect();
    let mut top = String::new();
    let mut depth = 0i32;
    let mut i = 0;
    while i < h.len() {
        match h[i] {
            '-' if h.get(i + 1) == Some(&'>') => i += 1,
            '<' => depth += 1,
            '>' => depth -= 1,
            c if depth == 0 => top.push(c),
            _ => {}
        }
        i += 1;
    }
    let after = if is_impl {
        top.rsplit(" for ").next().unwrap_or(&top)
    } else {
        &top
    };
    let path: String = after
        .trim_start()
        .trim_start_matches(['&', ' '])
        .trim_start_matches("dyn ")
        .chars()
        .take_while(|c| is_ident(*c) || *c == ':')
        .collect();
    path.rsplit("::").next().unwrap_or("").to_string()
}

/// 一份文件里的函数（住址名 · 原文 · 抹过字面量的那一份）与 `impl` 头（类型名 · 头那一段），按出现先后。
#[derive(Default)]
struct FileFns {
    fns: Vec<(String, String, String)>,
    impls: Vec<(String, String)>,
}

fn fns_in(src: &str) -> FileFns {
    let r: Vec<char> = src.chars().collect();
    let m = masked(&r);
    let mut out = FileFns::default();
    scan_items(&r, &m, 0, m.len(), None, &mut out);
    out
}

fn scan_items(r: &[char], m: &[char], from: usize, to: usize, owner: Option<&str>, out: &mut FileFns) {
    let mut i = from;
    while i < to {
        let is_impl = word_at(m, i, "impl");
        if is_impl || word_at(m, i, "trait") {
            let kw = if is_impl { 4 } else { 5 };
            let mut j = i + kw;
            while j < to && m[j] != '{' && m[j] != ';' {
                j += 1;
            }
            if j < to && m[j] == '{' {
                let header: String = m[i + kw..j].iter().collect();
                let ty = owner_of(&header, is_impl);
                let close = match_brace(m, j);
                out.impls.push((ty.clone(), header));
                scan_items(r, m, j + 1, close, Some(&ty), out);
                i = close + 1;
            } else {
                i = j + 1;
            }
            continue;
        }
        if word_at(m, i, "fn") {
            let mut j = i + 2;
            while j < to && m[j].is_whitespace() {
                j += 1;
            }
            let s = j;
            while j < to && is_ident(m[j]) {
                j += 1;
            }
            if j > s {
                let name: String = m[s..j].iter().collect();
                let mut depth = 0i32;
                let mut k = j;
                while k < to {
                    match m[k] {
                        '(' | '[' => depth += 1,
                        ')' | ']' => depth -= 1,
                        '{' | ';' if depth == 0 => break,
                        _ => {}
                    }
                    k += 1;
                }
                if k < to && m[k] == '{' {
                    let close = match_brace(m, k);
                    let key = owner.map_or(name.clone(), |t| format!("{t}::{name}"));
                    out.fns.push((key, r[k..=close].iter().collect(), m[k..=close].iter().collect()));
                    i = close + 1;
                } else {
                    i = k + 1;
                }
                continue;
            }
        }
        i += 1;
    }
}

/// 函数级调用图：每个函数 → 它体里叫到 / 点名的函数。
/// - `a::b::f` 路径（`crate::` · `super::` · `self::` · 本文件声明的子模块 · `use` 进来的名字打头）⇒ 那份文件里的 `f` / `T::f`；
///   那份文件里没有（再导出）⇒ 它引用到的文件里的同名函数。
/// - 光一个名字 `f` ⇒ 本文件的自由函数 `f`，或 `use …::*` 进来的那一个；`T::f`（`T` 不是模块）⇒ 本文件与它引用到的文件里的 `T::f`。
/// - 方法 `.f(` 与泛型参数 `C::f(` ⇒ 本文件与它引用到的文件里任一 `…::f`。
/// - 读一张常量 / 静态表不算叫了表里的闭包。
///
/// 每条边都落在本文件或它在文件级引用图（[`file_edges`]）上引用到的文件里：够得着的只会比文件级少。
struct FnGraph {
    bodies: std::collections::BTreeMap<FnAt, String>,
    edges: std::collections::BTreeMap<FnAt, std::collections::BTreeSet<FnAt>>,
    impls: std::collections::BTreeMap<String, Vec<(String, String)>>,
}

/// 一份文件的模块住址 · 本文件声明的子模块 · `use` 进来的名字（别名照别名）→ 绝对路径 · `use …::*` 的那几个模块。
struct UseContext {
    me: Vec<String>,
    children: std::collections::BTreeSet<String>,
    uses: std::collections::BTreeMap<String, Vec<String>>,
    globs: Vec<Vec<String>>,
}

/// `crate::` · `super::` · `self::` · 本文件声明的子模块打头的路径 ⇒ 绝对模块路径。
fn absolutize_in(me: &[String], children: &std::collections::BTreeSet<String>, segs: &[String]) -> Option<Vec<String>> {
    match segs.first()?.as_str() {
        "crate" => Some(segs[1..].to_vec()),
        "super" | "self" => {
            let mut base = me.to_vec();
            let mut i = 0;
            while i < segs.len() && (segs[i] == "super" || segs[i] == "self") {
                if segs[i] == "super" {
                    base.pop();
                }
                i += 1;
            }
            base.extend(segs[i..].iter().cloned());
            Some(base)
        }
        c if children.contains(c) => Some(me.iter().chain(segs.iter()).cloned().collect()),
        _ => None,
    }
}

fn use_context(rel: &str, src: &str) -> UseContext {
    let me = module_of(rel);
    let children: std::collections::BTreeSet<String> = src
        .split("mod ")
        .skip(1)
        .filter_map(|t| {
            let name: String = t.chars().take_while(|c| is_ident(*c)).collect();
            t[name.len()..].trim_start().starts_with(';').then_some(name)
        })
        .collect();
    let mut uses = std::collections::BTreeMap::new();
    let mut globs = Vec::new();
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(off) = src[from..].find("use ") {
        let at = from + off;
        from = at + 4;
        if at > 0 && !matches!(bytes[at - 1], b' ' | b'\n' | b'\t' | b';' | b'{' | b'}' | b')') {
            continue;
        }
        let Some(end) = src[at + 4..].find(';') else {
            break;
        };
        let joined: String = src[at + 4..at + 4 + end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace(" as ", "@")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if !joined.chars().all(|c| c.is_ascii_alphanumeric() || "_:{},*@".contains(c)) {
            continue;
        }
        for e in expand_use(&joined) {
            let (path, alias) = match e.split_once('@') {
                Some((p, a)) => (p.to_string(), Some(a.to_string())),
                None => (e.clone(), None),
            };
            let mut segs: Vec<String> =
                path.split("::").filter(|s| !s.is_empty()).map(String::from).collect();
            let glob = segs.last().is_some_and(|s| s == "*");
            if glob || segs.last().is_some_and(|s| s == "self") {
                segs.pop();
            }
            let Some(abs) = absolutize_in(&me, &children, &segs) else {
                continue;
            };
            if glob {
                globs.push(abs);
            } else if let Some(name) = alias.or_else(|| abs.last().cloned()) {
                uses.insert(name, abs);
            }
        }
    }
    UseContext { me, children, uses, globs }
}

fn fn_graph(tree: &Tree) -> FnGraph {
    use std::collections::{BTreeMap, BTreeSet};
    let mods: BTreeMap<Vec<String>, String> =
        tree.keys().map(|r| (module_of(r), r.clone())).collect();
    let file_edges = file_edges(tree);
    let per_file: BTreeMap<String, FileFns> =
        tree.iter().map(|(rel, src)| (rel.clone(), fns_in(src))).collect();
    // 文件 → 末段名 → 住址（自由函数的末段名就是它自己）。
    let mut by_name: BTreeMap<String, BTreeMap<String, Vec<FnAt>>> = BTreeMap::new();
    // 文件 → 类型名 → 它 `impl` 块里的函数（点名一个类型 ⇒ 它的方法都可能被叫：经 trait 注入的那一形）。
    let mut by_owner: BTreeMap<String, BTreeMap<String, Vec<FnAt>>> = BTreeMap::new();
    let mut bodies: BTreeMap<FnAt, String> = BTreeMap::new();
    let mut masked_bodies: BTreeMap<FnAt, String> = BTreeMap::new();
    for (rel, f) in &per_file {
        for (key, raw, mk) in &f.fns {
            let at: FnAt = (rel.clone(), key.clone());
            let last = key.rsplit("::").next().unwrap_or(key).to_string();
            let slot = by_name.entry(rel.clone()).or_default().entry(last).or_default();
            if !slot.contains(&at) {
                slot.push(at.clone());
            }
            if let Some((owner, _)) = key.rsplit_once("::") {
                let slot = by_owner.entry(rel.clone()).or_default().entry(owner.to_string()).or_default();
                if !slot.contains(&at) {
                    slot.push(at.clone());
                }
            }
            bodies.entry(at.clone()).or_default().push_str(raw);
            masked_bodies.entry(at).or_default().push_str(mk);
        }
    }
    let empty = BTreeSet::new();
    // 在这几份文件里按末段名找：`owned` ⇒ 只要 `T::f`（给了 `T` 就只要那一个类型的）；否则只要自由函数。
    let find = |files: &[String], name: &str, owner: Option<&str>, owned: bool| -> Vec<FnAt> {
        files
            .iter()
            .filter_map(|f| by_name.get(f).and_then(|m| m.get(name)))
            .flatten()
            .filter(|(_, k)| match (owned, owner) {
                (false, _) => !k.contains("::"),
                (true, None) => k.contains("::"),
                (true, Some(t)) => {
                    k.strip_suffix(name).and_then(|p| p.strip_suffix("::")) == Some(t)
                }
            })
            .cloned()
            .collect()
    };
    // 点名一个类型（大写打头）⇒ 这几份文件里它 `impl` 块的全部函数（值交给泛型 / trait 对象之后谁叫哪个看不见，全算上）。
    let owned_by = |files: &[String], t: &str| -> Vec<FnAt> {
        if !t.starts_with(|c: char| c.is_ascii_uppercase()) {
            return Vec::new();
        }
        files
            .iter()
            .filter_map(|f| by_owner.get(f).and_then(|m| m.get(t)))
            .flatten()
            .cloned()
            .collect()
    };
    // 绝对路径 ⇒ 函数：最长的模块前缀定文件，剩下的是 `f` 或 `T::f`。
    let resolve = |p: &[String]| -> Vec<FnAt> {
        let Some((n, g)) = (0..=p.len())
            .rev()
            .find_map(|n| mods.get(&p[..n]).map(|g| (n, g.clone())))
        else {
            return Vec::new();
        };
        let here = vec![g.clone()];
        let hop: Vec<String> = file_edges.get(&g).unwrap_or(&empty).iter().cloned().collect();
        match &p[n..] {
            [] => Vec::new(),
            [f] => {
                let mut hit = find(&here, f, None, false);
                hit.extend(owned_by(&here, f));
                if hit.is_empty() {
                    hit = find(&hop, f, None, false);
                    hit.extend(owned_by(&hop, f));
                }
                hit
            }
            [.., t, f] => {
                let mut hit = find(&here, f, Some(t), true);
                hit.extend(find(&here, f, None, false));
                if hit.is_empty() {
                    find(&hop, f, Some(t), true)
                } else {
                    hit
                }
            }
        }
    };
    let ctx: BTreeMap<&String, UseContext> =
        tree.iter().map(|(rel, src)| (rel, use_context(rel, src))).collect();
    let mut edges: BTreeMap<FnAt, BTreeSet<FnAt>> = BTreeMap::new();
    for (at, body) in &masked_bodies {
        let rel = &at.0;
        let cx = &ctx[rel];
        let here = vec![rel.clone()];
        let near: Vec<String> = here
            .iter()
            .cloned()
            .chain(file_edges.get(rel).unwrap_or(&empty).iter().cloned())
            .collect();
        let b: Vec<char> = body.chars().collect();
        let mut to = BTreeSet::new();
        let mut i = 0;
        while i < b.len() {
            let start_ok = (b[i].is_alphabetic() || b[i] == '_')
                && (i == 0 || !(is_ident(b[i - 1]) || b[i - 1] == ':'));
            if !start_ok {
                i += 1;
                continue;
            }
            let mut segs: Vec<String> = Vec::new();
            let mut j = i;
            loop {
                let s = j;
                while j < b.len() && is_ident(b[j]) {
                    j += 1;
                }
                segs.push(b[s..j].iter().collect());
                if b.get(j) == Some(&':')
                    && b.get(j + 1) == Some(&':')
                    && b.get(j + 2).is_some_and(|c| c.is_alphabetic() || *c == '_')
                {
                    j += 2;
                    continue;
                }
                break;
            }
            let prev = b[..i].iter().rev().find(|c| !c.is_whitespace()).copied();
            let next = b[j..].iter().find(|c| !c.is_whitespace()).copied();
            let call = next == Some('(') || (b.get(j) == Some(&':') && b.get(j + 2) == Some(&'<'));
            i = j.max(i + 1);
            if next == Some('!') {
                continue;
            }
            if prev == Some('.') {
                if call && segs.len() == 1 {
                    to.extend(find(&near, &segs[0], None, true));
                }
                continue;
            }
            let abs = absolutize_in(&cx.me, &cx.children, &segs).or_else(|| {
                cx.uses
                    .get(&segs[0])
                    .map(|u| u.iter().chain(segs[1..].iter()).cloned().collect())
            });
            if let Some(p) = abs {
                to.extend(resolve(&p));
                continue;
            }
            match segs.as_slice() {
                [f] => {
                    to.extend(find(&here, f, None, false));
                    to.extend(owned_by(&near, f));
                    for g in &cx.globs {
                        let p: Vec<String> = g.iter().chain(std::iter::once(f)).cloned().collect();
                        to.extend(resolve(&p));
                    }
                }
                [t, f] if t == "Self" => to.extend(find(&here, f, None, true)),
                [t, f] if call && t.len() == 1 && t.chars().all(|c| c.is_ascii_uppercase()) => {
                    to.extend(find(&near, f, None, true));
                }
                [.., t, f] if call => {
                    to.extend(find(&near, f, Some(t), true));
                    for g in &cx.globs {
                        let p: Vec<String> = g.iter().chain(segs.iter()).cloned().collect();
                        to.extend(resolve(&p));
                    }
                }
                _ => {}
            }
        }
        to.remove(at);
        edges.insert(at.clone(), to);
    }
    let impls = per_file.into_iter().map(|(rel, f)| (rel, f.impls)).collect();
    FnGraph { bodies, edges, impls }
}

/// 真起 tmux 的函数：[`tmux_spawning_files`] 那几份里，体里直呼 `Child::new("tmux")` 或经 `posix_shell(` 起 shell 的那几个。
fn tmux_spawning_fns(tree: &Tree, g: &FnGraph) -> std::collections::BTreeSet<FnAt> {
    let files = tmux_spawning_files(tree);
    g.bodies
        .iter()
        .filter(|((rel, _), _)| files.contains(rel))
        .filter(|(_, body)| {
            let squeezed: String = body.chars().filter(|c| !c.is_whitespace()).collect();
            squeezed.contains("Child::new(\"tmux\")") || body.contains("posix_shell(")
        })
        .map(|(at, _)| at.clone())
        .collect()
}

/// 从一组起点函数沿调用图走完，交回够得着的函数（含起点）。
fn reachable_fns(
    g: &FnGraph,
    from: &std::collections::BTreeSet<FnAt>,
) -> std::collections::BTreeSet<FnAt> {
    let mut seen = std::collections::BTreeSet::new();
    let mut todo: Vec<FnAt> = from.iter().cloned().collect();
    while let Some(f) = todo.pop() {
        if seen.insert(f.clone()) {
            todo.extend(g.edges.get(&f).into_iter().flatten().cloned());
        }
    }
    seen
}

/// 命令名 → 它的处理器函数（起点）。命令表各族（`stream/inbound/registry/*.rs`）里 typed 那几行
/// （`typed::blocking::<路径>()` · `typed::asynchronous::<路径>()` · 帧面硬臂那一档 `typed::frame::<路径>()`）点名一个类型，
/// 命令名从那个类型住的文件里它的 `impl Command for <类型>` 的 `const NAME` 读（异源：不读 `REGISTRY`）。
/// 起点：`impl … Handle for <类型>` 那一块里的函数（住哪份文件都算）；帧面硬臂那几条没有 `Handle`（本体在分派的硬臂里），
/// 起点退成那个类型住的那份文件里的全部函数。
fn handler_fns(
    tree: &Tree,
    g: &FnGraph,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<FnAt>> {
    let families: Vec<&String> = tree
        .iter()
        .filter(|(rel, _)| rel.starts_with("stream/inbound/registry/"))
        .map(|(_, src)| src)
        .collect();
    assert!(
        tree.iter()
            .any(|(rel, _)| rel == "stream/inbound/registry/bus.rs"),
        "后端树里没找到命令表族文件 `bus.rs`（找到 {} 份族文件）—— 遍历坏了",
        families.len()
    );
    let mods: std::collections::BTreeMap<Vec<String>, String> =
        tree.keys().map(|r| (module_of(r), r.clone())).collect();
    let mut out = std::collections::BTreeMap::new();
    for src in families {
        let at = src
            .find("const SPECS: &[CommandSpec] = &[")
            .expect("命令表族文件里锚不住 `SPECS`");
        let table = &src[at..];
        for ctor in [
            "typed::blocking::<",
            "typed::asynchronous::<",
            "typed::frame::<",
            "typed::frame_blocking::<",
        ] {
            for (at, _) in table.match_indices(ctor) {
                let rest = &table[at + ctor.len()..];
                let path: Vec<String> = rest[..rest.find('>').expect("typed 那一行没收尾")]
                    .split(':')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && s != "crate")
                    .collect();
                let ty = path.last().expect("typed 那一行没有类型名").clone();
                let file = file_of(&mods, &path)
                    .unwrap_or_else(|| panic!("typed 那一行的 `{ty}` 找不到它住的文件"));
                let name = guard_core::typed_command_name(&tree[&file], &ty)
                    .unwrap_or_else(|| panic!("`{file}` 里没有 `impl Command for {ty}`"));
                let handle_files: Vec<&String> = g
                    .impls
                    .iter()
                    .filter(|(_, hs)| {
                        hs.iter().any(|(t, h)| {
                            *t == ty
                                && h.split(" for ").next().is_some_and(|tr| tr.trim_end().ends_with("Handle"))
                        })
                    })
                    .map(|(rel, _)| rel)
                    .collect();
                let prefix = format!("{ty}::");
                let roots: std::collections::BTreeSet<FnAt> = g
                    .bodies
                    .keys()
                    .filter(|(rel, k)| {
                        if handle_files.is_empty() {
                            *rel == file
                        } else {
                            handle_files.contains(&rel) && k.starts_with(&prefix)
                        }
                    })
                    .cloned()
                    .collect();
                out.insert(name, roots);
            }
        }
    }
    out
}

/// 够得着 tmux、却**不**声明 `no_tmux` 的命令 —— 逐条写理由（tmux 在它那里是可选的：问不到就降级，命令本身照做）。
const REACHES_TMUX_WITHOUT_NO_TMUX: &[(&str, &str)] = &[
    (
        "terminal-end",
        "按名单里的句柄结束：名单两种宿主合一份，没 tmux ⇒ 只剩托管那几行，托管的照结束（托管口挂断子进程组，不碰 tmux）；\
         tmux 那一行过三道门，过门那一刻 tmux 起不来 ⇒ 落 `kill_failed`",
    ),
    (
        "terminal-take-input",
        "托管终端的接管：只认托管那一行（经托管口），对上 tmux 那一行才列一次名单说 `not_managed`；没 tmux ⇒ 照做",
    ),
    ("terminal-release-input", "同上（交还）"),
    (
        "session-window",
        "↗ 那一问：本机会话在 tmux 里才问 tmux 谁连着；不在 tmux 里（Windows 恒是）⇒ 链从会话进程自己起，命令照答",
    ),
    (
        "terminal-follow-ack",
        "终端实时预览：帧面硬臂那一档，起点是 `control/terminal_follow.rs` 整份（与 `terminal-follow` 同住）；它自己只往订阅线程送一个回执、不起 tmux",
    ),
    (
        "terminal-unfollow",
        "同上，只摘票（订阅线程自己收那个只读客户端）、不起 tmux",
    ),
    (
        "chores-merge",
        "几台的文件与数据：本机那一台就是 `data-report` 那一份（同一个降级：没 tmux ⇒ `tmux: false`）",
    ),
    (
        "data-report",
        "文件与数据：`tmux` 那一格问这台有没有 tmux（与起新会话框同一个判法），没 tmux ⇒ `tmux: false`、别的几格照答",
    ),
    (
        "session-open",
        "起会话 · 恢复 · 接回：要 tmux 而这台没有 ⇒ `place_unavailable`（不改道），直路本来用不着 tmux（Windows 本机照做）",
    ),
    (
        "session-arrive",
        "等起好的会话报到：认报到不碰 tmux；没见到时才抓起在的那个 tmux 会话的一屏当原话，没 tmux ⇒ 那一句只说没见到（读不到画面）",
    ),
    (
        "session-open-facts",
        "起新会话框的这台事实：没 tmux ⇒ `tmux: false`、只剩开终端那一形，别的几格照答",
    ),
    (
        "session-open-dir",
        "目录那一问：没 tmux ⇒ `tmuxName: null`（没有名字可铸），目录在不在照答",
    ),
    (
        "terminals-list",
        "终端名单：没 tmux ⇒ 这台没有 tmux 终端、答空名单（不是错）；托管终端做出来后名单里还有它们",
    ),
    (
        "terminals-board-here",
        "「终端」视图这台那几行：照 `terminals-list` 那一条（没 tmux ⇒ 空的那几行，不是错）",
    ),
    (
        "terminals-board",
        "「终端」视图那张名单：各台照 `terminals-board-here`（没 tmux 的那台是空的一组；没答的那台进那一组的 `down`，命令照答）",
    ),
    (
        "sessions-where",
        "问这几个会话在哪个 tmux 会话里：没 tmux ⇒ 逐个答「这台没 tmux」、命令照答",
    ),
    (
        "session-fork",
        "分叉：回复里「源会话在哪个终端」那一格读终端名单；没 tmux ⇒ 当不在任何终端里，分叉照做",
    ),
    (
        "session-restart",
        "换号重启：没 tmux ⇒ 这条会话不在任何终端里，答 `not_in_terminal`（与「名单里没有它」同一个码）",
    ),
    (
        "sessions-stop",
        "批量停：没 tmux ⇒ 逐个答「这台没 tmux」跳过、命令照答（停的对象本来就只在 tmux 里）",
    ),
    (
        "profiles-resolve",
        "设置窗合并预览的「等于」那一行：与 `ccm-print` 同一个 `control/ccm/mod.rs::plan_of`，经会话快照做铸名避让 —— \
         问不到 ⇒ 不退让、照算；它的能力是合并与渲计划，不是 tmux",
    ),
    (
        "profiles-read",
        "配置文件整份：`tmux` 那一格问这台有没有 tmux（`faces/profiles_face.rs` 交 `control::terminals::tmux_here`），没 tmux ⇒ `tmux: false`、别的几格照答",
    ),
    (
        "ccm-print",
        "〔合并主线时本条当场点出〕W5-ALIAS 的别名预览：`control/ccm/mod.rs::plan_of` 经 \
         `host::snapshot().taken_names().ok()` 问一次会话快照做铸名避让 —— 问不到 ⇒ `None` ⇒ 不退让\
         （`plan::build` 头注的诚实降级），预览照出；它的能力是渲计划，不是 tmux",
    ),
    (
        "session-terminals",
        "tmux 只在那个会话本来就在 tmux 里时被问（`list-clients`，socket 取自那个进程自己的 `TMUX`）；\
         不在 tmux 里的会话读进程自己的环境，整条链上没有 tmux ⇒ 不是必需品，不声明 `no_tmux`",
    ),
];

/// 🔴 帧面：够得着 tmux 的命令 == 声明 `no_tmux` 的 ∪ 登记的「tmux 可选」。
#[test]
fn every_frame_command_that_can_reach_tmux_declares_no_tmux() {
    use std::collections::BTreeSet;
    let tree = backend_tree();
    assert!(
        tree.iter().any(|(rel, _)| rel == "lib.rs"),
        "后端树里没有 `lib.rs`（收到 {} 份）—— 遍历坏了，下面全称恒真",
        tree.len()
    );
    let graph = fn_graph(&tree);
    let spawners = tmux_spawning_fns(&tree, &graph);
    // 只防人群塌成空集；是哪一份文件由 `tmux_is_spawned_only_in_the_host_tmux_file` 钉，主锚是下面两向相等。
    assert!(
        spawners.iter().any(|(rel, k)| rel == "host/tmux.rs" && k == "child"),
        "真起 tmux 的函数里没有 `host/tmux.rs` 的 `child` —— 取法坏了：{spawners:?}"
    );
    let handlers = handler_fns(&tree, &graph);
    let names: BTreeSet<&str> = crate::stream::inbound::REGISTRY
        .iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(
        handlers.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        names,
        "从源码里抽到的命令名与 `REGISTRY` 不一致 —— 抽块坏了"
    );
    let unresolved: BTreeSet<&str> = handlers
        .iter()
        .filter(|(_, f)| f.is_empty())
        .map(|(n, _)| n.as_str())
        .collect();
    assert!(
        unresolved.is_empty(),
        "这几条命令的本体住哪抽不出来（登记行里点不出类型）—— 它够不够得着 tmux，本条看不见：{unresolved:?}"
    );
    let reach: BTreeSet<&str> = handlers
        .iter()
        .filter(|(_, from)| !reachable_fns(&graph, from).is_disjoint(&spawners))
        .map(|(n, _)| n.as_str())
        .collect();
    let declared: BTreeSet<&str> = crate::stream::inbound::REGISTRY
        .iter()
        .filter(|s| s.codes.contains(&crate::NO_TMUX))
        .map(|s| s.name)
        .collect();
    let excused: BTreeSet<&str> = REACHES_TMUX_WITHOUT_NO_TMUX
        .iter()
        .map(|(n, _)| *n)
        .collect();
    assert!(
        declared.is_disjoint(&excused),
        "登记成「tmux 可选」的命令又声明了 `no_tmux`：{:?} —— 两句话自相矛盾，删一句",
        declared.intersection(&excused).collect::<Vec<_>>()
    );
    let want: BTreeSet<&str> = declared.union(&excused).copied().collect();
    let undeclared: Vec<_> = reach.difference(&want).collect();
    let ghosts: Vec<_> = want.difference(&reach).collect();
    assert!(
        undeclared.is_empty() && ghosts.is_empty(),
        "\n帧面「真够得着 tmux」与声明对不上。\n\
         ① 够得着、却既没声明 `no_tmux` 也没登记：{undeclared:?}\n\
            ⇒ 🔴 它在 Windows 上会被现推成「做得到」（现推只读声明）。tmux 是它的必需品 ⇒ 在 `codes` 里声明 `no_tmux`；\
         只是可选的（问不到就降级、命令照做）⇒ 登记进 `REACHES_TMUX_WITHOUT_NO_TMUX` 写清降级成什么。\n\
         ② 声明了 / 登记了、却根本够不着：{ghosts:?}\n\
            ⇒ 声明是鬼影（它会把一条在 Windows 上做得到的命令报成做不到），或者引用图取法坏了。\n\
         真起 tmux 的文件：{spawners:?}"
    );
}

/// 🔴 宿主口：真起 tmux 的生产文件恰好一份 `host/tmux.rs`（别处起 tmux 经 `host::tmux::child`）。
/// 删的时刻：tmux 那一形宿主整个删掉的那一版。
#[test]
fn tmux_is_spawned_only_in_the_host_tmux_file() {
    let spawners = tmux_spawning_files(&backend_tree());
    assert_eq!(
        spawners.into_iter().collect::<Vec<_>>(),
        vec!["host/tmux.rs".to_string()],
        "真起 tmux 的生产文件该恰好是 `host/tmux.rs`：起进程改走 `crate::host::tmux::child`，\
         `sh -c` 那两跳的脚本与起法也住那里"
    );
    // 正控：两种起法各在合成树上认得出（直呼 · 经 shell 送带 `tmux ` 的脚本），不带 tmux 的 shell 不算。
    let tree: Tree = [
        ("x.rs", "fn a() { let _ = Child::new( \"tmux\" ); }"),
        ("y.rs", "fn b() { posix_shell(&format!(\"tmux ls\")); }"),
        ("z.rs", "fn c() { posix_shell(\"ls\"); }"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    assert_eq!(
        tmux_spawning_files(&tree).into_iter().collect::<Vec<_>>(),
        vec!["x.rs".to_string(), "y.rs".to_string()]
    );
}

/// 🔴 正反两控：引用图在合成树上认得出 `use` 分组 / `super::` / `crate::` 链 / 子模块，也不乱连。
#[test]
fn the_tmux_reach_ruler_works_on_a_synthetic_tree() {
    let tree: Tree = [
        ("lib.rs", "pub mod a; pub mod b; pub mod c; pub mod d;"),
        ("a.rs", "use crate::{c, b::inner};\npub fn h() { c::go(); }"),
        ("b/mod.rs", "pub mod inner;\npub fn quiet() {}"),
        ("b/inner.rs", "pub fn go() { super::super::d::spawn(); }"),
        ("c.rs", "pub fn go() {}"),
        (
            "d.rs",
            "pub fn spawn() { let _ = crate::platform::child::Child::new(\"tmux\"); }",
        ),
        ("e.rs", "pub fn lonely() {}"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let edges = file_edges(&tree);
    let spawners = tmux_spawning_files(&tree);
    assert_eq!(
        spawners.into_iter().collect::<Vec<_>>(),
        vec!["d.rs".to_string()]
    );
    let from = |f: &str| std::iter::once(f.to_string()).collect();
    // 阳：a 经 `use crate::{…, b::inner}` → inner 经 `super::super::d` → d。
    assert!(
        reachable(&edges, &from("a.rs")).contains("d.rs"),
        "分组 use ＋ super 链没接上：{edges:?}"
    );
    // 阴：c 与 e 谁也不引用 ⇒ 够不着。
    assert!(!reachable(&edges, &from("c.rs")).contains("d.rs"));
    assert!(!reachable(&edges, &from("e.rs")).contains("d.rs"));
    // 阴：b/mod.rs 只声明了子模块 `inner`、没用它 ⇒ 不连（「声明子模块」不等于「引用它」）。
    assert!(
        !edges["b/mod.rs"].contains("b/inner.rs"),
        "只声明没使用的子模块被当成了引用"
    );
}

/// 紧跟 `(` 的最后一段是函数：文件内联模块里的 `super::f()` 不连顶层模块 `f`；`子模块::g()` 照连那个子模块。
#[test]
fn the_tmux_reach_ruler_reads_a_trailing_call_as_a_function() {
    let tree: Tree = [
        ("lib.rs", "pub mod f; pub mod k;"),
        (
            "k/mod.rs",
            "mod door;\nfn f() {}\nmod inner { pub fn run() { super::f(); door::g(); } }",
        ),
        ("k/door.rs", "pub fn g() {}"),
        (
            "f.rs",
            "pub fn spawn() { let _ = crate::platform::child::Child::new(\"tmux\"); }",
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let edges = file_edges(&tree);
    assert_eq!(
        edges["k/mod.rs"].iter().collect::<Vec<_>>(),
        vec!["k/door.rs"],
        "`super::f()` 不该连到顶层模块 `f`、`door::g()` 该连子模块：{edges:?}"
    );
}

/// `use … as 别名`：别名不改指向。指子模块的别名不连父模块（误红那一形）；指到 tmux 那一份的别名要连上（漏判那一形）。
#[test]
fn the_tmux_reach_ruler_reads_use_as_aliases() {
    let tree: Tree = [
        ("lib.rs", "pub mod f; pub mod g; pub mod h;"),
        ("f.rs", "use crate::g::calm as c;\npub fn h() { c::go(); }"),
        (
            "g/mod.rs",
            "pub mod calm;\npub fn spawn() { let _ = crate::platform::child::Child::new(\"tmux\"); }",
        ),
        ("g/calm.rs", "pub fn go() {}"),
        ("h.rs", "use crate::{f as ff, g as gg};\npub fn h() { gg::spawn(); ff::h(); }"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let edges = file_edges(&tree);
    let from = |f: &str| std::iter::once(f.to_string()).collect();
    assert_eq!(
        edges["f.rs"].iter().collect::<Vec<_>>(),
        vec!["g/calm.rs"],
        "`use crate::g::calm as c` 该只连 `g/calm.rs`：{edges:?}"
    );
    assert!(!reachable(&edges, &from("f.rs")).contains("g/mod.rs"));
    assert_eq!(
        edges["h.rs"].iter().collect::<Vec<_>>(),
        vec!["f.rs", "g/mod.rs"],
        "分组里的别名也要认：{edges:?}"
    );
}

/// 🔴 正反两控：函数级调用图在合成树上——同一份文件里不叫的函数不连（文件级会连）· 读一张常量表不算叫了表里的闭包 ·
/// 字符串里的花括号与路径不算；方法调用 · 经 `use` 进来的名字 · 点名一个类型（交给 trait 用）照连。
#[test]
fn the_function_level_ruler_works_on_a_synthetic_tree() {
    let tree: Tree = [
        ("lib.rs", "pub mod h; pub mod t; pub mod q; pub mod inj;"),
        (
            "t.rs",
            "pub fn child() -> Child { Child::new(\"tmux\") }\n\
             pub struct On;\nimpl On { pub fn read(&self) { child(); } }",
        ),
        (
            "q.rs",
            "use crate::t::child;\npub fn calm() {}\npub fn loud() { child(); }\n\
             pub const TABLE: &[fn()] = &[|| loud()];",
        ),
        (
            "inj.rs",
            "pub struct Prod;\nimpl Host for Prod { fn screen(&self) { crate::t::On.read(); } }",
        ),
        (
            "h.rs",
            "use crate::q;\n\
             pub struct A; impl Handle for A { fn run() { q::calm(); let _ = q::TABLE.len(); let s = \"{ crate::q::loud() }\"; } }\n\
             pub struct B; impl Handle for B { fn run() { let o = crate::t::On; o.read(); } }\n\
             pub struct C; impl Handle for C { fn run() { arrive(crate::inj::Prod); } }",
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let g = fn_graph(&tree);
    let spawn: std::collections::BTreeSet<FnAt> =
        [("t.rs".to_string(), "child".to_string())].into_iter().collect();
    let reaches = |root: &str| {
        let from = std::iter::once(("h.rs".to_string(), root.to_string())).collect();
        !reachable_fns(&g, &from).is_disjoint(&spawn)
    };
    assert!(!reaches("A::run"), "同文件里没叫的 `loud` · 常量表里的闭包 · 字符串里的路径被连上了：{:?}", g.edges);
    assert!(reaches("B::run"), "方法 `.read(` 没连到 `On::read`：{:?}", g.edges);
    assert!(reaches("C::run"), "点名的类型 `Prod` 没连到它的方法：{:?}", g.edges);
    // 文件级那一把在同一棵树上会把 A 量成够得着（`h.rs` → `q.rs` → `t.rs`）：本条正是函数级比它准的那一处。
    let from = std::iter::once("h.rs".to_string()).collect();
    assert!(reachable(&file_edges(&tree), &from).contains("t.rs"));
}

/// ccm 面每条能力的最小探针（位置动作 ＋ 旗标；`carriers` = 探针里为了让它起得来而**必须**带上的 tmux 载体旗标）。
const CCM_PROBES: &[(&str, &[&str], &[&str])] = &[
    ("account", &["--account", "acct-x"], &[]),
    ("account-via-backend", &["--account", "acct-x"], &[]),
    ("agent", &["--ccm-agent", "codex"], &[]),
    ("attach", &["--attach", "foo"], &[]), // 位置动作是 `--attach`
    (
        "backend-discover",
        &["--resume", "11111111-2222-3333-4444-555555555555"], // 交给 claude 的那一形
        &[],
    ),
    ("cwd", &["--cwd", "/srv"], &[]),
    ("detach", &["--ccm-tmux", "--detach"], &["--ccm-tmux"]),
    ("launcher", &["--launcher", "claude-nightly"], &[]),
    ("model", &["--model", "opus"], &[]),
    (
        "resume",
        &["--resume", "11111111-2222-3333-4444-555555555555"], // 交给 claude 的那一形
        &[],
    ),
    ("tmux", &["--ccm-tmux"], &[]),
    ("tmux-base", &["--tmux-base", "proj"], &[]),
    (
        "tmux-size",
        &["--ccm-tmux", "--tmux-size", "80x24"],
        &["--ccm-tmux"],
    ),
];

/// 计划那一层判不了的几条：`(能力, 算不算靠 tmux, 为什么)`。
const CCM_TMUX_EXCEPTIONS: &[(&str, bool, &str)] = &[
    (
        "base-url-across-tmux",
        true,
        "不是一个旗标，是容器路的一条性质（`plan.rs` 容器分支把 `ANTHROPIC_BASE_URL` 显式带进载荷内侧）—— 名字就是跨 tmux 的边界，探针非带 `--tmux` 不可、带了就恒判「靠 tmux」，判了等于没判",
    ),
    (
        "ccm-sid",
        true,
        "直路上**有**效果（启动期令牌，计划与基线不同 ⇒ 计划层判它「不靠 tmux」），但 Windows 上后端读不到别的进程的环境 ⇒ 认不回 agent 进程 —— 平台原语那一层计划里看不见，归 tmux 档（`control/ccm/mod.rs::CCM_TMUX_CARRIED` 那一行的依据）",
    ),
    ("new", false, "默认动作本身：探针与基线同形是定义使然（「直路上有没有效果」对它不成立）"),
    ("print", false, "渲染方式（吐出计划还是执行它），不进计划 —— 计划与基线同形是定义使然"),
];

/// 🔴 ccm 面：真靠 tmux 的能力（真解析器 ＋ 真计划现推）∪ 登记的例外 == `CCM_TMUX_CARRIED`。
#[test]
fn every_ccm_capability_that_rides_tmux_is_declared_tmux_carried() {
    use crate::ccm_line::argv::{parse, Parsed};
    use crate::control::ccm::plan::tests::{library_of, product_row};
    use crate::control::ccm::plan::{build, Env, Plan};
    use std::collections::BTreeSet;
    let env = Env {
        home: "/home/pi".into(),
        pwd: "/p".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        account_env: "CLAUDE_CONFIG_DIR".into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        ..Default::default()
    };
    // 账号那一格要一个**真在盘上**的目录（成品照盘上的目录判选不选得了）：借后端源码根（一定在，不建不删 ——
    // 建临时目录会让本文件在测试层分区里从「源码扫描」升成「集成」，`test_tiers` 那张表跟着要挪）。
    let acct_dir = guard_core::addr::backend_src_root();
    // 账号 0 排第一个：没标默认号 ⇒ 默认号是它（不指定号），基线不落到 `acct-x` 上（否则 `--account acct-x` 与基线同形）。
    let acct_dir = acct_dir.to_string_lossy().into_owned();
    let table = library_of(vec![
        product_row("zero", None, false),
        product_row("acct-x", Some(&acct_dir), false),
    ]);
    let plan_of = |args: &[&str]| -> Result<Plan, String> {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        // 探针把 ccm 选项写在前 ⇒ 喂解析器前换成 `--` 切分的排列。
        match parse(&crate::ccm_line::argv::tests::mixed_to_split(&a)) {
            Ok(Parsed::Opts(o)) => build(&o, &env, &table, None).map_err(|d| d.0),
            Ok(Parsed::Early(e)) => Err(format!("落进了立即结束那一支：{e:?}")),
            Err(d) => Err(d.0),
        }
    };
    let caps: BTreeSet<&str> = crate::control::ccm::CAPABILITIES.iter().copied().collect();
    let probed: BTreeSet<&str> = CCM_PROBES.iter().map(|(c, ..)| *c).collect();
    let excepted: BTreeSet<&str> = CCM_TMUX_EXCEPTIONS.iter().map(|(c, ..)| *c).collect();
    assert!(
        probed.is_disjoint(&excepted),
        "同一条能力既有探针又登记成例外：{:?}",
        probed.intersection(&excepted).collect::<Vec<_>>()
    );
    assert_eq!(
        probed.union(&excepted).copied().collect::<BTreeSet<_>>(),
        caps,
        "探针表 ∪ 例外表 与 `ccm-launcher` 的 `CAPABILITIES` 不相等 —— 新加的能力要配一个探针（或进例外表写清为什么计划层判不了）"
    );
    let baseline = plan_of(&[]).expect("基线（什么都不给）起不来 —— 夹具坏了");
    let mut derived: BTreeSet<&str> = BTreeSet::new();
    for (cap, argv, carriers) in CCM_PROBES {
        let p = plan_of(argv)
            .unwrap_or_else(|e| panic!("能力 `{cap}` 的探针 {argv:?} 起不来：{e} —— 探针写错了"));
        for c in *carriers {
            let without: Vec<&str> = argv.iter().copied().filter(|t| t != c).collect();
            assert!(
                plan_of(&without).is_err(),
                "能力 `{cap}` 的探针带了载体旗标 `{c}`，拿掉它照样起得来 ⇒ 那个旗标是白带的，本条会把它判成「靠 tmux」而冤枉它"
            );
        }
        let rides = matches!(p, Plan::Attach { .. } | Plan::Container(_));
        if !rides {
            assert_ne!(
                p, baseline,
                "能力 `{cap}` 的探针 {argv:?} 落在直路上，而计划与基线一模一样 ⇒ 离了 tmux 它什么都不做：\
                 要么它其实靠 tmux（登记进 `CCM_TMUX_CARRIED`），要么它是个声明了却不起作用的鬼影"
            );
        }
        if rides {
            derived.insert(cap);
        }
    }
    let want: BTreeSet<&str> = derived
        .union(
            &CCM_TMUX_EXCEPTIONS
                .iter()
                .filter(|(_, t, _)| *t)
                .map(|(c, ..)| *c)
                .collect(),
        )
        .copied()
        .collect();
    let declared: BTreeSet<&str> = CCM_TMUX_CARRIED.iter().copied().collect();
    assert_eq!(
        want, declared,
        "\n真靠 tmux 的 ccm 能力（真解析器 ＋ 真计划现推，∪ 登记的例外）与 `CCM_TMUX_CARRIED` 不相等。\n\
         多出来的（现推靠 tmux、却没登记）⇒ 🔴 没有 tmux 的平台上 `--ccm-probe` 会把它报成做得到 —— 补进 `control/ccm/mod.rs::CCM_TMUX_CARRIED`。\n\
         少了的（登记了、现推却落在直路上且有效果）⇒ 那一行登记冤枉了它，或者它的依据在计划层看不见（进例外表写清）。"
    );
}
