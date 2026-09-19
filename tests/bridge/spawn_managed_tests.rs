use super::*;

/// 本模块自己的源码（生产段）。
fn me() -> String {
    guard_core::production_code(include_str!("../../src/bridge/src/spawn_managed.rs"))
}

/// ★★ **三个策略一个都不许有 `Default`。**
///
/// 这是整件事的**承重墙**：有 `Default` 的那一刻，「每个落点被迫回答三个问题」
/// 就退化成「不写就按某个人当年的顺手值来」—— 而 `真相源/70` 那条 BUG
/// 正是「某个人当年的顺手值」。
///
/// ⚠ 它认的是源码形态（`derive(... Default ...)` 与 `impl Default for`），
/// 挡得住顺手加一个，挡不住换个名字绕过去。**比没有强，别读成证明。**
#[test]
fn the_three_policies_have_no_default() {
    let src = me();
    let d = format!("{}efault", "D");
    for ty in ["ConsolePolicy", "Lifetime", "StderrSink"] {
        let bad = format!("impl {d} for {ty}");
        assert!(
            !src.contains(&bad),
            "`{ty}` 有了 `{d}` 实现 —— 那三个必填参数当场变成「不写也行」，\n\
                 而本模块存在的全部理由就是让那件事**表达不出来**。"
        );
    }
    // `derive` 那一半：本模块生产段里的 derive 列表一个 `Default` 都不许有。
    for line in src.lines().filter(|l| l.contains("derive(")) {
        assert!(
            !line.contains(&d),
            "本模块的 derive 里出现了 `{d}`：{line}\n\
                 ⚠ 三个策略枚举共用这一条 —— 它们的 derive 列表逐字是\
                 `Debug, Clone, Copy, PartialEq, Eq`。"
        );
    }
}

/// ★ **反向自检**：上一条是「什么都没找到」型断言，它零命中地绿可能只是因为
/// 剥法把整份源码剥空了。这里钉住剥完之后那三个枚举**还在**。
#[test]
fn the_no_default_guard_is_actually_looking_at_the_enums() {
    let src = me();
    for ty in ["ConsolePolicy", "Lifetime", "StderrSink"] {
        assert!(
            src.contains(&format!("pub enum {ty}")),
            "剥完生产段之后找不到 `pub enum {ty}` —— 上一条此刻在空转"
        );
    }
    assert!(
        src.contains("derive("),
        "剥完之后一个 `derive(` 都没有 —— 上一条的第二半在空转"
    );
}

/// ★★★ **唯一出口这件事本身要有人数。**
///
/// # 它守什么
///
/// 全树生产段里，「真正起一个进程那一下」与「三条策略的平台原语」
/// **只许出现在本模块**。人群从源码派生（默认拒绝），不是手写清单。
///
/// # 为什么不是「数 `Command::new(` 」
///
/// `Command::new(` 是**装东西**那一下（argv / env / cwd 各落点自己的事），
/// 数它只会逼出一个什么都往里传的上帝函数。
/// 真正会出错的是**收尾那一下**：`.spawn()` / `.output()` / `.status()`
/// 以及三条平台原语。⇒ 人群锚在那儿。
/// 「谁在起进程」那张表另有其人：`write_site_registry::SPAWNS`（本条不替它）。
///
/// # ⚠ `build.rs` 刻意不在人群里
///
/// 它跑在**构建期**、在开发者机器上，`00 §1.5.2` 那三个问题
/// （窗口 / 随谁死 / 错误往哪去）对它一个都不成立：没有 GUI 宿主可弹窗，
/// 没有 monitor 进程可随，错误就该打到 `cargo` 的 stderr 上。
/// ⇒ 它在 `SPAWNS` 里申报、但**不进这个出口**。这一格是刻意的，不是漏了。
#[test]
fn the_spawn_verbs_and_platform_primitives_live_only_here() {
    /// 「真正起一个进程那一下」＋三条平台原语。**无条件**：这几个点调用
    /// 在本仓只可能长在 `Command` 上。
    const VERBS: &[&str] = &[
        ".spawn()",
        ".creation_flags(",
        ".process_group(",
        ".kill_on_drop(",
    ];
    /// 🔴 **只在「这份源码里确实造了 `Command`」时才算**。
    ///
    /// 射程边界，写下来：`.output()` / `.status()` 这两个名字**别的东西也有**
    /// （现打：`search.rs` 的 `index.status()` 是索引器的状态，和进程没关系）。
    /// 把它们无条件禁掉就是造一族假红，而假红会逼人去登记一条与事实无关的例外。
    /// ⇒ 用「同一份源码里有没有 `Command::new(`」当前提。
    /// **代价也写下来**：一个既造 `Command`、又在别处对别的类型调 `.output()` 的文件
    /// 会被误判 —— 那时该做的是把那一处改名/拆文件，不是把这两个词删掉。
    const VERBS_IF_BUILDS_A_COMMAND: &[&str] = &[".output()", ".status()"];
    // 🔴 〔搬树 2026-09-18 · `设计/99` 条 73〕**排掉的是谁、为什么 —— 明写。**
    //
    // 排掉 `src/bridge/src/spawn_managed.rs`：它**就是**那个唯一出口，
    // 那四个动词（`.spawn()` / `.creation_flags(` / `.process_group(` / `.kill_on_drop(`）
    // 按设计只许出现在它里面。人群是「**绕开**这个出口的地方」，本来就不含它自己。
    //
    // 上一版靠 `scan_tree!` 的 `file!()` 自摘 —— 当年本条住在 `spawn_managed.rs` 的
    // `#[cfg(test)]` 段里，「摘掉调用者」恰好等于「摘掉那个唯一出口」。剖分之后
    // `file!()` 指向本测试文件，那一刀**整个落空** ⇒ 唯一出口自己被报成违例，
    // 而报文教人「把 `cmd.spawn()` 换成 `spawn_managed_cmd(..)`」——**在它自己的实现里**。
    // ⇒ 换成明写的排除（摘不到它，`scan_tree_excluding` 当场红）。
    let files = guard_core::scan_tree_excluding(
        &crate::guard_support::crate_src_root(),
        &["rs"],
        &["src/bridge/src/spawn_managed.rs"],
    );
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for (path, raw) in &files {
        let prod = guard_core::production_code(raw);
        scanned += prod.len();
        for v in VERBS {
            if prod.contains(v) {
                offenders.push(format!("  {}: `{v}`", path.display()));
            }
        }
        if prod.contains(concat!("Command::", "new(")) {
            for v in VERBS_IF_BUILDS_A_COMMAND {
                if prod.contains(v) {
                    offenders.push(format!("  {}: `{v}`", path.display()));
                }
            }
        }
    }
    assert!(
        scanned > 100_000,
        "剥掉测试段后只剩 {scanned} 字节可扫 —— 这条会零命中地绿"
    );
    // 反向自检②：条件那一支**必须真的有人走过**。全树一份 `Command::new(` 都扫不到时，
    // `VERBS_IF_BUILDS_A_COMMAND` 那两条就是死规则，而死规则看起来和「干净」一模一样。
    let builders = files
        .iter()
        .filter(|(_, raw)| guard_core::production_code(raw).contains(concat!("Command::", "new(")))
        .count();
    assert!(
        builders >= 5,
        "全树生产段只有 {builders} 份源码在造 `Command` ——              `VERBS_IF_BUILDS_A_COMMAND` 那一支此刻近乎死规则（08-08 起这个数一直是两位数）"
    );
    assert!(
        offenders.is_empty(),
        "这些地方绕开了 `spawn_managed` 这个唯一出口：\n{}\n\n\
             ★ `15 §5.1 A3` / `00 §1.5.2`：起子进程的三个问题（要不要窗口 · 要不要随我死 · \
             错误往哪去）**必须在编译期各自回答一遍**。\n\
             ⇒ 把 `cmd.spawn()` 换成 `crate::spawn_managed::spawn_managed_cmd(&mut cmd, …)`，\
             三个策略照实写；`backend/` 那一半收注入参数（`ManagedSpawn`），不许自己认平台。\n\
             ⚠ 别在落点上直接写 `creation_flags` / `process_group` / `kill_on_drop` —— \
             那正是「同一形状出现三次」的来路。",
        offenders.join("\n")
    );
}

/// ★ **上一条的阴性对照**：那六个词不是瞎的 —— 本模块自己的生产段必须全都命中。
///
/// ⚠ 没有这一条的话，把 `VERBS` 写错一个字母（或者剥法哪天把整份源码剥空）
/// 会让主判据**零命中地绿**，而「唯一出口」这件事悄悄一个人都不数了。
#[test]
fn the_exit_itself_still_contains_every_verb_it_forbids_elsewhere() {
    let src = me();
    // ⚠ 三条平台原语各自在 `#[cfg]` 里 —— 剥生产段不剥 `cfg`，所以两边都看得到。
    for v in [
        ".spawn()",
        ".creation_flags(",
        ".process_group(",
        ".kill_on_drop(",
    ] {
        assert!(
            src.contains(v),
            "本模块自己的生产段里找不到 `{v}` —— 要么它搬走了（那主判据该换住址），\
                 要么剥法坏了（那主判据此刻在空转）"
        );
    }
}
