//! **`files-read` 能力声明的护栏** —— 三条硬边界的机器形态。
//!
//! # `K-G6` `KG62`：性质与人群，两行逐字（各自只许有一句）
//!
//! - **它守的性质是**：这一族**声明出去的东西**与**实现真的在做的事**对得上 —— 副作用档（只读）· 能力集在每个 target 上相等 · 保鲜机制逐 target 如实分开声明 · 重走周期可查询。
//! - **它扫的人群是**：`src/backend/files/` 下递归全部 `.rs` 的**整份源码文本**（含注释，fail-closed，与 `readonly_guard` 默认层同一条口径），外加 `files::CAPABILITIES` / `files::FRESHNESS` 两张声明表本身。
//!
//! # 🔴 为什么是这一份，而不是靠 `readonly_guard` 兜底
//!
//! 边界① 逐字：
//!
//! > 这一条要进 `CAPABILITIES` 的语义，**不是注释**：将来谁往这一族里加一个写操作，
//! > **能力声明这一侧就该先红**，而不是靠 `readonly_guard` 兜底 ——
//! > 两道都要，但声明那道更早。
//!
//! ⇒ 本份就是「声明那道」。它与 `readonly_guard` 的关系写清楚，免得被当成重复：
//!
//! | | `readonly_guard` | 本份 |
//! |---|---|---|
//! | 人群 | **整个后端** crate 的生产段 | **只有这一族** |
//! | 判的是 | 「有没有写盘的形状」——一条全局禁令 | 「**声明**的副作用档 == 从实现**派生**的副作用档」 |
//! | 红的时候在说 | 「你违反了那条铁律」 | 「你的**声明是假的**」 |
//! | 加一条写操作时 | 红（默认层当场） | 红（本份的相等断言），**而且它指的是声明那一侧** |
//!
//! 两道都会红，这是刻意的（「两道都要」）。
//!
//! # ⚠ 它**买不到**什么（逐条，别读宽）
//!
//! 1. **派生用的是 needle 表，是黑名单不是白名单。** 列不全 ——
//!    换一个命名空间的写法（`std::os::…` 那一族）本份看不见。
//!    那一半由 `readonly_guard` 的**白名单**层接着（它按动词收人、默认拒绝），
//!    而本份买的是「**声明这一侧先出声**」，不是完备性。
//! 2. **它不判那几行 `what` / `gap` 说得对不对**（同 `spawn_registry` 那条已登记的边界）：
//!    钉的是「说得出来」＋「说的不是三份一样的话」，不是「真的想过」。
//! 3. **`targets` 那一栏是声明，不是编译结果。** 「这四个 target 上都编得过」的真判据是
//!    门禁的 `muslbuild` / `winchk-backend` 那两格，本份钉不到。
//!    ⚠ 而 macOS 那一格**两道都没有**（本仓没有 darwin 的编译门禁），如实登记。

use super::*;

use crate::files::index::tests::resident_lock;

/// 「这一族有哪些」那张表里的名字，**逐字抄**。
///
/// 🔴 抄一份在这里是刻意的：它与 [`CAPABILITIES`] 构成**两向**对拍 ——
/// 一向治「设计里有而实现没声明」（能力漏了），一向治「实现声明了而设计里没有」
/// （偷偷长出一条没人裁过的能力）。只判一向，另一向那种失效永远逃得掉。
///
/// 🔴 **这张表跟着设计走，不是反过来**：那一节
/// 裁出第五、第六条（`files.index.rebuild` / `files.browse`）之后，本表**先改**、
/// 本条判据跟着绿 —— 那一节就是本条的源头。
/// ⚠ **不许**为了让某一侧变绿而从这张表里摘一个名字：摘掉就等于宣布
/// 「设计里从来没有那条能力」，而那正是本条两向对拍要挡的另一向。
///
/// ⚠末尾两条（`files.home` · `files.read.text`）的出处是
/// （窗口换走通道的那两问），** 那张表还没跟上**。
const REGISTERED: &[&str] = &[
    "files.browse",
    "files.find",
    // 要求：「文件管理器做按内容搜」；那张表同样还没跟上。
    "files.grep",
    "files.home",
    "files.index.rebuild",
    "files.index.status",
    "files.ls",
    // 出处用户 09-27「经那台后端链路按字节寻址分块读回」。
    "files.read.chunk",
    "files.read.text",
    // 要求：「算目录大小」；那张表同样还没跟上。
    "files.size",
    "files.stat",
];

/// 「改动盘上东西」的动词 —— **派生副作用档用的 needle**。
///
/// ⚠ 它不是登记表，是一张 needle 表（`scanning_guard_registry` 那边的
/// `NOT_A_REGISTRY_TABLE` 把这两类分开过）。
///
/// 🔴 **运行时拼**：直接写成字面量的话，本表自己就在本族目录之外，本来不会自伤；
/// 但同一批字面量也在 `readonly_guard` 的禁词表上，而那条判据**连注释一起扫** ——
/// 把它们原样写进任何一份住 `src/backend/` 的文件里都会当场红。
/// 本文件住 `tests/backend/`（在那条判据的人群之外），所以这里可以写字面量；
/// **拼**的理由只有一个：让「为什么这几个词危险」有地方写，而不是留一排裸串。
fn write_verbs() -> Vec<String> {
    [
        ("fs::", "write"),           // 覆盖写既有文件
        ("fs::", "create_dir"),      // 建目录
        ("fs::", "remove_file"),     // 删
        ("fs::", "remove_dir"),      // 删目录
        ("fs::", "rename"),          // 改名
        ("fs::", "copy"),            // 复制
        ("fs::", "hard_link"),       // 建硬链
        ("fs::", "soft_link"),       // 建软链（已废弃的那个名字）
        ("fs::", "symlink"),         // 建软链（今天的名字）
        ("fs::", "set_permissions"), // 事后改权限
        ("File::", "create"),        // 无 O_EXCL 的建（已存在会被截断）
        ("File::", "options"),       // 拿一个可写句柄
        ("Open", "Options"),         // 同上
        (".", "write_all("),         // 往一个句柄里写
        (".", "set_len("),           // 截断 / 扩长
        ("truncate", "(true)"),      // 截断
        ("append", "(true)"),        // 追加
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect()
}

/// 从一段源码**派生**出它的副作用档。
///
/// 🔴 **不剥注释，fail-closed** —— 与 `readonly_guard` 默认层逐字同一条口径。
/// 代价如实写：往本族的文档里写一个写盘动词的名字会**误红**。
/// 那正是 `readonly_guard` 自己踩过四次、并因此在 `control/files_write.rs` 头注里
/// 留了一整段「要改就改措辞，别去放宽护栏」的那件事。**本份照抄那条处置。**
fn derive_effect(src: &str) -> Effect {
    if write_verbs().iter().any(|v| src.contains(v.as_str())) {
        Effect::TouchesDisk
    } else {
        Effect::ReadsOnly
    }
}

/// 本族目录下的 `(文件名, 整份源码)`。
///
/// 走 `guard_core::scan_tree!`。
///
/// ⚠ 先前这里写着「两重保险」，而**第一重今天不生效**：
/// 宏自称按 `file!()` 摘除调用者自己那一份，判据由 `#[path]` 挂载之后 `file!()`
/// 是折返路径 ⇒ 后缀比不命中。承重的只剩第二重：本文件住 `tests/backend/files/`、
/// **根本不在被扫的那棵树里**。
fn family_sources() -> Vec<(String, String)> {
    let dir = crate::guard_support::src_root().join("files");
    let mut out: Vec<(String, String)> = guard_core::scan_tree!(&dir, &["rs"])
        .into_iter()
        .map(|(p, src)| {
            (
                p.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string(),
                src,
            )
        })
        .collect();
    out.sort();
    // ★ 反空真：扫不到东西时下面每一条都会「对空集全称成立」，恒绿。
    assert!(
        out.len() >= 4,
        "本族目录下只扫到 {} 份 `.rs` —— 采集坏了（本件落地时是 4 份：\
         mod / index / raw / browse_watch）。\n\
         🔴 「扫不到」与「都干净」在断言上一模一样，所以这一条必须在。",
        out.len()
    );
    let bytes: usize = out.iter().map(|(_, s)| s.len()).sum();
    assert!(
        bytes >= 20_000,
        "本族源码只扫到 {bytes} 字节 —— 采集到的不是真代码"
    );
    out
}

// ══════════════════════ 边界① 副作用档 ══════════════════════

/// 🔴🔴 **正题**：声明的副作用档 == 从实现派生出来的副作用档。
///
/// 这就是边界① 要的那条「声明那一侧先红」。
#[test]
fn the_declared_effect_equals_the_effect_derived_from_the_implementation() {
    let sources = family_sources();
    let mut declared: Vec<(&str, Effect)> = Vec::new();
    let mut derived: Vec<(&str, Effect)> = Vec::new();
    for cap in CAPABILITIES {
        declared.push((cap.name, cap.effect));
        let mut eff = Effect::ReadsOnly;
        let mut seen = 0usize;
        for f in cap.impl_files {
            let (_, src) = sources
                .iter()
                .find(|(name, _)| name == f)
                .unwrap_or_else(|| {
                    panic!(
                        "能力 `{}` 声明它的实现住 `{f}`，而本族目录下没有这份文件。\n\
                         ⇒ 登记挂空号：派生就会在**那一份**上瞎掉，而瞎掉与干净长得一样。",
                        cap.name
                    )
                });
            seen += 1;
            if derive_effect(src) == Effect::TouchesDisk {
                eff = Effect::TouchesDisk;
            }
        }
        assert_eq!(
            seen,
            cap.impl_files.len(),
            "能力 `{}` 的实现清单有条目没被读到",
            cap.name
        );
        derived.push((cap.name, eff));
    }
    assert_eq!(
        derived, declared,
        "🔴 **声明与实现对不上。**\n\
         左边是从 `src/backend/files/` 的源码里**派生**出来的副作用档，\n\
         右边是 `files::CAPABILITIES` 里**声明**的那一栏。\n\n\
边界① 逐字：「整族一个字节都不写……\n\
         将来谁往这一族里加一个写操作，**能力声明这一侧就该先红**，\n\
         而不是靠 `readonly_guard` 兜底 —— 两道都要，但声明那道更早。」\n\n\
         ⇒ 两条出路，**没有第三条**：\n\
         ① 把那处写操作去掉（这一族的存在理由是搜索，而搜索纯读）；\n\
         ② 真要给这一族写能力 ⇒ 回重裁，\n\
            并且同轮去 `readonly_guard::WRITE_WHITELIST_MODULES` 上签字。\n\
         🚫 **不许**把这里的 `effect` 改成 `TouchesDisk` 来对付这条红 ——\n\
            那是在把一条「实现越界了」的红改写成「我本来就打算越界」，\n\
            而 `readonly_guard` 的默认层会在下一格接着红。"
    );
}

/// ★★ **反空真**：上面那条相等，两侧必须**可能不相等**。
///
/// [`Effect`] 要是只有一个成员，`derived == declared` 就是 `x == x` —— 恒真。
/// 本仓对这一形的说法逐字：「恒等两侧同源会恒真」。
#[test]
fn the_effect_enum_can_actually_express_a_write() {
    assert_ne!(
        Effect::ReadsOnly,
        Effect::TouchesDisk,
        "副作用档只有一个成员 —— 那条相等断言退化成恒真"
    );
    assert_eq!(
        derive_effect("let _ = 1;"),
        Effect::ReadsOnly,
        "一段什么都没干的代码被派生成了「会动盘」"
    );
}

/// **阳性对照的语料** —— 一排**独立写出来**的、真的会动盘的代码片段。
///
/// 🔴🔴 **它为什么必须独立写，而不是从 [`write_verbs`] 拼出来** —— 这是本轮死值验
/// 当场逮到的一个真缺陷，记在这里别再犯：
///
/// 第一版的阳性对照是 `format!("let _ = {needle}")` —— **样本由针自己拼出来的**。
/// 于是把针拼错（`fs::rename` → `fs::rneame`）之后，那条判据**照样绿**：
/// 它拿错针去搜一段含着同一个错针的样本，当然搜得到。
/// 本仓对这一形的说法逐字：**「恒等两侧同源会恒真」**。
///
/// ⇒ 改成两侧异源：**语料在这里逐条手写**，针在 [`write_verbs`] 里。
/// 针拼错 ⇒ 对应那条语料没人认得 ⇒ 当场红（本轮实打：刀 7 从 GREEN 变 RED）。
const WRITE_SAMPLES: &[&str] = &[
    "std::fs::write(&p, b\"x\").unwrap();",
    "std::fs::create_dir(&p).unwrap();",
    "std::fs::create_dir_all(&p).unwrap();",
    "std::fs::remove_file(&p).unwrap();",
    "std::fs::remove_dir_all(&p).unwrap();",
    "std::fs::rename(&a, &b).unwrap();",
    "std::fs::copy(&a, &b).unwrap();",
    "std::fs::hard_link(&a, &b).unwrap();",
    "std::fs::soft_link(&a, &b).unwrap();",
    "std::fs::symlink(&a, &b).unwrap();",
    "std::fs::set_permissions(&p, perm).unwrap();",
    "let f = std::fs::File::create(&p).unwrap();",
    "let f = std::fs::File::options().write(true).open(&p).unwrap();",
    "let f = std::fs::OpenOptions::new().read(true).open(&p).unwrap();",
    "f.write_all(b\"x\").unwrap();",
    "f.set_len(0).unwrap();",
    "let o = OpenOptions::new().truncate(true);",
    "let o = OpenOptions::new().append(true);",
    "let o = OpenOptions::new().create(true);",
];

/// **阴性对照的语料** —— 一排本族真的在用、而且真的只读的写法。
///
/// 🔴 **为什么这一份是函数而不是 `const`，理由值钱，别顺手改回去**：
/// 头一版写成了一排字面量，其中一条逐字是「列一个目录」那个调用 ——
/// 而 `scanning_guard_registry::RAW_WALKS` 的第一根针恰好就是它（带左括号那一形）。
/// 结果：**本文件被那条元判据判成「在测试段里裸遍历目录」**，门禁当场红。
/// 它没有瞎报 —— 它的针真的在本文件里出现了；只是出现的位置是**一份阴性对照语料**，
/// 不是一次真的遍历。
/// ⇒ 处置照本仓已有的那一条（`no_timer_guard::periodic_wake_patterns` 的头注逐字：
///   「判据**运行时拼**：直接写字面量的话，本文件自己就会被下面的扫描命中」）：
///   **拼**出来，让那个字面量不在源码里。
/// 🚫 **不许**改成「往 `PENDING` 里加一行」—— 那张清单逐字「只许变短」。
fn read_samples() -> Vec<String> {
    [
        ("let rd = std::fs::read", "_dir(&dir)?;"),
        ("let md = std::fs::meta", "data(&p)?;"),
        ("let b = p.as_os_str().as_encoded", "_bytes();"),
        ("let t = e.file_type().map(|t| t.is", "_dir());"),
        ("let s = std::str::from", "_utf8(&bytes);"),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect()
}

/// ★★ **阳性对照**：每一段真的会动盘的代码，派生出来必须是「会动盘」。
///
/// 🔴 不钉这一格会怎样：`write_verbs()` 里拼错一个字之后，上面那条正题**照样绿** ——
/// 而它绿的原因从「实现是干净的」悄悄变成「针不认字」。
/// 本仓对这一族的说法逐字：**默认结局是恒绿**，五次里没有一次是被判据变红发现的。
#[test]
fn every_write_verb_needle_really_bites_on_an_independently_written_sample() {
    assert_eq!(
        write_verbs().len(),
        17,
        "needle 表的条数变了 —— 变了就回来把这个数改对，并说清加/减的是哪一条"
    );
    for s in WRITE_SAMPLES {
        assert_eq!(
            derive_effect(s),
            Effect::TouchesDisk,
            "这一段真的会动盘，而派生说它只读：\n  {s}\n\
             ⇒ needle 表里少了它那一条，或者对应那根针拼错了。\n\
             🔴 别把这条红改成「把样本删掉」—— 那是在把一个瞎掉的针藏起来。"
        );
    }
    // ★ 反向那半：needle 表里每一根针，都得**至少有一条**语料在行使它。
    //   没人行使的针 = 一根从来没被验过的针，它明天拼错了也不会有人知道。
    let unexercised: Vec<String> = write_verbs()
        .into_iter()
        .filter(|v| !WRITE_SAMPLES.iter().any(|s| s.contains(v.as_str())))
        .collect();
    assert_eq!(
        unexercised,
        Vec::<String>::new(),
        "这几根针没有任何一条阳性语料在行使它们 —— 它们没被验过"
    );
}

/// ★★ **阴性对照**：本族真的在用的那几种只读写法，不许被派生成「会动盘」。
///
/// 没有这一格的话，把 `derive_effect` 写成「恒返回 `TouchesDisk`」也能让
/// 上面那条阳性对照全绿，而正题会当场大红一片 —— 人的第一反应是去删正题。
#[test]
fn the_read_only_shapes_this_family_really_uses_are_not_flagged() {
    for s in read_samples() {
        assert_eq!(
            derive_effect(&s),
            Effect::ReadsOnly,
            "这一段是只读的，却被派生成了「会动盘」：\n  {s}\n\
             ⇒ 有一根针太宽了。宽到误红的针会被人绕开，那时它就不再守着什么。"
        );
    }
}

/// 🔴 **人群闭合**：四张 `impl_files` 的并集 == 本族目录下现打的全部 `.rs`。
///
/// 它治的是上面那条正题盖不到的那一形：**新加一份文件、不往任何一条能力的
/// 实现清单里登记** ⇒ 派生读不到它 ⇒ 往那一份里写盘，正题一声不吭。
#[test]
fn the_impl_file_tables_partition_the_family_directory() {
    let on_tree: std::collections::BTreeSet<String> =
        family_sources().into_iter().map(|(name, _)| name).collect();
    let declared: std::collections::BTreeSet<String> = CAPABILITIES
        .iter()
        .flat_map(|c| c.impl_files.iter().map(|f| f.to_string()))
        .collect();
    assert_eq!(
        declared, on_tree,
        "本族的实现清单与目录里现打的文件对不上。\n\
         盘上有而清单没有 ⇒ 🔴 **那一份不受派生管** —— 往它里面写盘，上面那条正题不会红。\n\
         清单有而盘上没有 ⇒ 登记挂空号。\n\
         ⚠ 写成**集合相等**而不是「份数相等」：份数相等在「搬走一份、又冒出一份」上是瞎的。"
    );
}

/// 这一族今天**一条写能力都没声明**。
#[test]
fn not_one_capability_in_this_family_declares_a_write() {
    let effects: Vec<Effect> = CAPABILITIES.iter().map(|c| c.effect).collect();
    let all_read: Vec<Effect> = CAPABILITIES.iter().map(|_| Effect::ReadsOnly).collect();
    assert_eq!(
        effects, all_read,
        "有能力声明了写副作用。这一族的存在理由是**搜索**（只有那一行），\n\
         写面住后端文件管理的另一个模块（`control/files_write.rs`），\n\
         不在这一族 —— 这一族纯读。"
    );
}

// ══════════════════════ 声明 ↔ 设计 ↔ 分派，三向 ══════════════════════

#[test]
fn the_capability_names_match_the_design_registry_in_both_directions() {
    let declared: std::collections::BTreeSet<&str> = capability_names().into_iter().collect();
    let designed: std::collections::BTreeSet<&str> = REGISTERED.iter().copied().collect();
    assert_eq!(
        declared, designed,
        "能力名与那张表对不上。\n\
         设计有而实现没声明 ⇒ 能力漏了；实现声明了而设计没有 ⇒ 长出了一条没人裁过的能力。"
    );
}

/// 🔴 **表里有、分派没有** 这一形必须红。
///
/// 本仓对它有一次真 bug 的记录（`p1t-removal-cause`：一条命令实现完整、
/// `match` 漏列它 ⇒ 回 `unknown argument`，而界面真的在发那条命令）。
#[test]
fn every_declared_capability_is_reachable_through_the_single_entry_point() {
    let _lock = resident_lock();
    let mut unreachable: Vec<&str> = Vec::new();
    for name in capability_names() {
        // 刻意不给参数：我们要分的是「这条能力压根没接线」与「参数不对」。
        if let Err(("unknown_capability", _)) = answer(name, &serde_json::json!({})) {
            unreachable.push(name);
        }
    }
    assert_eq!(
        unreachable,
        Vec::<&str>::new(),
        "这几条能力声明了却没接进那个唯一入口 —— 调用方会拿到「不是这一族的能力」"
    );
    // 反向那半：没声明的名字必须**被拒**（否则这条判据对一切都绿）。
    assert!(
        matches!(
            answer("files.delete", &serde_json::json!({})),
            Err(("unknown_capability", _))
        ),
        "一个没声明的能力名被接受了 —— 那上面那一半就不是在证明什么"
    );
}

/// 每条能力的契约面（参数 / 字段 / 错误码）写成数据之后，**不许是空壳**。
#[test]
fn every_capability_states_its_contract_surface() {
    for cap in CAPABILITIES {
        assert!(
            cap.purpose.chars().count() >= 8,
            "能力 `{}` 的 `purpose` 太短 —— 写得出来才登记",
            cap.name
        );
        assert!(
            !cap.fields.is_empty(),
            "能力 `{}` 一个出方向字段都没有 —— 那它答什么？",
            cap.name
        );
        let mut sorted = cap.fields.to_vec();
        sorted.sort_unstable();
        assert_eq!(
            cap.fields.to_vec(),
            sorted,
            "能力 `{}` 的字段表没排序 —— 排序是为了让 diff 读得出「加了哪一个」",
            cap.name
        );
        let mut args = cap.args.to_vec();
        args.sort_unstable();
        assert_eq!(
            cap.args.to_vec(),
            args,
            "能力 `{}` 的参数表没排序",
            cap.name
        );
    }
}

/// 出方向字段表与**真的回出去的那个 JSON** 对拍。
///
/// ⚠ 用一个手写清单去证明另一个手写清单是没有意义的（`inbound::CommandSpec::fields`
/// 那段头注逐字）—— 所以这里拿**真的调用一次**的输出去比。
#[test]
fn the_status_fields_match_what_the_call_really_returns() {
    let _lock = resident_lock();
    let v = answer("files.index.status", &serde_json::json!({})).expect("status 不该失败");
    let got: std::collections::BTreeSet<String> = v
        .as_object()
        .expect("status 回的该是一个对象")
        .keys()
        .cloned()
        .collect();
    let declared: std::collections::BTreeSet<String> = CAPABILITIES
        .iter()
        .find(|c| c.name == "files.index.status")
        .expect("这条能力必须在表里")
        .fields
        .iter()
        .map(|f| f.to_string())
        .collect();
    assert_eq!(
        got, declared,
        "`files.index.status` 真的回出去的字段与声明的那张表对不上"
    );
}

#[test]
fn the_find_fields_match_what_the_call_really_returns() {
    let _lock = resident_lock();
    let v = answer(
        "files.find",
        &serde_json::json!({"query": "nothing-matches-this"}),
    )
    .expect("find 不该失败");
    let got: std::collections::BTreeSet<String> = v
        .as_object()
        .expect("find 回的该是一个对象")
        .keys()
        .cloned()
        .collect();
    let declared: std::collections::BTreeSet<String> = CAPABILITIES
        .iter()
        .find(|c| c.name == "files.find")
        .expect("这条能力必须在表里")
        .fields
        .iter()
        .map(|f| f.to_string())
        .collect();
    assert_eq!(
        got, declared,
        "`files.find` 真的回出去的字段与声明的那张表对不上"
    );
}

/// `files.index.rebuild` 的出方向字段与**真的回出去的那个 JSON** 对拍，
/// 而且顺手把「条目数」钉成一条**相等**断言（夹具按构造知道自己有多少条）。
///
/// ⚠ 同族那两条（`status` / `find`）为什么也是这么写的：用一个手写清单去证明另一个
/// 手写清单没有意义（`inbound::CommandSpec::fields` 那段头注逐字）。
#[test]
fn the_index_rebuild_fields_match_what_the_call_really_returns() {
    let _lock = resident_lock();
    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    let fx = crate::files::index::tests::make_tree("rebuild-fields", 2, 3, 0);
    let v = answer(
        "files.index.rebuild",
        &serde_json::json!({"path": fx.root.to_str().expect("夹具路径是 ASCII")}),
    )
    .expect("rebuild 不该失败");
    let got: std::collections::BTreeSet<String> = v
        .as_object()
        .expect("rebuild 回的该是一个对象")
        .keys()
        .cloned()
        .collect();
    let declared: std::collections::BTreeSet<String> = CAPABILITIES
        .iter()
        .find(|c| c.name == "files.index.rebuild")
        .expect("这条能力必须在表里")
        .fields
        .iter()
        .map(|f| f.to_string())
        .collect();
    assert_eq!(
        got, declared,
        "`files.index.rebuild` 真的回出去的字段与声明的那张表对不上"
    );
    // ★ 主锚是**相等**，不是「> 0」：地板在「少走了一半」这个方向上是瞎的。
    assert_eq!(
        v.get("entries").and_then(serde_json::Value::as_u64),
        Some(fx.entries as u64),
        "走出来的条目数与夹具**按构造**造的条数不等 —— 那一趟没走完整棵树"
    );
    // ★ 而且它真的把常驻那一份换上去了（不然这条命令只是个回参好看的空转）。
    assert_eq!(
        index::status().entries,
        fx.entries,
        "`rebuild` 回参说走了这么多条，而常驻那一份不是这个数 —— 它没换上去"
    );
}

/// 🔴🔴 **根读不进去 ⇒ 拒，而且常驻那一份一个字节不动。**
///
/// # 它治的那一形（现打逼出来的，不是假想）
///
/// `index::build` 对一个打不开的根**不会失败**：只把 `unreadable_dirs` 加一，
/// 然后交一份**空快照**；而 `index::rebuild_once` 会把常驻那一份**整份换掉**。
/// ⇒ 调用方路径打错一个字母，手上那份好索引就被一份空的顶掉，
/// 而回参看起来像成功（`entries: 0` 与「这台机器上真的没文件」同形）。
///
/// ⇒ 主锚写成**相等**：被拒之前的条目数 == 被拒之后的条目数。
/// 「回了个错」这一半是必要不充分的 —— 回错的同时把索引清了，本仓就又多一次静默缩水。
#[test]
fn a_rebuild_on_an_unreadable_root_is_refused_without_touching_the_resident_index() {
    let _lock = resident_lock();
    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    let fx = crate::files::index::tests::make_tree("rebuild-keep", 2, 2, 0);
    answer(
        "files.index.rebuild",
        &serde_json::json!({"path": fx.root.to_str().expect("夹具路径是 ASCII")}),
    )
    .expect("先建一份好的");
    let before = index::status().entries;
    assert_eq!(before, fx.entries, "起跑状态就不对，下面那条比不了");

    let nowhere = std::env::temp_dir().join(format!("ccm-24f-noroot-{}", std::process::id()));
    std::fs::remove_dir_all(&nowhere).ok();
    let got = answer(
        "files.index.rebuild",
        &serde_json::json!({"path": nowhere.to_str().expect("ASCII")}),
    );
    assert!(
        matches!(got, Err(("unreadable", _))),
        "对一个打不开的根没有回 `unreadable`，回的是 {got:?}"
    );
    assert_eq!(
        index::status().entries,
        before,
        "🔴 被拒的那一趟把常驻索引换掉了 —— 一次打错的路径不许把好索引清成空的"
    );
}

/// `files.browse` 的出方向字段与真的回出去的那个 JSON 对拍。
///
/// ⚠ 顺手钉住**空数组是合法的**那一格：它的语义是「现在什么都没在看」⇒ 全卸。
/// 「少了 `dirs`」是另一件事（`bad_args`），由下面那条错误码判据行使。
#[test]
fn the_browse_fields_match_what_the_call_really_returns() {
    let _lock = resident_lock();
    let fx = crate::files::index::tests::make_tree("browse-fields", 2, 1, 0);
    let one = fx.root.join("d0000");
    let v = answer(
        "files.browse",
        &serde_json::json!({"dirs": [one.to_str().expect("夹具路径是 ASCII")]}),
    )
    .expect("browse 不该失败");
    let got: std::collections::BTreeSet<String> = v
        .as_object()
        .expect("browse 回的该是一个对象")
        .keys()
        .cloned()
        .collect();
    let declared: std::collections::BTreeSet<String> = CAPABILITIES
        .iter()
        .find(|c| c.name == "files.browse")
        .expect("这条能力必须在表里")
        .fields
        .iter()
        .map(|f| f.to_string())
        .collect();
    assert_eq!(
        got, declared,
        "`files.browse` 真的回出去的字段与声明的那张表对不上"
    );
    // ★ 相等断言，不是地板：一个目录进去 ⇒ 挂上 1 个、卸掉 0 个、拒掉 0 个。
    assert_eq!(
        (
            v.get("added").and_then(serde_json::Value::as_u64),
            v.get("removed").and_then(serde_json::Value::as_u64),
            v.get("rejected").and_then(serde_json::Value::as_u64),
        ),
        (Some(1), Some(0), Some(0)),
        "一个目录的那一趟差分算错了"
    );
    assert_eq!(
        v.get("browse_watch_cap")
            .and_then(serde_json::Value::as_u64),
        Some(crate::files::browse_watch::MAX_BROWSE_WATCHES as u64),
        "回参里那个上限不是后端真用的那个 —— 调用方按它决定该少送几个"
    );
    // ★ 空数组：**全卸**，而且它不是「少了 `dirs`」。
    let v2 = answer("files.browse", &serde_json::json!({"dirs": []})).expect("空数组是合法的");
    assert_eq!(
        (
            v2.get("added").and_then(serde_json::Value::as_u64),
            v2.get("removed").and_then(serde_json::Value::as_u64),
        ),
        (Some(0), Some(1)),
        "空数组没有把上一趟那一个卸掉 —— 那 watch 会一直留着，而用户已经不看它了"
    );
}

/// 新那两条自己声明的错误码，**都得真的出得来**（幽灵码检查）。
///
/// ⚠ `unreadable` 那一档由上面那条
/// [`a_rebuild_on_an_unreadable_root_is_refused_without_touching_the_resident_index`]
/// 行使（那条同时钉着「不许清索引」），这里只补形状类的那几个。
#[test]
fn the_new_two_capabilities_declared_codes_are_not_ghosts() {
    let _lock = resident_lock();
    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    for (cap, args, want) in [
        // `path` 那一族与同族其余两条逐字同一条路（`path_arg`）。
        (
            "files.index.rebuild",
            serde_json::json!({"path": ""}),
            "bad_path",
        ),
        (
            "files.index.rebuild",
            serde_json::json!({"path": 7}),
            "bad_path",
        ),
        // `dirs` **自己**的形状不对 ⇒ `bad_args`（少了它 / 不是数组）。
        ("files.browse", serde_json::json!({}), "bad_args"),
        (
            "files.browse",
            serde_json::json!({"dirs": "/tmp"}),
            "bad_args",
        ),
        // 数组里**某一项**不是路径 ⇒ `bad_path`。两个码刻意分得开。
        ("files.browse", serde_json::json!({"dirs": [7]}), "bad_path"),
        (
            "files.browse",
            serde_json::json!({"dirs": [{"b16": "zz"}]}),
            "bad_path",
        ),
        (
            "files.browse",
            serde_json::json!({"dirs": [""]}),
            "bad_path",
        ),
    ] {
        let got = answer(cap, &args);
        assert!(
            matches!(got, Err((c, _)) if c == want),
            "`{cap}` 对 {args:?} 该回 `{want}`，回的是 {got:?}"
        );
        let declared = CAPABILITIES
            .iter()
            .find(|c| c.name == cap)
            .expect("在表里")
            .codes;
        assert!(
            declared.contains(&want),
            "`{cap}` 的错误码表里没有 `{want}` —— 它真的会回这个码"
        );
    }
}

/// 🔴🔴 **`args` 那一侧终于也去对真解析器了**〔`24f` 第三刀补， 登记的那条欠账〕。
///
/// # 它补的是哪一格
///
/// 出方向那一侧早就有实打对拍（[`the_status_fields_match_what_the_call_really_returns`]
/// 那几条 —— **真调一次**，拿回来的键与声明的 `fields` 判相等）。
/// 而**入方向的 `args` 从来没有任何东西拿它去对真解析器** ——
/// 登记过这条欠账，而且它已经出过一次事：
/// `files.ls` 的 `args` 里写着 `ignore_ascii_case`，**而 `answer_ls` 一次都没读它**
///（从 `files.find` 抄过来的鬼影，`24f` 第二刀现打逮到、已摘）。
///
/// 🔴 **这比「少声明一个参数」更坏**：`src/doc/IPC-PROTOCOL.md §10` 是**冻结的线上契约**、
/// 读者在仓外，一个「写了也不起作用」的参数就是在那份文档里**撒谎** ——
/// 而调用方发了它、以为生效了，行为却一个字没变。
///
/// # 怎么判：**差分**，不是「名字出现在源码里」
///
/// 「`answer_ls` 里有没有 `ignore_ascii_case` 这个串」是**源码扫描**，
/// 而那个鬼影恰恰是**被声明、没被读** —— 扫描在它身上是瞎的（表自己就在被扫的树里）。
/// ⇒ 本条判的是**行为**：每个声明的参数都配一对只差**那一个键**的入参，
/// 两趟**真的调进去**，答案必须**不同**。答案相同 = 这个参数没被读 = 声明在撒谎。
///
/// 三条各自独立，缺一条本条就会退化：
/// ① **分区恒等** —— 探针表里每条能力的参数集 **==** 它 `args` 声明的那一套（**两向**）。
///    ⇒ 往声明里加一个参数而不写探针，当场红（那正是鬼影混进来的路）。
/// ② **一次只差一个键** —— 两份入参的键集对称差必须**恰好**是被测的那个参数。
///    ⇒ 不许拿「顺手把 path 也换了」的两趟去冒充「limit 被读了」。
/// ③ **阴性对照** —— 喂一个**没声明**的参数进去，答案必须**相同**。
///    没有它，一把「什么都说不同」的坏尺子照样全绿。这一格用的正是当年那个鬼影
///    （`files.ls` + `ignore_ascii_case`），它今天必须**不起作用**。
///
/// # ⚠ 它买不到什么（如实登记）
///
/// - 只买「**这个参数被读了**」，**不买**「读得对」：语义对不对仍然是各条命令自己的判据。
/// - 只覆盖**今天造得出差分**的参数。将来若有一个参数在任何输入下都不改变可观测答案
///   （纯旁路的开关），本条写不出探针 ⇒ 分区恒等会逼那个人在这里**当面交代**，
///   而不是让它静默进契约文档。**那是刻意的。**
#[test]
fn every_declared_arg_is_really_read_by_the_parser() {
    let _lock = resident_lock();

    // 语料：一棵合成树 ＋ 它的一个子目录（数据源纪律 —— 全合成，不碰真目录）。
    let fx = crate::files::index::tests::make_tree("args-probe", 3, 4, 0);
    let sub = fx.root.join("d0000");
    let p = |x: &std::path::Path| {
        serde_json::Value::String(x.to_str().expect("夹具路径是 ASCII").to_string())
    };

    // `files.find` 那几条要有一份**真的**常驻索引才谈得上差分。
    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    crate::files::index::rebuild_once(&fx.root).expect("本格独占跑，抢不到就是并发保护写错了");

    // `files.read.text` 的两份夹具 —— 放在那棵树**之外**（免得动了树的形状）。
    let text_dir = std::env::temp_dir().join(format!("ccm-f7a-args-{}", std::process::id()));
    std::fs::create_dir_all(&text_dir).expect("建夹具目录");
    let text_a = text_dir.join("a.txt");
    let text_b = text_dir.join("b.txt");
    std::fs::write(&text_a, b"alpha\n").expect("铺 a");
    std::fs::write(&text_b, b"beta\n").expect("铺 b");

    // `seq` / `stream` 那两条要一个先来过更大号的搜索框。
    for st in ["probe-seq", "probe-stream-a"] {
        crate::files::index::ticket(st, 9).expect("登记一个先来的号");
    }
    // 🔴 探针表：`(能力, 被测参数, 甲, 乙)` —— 甲乙只差那一个键，答案必须不同。
    //   ⚠ 顺序承重：`files.browse` 会往 overlay 里加东西、`files.index.rebuild` 会把
    //   常驻那一份整份换掉 ⇒ 两者都排在 `files.find` 之后，免得前一条把后一条的地基抽了。
    let probes: Vec<(&str, &str, serde_json::Value, serde_json::Value)> = vec![
        // `files.size` 的 `path`：整棵 vs 其中一个子目录，数必然不同。
        (
            "files.size",
            "path",
            serde_json::json!({ "path": p(&fx.root) }),
            serde_json::json!({ "path": p(&sub) }),
        ),
        (
            "files.ls",
            "path",
            serde_json::json!({ "path": p(&fx.root) }),
            serde_json::json!({ "path": p(&sub) }),
        ),
        (
            "files.ls",
            "limit",
            serde_json::json!({ "path": p(&fx.root), "limit": 1000 }),
            serde_json::json!({ "path": p(&fx.root), "limit": 1 }),
        ),
        (
            "files.stat",
            "path",
            serde_json::json!({ "path": p(&fx.root) }),
            serde_json::json!({ "path": p(&sub) }),
        ),
        // `files.find`：两侧都给 `under`（不给 ⇒ 家目录，而夹具不在家目录里）。
        (
            "files.find",
            "query",
            serde_json::json!({ "under": p(&fx.root), "query": "f0000" }),
            serde_json::json!({ "under": p(&fx.root), "query": "d0000" }),
        ),
        (
            "files.find",
            "under",
            serde_json::json!({ "under": p(&fx.root), "query": "f0000" }),
            serde_json::json!({ "under": p(&sub), "query": "f0000" }),
        ),
        (
            "files.find",
            "limit",
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "limit": 1000 }),
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "limit": 1 }),
        ),
        (
            "files.find",
            "sort",
            serde_json::json!({ "under": p(&fx.root), "query": "", "sort": "relevance" }),
            serde_json::json!({ "under": p(&fx.root), "query": "", "sort": "name" }),
        ),
        (
            "files.find",
            "desc",
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "desc": false }),
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "desc": true }),
        ),
        (
            "files.find",
            "scope",
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "scope": "under" }),
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "scope": "machine" }),
        ),
        (
            "files.find",
            "offset",
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "offset": 0 }),
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "offset": 1 }),
        ),
        // 号：这个搜索框先来过 9 ⇒ 1 被丢、10 照常。
        (
            "files.find",
            "seq",
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "stream": "probe-seq", "seq": 1 }),
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "stream": "probe-seq", "seq": 10 }),
        ),
        // 搜索框：甲那个先来过 9 ⇒ 号 1 被丢；乙那个是新的 ⇒ 照常。
        (
            "files.find",
            "stream",
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "stream": "probe-stream-a", "seq": 1 }),
            serde_json::json!({ "under": p(&fx.root), "query": "f0000", "stream": "probe-stream-b", "seq": 1 }),
        ),
        (
            "files.browse",
            "dirs",
            serde_json::json!({ "dirs": [] }),
            serde_json::json!({ "dirs": [p(&sub)] }),
        ),
        (
            "files.index.rebuild",
            "path",
            serde_json::json!({ "path": p(&fx.root) }),
            serde_json::json!({ "path": p(&sub) }),
        ),
        // 同族第七条：两份不同内容的文本 · 同一份在两个上限下（一个放得下、一个放不下）。
        (
            "files.read.text",
            "path",
            serde_json::json!({ "path": p(&text_a), "max_bytes": 1000 }),
            serde_json::json!({ "path": p(&text_b), "max_bytes": 1000 }),
        ),
        (
            "files.read.text",
            "max_bytes",
            serde_json::json!({ "path": p(&text_a), "max_bytes": 1000 }),
            serde_json::json!({ "path": p(&text_a), "max_bytes": 1 }),
        ),
        // `files.read.chunk` 的三个参数：各差一个键，读回的字节必然不同。
        (
            "files.read.chunk",
            "path",
            serde_json::json!({ "path": p(&text_a), "offset": 0, "len": 4 }),
            serde_json::json!({ "path": p(&text_b), "offset": 0, "len": 4 }),
        ),
        (
            "files.read.chunk",
            "offset",
            serde_json::json!({ "path": p(&text_a), "offset": 0, "len": 4 }),
            serde_json::json!({ "path": p(&text_a), "offset": 1, "len": 4 }),
        ),
        (
            "files.read.chunk",
            "len",
            serde_json::json!({ "path": p(&text_a), "offset": 0, "len": 4 }),
            serde_json::json!({ "path": p(&text_a), "offset": 0, "len": 2 }),
        ),
        // `files.grep` 的四个参数：那两份文本（`alpha` / `beta`）所在的目录上各差一个键，命中必然不同。
        (
            "files.grep",
            "path",
            serde_json::json!({ "path": p(&text_dir), "needle": "alpha" }),
            serde_json::json!({ "path": p(&fx.root), "needle": "alpha" }),
        ),
        (
            "files.grep",
            "needle",
            serde_json::json!({ "path": p(&text_dir), "needle": "alpha" }),
            serde_json::json!({ "path": p(&text_dir), "needle": "beta" }),
        ),
        (
            "files.grep",
            "ignore_ascii_case",
            serde_json::json!({ "path": p(&text_dir), "needle": "ALPHA", "ignore_ascii_case": true }),
            serde_json::json!({ "path": p(&text_dir), "needle": "ALPHA", "ignore_ascii_case": false }),
        ),
        (
            "files.grep",
            "limit",
            serde_json::json!({ "path": p(&text_dir), "needle": "a", "limit": 1000 }),
            serde_json::json!({ "path": p(&text_dir), "needle": "a", "limit": 1 }),
        ),
    ];

    // ── ① 分区恒等：探针表 ↔ `args` 声明，逐条能力**两向相等** ─────────────
    for cap in CAPABILITIES {
        let probed: std::collections::BTreeSet<&str> = probes
            .iter()
            .filter(|(c, ..)| *c == cap.name)
            .map(|(_, a, ..)| *a)
            .collect();
        let declared: std::collections::BTreeSet<&str> = cap.args.iter().copied().collect();
        assert_eq!(
            probed, declared,
            "\n能力 `{}` 的**入参探针**与它声明的 `args` 对不上。\n\
             声明里有而探针没有 ⇒ 🔴 那个参数**没有任何东西证明它真被读了** —— \n\
             `files.ls` 的 `ignore_ascii_case` 当年就是这么在契约文档里躺了一版。\n\
             探针有而声明没有 ⇒ 探针表腐了（登记表腐烂比没有登记更糟）。",
            cap.name
        );
    }

    // ── ② 逐条：只差一个键 · 答案必须不同 ──────────────────────────────────
    let keys = |v: &serde_json::Value| -> std::collections::BTreeSet<String> {
        v.as_object()
            .expect("入参该是一个对象")
            .keys()
            .cloned()
            .collect()
    };
    let mut checked = 0usize;
    for (cap, arg, a, b) in &probes {
        let (ka, kb) = (keys(a), keys(b));
        let diff: Vec<&String> = ka.symmetric_difference(&kb).collect();
        assert!(
            ka == kb || (diff.len() == 1 && diff[0] == arg),
            "`{cap}` / `{arg}` 那一对入参差的不止那一个键：{diff:?}\n\
             ⇒ 答案不同可能是**别的键**造成的，这一对证不了 `{arg}` 被读了"
        );
        let ra = format!("{:?}", answer(cap, a));
        let rb = format!("{:?}", answer(cap, b));
        assert_ne!(
            ra, rb,
            "\n🔴 `{cap}` 的参数 `{arg}` **改了也不起作用** —— 两趟只差这一个键，答案却一模一样。\n\
             那就是「声明了、解析器没读」，而 `src/doc/IPC-PROTOCOL.md §10` 是**冻结的\n\
             线上契约**、读者在仓外 ⇒ 这一条等于在那份文档里撒谎。\n\
             两条出路：把它真读起来，或者把它从 `args` 与 `§10` 里一起摘掉\n\
             （`files.ls` 的 `ignore_ascii_case` 走的是后一条）。"
        );
        checked += 1;
    }
    // 探针对数恒等：8 → 10（`files.read.text` 的 `path` · `max_bytes` 两对）。
    // 10 → 11（`files.size` 的 `path` 一对）。
    // 11 → 14（`files.read.chunk` 的 `path` · `offset` · `len` 三对）。
    // 14 → 18（`files.grep` 的 `path` · `needle` · `ignore_ascii_case` · `limit` 四对）。
    // 18 → 21（`files.find`：`needle` · `ignore_ascii_case` 两对走了；`query` · `under` · `offset` · `seq` · `stream` 五对来了）。
    // 21 → 24（`files.find` 的 `sort` · `desc` · `scope` 三对）。
    assert_eq!(
        checked, 24,
        "行使的探针对数变了 —— 本条的射程跟着变了，先查探针表"
    );
    std::fs::remove_dir_all(&text_dir).ok();

    // ── ③ 阴性对照：**没声明**的参数必须不起作用 ───────────────────────────
    //
    // 🔴 没有这一格，一把「什么都判不同」的坏尺子照样全绿。
    //    用的正是当年那个鬼影：`files.ls` + `ignore_ascii_case`。
    assert!(
        !CAPABILITIES
            .iter()
            .find(|c| c.name == "files.ls")
            .expect("在表里")
            .args
            .contains(&"ignore_ascii_case"),
        "`files.ls` 又声明了 `ignore_ascii_case` —— 那下面这条阴性对照就不是对照了"
    );
    let base = serde_json::json!({ "path": p(&fx.root) });
    let with_ghost = serde_json::json!({ "path": p(&fx.root), "ignore_ascii_case": true });
    assert_eq!(
        format!("{:?}", answer("files.ls", &base)),
        format!("{:?}", answer("files.ls", &with_ghost)),
        "喂一个**没声明**的参数进去，答案居然变了 —— 那上面那一批「不同」证不了任何事\n\
         （要么这把尺子坏了，要么 `files.ls` 偷偷长出了一个没登记的参数）"
    );
}

// ── 〔96 #13〕`args` 的另一个方向：**解析器读的键，都在声明里** ──────────────────
//
// 要求：「仍开着」逐字「能力声明里的 `args` 没有东西拿它去对真解析器（`fields` 那一侧有）」。
// 上面那条差分买的是「**声明了的都真被读**」（鬼影那一向）；本条补「**真被读的都声明了**」（漏登那一向）——
// 解析器偷偷多认一个键而声明与 `IPC-PROTOCOL.md §10` 都没有它，调用方就永远不知道那个开关存在。
// 两条合起来才是「`args` == 解析器真读的键」的两向相等。
//
// # 怎么取「解析器读的键」：从 `files/mod.rs` 的**生产段**现抽，异源于声明表
//
// 分派 `answer` 的每一臂 `"<能力>" => <函数>(args)` 定出入口；从入口起，沿「把 `args` 原样递下去的调用」
// （`path_arg(args)` / `limit_of(args)` 那一族，只认本文件里定义了的函数）走完，收每个函数体里
// `args.get("<键>")` 与 `args["<键>"]` 的字面量。函数体按大括号配平，字符串与字符字面量里的括号跳过。
//
// # ⚠ 买不到（如实）
//
// - 只认 `args.get("…")` / `args["…"]` 两种读法、只认参数就叫 `args`、只认原样递 `args` 的调用；
//   改成 `serde_json::from_value::<某结构>(args)` 之类的读法它看不见 —— 那一形今天零处（下面「读法只有这两种」那一格在数）。
// - 只看 `files/mod.rs` 一份：解析今天全在这里（其余三份只收已经解析好的结构，如 `index::FindArgs`）。

/// 一份生产段里 `fn <名>(` → 函数体（大括号配平；跳过字符串 / 原始字符串 / 字符字面量）。
fn fn_bodies(prod: &str) -> std::collections::BTreeMap<String, String> {
    let b = prod.as_bytes();
    let mut out = std::collections::BTreeMap::new();
    let mut search = 0;
    while let Some(off) = prod[search..].find("fn ") {
        let at = search + off;
        search = at + 3;
        // 前一个字符得是分词边界（别把 `xfn ` 认成定义）。
        if at > 0 && (b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_') {
            continue;
        }
        let name: String = prod[at + 3..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let Some(open_rel) = prod[at..].find('{') else {
            break;
        };
        // 签名里遇到 `;`（trait 里的声明 / 外部函数）⇒ 没有体。
        if prod[at..at + open_rel].contains(';') {
            continue;
        }
        let open = at + open_rel;
        let mut depth = 0i32;
        let mut i = open;
        let mut end = prod.len();
        while i < b.len() {
            match b[i] {
                b'"' => {
                    // 原始字符串 `r#"…"#`：前面是 `r` 或 `r#…`。
                    let mut hashes = 0;
                    let mut k = i;
                    while k > 0 && b[k - 1] == b'#' {
                        hashes += 1;
                        k -= 1;
                    }
                    let raw = k > 0 && b[k - 1] == b'r';
                    i += 1;
                    if raw {
                        let close = format!("\"{}", "#".repeat(hashes));
                        i = prod[i..]
                            .find(&close)
                            .map(|x| i + x + close.len())
                            .unwrap_or(b.len());
                    } else {
                        while i < b.len() && b[i] != b'"' {
                            if b[i] == b'\\' {
                                i += 1;
                            }
                            i += 1;
                        }
                        i += 1;
                    }
                    continue;
                }
                b'\'' => {
                    // 字符字面量 `'x'` / `'\n'`；否则是生命周期，照常往下走。
                    if i + 2 < b.len() && b[i + 1] != b'\\' && b[i + 2] == b'\'' {
                        i += 3;
                        continue;
                    }
                    if i + 3 < b.len() && b[i + 1] == b'\\' {
                        if let Some(x) = prod[i + 2..].find('\'') {
                            i = i + 2 + x + 1;
                            continue;
                        }
                    }
                }
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        out.entry(name)
            .or_insert_with(|| prod[open..end].to_string());
        search = end.min(prod.len()).max(search);
    }
    out
}

/// 从入口函数起，沿「原样递 `args`」的调用收 `args.get("…")` / `args["…"]` 的键。纯。
fn keys_read_from(
    entry: &str,
    bodies: &std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeSet<String> {
    let mut keys = std::collections::BTreeSet::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut todo = vec![entry.to_string()];
    while let Some(f) = todo.pop() {
        if !seen.insert(f.clone()) {
            continue;
        }
        let Some(body) = bodies.get(&f) else { continue };
        // 空白全挤掉再找：`args\n    .get("…")` 这种 rustfmt 折行（`answer_find` 的 `ignore_ascii_case` 就是这样写的）照样认得出。
        let body: String = body.chars().filter(|c| !c.is_whitespace()).collect();
        for pat in ["args.get(\"", "args[\""] {
            let mut rest = body.as_str();
            while let Some(p) = rest.find(pat) {
                let tail = &rest[p + pat.len()..];
                let key: String = tail.chars().take_while(|c| *c != '"').collect();
                if !key.is_empty() {
                    keys.insert(key);
                }
                rest = &rest[p + pat.len()..];
            }
        }
        for name in bodies.keys() {
            for call in [
                format!("{name}(args)"),
                format!("{name}(args,"),
                format!("{name}(&args"),
            ] {
                if body.contains(&call) {
                    todo.push(name.clone());
                }
            }
        }
    }
    keys
}

/// `answer` 的分派臂：能力名 → 入口函数名。纯。
fn answer_arms(
    bodies: &std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    let body = bodies
        .get("answer")
        .expect("`files/mod.rs` 生产段里找不到 `fn answer` —— 分派改名了，本条跟着改");
    let mut out = std::collections::BTreeMap::new();
    for line in body.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix('"') else {
            continue;
        };
        let Some(q) = rest.find('"') else { continue };
        let cap = &rest[..q];
        let Some(arrow) = rest[q..].find("=>") else {
            continue;
        };
        let callee: String = rest[q + arrow + 2..]
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if cap.starts_with("files.") && !callee.is_empty() {
            out.insert(cap.to_string(), callee);
        }
    }
    out
}

fn files_mod_prod() -> String {
    let path = crate::guard_support::src_root()
        .join("files")
        .join("mod.rs");
    guard_core::production_code(&std::fs::read_to_string(&path).expect("读 files/mod.rs"))
}

#[test]
fn every_key_the_parser_reads_is_a_declared_arg() {
    let prod = files_mod_prod();
    let bodies = fn_bodies(&prod);
    let arms = answer_arms(&bodies);
    // 分派臂 == 声明表（两向；本条的人群就是这张表，塌了下面的全称恒真）。
    let caps: std::collections::BTreeSet<&str> = CAPABILITIES.iter().map(|c| c.name).collect();
    let armed: std::collections::BTreeSet<&str> = arms.keys().map(String::as_str).collect();
    assert_eq!(
        armed, caps,
        "`answer` 的分派臂与 `CAPABILITIES` 对不上 —— 抽臂坏了，或者真漏了一条"
    );
    for cap in CAPABILITIES {
        let read = keys_read_from(&arms[cap.name], &bodies);
        let declared: std::collections::BTreeSet<String> =
            cap.args.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            read, declared,
            "\n能力 `{}` 的解析器（`{}` 起、沿原样递 `args` 的调用）**真读的键**与它声明的 `args` 不相等。\n\
             读了而没声明 ⇒ 🔴 一个调用方永远不知道的开关（`IPC-PROTOCOL.md §10` 是冻结的线上契约，读者在仓外）——\n\
             把它补进 `args` 与 `§10`（并补上面那条差分的探针），或者别读它。\n\
             声明了而没读 ⇒ 上面那条差分也会红（`files.ls` 的 `ignore_ascii_case` 鬼影那一形）。",
            cap.name, arms[cap.name]
        );
    }
    // 读法只有两种：出现第三种（反序列化成结构体）本条就看不见了，先在这里出声。
    for (name, body) in &bodies {
        assert!(
            !(body.contains("from_value") && body.contains("args")),
            "`files/mod.rs::{name}` 把 `args` 交给 `from_value` 反序列化 —— 本条只认 `args.get(\"…\")` / `args[\"…\"]`，\
             那一形的键它收不到（会把漏登判成干净）。改本条的取法，别让它悄悄变瞎。"
        );
    }
}

/// 🔴 正反两控：取法在合成源码上认得出「经 helper 递下去的读」，也不把别的值上的 `.get` 算进来。
#[test]
fn the_parser_key_ruler_sees_a_synthetic_read_and_ignores_other_values() {
    let src = r#"
fn helper(args: &serde_json::Value) -> u32 { args.get("depth").map(|_| 1).unwrap_or(0) }
fn answer_x(args: &serde_json::Value) -> u32 {
    let other = serde_json::json!({"ghost": 1});
    let _ = other.get("ghost");
    let s = format!("{{literal braces}} {}", 1);
    let c = '{';
    helper(args) + args["path"].as_u64().unwrap_or(0) as u32 + s.len() as u32 + c as u32
}
pub fn answer(name: &str, args: &serde_json::Value) -> u32 {
    match name {
        "files.x" => answer_x(args),
        _ => 0,
    }
}
"#;
    let bodies = fn_bodies(src);
    assert_eq!(
        answer_arms(&bodies).get("files.x").map(String::as_str),
        Some("answer_x")
    );
    let got = keys_read_from("answer_x", &bodies);
    let want: std::collections::BTreeSet<String> =
        ["depth", "path"].iter().map(|s| s.to_string()).collect();
    assert_eq!(
        got, want,
        "经 helper 递下去的 `depth` 与直接下标的 `path` 要收到；别的值上的 `.get(\"ghost\")` 不许收（字符串 / 字符里的大括号不许把函数体配歪）"
    );
    // 活样本锚：`files.grep` 真读 `ignore_ascii_case`，`files.ls` 不读（当年那个鬼影的两面）。
    let live = fn_bodies(&files_mod_prod());
    let arms = answer_arms(&live);
    assert!(keys_read_from(&arms["files.grep"], &live).contains("ignore_ascii_case"));
    assert!(!keys_read_from(&arms["files.ls"], &live).contains("ignore_ascii_case"));
}

// ══════════════════════ 边界② 跨 target ══════════════════════

/// 🔴 **判的是「能力在不在」，不是「新鲜度一样」**（边界② 逐字）。
#[test]
fn the_capability_set_is_equal_across_every_target() {
    assert!(
        TARGETS.len() >= 3,
        "target 全集只有 {} 个 —— 「跨 target 相等」在这个人群上没有内容",
        TARGETS.len()
    );
    let all: std::collections::BTreeSet<&str> = capability_names().into_iter().collect();
    for t in TARGETS {
        let here: std::collections::BTreeSet<&str> = CAPABILITIES
            .iter()
            .filter(|c| c.targets.contains(t))
            .map(|c| c.name)
            .collect();
        assert_eq!(
            here, all,
            "`{t:?}` 上的能力集与全体不相等。\n\
             🔴 这一格判的是**能力在不在** —— 那件事在三个平台上必须是同一个答案。\n\
             真有一条只能在某个平台上有 ⇒ 那是那条「豁免必须存在但要贵」，\n\
             要回那一篇立一张豁免表并逐条写明为什么不可能对等，**不是在这里放宽**。"
        );
    }
}

/// 🔴🔴 **边界② 最容易出错的那一格**：保鲜机制**刻意不判相等**，
/// 而且**不许三行抄成一样**。
///
/// 「把它们判成相等会**逼人写假声明**。
/// 这一格是本族最容易出错的地方。」
#[test]
fn freshness_is_declared_per_target_and_is_deliberately_not_judged_equal() {
    // ① 每个 target 恰好一行 —— 漏一个平台就是「那一格没人声明」。
    let declared: std::collections::BTreeSet<Target> = FRESHNESS.iter().map(|f| f.target).collect();
    let all: std::collections::BTreeSet<Target> = TARGETS.iter().copied().collect();
    assert_eq!(
        declared, all,
        "保鲜声明漏了 target（或者多了一个不在全集里的）"
    );
    assert_eq!(
        FRESHNESS.len(),
        TARGETS.len(),
        "有 target 被声明了两次 —— 那会让「这个平台怎么保鲜」有两个答案"
    );

    // ② 每一行都要说清它**没**买到什么。空的一律红。
    for f in FRESHNESS {
        assert!(
            f.how.chars().count() >= 10,
            "`{:?}` 那一行没说清保鲜怎么做",
            f.target
        );
        assert!(
            f.gap.chars().count() >= 20,
            "🔴 `{:?}` 那一行的 `gap` 是空的或者太短。\n\
             这一栏就是「判不了 ＋ 缺什么证据」的住址 —— 留空等于把一句没有读数的\n\
             声明当成事实。",
            f.target
        );
    }

    // ③ 🔴 **反向那半**：不许三行填成同一个机制串。
    //    那就是「为了让某条对拍变绿，把几行抄成一样」的长相，
    //    也就是说的那句「假声明」。
    let mechanisms: std::collections::BTreeSet<&str> = FRESHNESS.iter().map(|f| f.how).collect();
    assert!(
        mechanisms.len() >= 3,
        "{} 个 target 只给出了 {} 种保鲜机制的说法。\n\
         🔴 边界② 逐字：保鲜机制**逐平台不同**，\n\
         把它们说成一件事**就是那句「假声明」**。\n\
         （Linux 的 `inotify` · Windows 的 `ReadDirectoryChangesW` · macOS 的 `FSEvents`\n\
          在合并语义、延迟、丢事件的条件上都不是同一件事。）",
        TARGETS.len(),
        mechanisms.len()
    );

    // ④ 证据档不许全部自称现打 —— 本轮只有一台 Linux 机器。
    let measured = FRESHNESS
        .iter()
        .filter(|f| f.evidence == Evidence::Measured)
        .count();
    assert_eq!(
        measured, 1,
        "自称「本机现打」的 target 有 {measured} 个。\n\
         🔴 本轮只有一台机器（Linux）。多了就是把文献读数写成了实测；\n\
         少了就是连那一台都没量。⇒ 哪天真有了第二个平台的读数，\n\
         **连着那份读数的住址**一起改这个数。"
    );
    // 反向那半：那唯一一格必须**真的是** Linux 那一格（不许把标签挪到一个没机器的平台上）。
    assert_eq!(
        FRESHNESS
            .iter()
            .find(|f| f.evidence == Evidence::Measured)
            .map(|f| f.target),
        Some(Target::LinuxGnu),
        "自称现打的那一格不是本机那个 target"
    );
}

// ══════════════════════ 边界③ 周期可查询 ══════════════════════

/// 🔴 边界③：「『定期重走』的周期……**必须可查询**，不许只活在代码里」。
#[test]
fn the_rewalk_interval_is_part_of_the_declared_surface_and_is_really_queryable() {
    let _lock = resident_lock();
    // ① 它在 `files.index.status` 的**声明**字段表里。
    let status_cap = CAPABILITIES
        .iter()
        .find(|c| c.name == "files.index.status")
        .expect("这条能力必须在表里");
    assert!(
        status_cap.fields.contains(&"rewalk_interval_secs"),
        "重走周期不在 `files.index.status` 的字段表里 —— 那它就只活在代码里了"
    );
    // ② 它在**真的回出去**的那个 JSON 里，而且等于声明的那个常量。
    let v = answer("files.index.status", &serde_json::json!({})).expect("status 不该失败");
    assert_eq!(
        v.get("rewalk_interval_secs").and_then(|x| x.as_u64()),
        Some(crate::files::index::REWALK_INTERVAL_SECS),
        "线上回的那个周期与声明的常量不是同一个数"
    );
    // ③ 界面要显示的那个「多久前更新的」也必须在同一个答案里
    //    （不许让用户猜为什么搜不到）。
    assert!(
        status_cap.fields.contains(&"age_secs"),
        "新鲜度那个数不在字段表里"
    );
    assert!(v.get("age_secs").is_some(), "新鲜度那个数没有真的回出去");
    // ④ 周期是个**有意义的正数**。0 会让 `stale` 恒真、界面永远显示「该重走了」。
    assert!(
        crate::files::index::REWALK_INTERVAL_SECS > 0,
        "重走周期是 0 —— 那个声明没有内容"
    );
}

/// **冷启动首建那个数**与重走周期**分开钉**，而且同样可查询。
///
/// 要求「单列一个数并在搜索界面显示」⇒ 它必须在 `files.index.status` 的**声明**字段表里、
/// 真的回出去、等于那个常量；而且它不能被周期那一格顶替（两个数各是各的）。
#[test]
fn the_cold_first_build_estimate_is_its_own_declared_and_queryable_number() {
    let _lock = resident_lock();
    let status_cap = CAPABILITIES
        .iter()
        .find(|c| c.name == "files.index.status")
        .expect("这条能力必须在表里");
    assert!(
        status_cap.fields.contains(&"cold_first_build_secs"),
        "冷启动首建那个数不在 `files.index.status` 的字段表里 —— 那它就只活在代码里了"
    );
    let v = answer("files.index.status", &serde_json::json!({})).expect("status 不该失败");
    assert_eq!(
        v.get("cold_first_build_secs").and_then(|x| x.as_u64()),
        Some(crate::files::index::COLD_FIRST_BUILD_SECS),
        "线上回的那个首建估计与声明的常量不是同一个数"
    );
    // 「单列」：它不是周期那个数换了个名字（两个数都从同一个答案里取，各对各的常量）。
    assert_ne!(
        crate::files::index::COLD_FIRST_BUILD_SECS,
        crate::files::index::REWALK_INTERVAL_SECS,
        "首建估计与重走周期是同一个数 —— 那就不是「单列」，是同一个数报了两遍"
    );
    assert!(
        crate::files::index::COLD_FIRST_BUILD_SECS > 0,
        "首建估计是 0 —— 界面上会说「约 0 秒」，那句话没有内容"
    );
}

// ══════════════════════ 原始字节那一条 ══════════════════════

/// 有损解码的那几个调用 —— 运行时拼，理由同 [`write_verbs`]。
fn lossy_calls() -> Vec<String> {
    [
        ("to_string", "_lossy"), // `OsStr`/`Path` → String，非法字节变替换字符
        ("from_utf8", "_lossy"), // 同上，字节那一侧
        ("to_str", "_lossy"),    // 同族的第三个写法
        (".dis", "play()"),      // `Path` 的 Display 也是有损的
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect()
}

/// 一段源码里的有损解码命中 —— **抬成纯函数**，好让下面那条正题与
/// 它的阳性对照量**同一把尺子**（尺子一分叉，红灯就开始骗人）。
fn lossy_hits(label: &str, src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for n in lossy_calls() {
        if src.contains(n.as_str()) {
            out.push(format!("  {label}: {n}"));
        }
    }
    out
}

/// 🔴 本族**一处有损解码都没有**。
#[test]
fn no_lossy_decode_anywhere_in_the_family() {
    let mut bad: Vec<String> = Vec::new();
    for (name, src) in family_sources() {
        bad.extend(lossy_hits(&format!("files/{name}"), &src));
    }
    assert_eq!(
        bad,
        Vec::<String>::new(),
        "本族里出现了有损解码：\n{}\n\n\
         🔴：非 UTF-8 文件名「库层有损解码，**寻址不到**」。\n\
         「寻址不到」不是显示难看 —— 解过一次之后，回程拿着那串替换字符去找的\n\
         是一个**不存在的名字**。\n\
         ⇒ 要把路径变成人话，那件事归**界面**；后端这一侧一路走字节\n\
           （`files::raw::to_json` 给的两种形都是双向无损的）。",
        bad.join("\n")
    );
}

/// **阳性对照的语料** —— 逐条**独立写出来**的有损解码调用。
///
/// 🔴 与 [`WRITE_SAMPLES`] 同一条理由（那里有整段说明）：样本**不许**由针拼出来。
/// 针拼错时，自拼的样本会跟着错，两侧同源 ⇒ 恒真。
const LOSSY_SAMPLES: &[&str] = &[
    "let s = p.to_string_lossy().to_string();",
    "let s = String::from_utf8_lossy(&bytes);",
    "let s = os.to_str_lossy();",
    "eprintln!(\"{}\", p.display());",
];

/// ★★ **阳性对照**：每一段真的有损的代码，必须被那几根针咬住。
#[test]
fn every_lossy_needle_really_bites_on_an_independently_written_sample() {
    assert_eq!(
        lossy_calls().len(),
        4,
        "有损解码的 needle 条数变了 —— 回来把这个数改对，并说清加/减的是哪一条"
    );
    for s in LOSSY_SAMPLES {
        assert!(
            !lossy_hits("sample", s).is_empty(),
            "这一段是有损解码，针却没咬住：\n  {s}\n\
             ⇒ 对应那根针拼错了，或者表里少了它那一条。"
        );
    }
    // ★ 反向那半：每一根针都得**至少有一条**语料在行使它。
    let unexercised: Vec<String> = lossy_calls()
        .into_iter()
        .filter(|n| !LOSSY_SAMPLES.iter().any(|s| s.contains(n.as_str())))
        .collect();
    assert_eq!(
        unexercised,
        Vec::<String>::new(),
        "这几根针没有任何一条阳性语料在行使它们 —— 它们没被验过"
    );
    // ★ 阴性对照：走原始字节的代码不许被判成有损。
    assert_eq!(
        lossy_hits("clean", "let b = p.as_os_str().as_encoded_bytes();"),
        Vec::<String>::new(),
        "一段走原始字节的代码被判成了有损解码 —— 那会逼人绕开这条判据"
    );
}

/// 类型名是个**闭集**，而 [`crate::files::KINDS`] 就是它的住址。
#[test]
fn the_kind_names_are_a_closed_set_with_one_home() {
    let _lock = resident_lock();
    let mut sorted = KINDS.to_vec();
    sorted.sort_unstable();
    assert_eq!(KINDS.to_vec(), sorted, "类型名表没排序");
    let fx = crate::files::index::tests::make_tree("kinds", 1, 1, 0);
    let v = answer(
        "files.ls",
        &serde_json::json!({"path": fx.root.to_str().expect("夹具路径是 ASCII")}),
    )
    .expect("ls 不该失败");
    let rows = v
        .get("entries")
        .and_then(|e| e.as_array())
        .expect("entries 该是数组");
    assert_eq!(rows.len(), 1, "夹具根下只有一个子目录");
    let kind = rows[0]
        .get("kind")
        .and_then(|k| k.as_str())
        .expect("每行都要有 kind");
    assert!(
        KINDS.contains(&kind),
        "真的回出去的类型名 `{kind}` 不在那个闭集里"
    );
    assert_eq!(kind, "dir", "夹具根下那一项是个目录");
}

/// `files.stat` / `files.ls` 的路径参数**坏了就拒**，不猜。
#[test]
fn a_malformed_path_argument_is_refused_with_its_own_code() {
    let _lock = resident_lock();
    for (cap, args) in [
        ("files.stat", serde_json::json!({})),
        ("files.stat", serde_json::json!({"path": 7})),
        ("files.stat", serde_json::json!({"path": ""})),
        ("files.ls", serde_json::json!({"path": {"b16": "zz"}})),
    ] {
        let got = answer(cap, &args);
        assert!(
            matches!(got, Err(("bad_path", _))),
            "`{cap}` 对 {args:?} 没有回 `bad_path`，回的是 {got:?}"
        );
    }
    // `find` 那一侧是另一个码 —— 两条路的诊断不许混成一句。
    assert!(
        matches!(
            answer("files.find", &serde_json::json!({})),
            Err(("bad_args", _))
        ),
        "`files.find` 少了 `needle` 却没回 `bad_args`"
    );
}

/// 每条能力声明的错误码，都得**真的出得来**（幽灵码检查）。
///
/// ⚠ 它只覆盖今天造得出触发条件的那几个；`unreadable` 那一档要一个读不了的路径，
/// 用一个不存在的路径造（`io::ErrorKind` 不同，但走的是同一条出口）。
#[test]
fn the_declared_error_codes_are_not_ghosts() {
    let _lock = resident_lock();
    let nowhere = std::env::temp_dir().join(format!("ccm-24f-ghost-{}", std::process::id()));
    std::fs::remove_dir_all(&nowhere).ok();
    let arg = serde_json::json!({"path": nowhere.to_str().expect("ASCII")});
    for (cap, want) in [("files.ls", "not_found"), ("files.stat", "unreadable")] {
        let got = answer(cap, &arg);
        assert!(
            matches!(got, Err((c, _)) if c == want),
            "`{cap}` 对一个不存在的路径没有回 `{want}`，回的是 {got:?}"
        );
        let declared = CAPABILITIES
            .iter()
            .find(|c| c.name == cap)
            .expect("在表里")
            .codes;
        assert!(
            declared.contains(&"unreadable") && declared.contains(&"bad_path"),
            "`{cap}` 的错误码表漏了它真的会回的那几个"
        );
    }
    // `files.ls` 打不开的另两种：给的是一份文件 ⇒ `not_dir`；没有读权限 ⇒ `denied`（unix）。
    let base = std::env::temp_dir().join(format!("ccm-24f-ghost2-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(base.join("locked")).unwrap();
    std::fs::write(base.join("f.txt"), b"x").unwrap();
    let ls = |p: &std::path::Path| {
        answer(
            "files.ls",
            &serde_json::json!({"path": p.to_str().unwrap()}),
        )
    };
    assert!(
        matches!(ls(&base.join("f.txt")), Err(("not_dir", _))),
        "给一份文件没回 `not_dir`"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(base.join("locked"), std::fs::Permissions::from_mode(0o000))
            .unwrap();
        // root 读得动一切 ⇒ 这一格在 root 下判不了（那时列得出来，读数照实）。
        match ls(&base.join("locked")) {
            Err(("denied", _)) | Ok(_) => {}
            other => panic!("没权限没回 `denied`：{other:?}"),
        }
        std::fs::set_permissions(base.join("locked"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    std::fs::remove_dir_all(&base).ok();
}

// ══════════════════════ 第七、第八条 ══════════════════════
//
// 窗口换走通道的那两问。每条都**正控 ＋ 阴性对照同拍**：只验「该拒的拒了」，
// 一个恒 `Err` 的实现也全绿；只验「该成的成了」，一个不设上限的实现也全绿。

/// 一个本格独占的临时目录（`tag` 区分用例，`pid` 区分并发跑的进程）。
fn f7a_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-f7a-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).expect("建夹具目录");
    d
}

fn path_json(p: &std::path::Path) -> serde_json::Value {
    serde_json::Value::String(p.to_str().expect("夹具路径是 ASCII").to_string())
}

/// ★ 放得下的那一份**整份、逐字节**回来；回参的键 == 声明的 `fields`（两向）。
#[test]
fn a_text_under_the_cap_comes_back_whole_and_its_keys_are_the_declared_fields() {
    let d = f7a_dir("rt-ok");
    let f = d.join("note.md");
    let body = "第一行\n second line\n\ttab\n";
    std::fs::write(&f, body).expect("铺");
    let v = answer(
        "files.read.text",
        &serde_json::json!({ "path": path_json(&f), "max_bytes": 4096 }),
    )
    .expect("一份放得下的 UTF-8 文本被拒了");
    assert_eq!(
        v["text"].as_str(),
        Some(body),
        "回来的文本与盘上那份不逐字节相等"
    );
    assert_eq!(v["bytes"].as_u64(), Some(body.len() as u64));
    let got: std::collections::BTreeSet<&str> = v
        .as_object()
        .expect("对象")
        .keys()
        .map(String::as_str)
        .collect();
    let declared: std::collections::BTreeSet<&str> = CAPABILITIES
        .iter()
        .find(|c| c.name == "files.read.text")
        .expect("在表里")
        .fields
        .iter()
        .copied()
        .collect();
    assert_eq!(
        got, declared,
        "`files.read.text` 真回出去的键与声明的 `fields` 对不上"
    );
    std::fs::remove_dir_all(&d).ok();
}

/// 🔴 **超上限整趟拒、不截断** —— 恰好在上限上的那一份照收，多一个字节就拒，
/// 而且拒的那一趟**一个字都不交**（不是交半份）。
#[test]
fn one_byte_over_the_cap_is_refused_whole_and_exactly_at_the_cap_is_not() {
    let d = f7a_dir("rt-cap");
    let f = d.join("n.txt");
    std::fs::write(&f, "x".repeat(100)).expect("铺");
    let at = answer(
        "files.read.text",
        &serde_json::json!({ "path": path_json(&f), "max_bytes": 100 }),
    )
    .expect("恰好等于上限的那一份被拒了 —— 上限是「最多」，不是「少于」");
    assert_eq!(at["bytes"].as_u64(), Some(100));
    let over = answer(
        "files.read.text",
        &serde_json::json!({ "path": path_json(&f), "max_bytes": 99 }),
    );
    match over {
        Err(("too_large", m)) => assert!(
            copy_core::copy_matches_with("beFilesRead.text.tooLarge", &[("over", "1")], &m),
            "拒了，但那句话没说多了多少（用户据此知道要删掉多少）：{m}"
        ),
        other => panic!("🔴 超上限一个字节没被整趟拒（截断了？）：{other:?}"),
    }
    std::fs::remove_dir_all(&d).ok();
}

/// 三形「不是文本」各自落 `not_text`；读不到落 `unreadable`；参数形状落各自的码。
/// ⚠ 最后一条是**两向集合相等**：造得出来的码 == 声明的码（多一个是幽灵、少一个是没声明）。
#[test]
fn every_refusal_of_read_text_lands_on_its_own_declared_code() {
    let d = f7a_dir("rt-codes");
    let nul = d.join("nul.bin");
    std::fs::write(&nul, b"ab\0cd").expect("铺");
    let bad = d.join("latin1.txt");
    std::fs::write(&bad, [0x63u8, 0x61, 0x66, 0xE9]).expect("铺"); // 「café」的 Latin-1 字节
    let big = d.join("big.txt");
    std::fs::write(&big, "y".repeat(10)).expect("铺");
    let gone = d.join("gone.txt");
    let ceiling = crate::files::READ_TEXT_MAX_BYTES as u64;
    let cases: Vec<(serde_json::Value, &str)> = vec![
        (
            serde_json::json!({ "path": path_json(&nul), "max_bytes": 100 }),
            "not_text",
        ),
        (
            serde_json::json!({ "path": path_json(&bad), "max_bytes": 100 }),
            "not_text",
        ),
        (
            serde_json::json!({ "path": path_json(&d), "max_bytes": 100 }),
            "not_text",
        ),
        (
            serde_json::json!({ "path": path_json(&big), "max_bytes": 9 }),
            "too_large",
        ),
        (
            serde_json::json!({ "path": path_json(&gone), "max_bytes": 100 }),
            "unreadable",
        ),
        (serde_json::json!({ "max_bytes": 100 }), "bad_path"),
        (serde_json::json!({ "path": path_json(&big) }), "bad_args"),
        (
            serde_json::json!({ "path": path_json(&big), "max_bytes": 0 }),
            "bad_args",
        ),
        (
            serde_json::json!({ "path": path_json(&big), "max_bytes": ceiling + 1 }),
            "bad_args",
        ),
    ];
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for (args, want) in &cases {
        let got = answer("files.read.text", args);
        assert!(
            matches!(got, Err((c, _)) if c == *want),
            "`files.read.text` 对 {args} 该回 `{want}`，回的是 {got:?}"
        );
        seen.insert(want);
    }
    // 正控：天花板本身是收的（对一份小文件）—— 不然上面那一条「超天花板」证不了边界在哪。
    assert!(
        answer(
            "files.read.text",
            &serde_json::json!({ "path": path_json(&big), "max_bytes": ceiling })
        )
        .is_ok(),
        "`max_bytes` 恰好等于天花板被拒了"
    );
    let declared: std::collections::BTreeSet<&str> = CAPABILITIES
        .iter()
        .find(|c| c.name == "files.read.text")
        .expect("在表里")
        .codes
        .iter()
        .copied()
        .collect();
    assert_eq!(seen, declared, "造得出来的码与声明的码不是同一个集合");
    std::fs::remove_dir_all(&d).ok();
}

/// ★ `files.home`：有就交、说不出就拒（没有 / 空 / 相对 三形），**不拿别处兜底**。
///
/// ⚠ 喂的是纯函数 [`super::home_from`]，不去改测试进程的环境（理由在它的头注）。
/// 活的那一趟另判一格：本机这台后端真答得出一条**绝对**路径（正控，不与环境对答案）。
#[test]
fn home_is_given_when_the_environment_has_one_and_refused_otherwise() {
    let ok = super::home_from(Some("/home/someone".into())).expect("一条绝对路径被拒了");
    assert_eq!(ok["path"].as_str(), Some("/home/someone"));
    let keys: std::collections::BTreeSet<&str> = ok
        .as_object()
        .expect("对象")
        .keys()
        .map(String::as_str)
        .collect();
    let declared: std::collections::BTreeSet<&str> = CAPABILITIES
        .iter()
        .find(|c| c.name == "files.home")
        .expect("在表里")
        .fields
        .iter()
        .copied()
        .collect();
    assert_eq!(
        keys, declared,
        "`files.home` 回出去的键与声明的 `fields` 对不上"
    );
    for (h, what) in [
        (None, "没有"),
        (Some(std::ffi::OsString::new()), "空串"),
        (Some("relative/home".into()), "相对路径"),
    ] {
        let got = super::home_from(h);
        assert!(
            matches!(got, Err(("no_home", _))),
            "home 是「{what}」时没有回 `no_home`，回的是 {got:?} —— 那就是在猜一个起点"
        );
    }
    #[cfg(unix)]
    {
        let live = answer("files.home", &serde_json::json!({})).expect("本机这台后端说不出 home");
        let p = live["path"].as_str().expect("本机 home 是 UTF-8");
        assert!(p.starts_with('/'), "本机答出来的 home 不是绝对路径：{p}");
    }
}

// ══════════════════════ `files.stat` 送权限位 ══════════════════════
//
// 要求：文件窗口改权限时显示现值（窗口做不到、缺后端读口）·
// （P1）。

/// P1：`files.stat` 真回的键集 == 声明的 `fields`（两向，真调一次）；`mode` 就是那个文件此刻的低 12 位。
/// 异源：期望值由测试自己用 `set_permissions` 设下去（`0o640` / `0o4755` 两个真能设的值，第二个带 setuid 位 ——
/// 掩码若只取低 9 位就少了它）。
#[cfg(unix)]
#[test]
fn gp1_stat_reports_the_declared_fields_and_the_real_mode_bits() {
    use std::os::unix::fs::PermissionsExt as _;
    let _lock = resident_lock();
    let d = f7a_dir("gp1-mode");
    let f = d.join("m.txt");
    std::fs::write(&f, b"x").expect("铺文件");
    let declared: std::collections::BTreeSet<String> = CAPABILITIES
        .iter()
        .find(|c| c.name == "files.stat")
        .expect("在表里")
        .fields
        .iter()
        .map(|s| s.to_string())
        .collect();
    for want in [0o640_u32, 0o4755] {
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(want)).expect("设权限");
        let v = answer("files.stat", &serde_json::json!({ "path": path_json(&f) }))
            .expect("stat 一个在的文件不该失败");
        let got: std::collections::BTreeSet<String> =
            v.as_object().expect("对象").keys().cloned().collect();
        assert_eq!(got, declared, "`files.stat` 真回的键与声明对不上");
        assert_eq!(
            v.get("mode").and_then(serde_json::Value::as_u64),
            Some(u64::from(want)),
            "`mode` 应当是 {want:o}，回的是 {v}"
        );
    }
    std::fs::remove_dir_all(&d).ok();
}

/// `files.read.chunk`：非 UTF-8 名按字节寻址、逐块读回拼起来 == 盘上那份；越过末尾 ⇒ 空块 `eof`；`len` 越界 ⇒ `bad_args`；目录 ⇒ `not_text`。
/// 要求住址：用户 09-27「经那台后端链路按字节寻址（b16 路径）分块读回」· 下载对远端只读（这一条纯读）。
#[test]
#[cfg(unix)]
fn a_non_utf8_file_is_read_back_chunk_by_chunk_byte_for_byte() {
    use std::os::unix::ffi::OsStrExt as _;
    let dir = std::env::temp_dir().join(format!("ccm-rc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("夹具目录");
    let f = dir.join(std::ffi::OsStr::from_bytes(b"f\xfe.bin"));
    let body: Vec<u8> = (0..10_000u32).map(|i| (i % 253) as u8).collect();
    std::fs::write(&f, &body).expect("铺");
    let at = raw::to_json(raw::path_bytes(&f));
    let (mut got, mut off) = (Vec::new(), 0u64);
    // 有界：10 000 字节 / 4096 一块 ⇒ 三块读完；多转一圈就是 `eof` 没说实话。
    for round in 0.. {
        assert!(round < 4, "读了 {round} 块还没说 eof —— eof 不实");
        let v = answer_wire(
            "files-read-chunk",
            &serde_json::json!({ "path": at, "offset": off, "len": 4096 }),
        )
        .expect("读一块被拒");
        assert_eq!(v["size"], body.len());
        let hexed = v["content"]["b16"].as_str().expect("b16");
        got.extend(
            (0..hexed.len() / 2).map(|k| u8::from_str_radix(&hexed[2 * k..2 * k + 2], 16).unwrap()),
        );
        off = got.len() as u64;
        if v["eof"] == true {
            break;
        }
    }
    assert_eq!(got, body, "逐块拼回来不是盘上那份");
    let past = answer_wire(
        "files-read-chunk",
        &serde_json::json!({ "path": at, "offset": 99_999, "len": 10 }),
    )
    .unwrap();
    assert_eq!(
        (past["content"]["b16"].as_str(), past["eof"].as_bool()),
        (Some(""), Some(true))
    );
    let e = answer_wire(
        "files-read-chunk",
        &serde_json::json!({ "path": at, "offset": 0, "len": READ_CHUNK_MAX_BYTES + 1 }),
    )
    .unwrap_err();
    assert_eq!(e.0, "bad_args");
    let e = answer_wire(
        "files-read-chunk",
        &serde_json::json!({ "path": raw::to_json(raw::path_bytes(&dir)), "offset": 0, "len": 1 }),
    )
    .unwrap_err();
    assert_eq!(e.0, "not_text");
    std::fs::remove_dir_all(&dir).ok();
}

/// 指向目录的链接：`files-ls` 那一行 `kind` 照旧是 `symlink`（不跟），另带 `link_dir: true`；
/// 指向文件的 `false`；断链与非链接那几行不出这一格。
#[cfg(unix)]
#[test]
fn ls_says_which_symlinks_point_at_directories() {
    let d = f7a_dir("ls-link");
    let target = d.join("real-dir");
    std::fs::create_dir_all(&target).expect("建目标目录");
    std::fs::write(d.join("plain.txt"), b"x").expect("建文件");
    std::os::unix::fs::symlink(&target, d.join("to-dir")).expect("链接到目录");
    std::os::unix::fs::symlink(d.join("plain.txt"), d.join("to-file")).expect("链接到文件");
    std::os::unix::fs::symlink(d.join("nowhere"), d.join("dangling")).expect("断链");
    let v = answer("files.ls", &serde_json::json!({ "path": path_json(&d) })).expect("ls 不该失败");
    let mut got: Vec<(String, String, Option<bool>)> = v["entries"]
        .as_array()
        .expect("entries 该是数组")
        .iter()
        .map(|e| {
            let name = e["path"]
                .as_str()
                .unwrap()
                .rsplit('/')
                .next()
                .unwrap()
                .to_string();
            (
                name,
                e["kind"].as_str().unwrap().to_string(),
                e.get("link_dir").map(|b| b.as_bool().unwrap()),
            )
        })
        .collect();
    got.sort();
    let want = |n: &str, k: &str, l: Option<bool>| (n.to_string(), k.to_string(), l);
    assert_eq!(
        got,
        vec![
            want("dangling", "symlink", None),
            want("plain.txt", "file", None),
            want("real-dir", "dir", None),
            want("to-dir", "symlink", Some(true)),
            want("to-file", "symlink", Some(false)),
        ]
    );
    std::fs::remove_dir_all(&d).ok();
}

/// 列目录时读不出的那几项照数、不静默跳过：`files-ls` 回 `unreadable`（窗口照截断那样说一句）。
#[test]
fn a_directory_entry_that_cannot_be_read_is_counted_not_skipped() {
    let mut n = 0usize;
    let items: Vec<std::io::Result<u8>> = vec![
        Ok(1),
        Err(std::io::Error::other("坏的一项")),
        Ok(2),
        Err(std::io::Error::other("又一项")),
    ];
    let got: Vec<u8> = readable(items, &mut n).collect();
    assert_eq!((got, n), (vec![1, 2], 2));
    // 帧面那一条带着这一格（读得全时是 0，不是缺席）。
    let dir = std::env::temp_dir().join(format!("ccm-ls-unreadable-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a"), b"").unwrap();
    let v = answer_ls(&serde_json::json!({ "path": dir.to_string_lossy() })).unwrap();
    assert_eq!(v["unreadable"], serde_json::json!(0));
    std::fs::remove_dir_all(&dir).ok();
}
