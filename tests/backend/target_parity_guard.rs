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
//!
//! 测试侧的 [`Target`] 与 [`TARGET_GAPS`] 在库根占着名字（`lib.rs` 那一行 `use`）：生产里再立一张同名的，测试构建编不过。

use crate::control::ccm::CCM_TMUX_CARRIED;
use crate::platform::tmux::{TmuxPlatform, TMUX_PLATFORM};
use std::collections::BTreeSet;

/// 四个编译 target（Linux musl / gnu · Windows · macOS）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Target {
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
pub(crate) const TARGET_GAPS: &[(&str, &str, Target, &str)] = &[
    ("wire-commands", "kill", Target::Windows, NO_TMUX_YET),
    ("wire-commands", "launch", Target::Windows, NO_TMUX_YET),
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
    ("wire-commands", "plan-return", Target::Windows, NO_TMUX_YET),
    ("wire-commands", "session-say", Target::Windows, NO_TMUX_YET),
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
    ("cli-subcommands", "--kill", Target::Windows, NO_TMUX_YET),
    ("cli-subcommands", "--launch", Target::Windows, NO_TMUX_YET),
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
        "--plan-return",
        Target::Windows,
        NO_TMUX_YET,
    ),
    (
        "cli-subcommands",
        "--session-say",
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
    let cli = crate::SUBCOMMANDS
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
// - **帧面**：`inbound::REGISTRY` 每条命令的处理器（`run:` 闭包里点名的 `crate::…` 路径）所在文件，沿**文件级引用图**
//   （`use` / `crate::` / `super::` / 本文件声明的子模块）能不能走到一份**真起 `tmux`** 的生产文件（`Command::new("tmux")`，
//   或经 `platform::shell::posix_shell` 送一段带 `tmux ` 的脚本）。够得着的 == 声明 `no_tmux` 的 ∪ 登记的「够得着但 tmux 只是可选的」。
// - **ccm 面**：`control::ccm::CAPABILITIES` 每一条配一个最小探针 argv，经**真解析器 ＋ 真计划**（`argv::parse` → `plan::build`，纯、不起进程）
//   看它落在哪条路上：接回 / 容器 ⇒ 靠 tmux；直路 ⇒ 它在直路上必须**有效果**（计划与基线不同），否则它离了 tmux 什么都不做。
//   靠 tmux 的 ∪ 登记的例外 == `CCM_TMUX_CARRIED`。
//
// ⚠ 买不到（如实）：
// - 帧面的引用图是**文件级**的（拿不到函数级调用图）：同一份文件里「只用了一个常量」也算够得着 ⇒ 多判不少判，
//   多出来的进登记表逐条写理由；经函数指针 / trait 对象注入的调用（`inbound` 递给处理器的闭包）看不见。
// - `Run::Builtin` 与就地应答的那几条没有 `crate::` 路径，不在射程（逐条登记，新来一条没路径的会红）。
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
        if segs.len() > 1 {
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

/// 命令表各族（`stream/inbound/registry/*.rs`）生产段里每一条的 `run:` 闭包点名的 `crate::…` 路径所在文件（命令名 → 文件集）。
fn handler_files(
    tree: &Tree,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
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
        handler_files_of(&src[at..], &mods, &mut out);
        typed_handler_files_of(&src[at..], tree, &mods, &mut out);
    }
    out
}

/// 一族命令表里 typed 那几行（`typed::blocking::<路径>()` · `typed::asynchronous::<路径>()` · 帧面硬臂那一档 `typed::frame::<路径>()`）：本体就是那个类型住的文件，
/// 命令名从那份文件里它的 `impl Command for <类型>` 那一段的 `const NAME` 读（异源：不读 `REGISTRY`）。
fn typed_handler_files_of(
    table: &str,
    tree: &Tree,
    mods: &std::collections::BTreeMap<Vec<String>, String>,
    out: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) {
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
            let file = file_of(mods, &path)
                .unwrap_or_else(|| panic!("typed 那一行的 `{ty}` 找不到它住的文件"));
            let src = &tree[&file];
            let imp = src
                .find(&format!("Command for {ty} {{"))
                .unwrap_or_else(|| panic!("`{file}` 里没有 `impl Command for {ty}`"));
            let n = src[imp..]
                .find("const NAME: &'static str = \"")
                .expect("没有 `const NAME`")
                + imp;
            let name: String = src[n + "const NAME: &'static str = \"".len()..]
                .chars()
                .take_while(|c| *c != '"')
                .collect();
            out.insert(name, std::iter::once(file).collect());
        }
    }
}

/// 一族命令表（`SPECS` 那张表起）里逐块取 `run:` 闭包点名的文件，并进 `out`。
fn handler_files_of(
    table: &str,
    mods: &std::collections::BTreeMap<Vec<String>, String>,
    out: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) {
    for block in table.split("CommandSpec {").skip(1) {
        // 块到下一条 typed 那一行为止（typed 那几行不是 `CommandSpec {` 块，由 [`typed_handler_files_of`] 另认）。
        let block = block.split("crate::wire::typed::").next().unwrap_or(block);
        let Some(n) = block.find("name: \"") else {
            continue;
        };
        let name: String = block[n + 7..].chars().take_while(|c| *c != '"').collect();
        let Some(r) = block.find("run:") else {
            continue;
        };
        let run = &block[r..];
        let files: std::collections::BTreeSet<String> = paths_in(&[], run)
            .into_iter()
            .filter_map(|p| file_of(mods, &p))
            .filter(|f| !f.starts_with("stream/inbound/"))
            .collect();
        out.insert(name, files);
    }
}

/// 够得着 tmux、却**不**声明 `no_tmux` 的命令 —— 逐条写理由（tmux 在它那里是可选的：问不到就降级，命令本身照做）。
const REACHES_TMUX_WITHOUT_NO_TMUX: &[(&str, &str)] = &[
    (
        "terminal-follow-ack",
        "终端实时预览：与 `terminal-follow` 同住 `control/terminal_follow.rs`（引用图按文件取），它自己只往订阅线程送一个回执、不起 tmux",
    ),
    (
        "terminal-unfollow",
        "同上，只摘票（订阅线程自己收那个只读客户端）、不起 tmux",
    ),
    (
        "data-report",
        "文件与数据：`tmux` 那一格问这台有没有 tmux（与起新会话框同一个判法），没 tmux ⇒ `tmux: false`、别的几格照答",
    ),
    (
        "footprint-report",
        "足迹：与 `data-report` 同住 `footprint/mod.rs` 与帧面 `faces/footprint_report_face.rs`（引用图按文件取），它自己不问 tmux（tmux 那一行只 stat PATH）",
    ),
    (
        "session-open",
        "起会话 · 恢复 · 接回：要 tmux 而这台没有 ⇒ `place_unavailable`（不改道），直路本来用不着 tmux（Windows 本机照做）",
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
    ("bus-broadcast", "同 `bus-list`：挑在线的那一步读 `live`，问不到 tmux 就当「不知道谁在线」如实回，不回 `no_tmux`"),
    ("bus-kill", "同 `bus-list`（转调 `cc-kill`；tmux 只用来挂 `live`）"),
    ("bus-list", "`control/cc_bus.rs::agents_via_cc_list` 经 `gate::list_sessions().ok()` 挂「还活着吗」那一栏 —— 问不到回 `live: null`（「不假装知道」），命令本身照做；它的能力是转调 cc-bus，不是 tmux"),
    ("bus-send", "同 `bus-list`（投递转调 `cc-send`；tmux 只用来挂 `live`）"),
    ("bus-spawn", "同 `bus-list`（转调 `cc-spawn`；那个子进程自己起 tmux，本进程只经 gate 挂 `live`）"),
    ("bus-state", "同 `bus-list`（`agents` 那一半就是 `bus-list` 那一个函数）"),
    (
        "ccm-probe",
        "同 `ccm-print` 那一族（`control/ccm/mod.rs`）：`--ccm-probe` 那几行里 `capabilities=` 按本二进制的平台档摘掉 tmux 载体，\
         不问 tmux server；命令本身恒答",
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
         `session_snapshot::global().taken_names().ok()` 问一次会话快照做铸名避让 —— 问不到 ⇒ `None` ⇒ 不退让\
         （`plan::build` 头注的诚实降级），预览照出；它的能力是渲计划，不是 tmux",
    ),
    // 〔合并 SH1 × W5-AUX 时本判据当场点出〕
    ("bus-inbox", "同 `bus-list` 那个文件（`control/cc_bus.rs` 经 gate 挂 `live`）；读收件箱本身转调 `cc-log`，不用 tmux"),
    ("resync", "打标经 `identity_tag::tag`：不在 tmux / tmux 起不来 ⇒ 那个会话不打（结局说进日志），对齐照做、照回差异；它的能力是对表，不是 tmux"),
    ("hooks-diag", "同 `mcp-read`：只因与 `session-terminals` 同住 `feature_face.rs` 被连带；读 settings ＋ stat 本身不碰 tmux"),
    ("mcp-read", "与 `session-terminals` 同住 `feature_face.rs`（引用图按文件算，因此连带）；读 MCP 本身不碰 tmux"),
    ("tasks-list", "同 `mcp-read`：只因与 `session-terminals` 同住 `feature_face.rs` 被连带；本身不碰 tmux"),
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
    let edges = file_edges(&tree);
    let spawners = tmux_spawning_files(&tree);
    // 地板只防人群塌成空集；主锚是下面两向相等。现打（立格时）：7 份直呼 + 1 份经 shell（`observe/watcher.rs`）。
    assert!(
        spawners.len() >= 5,
        "真起 tmux 的文件只找到 {spawners:?} —— 取法坏了"
    );
    let handlers = handler_files(&tree);
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
        .filter(|(_, from)| !reachable(&edges, from).is_disjoint(&spawners))
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
    (
        "bus-register",
        &["--ccm-tmux", "--detach", "--bus-register"],
        &["--ccm-tmux", "--detach"],
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
    use crate::control::ccm::plan::{build, Account, AccountTable, Env, Plan};
    use std::collections::BTreeSet;
    let env = Env {
        home: "/home/pi".into(),
        pwd: "/p".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        account_env: "CLAUDE_CONFIG_DIR".into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        ..Default::default()
    };
    // 账号那一格要一个**真在盘上**的目录（`resolve_account` 会看它在不在）：借后端源码根（一定在，不建不删 ——
    // 建临时目录会让本文件在测试层分区里从「源码扫描」升成「集成」，`test_tiers` 那张表跟着要挪）。
    let acct_dir = guard_core::addr::backend_src_root();
    // 账号 0 排第一个：没标默认号 ⇒ 默认号是它（不指定号），基线不落到 `acct-x` 上（否则 `--account acct-x` 与基线同形）。
    let table = AccountTable::from_accounts(vec![
        Account {
            name: "zero".into(),
            config_dir: None,
            is_default: false,
        },
        Account {
            name: "acct-x".into(),
            config_dir: Some(acct_dir.to_string_lossy().into_owned()),
            is_default: false,
        },
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
