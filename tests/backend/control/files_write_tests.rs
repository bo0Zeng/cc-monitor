//! 〔步 23b · 2026-09-19〕`control/files_write.rs` 的行为判据 —— **钉住那两道围栏**。
//!
//! # 🔴 这一族买到的是什么、**买不到**什么（逐字，不许含糊）
//!
//! **买到的**（全部在本机临时目录上真跑，不是源码扫描）：
//!
//! - 围栏① 真的会拒：上跳段 · 绝对路径 · 空段 · 落进 Claude 树的写点；
//! - 围栏② 真的会拒：目标根里放一条**真的 symlink** 指向 Claude 树，解完之后被拦；
//! - 两道围栏**真的接在写入口上**：拒绝的那几次，盘上**没有**多出任何文件
//!   （删掉入口里那一句串联调用 ⇒ 文件会真落盘 ⇒ 这里当场红）；
//! - `O_EXCL` 真的是 `O_EXCL`：第二次写同一个名字失败，且**第一份内容一个字节没动**。
//!
//! **买不到的**（前提今天不成立，如实登记，不许读成「过了」）：
//!
//! 1. 🔴 **没有真远端**。本模块与本族判据全跑在**本机**文件系统上。
//!    「在一台真的远端机器上、经由后端跑过一次这条写路」这件事**本轮判不了** ——
//!    盘面上没有真远端，`设计/60 §7` 那条「未实测」照旧成立。
//! 2. 🔴 **没有覆盖「多账号同时在盘上」那一维**。`resolve_home` 只答得出此刻被选中的
//!    那一个配置根；另外几个账号目录靠「段以 `.claude` 开头」这条形状兜，
//!    而那条形状**换个名字就兜不住**。本族判据对那一维**不出声**。
//! 3. 🔴 **TOCTOU 那个窗没判**。围栏② 与落盘之间的竞态要真并发才量得出来，
//!    本族判据一格都没量。模块头注里登记的兜底（`O_EXCL` 挡住最后那一段）
//!    这里只验了「同名第二次会失败」，**没有**验「父目录在窗里被换掉」那一形。
//! 4. 🔴 **「这个模块该不该有写盘能力」不是这里判的**。那是政策，住
//!    `readonly_guard::WRITE_WHITELIST_MODULES` 那张登记表（`设计/60 §6.5.2 A`）。
//!    本族只判「围栏在不在、拦不拦得住」。

use super::*;

/// 一个本轮独占的临时目标根。`tag` 区分用例，`pid` 区分并发跑的进程。
fn temp_root(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-fw-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(&p).expect("建临时目标根");
    p
}

/// 一个**确定不在 Claude 树里**的假配置根，给纯函数用例当参数。
///
/// 纯函数全部把 `claude_home` 当**入参**收，刻意不去读环境变量 ——
/// `set_var` 在并发测试里是共享状态，本仓不往那条路上走。
fn fake_home() -> PathBuf {
    PathBuf::from("/nonexistent-claude-home-for-tests")
}

// ── 围栏核心判定：`is_inside_tree` ────────────────────────────────

/// ★ **量具自检**：这条判定两个方向都得会答。
///
/// 只验「该拒的拒了」是空真的一半：一个恒返回 `true` 的实现同样能过。
/// ⇒ 阴阳两侧同拍验，缺一侧这条就不是判据。
#[test]
fn the_claude_tree_predicate_answers_both_ways() {
    let home = PathBuf::from("/home/u/.claude");
    // ── 阳性：该认出来的几形 ──────────────────────────────────────
    for hit in [
        "/home/u/.claude",                            // 根自己
        "/home/u/.claude/projects/p/s.jsonl",         // 根底下
        "/home/u/.claude.json",                       // `~/.claude*` 那个星号
        "/home/u/.claude-accts/q/projects/p/s.jsonl", // 另一个账号的根
        "/srv/work/.claude/settings.json",            // 工程内的那棵
        "/home/u/.claudex/x",                         // 星号逐字包含它
    ] {
        assert!(
            is_inside_tree(&home, Path::new(hit)),
            "该判成 Claude 树却放过了：{hit}"
        );
    }
    // ── 阴性：普通路径一个都不许误伤 ────────────────────────────────
    for miss in [
        "/home/u/docs/a.md",
        "/home/u/claude/a.md", // 没有前导点 —— 这是普通目录
        "/srv/work/src/main.rs",
        "/home/u/notes/claude-notes.md",
    ] {
        assert!(
            !is_inside_tree(&home, Path::new(miss)),
            "普通路径被误伤成 Claude 树：{miss}"
        );
    }
}

/// 配置根被账号隔离切到一个**不带 `.claude` 字样**的地方时，第一条仍然认得出来。
///
/// 这一格是「段形状」那条兜不住的那一半 —— 两条各治一形，缺一条就有一族逃得掉。
#[test]
fn a_relocated_config_root_is_still_recognised() {
    let home = PathBuf::from("/opt/accts/q");
    assert!(
        is_inside_tree(&home, Path::new("/opt/accts/q/projects/p/s.jsonl")),
        "`CLAUDE_CONFIG_DIR` 切到不带 `.claude` 字样的地方就认不出来了 —— \
         那正是账号隔离每天在做的事"
    );
    assert!(
        !is_inside_tree(&home, Path::new("/opt/accts-other/x")),
        "按段比的语义丢了：`/opt/accts-other` 不在 `/opt/accts/q` 底下"
    );
}

// ── 围栏①（词法）──────────────────────────────────────────────────────

#[test]
fn the_lexical_fence_refuses_the_four_shapes() {
    let home = fake_home();
    let root = PathBuf::from("/srv/target");
    for (rel, word) in [
        ("../escape.txt", "上跳段"),
        ("a/../../escape.txt", "上跳段"),
        ("/etc/passwd", "绝对路径"),
        ("./a.txt", "当前目录段"),
        ("", "空的"),
        ("   ", "空的"),
    ] {
        let err = fence_lexical(&home, &root, rel)
            .expect_err(&format!("围栏① 放过了 {rel:?} —— 它该被拒"));
        assert!(
            err.contains("refuse write") && err.contains(word),
            "拒了，但说不清是哪一形（rel={rel:?}，错误={err}）"
        );
    }
}

/// ★ 阴性对照：干净的相对段必须**过得去**。
///
/// 没有这一格，一个「恒 `Err`」的围栏也能让上面那条全绿 —— 那是最典型的空真。
#[test]
fn the_lexical_fence_lets_a_clean_relative_path_through() {
    let home = fake_home();
    let root = PathBuf::from("/srv/target");
    let ok = fence_lexical(&home, &root, "docs/notes/a.md").expect("干净的相对段被误拒");
    assert_eq!(ok, root.join("docs/notes/a.md"), "落点算错了");
}

/// 目标根**自己**落在 Claude 树里时，相对段再干净也不行。
#[test]
fn a_target_root_inside_the_claude_tree_is_refused_outright() {
    let home = PathBuf::from("/home/u/.claude");
    let err = fence_lexical(&home, Path::new("/home/u/.claude/projects"), "p/s.jsonl")
        .expect_err("把「文件管理目标」指到 Claude 树里，竟然放行了");
    assert!(err.contains("Claude 数据源"), "拒了，但不是围栏拒的：{err}");
}

// ── 围栏②（现打，真 symlink）──────────────────────────────────────────

/// ★★ **本族最承重的一格**：目标根里藏一条指向 Claude 树的 symlink。
///
/// 围栏① 对它**完全看不见**（`docs/a.md` 在词法上干净得很），
/// 只有解完 symlink 再判一次才拦得住。⇒ 这一格红，说明围栏② 没了。
///
/// ★ 它同时钉着围栏② 里**两条判定的先后**：这条 symlink 既跑出了目标根、又落进了那几棵树，
/// 两条都会拒 —— 而诊断必须是「碰了 Claude 数据源」那一句（那是这条围栏立在这里的全部理由）。
/// 把顺序换回去，本格会以「说的不是 Claude」的形式红。
#[test]
#[cfg(unix)]
fn the_resolved_fence_catches_a_symlink_into_the_claude_tree() {
    let base = temp_root("symlink");
    let root = base.join("target");
    let home = base.join(".claude");
    std::fs::create_dir_all(&root).expect("建目标根");
    std::fs::create_dir_all(&home).expect("建假 Claude 根");
    std::os::unix::fs::symlink(&home, root.join("docs")).expect("放 symlink");

    // 围栏①：看不见 —— 这一句是**对照**，它证明围栏② 不是多余的。
    let lexical = fence_lexical(&home, &root, "docs/a.md").expect("围栏① 本来就该放过它");
    // 围栏②：解完之后当场拦下。
    let err = fence_resolved(&home, &root, &lexical)
        .expect_err("目标根里的 symlink 指进 Claude 树，围栏② 竟然放行了");
    assert!(
        err.contains("Claude 数据源"),
        "拒了，但不是 Claude 围栏拒的：{err}"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 同一道围栏的另一形：symlink 指到目标根**外面**（不是 Claude 树，只是越界）。
#[test]
#[cfg(unix)]
fn the_resolved_fence_catches_a_symlink_out_of_the_target_root() {
    let base = temp_root("escape");
    let root = base.join("target");
    let outside = base.join("outside");
    std::fs::create_dir_all(&root).expect("建目标根");
    std::fs::create_dir_all(&outside).expect("建外部目录");
    std::os::unix::fs::symlink(&outside, root.join("out")).expect("放 symlink");

    let lexical = fence_lexical(&fake_home(), &root, "out/a.md").expect("围栏① 该放过它");
    let err = fence_resolved(&fake_home(), &root, &lexical)
        .expect_err("symlink 指出目标根，围栏② 竟然放行了");
    assert!(err.contains("跑出了目标根"), "拒了，但说的不是越界：{err}");
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 阴性对照：没有 symlink 的干净路径，围栏② 必须放行并解出真路径。
#[test]
fn the_resolved_fence_lets_a_clean_path_through() {
    let root = temp_root("clean2");
    std::fs::create_dir_all(root.join("docs")).expect("建子目录");
    let lexical = fence_lexical(&fake_home(), &root, "docs/a.md").expect("围栏① 该放过它");
    let got = fence_resolved(&fake_home(), &root, &lexical).expect("围栏② 误拒了干净路径");
    assert_eq!(
        got.file_name().and_then(|s| s.to_str()),
        Some("a.md"),
        "解出来的落点文件名不对：{}",
        got.display()
    );
    assert!(
        got.starts_with(std::fs::canonicalize(&root).expect("解目标根")),
        "解出来的落点跑到目标根外面去了：{}",
        got.display()
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 父目录不在盘上 ⇒ 正常拒绝（本模块**不建目录**，那是白名单层明令禁止的）。
#[test]
fn a_missing_parent_directory_is_a_plain_refusal_not_a_silent_mkdir() {
    let root = temp_root("noparent");
    let err = fence_resolved(&fake_home(), &root, &root.join("nope/a.md"))
        .expect_err("父目录不在，却没拒");
    assert!(err.contains("父目录解析不了"), "拒的理由不对：{err}");
    assert!(
        !root.join("nope").exists(),
        "父目录被顺手建出来了 —— 白名单层不许建目录"
    );
    std::fs::remove_dir_all(&root).ok();
}

// ── 写入口：围栏真的接上了吗 ────────────────────────────────────────────

/// ★★ **防空转**：上面那些围栏用例全在函数上，围栏只要不接到写入口就是死代码，
/// 而那些用例照样绿。这一格直接打写入口，并**去盘上数**有没有多出文件。
///
/// 删掉 `create_new_file` 里那一句串联调用 ⇒ 下面那个文件会真落盘 ⇒ 本条当场红。
#[test]
fn the_write_entry_point_actually_goes_through_the_fence() {
    let base = temp_root("wired");
    let root = base.join("target");
    std::fs::create_dir_all(&root).expect("建目标根");
    let outside = base.join("escape.txt");

    let err = create_new_file(&root, "../escape.txt", b"x")
        .expect_err("写入口放过了一个上跳段 —— 围栏没接上");
    assert_eq!(
        err.code(),
        "refused",
        "拒是拒了，但档位不对（`refused` = 围栏拦的，`io_failed` = 盘上没成）：{err:?}"
    );
    assert!(
        err.message().contains("refuse write"),
        "拒了，但不是围栏拒的：{}",
        err.message()
    );
    assert!(
        !outside.exists(),
        "围栏说拒了，盘上却真的多出一份文件：{}",
        outside.display()
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 同上一格的 Claude 那一侧：写点落进 `.claude` 段，入口必须拒，且盘上不留东西。
#[test]
fn the_write_entry_point_refuses_a_claude_tree_target() {
    let base = temp_root("claudewire");
    let root = base.join(".claude");
    std::fs::create_dir_all(&root).expect("建一个名字以 .claude 开头的目标根");

    let err = create_new_file(&root, "a.md", b"x")
        .expect_err("写点落在 `.claude` 段底下，写入口竟然放行了");
    assert_eq!(err.code(), "refused", "档位不对：{err:?}");
    assert!(
        err.message().contains("Claude 数据源"),
        "拒了，但不是 Claude 围栏拒的：{}",
        err.message()
    );
    assert!(!root.join("a.md").exists(), "说拒了，文件却落盘了");
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 阴性对照 ＋ `O_EXCL`：干净的写要真的成功，而**第二次同名必须失败且不改既有内容**。
///
/// 这一格同时是整族的反空真锚：没有它，一个「什么都拒」的围栏能让上面全部变绿。
#[test]
fn a_clean_write_lands_once_and_never_overwrites() {
    let root = temp_root("excl");
    // 前提自检：临时目录自己不许落在 Claude 树里，否则下面这一格判的不是它该判的事。
    assert!(
        !root.components().any(|c| matches!(
            c,
            std::path::Component::Normal(s) if s.to_str().is_some_and(|x| x.starts_with(".claude"))
        )),
        "前提不成立：临时目录本身落在 Claude 树里（{}）—— 本格判不了",
        root.display()
    );

    let first = create_new_file(&root, "a.md", b"first").expect("干净的写被误拒");
    assert_eq!(
        std::fs::read(&first).expect("读回刚写的"),
        b"first",
        "写进去的字节不对"
    );

    let err = create_new_file(&root, "a.md", b"second").expect_err("同名第二次竟然成功了");
    // 🔴 档位在这一格是**承重**的：同名第二次是 `O_EXCL` 在开文件那一步兜住的，
    //    **不是**围栏拦的（围栏只判路径形状，它对「这条路径上已经有东西了」一无所知）。
    //    ⇒ 码必须是 `io_failed`；写成 `refused` 就说明有人把两档混成了一档。
    assert_eq!(
        err.code(),
        "io_failed",
        "同名第二次应该是 `O_EXCL` 兜的（`io_failed`），实得 `{}`：{err:?}",
        err.code()
    );
    assert!(
        err.message().contains("refuse write"),
        "失败了，但不是我们的报错：{}",
        err.message()
    );
    assert_eq!(
        std::fs::read(&first).expect("读回第一份"),
        b"first",
        "🔴 第二次写把既有内容改了 —— 那就不是 `O_EXCL`，是覆盖"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// ★★〔死值验补的一格 —— 09-19〕**围栏② 真的在写入口那条链上吗。**
///
/// # 它补的是一个**当场量出来的洞**，不是想出来的
///
/// 本族头一版里，围栏② 的三格全是**直接喂 `fence_resolved`** 的。
/// 死值验第一刀（把 `fenced_target` 里那句 `fence_resolved(…)` 换成 `Ok(lexical)`
/// —— 也就是**把围栏② 整个从串联里摘掉**）实测：**12 格全绿，一条没红。**
/// ⇒ 那时的围栏② 只是「一个函数被单测过」，**它在不在执行链上，盘上没有任何东西钉着**。
/// （判据不在执行链上就等于不存在。）
///
/// 本格从**入口**打进去，且专挑围栏① 看不见的那一形（词法上干净的 symlink）：
/// 摘掉围栏② ⇒ 文件会真的落进那棵树 ⇒ 本格当场红。
///
/// ⚠ 这里用「**路径段以那个前缀开头**」这一形来构造 Claude 树，而不是去改环境变量：
/// `resolve_home` 读的是进程环境，并发测试里改它是共享状态。两条判定里**这一条**
/// 不依赖环境 ⇒ 本格在任何机器上都判得动。
/// 🔴 **代价如实登记**：这也意味着本格**没有**盖到另一条判定（「在配置根之下」）
/// 在入口那条链上的情形 —— 那一维要真去切环境变量才判得了，本轮**判不了**。
#[test]
#[cfg(unix)]
fn the_write_entry_point_is_still_fenced_after_a_symlink_is_resolved() {
    let base = temp_root("wired2");
    let root = base.join("target");
    // 名字以那个前缀开头 ⇒ 不论环境变量指向哪，它都算 Claude 那几棵树里的一棵。
    let tree = base.join(".claude-deadvalue");
    std::fs::create_dir_all(&root).expect("建目标根");
    std::fs::create_dir_all(&tree).expect("建假树");
    std::os::unix::fs::symlink(&tree, root.join("docs")).expect("放 symlink");

    let err = create_new_file(&root, "docs/a.md", b"x")
        .expect_err("词法上干净、解完却落进那棵树 —— 写入口竟然放行了");
    assert_eq!(err.code(), "refused", "档位不对：{err:?}");
    assert!(
        err.message().contains("Claude 数据源"),
        "拒了，但不是围栏② 的 Claude 那一关拒的：{}",
        err.message()
    );
    assert!(
        !tree.join("a.md").exists(),
        "🔴 说拒了，文件却真的落进那棵树了：{}",
        tree.join("a.md").display()
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 上一格的**阴性对照**：同样走入口、同样经过一条 symlink，但指向的是
/// 目标根**里面**的一个普通目录 ⇒ 必须写得进去。
///
/// 没有这一格，一个「凡是碰到 symlink 就拒」的实现也能让上一格全绿。
#[test]
#[cfg(unix)]
fn a_symlink_that_stays_inside_the_target_root_still_writes() {
    let root = temp_root("wired3");
    std::fs::create_dir_all(root.join("real")).expect("建真目录");
    std::os::unix::fs::symlink(root.join("real"), root.join("link")).expect("放 symlink");

    let got = create_new_file(&root, "link/a.md", b"ok").expect("根内 symlink 被误拒");
    assert_eq!(std::fs::read(&got).expect("读回"), b"ok");
    assert!(
        got.starts_with(std::fs::canonicalize(&root).expect("解目标根")),
        "解出来的落点不在目标根里：{}",
        got.display()
    );
    std::fs::remove_dir_all(&root).ok();
}

// ════════════════════════════════════════════════════════════════════════════
//  命令面 ——〔波 5 ㈠ · 2026-09-23〕`设计/60 §8.6` **第 2 步**的验收
// ════════════════════════════════════════════════════════════════════════════
//
// `§8.6` 第 2 步逐字要验两件：
//   ① 那份原语从「**零消费者**」变成有；
//   ② 它那两道围栏（词法 ＋ 解 symlink 后再判）**在命令面这一侧也走得到**。
//
// 🔴 ② 不是「上面那几条已经验过了」的重复：上面那几条打的是 `create_new_file`
//   这个 Rust 函数，而命令面多了一层（取参 · 定 code · 拼回参）。
//   那一层**有它自己的失效形状**：把围栏的 `Err` 吞成一个成功回参、
//   或者把两档错误压成一个码 —— 两样都不会让上面任何一条红。

/// ★★ **正题 ①：零消费者变成有了。**
///
/// # 为什么是「跨文件」而不是「有人调过」
///
/// 本模块自己的判据也在调 `create_new_file` —— 拿那个当消费者是**恒真**的
///（判据永远在调它，那条零从来就不成立）。要问的是**生产段**里有没有人接。
/// ⇒ 人群是后端生产树上**除本模块自己以外**的每一份 `.rs` 的生产段。
///
/// ⚠ 它**只判「有」，不判「只从那一面来」** —— 后者是第 3 步那条判准
///（`readonly_guard` 第三层），本条刻意不替它作证。
#[test]
fn the_write_primitive_is_no_longer_a_zero_consumer_symbol() {
    let root = crate::guard_support::src_root();
    let needle = format!("files_{}::", "write");
    let mut consumers: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        scanned += 1;
        if rel == "control/files_write.rs" {
            continue; // 它自己不是自己的消费者
        }
        if guard_core::production_code(&src).contains(needle.as_str()) {
            consumers.push(rel);
        }
    }
    // 反空真：扫不到东西 ⇒ 下面那条「非空」永远说明不了什么。
    assert!(
        scanned >= 60,
        "后端生产树只扫到 {scanned} 份 `.rs` —— 采集坏了，本条此刻在空转"
    );
    consumers.sort();
    assert!(
        !consumers.is_empty(),
        "`control/files_write.rs` 在生产段里**一个跨文件消费者都没有** —— \
         那它还是 09-19 落地时那个「能力在、没人接」的状态，第 2 步没落成。\n\
         ⚠ 本模块自己那几条判据不算消费者（它们永远在调它，拿它当证据是恒真的）。"
    );
}

/// ★ 命令面这一侧走得到**围栏①（词法）**，而且盘上什么都不留。
#[test]
fn the_command_face_reaches_the_lexical_fence() {
    let base = temp_root("cmdlex");
    let root = base.join("target");
    std::fs::create_dir_all(&root).expect("建目标根");
    let outside = base.join("escape.txt");

    let (code, msg) = answer_wire(
        "files-create",
        &serde_json::json!({"root": root.to_str().expect("utf8"), "rel": "../escape.txt"}),
    )
    .expect_err("命令面放过了一个上跳段 —— 围栏① 在这一侧没接上");
    assert_eq!(code, "refused", "档位不对（{msg}）");
    assert!(msg.contains("上跳段"), "拒了，但不是词法那道拒的：{msg}");
    assert!(
        !outside.exists(),
        "命令面说拒了，盘上却真的多出一份文件：{}",
        outside.display()
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 命令面这一侧走得到**围栏②（解完 symlink 再判）**。
#[test]
#[cfg(unix)]
fn the_command_face_reaches_the_resolved_fence() {
    let base = temp_root("cmdres");
    let root = base.join("target");
    // 名字以那个前缀开头 ⇒ 不论环境变量指向哪，它都算 Claude 那几棵树里的一棵。
    let tree = base.join(".claude-deadvalue");
    std::fs::create_dir_all(&root).expect("建目标根");
    std::fs::create_dir_all(&tree).expect("建假树");
    std::os::unix::fs::symlink(&tree, root.join("docs")).expect("放 symlink");

    let (code, msg) = answer_wire(
        "files-create",
        &serde_json::json!({"root": root.to_str().expect("utf8"), "rel": "docs/a.md"}),
    )
    .expect_err("词法上干净、解完却落进那棵树 —— 命令面竟然放行了");
    assert_eq!(code, "refused", "档位不对（{msg}）");
    assert!(
        msg.contains("Claude 数据源"),
        "拒了，但不是围栏② 的 Claude 那一关拒的：{msg}"
    );
    assert!(
        !tree.join("a.md").exists(),
        "🔴 说拒了，文件却真的落进那棵树了：{}",
        tree.join("a.md").display()
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★★ **阴性对照 ＋ 「新建空文件」那一件**：不给 `content` ⇒ 真落一份 0 字节的文件。
///
/// 没有这一格，一个「命令面永远回 `refused`」的实现能让上面两条全绿。
#[test]
fn the_command_face_creates_an_empty_file_when_no_content_is_given() {
    let root = temp_root("cmdempty");
    let out = answer_wire(
        "files-create",
        &serde_json::json!({"root": root.to_str().expect("utf8"), "rel": "note/../a.md"}),
    );
    // 先把「当前目录段 / 上跳段一律拒」这条口径钉一下：上面那个 `..` 必须被拒，
    // 哪怕它抵消回了根里 —— 围栏① 逐字是「一律拒，让调用方送干净的段进来」。
    assert!(out.is_err(), "围栏① 的「不做规范化」那条口径松了");

    let data = answer_wire(
        "files-create",
        &serde_json::json!({"root": root.to_str().expect("utf8"), "rel": "a.md"}),
    )
    .expect("干净的一次新建被命令面误拒");
    assert_eq!(data["bytes"], serde_json::json!(0), "空文件应回 0 字节");
    let landed = data["path"].as_str().expect("回参里的 path 应是字符串形");
    assert_eq!(
        std::fs::read(landed).expect("读回刚建的"),
        Vec::<u8>::new(),
        "说是空文件，盘上却有内容"
    );

    // 给 `content` 的那一路也要真的写进去（否则「空」这件事说明不了什么）。
    let data = answer_wire(
        "files-create",
        &serde_json::json!({"root": root.to_str().expect("utf8"), "rel": "b.md", "content": "hi"}),
    )
    .expect("带内容的一次新建被误拒");
    assert_eq!(data["bytes"], serde_json::json!(2));
    assert_eq!(
        std::fs::read(data["path"].as_str().expect("path")).expect("读回"),
        b"hi",
        "写进去的字节不对"
    );

    // ★ 两档 code 在命令面这一侧**真的分得开**：同名第二次是 `O_EXCL` 兜的 ⇒ `io_failed`。
    let (code, msg) = answer_wire(
        "files-create",
        &serde_json::json!({"root": root.to_str().expect("utf8"), "rel": "b.md"}),
    )
    .expect_err("同名第二次竟然成功了");
    assert_eq!(
        code, "io_failed",
        "同名第二次应该是 `O_EXCL` 兜的（`io_failed`），实得 `{code}`：{msg}"
    );
    assert_eq!(
        std::fs::read(root.join("b.md")).expect("读回第一份"),
        b"hi",
        "🔴 第二次把既有内容改了 —— 那就不是 `O_EXCL`，是覆盖"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// ★ 参数那一层自己的失效形状：少参数 / 形状不对，各自落在**声明过的**那个码上。
#[test]
fn the_command_face_says_which_argument_is_wrong() {
    let root = temp_root("cmdargs");
    let r = root.to_str().expect("utf8");
    for (args, want) in [
        (serde_json::json!({"rel": "a.md"}), "bad_path"),
        (serde_json::json!({"root": "", "rel": "a.md"}), "bad_path"),
        (serde_json::json!({"root": r}), "bad_args"),
        (serde_json::json!({"root": r, "rel": 7}), "bad_args"),
        (
            serde_json::json!({"root": r, "rel": "a.md", "content": 7}),
            "bad_args",
        ),
    ] {
        let out = answer_wire("files-create", &args);
        let (code, msg) = match out {
            Err(e) => e,
            Ok(v) => panic!("这一组参数本该被拒，却过了：{args} ⇒ {v}"),
        };
        assert_eq!(code, want, "`{args}` 的码不对（{msg}）");
    }
    // 不认的命令名要被拒（否则上面每一条都对着一个「什么都收」的入口在断言）。
    assert!(
        answer_wire("files-ls", &serde_json::json!({})).is_err(),
        "写面的入口收下了一个不属于它的命令名"
    );
    std::fs::remove_dir_all(&root).ok();
}
