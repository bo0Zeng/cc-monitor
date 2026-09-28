//! 〔步 23b · 2026-09-19〕`control/files_write.rs` 的行为判据 —— **钉住那两道围栏**。
//!
//! # 🔴〔FN1 · 第四波 4C · 2026-09-25〕守的要求换了：用户 **V119**
//!
//! 用户原话「**文件管理器全部都可以改. 不需要任何围栏**」（`设计/99 §1` V119）⇒ 下面凡是
//! 「会话文件 ⇒ `refused`」的格子全部翻面成「会话文件 ⇒ **照做**，盘上逐字节核」；
//! 仍然会拒的只剩路径解析那几形（上跳 · 绝对路径 · 空段 · 父目录不在 · 解完链接跑出根）——
//! 那是「改的就是 `root ＋ rel` 那一格」的正确性，不是「不许改什么」。
//! 两道的旧名 `fence_lexical` / `fence_resolved` 今天叫 `lexical_in_root` / `resolve_parent_in_root`。
//! 下面「买到的」前三条是 FN1 之前的读数，读现状以本节为准。
//!
//! # 🔴 这一族买到的是什么、**买不到**什么（逐字，不许含糊）
//!
//! **买到的**（全部在本机临时目录上真跑，不是源码扫描）：
//!
//! - 围栏① 真的会拒：上跳段 · 绝对路径 · 空段 · **写点本身就是一份会话文件**；
//! - 围栏② 真的会拒：目标根里放一条**真的 symlink** 指向一份会话文件，解完之后被拦；
//! - 🔴〔波 5 ㈢ 09-23〕**放宽那一侧同样有判据**：`~/.claude/skills/**` ·
//!   `settings.json` · 另一个账号的账号库 —— 逐条**必须写得进去**（用户 09-23 裁
//!   「文件管理器该不该能改 `~/.claude` 里的东西. 可以.」）。
//!   ⚠ 这一侧的判据与上一侧**同等承重**：只验「该拒的拒了」，一个恒 `Err` 的围栏也全绿。
//! - 两道围栏**真的接在写入口上**：拒绝的那几次，盘上**没有**多出任何文件
//!   （删掉入口里那一句串联调用 ⇒ 文件会真落盘 ⇒ 这里当场红）；
//! - `O_EXCL` 真的是 `O_EXCL`：第二次写同一个名字失败，且**第一份内容一个字节没动**。
//!
//! **买不到的**（前提今天不成立，如实登记，不许读成「过了」）：
//!
//! 1. 🔴 **没有真远端**。本模块与本族判据全跑在**本机**文件系统上。
//!    「在一台真的远端机器上、经由后端跑过一次这条写路」这件事**本轮判不了** ——
//!    盘面上没有真远端，`设计/60 §7` 那条「未实测」照旧成立。
//! 2. ✅〔波 5 ㈢ 09-23 · **本条已假，留原话当墓碑**〕原话是
//!    「🔴 **没有覆盖「多账号同时在盘上」那一维**。`resolve_home` 只答得出此刻被选中的
//!    那一个配置根；另外几个账号目录靠「段以 `.claude` 开头」这条形状兜，
//!    而那条形状**换个名字就兜不住**」。
//!    围栏换成**结构判定**之后这一维不存在了：它不问配置根在哪。
//!    🔴 **别读成「变强了」** —— 代价是「整棵树」那一档的拦截面整个没了，而那是用户裁的。
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

// 🔴 〔波 5 ㈢ 09-23〕原来这里有一个 `fake_home()`（「一个确定不在 Claude 树里的假配置根，
//    给纯函数用例当参数」）。**围栏不再收配置根这个入参了** ⇒ 它一个调用方都没有。
//    留着它就是一个「看起来还在被用」的死夹具，删掉。
//    ⚠ 这件事本身是一条读数：**写侧围栏从此与「此刻选中哪个账号」无关**。

// ── 围栏核心判定：`is_session_record_file`〔波 5 ㈢ 09-23 换的〕──────────

/// ★ **量具自检**：这条判定两个方向都得会答。
///
/// 只验「该拒的拒了」是空真的一半：一个恒返回 `true` 的实现同样能过。
/// ⇒ 阴阳两侧同拍验，缺一侧这条就不是判据。
///
/// 🔴 **阴性那一侧这一轮是主角**：用户 09-23 裁「文件管理器该不该能改 `~/.claude`
/// 里的东西. **可以.**」⇒ 下面阴性表里那几条 `~/.claude/**` **09-23 之前是被拒的**，
/// 今天必须放行。哪一条回到「拒」，就是有人把那一裁悄悄收回去了。
#[test]
fn the_session_file_predicate_answers_both_ways() {
    use crate::agents::claudecode::paths::is_session_record_file;
    // ── 阳性：那几份具体的会话文件 ──────────────────────────────────
    for hit in [
        "/home/u/.claude/projects/-x/abc.jsonl", // `projects/` 下恰 2 段
        "/home/u/.claude/sessions/1234.json",    // pidfile，`sessions/` 下 1 段
        "/opt/accts/q/projects/-x/abc.jsonl",    // 配置根被切走，照样认得（结构判定）
        "C:\\Users\\me\\.claude\\projects\\p\\s.jsonl", // 反斜杠先归一
    ] {
        assert!(is_session_record_file(hit), "该判成会话数据却放过了：{hit}");
    }
    // ── 阴性：这些**必须**放行 ────────────────────────────────────────
    //    ★ 前四条是 09-23 那一裁买到的东西，**逐条承重**。
    for miss in [
        "/home/u/.claude/skills/my-skill/SKILL.md", // 用户 09-23 逐字点名的那一类
        "/home/u/.claude/settings.json", // `INVARIANTS §1` SS-14：写面绝不含它 —— 而「不含」≠「拒写」
        "/home/u/.claude-alt/q/.credentials.json", // 账号库
        "/home/u/.claude/projects/-x/sub/abc.jsonl", // `projects/` 下**多一层** ⇒ 不是会话文件那个位置
        "/home/u/.claude/projects/a.jsonl",          // 少一层
        "/home/u/.claude/projects/-x/abc.jsonl.bak", // 后缀差一截
        "/home/u/docs/a.md",
        "/srv/work/src/main.rs",
    ] {
        assert!(
            !is_session_record_file(miss),
            "🔴 这条路径被拒了，而 09-23 那一裁要求它放行：{miss}"
        );
    }
}

// 〔MIG-3a〕「两份会话围栏逐字节相同」那一条退役：桥那一份（`claude_data_fence.rs`〔散文墓碑〕）随它最后一个用户进了后端，全仓只剩 `paths.rs` 这一份。

// ── 围栏①（词法）──────────────────────────────────────────────────────

#[test]
fn the_lexical_fence_refuses_the_four_shapes() {
    let root = PathBuf::from("/srv/target");
    for (rel, word) in [
        ("../escape.txt", "不能有 ..："),
        ("a/../../escape.txt", "不能有 ..："),
        ("/etc/passwd", "绝对路径"),
        ("./a.txt", "不能有 .："),
        ("", "空的"),
        ("   ", "空的"),
    ] {
        let err =
            lexical_in_root(&root, rel).expect_err(&format!("围栏① 放过了 {rel:?} —— 它该被拒"));
        assert!(
            err.contains(word),
            "拒了，但说不清是哪一形（rel={rel:?}，错误={err}）"
        );
    }
}

/// ★ 阴性对照：干净的相对段必须**过得去**。
///
/// 没有这一格，一个「恒 `Err`」的围栏也能让上面那条全绿 —— 那是最典型的空真。
#[test]
fn the_lexical_fence_lets_a_clean_relative_path_through() {
    let root = PathBuf::from("/srv/target");
    let ok = lexical_in_root(&root, "docs/notes/a.md").expect("干净的相对段被误拒");
    assert_eq!(ok, root.join("docs/notes/a.md"), "落点算错了");
}

/// 🔴🔴 **〔波 5 ㈢ 09-23〕这一格整个翻了牌，而它是那一裁的正题。**〔FN1 · V119 再翻一次：向二也放行了〕
///
/// 原来的标题逐字是「目标根**自己**落在 Claude 树里时，相对段再干净也不行」，
/// 断言的是「把文件管理目标指到 `~/.claude` 底下 ⇒ 整个拒」。
/// **用户 09-23 逐字裁掉了那一句**：「文件管理器该不该能改 `~/.claude` 里的东西. 可以.」
///
/// ⇒ 今天判的是**两向**：
/// · 目标根指到 `~/.claude` 底下 ＋ 写点不是会话文件 ⇒ **放行**（那一裁买到的东西）；
/// · 同一个根 ＋ 写点**恰好**是那份会话文件 ⇒ **照旧拒**（那一裁没买到的东西）。
///
/// ⚠ 只留前一向就等于把围栏拆了；只留后一向就等于那一裁没落地。**两向缺一不可。**
#[test]
fn a_target_root_inside_the_claude_tree_is_allowed_and_so_is_a_session_file() {
    // 向一：skills 那一类 —— 必须放行。
    let ok = lexical_in_root(Path::new("/home/u/.claude/skills"), "my-skill/SKILL.md")
        .expect("🔴 用户 09-23 裁「可以」，而这条路径被拒了");
    assert_eq!(
        ok,
        PathBuf::from("/home/u/.claude/skills/my-skill/SKILL.md")
    );
    // 向二〔FN1 · V119〕：写点恰好是那份会话文件 —— 今天**也放行**（「不需要任何围栏」）。
    let ok = lexical_in_root(Path::new("/home/u/.claude/projects"), "-x/s.jsonl")
        .expect("🔴 V119「文件管理器全部都可以改」，而会话记录那一格被拒了");
    assert_eq!(ok, PathBuf::from("/home/u/.claude/projects/-x/s.jsonl"));
    // 向二之二：pidfile 那一形同样放行（两形从前都在判定里，只验一形等于半个判据）。
    let ok = lexical_in_root(Path::new("/home/u/.claude/sessions"), "4321.json")
        .expect("🔴 pidfile 那一格被拒了");
    assert_eq!(ok, PathBuf::from("/home/u/.claude/sessions/4321.json"));
}

// ── 围栏②（现打，真 symlink）──────────────────────────────────────────

/// ★★〔FN1 · V119 翻面〕目标根里藏一条 symlink，解完之后写点落到一份会话文件上 ⇒ 只要它还在根里，**放行**；
/// 指到根外 ⇒ 拒，而理由**只能是越界**（不再有「碰了 Claude 会话数据」那一句）。
///
/// 下面是 FN1 之前的原话（「本族最承重的一格」），留着说明这一格从哪来：
///
/// 围栏① 对它**完全看不见**（`docs/abc.jsonl` 在词法上干净得很 —— 它自己那一段
/// 不构成 `projects/<proj>/<sid>.jsonl` 那个形状），
/// 只有解完 symlink 再判一次才拦得住。⇒ 这一格红，说明围栏② 没了。
///
/// ⚠ 〔波 5 ㈢ 09-23〕语料跟着判定改了：此前放的 symlink 指向**一棵 `.claude` 树**，
/// 今天那已经不是拒绝理由（用户裁「可以」）⇒ 它得指向 `projects/<proj>/` ——
/// 也就是让解完之后的那条路径**恰好是**一份会话记录。
/// **这不是把用例改弱，是把它改到新判定真正的边界上**：旧语料在新判定下会放行，
/// 留着它只会让这一格以「围栏坏了」的假象红。
///
/// ★ 它同时钉着围栏② 里**两条判定的先后**：这条路径既跑出了目标根、又是一份会话文件，
/// 两条都会拒 —— 而诊断必须是「碰了 Claude 会话数据」那一句（那是这条围栏立在这里的全部理由）。
/// 把顺序换回去，本格会以「说的不是 Claude」的形式红。
#[test]
#[cfg(unix)]
fn a_symlink_onto_a_session_file_passes_inside_the_root_and_is_refused_only_for_escaping() {
    let base = temp_root("symlink");
    // 向一：根就是那个配置根，链接指到根里的会话目录 ⇒ 解完之后恰是 `projects/<proj>/<sid>.jsonl` ⇒ 放行。
    let root = base.join("cfg");
    let live = root.join("projects/-x");
    std::fs::create_dir_all(&live).expect("建会话目录");
    std::os::unix::fs::symlink(&live, root.join("docs")).expect("放 symlink");
    let lexical = lexical_in_root(&root, "docs/abc.jsonl").expect("解析① 本来就该放过它");
    let got = resolve_parent_in_root(&root, &lexical)
        .expect("🔴 V119：解完落到根里的一份会话记录上，竟然被拒了");
    assert_eq!(
        got,
        std::fs::canonicalize(&live)
            .expect("解会话目录")
            .join("abc.jsonl"),
        "解出来的落点不是那份会话记录"
    );
    // 向二：同一条链接放在**另一个根**里 ⇒ 解完跑出了根 ⇒ 拒，理由只许是越界。
    let other = base.join("target");
    std::fs::create_dir_all(&other).expect("建目标根");
    std::os::unix::fs::symlink(&live, other.join("docs")).expect("放 symlink");
    let lexical = lexical_in_root(&other, "docs/abc.jsonl").expect("解析① 本来就该放过它");
    let err = resolve_parent_in_root(&other, &lexical).expect_err("链接指出根外，解析② 竟然放行了");
    assert!(err.contains("外面，不动它"), "拒了，但说的不是越界：{err}");
    assert!(
        !err.contains("会话数据"),
        "🔴 V119 之后不该再有「会话数据」那一句：{err}"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★★ **上一格的阴性对照，而它是 09-23 那一裁在围栏② 上的正题。**
///
/// 同样是目标根里一条 symlink、同样指到一棵 `.claude` 树里，但解完之后落的是
/// `skills/` 那一类 ⇒ **必须写得进去**。
///
/// 没有这一格，把围栏② 改回「解完只要落在 `.claude` 树里就拒」会全绿 ——
/// 那正是这一刀要撤掉的那一档。
#[test]
#[cfg(unix)]
fn the_resolved_fence_lets_a_symlink_into_a_claude_tree_through_when_it_is_not_a_session_file() {
    let base = temp_root("symlinkok");
    let root = base.join("target");
    let skills = base.join(".claude/skills");
    std::fs::create_dir_all(&root).expect("建目标根");
    std::fs::create_dir_all(&skills).expect("建 skills 目录");
    std::os::unix::fs::symlink(&skills, root.join("s")).expect("放 symlink");

    let lexical = lexical_in_root(&root, "s/SKILL.md").expect("围栏① 该放过它");
    let err =
        resolve_parent_in_root(&root, &lexical).expect_err("这一格今天该以「跑出目标根」被拒");
    // 🔴 它仍然被拒，**但理由必须是越界，不是 Claude** —— 两者的差别就是这一裁的全部内容。
    assert!(
        err.contains("外面，不动它"),
        "🔴 拒的理由是 Claude 那一关，说明「整棵树」那一档没撤干净：{err}"
    );
    assert!(
        !err.contains("会话数据"),
        "🔴 skills 文件被判成了会话数据：{err}"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 同一件事**在目标根里面**做一遍：根就指在 `.claude` 底下，写点是 `skills/**`
/// ⇒ 两道围栏都得放行，而且要**真落盘**。
///
/// 上一格证明的是「理由换了」，这一格证明的是「**真的写得进去**」——
/// 缺了它，一个「一律以越界为由拒掉」的实现照样让上一格绿。
#[test]
fn a_write_into_a_claude_tree_that_is_not_session_data_really_lands() {
    let base = temp_root("skillswrite");
    // 目标根**自己**就在一棵名字以 `.claude` 开头的树里 —— 09-23 之前这是当场拒。
    let root = base.join(".claude/skills");
    std::fs::create_dir_all(&root).expect("建 skills 根");
    let got = create_new_file(&root, "my-skill/../SKILL.md", b"x");
    assert!(got.is_err(), "围栏① 的「不做规范化」那条口径松了");

    let got = create_new_file(&root, "SKILL.md", b"hello")
        .expect("🔴 用户 09-23 裁「可以」，而这一次写被拒了");
    assert_eq!(std::fs::read(&got).expect("读回"), b"hello");

    // 〔FN1 · V119〕同一棵树底下，**会话文件那一形也写得进去**（从前这里是阳性对照「照旧写不进去」）。
    let live = base.join(".claude/projects");
    std::fs::create_dir_all(live.join("-x")).expect("建 projects 根");
    let got = create_new_file(&live, "-x/s.jsonl", b"x").expect("🔴 V119：会话记录那一格被拒了");
    assert_eq!(std::fs::read(&got).expect("读回"), b"x");
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

    let lexical = lexical_in_root(&root, "out/a.md").expect("围栏① 该放过它");
    let err =
        resolve_parent_in_root(&root, &lexical).expect_err("symlink 指出目标根，围栏② 竟然放行了");
    assert!(err.contains("外面，不动它"), "拒了，但说的不是越界：{err}");
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 阴性对照：没有 symlink 的干净路径，围栏② 必须放行并解出真路径。
#[test]
fn the_resolved_fence_lets_a_clean_path_through() {
    let root = temp_root("clean2");
    std::fs::create_dir_all(root.join("docs")).expect("建子目录");
    let lexical = lexical_in_root(&root, "docs/a.md").expect("围栏① 该放过它");
    let got = resolve_parent_in_root(&root, &lexical).expect("围栏② 误拒了干净路径");
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
    let err =
        resolve_parent_in_root(&root, &root.join("nope/a.md")).expect_err("父目录不在，却没拒");
    assert!(err.contains("解析不了"), "拒的理由不对：{err}");
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
    assert_eq!(
        err.message(),
        copy_core::copy_text("beFilesWrite.path.parentStep", &[("path", "../escape.txt")]),
        "拒了，但不是围栏拒的"
    );
    assert!(
        !outside.exists(),
        "围栏说拒了，盘上却真的多出一份文件：{}",
        outside.display()
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 〔FN1 · V119 翻面〕写点**就是一份会话记录**（`projects/<proj>/<sid>.jsonl` · `sessions/<x>.json`），入口**照写**。
///
/// ⚠ 从前这一格断言「入口必须拒，且盘上不留东西」（再早是「落在 `.claude` 段底下就拒」）。
/// 用户 V119「文件管理器全部都可以改. 不需要任何围栏」⇒ 两形都真落盘，逐字节读回。
#[test]
fn the_write_entry_point_writes_a_session_file_too() {
    let base = temp_root("claudewire");
    let root = base.join(".claude");
    std::fs::create_dir_all(root.join("projects/-x")).expect("建会话目录");
    std::fs::create_dir_all(root.join("sessions")).expect("建 pidfile 目录");
    for rel in ["projects/-x/abc.jsonl", "sessions/4321.json"] {
        let got = create_new_file(&root, rel, b"{}\n")
            .unwrap_or_else(|e| panic!("🔴 V119：写入口拒了 {rel}：{e:?}"));
        assert_eq!(std::fs::read(&got).expect("读回"), b"{}\n", "{rel}");
    }
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
        err.message().starts_with("新建 "),
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
/// 本族头一版里，围栏② 的三格全是**直接喂 `resolve_parent_in_root`** 的。
/// 死值验第一刀（把 `resolve_in_root` 里那句 `resolve_parent_in_root(…)` 换成 `Ok(lexical)`
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
///
/// 〔FN1 · V119〕语料从「解完落到一份会话记录上」换成「解完落到**根外**」：前者今天放行，
/// 而这一格要钉的仍是「解析② 在写入口那条链上」—— 摘掉它，根外那一份就真落盘。
#[test]
#[cfg(unix)]
fn the_write_entry_point_is_still_fenced_after_a_symlink_is_resolved() {
    let base = temp_root("wired2");
    let root = base.join("target");
    let outside = base.join("outside");
    std::fs::create_dir_all(&root).expect("建目标根");
    std::fs::create_dir_all(&outside).expect("建根外目录");
    std::os::unix::fs::symlink(&outside, root.join("docs")).expect("放 symlink");

    let err = create_new_file(&root, "docs/abc.txt", b"x")
        .expect_err("词法上干净、解完却落到根外 —— 写入口竟然放行了");
    assert_eq!(err.code(), "refused", "档位不对：{err:?}");
    assert!(
        err.message().contains("外面，不动它"),
        "拒了，但不是解析② 拒的：{}",
        err.message()
    );
    assert!(
        !outside.join("abc.txt").exists(),
        "🔴 说拒了，文件却真的落到根外了：{}",
        outside.join("abc.txt").display()
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
    assert!(
        msg.contains("不能有 ..："),
        "拒了，但不是词法那道拒的：{msg}"
    );
    assert!(
        !outside.exists(),
        "命令面说拒了，盘上却真的多出一份文件：{}",
        outside.display()
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 命令面这一侧走得到**围栏②（解完 symlink 再判）**。
/// 〔FN1 · V119〕语料同上一族：链接指到根外（会话记录那一形今天放行）。
#[test]
#[cfg(unix)]
fn the_command_face_reaches_the_resolved_fence() {
    let base = temp_root("cmdres");
    let root = base.join("target");
    let outside = base.join("outside");
    std::fs::create_dir_all(&root).expect("建目标根");
    std::fs::create_dir_all(&outside).expect("建根外目录");
    std::os::unix::fs::symlink(&outside, root.join("docs")).expect("放 symlink");

    let (code, msg) = answer_wire(
        "files-create",
        &serde_json::json!({"root": root.to_str().expect("utf8"), "rel": "docs/abc.txt"}),
    )
    .expect_err("词法上干净、解完却落到根外 —— 命令面竟然放行了");
    assert_eq!(code, "refused", "档位不对（{msg}）");
    assert!(
        msg.contains("外面，不动它"),
        "拒了，但不是解析② 拒的：{msg}"
    );
    assert!(
        !outside.join("abc.txt").exists(),
        "🔴 说拒了，文件却真的落到根外了：{}",
        outside.join("abc.txt").display()
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

// ════════════════════════════════════════════════════════════════════════════
//  改动既有数据的那五件 ——〔波 5 ㈡ · 2026-09-23〕`设计/60 §8.6` **第 3 步**
// ════════════════════════════════════════════════════════════════════════════
//
// 每一件都**正控 ＋ 阴性对照同拍**：只验「该拒的拒了」，一个恒 `Err` 的实现也全绿；
// 只验「该成的成了」，一个不解路径的实现也全绿。
// 🔴〔FN1 · V119〕从前阴性那一侧是「那份会话文件一个字节没动」；今天会话文件那一侧翻成**正控**
//   （改得动、盘上逐字节核），阴性那一侧换成「链接指到根外 ⇒ 拒，根外那一份一个字节没动」。

/// 在 `base` 底下铺一份根外的文件（链接指过去用），回它的路径与原始字节。
#[cfg(unix)]
fn plant_outside(base: &Path) -> (PathBuf, Vec<u8>) {
    let dir = base.join("outside");
    std::fs::create_dir_all(&dir).expect("建根外目录");
    let at = dir.join("victim.txt");
    let bytes = b"outside\n".to_vec();
    std::fs::write(&at, &bytes).expect("铺根外文件");
    (at, bytes)
}

/// 在 `base` 底下铺一份「正在跑的会话记录」，回它的路径与原始字节。
fn plant_live_session(base: &Path) -> (PathBuf, Vec<u8>) {
    let dir = base.join("cfg/projects/-x");
    std::fs::create_dir_all(&dir).expect("建会话目录");
    let live = dir.join("abc.jsonl");
    let bytes = b"{\"type\":\"live\"}\n".to_vec();
    std::fs::write(&live, &bytes).expect("铺会话文件");
    (live, bytes)
}

#[test]
fn mkdir_builds_one_level_even_where_a_session_file_would_sit() {
    let base = temp_root("mk");
    let root = base.join(".claude"); // 09-23 之后：根在 `.claude` 底下是合法的
    std::fs::create_dir_all(root.join("projects")).expect("建根");
    // 正控：`skills` 那一类 —— 建得出来。
    let got =
        make_dir(&root, "skills").expect("🔴 `~/.claude/skills` 建不出来 —— 09-23 那一裁没落地");
    assert!(got.is_dir(), "说建了，盘上没有");
    // 不顺手补中间几层。
    let err = make_dir(&root, "a/b/c").expect_err("父目录不在，竟然建成了");
    assert_eq!(err.code(), "refused", "{err:?}");
    assert!(!root.join("a").exists(), "中间那几层被顺手补出来了");
    // 〔FN1 · V119〕一条长成会话文件形状的路径，建目录**也建得出来**（从前是阴性「不许」）。
    make_dir(&root, "projects/-x").expect("🔴 项目目录建不出来");
    make_dir(&root, "projects/-x/abc.jsonl").expect("🔴 V119：会话文件那个位置上建目录被拒了");
    assert!(root.join("projects/-x/abc.jsonl").is_dir());
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn rename_moves_inside_the_root_never_overwrites_and_moves_a_session_file_too() {
    let base = temp_root("mv");
    let root = base.join("cfg");
    let (live, bytes) = plant_live_session(&base);
    std::fs::write(root.join("a.md"), b"A").expect("铺 a");
    std::fs::write(root.join("b.md"), b"B").expect("铺 b");

    // 正控：改名成一个不存在的名字。
    let got = rename_entry(&root, "a.md", "c.md").expect("干净的改名被误拒");
    assert_eq!(std::fs::read(&got).expect("读回"), b"A");
    assert!(!root.join("a.md").exists(), "改名之后旧名还在");

    // 🔴 目标已在 ⇒ 拒，而且**两份内容都没动**。
    let err = rename_entry(&root, "c.md", "b.md").expect_err("改名顶掉了一份既有文件");
    assert_eq!(err.code(), "io_failed", "{err:?}");
    assert_eq!(
        std::fs::read(root.join("b.md")).expect("b"),
        b"B",
        "被顶掉的那份内容变了"
    );
    assert_eq!(std::fs::read(root.join("c.md")).expect("c"), b"A");

    // 〔FN1 · V119〕from 是会话文件 ⇒ **改得走**；to 是会话文件的名字 ⇒ **改得进去**（从前两向都拒）。
    let moved = rename_entry(&root, "projects/-x/abc.jsonl", "moved.jsonl")
        .expect("🔴 V119：会话文件改不走");
    assert_eq!(std::fs::read(&moved).expect("读改走的"), bytes);
    assert!(!live.exists(), "改名之后会话文件的旧名还在");
    let into = rename_entry(&root, "c.md", "projects/-x/new.jsonl")
        .expect("🔴 V119：普通文件改名成会话记录被拒了");
    assert_eq!(std::fs::read(&into).expect("读改进去的"), b"A");
    // 🔴 两个参数各过一遍路径解析：`to` 带上跳段 ⇒ 拒，源原样。
    let err = rename_entry(&root, "moved.jsonl", "../escaped.jsonl").expect_err("改名逃出了根");
    assert_eq!(err.code(), "refused", "{err:?}");
    assert!(
        root.join("moved.jsonl").exists(),
        "被拒的那一次把源文件挪走了"
    );
    assert!(!base.join("escaped.jsonl").exists());
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn delete_removes_files_empty_dirs_links_and_session_files_but_never_a_subtree() {
    let base = temp_root("rm");
    let root = base.join("cfg");
    let (live, bytes) = plant_live_session(&base);
    std::fs::write(root.join("x.md"), b"x").expect("铺");
    std::fs::create_dir_all(root.join("empty")).expect("铺空目录");
    std::fs::create_dir_all(root.join("full")).expect("铺非空目录");
    std::fs::write(root.join("full/keep.md"), b"k").expect("铺");
    std::os::unix::fs::symlink(&live, root.join("link.jsonl")).expect("放一条指向会话的链接");

    // 正控：文件 · 空目录。
    delete_entry(&root, "x.md").expect("删普通文件被误拒");
    assert!(!root.join("x.md").exists());
    delete_entry(&root, "empty").expect("删空目录被误拒");
    assert!(!root.join("empty").exists());
    // 链接：删的是链接本身，目标一个字节没动。
    delete_entry(&root, "link.jsonl").expect("删链接被误拒");
    assert!(
        std::fs::symlink_metadata(root.join("link.jsonl")).is_err(),
        "链接还在"
    );
    assert_eq!(
        std::fs::read(&live).expect("读会话"),
        bytes,
        "🔴 删链接时跟了过去"
    );

    // 🔴 不递归：非空目录 ⇒ 系统报错，里面的东西还在。
    let err = delete_entry(&root, "full").expect_err("非空目录被删掉了 —— 递归删没签字");
    assert_eq!(err.code(), "io_failed", "{err:?}");
    assert!(root.join("full/keep.md").exists(), "非空目录里的东西没了");

    // 〔FN1 · V119〕会话文件本身 ⇒ **删得掉**（从前是阴性「拒，盘上原样」）。
    delete_entry(&root, "projects/-x/abc.jsonl").expect("🔴 V119：会话文件删不掉");
    assert!(!live.exists(), "说删了，会话文件还在");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn chmod_follows_links_so_it_resolves_to_the_end_before_judging() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = temp_root("chmod");
    let root = base.join("cfg");
    let (live, _) = plant_live_session(&base);
    let (outside, _) = plant_outside(&base);
    let before = std::fs::metadata(&outside)
        .expect("读根外元数据")
        .permissions()
        .mode();
    std::fs::write(root.join("run.sh"), b"#!/bin/sh\n").expect("铺");
    std::os::unix::fs::symlink(&live, root.join("innocent.txt")).expect("放链接");
    std::os::unix::fs::symlink(&outside, root.join("away.txt")).expect("放指到根外的链接");

    // 正控。
    change_mode(&root, "run.sh", 0o700).expect("干净的改权限被误拒");
    let m = std::fs::metadata(root.join("run.sh"))
        .expect("读")
        .permissions()
        .mode();
    assert_eq!(m & 0o7777, 0o700, "说改了，盘上的权限位不对");

    // 〔FN1 · V119〕名字干净（`innocent.txt`），解到底是会话文件 ⇒ **照改**，改的是那份真文件。
    change_mode(&root, "innocent.txt", 0o600).expect("🔴 V119：经链接改会话文件的权限被拒了");
    let m = std::fs::metadata(&live)
        .expect("读会话元数据")
        .permissions()
        .mode();
    assert_eq!(m & 0o7777, 0o600, "改的不是链接那一头的真文件");
    // 🔴 阴性：名字干净（`away.txt`），解到底跑出了根 ⇒ 拒，而且根外那一份权限位没变。
    //    只解父目录的那一道（`resolve_in_root`）在这一形上是**瞎的** —— 这一格就是
    //    `resolve_existing_in_root` 存在的理由。
    let err =
        change_mode(&root, "away.txt", 0o000).expect_err("借一条链接把根外那一份改成了不可读");
    assert_eq!(err.code(), "refused", "{err:?}");
    let after = std::fs::metadata(&outside)
        .expect("读根外元数据")
        .permissions()
        .mode();
    assert_eq!(before, after, "🔴 根外那一份的权限位被改了");

    // 超出低 12 位 ⇒ 拒。
    let err = change_mode(&root, "run.sh", 0o100000).expect_err("高位被收下了");
    assert_eq!(err.code(), "refused", "{err:?}");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn overwrite_replaces_an_existing_regular_file_and_nothing_else() {
    let base = temp_root("ow");
    let root = base.join("cfg");
    let (live, _) = plant_live_session(&base);
    let (outside, bytes) = plant_outside(&base);
    std::fs::write(root.join("a.md"), b"old").expect("铺");
    std::os::unix::fs::symlink(&live, root.join("notes.md")).expect("放链接");
    std::os::unix::fs::symlink(&outside, root.join("away.md")).expect("放指到根外的链接");

    // 正控。
    let got = overwrite_text(&root, "a.md", b"new").expect("干净的覆盖写被误拒");
    assert_eq!(std::fs::read(&got).expect("读回"), b"new");

    // 〔FN1 · V119〕链接指向会话文件 ⇒ **照写**，写的是那份真文件，链接还是链接。
    overwrite_text(&root, "notes.md", b"edited\n").expect("🔴 V119：经链接覆盖会话文件被拒了");
    assert_eq!(std::fs::read(&live).expect("读会话"), b"edited\n");
    assert!(std::fs::symlink_metadata(root.join("notes.md"))
        .expect("meta")
        .file_type()
        .is_symlink());
    // 🔴 阴性：链接指到根外 ⇒ 拒，根外那一份一个字节没动。
    let err = overwrite_text(&root, "away.md", b"PWNED").expect_err("借链接覆盖了根外那一份");
    assert_eq!(err.code(), "refused", "{err:?}");
    assert_eq!(
        std::fs::read(&outside).expect("读根外"),
        bytes,
        "🔴 根外那一份被覆盖了"
    );
    // 不存在 ⇒ 拒（新建走 O_EXCL 那条路，不在这里）。
    assert!(
        overwrite_text(&root, "nope.md", b"x").is_err(),
        "覆盖写顺手新建了文件"
    );
    assert!(!root.join("nope.md").exists());
    // 目录 ⇒ 拒。
    let err = overwrite_text(&root, "projects", b"x").expect_err("对着目录覆盖写");
    assert_eq!(err.code(), "refused", "{err:?}");
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 命令面这一侧五条都**够得到**，路径解析在命令面上照样咬；〔FN1 · V119〕会话文件那一侧五条全做成。
#[test]
fn the_five_mutating_commands_are_reachable_and_fenced_on_the_command_face() {
    let base = temp_root("cmd5");
    let root = base.join("cfg");
    let (live, bytes) = plant_live_session(&base);
    std::fs::write(root.join("a.md"), b"a").expect("铺");
    let r = root.to_str().expect("utf8");
    let ok = |cmd: &str, args: serde_json::Value| {
        answer_wire(cmd, &args).unwrap_or_else(|e| panic!("`{cmd}` 被误拒：{e:?}"))
    };
    ok("files-mkdir", serde_json::json!({"root": r, "rel": "d"}));
    ok(
        "files-rename",
        serde_json::json!({"root": r, "from": "a.md", "to": "d/b.md"}),
    );
    ok(
        "files-write-text",
        serde_json::json!({"root": r, "rel": "d/b.md", "content": "zz", "expect": {"sha256": content_sha256(b"a")}}),
    );
    #[cfg(unix)]
    ok(
        "files-chmod",
        serde_json::json!({"root": r, "rel": "d/b.md", "mode": 384}),
    );
    ok(
        "files-delete",
        serde_json::json!({"root": r, "rel": "d/b.md"}),
    );
    assert!(!root.join("d/b.md").exists(), "删了一圈，盘上还在");

    // 〔FN1 · V119〕同样五条，对着那份会话文件 ⇒ **全部做成**（从前全部 `refused`）。
    assert!(!bytes.is_empty());
    ok(
        "files-mkdir",
        serde_json::json!({"root": r, "rel": "projects/-x/n.jsonl"}),
    );
    assert!(root.join("projects/-x/n.jsonl").is_dir());
    ok(
        "files-write-text",
        serde_json::json!({"root": r, "rel": "projects/-x/abc.jsonl", "content": "x", "expect": {"sha256": content_sha256(&bytes)}}),
    );
    assert_eq!(std::fs::read(&live).expect("读会话"), b"x");
    #[cfg(unix)]
    ok(
        "files-chmod",
        serde_json::json!({"root": r, "rel": "projects/-x/abc.jsonl", "mode": 384}),
    );
    ok(
        "files-rename",
        serde_json::json!({"root": r, "from": "projects/-x/abc.jsonl", "to": "projects/-x/z.jsonl"}),
    );
    assert!(!live.exists() && root.join("projects/-x/z.jsonl").exists());
    ok(
        "files-delete",
        serde_json::json!({"root": r, "rel": "projects/-x/z.jsonl"}),
    );
    assert!(
        !root.join("projects/-x/z.jsonl").exists(),
        "说删了，盘上还在"
    );
    // 阴性：路径解析那一形照旧在命令面上咬（上跳段 ⇒ `refused`，根外一个字节不多）。
    for (cmd, args) in [
        (
            "files-mkdir",
            serde_json::json!({"root": r, "rel": "../esc"}),
        ),
        (
            "files-rename",
            serde_json::json!({"root": r, "from": "keep0.md", "to": "../esc"}),
        ),
        (
            "files-write-text",
            serde_json::json!({"root": r, "rel": "../esc", "content": "x", "expect": {"sha256": "0".repeat(SHA256_HEX_LEN)}}),
        ),
        (
            "files-chmod",
            serde_json::json!({"root": r, "rel": "../esc", "mode": 0}),
        ),
        (
            "files-delete",
            serde_json::json!({"root": r, "rel": "../esc"}),
        ),
    ] {
        let (code, msg) = match answer_wire(cmd, &args) {
            Err(e) => e,
            Ok(v) => panic!("🔴 `{cmd}` 带上跳段成功了：{v}"),
        };
        assert_eq!(code, "refused", "`{cmd}` 档位不对：{msg}");
    }
    assert!(!base.join("esc").exists(), "🔴 根外多出了东西");
    // `files-write-text` 不给 `content` ⇒ 不许默认成空。
    std::fs::write(root.join("keep.md"), b"keep").expect("铺");
    let (code, _) = answer_wire(
        "files-write-text",
        &serde_json::json!({"root": r, "rel": "keep.md"}),
    )
    .expect_err("不给 content 竟然写了");
    assert_eq!(code, "bad_args");
    assert_eq!(
        std::fs::read(root.join("keep.md")).expect("读"),
        b"keep",
        "文件被清空了"
    );
    std::fs::remove_dir_all(&base).ok();
}

// ════════════════════════════════════════════════════════════════════════════
//  〔F7a · 第三波 · 2026-09-24〕`files-copy` —— 同根内复制（`设计/60 §13`）
// ════════════════════════════════════════════════════════════════════════════
//
// 正控 ＋ 阴性对照同拍（同上一节那条口径）；阴性那一侧每一条都去盘上核
// 「那份会话文件一个字节没动」「目标没被顺手建出来」—— 回了 `Err` 不算数。

/// 目录里有没有复制留下的暂存旁名（`.<名>.ccm-copy-<pid>-<序号>.part`）。
///
/// ⚠ **不列目录**（`scanning_guard_registry` 不许测试段裸遍历目录）：旁名的形状是确定的，
/// 本进程造过的序号是 `0..COPY_SEQ` ⇒ 逐个按名字问「在不在」。
fn copy_leftovers(dir: &Path, name: &str) -> Vec<String> {
    let upto = COPY_SEQ.load(std::sync::atomic::Ordering::Relaxed);
    (0..upto)
        .map(|seq| format!(".{name}.ccm-copy-{}-{seq}.part", std::process::id()))
        .filter(|n| std::fs::symlink_metadata(dir.join(n)).is_ok())
        .collect()
}

/// ★ 缺省不覆盖：复制出来逐字节相等；目标已在 ⇒ `io_failed`、**两份都一个字节没动**。
#[test]
fn copy_lands_a_byte_exact_copy_and_never_overwrites_unless_asked() {
    let base = temp_root("cp");
    let root = base.join("cfg");
    std::fs::create_dir_all(&root).expect("建根");
    std::fs::write(root.join("a.md"), b"AAAA\n").expect("铺 a");
    let (got, n) = copy_entry(&root, "a.md", "b.md", false).expect("干净的复制被误拒");
    assert_eq!(std::fs::read(&got).expect("读回"), b"AAAA\n");
    assert_eq!(n, 5, "回报的字节数与真复制的不相等");
    std::fs::write(root.join("b.md"), b"OLD").expect("改 b");
    let err = copy_entry(&root, "a.md", "b.md", false).expect_err("🔴 缺省复制顶掉了一份既有文件");
    assert_eq!(err.code(), "io_failed", "{err:?}");
    assert_eq!(
        std::fs::read(root.join("b.md")).expect("b"),
        b"OLD",
        "被拒的那一趟改了目标"
    );
    assert_eq!(
        std::fs::read(root.join("a.md")).expect("a"),
        b"AAAA\n",
        "源被动了"
    );
    assert!(
        copy_leftovers(&root, "b.md").is_empty(),
        "留下了暂存旁名：{:?}",
        copy_leftovers(&root, "b.md")
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 〔W5-FILES〕要求住址：`设计/60 §7 #11`「复制出来的新文件权限位不从源抄」（登记为开着的缺陷）。
///
/// 源 `0o751` / `0o600` —— 进程缺省（umask 022 ⇒ `0o644`）给不出来的两个值 ⇒ 抄没抄分得开。
/// 不覆盖（目标本身新建）与显式覆盖（暂存旁名换名上位）两支各一格；两格目标逐位 == 源。
#[test]
#[cfg(unix)]
fn a_copy_carries_the_source_permission_bits_on_both_branches() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = temp_root("cpm");
    let root = base.join("cfg");
    std::fs::create_dir_all(&root).expect("建根");
    let mode = |p: &Path| std::fs::metadata(p).expect("stat").permissions().mode() & 0o7777;
    for (src, bits) in [("x.sh", 0o751u32), ("secret", 0o600u32)] {
        std::fs::write(root.join(src), b"s").expect("铺源");
        std::fs::set_permissions(root.join(src), std::fs::Permissions::from_mode(bits))
            .expect("设源权限");
        let fresh = format!("{src}.new");
        copy_entry(&root, src, &fresh, false).expect("不覆盖那一支被拒");
        assert_eq!(
            mode(&root.join(&fresh)),
            bits,
            "不覆盖那一支没抄权限位（{src}）"
        );
        let old = format!("{src}.old");
        std::fs::write(root.join(&old), b"o").expect("铺旧目标");
        std::fs::set_permissions(root.join(&old), std::fs::Permissions::from_mode(0o644))
            .expect("设旧目标权限");
        copy_entry(&root, src, &old, true).expect("覆盖那一支被拒");
        assert_eq!(
            mode(&root.join(&old)),
            bits,
            "覆盖那一支没抄权限位（{src}）"
        );
    }
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 显式覆盖：目标换成新内容，**不留暂存旁名**；目标是一条链接时顶掉的是**链接本身**，
/// 它指着的那份会话记录一个字节没动（〔FN1〕这一条不是围栏：复制的目标作用在链接本身上，不跟过去）。
#[test]
#[cfg(unix)]
fn explicit_overwrite_replaces_the_link_itself_and_leaves_no_side_file() {
    let base = temp_root("cpo");
    let root = base.join("cfg");
    let (live, bytes) = plant_live_session(&base);
    std::fs::write(root.join("a.md"), b"NEW").expect("铺 a");
    std::fs::write(root.join("b.md"), b"OLD").expect("铺 b");
    copy_entry(&root, "a.md", "b.md", true).expect("显式覆盖被拒了");
    assert_eq!(std::fs::read(root.join("b.md")).expect("b"), b"NEW");
    // 目标是一条指向会话文件的链接：不覆盖 ⇒ `O_EXCL` 在链接上就失败；覆盖 ⇒ 顶掉链接。
    std::os::unix::fs::symlink(&live, root.join("ln.md")).expect("铺链接");
    let err = copy_entry(&root, "a.md", "ln.md", false).expect_err("链接上 O_EXCL 竟然成了");
    assert_eq!(err.code(), "io_failed", "{err:?}");
    copy_entry(&root, "a.md", "ln.md", true).expect("覆盖一条链接被拒了");
    assert!(
        !std::fs::symlink_metadata(root.join("ln.md"))
            .expect("ln")
            .file_type()
            .is_symlink(),
        "覆盖之后那一格还是链接 —— 那就是跟过去写了"
    );
    assert_eq!(
        std::fs::read(&live).expect("读会话"),
        bytes,
        "🔴 会话记录被经由链接改写了"
    );
    for n in ["b.md", "ln.md"] {
        assert!(
            copy_leftovers(&root, n).is_empty(),
            "留下了暂存旁名：{:?}",
            copy_leftovers(&root, n)
        );
    }
    std::fs::remove_dir_all(&base).ok();
}

/// 〔FN1 · V119 翻面〕源是会话文件 / 源是指向会话文件的链接 / 目标是会话文件的位置 ⇒ **全部复制成**；
/// 三条路径各过一遍路径解析：源经一条链接跑出根 · 目标带上跳段 ⇒ `refused`，根外一个字节不动、被拒的目标没被建出来。
#[test]
#[cfg(unix)]
fn copy_is_fenced_on_the_source_the_target_and_through_a_link() {
    let base = temp_root("cpf");
    let root = base.join("cfg");
    let (live, bytes) = plant_live_session(&base);
    let (outside, out_bytes) = plant_outside(&base);
    std::fs::write(root.join("a.md"), b"A").expect("铺 a");
    std::os::unix::fs::symlink(&live, root.join("peek.md")).expect("铺链接");
    std::os::unix::fs::symlink(&outside, root.join("away.md")).expect("铺指到根外的链接");
    for (from, to, want) in [
        ("projects/-x/abc.jsonl", "copied.jsonl", &bytes[..]),
        ("peek.md", "copied.md", &bytes[..]),
        ("a.md", "projects/-x/new.jsonl", &b"A"[..]),
    ] {
        let (at, _) = copy_entry(&root, from, to, false)
            .unwrap_or_else(|e| panic!("🔴 V119：{from} → {to} 被拒了：{e:?}"));
        assert_eq!(std::fs::read(&at).expect("读复制品"), want, "{from} → {to}");
    }
    for (from, to, why) in [
        ("away.md", "stolen.md", "源是指向根外的链接（解到底再判）"),
        ("a.md", "../stolen.md", "目标带上跳段"),
    ] {
        for overwrite in [false, true] {
            let err = copy_entry(&root, from, to, overwrite)
                .expect_err(&format!("🔴 {why}（overwrite={overwrite}）竟然复制成了"));
            assert_eq!(err.code(), "refused", "{why}：{err:?}");
        }
    }
    assert!(!root.join("stolen.md").exists() && !base.join("stolen.md").exists());
    assert_eq!(
        std::fs::read(&outside).expect("读根外"),
        out_bytes,
        "🔴 根外那一份被动了"
    );
    assert!(copy_leftovers(&root, "stolen.md").is_empty());
    std::fs::remove_dir_all(&base).ok();
}

/// 形状类的拒：目录 · 源即目标 · 上跳段 · `overwrite` 不是布尔 —— 各落自己的码，盘上不多一个字节。
/// 命令面那一侧真够得到它，回参的键 == `MANAGE_COMMANDS` 声明的 `fields`（两向）。
#[test]
fn copy_refuses_its_shapes_and_is_reachable_on_the_command_face() {
    let base = temp_root("cps");
    let root = base.join("cfg");
    std::fs::create_dir_all(root.join("d")).expect("建根");
    std::fs::write(root.join("a.md"), b"A").expect("铺 a");
    let r = root.to_str().expect("utf8");
    for (args, want) in [
        (
            serde_json::json!({"root": r, "from": "d", "to": "d2"}),
            "refused",
        ),
        (
            serde_json::json!({"root": r, "from": "a.md", "to": "a.md"}),
            "refused",
        ),
        (
            serde_json::json!({"root": r, "from": "a.md", "to": "../x.md"}),
            "refused",
        ),
        (
            serde_json::json!({"root": r, "from": "a.md", "to": "z.md", "overwrite": 1}),
            "bad_args",
        ),
        (serde_json::json!({"root": r, "from": "a.md"}), "bad_args"),
    ] {
        match answer_wire("files-copy", &args) {
            Err((c, m)) => assert_eq!(c, want, "{args}：{m}"),
            Ok(v) => panic!("🔴 {args} 竟然成了：{v}"),
        }
    }
    assert!(
        !root.join("d2").exists() && !root.join("z.md").exists() && !base.join("x.md").exists()
    );
    let v = answer_wire(
        "files-copy",
        &serde_json::json!({"root": r, "from": "a.md", "to": "c.md"}),
    )
    .expect("命令面上一趟干净的复制被拒了");
    let got: std::collections::BTreeSet<&str> = v
        .as_object()
        .expect("对象")
        .keys()
        .map(String::as_str)
        .collect();
    let declared: std::collections::BTreeSet<&str> = MANAGE_COMMANDS
        .iter()
        .find(|c| c.name == "files-copy")
        .expect("在表里")
        .fields
        .iter()
        .copied()
        .collect();
    assert_eq!(
        got, declared,
        "`files-copy` 真回出去的键与声明的 `fields` 对不上"
    );
    assert_eq!(std::fs::read(root.join("c.md")).expect("c"), b"A");
    std::fs::remove_dir_all(&base).ok();
}

// ════════════════════════════════════════════════════════════════════════════
//  〔FW5 · 第四波 · 2026-09-24〕递归删：**逐条目过围栏**（设计住 `调研/第四波记录/FW5.md` 第一节）
// ════════════════════════════════════════════════════════════════════════════
//
// 正控 ＋ 阴性对照同拍；阴性那一侧每一条都**去盘上核**「树里每一样东西都还在」
// —— 回了 `Err` 不算数（一个先删后报错的实现也回 `Err`）。
// ⚠ 不列目录（`scanning_guard_registry` 不许测试段裸遍历）：铺的是什么是已知的，逐个按名字问在不在。

/// 在 `root/t` 底下铺一棵小树，回树里每一样东西的相对段（含 `t` 自己）。
fn plant_tree(root: &Path) -> Vec<&'static str> {
    std::fs::create_dir_all(root.join("t/a/b")).expect("铺树");
    std::fs::create_dir_all(root.join("t/empty")).expect("铺空目录");
    std::fs::write(root.join("t/one.md"), b"1").expect("铺");
    std::fs::write(root.join("t/a/two.md"), b"2").expect("铺");
    std::fs::write(root.join("t/a/b/three.md"), b"3").expect("铺");
    vec![
        "t",
        "t/a",
        "t/a/b",
        "t/a/b/three.md",
        "t/a/two.md",
        "t/empty",
        "t/one.md",
    ]
}

fn still_there(root: &Path, rels: &[&str]) -> Vec<String> {
    rels.iter()
        .filter(|r| std::fs::symlink_metadata(root.join(r)).is_ok())
        .map(|r| (*r).to_string())
        .collect()
}

#[test]
#[cfg(unix)]
fn a_recursive_delete_removes_the_whole_tree_and_counts_every_entry_but_never_follows_a_link() {
    let base = temp_root("rmtree");
    let root = base.join("cfg");
    std::fs::create_dir_all(&root).expect("建根");
    let planted = plant_tree(&root);
    // 树里一条指向**树外目录**的链接：删的是链接本身，不走进去。
    let outside = base.join("outside");
    std::fs::create_dir_all(&outside).expect("铺树外");
    std::fs::write(outside.join("keep.md"), b"keep").expect("铺树外文件");
    std::os::unix::fs::symlink(&outside, root.join("t/a/out")).expect("放链接");

    let (at, removed) = delete_tree(&root, "t").expect("干净的递归删被误拒");
    assert_eq!(at.file_name().and_then(|n| n.to_str()), Some("t"));
    assert_eq!(
        removed,
        planted.len() + 1,
        "删掉的条数不对（树里 {} 条 ＋ 一条链接）",
        planted.len()
    );
    assert!(still_there(&root, &planted).is_empty(), "树里还剩东西");
    assert!(
        std::fs::symlink_metadata(root.join("t/a/out")).is_err(),
        "链接还在"
    );
    assert_eq!(
        std::fs::read(outside.join("keep.md")).expect("读树外"),
        b"keep",
        "🔴 递归删顺着链接走进了树外"
    );
    // 一份普通文件走递归那条路也只删它自己。
    std::fs::write(root.join("solo.md"), b"s").expect("铺");
    assert_eq!(delete_tree(&root, "solo.md").expect("删单个文件").1, 1);
    std::fs::remove_dir_all(&base).ok();
}

/// 〔FN1 · V119 翻面〕FW5 那时本节的正题是「顶上那个目录干净，底下藏着一份会话文件 ⇒ **整趟拒，一个字节不动**」。
/// 用户「文件管理器全部都可以改. 不需要任何围栏」⇒ 两形今天都**整棵删掉**、条数逐一数对：
/// 会话文件**在**树里（有人把 `projects/` 拷进了目标）· 目标就是 `~/.claude/projects/<proj>` 本身（命令面也一样）。
#[test]
fn a_session_file_anywhere_in_the_tree_goes_with_the_tree() {
    let base = temp_root("rmtree-live");
    let root = base.join("cfg");
    let (live, bytes) = plant_live_session(&base); // cfg/projects/-x/abc.jsonl
    let planted = plant_tree(&root);
    std::fs::create_dir_all(root.join("t/a/projects/-y")).expect("铺");
    std::fs::write(root.join("t/a/projects/-y/s1.jsonl"), b"{}\n").expect("藏一份会话形状的文件");
    let mut all = planted.clone();
    all.extend([
        "t/a/projects",
        "t/a/projects/-y",
        "t/a/projects/-y/s1.jsonl",
    ]);

    let (_, removed) = delete_tree(&root, "t").expect("🔴 V119：底下藏着会话文件，递归删被拒了");
    assert_eq!(removed, all.len(), "删掉的条数不对");
    assert!(still_there(&root, &all).is_empty(), "树里还剩东西");

    // 目标就是那个项目目录本身。
    assert!(!bytes.is_empty());
    let (_, removed) =
        delete_tree(&root, "projects/-x").expect("🔴 V119：项目目录连同会话文件删不掉");
    assert_eq!(removed, 2, "项目目录 ＋ 一份会话文件");
    assert!(!live.exists());
    // 命令面同一件事：`recursive: true` ⇒ 整个 `projects` 删掉。
    std::fs::create_dir_all(root.join("projects/-z")).expect("再铺");
    std::fs::write(root.join("projects/-z/s2.jsonl"), b"{}\n").expect("再铺一份会话");
    let r = root.to_str().expect("utf8");
    let v = answer_wire(
        "files-delete",
        &serde_json::json!({"root": r, "rel": "projects", "recursive": true}),
    )
    .expect("🔴 V119：命令面上递归删 projects 被拒了");
    assert_eq!(v["removed"], 3, "{v}");
    assert!(!root.join("projects").exists());
    std::fs::remove_dir_all(&base).ok();
}

/// ★ **执行趟只删计划里的**：计划之后新长出来的东西（含一份会话形状的文件）不被连带删掉。
///
/// 手工走一遍 [`delete_tree`] 的执行趟（计划 → 中间插一件事 → 倒序逐条删），把「两趟之间」那个窗摆出来。
#[test]
fn the_execution_pass_only_deletes_what_was_planned_and_stops_on_a_newcomer() {
    let base = temp_root("rmtree-new");
    let root = base.join("cfg");
    std::fs::create_dir_all(&root).expect("建根");
    plant_tree(&root);
    let plan = plan_tree(&root, "t").expect("计划被误拒");
    assert_eq!(
        plan.first().map(|p| p.rel.as_path()),
        Some(Path::new("t")),
        "先序：目标自己排第一"
    );
    // 两趟之间：有人往 `t/a/b` 里放了一份新文件。
    std::fs::write(root.join("t/a/b/new.jsonl"), b"new").expect("插一份");
    let mut removed = 0usize;
    let mut stopped = None;
    for p in plan.iter().rev() {
        match remove_planned(&root, p) {
            Ok(_) => removed += 1,
            Err(e) => {
                stopped = Some((p.rel.clone(), e));
                break;
            }
        }
    }
    let (at, e) = stopped.expect("🔴 计划之外的那一份被连带删掉了（整趟都成了）");
    assert_eq!(at, Path::new("t/a/b"), "停在了别处：{e:?}");
    assert_eq!(e.code(), "io_failed", "{e:?}");
    assert!(removed >= 1, "一条都没删 —— 这条判据没走到执行趟");
    assert_eq!(
        std::fs::read(root.join("t/a/b/new.jsonl")).expect("读新来的"),
        b"new",
        "🔴 计划之外新长出来的那一份被删了"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★ **执行趟每一条当场再过一次围栏**：两趟之间把计划里的一层目录换成一条指向树外的链接 ⇒
/// 那一层底下的那几条再判时父目录解出去了 ⇒ 拒；树外一个字节不动。
#[test]
#[cfg(unix)]
fn the_execution_pass_refences_every_entry_so_a_swapped_in_link_is_caught() {
    let base = temp_root("rmtree-swap");
    let root = base.join("cfg");
    std::fs::create_dir_all(&root).expect("建根");
    plant_tree(&root);
    let plan = plan_tree(&root, "t").expect("计划被误拒");
    // 树外那一份，名字与计划里 `t/a/b/three.md` 同名。
    let outside = base.join("outside");
    std::fs::create_dir_all(&outside).expect("铺树外");
    std::fs::write(outside.join("three.md"), b"victim").expect("铺受害者");
    std::fs::remove_file(root.join("t/a/b/three.md")).expect("挪走");
    std::fs::remove_dir(root.join("t/a/b")).expect("挪走");
    std::os::unix::fs::symlink(&outside, root.join("t/a/b")).expect("换成链接");

    let victim = plan
        .iter()
        .find(|p| p.rel == Path::new("t/a/b/three.md"))
        .expect("计划里有那一条");
    let err = remove_planned(&root, victim).expect_err("🔴 顺着换进来的链接删到了树外");
    assert_eq!(err.code(), "refused", "{err:?}");
    assert_eq!(
        std::fs::read(outside.join("three.md")).expect("读受害者"),
        b"victim",
        "🔴 树外那一份被删了"
    );
    // 重新计划再整趟跑：那一层这回被看成**一条链接**（不跟过去）⇒ 删的是链接本身，树外原样。
    delete_tree(&root, "t").expect("重新计划之后整棵树（连同那条链接本身）删不掉");
    assert!(std::fs::symlink_metadata(root.join("t")).is_err(), "树还在");
    assert_eq!(
        std::fs::read(outside.join("three.md")).expect("读受害者"),
        b"victim",
        "🔴 整趟递归删删到了树外"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 条目数上限：超了**整趟拒**，一条都不删（拿一个小上限验，不必真铺十万条）。
#[test]
fn a_tree_over_the_cap_is_refused_whole() {
    let base = temp_root("rmtree-cap");
    let root = base.join("cfg");
    std::fs::create_dir_all(&root).expect("建根");
    let planted = plant_tree(&root);
    let err = plan_tree_within(&root, Path::new("t"), 3).expect_err("超了上限竟然出了计划");
    assert_eq!(err.code(), "refused", "{err:?}");
    assert!(
        err.message().contains('3'),
        "没说上限是多少：{}",
        err.message()
    );
    assert_eq!(
        still_there(&root, &planted).len(),
        planted.len(),
        "计划趟动了盘"
    );
    // 阴性：上限恰好够 ⇒ 过。
    assert_eq!(
        plan_tree_within(&root, Path::new("t"), planted.len())
            .expect("恰好够的上限被误拒")
            .len(),
        planted.len()
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 命令面：`recursive` **显式**才递归；形状不对 ⇒ `bad_args`；出方向带 `removed`。
#[test]
fn the_command_face_recurses_only_when_asked_and_says_how_many() {
    let base = temp_root("rmtree-cmd");
    let root = base.join("cfg");
    std::fs::create_dir_all(&root).expect("建根");
    let planted = plant_tree(&root);
    let r = root.to_str().expect("utf8");
    // 不给 ⇒ 不递归（非空目录 ⇒ io_failed，树原样）。
    let (code, _) = answer_wire("files-delete", &serde_json::json!({"root": r, "rel": "t"}))
        .expect_err("🔴 没说 recursive 就删掉了整棵树");
    assert_eq!(code, "io_failed");
    // 给了但不是布尔 ⇒ 不猜。
    for bad in [serde_json::json!("yes"), serde_json::json!(1)] {
        let (code, _) = answer_wire(
            "files-delete",
            &serde_json::json!({"root": r, "rel": "t", "recursive": bad}),
        )
        .expect_err("`recursive` 收下了一个不是布尔的值");
        assert_eq!(code, "bad_args");
    }
    assert_eq!(
        still_there(&root, &planted).len(),
        planted.len(),
        "被拒的几次动了盘"
    );
    let v = answer_wire(
        "files-delete",
        &serde_json::json!({"root": r, "rel": "t", "recursive": true}),
    )
    .expect("显式递归删被误拒");
    assert_eq!(v["removed"], planted.len(), "{v}");
    assert!(still_there(&root, &planted).is_empty());
    // 非递归那一支也回 `removed`（恒 1）。
    std::fs::write(root.join("x.md"), b"x").expect("铺");
    let v = answer_wire(
        "files-delete",
        &serde_json::json!({"root": r, "rel": "x.md"}),
    )
    .expect("删文件被误拒");
    assert_eq!(v["removed"], 1, "{v}");
    std::fs::remove_dir_all(&base).ok();
}

// ── 〔FW5〕乱码文件名（非 UTF-8）：相对段走 `{"b16": …}` ──────────────────────

/// 🔴 非 UTF-8 的名字**能改名、能改权限、能删** —— 此前相对段只收 UTF-8 字符串，这一族一件都做不了。
///
/// 阴性对照同拍：b16 解出来的段照样逐段过路径解析（上跳段 ⇒ `refused`；〔FN1〕会话文件形状那一格今天放行）。
#[test]
#[cfg(unix)]
fn a_non_utf8_name_can_be_renamed_chmodded_and_deleted_through_b16() {
    use std::os::unix::ffi::OsStrExt as _;
    let base = temp_root("b16");
    let root = base.join("cfg");
    let (live, bytes) = plant_live_session(&base);
    let raw: &[u8] = b"caf\xe9.txt"; // Latin-1 的 é —— 不是合法 UTF-8
    let name = std::ffi::OsStr::from_bytes(raw);
    std::fs::write(root.join(name), b"latin").expect("铺乱码名文件");
    let r = root.to_str().expect("utf8");
    let b16 = |b: &[u8]| serde_json::json!({ "b16": b.iter().map(|x| format!("{x:02x}")).collect::<String>() });

    // 字符串那一形照旧寻址不到它（有损串里是 U+FFFD，不是那个字节）。
    assert!(
        answer_wire(
            "files-chmod",
            &serde_json::json!({"root": r, "rel": String::from_utf8_lossy(raw), "mode": 384}),
        )
        .is_err(),
        "有损串竟然寻址到了原文件 —— 那这条判据量不出 b16 的价值"
    );
    answer_wire(
        "files-chmod",
        &serde_json::json!({"root": r, "rel": b16(raw), "mode": 384}),
    )
    .expect("🔴 b16 的相对段改不了权限");
    // 改名：from 走 b16，to 是正常名字 —— 「改成一个读得出的名字」正是乱码名最常要的那一下。
    answer_wire(
        "files-rename",
        &serde_json::json!({"root": r, "from": b16(raw), "to": "cafe.txt"}),
    )
    .expect("🔴 b16 的相对段改不了名");
    assert_eq!(
        std::fs::read(root.join("cafe.txt")).expect("读改名后"),
        b"latin"
    );
    assert!(
        std::fs::symlink_metadata(root.join(name)).is_err(),
        "旧名还在"
    );
    // 改回去，再删。
    answer_wire(
        "files-rename",
        &serde_json::json!({"root": r, "from": "cafe.txt", "to": b16(raw)}),
    )
    .expect("改回乱码名被误拒");
    let v = answer_wire(
        "files-delete",
        &serde_json::json!({"root": r, "rel": b16(raw)}),
    )
    .expect("🔴 b16 的相对段删不掉");
    assert_eq!(v["removed"], 1);
    assert!(
        std::fs::symlink_metadata(root.join(name)).is_err(),
        "说删了，盘上还在"
    );

    // 阴性：b16 解出来的段照样逐段过路径解析。
    let (code, msg) = answer_wire(
        "files-delete",
        &serde_json::json!({"root": r, "rel": b16(b"../escape")}),
    )
    .expect_err("🔴 b16 那条路绕过了路径解析（上跳段）");
    assert_eq!(code, "refused", "上跳段：{msg}");
    // 〔FN1 · V119〕会话文件形状经 b16 那条路也删得掉。
    assert!(!bytes.is_empty());
    answer_wire(
        "files-delete",
        &serde_json::json!({"root": r, "rel": b16(b"projects/-x/abc.jsonl")}),
    )
    .expect("🔴 V119：b16 的会话文件删不掉");
    assert!(!live.exists());
    // 形状不对 ⇒ bad_args（不猜）。
    let (code, _) = answer_wire(
        "files-delete",
        &serde_json::json!({"root": r, "rel": {"b16": "zz"}}),
    )
    .expect_err("坏的十六进制被收下了");
    assert_eq!(code, "bad_args");
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 递归删碰到树里的乱码名子项：照样逐条目过围栏、照样删得掉（计划趟的段是字节，不是字符串）。
#[test]
#[cfg(unix)]
fn a_recursive_delete_walks_through_non_utf8_children() {
    use std::os::unix::ffi::OsStrExt as _;
    let base = temp_root("rmtree-b16");
    let root = base.join("cfg");
    std::fs::create_dir_all(root.join("t")).expect("建");
    let odd = std::ffi::OsStr::from_bytes(b"d\xff");
    std::fs::create_dir_all(root.join("t").join(odd)).expect("铺乱码目录");
    std::fs::write(root.join("t").join(odd).join("f"), b"f").expect("铺");
    let (_, removed) = delete_tree(&root, "t").expect("🔴 树里有乱码名就删不动了");
    assert_eq!(removed, 3);
    assert!(std::fs::symlink_metadata(root.join("t")).is_err());
    std::fs::remove_dir_all(&base).ok();
}

// ══════════════════════════════════════════════════════════════════════════
//  〔RW1 · 第四波 · 2026-09-24〕用户文件的读改写（`files-peek` / `files-put`）＋ 删历史会话
// ══════════════════════════════════════════════════════════════════════════
//
// 买到的：全在本机临时目录上真跑 —— CAS 真的拒（盘上一个字节不动）· 相同不写 · 备份逐字节 ＋ 权限位 ·
// 跟链接改真文件、链接本身还在 · 父目录只在要了时才补 · 会话文件照旧写不进去 · 暂存旁名不留 ·
// 删会话只认 sid、找到的那一份不是会话形状就拒。
// 买不到的：① 回读不符 ⇒ 回滚那一支（要在换名与回读之间插一次外部改动，本族没有注入口，一格没量）；
// ② 真远端；③ CAS 与换名之间那一个窗（TOCTOU）。

/// 目录里有没有替换留下的暂存旁名（`.<名>.ccm-put-<pid>-<序号>.part`）。
///
/// ⚠ **不列目录**（同 [`copy_leftovers`]：`scanning_guard_registry` 不许测试段裸遍历目录）：
/// 旁名形状确定、本进程造过的序号是 `0..PUT_SEQ` ⇒ 逐个按名字问「在不在」。
fn side_files(dir: &Path, name: &str) -> Vec<String> {
    let upto = PUT_SEQ.load(std::sync::atomic::Ordering::Relaxed);
    (0..upto)
        .map(|seq| format!(".{name}.ccm-put-{}-{seq}.part", std::process::id()))
        .filter(|n| std::fs::symlink_metadata(dir.join(n)).is_ok())
        .collect()
}

#[test]
fn peek_tells_absent_from_present_and_goes_through_the_same_resolution() {
    let base = temp_root("pk");
    let root = base.join("r");
    std::fs::create_dir_all(&root).expect("建根");
    std::fs::write(root.join("a.txt"), "hello\n").expect("铺 a");
    let got = peek_text(&root, "a.txt").expect("读一份在的文件");
    assert_eq!(got.text.as_deref(), Some("hello\n"));
    let none = peek_text(&root, "nope.txt").expect("不在不是错");
    assert_eq!(none.text, None, "不在的文件该答 `None`，不是空串");
    for (rel, want) in [("../x", "refused"), ("/etc/passwd", "refused")] {
        assert_eq!(peek_text(&root, rel).unwrap_err().0, want, "{rel}");
    }
    std::fs::create_dir_all(root.join("cfg/projects/-p")).expect("建会话目录");
    std::fs::write(root.join("cfg/projects/-p/s1.jsonl"), "{}\n").expect("铺会话");
    // 〔FN1 · V119〕从前「读改写的读那一半与写同一道围栏 —— 会话文件读不进来」；今天读得进来。
    assert_eq!(
        peek_text(&root, "cfg/projects/-p/s1.jsonl")
            .expect("🔴 V119：会话文件读不进读改写")
            .text
            .as_deref(),
        Some("{}\n")
    );
    std::fs::write(root.join("bin.dat"), [0xffu8, 0xfe, 0x00]).expect("铺二进制");
    assert_eq!(peek_text(&root, "bin.dat").unwrap_err().0, "not_text");
    std::fs::write(root.join("big.txt"), vec![b'x'; PEEK_MAX_BYTES + 1]).expect("铺大文件");
    assert_eq!(peek_text(&root, "big.txt").unwrap_err().0, "too_large");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn put_is_compare_and_swap_and_a_stale_expectation_writes_nothing() {
    let base = temp_root("pcas");
    let root = base.join("r");
    std::fs::create_dir_all(&root).expect("建根");
    std::fs::write(root.join("a.txt"), "A").expect("铺 a");
    for expect in [Some(&b"X"[..]), None] {
        match put_text(&root, "a.txt", b"B", expect, false, false) {
            Err(WriteRefusal::Stale(_)) => {}
            other => panic!("🔴 期望对不上却没回 stale：{other:?}"),
        }
        assert_eq!(
            std::fs::read(root.join("a.txt")).expect("a"),
            b"A",
            "stale 那一趟动了盘"
        );
    }
    match put_text(&root, "new.txt", b"N", Some(b"old"), false, false) {
        Err(WriteRefusal::Stale(_)) => {}
        other => panic!("🔴 「读的时候在、现在不在」没回 stale：{other:?}"),
    }
    assert!(!root.join("new.txt").exists(), "stale 那一趟建出了文件");
    let done = put_text(&root, "a.txt", b"B", Some(b"A"), false, false).expect("对上了就该写");
    assert!(done.changed && !done.created && done.backup.is_none());
    assert_eq!(std::fs::read(root.join("a.txt")).expect("a"), b"B");
    let same = put_text(&root, "a.txt", b"B", Some(b"B"), true, false).expect("相同");
    assert!(!same.changed, "内容相同该一个字节不写");
    assert!(same.backup.is_none(), "没写就不该留备份");
    let made = put_text(&root, "new.txt", b"N", None, false, false).expect("不在 ⇒ 建");
    assert!(made.changed && made.created);
    assert_eq!(std::fs::read(root.join("new.txt")).expect("new"), b"N");
    for n in ["a.txt", "new.txt"] {
        assert!(
            side_files(&root, n).is_empty(),
            "暂存旁名留下来了：{:?}",
            side_files(&root, n)
        );
    }
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn put_builds_parents_only_when_asked_and_each_level_is_fenced() {
    let base = temp_root("ppar");
    let root = base.join("r");
    std::fs::create_dir_all(&root).expect("建根");
    match put_text(&root, "d/e/f.txt", b"x", None, false, false) {
        Err(WriteRefusal::Refused(_)) => {}
        other => panic!("🔴 没要 `parents` 却补了父目录 / 没拒：{other:?}"),
    }
    assert!(!root.join("d").exists(), "没要 `parents` 也建了目录");
    put_text(&root, "d/e/f.txt", b"x", None, false, true).expect("要了就补");
    assert_eq!(std::fs::read(root.join("d/e/f.txt")).expect("f"), b"x");
    assert!(put_text(&root, "../out/f.txt", b"x", None, false, true).is_err());
    assert!(!base.join("out").exists(), "补父目录那一步逃出了目标根");
    std::fs::remove_dir_all(&base).ok();
}

#[cfg(unix)]
#[test]
fn put_backup_keeps_the_original_bytes_and_the_mode_survives_the_swap() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = temp_root("pbak");
    let root = base.join("r");
    std::fs::create_dir_all(&root).expect("建根");
    let rc = root.join(".bashrc");
    std::fs::write(&rc, "# mine\n").expect("铺 rc");
    std::fs::set_permissions(&rc, std::fs::Permissions::from_mode(0o600)).expect("600");
    let done = put_text(
        &root,
        ".bashrc",
        b"# mine\nnew\n",
        Some(b"# mine\n"),
        true,
        false,
    )
    .expect("写");
    let bak = done.backup.expect("要了备份却没有");
    assert_eq!(
        std::fs::read(&bak).expect("备份"),
        b"# mine\n",
        "备份不是原文"
    );
    let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    assert_eq!(mode(&bak), 0o600, "备份没沿用原权限位");
    assert_eq!(mode(&rc), 0o600, "换名上位之后原权限位丢了");
    assert_eq!(std::fs::read(&rc).expect("rc"), b"# mine\nnew\n");
    std::fs::remove_dir_all(&base).ok();
}

#[cfg(unix)]
#[test]
fn put_through_a_link_changes_the_real_file_and_the_link_stays_a_link() {
    let base = temp_root("plink");
    let root = base.join("r");
    std::fs::create_dir_all(root.join("dotfiles")).expect("建根");
    std::fs::write(root.join("dotfiles/bashrc"), "old\n").expect("铺真文件");
    std::os::unix::fs::symlink("dotfiles/bashrc", root.join(".bashrc")).expect("建链接");
    let got = peek_text(&root, ".bashrc").expect("经链接读");
    assert_eq!(got.text.as_deref(), Some("old\n"));
    put_text(&root, ".bashrc", b"new\n", Some(b"old\n"), false, false).expect("经链接写");
    assert!(
        std::fs::symlink_metadata(root.join(".bashrc"))
            .expect("meta")
            .file_type()
            .is_symlink(),
        "用户的链接被换成了一份普通文件"
    );
    assert_eq!(
        std::fs::read(root.join("dotfiles/bashrc")).expect("真文件"),
        b"new\n"
    );
    // 链接指到根外 ⇒ 解到底之后被拒，根外那一份一个字节不动。
    // 〔FN1 · V119〕从前这里的语料是「链接指到一份会话文件上」，今天那一形放行（见下一条）。
    let (outside, bytes) = plant_outside(&base);
    std::os::unix::fs::symlink(&outside, root.join("sneaky.txt")).expect("建链接");
    let got = put_text(&root, "sneaky.txt", b"x", Some(&bytes), false, false);
    assert!(matches!(got, Err(WriteRefusal::Refused(_))), "{got:?}");
    assert_eq!(std::fs::read(&outside).expect("根外"), bytes);
    std::fs::remove_dir_all(&base).ok();
}

/// 〔FN1 · V119 翻面〕从前叫「put 从不直接写会话文件」；今天读改写照样写得进会话文件（CAS 照旧）。
#[test]
fn put_writes_a_session_file_like_any_other_under_the_same_cas() {
    let base = temp_root("psess");
    let (live, bytes) = plant_live_session(&base);
    let stale = put_text(
        &base,
        "cfg/projects/-x/abc.jsonl",
        b"x",
        Some(b"not it"),
        false,
        false,
    );
    assert!(matches!(stale, Err(WriteRefusal::Stale(_))), "{stale:?}");
    assert_eq!(
        std::fs::read(&live).expect("会话"),
        bytes,
        "stale 那一趟动了盘"
    );
    let done = put_text(
        &base,
        "cfg/projects/-x/abc.jsonl",
        b"x",
        Some(&bytes),
        false,
        false,
    )
    .expect("🔴 V119：读改写写不进会话文件");
    assert!(done.changed);
    assert_eq!(std::fs::read(&live).expect("会话"), b"x");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn the_read_modify_write_commands_answer_with_their_declared_fields() {
    let base = temp_root("prmw");
    let root = base.join("r");
    std::fs::create_dir_all(&root).expect("建根");
    let r = root.to_str().expect("utf8");
    match answer_wire(
        "files-put",
        &serde_json::json!({"root": r, "rel": "a", "content": "x"}),
    ) {
        Err((c, _)) => assert_eq!(c, "bad_args", "缺 `expect` 该回 bad_args"),
        Ok(v) => panic!("🔴 没给 `expect` 竟然写了：{v}"),
    }
    assert!(!root.join("a").exists(), "没给 `expect` 的那一趟落了盘");
    match answer_wire(
        "files-put",
        &serde_json::json!({"root": r, "rel": "a", "content": "x", "expect": null, "backup": 1}),
    ) {
        Err((c, _)) => assert_eq!(c, "bad_args"),
        Ok(v) => panic!("🔴 `backup: 1` 竟然被当成了布尔：{v}"),
    }
    let keys = |v: &serde_json::Value| -> std::collections::BTreeSet<String> {
        v.as_object().expect("对象").keys().cloned().collect()
    };
    let declared = |name: &str| -> std::collections::BTreeSet<String> {
        MANAGE_COMMANDS
            .iter()
            .find(|c| c.name == name)
            .expect("在表里")
            .fields
            .iter()
            .map(|s| s.to_string())
            .collect()
    };
    let peek0 = answer_wire("files-peek", &serde_json::json!({"root": r, "rel": "a"}))
        .expect("peek 不在的文件");
    assert_eq!(peek0["exists"], false);
    assert_eq!(keys(&peek0), declared("files-peek"));
    let put = answer_wire(
        "files-put",
        &serde_json::json!({"root": r, "rel": "a", "content": "x", "expect": null}),
    )
    .expect("put");
    assert_eq!(keys(&put), declared("files-put"));
    assert_eq!(put["created"], true);
    let peek1 =
        answer_wire("files-peek", &serde_json::json!({"root": r, "rel": "a"})).expect("peek");
    assert_eq!(peek1["text"], "x");
    match answer_wire(
        "files-put",
        &serde_json::json!({"root": r, "rel": "a", "content": "y", "expect": "not-x"}),
    ) {
        Err((c, _)) => assert_eq!(c, "stale"),
        Ok(v) => panic!("🔴 期望对不上竟然写了：{v}"),
    }
    std::fs::remove_dir_all(&base).ok();
}

// ── 删历史会话：只收 sid（〔FN1〕原标题「会话文件围栏唯一的例外」，那道围栏 V119 拿掉了） ────────────────────────────────────────

/// 一个假的配置根：`projects/-p/<sid>.jsonl` ＋ 一份子代理那种更深的同名文件。
fn plant_home(base: &Path, sid: &str) -> PathBuf {
    let home = base.join("cfg");
    std::fs::create_dir_all(home.join("projects/-p")).expect("建项目目录");
    std::fs::write(home.join(format!("projects/-p/{sid}.jsonl")), "{}\n").expect("铺会话");
    home
}

#[test]
fn deleting_a_session_takes_only_a_sid_and_removes_exactly_that_file() {
    let base = temp_root("dsess");
    let sid = "2f1c9a4e-0b7d-4c1e-9a55-3b2f0c8d1e77";
    let home = plant_home(&base, sid);
    std::fs::write(home.join("projects/-p/other.jsonl"), "{}\n").expect("铺邻居");
    let gone = delete_session_with(sid, |s| {
        crate::agents::claudecode::paths::session_file_for_delete_in(&home, s)
    })
    .expect("删一份真在的会话");
    assert!(!gone.exists(), "说删了，文件还在");
    assert!(
        home.join("projects/-p/other.jsonl").exists(),
        "邻居被连带删了"
    );
    // 再删一次 ⇒ 找不到 ⇒ 拒（不是静默成功）。
    let again = delete_session_with(sid, |s| {
        crate::agents::claudecode::paths::session_file_for_delete_in(&home, s)
    });
    assert!(matches!(again, Err(WriteRefusal::Refused(_))), "{again:?}");
    for bad in ["../x", "a/b", "", "x.jsonl"] {
        let got = delete_session_with(bad, |s| {
            crate::agents::claudecode::paths::session_file_for_delete_in(&home, s)
        });
        assert!(
            matches!(got, Err(WriteRefusal::Refused(_))),
            "`{bad}` 竟然过了：{got:?}"
        );
    }
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn the_session_fence_holds_even_if_the_locator_is_swapped() {
    // 🔴 「例外」的定义是**它删的恰恰是会话、别的删不到**。把找文件那一步换成一个指向
    //    普通文件的定位器（= 将来有人改坏了适配层），删那一步照样得拒。
    let base = temp_root("dfence");
    std::fs::create_dir_all(base.join("docs")).expect("建目录");
    let notes = base.join("docs/notes.txt");
    std::fs::write(&notes, "keep me").expect("铺普通文件");
    let got = delete_session_with("s1", |_| Ok(notes.clone()));
    assert!(matches!(got, Err(WriteRefusal::Refused(_))), "{got:?}");
    assert_eq!(std::fs::read(&notes).expect("notes"), b"keep me");
    // 名字对得上、但不在 `projects/<proj>/` 底下（不是会话的形状）⇒ 只有「形状」那一问拦得住。
    let named = base.join("docs/s1.jsonl");
    std::fs::write(&named, "not a session").expect("铺同名普通文件");
    let got = delete_session_with("s1", |_| Ok(named.clone()));
    assert!(matches!(got, Err(WriteRefusal::Refused(_))), "{got:?}");
    assert!(named.exists(), "一份同名的普通文件被当成会话删了");
    // 名字对不上 sid（别的会话）同样拒。
    let home = plant_home(&base, "s2");
    let other = home.join("projects/-p/s2.jsonl");
    let got = delete_session_with("s1", |_| Ok(other.clone()));
    assert!(matches!(got, Err(WriteRefusal::Refused(_))), "{got:?}");
    assert!(other.exists(), "拿 s1 的名义删掉了 s2");
    std::fs::remove_dir_all(&base).ok();
}

#[cfg(unix)]
#[test]
fn a_session_that_is_a_link_out_of_the_record_tree_is_not_followed() {
    let base = temp_root("dlink");
    let home = base.join("cfg");
    std::fs::create_dir_all(home.join("projects/-p")).expect("建项目目录");
    std::fs::create_dir_all(base.join("elsewhere/projects/-q")).expect("建外面");
    let outside = base.join("elsewhere/projects/-q/s3.jsonl");
    std::fs::write(&outside, "{}\n").expect("铺外面那份");
    std::os::unix::fs::symlink(&outside, home.join("projects/-p/s3.jsonl")).expect("建链接");
    let got = delete_session_with("s3", |s| {
        crate::agents::claudecode::paths::session_file_for_delete_in(&home, s)
    });
    assert!(matches!(got, Err(WriteRefusal::Refused(_))), "{got:?}");
    assert!(outside.exists(), "跟着链接删到了记录树外面那一份");
    std::fs::remove_dir_all(&base).ok();
}

/// 〔RW1 · 第四波 09-24〕从 `profile_installer_tests.rs` 里那条 `install_preserves_explicit_acl_entries`〔散文墓碑〕搬来：
/// v1.7.9 那次事故（原 profile 上的 explicit ACE 被暂存文件的继承 ACL 顶掉，用户读不了自己的 `$PROFILE`）。
/// 写从 monitor 搬到后端之后，替换那一步在 Windows 上走**就地覆盖写**（`swap_in` 的 `cfg(windows)` 那一支）。
/// ⚠ 本机门禁是 Linux，这一条只在 Windows 上跑（`winchk-backend` 只编不跑）—— 如实登记为「没跑过」。
#[cfg(windows)]
#[test]
fn put_keeps_explicit_acl_entries_on_windows() {
    let base = temp_root("pacl");
    let root = base.join("r");
    std::fs::create_dir_all(&root).expect("建根");
    let p = root.join("profile.ps1");
    std::fs::write(&p, "Set-Alias g git\n").expect("铺");
    let add = std::process::Command::new("icacls")
        .arg(&p)
        .arg("/grant")
        .arg("Everyone:(R)")
        .output();
    let Ok(add) = add else { return };
    if !add.status.success() {
        return;
    }
    put_text(
        &root,
        "profile.ps1",
        b"Set-Alias g git\nnew\n",
        Some(b"Set-Alias g git\n"),
        true,
        false,
    )
    .expect("写");
    let out = std::process::Command::new("icacls")
        .arg(&p)
        .output()
        .expect("icacls");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout
            .lines()
            .any(|l| l.contains("Everyone:(R)") && !l.contains("(I)(R)")),
        "explicit Everyone:(R) ACE 被替换那一步顶掉了：\n{stdout}"
    );
    std::fs::remove_dir_all(&base).ok();
}

// ── 〔RM1e · 第四波〕带 CAS 的删（`files-delete` 的 `expect`）────────────────────────
//
// 要求住址：用户 09-24 **V110**（`设计/99 §1`）「引擎只算、文件管理来写」——全景删批注侧车是一次读改写的写那一半；
// `调研/第四波记录/RM1d.md §6 ⑤`「删批注没有 CAS …… 要闭合得给 `files-delete` 加 `expect`」。

/// ★ 盘上 == `expect` ⇒ 删；≠ ⇒ `stale`、一个字节不动；已经不在 ⇒ `stale`；
/// 目录 / 链接 ⇒ `refused`、原样；会话文件 ⇒ 围栏照旧 `refused`。
#[test]
#[cfg(unix)]
fn a_delete_with_expect_removes_only_the_bytes_it_was_told_about() {
    let base = temp_root("rm-cas");
    let root = base.join("cfg");
    let (live, bytes) = plant_live_session(&base);
    std::fs::write(root.join("a.json"), b"planned").expect("铺");
    std::fs::create_dir_all(root.join("d")).expect("铺目录");
    std::fs::write(root.join("t.json"), b"planned").expect("铺链接目标");
    std::os::unix::fs::symlink(root.join("t.json"), root.join("l.json")).expect("铺链接");

    // 不等 ⇒ stale，盘上逐字节不变（长度不同 · **长度相同、字节不同** 两形 —— 后一形是死值验 K7 首刀没砍中补的格）。
    for other in [&b"someone else"[..], &b"PLANNED"[..]] {
        let err = delete_file_expecting(&root, "a.json", other).expect_err("🔴 不等也删了");
        assert_eq!(err.code(), "stale", "{err:?}");
    }
    assert_eq!(std::fs::read(root.join("a.json")).expect("读"), b"planned");
    // 正控：等 ⇒ 删。
    delete_file_expecting(&root, "a.json", b"planned").expect("逐字节相等却被拒");
    assert!(!root.join("a.json").exists(), "说删了，盘上还在");
    // 已经不在 ⇒ stale（读的时候还在）。
    let err = delete_file_expecting(&root, "a.json", b"planned").expect_err("不在也回成功");
    assert_eq!(err.code(), "stale", "{err:?}");
    // 目录 / 链接 ⇒ refused，原样（链接指向的那份字节等于 expect 也不删）。
    for rel in ["d", "l.json"] {
        let err = delete_file_expecting(&root, rel, b"planned").expect_err("非普通文件被删了");
        assert_eq!(err.code(), "refused", "{rel}: {err:?}");
    }
    assert!(root.join("d").is_dir());
    assert!(
        std::fs::symlink_metadata(root.join("l.json")).is_ok_and(|m| m.file_type().is_symlink())
    );
    assert_eq!(std::fs::read(root.join("t.json")).expect("读"), b"planned");
    // 〔FN1 · V119〕会话文件：expect 对得上 ⇒ 删（从前「对得上也拒，围栏在 CAS 之前」）；对不上 ⇒ stale。
    let err = delete_file_expecting(&root, "projects/-x/abc.jsonl", b"not it")
        .expect_err("🔴 不等也删了");
    assert_eq!(err.code(), "stale", "{err:?}");
    assert_eq!(std::fs::read(&live).expect("读会话"), bytes);
    delete_file_expecting(&root, "projects/-x/abc.jsonl", &bytes)
        .expect("🔴 V119：会话文件对得上也删不掉");
    assert!(!live.exists());
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 命令面：`expect` 各形的码（字符串 / b16 · `null` · 与 `recursive` 同给 · 不等）；不给 ⇒ 行为不变。
#[test]
fn the_command_face_takes_expect_only_as_bytes_of_one_file() {
    let base = temp_root("rm-cas-cmd");
    let root = base.join("cfg");
    std::fs::create_dir_all(&root).expect("建根");
    let r = root.to_str().expect("utf8");
    std::fs::write(root.join("x.md"), b"xy").expect("铺");
    for (bad, want) in [
        (
            serde_json::json!({"root": r, "rel": "x.md", "expect": null}),
            "bad_args",
        ),
        (
            serde_json::json!({"root": r, "rel": "x.md", "expect": 1}),
            "bad_args",
        ),
        (
            serde_json::json!({"root": r, "rel": "x.md", "expect": "xy", "recursive": true}),
            "bad_args",
        ),
        (
            serde_json::json!({"root": r, "rel": "x.md", "expect": "x"}),
            "stale",
        ),
    ] {
        let (code, _) = answer_wire("files-delete", &bad).expect_err("该拒的收下了");
        assert_eq!(code, want, "{bad}");
    }
    assert_eq!(
        std::fs::read(root.join("x.md")).expect("读"),
        b"xy",
        "被拒的几次动了盘"
    );
    // b16 形：「xy」= 7879。
    let v = answer_wire(
        "files-delete",
        &serde_json::json!({"root": r, "rel": "x.md", "expect": {"b16": "7879"}}),
    )
    .expect("逐字节相等却被拒");
    assert_eq!(v["removed"], 1, "{v}");
    assert!(!root.join("x.md").exists());
    // 不给 expect ⇒ 旧行为（不看内容就删）。
    std::fs::write(root.join("y.md"), b"whatever").expect("铺");
    answer_wire(
        "files-delete",
        &serde_json::json!({"root": r, "rel": "y.md"}),
    )
    .expect("旧形被拒");
    assert!(!root.join("y.md").exists());
    std::fs::remove_dir_all(&base).ok();
}

/// 〔W5-VIS · E 吞错普查点名 `files_write` 那一处 `.ok();`〕**备份沿用不上原文件的权限位 ⇒ 删掉那份备份、整趟拒**（两向）：
/// 注入一个会失败的 chmod ⇒ `Io` 拒绝、话里带原因、备份那份不在了；成功的 chmod ⇒ `Ok`、备份还在。
/// 接线：`land_backup` 经 `keep_mode`，生产段零处 `set_permissions(…).ok()`。
///
/// 要求住址：`设计/15 §4.7 S5`（逐字）「处置不是别吞，是吞了要留一行日志」—— 这一处连日志都不够：一份权限放宽了的备份要当场收回。
#[cfg(unix)]
#[test]
fn w5vis_a_backup_that_cannot_keep_the_original_mode_is_removed_and_refused() {
    let base = std::env::temp_dir().join(format!("ccm-w5vis-keepmode-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let bak = base.join("x.ccm-backup-1");
    std::fs::write(&bak, b"secret").unwrap();
    use std::os::unix::fs::PermissionsExt;
    let e = keep_mode(
        &bak,
        std::fs::Permissions::from_mode(0o600),
        |_, _| Err(std::io::Error::other("w5vis 注入的 chmod 失败")),
        |b| std::fs::remove_file(b),
    )
    .expect_err("chmod 失败却放行了");
    match e {
        WriteRefusal::Io(m) => {
            assert!(
                m.contains("w5vis 注入的 chmod 失败") && m.contains("原文件没动"),
                "{m}"
            )
        }
        other => panic!("不是 Io 拒绝：{other:?}"),
    }
    assert!(!bak.exists(), "沿用不上权限位的那份备份还留在盘上");
    // 另一向：chmod 成功 ⇒ 放行、备份还在、权限位就是交进来的那个。
    std::fs::write(&bak, b"secret").unwrap();
    keep_mode(
        &bak,
        std::fs::Permissions::from_mode(0o600),
        |b, p| std::fs::set_permissions(b, p),
        |b| std::fs::remove_file(b),
    )
    .expect("chmod 成功却拒了");
    assert_eq!(
        std::fs::metadata(&bak).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // 接线。
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/files_write.rs"
    ));
    assert_eq!(
        prod.matches("set_permissions(&bak, p).ok()").count(),
        0,
        "那一处 `.ok()` 又回来了"
    );
    assert_eq!(
        prod.matches("keep_mode(").count(),
        2,
        "`keep_mode` 该恰好两处（定义 ＋ `land_backup` 里那一次）"
    );
    let at = prod.find("fn land_backup(").expect("land_backup 不在了");
    let body = &prod[at..at + prod[at..].find("\n}\n").expect("切不出 land_backup")];
    assert!(
        body.contains("keep_mode("),
        "`land_backup` 没经 `keep_mode`"
    );
    std::fs::remove_dir_all(&base).ok();
}

// ════════════════════════════════════════════════════════════════════════════
//  〔W5-FILES · 第五波〕复制目录（`files-copy` 的 `recursive: true`）
//  要求住址：`设计/60 §7 #6`「递归复制 · 复制目录 …… 递归复制要照递归删的形状逐条目过围栏」
//  ＋ 用户 V45「我能连 ssh 对机器文件进行什么操作，后端就应该能进行什么操作」。
//  设计全文 `调研/第四波记录/W5-FILES.md` §2.2。
// ════════════════════════════════════════════════════════════════════════════

/// 给判据用：一棵树逐条 `(相对段, 内容或 None=目录, 权限位)`，**按名字**问、不列目录
/// （`scanning_guard_registry` 不许测试段裸遍历目录）。
#[cfg(unix)]
fn tree_facts(base: &Path, names: &[&str]) -> Vec<(String, Option<Vec<u8>>, u32)> {
    use std::os::unix::fs::PermissionsExt as _;
    names
        .iter()
        .map(|n| {
            let p = base.join(n);
            let md = std::fs::symlink_metadata(&p).unwrap_or_else(|e| panic!("{n} 不在：{e}"));
            let body = if md.is_dir() {
                None
            } else {
                Some(std::fs::read(&p).expect("读"))
            };
            (n.to_string(), body, md.permissions().mode() & 0o7777)
        })
        .collect()
}

/// 相对 `t` 的那几条（`plant_tree` 的表去掉前缀 `t`）。
fn tails_of_tree() -> Vec<&'static str> {
    vec!["a", "a/b", "a/b/three.md", "a/two.md", "empty", "one.md"]
}

/// ★ 整棵复制：每一条逐字节 ＋ 权限位与源相等（目录权限位也抄，含一个只读目录）；回报的条数 / 字节数与手算相等。
#[test]
#[cfg(unix)]
fn a_recursive_copy_lands_the_whole_tree_with_bytes_and_modes() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = temp_root("cpt");
    let root = base.join("cfg");
    plant_tree(&root);
    std::fs::set_permissions(
        root.join("t/a/two.md"),
        std::fs::Permissions::from_mode(0o751),
    )
    .expect("设权限");
    std::fs::set_permissions(root.join("t/a/b"), std::fs::Permissions::from_mode(0o555))
        .expect("设只读目录");
    let got = copy_tree(&root, "t", "u").expect("干净的复制目录被拒了");
    assert_eq!(
        (got.files, got.dirs, got.bytes),
        (3, 4, 3),
        "条数 / 字节数与手算不等"
    );
    let mut tails = tails_of_tree();
    tails.insert(0, "");
    let names: Vec<String> = tails.iter().map(|t| t.to_string()).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let src = tree_facts(&root.join("t"), &refs);
    let dst = tree_facts(&root.join("u"), &refs);
    assert_eq!(dst, src, "复制出来的那棵与源不逐条相等（内容 / 权限位）");
    std::fs::set_permissions(root.join("t/a/b"), std::fs::Permissions::from_mode(0o755)).ok();
    std::fs::set_permissions(root.join("u/a/b"), std::fs::Permissions::from_mode(0o755)).ok();
    std::fs::remove_dir_all(&base).ok();
}

/// 整趟拒的几形：树里一个管道（设备 / 套接字同档）· 目标已在 · 目标在源里面 · 超上限 —— 每一形**一个字节都没建**、已在的一个字节没动。
/// 〔FILES2 · Q1〕第 ① 形原来是「树里一条链接」—— 主会话 09-27 裁「复制链接本身」之后链接照原样复制
/// （判据挪到 `a_recursive_copy_copies_each_link_itself_with_its_target_text_verbatim`），这里换成仍整趟拒的管道。
#[test]
#[cfg(unix)]
fn a_recursive_copy_refuses_whole_and_builds_nothing() {
    let base = temp_root("cptr");
    let root = base.join("cfg");
    plant_tree(&root);
    // ① 管道（不是目录 / 普通文件 / 链接 ⇒ 复制目录不建它）。
    let fifo = std::ffi::CString::new(root.join("t/a/pipe").to_string_lossy().as_bytes().to_vec())
        .expect("路径");
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0, "铺管道");
    let e = copy_tree(&root, "t", "u").expect_err("树里有管道却复制成了");
    assert_eq!(e.code(), "refused", "{e:?}");
    assert!(
        std::fs::symlink_metadata(root.join("u")).is_err(),
        "被拒的那一趟建了东西"
    );
    std::fs::remove_file(root.join("t/a/pipe")).expect("撤管道");
    // ② 目标已在：建目录那一步就失败，已在那一份一个字节不动。
    std::fs::create_dir(root.join("u")).expect("铺已在的目标");
    std::fs::write(root.join("u/keep"), b"K").expect("铺");
    let e = copy_tree(&root, "t", "u").expect_err("目标已在却合并进去了");
    assert_eq!(e.code(), "io_failed", "{e:?}");
    assert_eq!(std::fs::read(root.join("u/keep")).expect("keep"), b"K");
    assert!(
        std::fs::symlink_metadata(root.join("u/one.md")).is_err(),
        "合并进去了"
    );
    // ③ 目标在源里面。
    let e = copy_tree(&root, "t", "t/a/inner").expect_err("复制进自己里面竟然成了");
    assert_eq!(e.code(), "refused", "{e:?}");
    assert!(std::fs::symlink_metadata(root.join("t/a/inner")).is_err());
    // ④ 超上限（计划 7 条，上限 3）。
    let e = copy_tree_with(&root, Path::new("t"), Path::new("v"), 3).expect_err("超上限却复制了");
    assert_eq!(e.code(), "refused", "{e:?}");
    assert!(
        std::fs::symlink_metadata(root.join("v")).is_err(),
        "超上限那一趟建了东西"
    );
    // 正控：同一棵、上限够 ⇒ 成。
    copy_tree_with(&root, Path::new("t"), Path::new("v"), 7).expect("上限恰好够却被拒了");
    std::fs::remove_dir_all(&base).ok();
}

/// ★〔FILES2 · Q1〕要求住址：`设计/60 §6.2`「目录复制遇符号链接」· `§7` 第 9 条 Q1；主会话 09-27 裁
/// 「复制**链接本身**（不跟进去，目标文本原样；= GNU `cp -R` 缺省的 `-P`）」。
/// 三种链接（指向树里 · 指向根外的绝对路径 · 悬空）各复制成**一条链接**、目标文本逐字节相等；根外那一份一个字节没被碰；
/// 应答的 `links` == 3、`files` 不含链接。
#[test]
#[cfg(unix)]
fn a_recursive_copy_copies_each_link_itself_with_its_target_text_verbatim() {
    let base = temp_root("cptl");
    let root = base.join("cfg");
    plant_tree(&root);
    std::fs::write(base.join("outside.md"), b"O").expect("铺根外");
    let links = [
        ("t/a/in", std::path::PathBuf::from("../one.md")),
        ("t/out", base.join("outside.md")),
        ("t/a/b/dangling", std::path::PathBuf::from("no/such/thing")),
    ];
    for (at, to) in &links {
        std::os::unix::fs::symlink(to, root.join(at)).expect("铺链接");
    }
    let got = copy_tree(&root, "t", "u").expect("树里有链接的复制目录被拒了");
    assert_eq!(
        (got.files, got.dirs, got.links),
        (3, 4, 3),
        "条数与手算不等（链接不许算进 files）"
    );
    for (at, to) in &links {
        let dst = root.join(at.replacen("t/", "u/", 1));
        let md = std::fs::symlink_metadata(&dst).expect("复制出来的链接不在");
        assert!(
            md.file_type().is_symlink(),
            "{at} 没复制成链接（跟进去了？）"
        );
        assert_eq!(
            &std::fs::read_link(&dst).expect("读链接"),
            to,
            "{at} 的目标文本变了"
        );
    }
    assert_eq!(std::fs::read(base.join("outside.md")).expect("根外"), b"O");
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 执行趟中途失败 ⇒ **自己建的全撤掉**、源一个字节没动。注入：一份源文件读不了（计划只看名字与种类，读到它才失败）。
#[test]
#[cfg(unix)]
fn a_recursive_copy_that_fails_midway_undoes_what_it_built() {
    use std::os::unix::fs::PermissionsExt as _;
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("root 读得了 000 的文件 ⇒ 注入不成立，跳过");
        return;
    }
    let base = temp_root("cptu");
    let root = base.join("cfg");
    plant_tree(&root);
    std::fs::set_permissions(
        root.join("t/a/b/three.md"),
        std::fs::Permissions::from_mode(0o000),
    )
    .expect("设不可读");
    let e = copy_tree(&root, "t", "u").expect_err("读不了的源却复制成了");
    assert_eq!(e.code(), "io_failed", "{e:?}");
    assert!(
        e.message().contains("都撤掉了"),
        "没说回滚：{}",
        e.message()
    );
    assert!(
        std::fs::symlink_metadata(root.join("u")).is_err(),
        "🔴 半截复制留在了盘上"
    );
    std::fs::set_permissions(
        root.join("t/a/b/three.md"),
        std::fs::Permissions::from_mode(0o644),
    )
    .expect("复原");
    assert_eq!(
        std::fs::read(root.join("t/a/b/three.md")).expect("源"),
        b"3",
        "源被动了"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 命令面：不带 `recursive` 目录照旧 `refused`；与 `overwrite: true` 同给 ⇒ `bad_args`；带了 ⇒ 回的键 == 声明的 `fields`。
#[test]
fn the_command_face_copies_a_directory_only_when_asked() {
    let base = temp_root("cptc");
    let root = base.join("cfg");
    plant_tree(&root);
    let r = root.to_string_lossy().to_string();
    let e = answer_wire(
        "files-copy",
        &serde_json::json!({"root": r, "from": "t", "to": "u"}),
    )
    .expect_err("不带 recursive 复制了目录");
    assert_eq!(e.0, "refused", "{e:?}");
    let e = answer_wire(
        "files-copy",
        &serde_json::json!({"root": r, "from": "t", "to": "u", "recursive": true, "overwrite": true}),
    )
    .expect_err("recursive ＋ overwrite 竟然收了");
    assert_eq!(e.0, "bad_args", "{e:?}");
    assert!(std::fs::symlink_metadata(root.join("u")).is_err());
    let v = answer_wire(
        "files-copy",
        &serde_json::json!({"root": r, "from": "t", "to": "u", "recursive": true}),
    )
    .expect("带 recursive 的复制目录被拒");
    let got: std::collections::BTreeSet<&str> = v
        .as_object()
        .expect("对象")
        .keys()
        .map(String::as_str)
        .collect();
    let declared: std::collections::BTreeSet<&str> = MANAGE_COMMANDS
        .iter()
        .find(|c| c.name == "files-copy")
        .expect("在表里")
        .fields
        .iter()
        .copied()
        .collect();
    assert_eq!(got, declared);
    assert_eq!(
        (v["files"].as_u64(), v["dirs"].as_u64()),
        (Some(3), Some(4))
    );
    assert_eq!(
        std::fs::read(root.join("u/a/b/three.md")).expect("读"),
        b"3"
    );
    std::fs::remove_dir_all(&base).ok();
}

// ═══════════════════════════════════════════════════════════════════════════════════════
//  〔FW1 · 第四波 4D · 2026-09-25〕编辑器存盘的 CAS（摘要形 `expect: {"sha256": …}`）
// ═══════════════════════════════════════════════════════════════════════════════════════
//
// 要求住址：主会话裁 D-c「编辑器存盘带 CAS（`expect`）」（题面 `4d-lanes.md`「主会话本批裁的」）＋ 09-25 认可摘要形；
// `设计/60 §3.3`「读的那一刻与写的那一刻之间，盘上那份被别人改了 ⇒ `stale`，一个字节不写」（原写给 `files-put`，同一条理由）。

/// 摘要的算法住后端一处；判据拿**另一份实现**（`sha2`，只在测试期链接）对拍 —— 异源，两侧同源恒真那一形排除。
fn sha2_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn the_cas_digest_is_plain_sha256_as_another_implementation_computes_it() {
    for body in [
        b"".as_slice(),
        b"a",
        "中文\n行尾\r\n".as_bytes(),
        &[0u8; 100_000],
    ] {
        assert_eq!(content_sha256(body), sha2_hex(body));
    }
    assert_eq!(content_sha256(b"x").len(), SHA256_HEX_LEN);
}

/// ★ 读 → 存 → 再存：`files-read-text` 交的摘要就是盘上那份的；拿它存成；应答交的新摘要 == 新内容的，拿它再存也成（连存两次不自撞）。
#[test]
fn read_then_save_then_save_again_chains_on_the_digests_the_backend_hands_out() {
    let root = temp_root("cas-chain");
    std::fs::write(root.join("a.md"), b"one\n").expect("铺");
    let path = root.join("a.md");
    let read = crate::files::answer_wire(
        "files-read-text",
        &serde_json::json!({"path": path.to_str().unwrap(), "max_bytes": 1024}),
    )
    .expect("读");
    let sha = read["sha256"]
        .as_str()
        .expect("读的应答里该有 sha256")
        .to_string();
    assert_eq!(sha, sha2_hex(b"one\n"), "读交出来的摘要不是盘上那份的");
    let r = root.to_str().unwrap();
    let got = answer_wire(
        "files-write-text",
        &serde_json::json!({"root": r, "rel": "a.md", "content": "two\n", "expect": {"sha256": sha}}),
    )
    .expect("拿读到的摘要存，该成");
    assert_eq!(
        got["sha256"],
        sha2_hex(b"two\n"),
        "应答交的新摘要不是写进去那份的"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"two\n");
    answer_wire(
        "files-write-text",
        &serde_json::json!({"root": r, "rel": "a.md", "content": "three\n", "expect": {"sha256": got["sha256"]}}),
    )
    .expect("拿上一次应答的摘要再存，该成");
    assert_eq!(std::fs::read(&path).unwrap(), b"three\n");
}

/// ★★ **读完之后盘上被别人改了 ⇒ `stale`，一个字节没写**（E §E11 那一形：窗口开着 `~/.bashrc`，机器页写了别名块，回窗口存）。
/// 已经不在 ⇒ 同样 `stale`，而且不会被顺手建出来。
#[test]
fn a_save_over_a_file_changed_or_removed_since_it_was_read_is_stale_and_writes_nothing() {
    let root = temp_root("cas-stale");
    let r = root.to_str().unwrap();
    std::fs::write(root.join("rc"), b"mine\n").expect("铺");
    let seen = content_sha256(b"mine\n");
    std::fs::write(root.join("rc"), b"mine\n# alias block\n").expect("别人在这期间写了");
    let e = answer_wire(
        "files-write-text",
        &serde_json::json!({"root": r, "rel": "rc", "content": "mine edited\n", "expect": {"sha256": seen}}),
    )
    .expect_err("盘上已经变了，竟然写成了");
    assert_eq!(e.0, "stale", "{e:?}");
    assert_eq!(
        std::fs::read(root.join("rc")).unwrap(),
        b"mine\n# alias block\n",
        "stale 却动了盘"
    );

    std::fs::remove_file(root.join("rc")).unwrap();
    let e = answer_wire(
        "files-write-text",
        &serde_json::json!({"root": r, "rel": "rc", "content": "x", "expect": {"sha256": seen}}),
    )
    .expect_err("不在了，竟然写成了");
    assert_eq!(e.0, "stale", "{e:?}");
    assert!(!root.join("rc").exists(), "不在的那份被建出来了");
}

/// `expect` 必给、形状只收一种：缺了 · `null` · 字符串（逐字节形不给这条）· 大写 · 短一位 · 多一个键 ⇒ 全 `bad_args`，盘上一个字节不动。
#[test]
fn the_write_text_expect_is_required_and_takes_exactly_one_shape() {
    let root = temp_root("cas-shape");
    let r = root.to_str().unwrap();
    std::fs::write(root.join("a"), b"keep").expect("铺");
    let good = content_sha256(b"keep");
    let bad = [
        serde_json::json!({"root": r, "rel": "a", "content": "x"}),
        serde_json::json!({"root": r, "rel": "a", "content": "x", "expect": null}),
        serde_json::json!({"root": r, "rel": "a", "content": "x", "expect": "keep"}),
        serde_json::json!({"root": r, "rel": "a", "content": "x", "expect": {"sha256": good.to_uppercase()}}),
        serde_json::json!({"root": r, "rel": "a", "content": "x", "expect": {"sha256": &good[1..]}}),
        serde_json::json!({"root": r, "rel": "a", "content": "x", "expect": {"sha256": good, "b16": "00"}}),
    ];
    for args in bad {
        let e = answer_wire("files-write-text", &args).expect_err("形状不对竟然收了");
        assert_eq!(e.0, "bad_args", "{args}: {e:?}");
    }
    assert_eq!(std::fs::read(root.join("a")).unwrap(), b"keep");
    // 正控：同一份、形状对 ⇒ 写成（上面的拒不是因为别的）。
    answer_wire(
        "files-write-text",
        &serde_json::json!({"root": r, "rel": "a", "content": "x", "expect": {"sha256": good}}),
    )
    .expect("形状对的那一份该写成");
    assert_eq!(std::fs::read(root.join("a")).unwrap(), b"x");
}

// ═══════════════════════════════════════════════════════════════════════════════════════
//  〔FW1 · 第四波 4D · 2026-09-25〕`files-delete` 的「只删空目录」一形（主会话裁 SU1 问 2）
// ═══════════════════════════════════════════════════════════════════════════════════════
//
// 要求住址（逐字）：题面 `4d-lanes.md`「主会话本批裁的」「SU1 问 2：后端 `files-delete` 加显式『只删空目录』一形
// （带 `expect: empty-dir`），卸 skill 最后删空目录」；`设计/60 §3.3` `files-delete` 那一条（`expect` 是删的 CAS）。

/// ★ 空目录删掉；不空 ⇒ `stale`、里面一个字节不动；不在 ⇒ `stale`；是文件 / 指向目录的链接 ⇒ `refused`、原样留着。
#[test]
fn the_empty_dir_form_deletes_only_an_empty_real_directory() {
    let root = temp_root("fw1-emptydir");
    let r = root.to_str().unwrap();
    let del = |rel: &str| {
        answer_wire(
            "files-delete",
            &serde_json::json!({"root": r, "rel": rel, "expect": {"empty_dir": true}}),
        )
    };
    std::fs::create_dir_all(root.join("empty")).unwrap();
    del("empty").expect("空目录该删掉");
    assert!(!root.join("empty").exists());

    std::fs::create_dir_all(root.join("full")).unwrap();
    std::fs::write(root.join("full/keep.md"), b"keep").unwrap();
    let e = del("full").expect_err("不空竟然删了");
    assert_eq!(e.0, "stale", "{e:?}");
    assert_eq!(std::fs::read(root.join("full/keep.md")).unwrap(), b"keep");

    let e = del("nope").expect_err("不在竟然成了");
    assert_eq!(e.0, "stale", "{e:?}");

    std::fs::write(root.join("afile"), b"x").unwrap();
    let e = del("afile").expect_err("文件竟然按空目录删了");
    assert_eq!(e.0, "refused", "{e:?}");
    assert!(root.join("afile").exists());

    #[cfg(unix)]
    {
        std::fs::create_dir_all(root.join("realdir")).unwrap();
        std::os::unix::fs::symlink(root.join("realdir"), root.join("link")).unwrap();
        let e = del("link").expect_err("指向目录的链接竟然按空目录删了");
        assert_eq!(e.0, "refused", "{e:?}");
        assert!(root.join("link").exists() && root.join("realdir").is_dir());
    }
}

/// 形状只收恰好 `{"empty_dir": true}`：`false` ⇒ `bad_args`（逐字节形取不出）；多带一个 `b16` ⇒ 不是空目录形，
/// 按逐字节形取（`{"b16": …}`）⇒ 目标是目录 ⇒ `refused`；与 `recursive` 同给 ⇒ `bad_args`。每一形都一个字节不动。
#[test]
fn the_empty_dir_form_takes_exactly_one_shape() {
    let root = temp_root("fw1-emptydir-shape");
    let r = root.to_str().unwrap();
    std::fs::create_dir_all(root.join("d")).unwrap();
    for (expect, code) in [
        (serde_json::json!({"empty_dir": false}), "bad_args"),
        (
            serde_json::json!({"empty_dir": true, "b16": "00"}),
            "refused",
        ),
    ] {
        let e = answer_wire(
            "files-delete",
            &serde_json::json!({"root": r, "rel": "d", "expect": expect}),
        )
        .expect_err("形状不对竟然删了");
        assert_eq!(e.0, code, "{expect}: {e:?}");
    }
    let e = answer_wire(
        "files-delete",
        &serde_json::json!({"root": r, "rel": "d", "recursive": true, "expect": {"empty_dir": true}}),
    )
    .expect_err("与 recursive 同给竟然收了");
    assert_eq!(e.0, "bad_args");
    assert!(root.join("d").is_dir(), "拒了却动了盘");
}
