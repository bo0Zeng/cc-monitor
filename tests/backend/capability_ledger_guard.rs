//! 〔步 `8a` · 2026-09-21〕**能力清单汇总的护栏** —— `设计/96 §2` 第 2 层的机器形态。
//!
//! # `K-G6` `KG62`：性质与人群，两行逐字（各自只许有一句）
//!
//! - **它守的性质是**：`设计/96 §2` 第 2 层那份「由各能力面**汇总**而来」的清单，
//!   装的恰好是各族**自己**声明的那些能力 —— 一族不许漏进来，一条不许凭空多出来，
//!   而且射程那两类（资产 / 协议）各有真成员。
//! - **它扫的人群是**：`src/backend/` 下递归全部 `.rs` 的**生产段**里那些
//!   `const CAPABILITIES` 声明表（走 [`guard_core::scan_tree_excluding`] 的**明写**排除），
//!   外加 [`crate::CAPABILITY_FACES`] / [`crate::capability_ledger`] 两侧本身。
//!
//! # 🔴 两侧为什么不同源（这是本份最贵的一条，别读成样板话）
//!
//! [`crate::capability_ledger`] 是一个对 [`crate::CAPABILITY_FACES`] 的**纯函数** ——
//! 那是刻意的（`设计/96 §2` 第 2 层买的是「**不可能**不一致」，不是「不一致会被逮到」）。
//! 代价：拿它去对「各族声明的并集」会是 `x == x`，本仓对这一形的说法逐字
//! **「恒等两侧同源会恒真」**。
//!
//! ⇒ 本份的两条承重断言，两侧都**不是**汇总那一侧的镜子：
//!
//! | 断言 | 汇总那一侧 | 被对的那一侧 | 同一次编辑改得到两边吗 |
//! |---|---|---|---|
//! | [`the_ledger_is_exactly_the_roster_that_was_adjudicated`] | `src/backend/` 里各族的 `const` | [`ROSTER`]（住 `tests/backend/`，逐条点名） | ❌ 两棵树 |
//! | [`every_capability_table_in_the_tree_is_a_registered_face`] | [`crate::CAPABILITY_FACES`] 那张登记表 | **源码树上现打**出来的那一组文件 | ❌ 一边是 const、一边是文件系统 |
//!
//! 第一条治「往某一族里加/删一条能力」，第二条治「新长出一整族却没汇进来」——
//! **后者才是「汇总」这件事本身的失效形态**，而纯函数拦不住它（它只汇总登记过的面）。
//!
//! # ⚠ 它**买不到**什么（逐条，别读宽）
//!
//! 1. **`ROSTER` 是一张点名表，它证明不了「这些能力该存在」。** 它只证明
//!    「今天在盘上的这些，是被逐条看过一遍的」。该不该存在住 `设计/96 §2.9` 那种裁决里。
//!    ⇒ 与 `files/capability_guard.rs` 的 `REGISTERED` 同一条口径，那份头注逐字写过：
//!    **不许**为了让某一侧变绿而从点名表里摘一个名字。
//! 2. **`targets` 那一栏是声明，不是编译结果。** 「这四个 target 上都编得过」的真判据是
//!    门禁的 `muslbuild` / `winchk-backend` 那两格，本份钉不到；macOS 那一格**两道都没有**。
//!    本份只钉「每个面都说了话，而且说的不是空话」。
//! 3. **它不判 `target_basis` 那句话说得对不对**（同 `spawn_registry` / `FRESHNESS::gap`
//!    那条已登记的边界）：钉的是「说得出来」，不是「真的想过」。
//! 4. 🔴 **它完全不碰「在这台机器上做不到」那个轴**（`wire::Unavailable`）。
//!    那个轴是**运行期逐机器**的，本份与 [`crate::capability_ledger`] 都是**编译期**的。
//!    两者今天**谁也没汇进谁**，而那不是缺陷 —— 是 `设计/96 §2` 自己分开的两个轴
//!    （「target 答每个平台编不编得过」）。对上那两个轴是步 `8b` 的活，登记在
//!    `设计/99 §4.8.3 P12`，**挡在步 3.5 上**。

use super::{
    capability_ledger, parity_faces, CapabilityKind, Target, CAPABILITIES, CAPABILITY_FACES,
    TARGETS, TARGET_GAPS,
};

/// 〔PR1〕第 3 层人群里**声明过**的全部 `(面, 名)`（两张面表的 `declares()` 连起来，不收窄）。
fn parity_declared() -> Vec<(&'static str, &'static str)> {
    parity_faces()
        .flat_map(|f| (f.declares)().into_iter().map(move |n| (f.family, n)))
        .collect()
}

/// 🔴 **被逐条看过一遍的那份名单** —— `(族名, 能力名)`，**逐字点名**。
///
/// # 它为什么必须抄在这里（与 `files/capability_guard.rs::REGISTERED` 同一条理由）
///
/// 汇总那一侧是纯函数 ⇒ 往某一族里加一条能力，它**自动**就流进汇总里。
/// 没有这张表的话，那条相等断言对「悄悄多长出一条没人裁过的能力」**恒绿**。
/// 有了它，两向各治一种失效：
///
/// - 汇总里有、本表没有 ⇒ **有人加了一条能力，而没有任何人看过它**；
/// - 本表有、汇总里没有 ⇒ **一条能力被删/改名了，而别处的消费方还当它在**。
///
/// 🔴 **不许**为了让断言变绿就往本表里补一行了事 —— 补那一行的意思是
/// 「我看过这条能力，它该在清单里」。那句话有分量。
const ROSTER: &[(&str, &str)] = &[
    // ── `ccm-launcher`（`control/ccm/mod.rs`，`设计/96 §2` 射程：资产轴）────────────
    ("ccm-launcher", "account"),
    ("ccm-launcher", "account-via-backend"),
    ("ccm-launcher", "agent"),
    ("ccm-launcher", "attach"),
    ("ccm-launcher", "backend-discover"),
    ("ccm-launcher", "base-url-across-tmux"),
    ("ccm-launcher", "bus-register"),
    ("ccm-launcher", "ccm-sid"),
    ("ccm-launcher", "cwd"),
    ("ccm-launcher", "detach"),
    ("ccm-launcher", "launcher"),
    ("ccm-launcher", "model"),
    ("ccm-launcher", "new"),
    ("ccm-launcher", "print"),
    ("ccm-launcher", "resume"),
    ("ccm-launcher", "tmux"),
    ("ccm-launcher", "tmux-base"),
    ("ccm-launcher", "tmux-size"),
    // ── `files-read`（`files/mod.rs`，`设计/96 §2.9` 那张表）──────────────────────
    ("files-read", "files.browse"),
    ("files-read", "files.find"),
    // 〔F7a · 第三波 09-24〕我看过这条能力，它该在清单里：窗口「开在 home」那一问经通道问后端
    // （`设计/60 §13`；此前 monitor 为它单拨一条 SFTP）。纯读，资产轴，四个 target 都有。
    ("files-read", "files.home"),
    ("files-read", "files.index.rebuild"),
    ("files-read", "files.index.status"),
    ("files-read", "files.ls"),
    // 〔W5-FILES · 第五波〕我看过这条能力，它该在清单里：算目录大小（`设计/60 §6.2`，V45「能连 ssh 做什么后端就能做什么」）。
    // 纯读，资产轴，四个 target 都有（非 unix 上「不进别的文件系统」那一判不开口，如实写在 `files/size.rs` 头注）。
    ("files-read", "files.size"),
    // 〔F7a · 第三波 09-24〕同上：编辑器读一份文本经通道问后端（此前 SFTP 整份搬字节）。
    // 纯读，超上限整趟拒不截断，上限由调用方给、后端有自己的天花板。
    ("files-read", "files.read.text"),
    ("files-read", "files.stat"),
    // ── `stream-flags`（`lib.rs`，`设计/96 §2` 射程：**协议**轴）────────────────────
    ("stream-flags", "bg"),
    // 〔`设计/80 §8.7` 步 2，09-22〕**启动期令牌**（`CCM_RBIND_TOKEN`）。
    // 补这一行的意思是「我看过这条能力，它该在清单里」，逐条如下：
    // · 它是**协议轴**的（`CapabilityKind::Protocol`）—— 说的是「我认不认
    //   `--with-rbind-token` 这条流 flag」，与 `bg` / `tail-only` 同型；
    // · 它有一条**真**的可剥离 flag（`split_stream_flags` 剥它，
    //   `every_capability_token_is_strippable` 当场验），不是为了进这张表编的；
    // · 它为何非得是一条**能力**而不是“字段在不在”：`§8.6 ④` 逐字要「能力协商
    //   ＋老后端诚实降级」，而一个缺席的字段分不开「这台后端不报令牌」
    //   与「这条会话真的没令牌」（`lib.rs::CAPABILITIES` 头注的整段论证）。
    ("stream-flags", "rbind-token"),
    ("stream-flags", "tail-only"),
];

/// 源码树上**每一份**声明了 `const CAPABILITIES` 的文件（相对本 crate 源码树根）。
///
/// 🔴 **走 [`guard_core::scan_tree_excluding`] 的明写排除那一支。**
///
/// 依据是 `设计/99` 条 73 那条纪律逐字：「『摘掉我自己』**不许靠 `file!()`**」——
/// 那条靠 `file!()` 做后缀比的自摘，在「判据由 `#[path]` 挂进来」这一处**不生效**
///（给出来的是带 `..` 的折返路径，比不中规范化过的草垛）。
///
/// ⚠ 而本份**根本不需要**摘自己：本文件住 `tests/backend/`，不在被扫的那棵树里。
/// 明写名单在这里摘的是另一回事 —— 一份**真的在树里、却不是能力面**的文件，
/// 见 [`NOT_A_PRODUCTION_FACE`]。明写那一支自带「名单上每一条都必须真的摘到东西，
/// 摘到 0 份就 panic」，于是被摘的那份文件改名 / 搬走会**当场红**，
/// 而不是安静地回到人群里。
fn capability_table_files() -> Vec<String> {
    let root = crate::guard_support::src_root();
    let mut out: Vec<String> = Vec::new();
    let scanned = guard_core::scan_tree_excluding(&root, &["rs"], NOT_A_PRODUCTION_FACE);
    // ★ 反空真：扫不到东西时下面的集合相等会对空集成立，而登记表非空 ⇒ 其实会红；
    //   但红的话会指向「登记表多了三行」而不是「采集坏了」—— 那是**红错了地方**。
    assert!(
        scanned.len() >= 60,
        "本 crate 源码树下只扫到 {} 份 `.rs` —— 采集坏了（本件落地时 **72** 份：\n\
         树上 73 份，`NOT_A_PRODUCTION_FACE` 明写摘掉 1 份）。\n\
         🔴 这一条必须在：采集空了之后下面那条集合相等会红，但它会说\n\
         「登记表里有三族在源码树上找不到」—— 读起来像「有人删了三族」，\n\
         而真相是「尺子坏了」。**坏尺子会把真缺陷一起藏起来。**",
        scanned.len()
    );
    for (path, src) in scanned {
        let prod = crate::guard_support::production_side_of(&path, &src);
        if !prod.contains("const CAPABILITIES") {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push(rel);
    }
    out.sort();
    out
}

/// 排除名单 —— **在被扫的那棵树里、却不是一个生产能力面**的那些。
///
/// 🔴 每一条都要有理由，而且理由必须是「它**不是**一个能力面」，
/// 不是「把它算进来会红」。后者是改判据凑绿。
const NOT_A_PRODUCTION_FACE: &[&str] = &[
    // `agents/fake/` 整族是 `S6` 的**反向夹具**：一个假 agent，存在的唯一理由是让
    // 「把某一种能力删掉之后流程会不会在说得出话的地方停下来」变成常驻判据
    //（那份头注逐字）。它的 `CAPABILITIES` 是 12 条**中文的**知识名
    //（「会话记录根」「判活 cmdline」…），人群是 `agent_locality_guard::NEW_AGENT_BLOCKERS`。
    // ⇒ 它不是本后端对外声明的任何东西，汇进 `capability_ledger` 会让线上清单里
    //   凭空多出 12 条**没有任何线上面**的条目。
    "src/backend/agents/fake/mod.rs",
];

// ══════════════════ 承重断言一：汇总 == 被裁过的那份点名表（两向）══════════════════

/// 🔴🔴 **正题**：汇总之后那份清单，与 [`ROSTER`] 逐条相等（**双向差集空**）。
///
/// 这就是 `设计/96 §2` 第 2 层那条「`CAPABILITIES` 由它们汇总而来」的会红形态。
#[test]
fn the_ledger_is_exactly_the_roster_that_was_adjudicated() {
    let ledger: std::collections::BTreeSet<(String, String)> = capability_ledger()
        .into_iter()
        .map(|(f, n)| (f.to_string(), n.to_string()))
        .collect();
    let roster: std::collections::BTreeSet<(String, String)> = ROSTER
        .iter()
        .map(|(f, n)| ((*f).to_string(), (*n).to_string()))
        .collect();

    // ★ 反空真 ①：两侧都不许是空集 —— 空集对空集相等，而那读起来和真绿一样。
    assert!(
        !ledger.is_empty(),
        "汇总出来是空的 —— `CAPABILITY_FACES` 空了、或每个面的 `declares` 都交了空表"
    );
    assert_eq!(
        ROSTER.len(),
        roster.len(),
        "`ROSTER` 里有重复行 —— 去重之后 {} 条、原表 {} 条。\n\
         重复行会让「条数」这一维的地板恒真（抄一行就能把数凑上去）。",
        roster.len(),
        ROSTER.len()
    );

    let missing: Vec<_> = roster.difference(&ledger).collect();
    let extra: Vec<_> = ledger.difference(&roster).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "\n🔴 **汇总出来的清单与被裁过的那份点名表对不上。**\n\n\
         `ROSTER` 里有、汇总里没有（**能力被删/改名了，而消费方还当它在**）：{missing:?}\n\
         汇总里有、`ROSTER` 里没有（**多长出一条没人裁过的能力**）：{extra:?}\n\n\
         `设计/96 §2` 第 2 层逐字：「能力清单从实现派生，`CAPABILITIES` 由它们**汇总**而来，\n\
         不许手写」。汇总那一侧（[`crate::capability_ledger`]）是纯函数、改不了；\n\
         ⇒ 两条出路，**没有第三条**：\n\
         ① 那条能力本来就该加/该删 ⇒ 往 `ROSTER` 补/摘那一行，\n\
            **而补那一行的意思是「我看过这条能力，它该在清单里」**；\n\
         ② 那是一次误改 ⇒ 回去改那一族自己的声明表。\n\
         🚫 **不许**把 `ROSTER` 整张清空或换成从 `capability_ledger()` 算出来的东西 ——\n\
            那会让两侧同源，本条当场退化成恒真（本仓逐字「恒等两侧同源会恒真」）。"
    );
}

/// ★★ **反空真**：上面那条相等，必须**装得下** `files-read` 那一族，而且不是地板。
///
/// `files/mod.rs` 头注登记的缺口逐字是「**没有**把它汇进 `lib.rs::CAPABILITIES`」——
/// 本条就是那句话被填掉的凭据：那一族的**每一条**都要真的在汇总里。
#[test]
fn the_ledger_really_carries_the_files_read_family() {
    let ledger = capability_ledger();
    let names: std::collections::BTreeSet<&str> = ledger.iter().map(|(_, n)| *n).collect();
    let family_of: std::collections::BTreeMap<&str, &str> =
        ledger.iter().map(|(f, n)| (*n, *f)).collect();

    let declared = crate::files::capability_names();
    assert!(
        declared.len() >= 6,
        "`files::CAPABILITIES` 只声明了 {} 条 —— 本条在空转（本件落地时 6 条）",
        declared.len()
    );
    for n in &declared {
        assert!(
            names.contains(n),
            "`files-read` 声明的 `{n}` **不在汇总里** —— \
             `files/mod.rs` 头注登记的那个缺口还开着"
        );
        assert_eq!(
            family_of.get(n),
            Some(&"files-read"),
            "`{n}` 在汇总里挂在别的族名下"
        );
    }

    // ★ 反空真 ②：汇总不是「只有 files 那一族」—— 那样它就不是汇总，是一次转发。
    let families: std::collections::BTreeSet<&str> = ledger.iter().map(|(f, _)| *f).collect();
    assert!(
        families.len() >= 2,
        "汇总里只有 {} 个族（{families:?}）—— 「汇总」这个词至少要有两个东西可汇。\n\
         只有一族时，`设计/96 §2` 第 3 层那条跨 target 对等断言也无处可落。",
        families.len()
    );
    assert!(
        ledger.len() > declared.len(),
        "汇总 {} 条、`files-read` 自己 {} 条 —— 汇总没有装进任何别的族",
        ledger.len(),
        declared.len()
    );
}

/// ★ **每个能力名全局唯一** —— 否则「汇总」交出来的是一张会撞键的表。
///
/// ⚠ 这一条今天是**真在判事**，不是形式：`files-read` 的能力名带 `.`、线上名带 `-`
///（`files.ls` ↔ `files-ls`），而 `ccm-launcher` 那 18 条是裸词。
/// 两族哪天取了同一个词，下游按名字查就会拿到**另一族**的那一条。
#[test]
fn a_capability_name_belongs_to_exactly_one_face() {
    let ledger = capability_ledger();
    let mut seen: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for (family, name) in &ledger {
        seen.entry(name).or_default().push(family);
    }
    let dup: Vec<_> = seen.iter().filter(|(_, fs)| fs.len() > 1).collect();
    assert!(
        dup.is_empty(),
        "有能力名同时挂在多个面上：{dup:?}\n\
         ⇒ 按名字查汇总的下游会拿到「另一族的那一条」，而两边都报绿。"
    );
    assert_eq!(
        seen.len(),
        ledger.len(),
        "汇总里有重复的 `(族, 名)` 行 —— 某个面的 `declares` 交了重复项"
    );
}

// ══════════════ 承重断言二：源码树上的每一张能力表都是一个登记过的面 ══════════════

/// 🔴🔴 **这一条才是「汇总」本身的失效形态**：新长出一整族能力表，却没汇进来。
///
/// [`crate::capability_ledger`] 是对 [`crate::CAPABILITY_FACES`] 的纯函数
/// ⇒ 它**只汇总登记过的面**，对「有一族根本没登记」这件事结构上看不见。
/// ⇒ 那一格只能从**源码树**上现打，而那正是本条与上一条不同源的地方。
#[test]
fn every_capability_table_in_the_tree_is_a_registered_face() {
    let on_disk: std::collections::BTreeSet<String> =
        capability_table_files().into_iter().collect();
    let registered: std::collections::BTreeSet<String> = CAPABILITY_FACES
        .iter()
        .map(|f| f.declared_in.to_string())
        .collect();

    // ★ 反空真：源码树上一张能力表都没扫到 ⇒ 采集坏了（`files/mod.rs` 与 `lib.rs`
    //   两张是本件的前提，它们不可能不在）。
    assert!(
        on_disk.len() >= 3,
        "源码树上只扫到 {} 张 `const CAPABILITIES` 声明表（{on_disk:?}）—— 采集坏了。\n\
         本件落地时是 3 张（`lib.rs` / `files/mod.rs` / `control/ccm/mod.rs`；\n\
         `agents/fake/mod.rs` 那张由 `NOT_A_PRODUCTION_FACE` 明写摘掉）。",
        on_disk.len()
    );

    let unregistered: Vec<_> = on_disk.difference(&registered).collect();
    let phantom: Vec<_> = registered.difference(&on_disk).collect();
    assert!(
        unregistered.is_empty(),
        "\n🔴 **源码树上有能力声明表没汇进 `CAPABILITY_FACES`：**{unregistered:?}\n\n\
         这一形是**静默**的：那一族的能力照样能用、它自己的判据照样绿，\n\
         而 `设计/96 §2` 那份清单里没有它 ⇒ 第 3 层那条跨 target 对等断言\n\
         **对它整族全称成立**（空集），而空集上的全称命题恒真。\n\
         ⇒ 出路：往 `CAPABILITY_FACES` 加一行（顺带把它的 `targets` / `target_basis` 想清楚），\n\
         或者 —— 它真的不是一个对外声明的能力面的话 —— 往 `NOT_A_PRODUCTION_FACE` 上\n\
         写一条**说得出理由**的排除。🚫 理由不许是「算进来会红」。"
    );
    assert!(
        phantom.is_empty(),
        "\n`CAPABILITY_FACES` 登记的这几份文件在源码树上没有 `const CAPABILITIES`：{phantom:?}\n\
         ⇒ 登记挂空号：那个面的 `declares` 还在交名单，而声明表已经搬走/改形了\n\
         （`Capability::impl_files` 那条「登记挂空号」的同一形）。"
    );
}

// ══════════════════ 面自己那几栏：说了话，而且不是空话 ══════════════════

/// ★ **射程那两类各要有真成员** —— `设计/96 §2` 那条「清单的射程要能装下协议级能力」。
///
/// 只剩一类的时候，[`CapabilityKind`] 那个枚举就是装饰，而那条射程要求就是空话
///（同 `files::Effect` 那条「一个只有一个成员的枚举没法承载一条相等断言」）。
#[test]
fn both_halves_of_the_declared_reach_have_a_real_member() {
    assert_ne!(
        CapabilityKind::Asset,
        CapabilityKind::Protocol,
        "射程只有一类 —— 那条「装得下协议级能力」的要求退化成空话"
    );
    for want in [CapabilityKind::Asset, CapabilityKind::Protocol] {
        let faces: Vec<&str> = CAPABILITY_FACES
            .iter()
            .filter(|f| f.kind == want)
            .map(|f| f.family)
            .collect();
        assert!(
            !faces.is_empty(),
            "射程里的 `{want:?}` 这一类**一个面都没有**。\n\
             `设计/96 §2` 逐字：「清单的条目 ＝『我能管哪几类资产』＋『我认不认这条协议帧』\n\
             两类，第 2 层那条派生要把后者也派生进来；否则 `05 §8` 步 8 接上来的时候，\n\
             它要协商的东西在清单里找不到住址。」\n\
             ⇒ 这一类空着 = 那句话今天只是写在注释里。"
        );
        // 反空真：这一类下面的面必须真的声明了能力，不能是个空壳。
        let n: usize = CAPABILITY_FACES
            .iter()
            .filter(|f| f.kind == want)
            .map(|f| (f.declares)().len())
            .sum();
        assert!(
            n > 0,
            "射程里的 `{want:?}` 这一类下面 {faces:?} 一条能力都没声明"
        );
    }
}

/// ★ **每个面都要说自己在哪些 target 上有实现，而且要说依据**（第 2 层逐字）。
#[test]
fn every_face_declares_its_targets_and_says_on_what_basis() {
    assert!(
        !CAPABILITY_FACES.is_empty(),
        "`CAPABILITY_FACES` 是空的 —— 本条在空转"
    );
    for f in CAPABILITY_FACES {
        assert!(
            !f.family.is_empty(),
            "有一个面没有族名 —— 汇总出来的 `(族, 名)` 里族那一栏会是空串"
        );
        assert!(
            !f.targets.is_empty(),
            "面 `{}` 的 `targets` 是空的 —— `设计/96 §2` 第 2 层逐字要求\
             「每个能力面**声明自己在哪些 target 上有实现**」。\n\
             空表的语义是「一个平台都没有」，而它显然编得出来 ⇒ 这是漏填，不是声明。",
            f.family
        );
        // 🔴 依据不许留空，也不许是一句敷衍 —— 同 `files::Freshness::gap` 那条地板。
        //    这条地板挡的是「抄一个 `TARGETS` 上去」，`设计/96 §2.9` 管那叫**假声明**。
        assert!(
            f.target_basis.chars().count() >= 30,
            "面 `{}` 的 `target_basis` 只有 {} 个字符 —— 太短，等于没说。\n\
             `设计/96 §2.9` 逐字：判成相等会**逼人写假声明**。这一栏就是防那一形的：\n\
             写不出依据的话，那个 `targets` 是抄上去的。",
            f.family,
            f.target_basis.chars().count()
        );
        // 逐条 target 必须是全体里的一员（今天全体只有四个，写错一个编不过 ——
        // 但 `targets` 可以是任意子集，所以这一条判的是「不许出现全体之外的东西」，
        // 它今天由类型保证；留着的是下面那条**不许有重复**）。
        let mut sorted = f.targets.to_vec();
        sorted.sort_unstable();
        let n = sorted.len();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            n,
            "面 `{}` 的 `targets` 里有重复的 target —— 「在几个平台上有实现」这个数会被虚报",
            f.family
        );
        for t in f.targets {
            assert!(
                TARGETS.contains(t),
                "面 `{}` 声明了一个不在 `TARGETS` 里的 target `{t:?}`",
                f.family
            );
        }
    }
}

/// 🔴 **`lib.rs::CAPABILITIES` 那一处的语义一个字没被汇总改动。**
///
/// # 它治的是哪一形（`files/mod.rs` 头注逐字点过这件事）
///
/// 那份头注逐字：汇总没接的理由是「`lib.rs::CAPABILITIES` 那一处的语义今天是
/// **会在一次性查询判定前剥离对应 flag 的流能力**，本族六条都不是那种东西，
/// **硬塞进去会当场红，而且会是红对了**」。
///
/// ⇒ 步 `8a` 的做法是把那一处降格成汇总里的**一个面**（`stream-flags`），
/// 而**不是**把别族的能力塞进它。本条钉住那件事没有偷偷发生：
/// 那张表里每一条都必须仍然是一个 `stream-flags` 面的成员，
/// 而 `§26` 那条护栏（`every_capability_token_is_strippable`）继续管它自己。
///
/// ⚠ 本条**不重判** `§26`（那是 `main_stream_flag_tests` 的活，一个判定只有一个家）。
/// 它判的是**归属**：`CAPABILITIES` 的内容没有被汇总这件事污染。
#[test]
fn the_stream_flag_list_keeps_its_own_narrow_semantics() {
    assert!(
        !CAPABILITIES.is_empty(),
        "`lib.rs::CAPABILITIES` 是空的 —— 本条在空转"
    );
    let ledger = capability_ledger();
    for token in CAPABILITIES {
        assert!(
            ledger.contains(&("stream-flags", token)),
            "`lib.rs::CAPABILITIES` 里的 `{token}` 在汇总里不挂 `stream-flags` —— \
             那一处的语义被改了"
        );
    }
    let face: Vec<&str> = ledger
        .iter()
        .filter(|(f, _)| *f == "stream-flags")
        .map(|(_, n)| *n)
        .collect();
    assert_eq!(
        face,
        CAPABILITIES.to_vec(),
        "\n`stream-flags` 这个面交出来的名单与 `lib.rs::CAPABILITIES` 对不上。\n\
         🔴 **最要紧的失效方向是「多」**：有人把另一族的能力名塞进了那张表。\n\
         那张表的语义是「会在一次性查询判定前**剥离对应 flag**」（`§26` 死循环护栏），\n\
         塞一条没有 flag 的进去 ⇒ `every_capability_token_is_strippable` 当场红，\n\
         **而且是红对了**。汇总的正确接法是给它加一个**面**，不是往它里面塞。"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 逐能力豁免表（`TARGET_GAPS`）—— 2026-09-21 用户把本轴改判成「做得到」之后新立
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **豁免表里不许有幽灵** —— 每一条都要指着一个**真声明过**的能力。
///
/// 没有这一条，一张写错名字的豁免会**永远命中不了**，而它看起来像「这件事登记过了」。
/// 那正是本仓「不许留用不上的豁免」那条纪律要挡的形。
#[test]
fn every_target_gap_names_a_capability_that_really_exists() {
    // 〔PR1〕人群从第 2 层汇总扩到第 3 层那两张面表（命令面的豁免也在这张表里）。
    let ledger = parity_declared();
    let mut bad = Vec::new();
    for gap in TARGET_GAPS {
        let found = ledger
            .iter()
            .any(|(family, name)| *family == gap.family && *name == gap.capability);
        if !found {
            bad.push(format!("{} / {}", gap.family, gap.capability));
        }
    }
    assert!(
        bad.is_empty(),
        "豁免表里这几条指着**不存在**的能力：{bad:?}\n\
         ⇒ 一条指空的豁免永远命中不了，而它看起来像「这件事登记过了」。\n\
         改法：要么把名字改对，要么把那一行删掉（能力没了，豁免也该没）。"
    );
}

/// 🔴 每一条豁免都要答两件：**今天为什么做不到** ＋ **将来怎么办**。
///
/// 「暂时不做」是合法答复（用户 2026-09-21 逐字「**除非暂时不做**」），
/// 但**必须写出来** —— 不写就分不清「想过了、决定推后」与「压根没想」。
#[test]
fn every_target_gap_says_why_and_what_happens_next() {
    for gap in TARGET_GAPS {
        let why = gap.rationale;
        assert!(
            why.chars().count() >= 24,
            "`{} / {}` 那条豁免的理由只有 {} 个字 —— 太短，答不出「为什么」＋「将来怎么办」两件事。\n\
             原文：{why:?}",
            gap.family,
            gap.capability,
            why.chars().count()
        );
        // 「将来怎么办」那一半要有形状：要么指出替代方案，要么明写推后。
        let answers_future = ["暂时不做", "不该跨过去", "将来", "等那个", "后面"]
            .iter()
            .any(|k| why.contains(*k));
        assert!(
            answers_future,
            "`{} / {}` 那条豁免只说了「做不到」，没说**将来怎么办**。\n\
             ⇒ 「暂时不做」也算答复，但要写出来 —— 不写就分不清\
             「想过了、决定推后」与「压根没想」。原文：{why:?}",
            gap.family, gap.capability
        );
    }
}

/// 🔴 **反空真的锚：一张「什么都豁免」的表等于没有表。**
///
/// 被豁免的那一面，**必须还有真做得到的能力留在那个 target 上**。
/// 否则正确的做法是把整面从那个 target 上摘掉，而不是逐条豁免 ——
/// 那两件事在读数上长得一样（都是「这个 target 上做不到」），
/// 而**逐条豁免**这个形状之所以成立，靠的就是「有一部分真的做得到」。
///
/// ⚠ 同时钉住条数（现打 **6**）。这个数会随裁决动，动了就重新量、连理由一起改，
/// **不许为了绿把它算出来**（两侧同源就退化成恒真）。
#[test]
fn the_gap_table_never_exempts_a_whole_face_and_its_size_is_pinned() {
    // 〔PR1〕同上：命令面的豁免也在表里，`all` 要从第 3 层那两张面表里取，
    // 否则命令面那几族的 `all.len()` 是 0，下面那一减会下溢。
    let ledger = parity_declared();
    let gaps = TARGET_GAPS;

    assert_eq!(
        gaps.len(),
        16,
        "逐能力豁免现打 {} 条（〔FW5 · 09-24〕**16** = PR1 那 14 条 ＋ `files-chmod` / `--files-chmod` × Windows 2 条\n\
          —— 那条命令声明了 `no_unix_mode` 之后被现推出来，档 = 结构，理由住 `lib.rs` 表尾。\n\
         〔PR1 · 09-24〕**14** = 下面那 8 条 ＋ 命令面 6 条：\n\
          · 帧面 `capture-pane` / `kill` / `launch` × Windows 3 条、CLI 面同名 3 条 ——\n\
            不是新裁的，是命令面并进第 3 层之后被横向两向相等**现推出来**的，理由住 `lib.rs` 表尾。\n\
         2026-09-22 现打 8，全在 `ccm-launcher` × Windows：\n\
          · 6 条 = tmux 那一族，**读源码**推出来的；\n\
          · 2 条 = `bus-register` / `ccm-sid`，**真机现打**补的\n\
            —— 上一版的账把它们记成「做得到」，那两格是**错的**不是缺的，\n\
            逐条读数住 `真相源/106`）。\n\
         🔴 `P19`（09-22）把这个数从 **9** 减到 8：`agent` 那一条删了 —— 它那句「做不到」\n\
         的根因（`needs_bus_id(\"codex\")` 恒真 ⇒ 整条改走 `sh -c`）在源码里没了。\n\
         ⚠ **那一减不是真机复验来的**（Win11 虚拟机本轮没动）：对价是 `CAPABILITY_FACES`\n\
         那一栏 `target_basis` 里逐字写着的「🚫 买不到的那一维」，删那一行之前先读它。\n\
         这个数本身没有对错，但它变了说明有裁决动过 —— 连理由一起看。",
        gaps.len()
    );

    // 🔴 反空真：逐（面, target）看，被豁免的那一格必须还留着真做得到的能力。
    let mut faces: std::collections::BTreeSet<(&str, Target)> = Default::default();
    for g in gaps {
        faces.insert((g.family, g.target));
    }
    assert!(
        !faces.is_empty(),
        "豁免表是空的，而本条正题是「不许整面豁免」⇒ 它会零命中地绿。\n\
         今天不该是空的（`ccm-launcher` × Windows 那一族）—— 真清零了就连本条一起重判。"
    );
    for (family, target) in faces {
        let all: Vec<&str> = ledger
            .iter()
            .filter(|(f, _)| *f == family)
            .map(|(_, n)| *n)
            .collect();
        let gapped: Vec<&str> = gaps
            .iter()
            .filter(|g| g.family == family && g.target == target)
            .map(|g| g.capability)
            .collect();
        let left = all.len() - gapped.len();
        assert!(
            left > 0,
            "`{family}` 在 {target:?} 上**每一条**能力都被豁免了（{} 条全中）。\n\
             ⇒ 那不是「逐条豁免」，那是「整面在这个 target 上做不到」——\
             正确的写法是把 {target:?} 从那一面的 `targets` 里摘掉，而不是列满一张豁免表。\n\
             **两者在读数上长得一样，而只有一个说的是真话。**",
            all.len()
        );
        assert!(
            left < all.len(),
            "`{family}` × {target:?} 一条都没豁免，却出现在上面那个集合里 —— 本条的取集合那一步坏了"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// 〔PR1 · 2026-09-24〕`设计/96 §2` 第 3 层：四个 target 横向对等 ＋ 差异两档 —— 住隔壁那份
// ═══════════════════════════════════════════════════════════════════
#[path = "target_parity_guard.rs"]
mod target_parity_guard;
