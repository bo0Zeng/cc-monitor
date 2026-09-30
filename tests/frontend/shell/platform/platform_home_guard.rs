//! 要求住址：`设计/00 §1.6.4`「`platform/` | **操作系统** | 唯一允许平台原语与平台 `cfg`」· `设计/90 §4` 阶段 H「monitor 侧的 `platform/` 层」· 主会话 09-30 裁「壳生产段里平台 `cfg` / 平台原语只许在壳的 `platform/` 子模块（与 `src/common/host-core`）」。
//!
//! 〔P4b · 阶段 H〕**壳的源码人群里，平台形态只许住在允许区（[`HOMES`]）；还没收的逐份点名在 [`PENDING`]，名单只许变短。**
//!
//! - 人群：壳 `src/` 那棵模块树的生产段（`guard_core::scan_tree_excluding`：顺 `#[path]`，也收 manifest `[package.metadata.guard] population` 声明的兄弟包）。
//! - 形态：[`platform_forms`] —— 谓词点了操作系统的 `cfg(…)` / `cfg!(…)` / `cfg_attr(谓词, …)`，以及 [`PRIMITIVES`] 里的平台原语。
//! - 判：带形态、不在允许区的文件集 == 名单里此刻在盘上的那几行（两向）；名单条数 == [`PENDING_LEN`]（收一份：删一行、上限同拍减一）。
//!
//! 买不到：「两个平台都编得过」（真判据仍是门禁 `winchk` 的跨 target 编译）；运行期才分平台的东西（`"powershell.exe"` 这类程序名、命令串）不在形态集里。

use std::collections::BTreeSet;

/// 允许区：以 `/` 结尾的按前缀认，否则整串相等。住址是 `guard_core::module_address` 给的模块住址（兄弟包带包名）。
const HOMES: &[&str] = &[
    "platform/",
    // 〔P4〕前端宿主原语（两个前端共用）；P4 合入前本树还没有这个包。
    "host-core/",
    // 〔P4〕文件窗口包自己的平台层（P4 在建）。
    "cc-monitor-filewin/platform.rs",
];

/// 这一行此刻在不在盘上由谁的合入决定。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Presence {
    /// 在盘上、带形态 —— 不在就红（搬走 / 改名了改住址，收完了删行）。
    Now,
    /// P4 合入后才出现（文件窗口独立成包之后的新住址）：不在盘上不红；在了就与 `Now` 一样判。
    AfterP4,
}

/// **待收名单**：`(模块住址, 归谁收, 在不在)`。收完一份删一行，并把 [`PENDING_LEN`] 同拍减一。
const PENDING: &[(&str, &str, Presence)] = &[
    ("config.rs", "归 P4 收", Presence::Now),
    (
        "dial_host.rs",
        "待主会话派（不在 P4 / P4b 写区）",
        Presence::Now,
    ),
    (
        "filewin/download.rs",
        "归 P4 收（随窗口包搬走即删）",
        Presence::Now,
    ),
    (
        "filewin/fonts.rs",
        "归 P4 收（随窗口包搬走即删）",
        Presence::Now,
    ),
    (
        "filewin/lossy_pull.rs",
        "归 P4 收（随窗口包搬走即删）",
        Presence::Now,
    ),
    ("filewin/proc.rs", "归 P4 收", Presence::Now),
    (
        "filewin/scale.rs",
        "归 P4 收（随窗口包搬走即删）",
        Presence::Now,
    ),
    (
        "filewin/shell.rs",
        "归 P4 收（随窗口包搬走即删）",
        Presence::Now,
    ),
    ("launch.rs", "归 P4 收", Presence::Now),
    ("lib.rs", "归 P4 收", Presence::Now),
    ("local_backend.rs", "归 P4b 收（本路）", Presence::Now),
    (
        "local_backend_host.rs",
        "归 P4 收（P4 子步 2 已收，合入即删）",
        Presence::Now,
    ),
    ("utils.rs", "归 P4 收", Presence::Now),
    (
        "cc-monitor-filewin/download.rs",
        "归 P4 收（P4 合入后才出现）",
        Presence::AfterP4,
    ),
    (
        "cc-monitor-filewin/fonts.rs",
        "归 P4 收（P4 合入后才出现）",
        Presence::AfterP4,
    ),
    (
        "cc-monitor-filewin/lossy_pull.rs",
        "归 P4 收（P4 合入后才出现）",
        Presence::AfterP4,
    ),
    (
        "cc-monitor-filewin/scale.rs",
        "归 P4 收（P4 合入后才出现）",
        Presence::AfterP4,
    ),
    (
        "cc-monitor-filewin/shell.rs",
        "归 P4 收（P4 合入后才出现）",
        Presence::AfterP4,
    ),
];

/// 名单条数的上限，与 [`PENDING`] 恒等：删行时同拍减一；**不许加**（加一行 = 往壳里别处又写了平台代码）。
const PENDING_LEN: usize = 18;

/// `cfg` 谓词里点了操作系统的词（字符串字面量之外，按整词认）。`target_arch` 不在：那是架构，不是操作系统。
const CFG_WORDS: &[&str] = &[
    "windows",
    "unix",
    "target_os",
    "target_family",
    "target_env",
    "target_vendor",
];

/// 平台原语：生产段里出现就算（不带 `cfg` 也算 —— `env::consts::` 那一形就是不带的）。
const PRIMITIVES: &[&str] = &[
    "std::os::unix",
    "std::os::windows",
    "libc::",
    "windows::Win32",
    "windows::core",
    "use windows::",
    "windows_sys::",
    "winapi::",
    "windows_wv2::",
    "winit::platform::",
    "env::consts::",
    "extern \"system\"",
];

fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `s[open..]` 以 `(` 开头：返回配对的 `)` 的下标（跳过字符串字面量）。配不上 ⇒ `None`。
fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in s[open..].char_indices() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// 把字符串字面量的内容抹掉（`feature = "windows"` 不算点了操作系统）。
fn blank_strings(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_str = false;
    let mut esc = false;
    for c in s.chars() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => {
                    in_str = false;
                    out.push('"');
                }
                _ => {}
            }
            continue;
        }
        if c == '"' {
            in_str = true;
        }
        out.push(c);
    }
    out
}

/// `cfg_attr(谓词, …)` 只取谓词（第一个顶层逗号之前）。
fn first_arg(inner: &str) -> &str {
    let mut depth = 0usize;
    let mut in_str = false;
    for (i, c) in inner.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '(' if !in_str => depth += 1,
            ')' if !in_str => depth = depth.saturating_sub(1),
            ',' if !in_str && depth == 0 => return &inner[..i],
            _ => {}
        }
    }
    inner
}

/// 一段生产段里的平台形态（去重、排序）：`cfg(<谓词>)` 形与原语原文。
pub(crate) fn platform_forms(prod: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut from = 0usize;
    while let Some(rel) = prod[from..].find("cfg") {
        let at = from + rel;
        from = at + 3;
        if prod[..at].chars().next_back().is_some_and(ident_char) {
            continue;
        }
        let rest = &prod[at + 3..];
        let (skip, attr) = if rest.starts_with("_attr(") {
            (5, true)
        } else if rest.starts_with("!(") {
            (1, false)
        } else if rest.starts_with('(') {
            (0, false)
        } else {
            continue;
        };
        let open = at + 3 + skip;
        let Some(close) = matching_paren(prod, open) else {
            continue;
        };
        let inner = &prod[open + 1..close];
        let pred = if attr { first_arg(inner) } else { inner };
        let bare = blank_strings(pred);
        if CFG_WORDS
            .iter()
            .any(|w| guard_core::contains_word(&bare, w))
        {
            out.insert(format!("cfg({})", pred.trim()));
        }
    }
    for p in PRIMITIVES {
        if prod.contains(p) {
            out.insert((*p).to_string());
        }
    }
    out
}

fn at_home(addr: &str) -> bool {
    HOMES.iter().any(|h| {
        if h.ends_with('/') {
            addr.starts_with(h)
        } else {
            addr == *h
        }
    })
}

/// 人群：`(模块住址, 生产段)`。
fn population() -> Vec<(String, String)> {
    let root = crate::guard_support::crate_src_root();
    guard_core::scan_tree_excluding(&root, &["rs"], &[])
        .into_iter()
        .map(|(p, raw)| {
            (
                guard_core::module_address(&root, &p),
                guard_core::production_code(&raw),
            )
        })
        .collect()
}

/// ★ 主判据：带平台形态、不在允许区的文件集 == 待收名单里此刻在盘上的那几行。
#[test]
fn platform_forms_live_only_at_home_or_on_the_pending_list() {
    let pop = population();
    let on_disk: BTreeSet<&str> = pop.iter().map(|(a, _)| a.as_str()).collect();
    let mut offenders: Vec<(String, BTreeSet<String>)> = Vec::new();
    for (addr, prod) in &pop {
        let forms = platform_forms(prod);
        if !forms.is_empty() && !at_home(addr) {
            offenders.push((addr.clone(), forms));
        }
    }
    let found: BTreeSet<&str> = offenders.iter().map(|(a, _)| a.as_str()).collect();
    let listed: BTreeSet<&str> = PENDING
        .iter()
        .filter(|(a, _, p)| *p == Presence::Now || on_disk.contains(a))
        .map(|(a, ..)| *a)
        .collect();
    let gone: Vec<&str> = PENDING
        .iter()
        .filter(|(a, _, p)| *p == Presence::Now && !on_disk.contains(a))
        .map(|(a, ..)| *a)
        .collect();
    assert!(
        gone.is_empty(),
        "待收名单里这几行不在盘上了：{gone:?}\n搬走 / 改名 ⇒ 改成新住址；删了 ⇒ 删这一行（并把 PENDING_LEN 减一）。"
    );
    let unlisted: Vec<String> = offenders
        .iter()
        .filter(|(a, _)| !listed.contains(a.as_str()))
        .map(|(a, f)| format!("  {a}: {f:?}"))
        .collect();
    assert!(
        unlisted.is_empty(),
        "壳的生产段在允许区之外出现了平台形态（`设计/00 §1.6.4`：平台 cfg 与平台原语只许住 `platform/`）：\n{}\n\
         ⇒ 读法收进 `platform/` 里一个函数 / 常量，调用处改调；判定规则留在原处。不许往待收名单里加行。",
        unlisted.join("\n")
    );
    let done: Vec<&&str> = listed.iter().filter(|a| !found.contains(**a)).collect();
    assert!(
        done.is_empty(),
        "待收名单里这几份已经没有平台形态了：{done:?} ⇒ 收完了，删那一行并把 PENDING_LEN 减一（名单只许变短）。"
    );
    assert_eq!(
        PENDING.len(),
        PENDING_LEN,
        "待收名单条数与上限不等：删行要同拍把 PENDING_LEN 减一；上限不许加。"
    );
}

/// ★ 正控①：形态集不是瞎的 —— 每一种形态在合成样本里都被认出，反例一个都不认。
#[test]
fn the_form_matcher_sees_every_form_and_nothing_else() {
    let cfg = "cfg";
    let positives = [
        format!("#[{cfg}(windows)]\nfn a() {{}}"),
        format!("#[{cfg}(not(unix))]\nfn a() {{}}"),
        format!("#[{cfg}(any(windows, test))]\nfn a() {{}}"),
        format!("if {cfg}!(windows) {{}}"),
        format!("#[{cfg}(all(unix, not(target_os = \"macos\")))]\nfn a() {{}}"),
        format!("#[{cfg}(target_family = \"wasm\")]\nfn a() {{}}"),
        format!("#[{cfg}(target_env = \"gnu\")]\nfn a() {{}}"),
        format!("#[{cfg}(target_vendor = \"apple\")]\nfn a() {{}}"),
        format!("#[{cfg}_attr(windows, allow(dead_code))]\nfn a() {{}}"),
    ];
    for s in &positives {
        assert_eq!(platform_forms(s).len(), 1, "认不出这一形平台 cfg：{s}");
    }
    for p in PRIMITIVES {
        let s = format!("fn a() {{ {p}x; }}");
        assert!(platform_forms(&s).contains(*p), "认不出平台原语 `{p}`");
    }
    let negatives = [
        format!("#[{cfg}(test)]\nmod t {{}}"),
        format!("#[{cfg}_attr(not(debug_assertions), windows_subsystem = \"windows\")]"),
        format!("#[{cfg}(feature = \"windows\")]\nfn a() {{}}"),
        format!("#[{cfg}_attr(test, derive(ts_rs::TS))]\nstruct S;"),
        format!("if {cfg}!(debug_assertions) {{}}"),
        format!("#[{cfg}(target_arch = \"x86_64\")]\nfn a() {{}}"),
        format!("let my{cfg}(windows) = 1;"),
        "fn a() { crate::platform::window::exists(1); }".to_string(),
    ];
    for s in &negatives {
        assert!(
            platform_forms(s).is_empty(),
            "把不是平台形态的认成了：{s} ⇒ {:?}",
            platform_forms(s)
        );
    }
}

/// ★ 正控②：壳 `platform/` 里每一份（`mod.rs` 除外）都真的带平台形态 —— 形态集在真树上看得见它们，
/// 而 `platform/` 也不是工具函数堆（`platform/fs.rs` 头注：只放「同一件事在两个平台上做法不同」的原语）。
#[test]
fn every_file_in_the_shell_platform_layer_carries_a_platform_form() {
    let pop = population();
    let layer: Vec<&(String, String)> = pop
        .iter()
        .filter(|(a, _)| a.starts_with("platform/") && a.as_str() != "platform/mod.rs")
        .collect();
    assert!(
        !layer.is_empty(),
        "人群里一份 `platform/*.rs` 都没有 —— 遍历坏了"
    );
    for (addr, prod) in layer {
        assert!(
            !platform_forms(prod).is_empty(),
            "`{addr}` 住在 `platform/` 却一种平台形态都没有：要么形态集瞎了，要么它不该住这里（纯逻辑留通用层）。"
        );
    }
}
