//! `claude_data_fence` 自己那一族判据〔步 H2 · 2026-09-21，用户逐字裁「拆」〕。
//!
//! # 搬过来的两条 ＋ 新的三条
//!
//! 搬：`sftp_pool_tests` 原先那两条（一条判**判定**、一条判**拒绝**）的断言
//! **一条没少**，并进了下面那条两方向的相等断言里 —— 于是判定与拒绝这一对
//! 在同一条判据里一起被喂，而不是各自一条、各自有可能单独变瞎。
//! ⚠ 那两个旧判据名此处**刻意不复述**：它们已经不在盘上了，写出来会让
//! `structural_scan::every_dead_name_named_in_the_prose_is_declared_dead` 当场点名
//! （本轮首跑就红在这里，两处）。
//!
//! 新：① 判定**只有一个家**（人群按事实取样，不是按谁调了它）·
//! ② 围栏**够不着线** · ③ 旧住址的**递减棘轮**。
//!
//! # ⚠ 为什么第一条的人群不能是「谁调了这个函数」
//!
//! 那样取样是**恒真**的：凡是调 `is_protected_claude_data_path` 的地方，
//! 按构造问的就是同一个函数。真正要挡的是**另写一份判定** —— 有人在别处手搓
//! 一个 `path.contains(…) && path.ends_with(…)`。
//! ⇒ 人群改按**事实**取样：生产段里带「Claude 数据布局的路径串」的每一份文件。

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 判定这一族的**唯一住址**（仓相对）。别处再写一份 `pub fn` 就是第二个家。
const FENCE_HOME: &str = "src/bridge/src/claude_data_fence.rs";

// ════════════════════════════════════════════════════════════════════════════
//  一、围栏两个方向 —— 成对的样本，两条都是相等断言
// ════════════════════════════════════════════════════════════════════════════

/// `(受保护的那一条, 与它**只差一处结构**的普通路径, 差在哪)`。
///
/// 🔴 **刻意成对**，而不是两张各自独立的清单。
///
/// 两张独立清单的失效模式是具体的：阳性那张列一堆 `.jsonl`、阴性那张列一堆 `.rs`，
/// 于是「把判定改成 `path.ends_with(\".jsonl\")`」这种**退化**两张都过 ——
/// 而那正好丢掉 batch20 那次审计修买到的东西（结构判定，不靠 `.claude` 字面）。
/// 成对之后，每一对都在钉「判定的边界**恰好**在它自称的那个位置上」。
///
/// ⚠ 搬迁登记：这些样本逐条来自 `sftp_pool_tests` 原先那两条判据，
/// 一条都没删；成对那一栏是本条新加的。
const FENCE_BOUNDARY_PAIRS: &[(&str, &str, &str)] = &[
    (
        "/home/pi/.claude/projects/-x/abc.jsonl",
        "/home/pi/.claude/projects/-x/abc.jsonl.bak",
        "后缀 —— `projects/<proj>/<sid>.jsonl` 恰好以 `.jsonl` 收尾才是会话文件；\
         旁边那份备份不是 Claude 在写的那一个",
    ),
    (
        "/home/pi/.claude/projects/-x/abc.jsonl",
        "/home/pi/.claude/projects/-x/sub/abc.jsonl",
        "**层数** —— `projects/` 下恰 2 段。多一层就不是会话文件那个位置了\
         （⚠ 这一形的代价登记在 [`THE_SHAPES_THIS_FENCE_DOES_NOT_COVER`]：\
         子 agent 的记录正落在这个位置上）",
    ),
    (
        "/home/pi/x/projects/-x/abc.jsonl",
        "/home/pi/x/projects/a.jsonl",
        "**层数（少一层）** —— `projects/` 下只 1 段的普通文件不许误伤。\
         ★ 这一对同时钉着「判定不靠 `.claude` 字面」：左边那条路径里根本没有 `.claude`，\
         它照样被挡（batch20 审计修闭的 `CLAUDE_CONFIG_DIR` 重定位缺口）",
    ),
    (
        "/home/u/.claude/sessions/123.json",
        "/home/pi/.claude/settings.json",
        "**目录** —— pidfile 住 `sessions/` 底下；`settings.json` 不在那儿，\
         而且它**刻意不受保护**（`INVARIANTS §1` 的 SS-14 那一条：写面绝不含 settings）",
    ),
    (
        "/home/u/mydata/sessions/123.json",
        "/x/sessions/sub/y.json",
        "**层数** —— `sessions/` 下恰 1 段。再嵌一层目录就不是 Claude 那个单层结构了。\
         ★ 左边同样是 `CLAUDE_CONFIG_DIR` 重定位之后的形状（目录名不叫 `.claude`）",
    ),
    (
        "C:\\Users\\me\\.claude\\projects\\p\\s.jsonl",
        "C:\\Users\\me\\proj\\main.rs",
        "反斜杠归一 —— Windows 形态的路径必须先归一成 `/` 再判，\
         否则面板从 Windows 侧写远端时整道围栏是瞎的。\
         🔴 **它的项目段刻意叫 `p`，不带前导 `-`** —— 别改成 `-p`：\
         死值验 `M12` 现打，那是本表唯一一条不带前导 `-` 的样本，\
         而它就是那一刀唯一的钉子（理由逐字写在本表下面那条判据的\
         「它钉不了什么」里）",
    ),
];

/// 🔴 **围栏两个方向，两条都是相等断言**：受保护的**全部**被挡、普通的**全部**过。
///
/// # 为什么不是「至少挡住了几条」
///
/// 地板在「变少」那个方向上是瞎的：判定被改窄之后，剩下还挡得住的那几条照样让地板过。
/// 这里两侧都是 `assert_eq!(命中数, 样本数)` ——
/// 少挡一条、或多挡一条（误伤普通文件）都点名那一条。
///
/// # 「不过网」那一半在哪
///
/// 本条只喂字符串、不起任何连接 —— 这件事本身由 [`the_fence_cannot_reach_the_wire`]
/// 从结构上钉住（本模块生产段里零传输符号），
/// 而「池子那七条命令**先判后拨**」由
/// `remote_write_registry_tests::a_fenced_write_refuses_before_it_touches_the_wire` 钉住。
/// 三条各管一段，别把其中任何一条读成全部。
#[test]
fn the_fence_refuses_every_protected_shape_and_passes_every_ordinary_one() {
    use crate::claude_data_fence::{guard_write, is_protected_claude_data_path};

    // 反空真：样本表空了，下面两条相等断言就是 `0 == 0`。
    assert!(
        FENCE_BOUNDARY_PAIRS.len() >= 6,
        "成对样本只剩 {} 对 —— 本条此刻在空转（2026-09-21 现打 6 对）",
        FENCE_BOUNDARY_PAIRS.len()
    );

    // ── 方向一：受保护的必须被挡（判定为真 **且** `guard_write` 真的回 `Err`）──
    let blocked: Vec<&str> = FENCE_BOUNDARY_PAIRS
        .iter()
        .filter(|(p, ..)| is_protected_claude_data_path(p) && guard_write(p).is_err())
        .map(|(p, ..)| *p)
        .collect();
    let leaked: Vec<&(&str, &str, &str)> = FENCE_BOUNDARY_PAIRS
        .iter()
        .filter(|(p, ..)| !blocked.contains(p))
        .collect();
    assert_eq!(
        blocked.len(),
        FENCE_BOUNDARY_PAIRS.len(),
        "这几条 Claude 数据路径**没被挡住**：{leaked:?}\n\n\
         ★ 后果是具体的：那台机器上**正被 Claude 打开**的会话文件能被文件面板\n\
         删掉 / 改走 / 覆盖 / 改成不可读，而 monitor 这边只会看到会话突然坏了。\n\
         ⚠ 这是 `src/doc/INVARIANTS.md` `§1` 底下 F47 与 F03b 两段澄清\n\
         **共同**依赖的那一个判定 —— 它松一格，两段澄清同时失去依据。"
    );

    // ── 方向二：普通路径必须过（判定为假 **且** `guard_write` 真的回 `Ok`）──
    let passed: Vec<&str> = FENCE_BOUNDARY_PAIRS
        .iter()
        .filter(|(_, q, _)| !is_protected_claude_data_path(q) && guard_write(q).is_ok())
        .map(|(_, q, _)| *q)
        .collect();
    let hurt: Vec<&(&str, &str, &str)> = FENCE_BOUNDARY_PAIRS
        .iter()
        .filter(|(_, q, _)| !passed.contains(q))
        .collect();
    assert_eq!(
        passed.len(),
        FENCE_BOUNDARY_PAIRS.len(),
        "这几条**普通用户文件**被误伤了：{hurt:?}\n\n\
         ★ 误伤方向同样贵：F47 面板的正题是「浏览/传输任意用户文件」，\n\
         围栏宽一格就是一件用户做不了的事，而报错只会说「拒绝写 Claude 数据源文件」。\n\
         ⚠ 每一对的第三栏写着它俩**差在哪** —— 先读那一栏，再决定是判定错了还是样本错了。"
    );

    // 成对那件事本身要有牙：两侧不许是同一条路径（否则这一对什么都没钉）。
    let degenerate: Vec<&(&str, &str, &str)> = FENCE_BOUNDARY_PAIRS
        .iter()
        .filter(|(p, q, _)| p == q)
        .collect();
    assert!(
        degenerate.is_empty(),
        "这几对的两侧是同一条路径：{degenerate:?} —— 那一对两个方向都在说同一件事"
    );
    for (p, q, why) in FENCE_BOUNDARY_PAIRS {
        assert!(
            why.chars().count() > 20,
            "`{p}` / `{q}` 那一对没说清差在哪，像是占位：「{why}」"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  二、判定只有一个家 —— 人群按**事实**取样
// ════════════════════════════════════════════════════════════════════════════

/// 「读 Claude 数据布局」这件事的结构特征：**带引号的**路径串。
///
/// ⚠ 引号刻意是针的一部分：`tool_registry` 里那个给界面看的
/// `"~/.claude/projects/"` 含子串 `/projects/`，但它不是一处路径判定
/// —— 匹配单位比事实小就会把它拖进人群（`needle_anchor_registry` 治的那一族）。
fn layout_needles() -> Vec<String> {
    // 运行时拼：写成字面量的话，本文件自己就是一处命中
    //（`scanning_guard_registry` 头注那条「判据在自己的常量里找到自己」）。
    let (q, slash) = ('"', '/');
    ["projects", "sessions"]
        .iter()
        .map(|seg| format!("{q}{slash}{seg}{slash}{q}"))
        .collect()
}

/// `(仓相对住址, 它是哪一个判定, 为什么它不是第二份)`。**默认拒绝**：
/// 生产段里冒出第三份就得回来论证。
const LAYOUT_READERS: &[(&str, &str, &str)] = &[
    (
        FENCE_HOME,
        "claude_data_fence：**禁写**向的那一道",
        "本族唯一的那个判定。`INVARIANTS §1` 底下 F47（SFTP 面板 / 原生文件窗口）\
         与 F03b（收件箱编辑的纵深②）两段澄清共用它。",
    ),
    (
        "src/bridge/src/sftp.rs",
        "sftp::is_safe_remote_jsonl：**方向相反**的那一道",
        "它的正题恰恰是「**只许**删 `projects/**/*.jsonl`」——\
         `INVARIANTS §1` 例外 3（历史浏览器删远端会话，用户显式 + 二次确认）。\
         🔴 **两道方向相反，不许互相替代、不许合并**：合成一个之后，\
         「哪些路径不许写」与「哪些路径才许删」会共用一个真相，\
         而它们要的恰好是补集。`remote_write_registry_tests` 的 `REMOTE_WRITES` 表里\
         `remove_remote_file` 那一行逐字记着这件事。",
    ),
];

/// 🔴 **「这是不是受保护路径」这一问，全仓只有一个家。**
///
/// # 两向相等，两层
///
/// ① **人群按事实**：生产段里带 Claude 数据布局路径串的文件，集合与
///    [`LAYOUT_READERS`] 逐条相等（多一份 ⇒ 有人另写了一份判定；
///    少一份 ⇒ 某一道退役了，登记要跟着删）。
/// ② **定义只有一处**：`pub fn is_protected_claude_data_path` 在三棵树的生产段里
///    **恰好一次**，而且住 [`FENCE_HOME`]。
///
/// # ⚠ 它钉不了什么（如实登记）
///
/// - 人群按**那两个路径串**取样。有人用 `format!` 把 `projects` 拼出来、
///   或者改判 `.jsonl` 后缀而完全不提目录名，本条看不见那一份。
///   ⇒ 本条挡的是「顺手抄一份判定过去」这个**实际会发生的动作**，
///   不是一个蓄意伪装的对手。
/// - 它判「谁读这个布局」，判不了「读得对不对」——
///   那是 [`the_fence_refuses_every_protected_shape_and_passes_every_ordinary_one`] 那一半。
///
/// # 🔴 那一半自己的盲区 —— 死值验 `M12` 现打，如实登记
///
/// [`FENCE_BOUNDARY_PAIRS`] 是**我写的样本**，不是从事实派生的人群
/// ⇒ 一种「恰好放过每一条样本」的收窄，它**看不见**。
/// `M12` 把判定收成 `parts[0].starts_with('-')`（项目段必须带前导 `-`，
/// 那是 Claude 的目录编码常见形，看着像对的）——
/// 真实后果是：项目路径不以 `-` 开头的那些机器上，**整道围栏形同不存在**。
///
/// 那一刀**红了**，而它红的理由值得写下来：本表六对里**只有一对**
/// （那条 Windows 形态的）项目段不带前导 `-`。
/// ⇒ 换句话说，它是**靠一条样本的偶然取名**钉住的，不是靠人群的构造。
/// 那条样本的取名从此是**承重**的（已在它那一行逐字写明「别改成 `-p`」）。
///
/// ⚠ **不假装这已经修好**：真要按构造挡住这一族，人群得从「路径的真实形状空间」
/// 派生（属性测试 / 穷举那几段的形状），本仓今天没有那个量具。已登记，未做。
#[test]
fn the_protected_path_judgement_has_exactly_one_home() {
    let root = repo_root();
    let needles = layout_needles();
    let mut on_disk: Vec<String> = Vec::new();
    let mut defs: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    // 拼出来的，免得命中本文件自己的说明（同上）。
    let def_sig = format!("pub fn is_protected_claude_{}_path", "data");
    // 🔴 走 `guard_core::scan_tree!` 而不是自己遍历（`scanning_guard_registry` 那条
    //    递减棘轮明禁裸遍历）。**本文件不在被扫的那两棵树里** —— 它住 `tests/bridge/`，
    //    而下面扫的是 `src/bridge/src` 与 `src/backend` 两棵生产树。
    for sub in ["src/bridge/src", "src/backend"] {
        let files = guard_core::scan_tree!(&root.join(sub), &["rs"]);
        scanned += files.len();
        for (path, src) in files {
            let prod = guard_core::production_code(&src);
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if needles.iter().any(|n| prod.contains(n.as_str())) {
                on_disk.push(rel.clone());
            }
            if prod.contains(&def_sig) {
                defs.push(rel);
            }
        }
    }
    // 抽取器自检①：**采集量地板**。射程被打空 ⇒ 两边都空 ⇒ 相等断言恒真。
    assert!(
        scanned >= 150,
        "两棵生产树只采到 {scanned} 份 `.rs`（2026-09-21 现打：`src/bridge/src` 112 ＋ \
         `src/backend` 72）—— 扫描面坏了，本条此刻是空转的"
    );
    // 抽取器自检②：**针还认得出东西**。针拼错 ⇒ 人群空 ⇒ 同样恒真。
    assert!(
        !on_disk.is_empty(),
        "两个布局针（{needles:?}）在两棵生产树里一处都没命中 —— 针坏了，不是仓变干净了"
    );

    on_disk.sort();
    on_disk.dedup();
    let mut declared: Vec<String> = LAYOUT_READERS.iter().map(|(f, ..)| f.to_string()).collect();
    declared.sort();
    // 🔴 **相等断言，两向。地板不行** —— 「变少」那个方向上地板是瞎的，
    //    而某一道悄悄退役正是要抓的事之一。
    assert_eq!(
        declared,
        on_disk,
        "「谁在读 Claude 的数据布局」与登记表对不上。\n  \
         登记了而盘上没有（那一道退役了？登记要跟着删，别留僵尸账）：{:?}\n  \
         盘上有而没登记（🔴 **很可能是有人另写了一份判定**）：{:?}\n\n\
         ★ 这一族的全部意义是「**哪些路径是 Claude 自己的数据**只有一个真相」。\n\
         第二份判定的代价不是重复代码，是**两份会分叉**：修了一份、另一份照旧放行，\n\
         而两份都「看起来在挡」。\n\
         ⇒ 处置：把新那一处改成调 `claude_data_fence::is_protected_claude_data_path`；\n\
         真要新立一道（比如又一个方向相反的白名单），往 `LAYOUT_READERS` 加一行\n\
         并写清**为什么它不是第二份**。",
        declared
            .iter()
            .filter(|d| !on_disk.contains(d))
            .collect::<Vec<_>>(),
        on_disk
            .iter()
            .filter(|u| !declared.contains(u))
            .collect::<Vec<_>>()
    );

    // ② 定义恰好一处，且住那个家。
    assert_eq!(
        defs,
        vec![FENCE_HOME.to_string()],
        "`{def_sig}` 的定义不止一处、或者不住 `{FENCE_HOME}` 了（实得 {defs:?}）。\n\
         ★ 「一个家」这件事是编译器答案的那一半：`sftp_pool` 里那一行是 `pub use`\n\
         （转出住址，不是第二个家），由 [`the_old_address_is_down_to_its_last_consumer`] 数着。"
    );

    for (f, which, why) in LAYOUT_READERS {
        assert!(
            why.chars().count() > 20,
            "`{f}`（{which}）那一行的理由太短，像是占位：「{why}」"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  三、围栏够不着线
// ════════════════════════════════════════════════════════════════════════════

/// 🔴 **围栏判得动「被挡」这件事，靠的是它一行传输代码都没有。**
///
/// # 它买的是什么
///
/// F47 澄清段逐字要求「每次写都是面板内一次直接用户手势（**绝无自动/后台写**）」——
/// 而「被挡住」这件事如果要一次往返才判得出来，那它就：
/// ① 在一台连不上的机器上判不动（本仓红线：判据里不许起真连接）；
/// ② 意味着**踩线的那条路径已经上过线了**，而围栏的全部意义是它别上线。
///
/// ⇒ 本条把「够不着线」钉成结构事实：本模块的生产段里
/// **零传输符号、零 `async`、零文件 IO**。判定的语料只有一个 `&str`。
///
/// ⚠ 它钉不了「调用方先判后拨」—— 那一半在
/// `remote_write_registry_tests::a_fenced_write_refuses_before_it_touches_the_wire`。
#[test]
fn the_fence_cannot_reach_the_wire() {
    let raw = std::fs::read_to_string(repo_root().join(FENCE_HOME))
        .unwrap_or_else(|e| panic!("{FENCE_HOME} 读不到：{e} —— 文件搬了就把这里一起改"));
    let prod = guard_core::production_code(&raw);
    // 抽取器自检：剥完还得剩下那个判定本身，否则下面全是「在空文本里找不到」。
    assert!(
        prod.contains("rfind") && prod.contains("ends_with"),
        "剥完生产段之后判定不在里面了 —— 剥法或住址坏了，本条此刻是空转的"
    );
    // 针运行时拼，免得本文件自己成为一处命中。
    let forbidden: Vec<String> = [
        ("with_", "sftp("),
        ("connect_", "sftp("),
        ("Sftp", "Session"),
        ("russh", "_sftp"),
        ("async", " "),
        (".await", ""),
        ("std::fs", "::"),
        ("tokio", "::"),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect();
    let found: Vec<&String> = forbidden
        .iter()
        .filter(|n| prod.contains(n.as_str()))
        .collect();
    assert!(
        found.is_empty(),
        "围栏模块的生产段里出现了传输 / 异步 / 文件 IO 符号：{found:?}\n\n\
         ★ 那会让「这条路径被挡住」从一个**纯判定**变成一次可能失败、可能要网络的操作。\n\
         后果有两个方向：① 判据里判不动它（本仓不许起真连接）；\n\
         ② 围栏自己开始依赖「先连上去看一眼」，而踩线的那条路径那时已经上过线了。\n\
         ⇒ 要「看一眼那场会话是不是真活着」，那是另一件事、要用户拍（见本模块头注最后一节）。"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  四、旧住址的递减棘轮
// ════════════════════════════════════════════════════════════════════════════

/// 还在走 `sftp_pool` 那个**转出住址**的文件。**恰好一份，只许变短。**
const OLD_ADDRESS_HOLDOUTS: &[(&str, &str)] = &[(
    "src/bridge/src/filewin/writeops.rs",
    "`fenced_path` 逐字写着 `sftp_pool::is_protected_claude_data_path`。\
     `filewin/` 那棵树本轮在别人的写区里（那五条文件操作刚接到窗口上）⇒ 本轮不碰它。\
     ⚠ 它**问的是同一个函数**（`pub use` 是编译器保证的同一份），\
     所以这不是「第二份判定」，只是「第二个写法」。",
)];

/// 🔴 **旧住址是一条递减棘轮，不是一个永久别名。**
///
/// `sftp_pool` 里那行 `pub use` 只为还没改过来的消费者存在。本条钉两件事：
/// ① 还在用旧住址的文件集合**逐条相等**（多一份 ⇒ 有人往回写了；
///    少一份 ⇒ 那一份改过来了 ⇒ **连那行 `pub use` 一起删**，别留成用不上的豁免）；
/// ② `sftp_pool.rs` 里**没有**第二份定义（只许有那一行 `pub use`）。
///
/// ⚠ 本条会在「那一份改过来」的那天变红 —— **那是设计如此**：
/// 它要求删掉转出住址这件事和改调用点同一拍做，而不是把一个死别名留在盘上。
#[test]
fn the_old_address_is_down_to_its_last_consumer() {
    let root = repo_root();
    // 拼出来的针（同上）：`sftp_pool::is_protected_claude_data_path` 这个**旧写法**。
    let old = format!("sftp_{}::is_protected_claude_data_path", "pool");
    let mut holdouts: Vec<String> = Vec::new();
    let files = guard_core::scan_tree!(&root.join("src/bridge/src"), &["rs"]);
    assert!(
        files.len() >= 100,
        "`src/bridge/src` 只采到 {} 份 `.rs`（2026-09-21 现打 112）—— 扫描面坏了",
        files.len()
    );
    for (path, src) in &files {
        let prod = guard_core::production_code(src);
        if prod.contains(old.as_str()) {
            holdouts.push(
                path.strip_prefix(&root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    holdouts.sort();
    let mut declared: Vec<String> = OLD_ADDRESS_HOLDOUTS
        .iter()
        .map(|(f, _)| f.to_string())
        .collect();
    declared.sort();
    assert_eq!(
        declared,
        holdouts,
        "还在走旧住址（`{old}`）的文件与登记对不上。\n  \
         登记了而盘上没有：{:?} —— 🔴 **那一份改过来了 ⇒ 同一拍把 `sftp_pool.rs` 里\n  \
         那行 `pub use` 与这里这一行一起删掉**（棘轮只许往下走，不许留死别名）。\n  \
         盘上有而没登记：{:?} —— 有人往回写了旧住址；新代码一律走 \n  \
         `claude_data_fence::is_protected_claude_data_path`。",
        declared
            .iter()
            .filter(|d| !holdouts.contains(d))
            .collect::<Vec<_>>(),
        holdouts
            .iter()
            .filter(|h| !declared.contains(h))
            .collect::<Vec<_>>()
    );
    // ② 池子里只许有那行 `pub use`，不许有第二份定义。
    let pool = std::fs::read_to_string(root.join("src/bridge/src/sftp_pool.rs"))
        .expect("sftp_pool.rs 读不到");
    let pool_prod = guard_core::production_code(&pool);
    let def = format!("fn is_protected_claude_{}_path", "data");
    assert!(
        !pool_prod.contains(&def),
        "`sftp_pool.rs` 里又长出了一份 `{def}` —— 那是第二个家。\
         判定的家是 `{FENCE_HOME}`，池子那边只许有一行 `pub use`。"
    );
    let reexport = format!(
        "pub use crate::claude_data_fence::is_protected_claude_{}_path",
        "data"
    );
    assert!(
        pool_prod.contains(&reexport),
        "`sftp_pool.rs` 里那行转出住址不见了，而 {} 还在用旧写法 —— \
         那边此刻编译不过。要么把它一起改掉（连本条这一行删掉），要么把 `pub use` 留着。",
        holdouts.join(" / ")
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  五、这道围栏**不**覆盖哪几类路径 —— 读数，不是待办
// ════════════════════════════════════════════════════════════════════════════

/// `(一条真实形状的路径, 它是什么, 为什么今天没被挡)`。
///
/// 🔴 **这张表是读数，不是 TODO。** 往里加一类、或者把某一类改成「已挡」，
/// 都是**扩围栏的射程** —— 那是另一件事，要用户拍
/// （`设计/99 §2 Q2` 那一格裁的只有「拆成独立一族」这件事）。
///
/// # 它为什么必须是一条会红的判据，而不是一段散文
///
/// 散文里写「围栏不管 X」是**没人守着**的：哪天有人把 X 一起挡了（或者反过来，
/// 以为挡着其实没挡），散文照旧那么写。
/// 本条要求每一行**今天真的是放行的** ⇒ 谁动了射程，谁当场收到点名。
const THE_SHAPES_THIS_FENCE_DOES_NOT_COVER: &[(&str, &str, &str)] = &[
    (
        "/home/u/.claude/projects/-x-proj",
        "**装着 live jsonl 的那个项目目录本身**",
        "判定按 `projects/` 下恰 2 段取，目录本身只有 1 段 ⇒ 放行。\
         🔴 后果不是零：`sftp_delete` 走的是非递归的 `remove_dir`（非空即失败），\
         但 `sftp_rename` 能把整个目录**改走**、`sftp_chmod` 能把它改成不可进入 —— \
         两者都会弄坏那底下正在跑的会话。**只登记，不扩射程。**",
    ),
    (
        "/home/u/.claude/projects/-x-proj/<sid>/subagents/agent-ab12.jsonl",
        "**子 agent 的记录**（`subagent` 模块头注逐字写的那个布局）",
        "它在 `projects/` 下有 4 段 ⇒ 放行。它是 Claude 写的 jsonl，\
         `INVARIANTS §1` 那条只读铁律的字面对象是 `projects/**/*.jsonl` —— \
         **字面上盖得住它，而今天这个结构判定盖不住**。只登记。",
    ),
    (
        "/home/u/.claude/tasks/<sid>/17.json",
        "**Claude CLI 的 task 列表**（`tasks` 模块读的那一族）",
        "既不在 `projects/` 也不在 `sessions/` 下 ⇒ 放行。\
         `INVARIANTS §1` 的对象逐字只有 jsonl 与 pidfile，没点过 tasks ⇒ \
         「要不要管」没人裁定过。只登记。",
    ),
    (
        "/home/u/.claude/settings.json",
        "Claude 自己的设置",
        "**刻意放行** —— `INVARIANTS §1` 的 SS-14 那一条逐字写着写面绝不含 settings，\
         而这道围栏是「不许写 Claude 的**会话数据**」，不是「不许碰 `.claude/` 整棵树」。\
         这一行不是缺口，是边界。",
    ),
];

/// 🔴 上面那张「不覆盖」表**每一行今天真的放行** —— 谁动了射程，当场点名。
#[test]
fn every_uncovered_shape_is_still_uncovered_today() {
    use crate::claude_data_fence::is_protected_claude_data_path;
    assert!(
        THE_SHAPES_THIS_FENCE_DOES_NOT_COVER.len() >= 4,
        "「不覆盖」那张表只剩 {} 行 —— 本条此刻在空转（2026-09-21 现打 4 行）",
        THE_SHAPES_THIS_FENCE_DOES_NOT_COVER.len()
    );
    let now_covered: Vec<&(&str, &str, &str)> = THE_SHAPES_THIS_FENCE_DOES_NOT_COVER
        .iter()
        .filter(|(p, ..)| is_protected_claude_data_path(p))
        .collect();
    assert!(
        now_covered.is_empty(),
        "这几类路径**今天被挡住了**，而登记表说它们没被挡：{now_covered:?}\n\n\
         两种可能，处置完全不同：\n\
         ① 有人扩了围栏的射程 ⇒ 🔴 那是一件**要用户拍**的事（`设计/99 §2 Q2` 只裁了「拆」），\n\
            而且扩射程的代价是误伤面：F47 面板的正题是「浏览/传输任意用户文件」；\n\
         ② 这一行的样本路径写歪了，其实压根不是那一类 ⇒ 改样本。\n\
         ⚠ **不要靠删掉这一行让它变绿** —— 那等于把一条已登记的读数擦掉。",
    );
    for (p, what, why) in THE_SHAPES_THIS_FENCE_DOES_NOT_COVER {
        assert!(
            why.chars().count() > 20,
            "`{p}`（{what}）那一行没写清为什么没被挡，像是占位：「{why}」"
        );
    }
}
