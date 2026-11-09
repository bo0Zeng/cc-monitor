//! # 要求：`INVARIANTS §2`（含「唯一的明文例外：`CCM_DATA_DIR`」那一段）· 一台机器一个家 `~/.cc-monitor/`
//!
//! `with_nothing_set_it_is_the_documented_default` 点的是 `§2` 正文，逐字「monitor 自己的 data dir 永远是 `~/.cc-monitor/`」。
//! 其余几条判的是那句「永远」的**出口**（`config.rs::monitor_data_dir_from`），逐字点 `§2` 例外段的四条规矩：
//! 「只认**绝对路径**；空串 == 没设」·「给了但不合法（相对路径）⇒ **`None`，不退回用户真 profile**」·
//! 「全树只经 `config.rs::resolve_monitor_data_dir` 派生」。
//! 〔JA1 点址 2026-09-24：当时缺条、候选升格 · TL2 2026-09-25 补成 `§2` 的明文例外（IV1 报的第 3 件）〕

use super::*;

// ════════════════════════════════════════════════════════════════════════
// 🔴`CCM_DATA_DIR` —— 那个目录此前没有任何出口
// ════════════════════════════════════════════════════════════════════════
//
// # 这一摞为什么不碰环境变量
//
// `lib_env_scrub_tests` 那条判据的头注逐字：
// 「**绝不能在测试里 set/remove 真实的 `CLAUDE_*` 变量（会干扰并发测试与宿主环境）**」
// —— cargo test 多线程跑、进程级 env 是共享的，而 `resolve_monitor_data_dir`
// 有 **8 处**消费者（凭据库 · 历史元数据 · 自启 · config.json · 设置面板那一块 …）。
// ⇒ 本摞全部走 `monitor_data_dir_from(env_val, home)` 这个**纯**入口，一次 `set_var` 都没有。
//
// ⚠ 代价如实记：**「`resolve_monitor_data_dir` 真的去读那个 env」这一格本摞买不到**。
// 它由末尾那条源码判据（那一处 `env::var(DATA_DIR_ENV)` 恰好一处）顶着，
// 而那是**代理**不是行为。

fn home() -> std::path::PathBuf {
    std::path::PathBuf::from("/home/u")
}

/// 夹具里「一条绝对路径」**按平台取那一形**：`/tmp/iso-42` 在 Windows 上没有盘符，不是绝对路径
/// （按进程当前那一盘解）⇒ 拿它当绝对路径的夹具在那边会被**正确地**拒掉。
const ABS: &str = if cfg!(windows) {
    r"C:\iso-42"
} else {
    "/tmp/iso-42"
};

/// 没设 ⇒ 默认落点，而它就是设计写死的那一条。
#[test]
fn with_nothing_set_it_is_the_documented_default() {
    let got = monitor_data_dir_from(None, Some(home())).expect("有 home 却算不出落点");
    assert_eq!(got, std::path::PathBuf::from("/home/u/.cc-monitor"));
}

/// 给了一条绝对路径 ⇒ **原样用**，而且**不掺 home**。
///
/// 后一半要紧：掺了的话「隔离到临时目录」会变成「在临时目录下又建一层 .claude」。
#[test]
fn an_absolute_override_is_used_verbatim() {
    let got = monitor_data_dir_from(Some(ABS), Some(home())).expect("绝对路径被拒了");
    assert_eq!(got, std::path::PathBuf::from(ABS));
    // 🔴 连 home 都拿不到时它照样成立 —— 证明这条出口**不偷偷依赖 home**。
    assert_eq!(
        monitor_data_dir_from(Some(ABS), None),
        Some(std::path::PathBuf::from(ABS)),
        "没有 home 时那条绝对路径也被拒了 —— 那说明它偷偷拿 home 兜了一下"
    );
    // 周围空白不算内容。
    assert_eq!(
        monitor_data_dir_from(Some(&format!("  {ABS}  ")), Some(home())),
        Some(std::path::PathBuf::from(ABS))
    );
}

/// 设成空串 == 没设（shell 里 `CCM_DATA_DIR=` 是最常见的「取消」写法）。
#[test]
fn an_empty_value_means_unset_not_broken() {
    let def = monitor_data_dir_from(None, Some(home()));
    for empty in ["", "   ", "\t", "\n"] {
        assert_eq!(
            monitor_data_dir_from(Some(empty), Some(home())),
            def,
            "`{empty:?}` 没被当成「没设」"
        );
    }
}

/// 🔴🔴 **相对路径 ⇒ `None`，而不是退回用户真 profile。**
///
/// # 这一条是这整件事存在的理由
///
/// 退回真 profile 看起来「更稳」，实际是**反面**：那一趟自动化会以为自己被隔离了，
/// 而它正在写用户的东西 —— `config.json` · tab 集合名 · 固定了哪些 tab · 凭据库。
/// 理由是「**集合名是用户手写的真相，不是能重算的缓存**」。
///
/// 🔴 这一形 2026-09-21 在那台 Win11 虚拟机上**真发生过**：跑 tier-2 时
/// `auto-launch.json` 从 87 字节被改成 133 字节，那一路只能靠跑前备份、跑后还原
/// 才做到零残留 —— 而「靠每次记得备份」不是一个机制，是一次运气。
#[test]
fn a_relative_override_refuses_instead_of_quietly_using_the_real_profile() {
    let real = monitor_data_dir_from(None, Some(home()));
    // Windows 上还有两形也按进程当前那一盘解（与相对路径同一条理由拒）：有根没盘符（`/x` · `\x`）、有盘符没根（`C:x`）。
    // 同一个 `/x` 在 Linux 上是绝对路径、原样用 —— 那一半由 `ABS`（Linux 上就是这一形）钉着。
    let windows_relative: &[&str] = if cfg!(windows) {
        &["/x", r"\x", "C:x"]
    } else {
        &[]
    };
    for rel in ["iso", "./iso", "../iso", "a/b"]
        .iter()
        .chain(windows_relative)
        .copied()
    {
        let got = monitor_data_dir_from(Some(rel), Some(home()));
        assert_eq!(
            got, None,
            "`{rel}` 没被拒 —— 实得 {got:?}。\n\
             ★ 如果它等于默认落点，那正是这条出口存在要挡的那一形：\n\
               一趟以为自己被隔离了的自动化，正在写用户真 profile，而且没有一句话。"
        );
        assert_ne!(got, real, "`{rel}` 被静默退回了真 profile");
    }
    // 阴性对照：绝对路径**不**走这一支（少了这一半，上面可以靠「什么都拒」全绿）。
    assert!(monitor_data_dir_from(Some(ABS), Some(home())).is_some());
}

/// 连 home 都拿不到、又没给 env ⇒ `None`（老行为，一个字没改）。
#[test]
fn no_home_and_no_override_is_still_none() {
    assert_eq!(monitor_data_dir_from(None, None), None);
}

/// 🔴 **那个出口真的盖住了 monitor 这一侧的全部** —— monitor 经 `config.rs` 转交共享那一份规则、只读一处 env。
///
/// ⚠ 判源码是**代理**（同族先例 `transfer_tests::the_real_adapters_speak_only_through_the_channel`）。
/// 「没人自己拼那条路径」那一半挪进 [`the_data_dir_is_spelled_in_one_place`]（全 `src/` 生产段，不只 monitor 这棵树）。
#[test]
fn nothing_else_in_the_monitor_tree_builds_that_path_itself() {
    // 规则住契约包（`relay_route_core::monitor_data_dir`，远端常驻后端按同一份推默认路径）⇒
    //   `config.rs`（原 `paths.rs`）转交它。
    assert!(
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/config.rs"))
            .contains("relay_route_core::monitor_data_dir("),
        "`config.rs`（原 `paths.rs`）不再转交共享那一份规则"
    );

    // 那个 env 名字**只在一处被读** —— 否则「读了哪个变量」会长出第二种答案。
    let me = guard_core::production_code(include_str!("../../../src/frontend/shell/src/config.rs"));
    let read = format!("env::var({})", "DATA_DIR_ENV");
    assert_eq!(
        me.matches(read.as_str()).count(),
        1,
        "`config.rs`（原 `paths.rs`）里读 `{}` 的地方不是恰好一处",
        crate::config::DATA_DIR_ENV
    );
    // 反空真：这把尺子认得出「不在」。
    assert!(!me.contains("env::var(DATA_DIR_ENV_THAT_DOES_NOT_EXIST)"));
}

/// 要求住址：「一台机器一个家 `~/.cc-monitor/`……**不写搬家代码、不认老路径**」· `INVARIANTS §2` 规矩 3「默认住址只在 `relay_route_core::monitor_data_dir` 拼」。
///
/// ① `src/` 生产段（`.rs` 剥掉测试段；`src/doc` 与 `.md` 散文、不进 git 的 `.cargo/` · `gen/` · `embedded-backends/` 不算）**零处**旧住址的名字；
/// ② 默认住址那个目录名在 `relay-route-core/src/lib.rs` 生产段恰好一处，且就在 `monitor_data_dir` 里。
/// 正控：同一个判定认得出合成语料里的旧名。
#[test]
fn the_data_dir_is_spelled_in_one_place() {
    let old = ["claudecode", "frontend"].join("-");
    let spells_old = |rel: &str, text: &str| -> bool {
        let prod = if rel.ends_with(".rs") {
            guard_core::production_code(text)
        } else {
            text.to_string()
        };
        prod.contains(old.as_str())
    };
    let root = guard_core::addr::repo_src_root();
    let skip = [
        "doc/",
        "frontend/shell/.cargo/",
        "frontend/shell/gen/",
        "frontend/shell/embedded-backends/",
    ];
    let mut scanned = 0usize;
    let mut hits: Vec<String> = Vec::new();
    for rel in guard_core::files_under(&root) {
        if rel.ends_with(".md") || skip.iter().any(|p| rel.starts_with(p)) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root.join(&rel)) else {
            continue; // 二进制（图标之类）
        };
        scanned += 1;
        if spells_old(&rel, &text) {
            hits.push(rel);
        }
    }
    assert!(scanned > 500, "只扫到 {scanned} 份 —— 扫描面塌了");
    assert_eq!(
        hits,
        Vec::<String>::new(),
        "生产段里还有数据目录的旧住址（不认老路径）"
    );
    assert!(
        spells_old("x.ts", &format!("const d = '~/.claude/{old}';")),
        "正控：认不出旧名"
    );
    // 阴性对照：测试段里的旧名不算（夹具许写旧住址）。
    assert!(!spells_old(
        "x.rs",
        &format!("fn f() {{}}\n\n#[cfg(test)]\nmod tests {{\n    const D: &str = \"{old}\";\n}}\n")
    ));

    let layout = guard_core::production_code(include_str!(
        "../../../src/common/relay-route-core/src/lib.rs"
    ));
    let dir_lit = format!("{:?}", ".cc-monitor");
    assert_eq!(
        layout.matches(dir_lit.as_str()).count(),
        1,
        "relay-route-core 里数据目录名不是恰好一处"
    );
    let at = guard_core::find_pinned(&layout, "pub fn monitor_data_dir(")
        .expect("monitor_data_dir 不在了");
    let body = &layout[at..at + layout[at..].find("\n}\n").expect("没收尾")];
    assert!(
        body.contains(dir_lit.as_str()),
        "默认住址不在 monitor_data_dir 里拼：{body}"
    );
}

/// 两棵生产树（monitor 前端树 · 后端）里 `dirs::home_dir` 的调用：允许的只有下面这几处，逐条说理由（其余一律改调 `relay_route_core::home_dir`）。
const DIRS_HOME_ELSEWHERE: &[(&str, &str)] = &[];

/// 一份源码（原文）的生产段里 `dirs::home_dir(` 几处。
fn dirs_home_calls(src: &str) -> usize {
    guard_core::production_code(src)
        .matches("dirs::home_dir(")
        .count()
}

/// ★ 要求：「「哪个环境变量算家」是两侧必须对上的**契约** ⇒ 收成契约包里唯一一个函数（`relay_route_core::home_dir`）」·
/// 「monitor 生产段为数据目录 / 家目录用 `dirs::home_dir` 零命中（有别的正当用途就登记理由）」。
/// 调用点现扫（异源：读两棵树的源码，不信任何一侧自报）：① monitor 那份数据目录（`config.rs::resolve_monitor_data_dir` → 壳取家目录那一处 `data_paths.rs::home`）、
/// 文件窗口取家那一处（`filewin/src/shell.rs::local_home`）与后端那一处家目录（`platform/paths.rs` 的 `home_dir` · `home_dir_from`）都调契约包这一个函数；
/// ② 两棵生产树里 `dirs::home_dir(` == [`DIRS_HOME_ELSEWHERE`]（两向）。正控：往 `data_paths.rs` 副本里塞回一处数得出。
#[test]
fn both_halves_read_the_home_through_the_one_contract_function() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let read = |rel: &str| {
        std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读不到 `{rel}`：{e}"))
    };
    let body_of = |src: &str, head: &str| -> String {
        let prod = guard_core::production_code(src);
        let at = prod
            .find(head)
            .unwrap_or_else(|| panic!("找不到 `{head}` —— 改名了就回来改本条"));
        let tail = &prod[at..];
        tail[..tail.find("\n}").expect("函数体没收尾")].to_string()
    };
    // monitor 那一侧分两跳：数据目录经壳取家目录的唯一住址（`data_paths::home`），那一处调契约包这一个函数。
    let config = read("src/frontend/shell/src/config.rs");
    assert!(
        body_of(&config, "pub fn resolve_monitor_data_dir()").contains("crate::data_paths::home()"),
        "monitor 的数据目录不经 `data_paths::home` 取家 —— 两侧会认两个家"
    );
    let shell_home = read("src/frontend/shell/src/data_paths.rs");
    assert!(
        body_of(&shell_home, "pub(crate) fn home()").contains("relay_route_core::home_dir()"),
        "壳取家目录那一处不经 `relay_route_core::home_dir` —— 两侧会认两个家"
    );
    let filewin = read("src/frontend/filewin/src/shell.rs");
    assert!(
        body_of(&filewin, "fn local_home()").contains("relay_route_core::home_dir()"),
        "文件窗口取家目录那一处不经 `relay_route_core::home_dir`"
    );
    let paths = read("src/backend/platform/paths.rs");
    assert!(body_of(&paths, "pub fn home_dir()").contains("relay_route_core::home_dir()"));
    assert!(
        body_of(&paths, "pub fn home_dir_from(").contains("relay_route_core::home_dir_from(get)")
    );
    let mut got: Vec<String> = Vec::new();
    for tree in ["src/frontend", "src/backend"] {
        let mut files = guard_core::scan_tree_excluding(&root.join(tree), &["rs"], &[]);
        // 前端那一侧 ＋ monitor 人群声明里住在它外面的兄弟包（通道 · 宿主原语 · 开窗契约）。
        let frontend = root.join("src/frontend");
        if tree == "src/frontend" {
            for (_, t) in guard_core::population_trees(&frontend.join("shell/src")) {
                if !t.starts_with(&frontend) {
                    files.extend(guard_core::scan_tree_excluding(&t, &["rs"], &[]));
                }
            }
            files.sort_by(|a, b| a.0.cmp(&b.0));
            files.dedup_by(|a, b| a.0 == b.0);
        }
        assert!(
            files.len() > 50,
            "`{tree}` 只扫到 {} 份 —— 遍历坏了",
            files.len()
        );
        for (p, src) in files {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            for _ in 0..dirs_home_calls(&src) {
                got.push(rel.clone());
            }
        }
    }
    got.sort();
    let want: Vec<String> = DIRS_HOME_ELSEWHERE
        .iter()
        .map(|(f, _)| f.to_string())
        .collect();
    assert_eq!(got, want, "`dirs::home_dir` 的调用点变了 ⇒ 改调 `relay_route_core::home_dir`；确有别的正当用途的写进表里说清");
    let planted = shell_home.replace("relay_route_core::home_dir()", "dirs::home_dir()");
    assert_ne!(planted, shell_home, "正控的锚没落在靶上");
    assert!(dirs_home_calls(&planted) >= 1);
}
