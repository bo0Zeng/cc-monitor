//! F11：**耐久文档里「描述当下」的那些字段，与代码对拍。**
//!
//! # 病：文档寿命比「当下」长
//!
//! F07 顺出的一般化，F04b/F04c 各自又验证一次：**「状态列」与「实测答案」是耐久文档里
//! 最易腐的两种字段** —— 它们描述的是**当下**，而文档的寿命比「当下」长得多。
//! 六轮加总已经订正了 **14 处**，而其中最险的一处（`INVARIANTS §A5`「kill 无此白名单」）
//! **自 F04 起就假了**、连着好几轮没人发现。
//!
//! ⚠ 更要紧的是 **F07 自己就是这个病的受害者**：它订正了 §33b 三问的答案 ①，
//! 却漏了**同一节里 11 行之前那一行说着同一句话的单元格**（`control/launch.rs`
//! 「零生产调用方」）。**订正手头那一处，不等于订正那句话。**
//! 那与 F01 的四处「每 ~8s」是同一个病，只是这次犯在「订正」这个动作上。
//!
//! # 处置不是「以后记得更新」，是**把那个数搬回文档、让判据去读它**
//!
//! 本模块的核心手法：**判据不自己写那个数** —— 它从文档里把数抽出来，再与现场量的比。
//! 于是那个数**只有一个家（文档）**，而「文档与现实对不上」变成一条会红的机检。
//! 这同时满足定框 §4 那条「同一个数不许两侧各写一份」：代码里没有第二份。
//!
//! # 扫描面为什么是「状态列」而不是「所有实测句」
//!
//! 实测（F11 摸底）：`doc/` 十一个文件 4158 行里，**表头含「状态」的表只有一张、六行**
//! （`INVARIANTS.md §33b`）—— 可枚举、可穷尽、后果最重（它是「下一个执行 U8c-3 的人
//! 唯一会读的依据」）。而「实测句」那一族有 **63 句**、绝大多数是散文，
//! **钉不住**（见 §诚实边界）。⇒ 状态列**逐格登记**，可数的实测断言**挑出来登记**，
//! 其余如实记为诚实边界。

/// 耐久文档里**每一个**「状态列」单元格 → 它的**现场量法**。`(件名, 量法键)`。
///
/// # ⚠ 这里**刻意不存「文档写的状态」**
///
/// 第一版存了 —— 于是 `STATUS_CELLS` 成了文档那一列的**第二份副本**，而两份副本必漂：
/// **E4 变异（把文档里 `U8c-3` 的「待做」改成「已交付」）时五条判据全绿**，
/// 因为判据比的是「登记表里那份副本 ↔ 现场」，文档那份根本没参与。
///
/// ★ 那正是本模块开头声称要治的病，我自己在同一个文件里又犯了一次 ——
/// **而且只有变异复验能发现**（基线全绿时它看起来完全正常）。
/// ⇒ 状态**只从文档里读**，这里只留「怎么量」。判据 = 文档说的 ↔ 现场量的。
#[cfg(test)]
const STATUS_CELLS: &[(&str, &str)] = &[
    ("U8c-1", "payload-kernel-exists"),
    ("U8c-2a", "usage-probe-uses-the-kernel"),
    ("U8c-2b-0", "posix-quote-has-one-home"),
    ("U8c-2c-1", "ccm-invocation-kernel-exists"),
    ("U8c-2c-2", "production-ts-calls-the-rust-renderers"),
    ("U8c-3", "ts-renderer-still-there"),
    // 〔F19〕`doc/ARCHITECTURE.md` §2.1 的「backend 四层在 monitor 侧落地到哪一步」表。
    // ⚠ 它是**本条判据族第一次被一张新表触发**：F19 往 ARCHITECTURE 写下这张表时，
    // `the_doc_scan_actually_reads_the_durable_docs` 当场红（表张数 1 → 2），
    // 逼着这四格各配一条现场量法 —— 那正是这条判据存在的目的。
    ("`control/`", "monitor-backend-control-landed"),
    ("`observe/`", "monitor-backend-observe-landed"),
    ("`platform/`", "monitor-backend-platform-landed"),
    ("`common/`", "monitor-backend-common-landed"),
];

// ═════════════════════════════════════════════════════════════════════════════
// `K-R73` `KR73D3`：**「量法与它声称的性质对不上」在本模块里数一遍**
// ═════════════════════════════════════════════════════════════════════════════
//
// # 为什么要在这里数
//
// 本模块头注自己记着这个病的三次发作（F01 的四处「每 ~8s」· F07 漏同节 11 行前那格 ·
// `K-P5f` 漏 `INVARIANTS.md` 并新写第五份副本）。`DECISIONS.md#R29` **裁定零**是第四次，
// 而且犯在「量法」上：`control/` 那格上方逐字警告「刻意不是裸 `is_dir`」，
// 而**紧接着的三格全是裸 `is_dir()`** —— 写下警告的人在下面三行里连犯三次。
//
// ⇒ 这张表是那次裁定要的**普查**：`STATUS_CELLS` 每一格的现场量法，
// 与它那格声称的话**对不对得上**，对不上的**逐条点名**。
// 🔴 **不要求本件全修**（改一条量法就是动一条判据的射程，那要各自论证）——
// 要求的是**数得出、点得名**，并且这张表**不许悄悄少一行**。
//
// # 两条穿过全表的读数（分母都是 `STATUS_CELLS` 的 10 格）
//
// ① **5 条**量法是「读一份文件、在里面找一根针」。**只有 1 条断言了那份文件存在**
//    （`posix-quote-has-one-home`，F12 那次 `/full-audit` 逮到之后补的）。
//    另外 4 条读不到文件时只会**静默返回空串** —— 那正是 F12 那条订正注释里逐字写的形。
//    ⚠ 说清它今天为什么还不是假绿：那 4 格的文档状态都是「已交付」⇒ 文件没了 ⇒ 量法 `false`
//    ⇒ 与「已交付」对不上 ⇒ **红**。它们**fail closed**，但报错文案会把人指向
//    「事实前进了而文档没跟」，而真相是「量法读的那份文件不见了」。
//    **哪天某一格翻成「待做」，这一支就从「诊断误导」变成「静默同意」。**
// ② **那 5 条针全是裸子串**（`contains("fn render_payload")` 这一形）——
//    `needle_anchor_registry` 头注治的正是这个族（「匹配单位比事实小」，本仓实测四次
//    都是把 needle 撑大就照样绿）。⇒ **F12 的教训补在了 1/5 处，F24 的教训一处都没补。**
//
// # 这张表**不是**要求一个总数
//
// 「报一个总数而不逐条点名」是本区反复抓的那一形。⇒ 下面 10 行，一格一行，
// 每行自带「它到底量了什么、与那句话差在哪」。棘轮只是**别再长**，不是这张表的正题。

/// 一条现场量法的**形状**。
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MeasureShape {
    /// 钉住**那个唯一的住户**：那份文件在不在。
    PinsTheOneResident,
    /// 读**剥过注释的生产段**、在里面找一根裸子串针。
    ProdNeedle,
    /// 读**没剥过注释的原文**（TS / MD）找针 ⇒ 把那句话注释掉，它照样命中。
    RawTextNeedle,
    /// 反向：那样东西**不在**才算交付。
    AbsenceIsTheClaim,
    /// 这条能力线里**真有住户**（`K-R73` 把两格裸 `is_dir()` 收窄成的形）。
    LineHasResidents,
}

/// 这条量法与它那格声称的话**对不对得上**。
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verdict {
    /// 对得上：量的就是那句话说的那件事（可能仍有登记在案的代价，但代价不是「量错了东西」）。
    Holds,
    /// **对不上**：量到的比那句话说的**宽**（或**窄**），下面那一栏逐条写清差在哪。
    FallsShort,
}

/// **普查表**：`STATUS_CELLS` 每一格的现场量法 → `(量法键, 形状, 判定, 差在哪 / 代价是什么)`。
///
/// 🔴 **一格一行，两个方向都对拍**（见 [`tests::every_status_cell_measure_is_in_the_census`]）：
/// `STATUS_CELLS` 新加一格而这里没跟 ⇒ 红；这里留着一行而那边没了 ⇒ 也红。
/// 少了这一条，这张表会跟本模块治的那些文档副本一样腐。
#[cfg(test)]
const MEASURE_CENSUS: &[(&str, MeasureShape, Verdict, &str)] = &[
    (
        "payload-kernel-exists",
        MeasureShape::ProdNeedle,
        Verdict::FallsShort,
        "声称「载荷内核在 `backend/control/payload.rs`」，量的是生产段里有没有子串          `fn render_payload` ⇒ **`fn render_payload_v2` 之类以它打头的名字照样命中**         （`needle_anchor_registry` 治的那一族）。另**没有断言那份文件存在** ——          读不到只会静默返回空串",
    ),
    (
        "usage-probe-uses-the-kernel",
        MeasureShape::ProdNeedle,
        Verdict::FallsShort,
        "声称「用量探针在调 `usage_probe_payload` 入口」，量的是裸子串 ⇒ 入口改名成         以它打头的另一个名字时照样绿。⚠ 这一格已经**被自己红过一次**并改对了标的         （原先量 `render_payload`），改的是「量哪一个」，没改「用什么单位量」。         另无存在性断言",
    ),
    (
        "posix-quote-has-one-home",
        MeasureShape::ProdNeedle,
        Verdict::FallsShort,
        "本模块**唯一**断言了「量法读的那份文件存在」的一格（F12 那次 `/full-audit`          逮到左支读一个不存在的文件、恒 `false` 之后补的）。⇒ F12 那半的教训补上了；         **F24 那半没有** —— 针仍是裸子串 `shell_quote_core::posix_quote`",
    ),
    (
        "ccm-invocation-kernel-exists",
        MeasureShape::ProdNeedle,
        Verdict::FallsShort,
        "与 `payload-kernel-exists` 逐字同形：裸子串 `fn render_ccm_invocation` ＋          无存在性断言。**同一个形状在本表里出现三次**，说明它不是某一格的疏忽",
    ),
    (
        "production-ts-calls-the-rust-renderers",
        MeasureShape::RawTextNeedle,
        Verdict::FallsShort,
        "🔴 **本表唯一一条今天就用一刀验过的**：它声称「**生产** TS 主路在调那两条 Rust \
         渲染命令」，而量法用的是 `read()` 而不是 `prod()` ⇒ **没剥注释**。\
         09-12 现打：把 `src/remote-launch-run.ts` 里那 3 行调用**整行注释掉**，\
         本模块 19 条判据 **一条不红**。\
         ⚠ **别把这条读成「那个性质没人守」** —— 真正接住它的是 \
         `backend/control/launch_wire.rs` 里那条生产接线钉（同一刀下它**当场红**，\
         最小面 1）：那一条走 `production_ts()`，行首与行尾注释都剥，\
         头注逐字写着「行尾注释里的提及不算数」。\
         ⇒ 本模块这一格是**同一个事实的第二份、而且更弱的那一份**，\
         它的害处不是漏守，是**让人以为这一格自己有牙**。\
         ⚠ 修法不是「TS 也走 `production_code`」（那份剥法是按 Rust 的 `#[cfg(test)]` 写的），\
         要么复用那条已有的 TS 剥法，要么把这一格摘掉、指向那条判据 —— 两条都要论证，另开一件",
    ),
    (
        "ts-renderer-still-there",
        MeasureShape::AbsenceIsTheClaim,
        Verdict::Holds,
        "反向量法：`src/session-backend.ts` **不在**才算交付，与那格声称的话对得上。         ⚠ 登记一处**可读性陷阱**（不是洞）：键名逐字是「still-there」，而它为 `true` 时         的意思是「**已经删了**」—— 键名与布尔方向相反，读的人容易读反",
    ),
    (
        "monitor-backend-control-landed",
        MeasureShape::PinsTheOneResident,
        Verdict::Holds,
        "钉住那个唯一的回落分流器在不在。**代价登记在案**：它钉的是那**一个文件名** ⇒          住户改名或再搬家时会**假红**（红得对不对要人判）。这与 `observe/` 那格是同一个代价",
    ),
    (
        "monitor-backend-observe-landed",
        MeasureShape::PinsTheOneResident,
        Verdict::Holds,
        "〔`K-R71` 09-12 按 `R29` 裁定一从裸 `is_dir()` 收窄成钉住那个唯一的读面传输〕。         代价与 `control/` 那格逐字相同。⚠ **`K-R73` 一个字都没动它**",
    ),
    (
        "monitor-backend-platform-landed",
        MeasureShape::LineHasResidents,
        Verdict::Holds,
        "〔`K-R73` 09-12 按 `R29` 裁定二从裸 `is_dir()` 收窄〕。**不许硬指住户**：         这条线今天没有那个唯一的住户，指一个就是替未来的人做决定 ⇒ 量的是         「目录在 ∧ 里面真有住户」，空壳目录直接红。代价见那份量法的头注",
    ),
    (
        "monitor-backend-common-landed",
        MeasureShape::LineHasResidents,
        Verdict::Holds,
        "与 `platform/` 那格逐字同形。⚠ 这条线的文档那一格逐字写着「**刻意不建**：         monitor 侧的共用面住 `src-tauri/crates/*`」⇒ 它大概率**永远**停在这一支，         而空壳目录那条红正是为它准备的",
    ),
];

// ═════════════════════════════════════════════════════════════════════════════
// `K-R105` `KR105D1`：**`INVARIANTS §33b` 那三问的答案，也是「描述当下」的字段**
// ═════════════════════════════════════════════════════════════════════════════
//
// # 为什么它非补不可：本模块头注点名的那次事故，就是这三问
//
// 头注逐字记着「**F07 自己就是这个病的受害者**：它订正了 §33b 三问的答案 ①，
// 却漏了同一节里 11 行之前那一格」。⇒ 那一拍之后，本模块给**那一格**配了机检
//（[`tests::the_doc_number_for_production_launch_calls_matches_reality`]），
// **却始终没给三问的答案本身配一条** —— 于是它们又腐了两轮：
//
// - **③**：`K-R59`（09-11，定框 `K35`）把 `daemonless` 整档删了 ⇒ 第三问的前提不存在了。
//   那一拍的订正**只落在 08-14 那张复裁表的 ③ 行里**，08-04 三问表里的 ③ 行原封不动。
// - **①**：`K-P2` `D3`（09-03）把 `ccm` 那条路接到了后端的一次性口上 ⇒
//   「起会话这一格」在**那棵树上**切过去了。`launch_wire.rs` 里逐字记了这件事
//   （「三问的答案① 变了」），而 §33b 的表**一个字没动**。
//
// ⇒ 两次都不是「没人知道」，是「**知道的人写在别处**」。本组判据买的就是这一件事：
//   **改一问所依赖的行为而不改那问的答案 ⇒ 当场红。**
//
// # 手法：与 `STATUS_CELLS` 逐字同形 —— 表里**不存答案**，只存「怎么量」
//
// 答案是一整段散文，机器比不了。⇒ 每一问在文档里必须带一个**判词**
//（`〔现打…〕` 那一族），判词的**闭集**写在下面这张表里，**由机器挑一个**，
// 再断言文档里出现的正是它、且**别的判词一个都不许出现**。
// 散文怎么写不管（不做语义审查），但那个判词必须与现场一致。
//
// ⚠ 诚实边界，写出来别读大：它钉的是**判词**，不是那段散文。
// 一段与判词相符、其余全说反了的答案，本条静默。它买到的是
// 「三问的答案**不会静默地过期**」，不是「答案写得对」。

/// §33b 三问 → `(问号, 那一问的判词闭集)`。**判词由机器挑，本表不存答案。**
///
/// 闭集第一个元素是「什么都没发生」那一档，最后一个是「这一问可以放行了」那一档 ——
/// 顺序**不承重**（判据按相等比），写成这样只是给读的人一个方向感。
#[cfg(test)]
const THIRTY_THREE_B_QUESTIONS: &[(&str, &[&str])] = &[
    (
        "① 生产切到 daemon 的 launch 了吗",
        &["〔现打①〕一格没切", "〔现打①〕部分切", "〔现打①〕全切"],
    ),
    (
        "② attach 那条串归谁产",
        &["〔现打②〕前端仍产 attach", "〔现打②〕后端全产 attach"],
    ),
    (
        "③ daemonless 的远端还要不要能起会话",
        &["〔现打③〕那一档还在", "〔现打③〕已退役"],
    ),
];

/// **对不上那一栏的递减棘轮**（09-12 现打 **5** 条，全部是「读文件找针」那一支）。
///
/// 🔴 **只许降。** 修好一条就把这个数调下来，**不许调上去让今天好过**。
/// ⚠ 它只挡「别再长」，**不代表这 5 条已经排期** —— 排期是另一件事，
/// 而把「已知的欠账」和「有人在还」混成一句话，正是本模块治的那个病。
#[cfg(test)]
const FALLS_SHORT_CEILING: usize = 5;

// ═════════════════════════════════════════════════════════════════════════════
// `K-P5g` `KP5GD3`：**一句话散在好几处** —— 本模块头注那个病的第三次发作
// ═════════════════════════════════════════════════════════════════════════════
//
// # 病史（三次，一次比一次贵）
//
// 本模块头注逐字记着前两次：F01 的四处「每 ~8s」；F07 订正了 §33b 三问的答案 ①、
// 却漏了**同一节里 11 行之前**那个说着同一句话的单元格 ——
// 「**订正手头那一处，不等于订正那句话**」。
//
// 第三次是 `K-P5f`（09-02）：`/proc/<pid>/environ` 从读一个环境变量变成读两个，
// 那一拍把「读几个」这句话改对了三处、**漏了 `doc/INVARIANTS.md` 那一处**，
// 而且同一拍还给这句话**新写了第五份副本**（`accounts_query.rs` 里那条判据的头注）。
//
// # 🔴 为什么那句假话活得下来：**它不在任何一张登记表里**
//
// `K-P5g` 派工前现打：`STATUS_CELLS` 登记 11 格，针 `environ` / `CLAUDE_CONFIG_DIR`
// 在整个 `doc_claim_registry.rs` 里 **0 命中** ⇒ 改那句话不会打破任何闸，
// 于是它只能靠「下一个人记得来改」活着 —— 而那正是本模块开头声称要治的病。
//
// ⇒ **本组判据不是来补那一处的**（补一处是 `KP5GD2` 的活，一次性）。
//   它买的是：**这句话再多一份副本、或哪份副本对不上现场，都会有东西变红。**
//
// # 手法：人群**扫出来**，登记表只说「这一份是哪一类」
//
// 与 `STATUS_CELLS` 同形（那张表刻意不存文档写的状态，只存「怎么量」）：
// 这里也**不存那个计数词**，只存「这一份属于哪一类」。计数词从**生产代码**里数出来
// （daemon 侧真读了几个环境变量），再与每一份副本上写着的对拍 ⇒ 那个数只有一个家。
//
// ⚠ 分类是必需的，不是偷懒：盘上确实有几份副本**逐字带着旧说法**，
// 因为它们是在**引用那句假话本身**（病史 / 判据失败文案）。
// 词法扫描分不开「在断言」与「在引述」，那一格只能由人裁 —— 那就是这张表存在的理由。
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EnvKeyClaim {
    /// **断言当下**：这一份真的在说「今天从进程环境里读几个」。⇒ 过计数词对拍。
    Asserts,
    /// **在引述那句话本身**（病史 / 判据文案）。它逐字带着旧说法是**故意的**，不判计数词。
    Quotes,
    /// 同一句式，但**主语不是进程环境**（说的是别的抽取器抠出几样东西）。不判计数词。
    OtherSubject,
}

/// 「从 `/proc/<pid>/environ` 读几个」这句话在盘上的**每一份副本** → 它是哪一类。
///
/// `(仓相对路径, 那一行里的一段锚点原文, 类别)`。
///
/// ⚠ **锚点刻意避开扫描器用的那两个字**，于是本文件自己**不会**成为它所描述的人群的一员
/// （`the_registry_file_itself_stays_out_of_that_population` 钉着这条性质）。
#[cfg(test)]
const ENV_KEY_CLAIM_SITES: &[(&str, &str, EnvKeyClaim)] = &[
    // ── 断言当下的那几份 ──────────────────────────────────────────────────
    // 🔴 `K-P5f` 漏的就是这一份：铁律那一节，全树寿命最长的文档。
    (
        "doc/INVARIANTS.md",
        "绝不回传整个环境快照",
        EnvKeyClaim::Asserts,
    ),
    (
        "doc/IPC-PROTOCOL.md",
        "`--session-accounts [--accts-dir <p>]`",
        EnvKeyClaim::Asserts,
    ),
    (
        "remote-daemon-proto/src/observe/accounts_query.rs",
        "//! - `/proc/<pid>/environ`",
        EnvKeyClaim::Asserts,
    ),
    // `K-P5f` 同一拍新写的第五份 —— 它自己就是「订正的同时又添一份副本」的活证据。
    (
        "remote-daemon-proto/src/observe/accounts_query.rs",
        "守的性质：",
        EnvKeyClaim::Asserts,
    ),
    (
        "src-tauri/src/local_accounts.rs",
        "、`configDir` 过白名单",
        EnvKeyClaim::Asserts,
    ),
    // 发版说明里那一份（`3.7.0` 起草，09-09）。**它在断言当下，不是在引述**：
    // 用户读发版说明是为了知道「装上这一版之后，这东西今天做什么」——
    // 那正是 `Asserts` 的定义，而不是病史 / 判据文案那一类。
    //
    // 病史（本族第四次发作，而这一次的传播路径是新的）：起草时照抄了 `E79`
    // 那条提交正文（07-31）的说法，而第二个键是 `K-P5f`（09-02）加的
    // ⇒ 草稿把一句已经过期的话，抄进了**用户读的第一份文档**。
    // 前三次都是「改了这一处、漏了那一处」，这一次是「从一处过期的原文里再生一份副本」——
    // 副本的来源可以是任何一份旧文，扫描面因此必须是全仓，不是几个目录。
    //
    // 为什么不是 `Quotes`（PM 初判如此，实现方顶回，理由如下）：
    // 登记成 `Quotes` 今天**会绿** —— 草稿写的计数词与现场那个数不等，
    // 反洗白那一格只在「数正好对上」时才报，所以它会静静放行。
    // 那是最坏的一种绿：一条判据替一句假话背书，而这句假话住在发版说明里。
    // 登记成 `Asserts` 则把它钉在生产代码那个数上，往后再变一次就红。
    //
    // 挑锚点的硬约束（09-09 撞过，两条门禁同拍红，下一个来加锚点的人照此办）：
    // **锚点是拿来被匹配的串，不是拿来复述内容的** ⇒ 里面不许出现别的判据正在数的字面量。
    // 第一版锚点把原句里那两个环境变量名照抄了进来，于是
    // `launcher_identity_registry`（数「身份那个变量名在 `src-tauri/src` 生产段里几处」，期望 1）
    // 与 `local_read_surface_registry`（数「本机读面每份文件几行」）**双双多算一处**。
    //
    // 病灶**不在注释**（现打核过）：`production_code` 先剥块注释、再整行滤掉 `//` 开头的行，
    // 所以上面这几段解释一个字都进不了那两条判据的语料。真正的成因是本表**住在顶层、
    // 只挂了一个测试期属性**，而 `test_module_ranges` 只剥「那个属性紧跟着一个带花括号体的
    // 测试模块」那一形 ⇒ 本表的**字符串字面量一律被当成生产段**。
    // （本文件那个测试模块**里面**的字面量剥得掉 —— 同文件别处几处带本机数据目录名的串就在
    //  里面、从来没被数进去过；剥不掉的恰恰是模块**外面**的这三张表。这个不对称是全部成因。）
    // ⇒ 这张表里的每一个锚点，都要按「它会被全仓的字面量计数判据看见」来挑。
    (
        "CHANGELOG.md",
        "绝不回传整个环境快照 —— 那里面有用户全部的密钥类环境变量",
        EnvKeyClaim::Asserts,
    ),
    // ── 在引述那句话本身的那几份（逐字带着旧说法是故意的）────────────────
    (
        "remote-daemon-proto/src/observe/accounts_query.rs",
        "文档那一行把",
        EnvKeyClaim::Quotes,
    ),
    (
        "remote-daemon-proto/src/observe/accounts_query.rs",
        "那句诚实边界：文档与本文件头注",
        EnvKeyClaim::Quotes,
    ),
    (
        "remote-daemon-proto/src/observe/accounts_query.rs",
        "里没点名",
        EnvKeyClaim::Quotes,
    ),
    (
        "src-tauri/src/local_accounts.rs",
        "不同拍改它就是在盘上留一句假话",
        EnvKeyClaim::Quotes,
    ),
    // ── 同句式、别的主语 ──────────────────────────────────────────────────
    (
        "remote-daemon-proto/src/observe/accounts_query.rs",
        "从出参 `json!`",
        EnvKeyClaim::OtherSubject,
    ),
];

#[cfg(test)]
mod tests {
    use super::{
        EnvKeyClaim, Verdict, ENV_KEY_CLAIM_SITES, FALLS_SHORT_CEILING, MEASURE_CENSUS,
        STATUS_CELLS, THIRTY_THREE_B_QUESTIONS,
    };
    use std::path::{Path, PathBuf};

    const INVARIANTS: &str = include_str!("../../doc/INVARIANTS.md");

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    /// `doc/` 下**递归**收 `.md`。
    ///
    /// 〔audit-0805 08-06〕原来用非递归 `read_dir` —— 今天 `doc/` 恰好是平的（11 份、零子目录），
    /// 所以那不是活缺陷；但**新建一个 `doc/design/` 就整目录隐形**，而且不会有任何信号。
    fn doc_files() -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = Vec::new();
        let mut stack = vec![repo_root().join("doc")];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "md") {
                    v.push(p);
                }
            }
        }
        v.sort();
        v
    }

    /// 全仓的入口 `README*.md`（**派生，不是手写清单**）。
    ///
    /// # 〔audit-0805 08-06〕这张表原来是我手写的七条，而仓里有八份
    ///
    /// 少的那一份是 **`README.en.md`** —— 于是英文入口文档里的符号引用与路径引用
    /// **一处都没人守**。同一个文件在本会话里已经是第二次成为盲区
    ///（上一次是 `doc_copy_registry` 那边：它带着一个陈旧的 `13 tokens`，
    /// 而那张表的锚点全是中文措辞，英文散文一条都对不上）。
    ///
    /// ⇒ 病根与本会话反复量到的同一条：**手写清单描述人群**。
    /// 改成扫出来：仓根往下找 `README*.md`，摘掉 vendor / 依赖 / 构建产物。
    fn entry_readmes() -> Vec<PathBuf> {
        const SKIP: &[&str] = &["node_modules", "target", "dist", "vendor", ".git"];
        let mut v: Vec<PathBuf> = Vec::new();
        let mut stack = vec![repo_root()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if p.is_dir() {
                    if !SKIP.contains(&name) {
                        stack.push(p);
                    }
                } else if name.starts_with("README") && name.ends_with(".md") {
                    v.push(p);
                }
            }
        }
        v.sort();
        v
    }

    /// 一张「表头含状态」的表：`(文件名, 表头行号, 各行 = (件名, 首格原文, 状态格原文))`。
    ///
    /// 件名剥掉 `*`/`✅` 便于对表；**首格原文留着**，因为本仓的约定是
    /// 「✅ 打在件名上、日期写在状态列」—— 判「文档说完没完」要同时看这两处。
    #[allow(clippy::type_complexity)]
    fn status_tables() -> Vec<(String, usize, Vec<(String, String, String)>)> {
        let mut out = Vec::new();
        for p in doc_files() {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let raw = std::fs::read_to_string(&p).unwrap_or_default();
            let lines: Vec<&str> = raw.lines().collect();
            let mut i = 0usize;
            while i < lines.len() {
                let is_head = lines[i].starts_with('|')
                    && lines[i].contains("状态")
                    && i + 1 < lines.len()
                    && lines[i + 1].starts_with('|')
                    && lines[i + 1]
                        .chars()
                        .all(|c| matches!(c, '|' | '-' | ':' | ' '));
                if !is_head {
                    i += 1;
                    continue;
                }
                let mut rows = Vec::new();
                let mut j = i + 2;
                while j < lines.len() && lines[j].starts_with('|') {
                    let cells: Vec<&str> = lines[j].trim_matches('|').split('|').collect();
                    let raw_first = cells.first().unwrap_or(&"").trim().to_string();
                    let item = raw_first.replace(['*', '✅'], "").trim().to_string();
                    let status = cells.last().unwrap_or(&"").trim().to_string();
                    rows.push((item, raw_first, status));
                    j += 1;
                }
                out.push((name.clone(), i + 1, rows));
                i = j;
            }
        }
        out
    }

    /// ★ 抽取器自检：扫描面没缩水。坏掉时下面几条会零命中零失败地绿。
    #[test]
    fn the_doc_scan_actually_reads_the_durable_docs() {
        let files = doc_files();
        assert!(
            files.len() >= 11,
            "`doc/` 只扫到 {} 个 .md —— 遍历坏了（F11 摸底实测 11 个）",
            files.len()
        );
        let total: usize = files
            .iter()
            .map(|p| {
                std::fs::read_to_string(p)
                    .map(|s| s.lines().count())
                    .unwrap_or(0)
            })
            .sum();
        assert!(
            total >= 4000,
            "`doc/` 总共只剩 {total} 行 —— 路径或读法坏了（摸底实测 4158 行）"
        );
        let tables = status_tables();
        // 〔F19〕1 → **2**：`doc/ARCHITECTURE.md` §2.1 新增「backend 四层落地」表。
        // ⚠ 这不是「为了绿而改数字」——改数字的**前提**是那张表的每一格都已登记进
        // `STATUS_CELLS` 并配了现场量法（本条的报错文案逐字要求的就是这件事）。
        assert_eq!(
            tables.len(),
            2,
            "「表头含状态」的表张数变了（实得 {:?}）—— **这不是让你改数字**：\n\
             新出现一张就把它的每一格登记进 `STATUS_CELLS` 并配一条现场量法；\n\
             少了一张就说明表被删了或表头措辞变了（那本条会零命中地绿，所以它必须红）。",
            tables
                .iter()
                .map(|(f, l, r)| format!("{f}:{l}（{} 行）", r.len()))
                .collect::<Vec<_>>()
        );
    }

    /// ★ **两个方向**：文档里每一格都登记了；登记表里没有文档里已经不存在的件。
    #[test]
    fn every_status_cell_is_registered() {
        let mut in_doc: Vec<String> = status_tables()
            .into_iter()
            .flat_map(|(_, _, rows)| rows)
            .map(|(item, _, _)| item)
            .collect();
        in_doc.sort();
        let mut registered: Vec<String> = STATUS_CELLS.iter().map(|(k, _)| k.to_string()).collect();
        registered.sort();
        assert_eq!(
            in_doc, registered,
            "耐久文档的「状态列」与登记表对不上。\n\
             多出来的格请登记进 `STATUS_CELLS` 并写一条**现场量法**（不是抄它的状态，\n\
             是写「怎么从代码里量出这个状态还对不对」）；\n\
             登记表里多出来的件说明文档改了而这里没跟。"
        );
    }

    /// 生产段里 `.call("launch")` 的处数（**不数测试段、不数本仓的说明文字**）。
    fn production_launch_calls() -> usize {
        let mut n = 0usize;
        let mut stack = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        // 运行时拼，免得命中本文件自己。
        let verb = format!(".call(\"{}\"", "launch");
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                    continue;
                }
                // `launch_wire.rs` 的头注里逐字写着那个串（F07 立的例外，沿用）。
                if p.file_name().is_some_and(|s| s == "launch_wire.rs") {
                    continue;
                }
                let src =
                    guard_core::production_code(&std::fs::read_to_string(&p).unwrap_or_default());
                n += src.matches(verb.as_str()).count();
            }
        }
        n
    }

    /// ★★ **核心手法：判据从文档里读那个数，不自己写一份。**
    ///
    /// # 它抓的是什么
    ///
    /// `INVARIANTS §33b` 有一格逐字写着「生产段 `.call("launch")` 今天 **N 处**」。
    /// 本条把那个 N 抽出来，与现场数的比。⇒ 那个数**只有一个家（文档）**，
    /// 而「有人加了第三处调用而文档还写着 2」变成一条会红的机检。
    ///
    /// ⚠ **F07 漏掉的正是这一格。** 它订正了三问的答案 ①（11 行之后那一处），
    /// 而这一行说着同一句话（「零生产调用方 —— 只有一处且在 `cfg(test)` 里」）**没被碰**。
    /// **订正手头那一处，不等于订正那句话。**
    #[test]
    fn the_doc_number_for_production_launch_calls_matches_reality() {
        let marker = "〔机检〕生产段 `.call(\"launch\")` 处数：";
        // ⚠ **F12 扩扫描面**：第一版只读 `INVARIANTS.md` 一份 ⇒
        // `/full-audit` 逮到**第四份副本**住在 `doc/IPC-PROTOCOL.md:584`（「生产路径今天还没切过来」），
        // 而它**结构上永远不会红**。⇒ 先扫全 `doc/**`，任何一份里出现同一句旧断言都要红。
        {
            let stale = "生产路径今天还没切过来";
            let mut offenders: Vec<String> = Vec::new();
            for p in doc_files() {
                if std::fs::read_to_string(&p)
                    .map(|t| t.contains(stale))
                    .unwrap_or(false)
                {
                    offenders.push(p.file_name().unwrap().to_string_lossy().to_string());
                }
            }
            assert!(
                offenders.is_empty(),
                "这些耐久文档还写着「{stale}」：{offenders:?} —— 那句话在 U8a-2c-1 之后就假了。\n\
                 ⚠ **它是同一句话的第四份副本**，而 F11 的机检只读 `INVARIANTS.md` ⇒ 它结构上够不着。\n\
                 这正是本模块头注那条纪律的反例：**订正一句假话时先把它的全部副本找出来。**"
            );
        }
        let at = INVARIANTS.find(marker).unwrap_or_else(|| {
            panic!(
                "`INVARIANTS.md` 里找不到锚点 {marker:?} —— 那句话被改写了。\n\
                 本条判据的整个价值就是「那个数只有一个家」；\n\
                 改措辞就把它变成零命中地绿，所以宁可让它红。"
            )
        });
        let tail = &INVARIANTS[at + marker.len()..];
        let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        let want: usize = digits
            .parse()
            .unwrap_or_else(|_| panic!("锚点后面不是数字，实得 {:?}", &tail[..12.min(tail.len())]));
        let got = production_launch_calls();
        assert_eq!(
            want, got,
            "文档写着生产段有 {want} 处 `.call(\"launch\")`，实测 {got} 处。\n\
             ⚠ **这两个数只允许有一个家（文档那一处）** —— 别在代码里再写一份，\n\
             回去改文档那个数，并顺手想一想：多出来的那处是不是又切了一格？\n\
             （F04c 加的那处是 `send-keys`，**不是**又切了一格「起会话」——\n\
             那两件事很容易被混成一个数，`launch_wire` 那条判据就是为此改过度量的。）"
        );
    }

    /// ★ 「外层载荷那几个产出方」—— 逐个**按文档说的状态**复核。
    ///
    /// 这条是「可数的实测断言」里第二条能钉的。⚠ 它**只钉住「在不在」**，
    /// 钉不住「它们各自还是不是生产在跑」—— 那需要真远端/真安装包（ROADMAP §5）。
    ///
    /// # 🔴 `K-R104`（09-13）：它从「四个都还在」变成「各自是不是文档说的那个状态」
    ///
    /// 上一版逐字叫 `the_four_outer_layer_producers_are_all_still_there`，〔散文墓碑〕
    /// 断的是**四条存在性**。而 `K-R104` 让其中一条**真的退役了** ——
    /// `account_usage.rs::build_usage_probe_cmd`（用量探针那条 shell 串）
    /// 随编排搬上后端帧面而整个不存在了。
    ///
    /// ⇒ 按它自己报错文案里那句话办：「**这多半是好事** …… 回去把它和 `INVARIANTS §33b`
    /// 那句『四个产出方，一个都没退役』一起重裁」。**重裁的结果不是删掉本条**，
    /// 是把那一格从「必须在」翻成「**必须不在**」——
    /// 退役了却又长回来（有人重新在 monitor 里拼一条 tmux 编排串）同样要红。
    /// 剩下三条照旧钉存在性。
    #[test]
    fn the_outer_layer_producers_are_in_the_state_the_doc_claims() {
        let root = repo_root();
        let checks: &[(&str, bool)] = &[
            (
                "session-backend.ts（TS 生产远端主路）",
                root.join("src/session-backend.ts").is_file(),
            ),
            (
                "control/launch.rs（daemon argv）",
                root.join("remote-daemon-proto/src/control/launch.rs")
                    .is_file(),
            ),
            (
                // 🔴 **翻面**（`K-R104` 09-13）：这一条**退役了**，所以这里断的是「它不在」。
                //    读得到文件是前提（读不到会静默变成 `unwrap_or(false)` ⇒ 恒 true 的假绿），
                //    所以两半都写出来：文件必须在 ＋ 那个函数必须不在。
                "account_usage.rs::build_usage_probe_cmd（用量探针 shell 串，`K-R104` 已退役 —— 这一格断的是它**不许回来**）",
                std::fs::read_to_string(root.join("src-tauri/src/account_usage.rs"))
                    .map(|s| !s.contains("fn build_usage_probe_cmd"))
                    .unwrap_or(false),
            ),
            (
                // 🔴 〔`K-R48` 第二拍 09-11〕住址换了，**产出方本身没退役**：
                //    〔用@09-11 `K33`〕那个 bash 脚本删了，「用户终端那条路」今天由
                //    后端本体的一次性模式渲（`control::ccm::plan::render_container`）。
                //    ⇒ `INVARIANTS §33b` 那句「四个产出方，一个都没退役」**仍然成立**，
                //    只是第四个的住址从 `shared/ccm` 变成了 `control/ccm/plan.rs`。
                "control/ccm/plan.rs（用户终端那条路）",
                root.join("remote-daemon-proto/src/control/ccm/plan.rs")
                    .is_file(),
            ),
        ];
        let missing: Vec<&str> = checks
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(n, _)| *n)
            .collect();
        assert!(
            missing.is_empty(),
            "「外层产出方」里这几格与文档说的状态对不上：{missing:?}\n\
             · 还没退役的那几条**不在了** ⇒ **这多半是好事**：有产出方退役了 ⇒\n\
               `INVARIANTS §33b` 那张表过期了，回去把它和 U8c-3 的前置一起重裁。\n\
             · 已退役的那条**又回来了** ⇒ 那是有人重新在 monitor 里拼一条 tmux 编排串\n\
               （`K-R104` 刚把它整条搬上后端帧面）—— 回去看 `account_usage.rs` 的头注。"
        );
    }

    /// 🔴🔴 **`K-R105` `KR105D1`：`§33b` 三问的答案，逐问与现场对拍。**
    ///
    /// 立项理由与手法住 [`THIRTY_THREE_B_QUESTIONS`] 上方那一段（本条不复述）。
    /// 一句话：**改一问所依赖的行为而不改那问的答案 ⇒ 当场红。**
    ///
    /// # 三条量法，逐条写清它量的是什么
    ///
    /// - **①「生产切到 daemon 的 `launch` 了吗」** —— 量「哪几棵树的生产段真的发
    ///   `create-or-attach`」。**两棵树各算一格**：monitor 自己那条 `↗` 路
    ///   （`src-tauri/src/**.rs`）与后端自带的 CLI 面（`control/ccm/`）。
    ///   🔴 08-14 那一版只量了前一棵 ⇒ `K-P2` `D3`（09-03）把后一棵翻正之后，
    ///   那个读数**在它自己的尺子上仍然是对的**，而它答的那一问已经不是原来那一问了。
    ///   ⇒ 本条把两棵树都收进来，`部分切` 与 `全切` 因此分得开。
    /// - **②「attach 那条串归谁产」** —— 量「生产 TS 里还有没有人问座要 attach」。
    ///   daemon **结构上不产 attach**（`control/launch.rs` 头注逐字「本模块不 attach，一次都不」，
    ///   `parse_request` 的错文案逐字「attach 是平面 ③，不归 daemon」）⇒
    ///   前端不产的那天，就是这一问有第二个答案的那天。
    /// - **③「daemonless 的远端还要不要能起会话」** —— 量那一档的**三个载体**
    ///   （落盘字段 · 界面那个 input · 数据源那条轮询回落）。
    ///   刻意**不数 `daemonless` 这个词**：`remote-config.ts` 里还留着一处认旧配置的墓碑，
    ///   数名字会把它读成回潮（口径与 `launch_wire.rs` 那条同源）。
    ///
    /// # ⚠ 剥法用的是哪一份
    ///
    /// TS 用 [`guard_core::strip_comment_lines`]（块注释 + 整行 + 行尾），**不是**
    /// `production_code` —— 后者按 Rust 的 `#[cfg(test)]` 写，喂 TS 会多剥或少剥。
    /// Rust 侧照旧 `production_code`。⇒ 注释里怎么解释这三问都不算数，只看生产段。
    #[test]
    fn the_three_questions_in_33b_have_todays_answers() {
        let root = repo_root();
        let read = |rel: &str| std::fs::read_to_string(root.join(rel)).unwrap_or_default();
        let prod_rs = |rel: &str| guard_core::production_code(&read(rel));
        let prod_ts = |rel: &str| guard_core::strip_comment_lines(&read(rel));

        // ── 量法 ① ────────────────────────────────────────────────────────────
        // 运行时拼，免得命中本文件自己的说明。
        let mode = format!("\"create-or-{}\"", "attach");
        let word = format!("create-or-{}", "attach");
        let mut monitor_emits = false;
        let mut scanned_rs = 0usize;
        for (p, raw) in guard_core::scan_tree!(&root.join("src-tauri/src"), &["rs"]) {
            // `launch_wire.rs` 的说明里逐字写着那个串（F07 立的例外，本条沿用同一条）。
            if p.file_name().is_some_and(|n| n == "launch_wire.rs") {
                continue;
            }
            scanned_rs += 1;
            if guard_core::production_code(&raw).contains(mode.as_str()) {
                monitor_emits = true;
            }
        }
        // ★ 抽取器自检：人群没缩水（否则 `monitor_emits` 恒 false ⇒ ① 永远读成「部分切」）。
        assert!(
            scanned_rs >= 50,
            "只扫到 {scanned_rs} 个 monitor 侧 `.rs` —— 遍历坏了，量法 ① 会零命中地绿"
        );
        let ccm: String = ["mod.rs", "argv.rs", "plan.rs"]
            .iter()
            .map(|f| prod_rs(&format!("remote-daemon-proto/src/control/ccm/{f}")))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            ccm.lines().count() >= 300,
            "`control/ccm/` 三份的生产段只剩 {} 行 —— 读错了或剥法把代码也剥了，量法 ① 会零命中地绿",
            ccm.lines().count()
        );
        let ccm_emits = guard_core::contains_word(&ccm, &word);
        let a1 = match (monitor_emits, ccm_emits) {
            (false, false) => "〔现打①〕一格没切",
            (true, true) => "〔现打①〕全切",
            _ => "〔现打①〕部分切",
        };

        // ── 量法 ② ────────────────────────────────────────────────────────────
        // 座本身（`session-backend.ts`）不算 —— 它是被问的那一层，不是问的人。
        let seat_attach = format!("SESSION_BACKEND.{}", "attach");
        let mut askers: Vec<String> = Vec::new();
        let mut scanned_ts = 0usize;
        for (p, raw) in guard_core::scan_tree!(&root.join("src"), &["ts"]) {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            // 座本身不算 —— 它是被问的那一层，不是问的人。
            if name.ends_with(".test.ts")
                || name.ends_with(".vitest.ts")
                || name == "session-backend.ts"
            {
                continue;
            }
            scanned_ts += 1;
            let code = guard_core::strip_comment_lines(&raw);
            // 整词，不是裸子串：`…attachFoo` 不算（与 `launch_wire` 那把尺子同口径）。
            if guard_core::contains_word(&code, seat_attach.as_str()) {
                askers.push(name);
            }
        }
        assert!(
            scanned_ts >= 100,
            "只扫到 {scanned_ts} 份生产 TS —— 遍历坏了，量法 ② 会零命中地绿"
        );
        let a2 = if askers.is_empty() {
            "〔现打②〕后端全产 attach"
        } else {
            "〔现打②〕前端仍产 attach"
        };
        // ★ 反向锚点：daemon 那条「不 attach」的结构事实还在。它没了，② 的两档都说不清。
        // 🔴 **钉整行，不是子串**（`needle_anchor_registry` 治的那一族：匹配单位比事实小）。
        let launch_rs = read("remote-daemon-proto/src/control/launch.rs");
        let no_attach = "//! 开不了你面前的窗）。所以本模块**不 attach**，一次都不。";
        assert!(
            guard_core::pin_line(&launch_rs, no_attach).is_ok(),
            "`control/launch.rs` 头注里那一行「本模块不 attach，一次都不」不在了 ——\n\
             ② 这一问的整个形状建立在它上面（后端结构上开不了你面前的窗）。\n\
             真要改，回 `INVARIANTS §33b` 与 `U8c-3` 重裁，别只改头注。"
        );

        // ── 量法 ③ ────────────────────────────────────────────────────────────
        // 断的是**载体**不是名字（口径与 `launch_wire.rs` 那条同源，理由见本条头注）。
        let carriers: [(&str, bool); 3] = [
            (
                "落盘字段 `REMOTE_HOST_FIELDS`",
                prod_ts("src/remote-config.ts").contains(r#""daemonless","#),
            ),
            (
                "机器卡片那个 input",
                prod_ts("src/settings/machine-card.ts").contains("daemonlessInput"),
            ),
            (
                "数据源那条轮询回落",
                prod_rs("src-tauri/src/ssh_source.rs").contains("daemonless_stream_loop"),
            ),
        ];
        // ★ 抽取器自检：三份语料都真的读到了（读不到只会静默返回空串 ⇒ 三格全 false ⇒ 假「已退役」）。
        for rel in [
            "src/remote-config.ts",
            "src/settings/machine-card.ts",
            "src-tauri/src/ssh_source.rs",
        ] {
            assert!(
                read(rel).len() > 3000,
                "量法 ③ 读的 {rel} 只有 {} 字节 —— 读不到的文件只会静默返回空串",
                read(rel).len()
            );
        }
        let back: Vec<&str> = carriers
            .iter()
            .filter(|(_, on)| *on)
            .map(|(n, _)| *n)
            .collect();
        let a3 = if back.is_empty() {
            "〔现打③〕已退役"
        } else {
            "〔现打③〕那一档还在"
        };

        // ── 逐问与文档对拍 ────────────────────────────────────────────────────
        let derived = [a1, a2, a3];
        assert_eq!(
            THIRTY_THREE_B_QUESTIONS.len(),
            derived.len(),
            "三问表的行数与量法条数对不上 —— 加一问要同时加一条量法"
        );
        for ((q, verdicts), got) in THIRTY_THREE_B_QUESTIONS.iter().zip(derived) {
            assert!(
                verdicts.contains(&got),
                "`{q}` 量出来的判词 {got:?} 不在它自己的闭集里 —— 量法与表对不上"
            );
            let present: Vec<&&str> = verdicts
                .iter()
                .filter(|v| INVARIANTS.contains(**v))
                .collect();
            assert_eq!(
                present.len(),
                1,
                "`{q}`：`INVARIANTS.md` 里出现的判词是 {present:?} —— 必须**恰好一个**。\n\
                 · 一个都没有 ⇒ 那一问的答案没带判词（改措辞就把本条变成零命中地绿，\n\
                   所以宁可让它红）；\n\
                 · 出现两个以上 ⇒ 同一问在文档里有两份互相矛盾的答案。\n\
                 闭集：{verdicts:?}"
            );
            assert_eq!(
                *present[0], got,
                "🔴 **`{q}` 的答案过期了。**\n\
                 文档里写着 {:?}，现打是 {got:?}。\n\
                 · **事实前进了而答案没跟**（这一族在本节犯过至少三次：F07 · `K-R59` · `K-P2 D3`）\n\
                   ⇒ 改文档那一格，并同轮问一句：这一问挡着的那件事，今天还挡不挡得住？\n\
                 · **答案改了而事实没动** ⇒ 那是有人在文档里许了一个还没兑现的愿。\n\
                 ⚠ 现场读数：① monitor 树发 `create-or-…`={monitor_emits} · `control/ccm/` 发={ccm_emits}；\n\
                 ② 生产 TS 里还问座要 attach 的：{askers:?}；③ 那一档还在场的载体：{back:?}。",
                present[0]
            );
        }
    }

    /// ★★ `K-R73` `KR73D3`：**普查表与 `STATUS_CELLS` 两个方向对拍**，外加那条递减棘轮。
    ///
    /// # 没有这一条，那张普查表会跟本模块治的那些文档副本一样腐
    ///
    /// 新加一格量法而普查表没跟 ⇒ 那一格**没人判过它量的是不是它声称的那件事**，
    /// 而这正是 `R29` 裁定零那次的形状（`observe`/`platform`/`common` 三格是**跟着**
    /// `control/` 那格一起写下去的，没有一格被单独问过「你量的是落地还是有个目录」）。
    /// 反方向也要：普查表留着一行而 `STATUS_CELLS` 那边没了 ⇒ 那一行在描述一个不存在的量法。
    #[test]
    fn every_status_cell_measure_is_in_the_census() {
        let mut cells: Vec<&str> = STATUS_CELLS.iter().map(|(_, how)| *how).collect();
        cells.sort();
        cells.dedup();
        let mut census: Vec<&str> = MEASURE_CENSUS.iter().map(|(k, ..)| *k).collect();
        census.sort();
        // 同一个量法键不许在普查表里出现两次 —— 两行说法不一致时谁也不知道哪行算数。
        let mut uniq = census.clone();
        uniq.dedup();
        assert_eq!(
            census, uniq,
            "普查表里有重复的量法键 —— 一个事实恰好一个住址"
        );
        assert_eq!(
            cells, census,
            "`STATUS_CELLS` 的量法键与 `MEASURE_CENSUS` 对不上。\n\
             **`STATUS_CELLS` 多出来的**：新加了一格量法却没判过「它量的是不是它声称的那件事」——\n\
             那正是 `DECISIONS.md#R29` 裁定零那次的形状（三格裸 `is_dir()` 是跟着上一格一起写下去的）。\n\
             **普查表多出来的**：那一行在描述一个已经不存在的量法，摘掉它。"
        );
        // 分母自检：表空了上面那个等号会退化成「空 == 空」。
        assert!(
            MEASURE_CENSUS.len() >= 10,
            "普查表只剩 {} 行（09-12 现打 10 行）—— 少于分母说明有人在偷偷删行",
            MEASURE_CENSUS.len()
        );
        for (k, _, _, why) in MEASURE_CENSUS {
            assert!(
                why.trim().chars().count() >= 30,
                "`{k}` 那一行没写清「它到底量了什么、与那句话差在哪」（只有 {} 字）",
                why.trim().chars().count()
            );
        }
        // ★ 递减棘轮：**对不上**那一栏只许比今天少。
        let falls_short = MEASURE_CENSUS
            .iter()
            .filter(|(_, _, v, _)| *v == Verdict::FallsShort)
            .map(|(k, ..)| *k)
            .collect::<Vec<_>>();
        assert!(
            falls_short.len() <= FALLS_SHORT_CEILING,
            "「量法与它声称的性质对不上」涨到 {} 条了 > 棘轮上限 {FALLS_SHORT_CEILING}（09-12 现打 5）。\n\
             逐条：{falls_short:?}\n\
             ⚠ **不许把上限调上去让今天好过** —— 这是递减棘轮。",
            falls_short.len()
        );
    }

    /// `K-R73` `KR73D2`：`backend/` 下一条能力线**落地了没有**。
    ///
    /// # 量的必须是**落地**，不是**有个目录**（`DECISIONS.md#R29` 裁定零）
    ///
    /// 这一格原本是裸 `is_dir()`。而**紧挨着上面那几行**的 `control/` 那格逐字警告过：
    /// 「目录空着也算「有目录」，而这一格要主张的是控制面真的住进来了」——
    /// 写下那条警告的人在下面三行里连犯三次它警告的那件事（`observe/` 已由 `K-R71` 收窄，
    /// 这两格归本件）。
    ///
    /// # 为什么不像 `control/` / `observe/` 那两格那样钉住一个住户的**文件名**
    ///
    /// 那两条线各有「那个唯一的住户」可钉。`platform/` 与 `common/` **今天没有**（目录都还不在）
    /// ⇒ 随手指一个文件名就是**替未来的人做决定**。这里量的是一个说得出口的形：
    /// **目录在 ∧ 里面至少有一个不是 `mod.rs` 的 `.rs`**。
    ///
    /// ⚠ **代价写明，别读宽**：这个形分不清「真住户」与「一份占位的 `.rs`」，
    /// 也不像 `control/` 那格那样钉住**是谁**在里面。等这条线真有那个唯一住户的那天，
    /// 换成与 `control/` 同形（钉住住户名）是**更紧**的一格 —— 那是那个人的活，不是本件的。
    ///
    /// # 🔴 空目录的答案不是 `false`，是**红**
    ///
    /// 目录不存在 ⇒ `false`（今天两格都在这一支，与文档那两格的「待做」对得上）。
    /// 目录**在**、里面却没有住户 ⇒ 直接 panic：那是一份**装饰**，
    /// 而装饰的危险不在它今天算 `true` 还是 `false`，在于**它让下一个人只要顺手改一下文档
    /// 那一格就能把它洗成「已交付」**（`K-R71` 的 `7u` 逮到的正是这一形：
    /// 空的 `observe/` ＋ 文档说「已交付」，monitor lib 1397 条一条不红）。
    fn a_capability_line_has_landed(root: &Path, line: &str) -> bool {
        layer_has_landed_at(&root.join("src-tauri/src/backend").join(line))
    }

    /// [`a_capability_line_has_landed`] 的**根可注入**版本。
    ///
    /// 抽出这一层只为一件事：下面那条反向自检要让**这一份量法本身**（不是它的复刻）
    /// 跑在真目录上 —— 自检若另写一份判断，它证明的是那一份、不是量法。
    fn layer_has_landed_at(dir: &Path) -> bool {
        if !dir.is_dir() {
            return false;
        }
        let mut residents: Vec<String> = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).expect("读能力线目录") {
                let p = entry.expect("目录项").path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) == Some("rs")
                    && p.file_name().and_then(|x| x.to_str()) != Some("mod.rs")
                {
                    residents.push(p.to_string_lossy().into_owned());
                }
            }
        }
        assert!(
            !residents.is_empty(),
            "`{}` 建出来了，可里面一个住户都没有（只有 `mod.rs` 也算没有）。\n\
             ⚠ 一个空的能力线目录是**装饰**：它自己不说假话，但它让下一个人\n\
             只要顺手把 `doc/ARCHITECTURE.md` 那一格改成「已交付」就全绿。\n\
             两条出路，别默认第一条：① 把那个住户真的搬进来；\n\
             ② 这一层其实还不需要 ⇒ 把目录删掉，让「待做」继续是真的。",
            dir.display()
        );
        true
    }

    /// ★ 反向自检：上面那份量法**真的在量**，两个方向都验。
    ///
    /// # 没有这一格会怎样
    ///
    /// 把 [`layer_has_landed_at`] 改成恒 `false`，那两格就**永远说「没落地」** ——
    /// 与文档今天的「待做」一直对得上，真落地那天同样没人红。
    /// 反过来改成恒 `true`，今天当场红（那一支由 ② 接住）。
    /// **两个方向都要有刀**，所以这里既有非空对照、也有空对照。
    #[test]
    fn the_capability_line_landing_probe_actually_bites() {
        let root = repo_root();
        // ① **非空对照，而且是活体**：`control/` 这条线盘上确实有住户 ⇒ 必须 `true`。
        //    量法一旦被改成恒 `false`，这一格当场红。
        //    ⚠ 刻意用 `control/` 而不是 `observe/`：`observe/` 那一格 09-12 刚被 `K-R71`
        //    按 `R29` 裁定一动过，本件一个字都不碰它。
        assert!(
            layer_has_landed_at(&root.join("src-tauri/src/backend/control")),
            "`backend/control/` 这条线明明住满了人，量法却说它没落地 —— \
             那说明这份量法此刻是恒 `false`，而恒 `false` 让那两格永远说「没落地」"
        );
        // ② 目录不存在 ⇒ `false`（今天 `platform/` 与 `common/` 就在这一支）。
        //    量法一旦被改成恒 `true`，这一格当场红。
        let gone = std::env::temp_dir().join(format!("ccm-cl-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&gone);
        assert!(
            !layer_has_landed_at(&gone),
            "一个根本不存在的目录被量成「落地了」"
        );
        // ③ 目录在、只有 `mod.rs` ⇒ **装饰，必须红**。
        //    🔴 这就是件计划那条死值验（「建一个空的 `backend/platform/` 目录」）的活体版：
        //    真判据跑在真目录上，而不是靠喂字符串。
        let sham = std::env::temp_dir().join(format!("ccm-cl-sham-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sham);
        std::fs::create_dir_all(&sham).expect("造夹具目录");
        std::fs::write(sham.join("mod.rs"), "//! 什么都不声明\n").expect("写夹具");
        let r = std::panic::catch_unwind(|| layer_has_landed_at(&sham));
        assert!(
            r.is_err(),
            "只有一份 `mod.rs` 的空壳目录被量成了「没落地」而不是红 —— \
             那让「建个目录 + 顺手改一下文档」重新变成一条全绿的路"
        );
        // ④ 目录在、有一个真住户 ⇒ `true`。这一格与 ③ 一起把「有住户」这个形钉住：
        //    少了它，量法可以退化成「目录里有 `mod.rs` 就红」这种谁都过不去的东西。
        let real = std::env::temp_dir().join(format!("ccm-cl-real-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&real);
        std::fs::create_dir_all(&real).expect("造夹具目录");
        std::fs::write(real.join("mod.rs"), "//! 说明\n").expect("写夹具");
        std::fs::write(real.join("one.rs"), "pub fn ok() -> usize { 0 }\n").expect("写夹具");
        assert!(
            layer_has_landed_at(&real),
            "目录里有一个真住户，量法却说它没落地"
        );
        for d in [gone, sham, real] {
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    /// ★★ 逐格跑「现场量法」：**文档里那一格**记的状态今天还对不对。
    ///
    /// ⚠ 状态**从文档读**，不从登记表读 —— 见 `STATUS_CELLS` 的头注：
    /// 第一版存了一份副本，E4 变异（只改文档里的状态）时五条判据**全绿**。
    ///
    /// ⚠ 量法都是**结构性**的（文件/符号在不在、生产段有没有在调），**不是**跑功能。
    /// 那是刻意的：这一族的失效形态是「事实变了而文档没跟」，
    /// 而结构性事实恰好是变了就一定能看见的那种。
    #[test]
    fn each_registered_status_still_matches_reality() {
        let root = repo_root();
        let read = |rel: &str| std::fs::read_to_string(root.join(rel)).unwrap_or_default();
        let prod = |rel: &str| guard_core::production_code(&read(rel));
        // 文档那张表：件名 → (首格原文, 状态格原文)
        let cells: std::collections::BTreeMap<String, (String, String)> = status_tables()
            .into_iter()
            .flat_map(|(_, _, rows)| rows)
            .map(|(item, raw, status)| (item, (raw, status)))
            .collect();
        for (item, how) in STATUS_CELLS {
            let (delivered, why) = match *how {
                "payload-kernel-exists" => (
                    prod("src-tauri/src/backend/control/payload.rs").contains("fn render_payload"),
                    "载荷内核在 `backend/control/payload.rs`",
                ),
                // ⚠ 第一版量法写的是 `render_payload`，**红了**——那是我选错了标的：
                // 用量探针走的是内核的另一个入口 `payload::usage_probe_payload`
                // （账号前缀 + 嵌套 env 清理 + 启动器，**无 cd**）。判据自己的报错文案
                // 逐字预言了这一种可能（「或者本条量法本身选错了标的」），照它改。
                "usage-probe-uses-the-kernel" => (
                    prod("src-tauri/src/account_usage.rs")
                        .contains("backend::control::payload::usage_probe_payload"),
                    "用量探针在调载荷内核的 `usage_probe_payload` 入口",
                ),
                // ⚠ **F12 订正**：第一版左支读的是 `src-tauri/src/shell_quote.rs` —— **那个文件不存在**
                // ⇒ 左支恒 false，整条判据只靠右支撑着（`/full-audit` 逮到的）。
                // 这正是「判据自己会不会错」那一问要问的东西：**读一个不存在的文件不会报错，
                // 只会静默返回空串**，而 `||` 让它看起来像「两条都在查」。
                // ⇒ 改成读真正的家，并加一条「文件必须存在」的断言，杜绝同样的静默空转。
                "posix-quote-has-one-home" => (
                    {
                        let host = "src-tauri/src/ssh_source.rs";
                        assert!(
                            repo_root().join(host).is_file(),
                            "量法读的 {host} 不存在 —— 读不到的文件只会静默返回空串"
                        );
                        prod(host).contains("shell_quote_core::posix_quote")
                    },
                    "Rust 侧的 POSIX quote 收在共享 crate（另有 `quote_singleton_guard` 单点守卫）",
                ),
                "ccm-invocation-kernel-exists" => (
                    prod("src-tauri/src/backend/control/ccm_invocation.rs")
                        .contains("fn render_ccm_invocation"),
                    "ccm 调用行内核在 `backend/control/ccm_invocation.rs`",
                ),
                "production-ts-calls-the-rust-renderers" => (
                    read("src/remote-launch-run.ts").contains("commands.render_ccm_launch(")
                        && read("src/remote-launch-run.ts")
                            .contains("commands.render_launch_payload("),
                    "生产 TS 主路在调那两条 Rust 渲染命令",
                ),
                // 「待做」那一格：**反向**量法 —— TS 渲染器还在，就说明确实还没删。
                "ts-renderer-still-there" => (
                    !root.join("src/session-backend.ts").is_file(),
                    "TS 渲染器已经删了",
                ),
                // 〔F19〕四层落地量法。⚠ 量的是「**这一层在 monitor 侧落地了没有**」，
                // **不是**「它内部已经干净了」——后者各有各的判据（`platform/` 是 C10 的
                // 跨 target 编译，今天不成立）。把两件事混进一格会让这一格永远说不清。
                //
                // ⚠ `control/` 那格刻意不是裸 `is_dir`：目录空着也算「有目录」，
                // 而这一格要主张的是**控制面真的住进来了** ⇒ 钉住那个唯一的分流器在里面。
                "monitor-backend-control-landed" => (
                    root.join("src-tauri/src/backend/control/daemon_route.rs")
                        .is_file(),
                    "monitor 侧 `backend/control/` 在，且那个唯一的回落分流器住在里面",
                ),
                // 🔴 〔`K-R71` 09-12，PM 裁定一〕**这一格原本是裸 `is_dir`** —— 原文逐字：
                // `root.join("src-tauri/src/backend/observe").is_dir()` ＋ 说明串
                // 「monitor 侧 `backend/observe/` 目录存在」。
                // ★ 它犯的正是**紧挨着上面那三行警告**说的那件事：目录空着也算「有目录」。
                //   ⇒ 同一段代码里，写下那条警告的人在下一格就犯了它警告的事（`K-R71` 的 `7u` 逮到：
                //   把整次搬运掏空、只留一个空的 `observe/` 与一份什么都不声明的 `mod.rs`，
                //   而文档继续宣称「已交付」—— monitor lib **1397 条一条不红**）。
                // ⇒ 收窄成与 `control/` 那格同形：钉住那个**唯一的住户**。
                // ⚠ **这不是新判据，是把一条已有判据收窄到它自己声称守的性质** ——
                //   本格改动不加任何 `#[test]`，判据条数一格没涨。
                "monitor-backend-observe-landed" => (
                    root.join("src-tauri/src/backend/observe/local_query.rs")
                        .is_file(),
                    "monitor 侧 `backend/observe/` 在，且那个唯一的读面传输住在里面",
                ),
                // 🔴 〔`K-R73` 09-12，PM 裁定二排期到本件〕**这两格原本是裸 `is_dir`** —— 原文逐字：
                //   `root.join("src-tauri/src/backend/platform").is_dir()` ＋ 说明串
                //   「monitor 侧 `backend/platform/` 目录存在」（`common/` 同形）。
                //   它们**今天不说假话**（两个目录都不在，`false` 是真的），**建目录那一刻才会** ——
                //   与 `observe/` 那一格 09-12 的遭遇逐字同形。
                // ⚠ **本件不许硬指一个住户**：这两条线今天没有「那个唯一的住户」可钉，
                //   随手指一个文件名就是替未来的人做决定。⇒ 收窄成一个**说得出口的形**，
                //   见 [`a_capability_line_has_landed`]：目录在 ∧ 里面真有住户。
                "monitor-backend-platform-landed" => (
                    a_capability_line_has_landed(&root, "platform"),
                    "monitor 侧 `backend/platform/` 在，且里面**真有住户**（至少一个不是 `mod.rs` 的 `.rs`）",
                ),
                "monitor-backend-common-landed" => (
                    a_capability_line_has_landed(&root, "common"),
                    "monitor 侧 `backend/common/` 在，且里面**真有住户**（至少一个不是 `mod.rs` 的 `.rs`）",
                ),
                other => panic!("`{item}` 的量法键 {other:?} 没有实现 —— 登记表与实现漂了"),
            };
            let (raw_first, status) = cells
                .get(*item)
                .unwrap_or_else(|| panic!("`{item}` 不在文档那张表里 —— 另一条判据会先红"));
            // 本仓的约定：**✅ 打在件名上、日期写在状态列**；或者状态列直接写「已交付」。
            //
            // ⚠ **「先出现的那个词算数」**，不是「含哪个词」——状态格里常常带订正叙事
            // （`U8c-2c-2` 那格逐字写着「本列此前写『待做』」），两个词都在里面。
            // 第一版用 `contains` 判，当场被它红了；「先出现的算数」既能容纳叙事，
            // 又不至于把判定权交给措辞。
            let done_at = status.find("已交付");
            let todo_at = status.find("待做").or_else(|| status.find("未做"));
            let says_done = match (done_at, todo_at) {
                _ if raw_first.contains('✅') => true,
                (Some(d), Some(t)) => d < t,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => panic!(
                    "`{item}` 的状态格里既没有「已交付」也没有「待做」（首格 {raw_first:?} / \
                     状态 {status:?}）—— 改措辞就把本条变成零命中地绿，所以宁可让它红。"
                ),
            };
            assert_eq!(
                delivered, says_done,
                "`{item}`：文档那一格说「已交付 = {says_done}」（状态列原文 {status:?}），\n\
                 而现场量法说「{why}」= {delivered}。\n\
                 ⚠ 两种可能，都要动手：\n\
                 · 事实前进了而状态列没跟（**这一族最常见**，六轮 14 处都是它）⇒ 改文档；\n\
                 · 或者本条量法本身选错了标的（U8c-2a 就这么红过一次）⇒ 改量法 + 写清为什么。"
            );
        }
    }

    /// ★★ **F12 全局变异抽样抓到的缺口**：文档里**枚举的一组标识符**没人对拍。
    ///
    /// # 它是怎么被发现的
    ///
    /// Phase G 的全局变异抽样里，把 daemon `inbound.rs` 的 `unknown_command`
    /// **三处一起改名**成 `unknown_cmd` —— **daemon 253 条全绿**。
    /// 而 `doc/IPC-PROTOCOL.md` 逐条列着六个**协议级**错误码，语义是
    /// 「客户端代码写错了，别重试」—— 那是**仓外可见的契约**（`resolve` 那条已经与 aterm 冻结）。
    ///
    /// ⚠ F11 建这个登记表时扫的是「状态列」与「可数的实测断言」两族，
    /// **漏了第三种形状：文档里枚举的一组标识符**。
    /// 那不是「F11 做漏了」——是**摸底时的分族本身不完整**，
    /// 而**只有跨模块的变异抽样能发现这种「整族缺口」**（本工作区自己的判据都在
    /// 各自那件的范围里看，看不到「有一族根本没人管」）。
    ///
    /// ⇒ 这条也是 skill 那句「**变异存活分布不会说谎**」在本工作区拿到的实货。
    #[test]
    fn the_protocol_level_error_codes_in_the_doc_are_the_ones_the_daemon_uses() {
        const IPC: &str = include_str!("../../doc/IPC-PROTOCOL.md");
        let marker = "**协议级**由 `inbound.rs` 独占 ——";
        let at = IPC.find(marker).unwrap_or_else(|| {
            panic!(
                "`IPC-PROTOCOL.md` 里找不到锚点 {marker:?} —— 那句话被改写了。\n\
                 本条判据的价值是「那份清单只有一个家（文档）」，改措辞就把它变成零命中地绿。"
            )
        });
        // ⚠ 收尾锚点是**清单本身的收尾**（`，语义是`），不是段落结束 ——
        // 第一版切到空行，把后面几句里的 `invalid_args`/`launch`/`resolve` 也收进来了
        // （抽到 9 个而真值 6 个）。**抽取器自检当场把它拦下来了**，这就是它的岗位。
        let seg = &IPC[at..];
        let end = seg
            .find("，语义是")
            .expect("找不到清单的收尾锚点「，语义是」—— 那句话被改写了");
        let seg = &seg[..end];
        let mut in_doc: Vec<&str> = Vec::new();
        let mut rest = seg;
        while let Some(a) = rest.find('`') {
            rest = &rest[a + 1..];
            let Some(b) = rest.find('`') else { break };
            let word = &rest[..b];
            rest = &rest[b + 1..];
            // 只收「像错误码」的词：全小写 + 下划线，且不是模块名。
            if !word.is_empty()
                && word.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                && word != "inbound"
            {
                in_doc.push(word);
            }
        }
        in_doc.sort();
        in_doc.dedup();
        // ★ 抽取器自检：这一段里就该有六个码；抽不到就说明剥法坏了。
        assert_eq!(
            in_doc.len(),
            6,
            "从文档那一段只抽到 {} 个协议级错误码（{in_doc:?}）—— 剥法坏了，\n\
             下面那条会零命中地绿。文档实测是六个：bad_request · line_too_long ·\n\
             unknown_command · duplicate_id · handler_panicked · not_cancellable。",
            in_doc.len()
        );
        let daemon =
            std::fs::read_to_string(repo_root().join("remote-daemon-proto/src/inbound.rs"))
                .expect("读不到 daemon 的 inbound.rs");
        let prod = guard_core::production_code(&daemon);
        let missing: Vec<&&str> = in_doc
            .iter()
            .filter(|c| !prod.contains(&format!("\"{c}\"")))
            .collect();
        assert!(
            missing.is_empty(),
            "`IPC-PROTOCOL.md` 列着这些协议级错误码，而 daemon 生产段里**找不到**：{missing:?}\n\
             ⚠ 它是**仓外可见的契约**（`resolve` 那条已经与仓外 aterm 冻结）——\n\
             改名 = 静默毁约：对端拿到一个它不认识的码，而两侧的测试都不会红\n\
             （F12 的全局变异抽样就是这么把这个缺口逮出来的：三处一起改名，daemon 253 条全绿）。\n\
             要改就两侧一起改，并想清楚仓外消费方。"
        );
    }

    /// 〔audit-0805 08-06〕**`doc/` 里点名的代码符号必须解析得到，且住在文档说的那个文件里。**
    ///
    /// **为什么建它**：08-06 把本会话逮到的每一处文档/计划腐坏按机制归了族，主力是
    /// **停滞式** —— 世界变了、文本一个字没动（改代码的那个提交**碰过**那份文件，
    /// 却把已假的那句原样留着）。这一族**结构上救不了**靠「改法纪律」：留痕是「怎么改」的规矩，
    /// 而这族的定义就是没人来改。能接住它的只有一样：**一条会红的判据读到那句散文**（E12 ①）。
    ///
    /// `file.rs::symbol` 是 `doc/` 里**最可机检**的一族散文：改名 / 删除 / 搬家都让它变假，
    /// 而**没有任何东西会红**。建判据当天实测 **73 处引用、真腐 0**——
    /// ⚠ **「今天全对」正是建它的理由，不是不建的理由**：干净是纪律攒出来的，
    /// 而纪律不在门禁里就只是运气，`doc/` 这四个文件此前**一条判据都没读过**。
    ///
    /// **口径**（比「符号存在」严一档，建时实测不误红）：文档写 `a.rs::foo`，
    /// 就要求 `foo` 的声明**出现在 `a.rs` 里** —— 只对符号名会放过「搬到别的文件」，
    /// 而搬家恰恰是本仓重构的常见形态。
    ///
    /// ⚠ ~~抽取器摘除调用者自己（`scan_tree!` 按构造如此）⇒ 只在 `doc/` 引用本文件里的符号时
    /// 才会误红~~ —— **08-06 当天就误红了一次**（`DEVELOPMENT.md` 指向本文件里的一条判据）。
    /// 现在本文件自己也进扫描面（见下方 `srcs.push`）。**「只在极少数情况下会错」不是边界，是欠账。**
    #[test]
    fn every_code_symbol_named_in_the_docs_still_resolves() {
        /// 例外表：**每条都写清「为什么它解析不到却是对的」**。
        /// 下面有一条自检把「已经不需要的例外」揪出来 —— 例外表自己也会腐。
        const EXCEPTIONS: &[(&str, &str)] = &[
            (
                "setup",
                "tauri 的 `.setup(move |app| …)` 钩子闭包 —— 是真东西，但不是一处声明",
            ),
            (
                "monitor_get_active_ids",
                "`CONTRIBUTING.md` 里的**示例占位符**（教人「照这样加一行」），本就不指向真符号",
            ),
            (
                "run_tmux_reconcile_poller",
                "`INVARIANTS.md` 那句逐字写着它**已删**（audit-fixes F03.2）—— 历史句，\
                 删掉反而丢掉「为什么今天没有 poller」的解释",
            ),
            (
                "build_usage_probe_cmd",
                "★〔`K-R104` 09-13〕`INVARIANTS.md` §33b 那两处逐字写着它**已退役** \
                 —— 用量探针的整条编排搬上后端帧面之后，monitor 一个 shell 字符都不渲染。\
                 那两句正是「外层四个产出方里退役了哪一个、为什么」的解释，\
                 **删掉这个地址反而丢掉线索**（同上面 `run_tmux_reconcile_poller` 那条）。\
                 ⚠ 它今天不是无人看管的：`the_outer_layer_producers_are_in_the_state_the_doc_claims` \
                 把那一格**翻面**钉着 —— 这个函数要是回来了，那条会红。",
            ),
        ];
        const KW: &[&str] = &[
            "fn", "struct", "enum", "const", "static", "trait", "mod", "type",
        ];

        // ── 收全仓声明：符号名 → 它出现在哪些文件名里
        let mut srcs: Vec<(PathBuf, String)> = Vec::new();
        for root in [
            "src-tauri/src",
            "src-tauri/crates",
            "remote-daemon-proto/src",
        ] {
            srcs.extend(guard_core::scan_tree!(&repo_root().join(root), &["rs"]));
        }
        // 〔08-06 第二次补扫描面〕**把本文件自己也收进来**。
        // `scan_tree!` 按构造摘除调用者（那是 F23「判据读到自己」的防护），
        // 但这里收的是**声明**不是语料 —— 摘掉自己会让「`doc/` 指向本文件里的符号」被误判成腐。
        // ⚠ 这不是假设：头注原本写着「只在这种情况下才会误红」，而 08-06 当天就发生了
        // （`DEVELOPMENT.md` 指向本文件的 `the_backend_test_command_in_the_docs_matches_ci`）。
        // ⇒ 把「已知的例外」变成「已修的缺陷」，头注那句警告随之删掉。
        srcs.push((
            PathBuf::from("doc_claim_registry.rs"),
            include_str!("doc_claim_registry.rs").to_string(),
        ));
        // `build.rs` 是单文件、不在任何被扫的目录下 —— 第一版就漏了它，
        // 于是 `build.rs::emit_daemon_build_id` 被当成「腐了」。**抽取器的扫描面要自己说清楚。**
        let br = repo_root().join("src-tauri/build.rs");
        let br_src = std::fs::read_to_string(&br).expect("读不到 src-tauri/build.rs");
        srcs.push((br, br_src));

        let mut decl: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
            std::collections::BTreeMap::new();
        for (p, raw) in &srcs {
            let fname = p
                .file_name()
                .expect("源文件名")
                .to_string_lossy()
                .to_string();
            // 注释里的 `fn foo` 不算声明 —— 否则「注掉一个函数」这种变异会被判据放过。
            let stripped = guard_core::strip_comment_lines(raw);
            for line in stripped.lines() {
                let mut it = line.split_whitespace().peekable();
                while let Some(tok) = it.next() {
                    if !KW.contains(&tok) {
                        continue;
                    }
                    let Some(next) = it.peek() else { continue };
                    let ident: String = next
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if !ident.is_empty() {
                        decl.entry(ident).or_default().insert(fname.clone());
                    }
                }
            }
        }
        // ★ 抽取器自检 1：收不到足够多的声明 ⇒ 遍历坏了，下面整条会零命中地绿。
        assert!(
            decl.len() > 2000,
            "全仓只抽到 {} 个声明符号 —— 遍历或剥法坏了（建判据当天实测 3073 个 / 133 个源文件）",
            decl.len()
        );

        // ── 收 doc/ 里的 `file.rs::symbol`
        let mut refs: Vec<(String, usize, String, String)> = Vec::new();
        // 〔08-06 扩面〕**不止 `doc/`**：各目录的 `README.md` 同样在教人「去看哪条判据」，
        // 而它们此前不在扫描面里 —— 我当天就在 `e2e/README.md` 里写下一个指针，
        // 于是那个指针**没有任何东西守着**。⇒ 把入口 README 一并收进来。
        // 实测扩面当日：这些 README 里共 11 处这种引用，**解析不到 0 处**（不误红）。
        let mut targets: Vec<PathBuf> = doc_files();
        let base = targets.len();
        targets.extend(entry_readmes());
        // ★ 扩面自检：入口 README 收不到 ⇒ 路径写错了，扩面等于没做。
        assert!(
            targets.len() >= base + 5,
            "入口 README 只收到 {} 个（doc/ 之外）—— 路径写错了，扩面是空转的",
            targets.len() - base
        );
        for p in targets {
            // ⚠ 用**仓相对路径**而不是裸文件名：扩面后有七个 `README.md`，
            // 裸名会让诊断把 `e2e/README.md` 打印成 `doc/README.md` —— 指错地方的诊断
            // 比没有诊断更费时间（本会话反复吃过「读诊断」的亏）。
            let fname = p
                .strip_prefix(repo_root())
                .unwrap_or(&p)
                .to_string_lossy()
                .to_string();
            let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
            for (i, line) in text.lines().enumerate() {
                let b = line.as_bytes();
                let mut from = 0usize;
                while let Some(k) = line[from..].find(".rs::") {
                    let at = from + k;
                    // 往前收路径：只走 ASCII 路径字符 ⇒ 遇到中文（多字节）自然停在字符边界上。
                    let mut s = at;
                    while s > 0 && {
                        let c = b[s - 1] as char;
                        c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '/' || c == '-'
                    } {
                        s -= 1;
                    }
                    let base = line[s..at + 3]
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .to_string();
                    let mut e = at + 5;
                    while e < b.len() && {
                        let c = b[e] as char;
                        c.is_ascii_alphanumeric() || c == '_'
                    } {
                        e += 1;
                    }
                    let sym = line[at + 5..e].to_string();
                    if !sym.is_empty() && !base.is_empty() {
                        refs.push((fname.clone(), i + 1, base, sym));
                    }
                    from = at + 5;
                }
            }
        }
        // ★ 抽取器自检 2：`doc/` 里本来就有几十处 —— 抽到个位数就是剥法坏了。
        assert!(
            refs.len() >= 70,
            "只抽到 {} 处 `file.rs::symbol` —— 剥法坏了（`doc/` 当日 73 处，扩面后另加各 README 11 处）",
            refs.len()
        );

        // ★ 自检 3：例外表保鲜。例外是**欠账**，不是免检章。
        for (sym, why) in EXCEPTIONS {
            let used: Vec<&(String, usize, String, String)> =
                refs.iter().filter(|r| r.3 == *sym).collect();
            assert!(
                !used.is_empty(),
                "例外表里的 `{sym}` 在 `doc/` 里已经没人写了 —— 删掉这一行。\n\
                 （它当初的理由：{why}）"
            );
            let still_needed = used
                .iter()
                .any(|r| !matches!(decl.get(&r.3), Some(fs) if fs.contains(&r.2)));
            assert!(
                still_needed,
                "例外 `{sym}` 现在**解析得到了** —— 删掉这条例外，别让例外表替真判据挡枪。\n\
                 （它当初的理由：{why}）"
            );
        }

        let bad: Vec<String> = refs
            .iter()
            .filter(|r| !EXCEPTIONS.iter().any(|(s, _)| *s == r.3))
            .filter_map(|(f, ln, base, sym)| match decl.get(sym) {
                None => Some(format!(
                    "{f}:{ln}  `{base}::{sym}` —— **全仓找不到这个符号**（改名或删了）"
                )),
                Some(fs) if !fs.contains(base) => Some(format!(
                    "{f}:{ln}  `{base}::{sym}` —— 符号还在，但**搬家了**：现住 {:?}",
                    fs.iter().collect::<Vec<_>>()
                )),
                _ => None,
            })
            .collect();
        assert!(
            bad.is_empty(),
            "`doc/` 点名了这些代码符号，而它们今天对不上：\n{}\n\n\
             ⚠ 这是**停滞式腐坏**的典型形态：改代码的人不会回来改文档，而在本判据之前
             **没有任何东西会因此变红**。两条修法（E12）：① 把文档改对；\
             ② 那句话若只是历史，就写清「已删 / 已改名」并进本条的例外表（带理由）。",
            bad.join("\n")
        );
    }

    /// 〔audit-0805 08-06〕**`doc/` 里点名的仓内文件路径必须解析得到。**
    ///
    /// 与上一条（`file.rs::symbol`）同族、更宽一档：符号那条只看得见 `.rs`，
    /// 而 `doc/` 里点名的还有 `.ts` / `.sh` / `.mjs` / `.json` / `.yml`。
    /// 建判据当天实测 **119 处**带目录的路径引用，逐条核完**真腐 1 处**：
    /// `INVARIANTS.md` 里的 `remote-daemon-proto/src/accounts_query.rs`
    /// —— 那个文件早已搬进 `observe/`，而**没有任何东西会因此变红**（本条即为此建）。
    ///
    /// ⚠ **解析口径用 `git ls-files` 而不是磁盘**：磁盘会把「本机生成、CI 里还不存在」的
    /// 生成物也算成解析得到（`src-tauri/gen/schemas/**` 就是），那样判据在两个环境里结论不同 ——
    /// 而**结论随环境变的判据比没有判据更坏**。生成物走例外表，理由写明。
    ///
    /// ⚠ 匹配用**后缀**：文档常按「隐含根」写（`control/gate.rs` 指的是
    /// `remote-daemon-proto/src/control/gate.rs`）。第一版用全路径相等，
    /// 一口气误报 36 处 —— 又一次**匹配单位比事实小**。
    #[test]
    fn every_repo_path_named_in_the_docs_still_resolves() {
        /// 例外：**解析不到却是对的**。三种形状，每种都在本仓真实出现过。
        const EXCEPTIONS: &[(&str, &str)] = &[
            (
                "src-tauri/gen/schemas/acl-manifests.json",
                "tauri 构建生成物 + gitignore：磁盘上有、`git ls-files` 里没有，且不同环境有无不定",
            ),
            (
                "shared/ccm-wrapper.sh",
                "**历史句**：原文逐字写着「取代已删除的 …」——删掉它反而丢掉「今天为什么没有 wrapper」",
            ),
            (
                "e2e/tmux-guarded-acceptance.sh",
                "**历史句**〔`K-R72` 09-12〕：`INVARIANTS §34` 那一段逐字在说「这套 e2e 的输入源是\
                 那个已被删掉的 builder ⇒ 整套删了」——它点这个路径正是为了说清**哪一套没了**；\
                 删掉这句话，读的人只会看见「三道门少了一层真机验收」而不知道为什么",
            ),
            (
                "src/cards/memory-recall.ts",
                "**示例占位**：原文是「通常新建 `…`」，教人照着建一个，本就不指向现存文件",
            ),
            (
                "code-picture/doc/agents/claude-code.md",
                "**跨仓引用**：另一个仓的语料，本仓解析不到是正常的",
            ),
            (
                "agents/claude-code.md",
                "同上（同一句里的简写形）",
            ),
            (
                "account-ux/MASTERPLAN.md",
                "**计划工作区**住在 `.claude/planned-build/`（另一个 git 仓）",
            ),
            (
                "unify-launch/MASTERPLAN.md",
                "同上",
            ),
            (
                ".claude/planned-build/account-isolation/DESIGN-account-switching.md",
                "同上：计划仓里的设计稿，不在本仓",
            ),
            (
                "/.mcp.json",
                "指的是**用户项目目录**下的 `.mcp.json`（MCP 项目配置），不是本仓文件",
            ),
        ];
        const EXTS: &[&str] = &["rs", "ts", "sh", "mjs", "json", "yml", "toml", "md", "py"];

        let tracked: Vec<String> = {
            let out = std::process::Command::new("git")
                .args(["ls-files"])
                .current_dir(repo_root())
                .output()
                .expect("跑不动 `git ls-files` —— 本判据的解析口径就是它");
            assert!(out.status.success(), "`git ls-files` 非零退出");
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect()
        };
        // ★ 自检 1：文件清单太短 ⇒ 口径坏了，下面会把一切都判成「指不到」。
        assert!(
            tracked.len() > 300,
            "`git ls-files` 只列出 {} 个文件 —— 口径坏了（本仓实测上千个）",
            tracked.len()
        );

        // ── 抽 `doc/` 里反引号包着、**带目录**的路径
        let mut refs: Vec<(String, usize, String)> = Vec::new();
        for p in doc_files() {
            let fname = p
                .file_name()
                .expect("doc 文件名")
                .to_string_lossy()
                .to_string();
            let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
            for (i, line) in text.lines().enumerate() {
                for chunk in line.split('`').skip(1).step_by(2) {
                    let c = chunk.trim();
                    if !c.contains('/') || c.contains(' ') || c.contains("::") {
                        continue;
                    }
                    let Some(ext) = c.rsplit('.').next() else {
                        continue;
                    };
                    if !EXTS.contains(&ext) || c.starts_with("http") {
                        continue;
                    }
                    if !c
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || "_./-".contains(ch))
                    {
                        continue;
                    }
                    refs.push((fname.clone(), i + 1, c.to_string()));
                }
            }
        }
        // ★ 自检 2：数量地板 **+ 锚点**。
        //
        // ⚠ 只有数量地板是**不够**的，这一条是变异当场量出来的：把「带目录才算」那个条件反过来
        // （于是收的是 `lib.rs` 这类**不带目录**的名字），`refs.len()` 照样过 90 ——
        // **地板对「收的是不是同一类东西」完全是瞎的**，它只数个数。
        // 补一个必须在场的锚点，人群换了就当场红。
        assert!(
            refs.len() >= 90,
            "`doc/` 里只抽到 {} 处带目录的路径引用 —— 剥法坏了（建判据当天实测 119 处）",
            refs.len()
        );
        const CANARY: &str = "src/session-backend.ts";
        assert!(
            refs.iter().any(|(_, _, c)| c == CANARY),
            "抽到了 {} 条，但**锚点 `{CANARY}` 不在里面** —— 收的多半不是「带目录的仓内路径」这一类了。\n\
             （数量地板只数个数，换一群东西照样能喂饱它。）",
            refs.len()
        );

        // `doc/` 在仓根下一层 ⇒ 文中的 `../src/README.md` 说的就是仓根的 `src/README.md`。
        // 不归一化就会把五处**完全正确**的相对写法判成腐 —— 判据误报比漏报更快被人关掉。
        let resolves = |c: &str| {
            let c = c.trim_start_matches("../");
            tracked
                .iter()
                .any(|t| t == c || t.ends_with(&format!("/{c}")))
        };

        // ★ 自检 3：例外表保鲜 —— 例外是欠账不是免检章。
        for (path, why) in EXCEPTIONS {
            assert!(
                refs.iter().any(|(_, _, c)| c == path),
                "例外表里的 `{path}` 在 `doc/` 里已经没人写了 —— 删掉这一行。（当初的理由：{why}）"
            );
            assert!(
                !resolves(path),
                "例外 `{path}` 现在**解析得到了** —— 删掉这条例外，别让例外表替真判据挡枪。\n\
                 （当初的理由：{why}）"
            );
        }

        let bad: Vec<String> = refs
            .iter()
            .filter(|(_, _, c)| !EXCEPTIONS.iter().any(|(e, _)| e == c))
            .filter(|(_, _, c)| !resolves(c))
            .map(|(f, ln, c)| format!("  doc/{f}:{ln}  `{c}`"))
            .collect();
        assert!(
            bad.is_empty(),
            "`doc/` 点名了这些仓内路径，而 `git ls-files` 里找不到（含后缀匹配）：\n{}\n\n\
             ⚠ 同 `file.rs::symbol` 那条：**搬家 / 改名 / 删除都让它变假，而此前没有东西会红**。\n\
             修法（E12）：① 把路径改对；② 若那句只是历史或示例，写清楚并进例外表（带理由）。",
            bad.join("\n")
        );
    }

    /// 〔audit-0805 08-06〕**发版版本号六处必须一致**（`package.json` 是权威源，其余对拍）。
    ///
    /// **为什么建它**：`doc/RELEASING.md` 自己逐字记着 ——
    /// 「v3.1→v3.4 **连续四次**发版漏改 README，于是 README 的『当前版本』长期落后一个大版本；
    /// BACKLOG 早把『checklist 里没有 README 这一条』点名为**机制性根因**，
    /// 而根因没修 ⇒ 第四次照样复发」。
    ///
    /// 那次的修法是**在 checklist 里加一行散文**。散文接不住它：第四次复发时 checklist 已经在了。
    /// ⇒ 这正是 E12 ①「送进一条会红的判据」该管的形状，也是 E3 的标准解
    /// （一个事实六个副本 ⇒ 定权威源 + 其余对拍）。
    ///
    /// ⚠ 本条**故意在发版中途也会红**：改了 `package.json` 而 README 还没跟上时它就红 ——
    /// 那不是误报，那是它的岗位（`RELEASING` 的 checklist 要求这几处一起改）。
    ///
    /// ⚠ 建判据当天六处全部是 `3.6.0`，**一处不差** —— 又是「今天干净、但没人守着」：
    /// 这个位置**已经腐过四次**，靠的是人记得，不是机制。
    ///
    /// # 与 `release.yml` 那道 guard 的关系（E3：别造第二个权威源）
    ///
    /// 建完本条的**第二天**才查到：`release.yml` 里早有一道
    /// 「Verify version consistency with tag」，查**四处**（`package.json` / `tauri.conf.json` /
    /// `Cargo.toml` / **`Cargo.lock`**），CHANGELOG 记着它是「防 v2.4.2 漂移事故复发」加的。
    ///
    /// 两者**不是一个事实两个权威源**，因为比的东西不同：
    /// 那道 guard 的权威是 **git tag**（发版时「代码里的版本 == tag」），
    /// 本条的权威是 `package.json`（任何时候「所有副本彼此一致」）。
    ///
    /// ★ 但它给出一条更要紧的教训：**「已经有 guard」不等于「有信号」**。
    /// `release.yml` 逐字写着 `on: push: tags: ['v*']` —— 而〔用 08-05〕已裁定不再 push
    /// ⇒ 那道 guard **今天结构上一次都不会跑**。README 那四次复发也正是发生在
    /// 「代码侧四处有人查、README 没人查」的缝里。
    ///
    /// # 诚实边界：`Cargo.lock` **钉不进本条**（试过，是恒绿的）
    ///
    /// 先把它当第七处加了进来，然后按 E11 造变异（把 lock 里 monitor 包改成 `3.5.0`）——
    /// **测试照样绿**。查下去才明白：`cargo test` 启动时会先解析依赖，
    /// **把 `Cargo.lock` 自动改回与 `Cargo.toml` 一致**（实测：变异后 `3.5.0` → 跑完 `3.6.0`，exit 0）。
    /// ⇒ 任何住在 `cargo test` 里的判据都**看不见** lock 漂移：它在被观察之前就被治好了。
    ///
    /// ⚠ 也不能改成读 `git show HEAD:` 那一份：那样「bump 版本」这个提交本身会被自己挡住
    /// （提交前跑测试 → HEAD 里还是旧版本 → 红 → 提交不了），**造出一个解不开的死结**。
    ///
    /// ⇒ 结论如实记着：lock 这一处**本地钉不住**，唯一能查它的是 `release.yml` 那道
    /// PowerShell 步骤（它不经 cargo 读文件），而那道今天不会跑。这是一条**真的诚实边界**，
    /// 不是「以后补」—— 记进 `ROADMAP §5`。
    ///
    /// ★ 它差一点就成了本仓最讨厌的那种东西：**一条永远不会红的判据**。
    /// 逮住它的不是「测试失败」，是**变异之后诊断栏一个字都没有** —— 只看 exit code 会当它绿了。
    /// **那道版本 guard 被 Linux job「继承」这件事，压在一条 `needs:` 边上**〔08-08〕。
    ///
    /// `release.yml` 的 `build-linux` 头上逐字写着为什么它串在 Windows 之后：
    ///
    /// > `build-windows` 里那道**四处版本号与 tag 一致**的检查因此**被继承** —— 版本漂了
    /// > 先失败，本 job 根本不会起。**不重复实现那道检查**（重复 = 又一个会漂的副本）。
    ///
    /// 那是一条**正确的 E3 决定**（别造第二个权威源），而它的正确性**整个压在
    /// `needs: [build-daemons, build-windows]` 这一行上**。谁为了「发版快一点」把
    /// `build-windows` 从 needs 里摘掉，两件事同时发生，且都不会有人说话：
    ///
    /// 1. **`.deb` 的版本再没人查** —— 那道 guard 正是「防 v2.4.2 漂移事故复发」加的；
    /// 2. 两个 job 会**同时** `action-gh-release`，竞争同一个 release（那正是当初串起来的理由 ①）。
    ///
    /// ⇒ 本条钉两件：那条边还在 · 那道 guard**仍然只有一处**（没被人「顺手也加到 Linux」，
    /// 那会变成第二个会漂的副本，正是上面那段论证要避免的）。
    ///
    /// ⚠ 与上一条的分工：上一条比的是**六处副本彼此一致**（权威是 `package.json`），
    /// 本条不看版本号，只看**那道以 tag 为权威的检查还罩不罩得住 Linux 产物**。
    #[test]
    fn the_linux_job_still_inherits_the_version_guard() {
        let rel = guard_core::strip_hash_comment_lines(
            &std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("仓根")
                    .join(".github/workflows/release.yml"),
            )
            .expect("读不到 release.yml"),
        );
        let lines: Vec<&str> = rel.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.trim_end() == "  build-linux:")
            .unwrap_or_else(|| panic!("`release.yml` 里找不到 `build-linux:` job —— job 名变了或它被删了，本条会零命中地绿"));
        // `needs:` 必须在这个 job 的头部（`steps:` 之前）——不然读到的是别人的。
        let head_end = lines[at..]
            .iter()
            .position(|l| l.trim() == "steps:")
            .unwrap_or_else(|| panic!("`build-linux` 里找不到 `steps:` —— 段界读法坏了"));
        let head = lines[at..at + head_end].join("\n");
        // ⚠ 不用裸 `contains`：`needle_anchor` 棘轮当场把本条判为「语料上的裸匹配」（33→35），
        // 而它是对的 —— 「文件里某处有 `needs:`、某处有 `build-windows`」和
        // 「**那条 needs 上有 build-windows**」是两回事（前者被两行毫不相干的字就满足了）。
        // 改成：先取出那一条 `needs:` 行，再在**那一行**里按词匹配。
        let needs_line = lines[at..at + head_end]
            .iter()
            .find(|l| l.trim_start().starts_with("needs:"))
            .copied()
            .unwrap_or("");
        assert!(
            guard_core::contains_word(needs_line, "build-windows"),
            "`build-linux` 不再依赖 `build-windows` 了。它的头部现在是：\n{head}\n\n\
             ★ 两件事同时发生，且都不会有人说话：\n\
             1. **`.deb` 的版本再没人查** —— 那道「四处版本号与 tag 一致」的检查只住在 \n\
                `build-windows` 里，而 `build-linux` 头注逐字写着「因此被继承 …… \n\
                **不重复实现那道检查**（重复 = 又一个会漂的副本）」。那道 guard 是\n\
                「防 v2.4.2 漂移事故复发」加的。\n\
             2. 两个 job 会**同时** `action-gh-release`，竞争同一个 release —— \n\
                那正是当初把它们串起来的理由 ①。\n\
             ⇒ 真要并行，就得先解决这两件（比如把版本检查提成独立 job 让两边都 needs 它），\n\
             而不是只删这条边。"
        );

        // 那道 guard 仍然**只有一处**：既没被删，也没被「顺手也加到 Linux」。
        let guard_steps = lines
            .iter()
            .filter(|l| guard_core::contains_word(l, "Verify version consistency with tag"))
            .count();
        assert_eq!(
            guard_steps, 1,
            "「Verify version consistency with tag」这道步骤在 `release.yml` 里出现 {guard_steps} 次（应为 1）。\n\
             0 次 = 它被删了（那道 guard 是防 v2.4.2 漂移事故复发的，删之前先说清谁接）；\n\
             ≥2 次 = 有人在 Linux 那边**重复实现**了它 —— 那正是 `build-linux` 头注逐字反对的\n\
             「又一个会漂的副本」（E3）。真要两边都查，就把它提成一个独立 job。"
        );
    }

    #[test]
    fn the_release_version_is_the_same_in_all_six_places() {
        /// 从 `hay` 里按 `needle` 抠出紧随其后的 `X.Y.Z`。
        /// **needle 必须恰好命中一次** —— 命中零次（那行被改写）或多次（抠错地方）都当场红，
        /// 否则这条判据会在「读不到东西」的时候安静地绿。
        fn pick(who: &str, hay: &str, needle: &str) -> String {
            let n = hay.matches(needle).count();
            assert_eq!(
                n, 1,
                "在 {who} 里，锚点 {needle:?} 命中 {n} 次（要求恰好 1 次）——\n\
                 那一行被改写或挪走了。**先修锚点再谈版本对不对**，否则本条会零命中地绿。"
            );
            let at = hay.find(needle).expect("上面已断言命中一次") + needle.len();
            let rest = &hay[at..];
            let end = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(rest.len());
            let v = &rest[..end];
            assert!(
                v.split('.').count() == 3 && v.split('.').all(|s| !s.is_empty()),
                "{who} 在锚点之后抠到的是 {v:?} —— 形状不像 `X.Y.Z`"
            );
            v.to_string()
        }

        let root = repo_root();
        let rd = |p: &str| {
            std::fs::read_to_string(root.join(p)).unwrap_or_else(|e| panic!("读 {p} 失败: {e}"))
        };
        let (pkg, cargo, conf, readme, readme_en) = (
            rd("package.json"),
            rd("src-tauri/Cargo.toml"),
            rd("src-tauri/tauri.conf.json"),
            rd("README.md"),
            rd("README.en.md"),
        );

        // 权威源（E3）：npm 包清单。其余五处都只是它的副本。
        let authority = pick("package.json", &pkg, "\n  \"version\": \"");
        let others: [(&str, String); 5] = [
            (
                "src-tauri/Cargo.toml",
                pick("src-tauri/Cargo.toml", &cargo, "\nversion = \""),
            ),
            (
                "src-tauri/tauri.conf.json",
                pick("src-tauri/tauri.conf.json", &conf, "\n  \"version\": \""),
            ),
            (
                "README.md 抬头那行",
                pick("README.md 抬头那行", &readme, "当前版本: v"),
            ),
            (
                "README.md 「项目当前状态」块",
                pick("README.md 「项目当前状态」块", &readme, "- **版本**：v"),
            ),
            (
                "README.en.md 抬头那行",
                pick("README.en.md 抬头那行", &readme_en, "| Current: v"),
            ),
        ];

        let off: Vec<String> = others
            .iter()
            .filter(|(_, v)| *v != authority)
            .map(|(who, v)| format!("  {who}：{v}"))
            .collect();
        assert!(
            off.is_empty(),
            "版本号对不上。权威源 `package.json` = {authority}，而这几处是别的数：\n{}\n\n\
             ⚠ 这个位置**已经连续腐过四次**（v3.1→v3.4 每次发版都漏改 README，\n\
             `doc/RELEASING.md` 自己记着这件事）。当时的修法是往 checklist 里加一行散文，\n\
             而第四次复发时那行散文已经在了 —— 所以现在由本条判据接着。\n\
             修法：把落后的那几处改成 {authority}（`RELEASING.md § 1` 的 checklist 列了全部落点）。",
            off.join("\n")
        );
    }

    /// 〔audit-0805 08-06〕**文档里写成 `CONST = 数` 的，代码里那个常量必须真是这个数。**
    ///
    /// **这是定框 E12 自己点名的洞**：E12 的 ⚠ 逐字写着「那四个准确的细节数
    /// （`CHUNK_SIZE=600` 等）**一个都不在它的扫描面里**」—— 本模块此前只管
    /// 「状态列」与几种极窄形态，`CONST = 数` 这一族**没人读**。
    ///
    /// **变异实证（先红后信的反面：它当时是绿的）**：把 daemon 生产常量
    /// `REPLY_BURST` 从 8 改成 3 —— daemon 全套 + monitor 全套**都绿**，
    /// 而 `IPC-PROTOCOL.md` 逐字写着「`main.rs::REPLY_BURST = 8`，连发 8 条后强制让位一次」。
    /// 也就是说：**协议文档里的一个行为常量，代码改了不会有任何东西红。**
    ///
    /// 手法沿用本模块的核心做法：**判据不自己写那个数**，从文档里抽出来再与代码比 ——
    /// 于是那个数只有一个家（文档），改代码不改文档就红，改文档不改代码也红。
    #[test]
    fn every_constant_value_quoted_in_the_docs_matches_the_code() {
        /// 长得像常量、其实不是 Rust 常量的。逐条写清它是什么。
        const EXCEPTIONS: &[(&str, &str)] = &[
            (
                "CLAUDECODE",
                "**环境变量**（`CLAUDECODE=1`），不是 Rust 常量",
            ),
            ("CLAUDE_CODE_CHILD_SESSION", "同上，环境变量"),
        ];

        /// 从一行里抽 `NAME = 数`（大写标识符 ≥4 字符）。反引号可有可无。
        fn scan_line(line: &str) -> Vec<(String, String)> {
            let b: Vec<char> = line.chars().collect();
            let mut out = Vec::new();
            let mut i = 0usize;
            while i < b.len() {
                if !(b[i].is_ascii_uppercase()) {
                    i += 1;
                    continue;
                }
                let s = i;
                while i < b.len()
                    && (b[i].is_ascii_uppercase() || b[i].is_ascii_digit() || b[i] == '_')
                {
                    i += 1;
                }
                let name: String = b[s..i].iter().collect();
                if name.len() < 4 {
                    continue;
                }
                let mut j = i;
                while j < b.len() && (b[j] == '`' || b[j] == ' ') {
                    j += 1;
                }
                if j >= b.len() || (b[j] != '=' && b[j] != '：' && b[j] != ':') {
                    continue;
                }
                j += 1;
                while j < b.len() && (b[j] == '`' || b[j] == ' ') {
                    j += 1;
                }
                let vs = j;
                while j < b.len() && (b[j].is_ascii_digit() || b[j] == '_') {
                    j += 1;
                }
                if j > vs {
                    let v: String = b[vs..j].iter().filter(|c| **c != '_').collect();
                    out.push((name, v));
                }
                i = j;
            }
            out
        }

        let mut claims: Vec<(String, usize, String, String)> = Vec::new();
        for p in doc_files() {
            let fname = p
                .file_name()
                .expect("doc 文件名")
                .to_string_lossy()
                .to_string();
            let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
            for (i, line) in text.lines().enumerate() {
                for (n, v) in scan_line(line) {
                    claims.push((fname.clone(), i + 1, n, v));
                }
            }
        }
        // ★ 自检 1 + 锚点：只有数量地板不够（本会话实测过「人群被换掉、地板照样过」）。
        assert!(
            claims.len() >= 5,
            "`doc/` 里只抽到 {} 处 `CONST = 数` —— 剥法坏了（建判据当天实测 6 处）",
            claims.len()
        );
        // ⚠ 锚点**只认名字、不认值** —— 第一版把 `&& v == "8"` 也写进来了，
        // 于是判据自己成了那个数的第二份副本（正是本模块头注警告的形态）：
        // 合法地把常量改成别的数时，红的会是锚点而不是对拍，诊断指错方向。
        assert!(
            claims.iter().any(|(_, _, n, _)| n == "REPLY_BURST"),
            "锚点 `REPLY_BURST` 不在抽到的清单里 —— 收的多半不是这一类了。\n\
             （它是本条的立项样本：改代码不改文档时，全仓一条都不会红。）"
        );

        // ── 代码侧：常量名 → 它被定义成的那些值
        let mut defined: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
            std::collections::BTreeMap::new();
        let mut srcs: Vec<(PathBuf, String)> = Vec::new();
        for root in [
            "src-tauri/src",
            "src-tauri/crates",
            "remote-daemon-proto/src",
        ] {
            srcs.extend(guard_core::scan_tree!(&repo_root().join(root), &["rs"]));
        }
        for (_, raw) in &srcs {
            for line in guard_core::strip_comment_lines(raw).lines() {
                let t = line.trim();
                let rest = match t
                    .strip_prefix("const ")
                    .or_else(|| t.strip_prefix("static "))
                {
                    Some(r) => r,
                    None => match t
                        .strip_prefix("pub const ")
                        .or_else(|| t.strip_prefix("pub static "))
                    {
                        Some(r) => r,
                        None => continue,
                    },
                };
                let Some((name, tail)) = rest.split_once(':') else {
                    continue;
                };
                let name = name.trim();
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                {
                    continue;
                }
                let Some((_, val)) = tail.split_once('=') else {
                    continue;
                };
                let v: String = val
                    .trim()
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '_')
                    .filter(|c| *c != '_')
                    .collect();
                if !v.is_empty() {
                    defined.entry(name.to_string()).or_default().insert(v);
                }
            }
        }
        // ★ 自检 2：代码侧一个常量都收不到 ⇒ 下面会把每条都判成「代码里没有」。
        assert!(
            defined.len() >= 20,
            "全仓只收到 {} 个数值常量定义 —— 剥法坏了",
            defined.len()
        );

        // ★ 自检 3：例外保鲜。
        for (name, why) in EXCEPTIONS {
            assert!(
                claims.iter().any(|(_, _, n, _)| n == name),
                "例外 `{name}` 在 `doc/` 里已经没人写了 —— 删掉这一行。（当初的理由：{why}）"
            );
            assert!(
                !defined.contains_key(*name),
                "例外 `{name}` 现在**真是一个 Rust 常量了** —— 删掉这条例外，让它进对拍。（当初的理由：{why}）"
            );
        }

        let bad: Vec<String> = claims
            .iter()
            .filter(|(_, _, n, _)| !EXCEPTIONS.iter().any(|(e, _)| e == n))
            .filter_map(|(f, ln, n, v)| match defined.get(n) {
                None => None, // 代码里没有同名常量：可能是别的语言/外部约定，不在本条管辖内
                Some(vs) if !vs.contains(v) => Some(format!(
                    "  doc/{f}:{ln}  文档说 `{n} = {v}`，代码里实为 {:?}",
                    vs.iter().collect::<Vec<_>>()
                )),
                _ => None,
            })
            .collect();
        assert!(
            bad.is_empty(),
            "文档写死的常量值与代码对不上：\n{}\n\n\
             ⚠ 立项样本就是这么溜掉的：`REPLY_BURST` 8→3，daemon 与 monitor **两套全绿**。\n\
             修法（E12）：① 把文档改对；② 或者那句话本就不该写死数字 —— 改成指常量名。",
            bad.join("\n")
        );
    }

    /// 〔audit-0805 08-06〕**`scripts/` 里的每个文件都要在它自己的 README 里登记。**
    ///
    /// # 逮到的是一条「找不到」的缺陷
    ///
    /// 08-06 实测：`scripts/` 有三个脚本，而 `scripts/README.md` 的表**只列了 `run.ps1`**。
    /// 漏掉的两个里有 `verify-committed-state.sh` —— 它的头注逐字写着
    /// 「本仓不 push ⇒ CI 从来没见过这些 commit，**所以这道门必须在本机跑**」。
    /// ⇒ 一个「必须本机跑」的门，**照目录 README 是找不到的**；
    /// 本会话是靠 grep 撞见它的，而那不是别人会走的路。
    ///
    /// 这类缺陷不会让任何测试变红，也不会让任何人报错 —— 它只是让**下一个人找不到**。
    /// 本条把「找得到」变成机检。
    ///
    /// ⚠ 只钉**存在性**，不钉描述内容：描述会随脚本演进，钉了就是下一个假陈述
    /// （本模块头注记着 `STATUS_CELLS` 那次教训）。要读细节去看脚本自己的头注。
    #[test]
    fn every_script_in_the_directory_is_listed_in_its_readme() {
        let dir = repo_root().join("scripts");
        let readme =
            std::fs::read_to_string(dir.join("README.md")).expect("读不到 scripts/README.md");
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .expect("读不到 scripts/")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n != "README.md")
            .collect();
        files.sort();
        // ★ 抽取器自检：目录空了或读法坏了 ⇒ 下面会零命中地绿。
        assert!(
            files.len() >= 3,
            "`scripts/` 只扫到 {} 个文件（README 之外）—— 遍历坏了（08-06 实测 3 个）",
            files.len()
        );
        let missing: Vec<&String> = files
            .iter()
            .filter(|n| !readme.lines().any(|l| l.contains(n.as_str())))
            .collect();
        assert!(
            missing.is_empty(),
            "`scripts/` 里这些文件在 `scripts/README.md` 里查不到：{missing:?}\n\n\
             ⚠ 这不会让任何测试变红，也不会让任何人报错 —— 它只是让**下一个人找不到**。\n\
             08-06 实测：那张表当时只有 `run.ps1`，于是照它找不到 `verify-committed-state.sh`，\n\
             而那是全仓**唯一量「提交状态」且必须在本机跑**的门。\n\
             ⇒ 加一行就行；细节写在脚本自己的头注里，别在 README 里抄第二份。"
        );
    }

    /// 〔audit-0805 08-06〕**开发者入口文档里的后端测试命令，必须与 `ci.yml` 逐字相同。**
    ///
    /// # 逮到的是「照它做会少测」
    ///
    /// `doc/DEVELOPMENT.md` 的「跑测试」节此前逐字写着
    /// `cargo test --lib          # 全部单元测试` —— 而 `--lib` **只覆盖根包**，
    /// 六个共享 crate 一条都不跑。新人照入口文档做，得到的是一个**少测**的读数，
    /// 而它长得和全量读数一模一样（都是「ok. N passed」）。
    ///
    /// 这一条属于本会话新命名的那类缺陷：**产物没错，通往它的路是错的**
    /// —— 没有任何测试会因为文档里写错命令而变红。
    ///
    /// # 手法：不在判据里写那个命令
    ///
    /// 命令的**唯一的家是 `ci.yml`**。本条从两边各抽一次再比 ——
    /// 于是改 CI 而不改文档会红，改文档而不改 CI 也会红，
    /// 而判据自己**不持有第三份副本**（`STATUS_CELLS` 那次的教训，见本模块头注）。
    #[test]
    fn the_backend_test_command_in_the_docs_matches_ci() {
        let ci = std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml"))
            .expect("读不到 ci.yml");
        // `rust` job 里那条 `cargo test …` —— 剔注释，只认真会跑的行。
        let cmd = ci
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .find_map(|l| l.strip_prefix("run: "))
            .into_iter()
            .chain(
                ci.lines()
                    .map(str::trim)
                    .filter(|l| !l.starts_with('#'))
                    .filter_map(|l| l.strip_prefix("run: ")),
            )
            .find(|c| c.starts_with("cargo test --workspace"))
            .unwrap_or_else(|| {
                panic!(
                    "`ci.yml` 里找不到 `cargo test --workspace …` 那一步 —— \n\
                     命令的家变了，本条的读法要跟着改（否则它会零命中地绿）。"
                )
            })
            .to_string();

        let dev = std::fs::read_to_string(repo_root().join("doc/DEVELOPMENT.md"))
            .expect("读不到 doc/DEVELOPMENT.md");
        assert!(
            dev.lines().any(|l| l.contains(cmd.as_str())),
            "`doc/DEVELOPMENT.md` 的「跑测试」节里没有 CI 那条命令：\n  {cmd}\n\n\
             ⚠ 它此前写的是 `cargo test --lib` 并标成「全部单元测试」——\n\
             而 `--lib` **只覆盖根包**，六个共享 crate 一条都不跑。\n\
             新人照入口文档做会得到一个**少测**的读数，而它长得和全量读数一模一样。\n\
             ⇒ 命令的唯一的家是 `ci.yml`，文档要与它逐字一致（本条不持有第三份副本）。"
        );
        // ★ 反向，且**扫全 `doc/` 而不只是这一份**〔08-06 第二刀〕。
        //
        // 第一刀只查了 `DEVELOPMENT.md`，而同一条少测命令在 `CONTRIBUTING.md` 里**还有三处**
        // （删完跑 / 发版前 checklist / 新 IPC 命令后的检查）。
        // ⇒ 那正是本仓 F07 记过的「**订正手头那一处，不等于订正那句话**」，我又犯一次。
        //
        // 判法：**裸的 `cargo test --lib`**（后面既没有过滤串也没有 `--`）在 `doc/` 里一处都不许有。
        // 带过滤（`cargo test --lib parser`）与带 `--`（`-- --nocapture` / `-- --ignored`）是
        // 合法的部分跑法，不误伤 —— 建判据当日实测：合法的四处、裸的三处，分得干净。
        //
        // ⚠ 顺带记：第一版的针（同一行出现 `--lib` 与「全部」）**当场命中了我自己的订正句**，
        // 这一版的针（裸命令）**又一次命中它** —— F23「更正时引用旧措辞」在本仓已第四次。
        // 处置沿用仓里既有的那条：**改写订正句，别让它复现原命令**（已改成「`--lib` 那种跑法不是全量」）。
        let mut bare: Vec<String> = Vec::new();
        for p in doc_files() {
            let name = p
                .file_name()
                .expect("doc 文件名")
                .to_string_lossy()
                .to_string();
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            for (i, l) in text.lines().enumerate() {
                let Some((_, after)) = l.split_once("cargo test --lib") else {
                    continue;
                };
                let next = after
                    .trim_start()
                    .split(|c: char| c.is_whitespace())
                    .next()
                    .unwrap_or("");
                let is_partial = !next.is_empty()
                    && !next.starts_with('`')
                    && !next.starts_with('|')
                    && !next.starts_with('+')
                    && !next.starts_with('）')
                    && !next.starts_with(')');
                if !is_partial {
                    bare.push(format!("  doc/{name}:{}  {}", i + 1, l.trim()));
                }
            }
        }
        assert!(
            bare.is_empty(),
            "`doc/` 里这些地方把**裸的** `cargo test --lib` 当成全量跑法：\n{}\n\n\
             ⚠ 它只覆盖**根包**，六个共享 crate 一条都不跑，而读数长得和全量一模一样\n\
             （都是「ok. N passed」）—— 照文档做的人不会察觉自己少测了。\n\
             ⇒ 换成 `ci.yml` 里那条 `--workspace --exclude …`；\n\
             真要跑部分，请带过滤串（`cargo test --lib <模块>`）或 `--`（`-- --nocapture`）。",
            bare.join("\n")
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // `K-P5g` `KP5GD3`：**「读几个」那句话的每一份副本，都被登记住**
    //
    // 分工（照 `STATUS_CELLS` 那一族的形状）：
    //   · 人群 **扫出来**（`env_key_claim_lines`）—— 手写清单描述人群，是本仓最贵的病之一；
    //   · 登记表 **只说「这一份是哪一类」**，不存那个数；
    //   · 那个数从 **生产代码** 数出来（`env_keys_actually_read`）—— 它只有一个家。
    // ⇒ 多写一份副本 ⇒ `every_copy_of_that_sentence_is_registered` 红；
    //   哪份副本上的数与现场对不上 ⇒ `every_registered_copy_says_the_number_we_actually_read` 红。
    // ═════════════════════════════════════════════════════════════════════════

    /// 扫描器的针，**拆成两半、分行写**。
    ///
    /// 🔴 合起来写在同一行上，本文件立刻成为它自己所描述的人群的一员
    /// （登记表会开始登记自己）—— `the_registry_file_itself_stays_out_of_that_population`
    /// 钉着这条性质，**别把这两行并回一行**，也别在本文件里把这两个词写在同一行上。
    const NEEDLE_HEAD: &str = "只抠";
    const NEEDLE_TAIL: &str = "键";

    /// 全仓（`git ls-files` 口径）扫出那句话的每一份副本：`(仓相对路径, 行号, 行文)`。
    fn env_key_claim_lines() -> Vec<(String, usize, String)> {
        const EXTS: &[&str] = &[
            "rs", "ts", "tsx", "sh", "mjs", "json", "yml", "toml", "md", "py",
        ];
        let out = std::process::Command::new("git")
            .args(["ls-files"])
            .current_dir(repo_root())
            .output()
            .expect("跑不动 `git ls-files` —— 本组判据的人群口径就是它");
        assert!(out.status.success(), "`git ls-files` 非零退出");
        let files: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect();
        // ★ 自检：清单太短 ⇒ 口径坏了，下面整组会零命中地绿。
        assert!(
            files.len() > 300,
            "`git ls-files` 只列出 {} 个文件 —— 口径坏了（本仓实测七百多）",
            files.len()
        );
        let mut hits: Vec<(String, usize, String)> = Vec::new();
        for rel in files {
            let Some(ext) = rel.rsplit('.').next() else {
                continue;
            };
            if !EXTS.contains(&ext) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(repo_root().join(&rel)) else {
                continue;
            };
            for (i, l) in text.lines().enumerate() {
                if l.contains(NEEDLE_HEAD) && l.contains(NEEDLE_TAIL) {
                    hits.push((rel.clone(), i + 1, l.to_string()));
                }
            }
        }
        hits
    }

    /// daemon **今天真的**从 `/proc/<pid>/environ` 里读几个环境变量 —— 从生产代码数出来。
    ///
    /// ★ 这个数**只有一个家**（那段生产代码）：不是本文件里的常量，也不是文档里那个词。
    /// 这正是本模块头注那条手法：「判据不自己写那个数 —— 它把数抽出来，再与现场量的比」。
    fn env_keys_actually_read() -> usize {
        let p = repo_root().join("remote-daemon-proto/src/observe/accounts_query.rs");
        let raw = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
        assert!(
            raw.len() > 20_000,
            "只读到 {} 字节的 `accounts_query.rs` —— 没读到真文件，本组在空转",
            raw.len()
        );
        // ⚠ 剥生产段**走 `guard_core` 那一份**（与全树共用同一个区间判定），本文件不另写一条。
        let prod = guard_core::production_code(&raw);
        guard_core::assert_no_test_code("accounts_query.rs", &prod);
        // ⚠ 锚点用**带边界的钉法**，不写裸 `contains`
        //（`needle_anchor_registry` 那条递减棘轮：语料变量上的裸 `contains` 只许比今天少）。
        guard_core::find_pinned(&prod, "fn session_accounts(agent_home: &Path").unwrap_or_else(
            |e| panic!("`accounts_query.rs` 生产段的锚点挪了 —— 下面两条会零命中地绿：{e}"),
        );
        let n = prod.matches("proc_env_var(pid, ").count();
        assert!(
            n > 0,
            "生产段里一处 `proc_env_var(pid, …)` 都没有 —— 抽取器坏了，本组在空转"
        );
        n
    }

    /// N → 中文里写这个数**允许**用的那几个字。读到第三个变量时来这儿补一行。
    fn count_words_for(n: usize) -> &'static [char] {
        match n {
            1 => &['一'],
            2 => &['两', '二'],
            3 => &['三'],
            4 => &['四'],
            _ => panic!("现在读 {n} 个了 —— 来 `count_words_for` 补上这个数中文怎么写"),
        }
    }

    /// 从一份副本里抠出它写着的那个计数词。`None` = 那处没写数（写的是「几」「N」之类）。
    fn count_word_in(line: &str) -> Option<char> {
        let at = line.find(NEEDLE_HEAD)? + NEEDLE_HEAD.len();
        let rest = &line[at..];
        let k = rest.find('个')?;
        rest[..k]
            .chars()
            .next_back()
            .filter(|c| "一二两三四五六七八九十".contains(*c))
    }

    /// ★ 抽取器自检：扫不到东西 / 只扫到一个文件 ⇒ 下面三条会零命中地绿。
    #[test]
    fn the_environ_key_claim_scan_is_not_zero_hit() {
        let hits = env_key_claim_lines();
        assert!(
            hits.len() >= 8,
            "只扫到 {} 份副本（`K-P5g` 建判据当天实测 10 份 / 4 个文件）—— 针或人群坏了",
            hits.len()
        );
        let files: std::collections::BTreeSet<&str> =
            hits.iter().map(|(f, _, _)| f.as_str()).collect();
        assert!(
            files.len() >= 3,
            "只扫到 {} 个文件 —— 人群塌了：{files:?}",
            files.len()
        );
        // 锚点：数量地板对「收的是不是同一类东西」是瞎的（本模块另一条判据现打过这一课）。
        // 🔴 用 `K-P5f` 漏掉的那一处当锚点 —— 它不在场就说明这条判据没在看该看的地方。
        const CANARY: &str = "doc/INVARIANTS.md";
        assert!(
            hits.iter().any(|(f, _, _)| f == CANARY),
            "扫到了 {} 份，但**锚点 `{CANARY}` 不在里面** —— 收的多半不是那句话了",
            hits.len()
        );
        // 生产段那个数抽得出来，否则下面那条恒绿。
        assert!(env_keys_actually_read() >= 1);
    }

    /// ★★ **多一份副本、少一份副本，都红。**
    ///
    /// # 这条买的是什么（`KP5GD3` 的正题）
    ///
    /// `K-P5f` 那一拍订正了三处、漏了第四处，**而没有任何东西因此变红** ——
    /// 因为那句话根本不在任何一张表里。本条把「这句话散在哪几处」变成一件
    /// **有闸看着**的事：再添一份副本，作者必须来这里说清它是哪一类。
    ///
    /// ⚠ 它买不到「那句话说得对不对」（那是评审的活），只买「每一份都在册」。
    #[test]
    fn every_copy_of_that_sentence_is_registered() {
        let hits = env_key_claim_lines();
        let matches_site = |rel: &str, line: &str, site: &(&str, &str, EnvKeyClaim)| {
            rel == site.0 && line.contains(site.1)
        };

        // ① 扫到的每一份都得在册，且**只对上一条**（锚点不许含糊）。
        let mut orphans: Vec<String> = Vec::new();
        for (rel, ln, line) in &hits {
            let n = ENV_KEY_CLAIM_SITES
                .iter()
                .filter(|s| matches_site(rel, line, s))
                .count();
            if n != 1 {
                orphans.push(format!(
                    "  {rel}:{ln}  对上 {n} 条登记（该是 1）\n      {}",
                    line.trim()
                ));
            }
        }
        assert!(
            orphans.is_empty(),
            "\n★★ 这几份副本没在册（或锚点含糊）：\n{}\n\n\
             ⚠ 这句话说的是「daemon 从 `/proc/<pid>/environ` 读几个环境变量」，\
             它在盘上**散着好几份**。`K-P5f` 那一拍改了三处、漏了第四处 ——\n\
             **订正手头那一处，不等于订正那句话**（本模块头注对同一个病记过两次）。\n\
             ⇒ 新写一份副本，就来 `ENV_KEY_CLAIM_SITES` 登记它是哪一类：\n\
             `Asserts`（在断言当下，要过计数词对拍）/ `Quotes`（在引述那句话本身）/\n\
             `OtherSubject`（同句式但主语不是进程环境）。",
            orphans.join("\n")
        );

        // ② 反向：登记表自己也会腐 —— 在册却扫不到，说明那处已经改写/删了。
        let stale: Vec<String> = ENV_KEY_CLAIM_SITES
            .iter()
            .filter(|s| {
                hits.iter()
                    .filter(|(rel, _, line)| matches_site(rel, line, s))
                    .count()
                    != 1
            })
            .map(|(f, a, c)| format!("  {f}  锚点 {a:?}（登记为 {c:?}）"))
            .collect();
        assert!(
            stale.is_empty(),
            "\n这几条登记在盘上对不到**恰好一处**（改写了 / 删了 / 锚点现在能对上多处）：\n{}\n\n\
             ⇒ 那处真没了就删掉这一行；只是挪了就换锚点。**别让登记表替真判据挡枪。**",
            stale.join("\n")
        );
    }

    /// ★★ **每一份「在断言当下」的副本，写的数必须等于生产代码今天真读的那个数。**
    ///
    /// 这条就是 `K-P5f` 漏掉第四处时**本该变红**的那条。
    #[test]
    fn every_registered_copy_says_the_number_we_actually_read() {
        let n = env_keys_actually_read();
        let want = count_words_for(n);
        let hits = env_key_claim_lines();
        let find = |site: &(&str, &str, EnvKeyClaim)| {
            hits.iter()
                .find(|(rel, _, line)| rel == site.0 && line.contains(site.1))
                .cloned()
        };

        let mut bad: Vec<String> = Vec::new();
        for site in ENV_KEY_CLAIM_SITES {
            let Some((rel, ln, line)) = find(site) else {
                continue; // 上一条判据专管「在册却扫不到」，这里不重复报
            };
            let got = count_word_in(&line);
            match site.2 {
                EnvKeyClaim::Asserts => {
                    let ok = got.is_some_and(|c| want.contains(&c));
                    if !ok {
                        bad.push(format!(
                            "  {rel}:{ln}  写的是 {got:?}，而现场是 {n}（该写 {want:?}）\n      {}",
                            line.trim()
                        ));
                    }
                }
                // 🔴 **反洗白**：引述那一类不许悄悄装着一句「正好也对」的断言 ——
                // 否则把一处真断言登记成 `Quotes` 就能绕开上面那格。
                EnvKeyClaim::Quotes | EnvKeyClaim::OtherSubject => {
                    if got.is_some_and(|c| want.contains(&c)) {
                        bad.push(format!(
                            "  {rel}:{ln}  登记成 {:?}，可它写的数（{got:?}）与现场一致\n      \
                             ⇒ 它其实是在**断言当下**，改登记成 `Asserts`。\n      {}",
                            site.2,
                            line.trim()
                        ));
                    }
                }
            }
        }
        assert!(
            bad.is_empty(),
            "\n★★ 「daemon 从 `/proc/<pid>/environ` 读几个环境变量」这句话，\
             盘上这几份与现场对不上：\n{}\n\n\
             现场那个数从生产代码数出来（`accounts_query.rs` 生产段里 `proc_env_var(pid, …)` 的处数 = {n}），\n\
             ⇒ 要么是代码改了而这几份没跟着改（**盘上留了假话**），\n\
             要么是抽取器坏了。⚠ 改的时候**每一份都要改** ——\n\
             `K-P5f` 就是改了三处漏了第四处，而当时没有任何东西会红。",
            bad.join("\n")
        );
    }

    /// 本文件**自己不许进那个人群**：登记表登记自己会变成一条自指的死循环。
    ///
    /// 手法是把针拆成两半分行写（见 `NEEDLE_HEAD` / `NEEDLE_TAIL` 头注）。
    /// 这条钉住那条性质 —— 有人把它们并回一行时当场红，而不是等到人群悄悄多出几条。
    #[test]
    fn the_registry_file_itself_stays_out_of_that_population() {
        let me = include_str!("doc_claim_registry.rs");
        assert!(
            me.len() > 20_000,
            "include_str! 只读到 {} 字节 —— 没读到自己，本条在空转",
            me.len()
        );
        let self_hits: Vec<usize> = me
            .lines()
            .enumerate()
            .filter(|(_, l)| l.contains(NEEDLE_HEAD) && l.contains(NEEDLE_TAIL))
            .map(|(i, _)| i + 1)
            .collect();
        assert!(
            self_hits.is_empty(),
            "本文件第 {self_hits:?} 行把针的两半写到了同一行上 —— \
             登记表于是成了它自己所描述的人群的一员。拆开写。"
        );
        // 非空对照：针本身是有效的（同一把尺子在别处确实抓得到东西）。
        assert!(!env_key_claim_lines().is_empty());
    }
}
