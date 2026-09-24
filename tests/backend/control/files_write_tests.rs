//! 〔步 23b · 2026-09-19〕`control/files_write.rs` 的行为判据 —— **钉住那两道围栏**。
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

// ── 围栏核心判定：`is_protected_session_file`〔波 5 ㈢ 09-23 换的〕──────────

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
    use crate::agents::claudecode::paths::is_protected_session_file;
    // ── 阳性：那几份具体的会话文件 ──────────────────────────────────
    for hit in [
        "/home/u/.claude/projects/-x/abc.jsonl", // `projects/` 下恰 2 段
        "/home/u/.claude/sessions/1234.json",    // pidfile，`sessions/` 下 1 段
        "/opt/accts/q/projects/-x/abc.jsonl",    // 配置根被切走，照样认得（结构判定）
        "C:\\Users\\me\\.claude\\projects\\p\\s.jsonl", // 反斜杠先归一
    ] {
        assert!(
            is_protected_session_file(hit),
            "该判成会话数据却放过了：{hit}"
        );
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
            !is_protected_session_file(miss),
            "🔴 这条路径被拒了，而 09-23 那一裁要求它放行：{miss}"
        );
    }
}

/// 🔴🔴 **两个 crate 里那两份判定的函数体，逐字节相同。**
///
/// 两棵树之间没有共享落点（`src/backend` 刻意不在 monitor 那个 workspace 里），
/// 而 `设计/60 §8.8` 记着上一次「把围栏搬成共享 crate」当天就被撤回。
/// ⇒ 统一只能靠「两份**逐字**副本 ＋ 一条相等断言」。本条是后端这一侧那一份；
/// 桥那一侧还有一份同形的（`tests/bridge/claude_data_fence_tests.rs`），
/// 两侧各自跑得起来 —— 只跑一棵树的人也逃不掉。
///
/// ⚠ 它钉的是**函数体**，不钉函数名（两侧刻意不同名：同名会让桥那条
/// 「`pub fn is_protected_claude_data_path` 全仓恰好一次」的断言红，而那条断言是对的）。
#[test]
fn the_two_copies_of_the_session_fence_are_byte_identical() {
    fn body(src: &str, sig: &str) -> String {
        let at = src
            .find(sig)
            .unwrap_or_else(|| panic!("语料里找不到 `{sig}` —— 抽取坏了，本条此刻在空转"));
        let open = src[at..].find('{').expect("找不到函数体开头") + at;
        let close = src[open..].find("\n}\n").expect("找不到函数体结尾") + open;
        src[open + 1..close + 1].to_string()
    }
    let root = crate::guard_support::repo_root();
    let mine = std::fs::read_to_string(root.join("src/backend/agents/claudecode/paths.rs"))
        .expect("读后端那一份");
    let theirs = std::fs::read_to_string(root.join("src/bridge/src/claude_data_fence.rs"))
        .expect("读桥那一份");
    // 针**运行时拼**：写成字面量的话本文件自己就成了第三处住址。
    let a = body(&mine, &format!("pub fn is_protected_session_{}(", "file"));
    let b = body(
        &theirs,
        &format!("pub fn is_protected_claude_{}_path(", "data"),
    );
    // 反空真：抽出来的必须是真代码。
    assert!(
        a.len() > 400 && a.contains("rfind"),
        "后端那一份抽出来只有 {} 字节 —— 抽取坏了",
        a.len()
    );
    assert_eq!(
        a, b,
        "🔴 **两份会话围栏分叉了。**\n\
         这两份函数体必须逐字节相同 —— 它们是**同一个判定**，\n\
         两份存在的唯一理由是两个 crate 之间没有共享落点（`src/backend` 刻意不在\n\
         monitor 那个 workspace 里，`设计/60 §8.8` 记着搬成共享 crate 被撤回过）。\n\
         ⇒ 处置：改了一侧就把同一段字节抄到另一侧。\n\
         ★ 分叉的代价不是重复代码，是**两份会给出不同答案**：同一次「往 `~/.claude` 里写」\n\
         在后端那条路与桥那条路上结果不同，而界面上看不出这个区别\n\
         （`设计/60 §8.7` 逐字记着这个后果，这一刀治的就是它）。"
    );
}

// ── 围栏①（词法）──────────────────────────────────────────────────────

#[test]
fn the_lexical_fence_refuses_the_four_shapes() {
    let root = PathBuf::from("/srv/target");
    for (rel, word) in [
        ("../escape.txt", "上跳段"),
        ("a/../../escape.txt", "上跳段"),
        ("/etc/passwd", "绝对路径"),
        ("./a.txt", "当前目录段"),
        ("", "空的"),
        ("   ", "空的"),
    ] {
        let err =
            fence_lexical(&root, rel).expect_err(&format!("围栏① 放过了 {rel:?} —— 它该被拒"));
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
    let root = PathBuf::from("/srv/target");
    let ok = fence_lexical(&root, "docs/notes/a.md").expect("干净的相对段被误拒");
    assert_eq!(ok, root.join("docs/notes/a.md"), "落点算错了");
}

/// 🔴🔴 **〔波 5 ㈢ 09-23〕这一格整个翻了牌，而它是那一裁的正题。**
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
fn a_target_root_inside_the_claude_tree_is_allowed_unless_the_write_point_is_a_session_file() {
    // 向一：skills 那一类 —— 必须放行。
    let ok = fence_lexical(Path::new("/home/u/.claude/skills"), "my-skill/SKILL.md")
        .expect("🔴 用户 09-23 裁「可以」，而这条路径被拒了");
    assert_eq!(
        ok,
        PathBuf::from("/home/u/.claude/skills/my-skill/SKILL.md")
    );
    // 向二：写点恰好是那份会话文件 —— 照旧拒。
    let err = fence_lexical(Path::new("/home/u/.claude/projects"), "-x/s.jsonl")
        .expect_err("写点就是一份会话记录，竟然放行了");
    assert!(
        err.contains("Claude 会话数据"),
        "拒了，但不是围栏拒的：{err}"
    );
    // 向二之二：pidfile 那一形也要拦（两形都在判定里，只验一形等于半个判据）。
    let err = fence_lexical(Path::new("/home/u/.claude/sessions"), "4321.json")
        .expect_err("pidfile 竟然放行了");
    assert!(err.contains("Claude 会话数据"), "拒的理由不对：{err}");
}

// ── 围栏②（现打，真 symlink）──────────────────────────────────────────

/// ★★ **本族最承重的一格**：目标根里藏一条 symlink，解完之后写点落到会话文件上。
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
fn the_resolved_fence_catches_a_symlink_onto_a_session_file() {
    let base = temp_root("symlink");
    let root = base.join("target");
    // 解完之后必须长成 `<任意>/projects/<proj>/<sid>.jsonl`（`projects/` 下恰 2 段）。
    let live = base.join("cfg/projects/-x");
    std::fs::create_dir_all(&root).expect("建目标根");
    std::fs::create_dir_all(&live).expect("建会话目录");
    std::os::unix::fs::symlink(&live, root.join("docs")).expect("放 symlink");

    // 围栏①：看不见 —— 这一句是**对照**，它证明围栏② 不是多余的。
    let lexical = fence_lexical(&root, "docs/abc.jsonl").expect("围栏① 本来就该放过它");
    // 围栏②：解完之后当场拦下。
    let err = fence_resolved(&root, &lexical)
        .expect_err("目标根里的 symlink 指到会话目录，围栏② 竟然放行了");
    assert!(
        err.contains("Claude 会话数据"),
        "拒了，但不是 Claude 围栏拒的：{err}"
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

    let lexical = fence_lexical(&root, "s/SKILL.md").expect("围栏① 该放过它");
    let err = fence_resolved(&root, &lexical).expect_err("这一格今天该以「跑出目标根」被拒");
    // 🔴 它仍然被拒，**但理由必须是越界，不是 Claude** —— 两者的差别就是这一裁的全部内容。
    assert!(
        err.contains("跑出了目标根"),
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

    // 而同一个根底下，**会话文件那一形照旧写不进去**（阳性对照，同拍）。
    let live = base.join(".claude/projects");
    std::fs::create_dir_all(&live).expect("建 projects 根");
    let err = create_new_file(&live, "-x/s.jsonl", b"x").expect_err("会话记录竟然写进去了");
    assert_eq!(err.code(), "refused", "档位不对：{err:?}");
    assert!(!live.join("-x/s.jsonl").exists(), "说拒了，文件却落盘了");
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

    let lexical = fence_lexical(&root, "out/a.md").expect("围栏① 该放过它");
    let err = fence_resolved(&root, &lexical).expect_err("symlink 指出目标根，围栏② 竟然放行了");
    assert!(err.contains("跑出了目标根"), "拒了，但说的不是越界：{err}");
    std::fs::remove_dir_all(&base).ok();
}

/// ★ 阴性对照：没有 symlink 的干净路径，围栏② 必须放行并解出真路径。
#[test]
fn the_resolved_fence_lets_a_clean_path_through() {
    let root = temp_root("clean2");
    std::fs::create_dir_all(root.join("docs")).expect("建子目录");
    let lexical = fence_lexical(&root, "docs/a.md").expect("围栏① 该放过它");
    let got = fence_resolved(&root, &lexical).expect("围栏② 误拒了干净路径");
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
    let err = fence_resolved(&root, &root.join("nope/a.md")).expect_err("父目录不在，却没拒");
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

/// 同上一格的 Claude 那一侧：写点**就是一份会话记录**，入口必须拒，且盘上不留东西。
///
/// ⚠ 〔波 5 ㈢ 09-23〕原来这一格建的根是 `<临时>/.claude`、写点 `a.md`，
/// 断言「落在 `.claude` 段底下就拒」。**那一句今天是假的**（用户裁「可以」）
/// ⇒ 语料换成真正还被拦的那一形：`projects/<proj>/<sid>.jsonl`。
#[test]
fn the_write_entry_point_refuses_a_live_session_file() {
    let base = temp_root("claudewire");
    let root = base.join(".claude/projects");
    std::fs::create_dir_all(root.join("-x")).expect("建会话目录");

    let err = create_new_file(&root, "-x/abc.jsonl", b"x")
        .expect_err("写点就是一份会话记录，写入口竟然放行了");
    assert_eq!(err.code(), "refused", "档位不对：{err:?}");
    assert!(
        err.message().contains("Claude 会话数据"),
        "拒了，但不是 Claude 围栏拒的：{}",
        err.message()
    );
    assert!(!root.join("-x/abc.jsonl").exists(), "说拒了，文件却落盘了");
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
    // 解完之后要恰好长成 `<任意>/projects/<proj>/<sid>.jsonl`。
    let live = base.join("cfg/projects/-x");
    std::fs::create_dir_all(&root).expect("建目标根");
    std::fs::create_dir_all(&live).expect("建会话目录");
    std::os::unix::fs::symlink(&live, root.join("docs")).expect("放 symlink");

    let err = create_new_file(&root, "docs/abc.jsonl", b"x")
        .expect_err("词法上干净、解完却落到一份会话记录上 —— 写入口竟然放行了");
    assert_eq!(err.code(), "refused", "档位不对：{err:?}");
    assert!(
        err.message().contains("Claude 会话数据"),
        "拒了，但不是围栏② 的 Claude 那一关拒的：{}",
        err.message()
    );
    assert!(
        !live.join("abc.jsonl").exists(),
        "🔴 说拒了，文件却真的落进那个会话目录了：{}",
        live.join("abc.jsonl").display()
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
    // 解完之后恰好长成 `<任意>/projects/<proj>/<sid>.jsonl`。
    let live = base.join("cfg/projects/-x");
    std::fs::create_dir_all(&root).expect("建目标根");
    std::fs::create_dir_all(&live).expect("建会话目录");
    std::os::unix::fs::symlink(&live, root.join("docs")).expect("放 symlink");

    let (code, msg) = answer_wire(
        "files-create",
        &serde_json::json!({"root": root.to_str().expect("utf8"), "rel": "docs/abc.jsonl"}),
    )
    .expect_err("词法上干净、解完却落到一份会话记录上 —— 命令面竟然放行了");
    assert_eq!(code, "refused", "档位不对（{msg}）");
    assert!(
        msg.contains("Claude 会话数据"),
        "拒了，但不是围栏② 的 Claude 那一关拒的：{msg}"
    );
    assert!(
        !live.join("abc.jsonl").exists(),
        "🔴 说拒了，文件却真的落进那个会话目录了：{}",
        live.join("abc.jsonl").display()
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
// 只验「该成的成了」，一个没有围栏的实现也全绿。
// 🔴 阴性那一侧每一条都**去盘上核**「那份会话文件一个字节没动」—— 回了 `Err` 不算数。

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
fn mkdir_builds_one_level_and_refuses_a_session_shaped_target() {
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
    // 阴性：一条长成会话文件形状的路径，建目录也不许。
    let err = make_dir(&root, "projects/-x")
        .map(|_| ())
        .and_then(|_| make_dir(&root, "projects/-x/abc.jsonl").map(|_| ()));
    let err = err.expect_err("会话文件那个位置上竟然建出了目录");
    assert_eq!(err.code(), "refused", "{err:?}");
    assert!(!root.join("projects/-x/abc.jsonl").exists());
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn rename_moves_inside_the_root_and_never_overwrites_or_touches_a_session_file() {
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

    // 🔴 两个参数各过一遍围栏：from 是会话文件 ⇒ 拒；to 是会话文件的名字 ⇒ 拒。
    let err =
        rename_entry(&root, "projects/-x/abc.jsonl", "moved.jsonl").expect_err("会话文件被改走了");
    assert_eq!(err.code(), "refused", "{err:?}");
    let err = rename_entry(&root, "c.md", "projects/-x/new.jsonl")
        .expect_err("普通文件被改名成了一份会话记录");
    assert_eq!(err.code(), "refused", "{err:?}");
    assert_eq!(
        std::fs::read(&live).expect("读会话"),
        bytes,
        "🔴 会话文件被动了"
    );
    assert!(root.join("c.md").exists(), "被拒的那一次把源文件挪走了");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn delete_removes_files_empty_dirs_and_links_but_never_a_session_file_or_a_subtree() {
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

    // 🔴 阴性：会话文件本身 ⇒ 拒，盘上原样。
    let err = delete_entry(&root, "projects/-x/abc.jsonl").expect_err("会话文件被删了");
    assert_eq!(err.code(), "refused", "{err:?}");
    assert_eq!(std::fs::read(&live).expect("读会话"), bytes);
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn chmod_follows_links_so_it_resolves_to_the_end_before_judging() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = temp_root("chmod");
    let root = base.join("cfg");
    let (live, _) = plant_live_session(&base);
    let before = std::fs::metadata(&live)
        .expect("读会话元数据")
        .permissions()
        .mode();
    std::fs::write(root.join("run.sh"), b"#!/bin/sh\n").expect("铺");
    std::os::unix::fs::symlink(&live, root.join("innocent.txt")).expect("放链接");

    // 正控。
    change_mode(&root, "run.sh", 0o700).expect("干净的改权限被误拒");
    let m = std::fs::metadata(root.join("run.sh"))
        .expect("读")
        .permissions()
        .mode();
    assert_eq!(m & 0o7777, 0o700, "说改了，盘上的权限位不对");

    // 🔴 阴性：名字干净（`innocent.txt`），解到底却是会话文件 ⇒ 拒，而且权限位没变。
    //    只解父目录的围栏（`fenced_target`）在这一形上是**瞎的** —— 这一格就是
    //    `fenced_existing` 存在的理由。
    let err =
        change_mode(&root, "innocent.txt", 0o000).expect_err("借一条链接把会话文件改成了不可读");
    assert_eq!(err.code(), "refused", "{err:?}");
    let after = std::fs::metadata(&live)
        .expect("读会话元数据")
        .permissions()
        .mode();
    assert_eq!(before, after, "🔴 会话文件的权限位被改了");

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
    let (live, bytes) = plant_live_session(&base);
    std::fs::write(root.join("a.md"), b"old").expect("铺");
    std::os::unix::fs::symlink(&live, root.join("notes.md")).expect("放链接");

    // 正控。
    let got = overwrite_text(&root, "a.md", b"new").expect("干净的覆盖写被误拒");
    assert_eq!(std::fs::read(&got).expect("读回"), b"new");

    // 🔴 阴性：链接指向会话文件 ⇒ 拒，会话一个字节没动。
    let err = overwrite_text(&root, "notes.md", b"PWNED").expect_err("借链接覆盖了会话文件");
    assert_eq!(err.code(), "refused", "{err:?}");
    assert_eq!(
        std::fs::read(&live).expect("读会话"),
        bytes,
        "🔴 会话被覆盖了"
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

/// ★ 命令面这一侧五条都**够得到**，而且那道围栏在命令面上照样咬。
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
        serde_json::json!({"root": r, "rel": "d/b.md", "content": "zz"}),
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

    // 阴性：同样五条，对着那份会话文件 ⇒ 全部 `refused`，会话一个字节没动。
    for (cmd, args) in [
        (
            "files-mkdir",
            serde_json::json!({"root": r, "rel": "projects/-x/n.jsonl"}),
        ),
        (
            "files-rename",
            serde_json::json!({"root": r, "from": "projects/-x/abc.jsonl", "to": "z"}),
        ),
        (
            "files-write-text",
            serde_json::json!({"root": r, "rel": "projects/-x/abc.jsonl", "content": "x"}),
        ),
        (
            "files-chmod",
            serde_json::json!({"root": r, "rel": "projects/-x/abc.jsonl", "mode": 0}),
        ),
        (
            "files-delete",
            serde_json::json!({"root": r, "rel": "projects/-x/abc.jsonl"}),
        ),
    ] {
        let (code, msg) = match answer_wire(cmd, &args) {
            Err(e) => e,
            Ok(v) => panic!("🔴 `{cmd}` 对着会话文件成功了：{v}"),
        };
        assert_eq!(code, "refused", "`{cmd}` 档位不对：{msg}");
    }
    assert_eq!(
        std::fs::read(&live).expect("读会话"),
        bytes,
        "🔴 会话被动了"
    );
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

/// ★ 显式覆盖：目标换成新内容，**不留暂存旁名**；目标是一条链接时顶掉的是**链接本身**，
/// 它指着的那份会话记录一个字节没动。
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

/// 🔴 三条路径各过一遍围栏：源是会话文件 / 源是指向会话文件的链接 / 目标是会话文件的位置
/// ⇒ 全部 `refused`；会话一个字节没动，被拒的目标没被建出来。
#[test]
#[cfg(unix)]
fn copy_is_fenced_on_the_source_the_target_and_through_a_link() {
    let base = temp_root("cpf");
    let root = base.join("cfg");
    let (live, bytes) = plant_live_session(&base);
    std::fs::write(root.join("a.md"), b"A").expect("铺 a");
    std::os::unix::fs::symlink(&live, root.join("peek.md")).expect("铺链接");
    for (from, to, why) in [
        ("projects/-x/abc.jsonl", "stolen.jsonl", "源就是会话文件"),
        (
            "peek.md",
            "stolen.md",
            "源是指向会话文件的链接（解到底再判）",
        ),
        ("a.md", "projects/-x/new.jsonl", "目标落在会话文件的位置上"),
    ] {
        for overwrite in [false, true] {
            let err = copy_entry(&root, from, to, overwrite)
                .expect_err(&format!("🔴 {why}（overwrite={overwrite}）竟然复制成了"));
            assert_eq!(err.code(), "refused", "{why}：{err:?}");
        }
    }
    assert!(!root.join("stolen.jsonl").exists() && !root.join("stolen.md").exists());
    assert!(!root.join("projects/-x/new.jsonl").exists());
    assert_eq!(
        std::fs::read(&live).expect("读会话"),
        bytes,
        "🔴 会话被动了"
    );
    assert!(copy_leftovers(&root.join("projects/-x"), "new.jsonl").is_empty());
    assert!(
        copy_leftovers(&root, "stolen.md").is_empty()
            && copy_leftovers(&root, "stolen.jsonl").is_empty()
    );
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
fn peek_tells_absent_from_present_and_goes_through_the_same_fence() {
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
    assert_eq!(
        peek_text(&root, "cfg/projects/-p/s1.jsonl").unwrap_err().0,
        "refused",
        "读改写的读那一半与写同一道围栏 —— 会话文件读不进来"
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
        Err(WriteRefusal::Fenced(_)) => {}
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
    // 链接指到一份会话文件上 ⇒ 解到底之后被拒，那份会话一个字节不动。
    let (live, bytes) = plant_live_session(&root);
    std::os::unix::fs::symlink(&live, root.join("sneaky.txt")).expect("建链接");
    let got = put_text(&root, "sneaky.txt", b"x", Some(&bytes), false, false);
    assert!(matches!(got, Err(WriteRefusal::Fenced(_))), "{got:?}");
    assert_eq!(std::fs::read(&live).expect("会话"), bytes);
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn put_never_writes_a_session_file_directly() {
    let base = temp_root("psess");
    let (live, bytes) = plant_live_session(&base);
    let got = put_text(
        &base,
        "cfg/projects/-x/abc.jsonl",
        b"x",
        Some(&bytes),
        false,
        false,
    );
    assert!(matches!(got, Err(WriteRefusal::Fenced(_))), "{got:?}");
    assert_eq!(std::fs::read(&live).expect("会话"), bytes);
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

// ── 删历史会话：会话文件围栏唯一的例外 ────────────────────────────────────────

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
    assert!(matches!(again, Err(WriteRefusal::Fenced(_))), "{again:?}");
    for bad in ["../x", "a/b", "", "x.jsonl"] {
        let got = delete_session_with(bad, |s| {
            crate::agents::claudecode::paths::session_file_for_delete_in(&home, s)
        });
        assert!(
            matches!(got, Err(WriteRefusal::Fenced(_))),
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
    assert!(matches!(got, Err(WriteRefusal::Fenced(_))), "{got:?}");
    assert_eq!(std::fs::read(&notes).expect("notes"), b"keep me");
    // 名字对得上、但不在 `projects/<proj>/` 底下（不是会话的形状）⇒ 只有「形状」那一问拦得住。
    let named = base.join("docs/s1.jsonl");
    std::fs::write(&named, "not a session").expect("铺同名普通文件");
    let got = delete_session_with("s1", |_| Ok(named.clone()));
    assert!(matches!(got, Err(WriteRefusal::Fenced(_))), "{got:?}");
    assert!(named.exists(), "一份同名的普通文件被当成会话删了");
    // 名字对不上 sid（别的会话）同样拒。
    let home = plant_home(&base, "s2");
    let other = home.join("projects/-p/s2.jsonl");
    let got = delete_session_with("s1", |_| Ok(other.clone()));
    assert!(matches!(got, Err(WriteRefusal::Fenced(_))), "{got:?}");
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
    assert!(matches!(got, Err(WriteRefusal::Fenced(_))), "{got:?}");
    assert!(outside.exists(), "跟着链接删到了记录树外面那一份");
    std::fs::remove_dir_all(&base).ok();
}
