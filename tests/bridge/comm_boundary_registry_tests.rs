//! 通信层边界登记表的判据本体 —— 模块头注（这张表为什么存在 · 成员怎么认 ·
//! 买到什么买不到什么）住 `src/bridge/src/comm_boundary_registry.rs`，不在这里抄第二份。

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

// ════════════════════════════════════════════════════════════════════════════
//  一、边界登记表本体（`设计/05 §8` 步 1 的另一半：画出通信层的边界）
// ════════════════════════════════════════════════════════════════════════════

/// ★★ **边界登记表** —— `(仓根相对路径, 为什么它属于通信层)`。
///
/// 🔴 **〔`设计/05 §8` 步 3，2026-09-20〕人群第一次非空：2 份。**
/// 步 1 逐字「判据先对空集成立，随搬迁逐步收紧」——「收紧」从这一拍开始。
///
/// # ⚠ 步 3 的题面是「把**传输面**圈出来」，而传输面**一份都没进来**
///
/// `§8` 步 3 逐字：「把传输面（SSH / SFTP / 池 / 重连）从 `monitor` 的 Rust 半
/// **圈出来**，业务先不动」。现打的结论是：**那三份今天一份都圈不进来** ——
/// `ssh_source.rs` / `sftp.rs` / `sftp_pool.rs` 的**生产段**里都有业务词，
/// 而 `设计/05 §2` 逐字「**`C1` 的豁免必须为零**」⇒ 不许开口子，只能不圈 ＋ 写清。
/// 逐份咬在哪（词 · 处数 · 判词）写在 `真相源/` 那份读数里，**不在这里抄第二份**。
///
/// ⇒ 下面这两份是**通信层自己的词汇**那一档（`§2` 逐字四样里的「地址」与
/// `§4.5.2` 的「失败语义」），不是传输面。**别把非空读成「传输面进来了」。**
///
/// # 往里加一条要同拍做三件事，缺一当场红
///
/// 1. 给那份文件的**文件头注释**盖上 [`MARK`] 那枚标记（盘上那一侧）；
/// 2. 在这张表里加一行，理由写**为什么它是纯传输**，不是「先放这儿」；
/// 3. 把 `src/bridge/src/comm_boundary_registry.rs` 头注里那句
///    「登记在册的通信层成员：N 份」的 N 改掉（散文那一侧）。
///
/// ⚠ 这张表**不是**豁免清单。进了这张表的文件从此被 `C1`–`C5` ＋ `X1`–`X6`
/// 十一条一起管着 —— 登记是**上锁**，不是**放行**。
const REGISTERED: &[(&str, &str)] = &[
    (
        "src/bridge/src/origin.rs",
        "面 A 的**寻址键**本体。`设计/05 §2` 逐字列了这一层认识的四样东西，第一样是\
         「**地址**（`origin` / 路由键）」；`§4` 那张一层两面图里面 A 的寻址逐字就是 `origin`。\
         它**只是**那个地址：零业务词 · 零读盘 · 零起进程 · 零期限字面量。\
         步 2 刚把它收得更紧（`Unspecified` 退役 ⇒ 地址只有一种线上形状，`null` 进不来）。",
    ),
    (
        "src/bridge/src/backend/control/backend_route.rs",
        "面 A 的**失败语义**本体（`设计/05 §4.5.2`）。它把 `CallError` 翻成三态，\
         判准逐字是「能不能证明这条命令根本没发出去」—— 那就是 `§3.3.1` 的 `reach` \
         在今天这棵树上的样子，也是 `X1` 点名的三个线上类型之一（`CallError`）\
         今天唯一一处**穷尽**的 `match`。它不知道会话/账号/skill/agent/tmux，\
         不读盘、不起进程、不绑端口、不写期限。\
         ⚠ `CallError` 的**定义**所在（同目录 `inbound_client.rs`）**没有**跟着进来：\
         C1 在它身上咬到 `agent` 三处（cc-bus 的 `extras.agent`）、X4 咬到一处 `try_send`。\
         类型的家还在外面，而用它做分流的这一份先进来了 —— 那是 C1 指的下一刀，不是矛盾。",
    ),
];

/// 通信层**对前端的入口符号** —— `(符号名, 说明)`。`C3` 与 `X6` 的人群从这儿派生。
///
/// `设计/05 §3.1`：前端只有两个动作（`call` / `subscribe`）。它们今天还不存在
/// ⇒ 本表为空 ⇒ 「前端调用点全集」是空集。
/// ⚠ 留着这个形态而不是等到那天再加：**空表也要有住址**，
/// 否则 `X6` 落地那天没有地方写人群，只能现编一个 —— 那正是本仓治的「人群靠目录」。
const ENTRIES: &[(&str, &str)] = &[];

/// 成员标记：一份文件属于通信层，当且仅当它的文本里带着这个词。
///
/// ⚠ **为什么是「文件自己带标记」而不是「住在某个目录下」**：恒等的两侧要异源。
/// 目录那种写法的两侧（「扫这个目录」与「表里写的路径」）都由改表的同一个人一次编辑改掉
/// ⇒ 退化成恒真。标记那一侧住在**成员文件自己的文本里**，是另一个人另一次编辑写的。
const MARK: &str = "COMM-LAYER-MEMBER";

// ════════════════════════════════════════════════════════════════════════════
//  二、语料（盘上那一侧）
// ════════════════════════════════════════════════════════════════════════════

/// 语料根 ＋ 每根**明写的**排除名单（`设计/16 §5.4b` 纪律 2、4）。
///
/// ⚠ 两个根**互不包含**（纪律 1）。排除的那两份是本判据自己的两半 ——
/// 它们里面逐字写着 [`MARK`]，收进语料就是「判据在自己的散文里找到了自己」
/// （`scanning_guard_registry` 头注治的那一族，实测栽过五次）。
/// ★ `guard_core::scan_tree_excluding` 自带「名单上每一条都必须真的摘到东西」的自检
/// ⇒ 这两份改名 / 搬走 ⇒ **当场 panic**，不会安静地多扫两份。
const CORPUS_ROOTS: &[(&str, &[&str])] = &[
    ("src", &["src/bridge/src/comm_boundary_registry.rs"]),
    ("tests", &["tests/bridge/comm_boundary_registry_tests.rs"]),
];

/// 语料的后缀面。
///
/// ⚠ **诚实边界**：通信层哪天落一份别的后缀的文件（`.mts` / 无扩展名的脚本 / `.py`），
/// 本判据**一个字都看不见** —— 它既不在盘上那一侧，也就不会与登记表分叉。
/// 挡这一形的不是本条，是下面 [`CORPUS_WITNESS`]：每个后缀各钉一个真住址，
/// 后缀表被改空 / 改错时当场红，改**窄**则由那个后缀自己的见证接住。
const CORPUS_EXTS: &[&str] = &["rs", "ts", "toml"];

/// 语料里**刻意不收**的两块 —— `(路径片段, 为什么)`。
///
/// 照 `653b35eb` 那一拍的教训写成**明写的排除**，不靠目录位置：
/// 那次搬树之后 `evidence/` 跟着 `tests/` 自己回来了，而头注里那句「刻意不收」
/// 当时靠的是位置 ⇒ 审计记录里复述的探针串被喂给了探针。
///
/// ⚠ 下面 [`corpus`] 会**数每一条真的摘掉了几份**，摘到 0 份当场红 ——
/// 「排除悄悄失效」与「排除在生效」在终端上一模一样，那正是本仓反复记的那一形。
const CORPUS_DROP: &[(&str, &str)] = &[
    (
        "/tests/evidence/",
        "审计记录会逐字复述判据的串；喂给判据就是自己证明自己",
    ),
    (
        "/vendor/",
        "第三方代码不为我们的边界投票（它不会盖我们的标记，却会被我们的判据数进语料量）",
    ),
];

/// 语料见证：这几份**逐字的真住址**必须出现在这一趟的扫描面里。
///
/// 🔴 **这是反空真的第二样**（第一样是相等断言，第三样是阳性对照）。
/// 人群为空的日子里，「扫描面是活的」这件事没有任何别的东西能证明：
/// 根写错 / 后缀过滤打空 / 排除摘过头，三种都会让盘上那一侧安静地变成 0，
/// 而 `0 == 登记的 0` **照样绿**。
///
/// ⚠ 刻意**不是**地板（`>= N`）：地板在「变少」方向是瞎的 ——
/// 本仓逐字记过「第一版是 `checked >= 3`，而实测 `checked = 7` ⇒ 余量 2.3 倍，
/// 4 个块可以静默掉出采集面而地板照绿」。这里用的是**逐个住址的集合包含**。
/// ⚠ 也刻意**不是**「把 `CORPUS_ROOTS` 再抄一遍」：抄一份的话，
/// 「根少了一个」与「见证少了一条」会被同一次编辑一起改掉 ⇒ 恒真。
/// 形状照 `scanning_guard_registry` 的 `MUST_BE_IN_REACH`：拿**盘上真有的那一份**当见证。
const CORPUS_WITNESS: &[(&str, &str)] = &[
    ("src/bridge/src/lib.rs", "bridge 那棵 Rust 树 · 后缀 rs"),
    ("src/backend/wire.rs", "backend 那棵 Rust 树 · 后缀 rs"),
    ("src/tabs.ts", "前端那棵 TS 树 · 后缀 ts"),
    (
        "src/bridge/Cargo.toml",
        "清单面（`C2` 的依赖图那一侧要读它）· 后缀 toml",
    ),
    (
        "tests/bridge/guard_support_tests.rs",
        "tests 那棵树 —— 判据剖分之后半个仓的 `.rs` 住在这儿",
    ),
];

/// 走一遍两棵语料树，返回 `(仓根相对路径, 原文)`，已排序。
///
/// ⚠ 走的是 `guard_core::scan_tree_excluding`，**不裸 `read_dir`**
/// （`scanning_guard_registry` 那条递减棘轮明禁，且那条棘轮**只许往下拧**）。
fn corpus() -> Vec<(String, String)> {
    let root = repo_root();
    let mut raw: Vec<(String, String)> = Vec::new();
    for (sub, excluded) in CORPUS_ROOTS {
        for (p, text) in guard_core::scan_tree_excluding(&root.join(sub), CORPUS_EXTS, excluded) {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            raw.push((rel, text));
        }
    }
    let mut kept: Vec<(String, String)> = Vec::new();
    let mut dropped = vec![0usize; CORPUS_DROP.len()];
    for (rel, text) in raw {
        let probe = format!("/{rel}");
        let mut keep = true;
        for (i, (pat, _)) in CORPUS_DROP.iter().enumerate() {
            if probe.contains(*pat) {
                dropped[i] += 1;
                keep = false;
            }
        }
        if keep {
            kept.push((rel, text));
        }
    }
    let dead: Vec<String> = CORPUS_DROP
        .iter()
        .zip(&dropped)
        .filter(|(_, n)| **n == 0)
        .map(|((pat, why), _)| format!("  `{pat}` —— 摘到 0 份（登记的理由：{why}）"))
        .collect();
    assert!(
        dead.is_empty(),
        "语料排除名单上有条目**一份都没摘到**：\n{}\n\n\
         ⇒ 它改名了 / 搬走了 / 片段写错了。这一格非红不可：\n\
         排除悄悄失效之后那批文件会**回到语料里**，而「排除在生效」与「排除是空转」\n\
         在终端上一模一样（`653b35eb` 那一拍的 `evidence/` 回流就是这么发生的）。",
        dead.join("\n")
    );
    kept.sort();
    kept
}

/// 一段文本是不是「自称通信层成员」。
///
/// 走 `guard_core::contains_word`（带边界），不是裸子串 ——
/// 否则 `XCOMM-LAYER-MEMBERS` 这种被撑大的写法会照样命中
/// （`needle_anchor_registry` 治的那一族：匹配单位比事实小）。
fn claims_membership(text: &str) -> bool {
    guard_core::contains_word(text, MARK)
}

/// 盘上那一侧：整棵语料里自称成员的那些路径（已排序、去重）。
fn members_on_disk() -> Vec<String> {
    corpus()
        .into_iter()
        .filter(|(_, text)| claims_membership(text))
        .map(|(rel, _)| rel)
        .collect()
}

/// 登记那一侧。
fn members_registered() -> Vec<String> {
    let mut out: Vec<String> = REGISTERED.iter().map(|(p, _)| (*p).to_string()).collect();
    out.sort();
    out
}

// ════════════════════════════════════════════════════════════════════════════
//  三、共用的那条相等断言 —— 每条判据的第一句
// ════════════════════════════════════════════════════════════════════════════

/// 一份成员：`(仓根相对路径, 生产段)`。
struct Member {
    rel: String,
    prod: String,
}

/// 剥生产段 —— 按后缀分派，**一份文件一种剥法**。
///
/// ⚠ 为什么判据看的是生产段而不是整份文件：注释里写「本层与 tmux 无关」是**散文**，
/// 不是代码。拿整份文件判的话，成员文件连解释自己为什么干净都做不到，
/// 而那种摩擦的终局是有人回来把判据削掉。
/// ⚠ 代价如实记：**注释里长出来的业务词本族判据看不见**。
///
/// 🔴 **三档全部调共享原语，一行剥法都不自己写。**
/// 第一版的 `.ts` 那档内联了一个 `starts_with("//")` 过滤，
/// `structural_scan::every_comment_stripping_transformer_is_registered` 当场逮住它
/// 并逐字问「共享原语 `guard_core::strip_comment_lines` 为什么不够」—— 答案是**够**。
/// ⇒ 那不是登记一条豁免的理由，是改成调它的理由（`E3`：一个事实一个权威源）。
fn production_of(rel: &str, raw: &str) -> String {
    if rel.ends_with(".rs") {
        // 连 `#[cfg(test)]` 整块一起剥（判据要看的是生产段）。
        return guard_core::production_code(raw);
    }
    if rel.ends_with(".toml") {
        return guard_core::strip_hash_comment_lines(raw);
    }
    // `.ts`：没有 `#[cfg(test)]` 这回事，剥注释就够 —— `//` 那套形态与 Rust 同形。
    guard_core::strip_comment_lines(raw)
}

/// ★★ **每条判据的第一句**：先做那条相等断言，再把人群交出去。
///
/// `设计/05 §3.3.6` 逐字：绿的两条理由是 ①「盘上实际条数 == 登记表条数」
/// ②「人群里没有违例」。**① 在这里，② 在各条判据里** —— 一个事实一个住址（`E3`）：
/// 十一条判据不许各写一份自己的相等断言，那样十一份会各自漂。
fn boundary() -> Vec<Member> {
    assert_the_two_sides_agree();
    let root = repo_root();
    REGISTERED
        .iter()
        .map(|(rel, why)| {
            let p = root.join(rel);
            let raw = std::fs::read_to_string(&p).unwrap_or_else(|e| {
                panic!(
                    "登记表里写着 `{rel}`（理由：{why}），而它读不出来：{e}\n\
                     ⇒ 路径漂了 / 文件被删了。**不许当成「那就少判一份」** ——\n\
                     人群缩水与「全都合规」在终端上一模一样。"
                )
            });
            let prod = production_of(rel, &raw);
            Member {
                rel: (*rel).to_string(),
                prod,
            }
        })
        .collect()
}

/// 两侧**两向集合相等** ＋ 扫描面见证。[`boundary`] 与那条独立判据共用它。
fn assert_the_two_sides_agree() {
    let all = corpus();
    let seen: BTreeSet<&str> = all.iter().map(|(rel, _)| rel.as_str()).collect();
    let missing_witness: Vec<String> = CORPUS_WITNESS
        .iter()
        .filter(|(p, _)| !seen.contains(*p))
        .map(|(p, why)| format!("  {p} —— {why}"))
        .collect();
    assert!(
        missing_witness.is_empty(),
        "这几份**应当**在本趟的扫描面里，而一份都没扫到：\n{}\n\n\
         本趟语料共 {} 份（根：{:?} · 后缀：{CORPUS_EXTS:?}）。\n\n\
         🔴 别读成「那些文件没了」—— 它红的多半是**扫描面被改窄了**：\n\
         根少了一个 / 后缀表改了 / 排除名单摘过头。\n\
         ★ 而扫描面一旦打空，下面那条相等断言会变成 `0 == 0` 并**安静地全绿** ——\n\
         人群为空的日子里，这一条是唯一还在说话的东西。",
        missing_witness.join("\n"),
        all.len(),
        CORPUS_ROOTS.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
    );

    let on_disk: BTreeSet<String> = members_on_disk().into_iter().collect();
    let registered: BTreeSet<String> = members_registered().into_iter().collect();
    let unregistered: Vec<&String> = on_disk.difference(&registered).collect();
    let ghosts: Vec<&String> = registered.difference(&on_disk).collect();
    assert!(
        unregistered.is_empty(),
        "这几份文件**自称**通信层成员（文件里盖着那枚标记），却不在登记表里：{unregistered:?}\n\n\
         ⇒ 搬进来了而没人挡 —— `设计/05 §8` 逐字警告过的那一形：\n\
         「否则搬进来的东西没人挡，通信层当天就长业务」。\n\
         ⇒ 处置：往 `REGISTERED` 里加一行并写清**为什么它是纯传输**，\n\
         然后跑一遍 `C1`–`C5` / `X1`–`X6`（它们从此开始管这份文件）。"
    );
    assert!(
        ghosts.is_empty(),
        "登记表里这几条在盘上**找不到对应的成员**：{ghosts:?}\n\n\
         两种可能，都得有人看一眼：\n\
         ① 文件搬走 / 改名了 ⇒ 改登记表里的路径；\n\
         ② 文件还在，但那枚标记被删了 ⇒ 它是不是不该再算通信层了？\n\
         ⚠ **不许直接把这一行从表里删掉了事** —— 那会让人群静默缩水，\n\
         而人群缩水与「全都合规」在终端上一模一样。"
    );
    assert_eq!(
        on_disk.len(),
        registered.len(),
        "盘上 {} 份 · 登记 {} 份 —— 两向集合都查过还对不上，说明取法本身坏了（重复路径？）",
        on_disk.len(),
        registered.len()
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  四、判据清单（元判据用它对拍「这十五条真的在跑」）
// ════════════════════════════════════════════════════════════════════════════

/// `(编号, 判据函数名, 它钉什么)`。**闭集**，与本文件里真实的 `#[test]` 两向相等。
const CRITERIA: &[(&str, &str, &str)] = &[
    (
        "锚",
        "the_boundary_registry_and_the_disk_agree_two_ways",
        "盘上自称成员的集合 == 登记表的集合（两向）＋ 扫描面见证",
    ),
    (
        "散文",
        "the_boundary_registry_says_out_loud_how_big_it_is_today",
        "模块头注那句「登记在册 N 份」== 登记表长度 == 盘上份数（三方）",
    ),
    (
        "识别器",
        "the_membership_scanner_would_see_a_new_member_and_ignore_a_bystander",
        "合成一棵小树：盖了标记的必须被看见，没盖的必须看不见",
    ),
    (
        "C1",
        "c1_no_business_vocabulary_inside_the_boundary",
        "源码不许出现业务词表（`设计/05 §2`，豁免必须为零）",
    ),
    (
        "C2",
        "c2_no_business_crate_dependency_inside_the_boundary",
        "不许依赖任何业务 crate",
    ),
    (
        "C3",
        "c3_the_word_transport_never_crosses_the_boundary",
        "前端发出的请求里不许含 `transport`（`transport` 是本层的内部选择）",
    ),
    (
        "C4",
        "c4_nothing_inside_the_boundary_reads_disk_or_environment",
        "不许读盘、不许读环境变量 —— 凭据由后端交给它",
    ),
    (
        "C5",
        "c5_nothing_inside_the_boundary_spawns_a_process_or_binds_a_port",
        "不许起进程、不许绑端口 —— 它只用别人交给它的通道",
    ),
    (
        "X1",
        "x1_every_match_on_the_three_wire_types_is_exhaustive",
        "对 `CallError` / `Item` / `Reach` 的 `match` 穷尽、零 `_ =>`",
    ),
    (
        "X2",
        "x2_no_deadline_literal_lives_inside_the_boundary",
        "生产段零期限字面量（期限的值归后端）",
    ),
    (
        "X3",
        "x3_every_hop_construction_names_its_reach",
        "`Hop` 的每一个构造点都显式给 `reach`，无默认值",
    ),
    (
        "X4",
        "x4_the_only_way_to_drop_is_to_say_gap",
        "丢弃只能经 `Item::Gap` 表达 —— 零 `try_send`、零静默 drop",
    ),
    (
        "X5",
        "x5_every_budget_until_derivation_only_tightens",
        "`until` 的每一处派生都是 `min`",
    ),
    (
        "X6",
        "x6_every_frontend_call_site_passes_an_explicit_budget",
        "前端对入口的调用点一律显式给 `Budget`",
    ),
    (
        "元",
        "every_criterion_is_on_the_execution_chain",
        "上面这张表与本文件里真实的 `#[test]` 两向相等",
    ),
];

// ════════════════════════════════════════════════════════════════════════════
//  五、锚：相等断言 ＋ 散文对拍 ＋ 识别器阳性对照
// ════════════════════════════════════════════════════════════════════════════

/// ★★ **主锚** —— 盘上那一侧与登记那一侧**两向集合相等**。
///
/// # 它为什么是主锚（而不是模块头注里那句手写的份数）
///
/// 两侧**异源**：盘上那一侧来自成员文件**自己的文本**（搬文件的人写的），
/// 登记那一侧来自 [`REGISTERED`]（立表的人写的）。同源的那种恒等会退化成恒真 ——
/// 一次编辑同时改掉两侧，断言一个字都不会说。
///
/// # 买到 / 买不到
///
/// **买到**：搬进来不登记 ⇒ 红 · 登记了盘上没有 ⇒ 红 · 路径漂了 ⇒ 红 ·
/// 标记被删了 ⇒ 红 · 扫描面被打空 ⇒ 红（见证那一段）。
/// **买不到**：「盖标记盖对了没有」。一份真的在做传输却不盖标记的文件，
/// 本条**一个字都看不见**（`src/bridge/src/comm_boundary_registry.rs` 头注逐字登记过这条边界）。
#[test]
fn the_boundary_registry_and_the_disk_agree_two_ways() {
    assert_the_two_sides_agree();
    let n = REGISTERED.len();
    // 人群为空的那天，这一行是它**说得出口**的形态（`--nocapture` 可见）。
    println!(
        "〔通信层·边界登记表〕今天人群 {n} 份{}；语料 {} 份（根 {:?}）。\
         绿的理由是 `{n} == {n}`，不是「扫不到」。",
        if n == 0 { "（空集）" } else { "" },
        corpus().len(),
        CORPUS_ROOTS.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
    );
}

/// 从模块头注里抠「登记在册的通信层成员：N 份」那个 N。
///
/// 针走 `guard_core::find_pinned`（**恰好一处 ＋ 两侧有边界**）：
/// 那句话在模块头注里出现第二次、或者被撑大，都当场红 ——
/// 「断言指不明是哪一处」正是 `needle_anchor_registry` 那一族。
/// ⚠ 「读的是不是那份文件」由调用方那条 `#[path]` 自检钉住（见下）。
fn population_claimed_in_prose(module_src: &str) -> usize {
    let needle = "登记在册的通信层成员：";
    let at = guard_core::find_pinned(module_src, needle).unwrap_or_else(|e| {
        panic!(
            "在 `src/bridge/src/comm_boundary_registry.rs` 里钉不住那句「登记在册…」：{e}\n\
             ⇒ 那句话被改写 / 被删了 / 出现了第二处。它是散文那一侧的**唯一**住址。"
        )
    });
    let tail = &module_src[at + needle.len()..];
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or_else(|_| {
        panic!(
            "那句话后面跟的不是数字（读到 `{}`）",
            &tail[..tail.len().min(24)]
        )
    })
}

/// ★ **散文那一侧也得对得上** —— 「今天是空的」这句话由机器守着。
///
/// # 它治的是任务书点名的那一格
///
/// 「人群为空时，判据要能说出『今天是空的』而不是静默变绿 —— 两者在终端上一模一样」。
/// 本条把那句话变成**三方相等**：模块头注里的 N · [`REGISTERED`] 的长度 ·
/// 盘上现扫的份数。搬进来一份而忘了改散文 ⇒ 红；把散文改大而盘上没有 ⇒ 红。
///
/// ⚠ **诚实边界**：三条腿里只有「盘上」那条是异源的，另两条（散文与表）
/// 都由改这件事的同一个人写。它买的是「三处不许各说各的」，
/// **不买**「这个数是对的」—— 那一格由上面那条主锚买。
#[test]
fn the_boundary_registry_says_out_loud_how_big_it_is_today() {
    let module_src = include_str!("../../src/bridge/src/comm_boundary_registry.rs");
    let claimed = population_claimed_in_prose(module_src);
    let registered = REGISTERED.len();
    let on_disk = members_on_disk().len();
    assert_eq!(
        claimed, registered,
        "模块头注说「登记在册 {claimed} 份」，而 `REGISTERED` 里是 {registered} 行。\n\
         ⇒ 改表的时候忘了改那句话（或者反过来）。两处必须同拍。"
    );
    assert_eq!(
        registered, on_disk,
        "登记 {registered} 份，盘上自称成员的有 {on_disk} 份 —— 主锚那条会说得更细，先去看它。"
    );
    // 抽取器自检①：`include_str!` 指的真是那份模块，不是随便一份带那句话的文件。
    // 证据选的是**只有那份文件才会有的东西** —— 它把本判据挂进来的那行 `#[path]`。
    let attach = format!(
        "#[{} = \"../../../tests/bridge/comm_boundary_registry_tests.rs\"]",
        "path"
    );
    guard_core::find_pinned(module_src, &attach).unwrap_or_else(|e| {
        panic!(
            "读进来的这份文本里钉不住那行 `#[path]`：{e}\n\
             ⇒ `include_str!` 指错了地方（或者那条挂载改了）。\n\
             ★ 没有这一格的话，指到任何一份复述过那句话的文件上，本条都照样绿。"
        )
    });
    // 抽取器自检②：读进来的不是空文本。
    assert!(
        module_src.len() > 2_000,
        "读进来的模块源码只有 {} 字节 —— 本条在空转",
        module_src.len()
    );
}

/// ★ **识别器的阳性对照** —— 人群为空时，判据买到的全部就是这一条。
///
/// 合成一棵小树：一份盖了标记的、一份没盖的、一份把标记**撑大**了的
/// （`COMM-LAYER-MEMBERSHIP`，`needle_anchor_registry` 治的那一族）。
/// 走的是与真树**同一个** [`claims_membership`] 与 `guard_core::scan_tree_excluding`
/// —— 换一条路子的话，这条对照证明不了真树上那条还活着。
#[test]
fn the_membership_scanner_would_see_a_new_member_and_ignore_a_bystander() {
    // 识别器那一层（纯文本，不碰盘）。
    assert!(
        claims_membership(&format!("//! 一条传输线。{MARK}\n")),
        "识别器认不出盖了标记的文件 —— 上面那条主锚此刻在空转"
    );
    assert!(
        !claims_membership("//! 一条普通的业务线。\n"),
        "识别器把没盖标记的文件也算成了成员"
    );
    assert!(
        !claims_membership(&format!("//! {MARK}SHIP —— 这不是那枚标记\n")),
        "标记被**撑大**之后照样命中 —— 匹配单位比事实小（`needle_anchor_registry` 那一族）"
    );

    // 遍历 ＋ 识别器**合起来**那一层：真的走一趟文件系统。
    let dir = std::env::temp_dir().join(format!(
        "cc-monitor-comm-boundary-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("nested")).expect("造合成树");
    std::fs::write(
        dir.join("nested/a_member.rs"),
        format!("//! {MARK}\nfn f() {{}}\n"),
    )
    .expect("写成员");
    std::fs::write(dir.join("a_bystander.rs"), "fn g() {}\n").expect("写旁观者");
    std::fs::write(dir.join("not_scanned.md"), format!("{MARK}\n")).expect("写非语料后缀");
    let found: Vec<String> = guard_core::scan_tree_excluding(&dir, CORPUS_EXTS, &[])
        .into_iter()
        .filter(|(_, text)| claims_membership(text))
        .map(|(p, _)| {
            p.strip_prefix(&dir)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        found,
        vec!["nested/a_member.rs".to_string()],
        "合成树上的成员没被认准（实得 {found:?}）——\n\
         ⇒ 遍历与识别器**合起来**那一层坏了。真树今天人群为空，\n\
         这条对照是「它还认得出成员」的唯一证据。"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  六、C1–C5：`设计/05 §2` 的铁律
// ════════════════════════════════════════════════════════════════════════════

/// 业务词表 —— `设计/05 §2` 的 `C1` 逐字九个词。
const BUSINESS_WORDS: &[&str] = &[
    "session", "sid", "account", "skill", "mcp", "tmux", "claude", "jsonl", "agent",
];

fn business_words_in(text: &str) -> Vec<&'static str> {
    BUSINESS_WORDS
        .iter()
        .filter(|w| guard_core::contains_word(text, w))
        .copied()
        .collect()
}

/// ★ `C1` —— 源码不许出现业务词表。
///
/// `设计/05 §2` 逐字：「⚠ **`C1` 的豁免必须为零。** 一旦开始豁免，它就变成第三个业务的家」
/// ⇒ 本条**没有白名单，也不给一个**。要留口子，先去改设计。
///
/// **买到**：登记成员的生产段里，那九个词**作为独立的词**一个都不许出现
/// （`guard_core::contains_word` 带边界匹配 ⇒ `sessionize` / `accounting`
/// 这种更长的标识符**不会被误当成命中**，本条不制造那一族假红）。
///
/// **买不到**：
/// ① 注释里的业务词（本条只看生产段，理由见 [`production_of`]）；
/// ② **换了名字的业务** —— 把 `session_id` 改叫 `handle_id` 照样过；
/// ③ 🔴 **业务词被包在更长的标识符里** —— `session_id` / `agent_name` / `mcp_path`
///    这一族**本条一个都看不见**，因为词尾紧跟 `_` 是标识符字符、不构成边界。
///
/// ⚠ ③ 是〔步 3 09-20〕死值验现打出来的，不是推出来的：同一处注
/// `fn touch(&self, session: u8)` ⇒ **当场红**（`["session"]`）；换成
/// `fn touch(&self, session_id: u8)` ⇒ **十五条全绿**。
/// 🔴 本行上一版逐字写着那两种写法「**也不会被它蒙混**」—— **那半句是假的**，
/// 上面那一刀就是反例。假话留在射程说明里比没有说明更坏（读的人会以为这一格有人守着），
/// 所以逐字改掉。
///
/// ⚠ **这不是「顺手把匹配单位放宽」的理由。** 放宽成裸 `contains` 会让
/// `session` 命中 `sessionize`、`sid` 命中 `considered`/`residual` ⇒ 一族假红，
/// 而本仓逐字记过「假红比不查更坏（它会训练人绕过判据）」。
/// **该不该把 `C1` 的匹配单位从「词」改成「标识符里含这个词」是一道设计题**
/// （要连着答「传输自己的 `session`（russh 的那个）怎么办」），
/// 归 `设计/05 §2` 那张表的主人拍 —— **本件只把洞登记出来，不擅自改判据。**
///
/// 词表守的是「别把已知的业务词搬进来」，不是「这一层真的零业务语义」。
#[test]
fn c1_no_business_vocabulary_inside_the_boundary() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .filter_map(|m| {
            let hits = business_words_in(&m.prod);
            (!hits.is_empty()).then(|| format!("  {} —— {hits:?}", m.rel))
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "通信层成员的生产段里出现了业务词：\n{}\n\n\
         `设计/05 §2` 铁律：**通信层不知道什么是会话、账号、skill、agent。**\n\
         它只知道地址（`origin` / 路由键）· 操作名 · 载荷 · 流的订阅与分发。\n\
         ⚠ **豁免必须为零** —— 本条没有白名单，别来加。",
        offenders.join("\n")
    );
    // 阳性对照：人群为空的日子里，这才是本条真正跑过的东西。
    for w in BUSINESS_WORDS {
        // ⚠ 词要**独立成词**：写成 `let _{w}` 的话前一个字符是 `_`（标识符字符）
        //   ⇒ `contains_word` 判它没有边界、不算命中，这条对照会在自己身上栽一次。
        let synthetic = format!("pub fn route(id: &str) -> u8 {{ let {w} = id; 0 }}\n");
        assert!(
            business_words_in(&synthetic).contains(w),
            "词表里写着 `{w}`，识别器却认不出它 —— 词表与识别器脱钩了"
        );
    }
    assert!(
        business_words_in("pub fn route(origin: &str, op: &str, payload: &[u8]) {}\n").is_empty(),
        "一段**只用位置词**的干净代码被判成有业务词 —— 假红比不查更坏"
    );
    assert_eq!(
        pop.len(),
        REGISTERED.len(),
        "人群与登记表对不上 —— 主锚那条会说得更细"
    );
}

/// 业务 crate —— `设计/05 §2` 的 `C2` 逐字五个（`creds-core` 是它特意加粗的那个）。
///
/// ⚠ `usage-core` 今天盘上**不存在**（`设计/50` 把用量整删了）。留在表里是对的：
/// 本表是**禁入名单**，不是现存清单 —— 哪天它回来，这条闸已经在那儿了。
const BUSINESS_CRATES: &[&str] = &[
    "branch-core",
    "usage-core",
    "acct-core",
    "search-core",
    "creds-core",
];

/// 一份文本里引到了哪几个业务 crate（连字符形与下划线形都认）。
fn business_crates_in(text: &str) -> Vec<&'static str> {
    BUSINESS_CRATES
        .iter()
        .filter(|c| {
            let underscored = c.replace('-', "_");
            guard_core::contains_word(text, c) || guard_core::contains_word(text, &underscored)
        })
        .copied()
        .collect()
}

/// ★ `C2` —— 不许依赖任何业务 crate。
///
/// **买到**：登记成员（`.rs` 的 `use` / 路径，`.toml` 的依赖表）里引到那五个 crate ⇒ 红。
/// **买不到**：① **传递依赖** —— 本条读的是文本，不是 `cargo metadata` 的依赖图；
/// 经由第三个 crate 间接吃进 `creds-core` 它看不见。那一格要等通信层真有自己的
/// `Cargo.toml` 之后才判得了（缺的证据：一份该 crate 的 `cargo tree` 读数）。
/// ② **别的业务 crate** —— 名单是 `设计/05 §2` 那五个，第六个业务 crate 不在人群里。
#[test]
fn c2_no_business_crate_dependency_inside_the_boundary() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .filter_map(|m| {
            let hits = business_crates_in(&m.prod);
            (!hits.is_empty()).then(|| format!("  {} —— {hits:?}", m.rel))
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "通信层成员引到了业务 crate：\n{}\n\n\
         `设计/05 §2` `C2`：通信层是纯基础设施，业务 crate 一个都不许依赖。\n\
         ⇒ 真需要那份数据，让**后端交给它**（`C4` 是同一句话的另一面）。",
        offenders.join("\n")
    );
    for c in BUSINESS_CRATES {
        let dashed = format!("{c} = {{ path = \"../{c}\" }}\n");
        assert!(
            business_crates_in(&dashed).contains(c),
            "禁入名单里写着 `{c}`，识别器认不出它的清单形 —— 名单与识别器脱钩了"
        );
        let used = format!("use {}::Thing;\n", c.replace('-', "_"));
        assert!(
            business_crates_in(&used).contains(c),
            "禁入名单里写着 `{c}`，识别器认不出它的 `use` 形 —— 名单与识别器脱钩了"
        );
    }
    assert!(
        business_crates_in("use guard_core::scan_tree;\nshell-quote-core = \"1\"\n").is_empty(),
        "非业务 crate 被判成业务 crate —— 假红比不查更坏"
    );
}

/// 一行里既有公开面的记号、又有 `transport` 这个词。
fn public_line_leaks_transport(line: &str) -> bool {
    let public = line.contains("pub ") || line.contains("export ");
    public && guard_core::contains_word(line, "transport")
}

/// ★ `C3` —— 前端发出的请求里不许含 `transport`。
///
/// `设计/05 §3.2` 逐字：「`transport` 是通信层的**内部**选择，前端不知道」
/// ⇒ 这个词在层**内部**是合法的，只有跨出边界的那一面不许有它。
///
/// 本条按后缀分两档判：
/// - **`.rs` 成员**：只判**公开面那几行**（含 `pub ` 的行）。层内部的
///   `let transport = pick(origin);` 是对的，不该红。
/// - **`.ts` 成员**：整段生产段零 `transport` —— TS 那一侧**就是**前端的脸，
///   `§3.2` 逐字「前端不知道」。
///
/// **买不到**：① `.rs` 里 `pub struct` 的字段若不带 `pub`（同模块可见），本条看不见；
/// ② 换个名字（`via` / `channel_kind`）传同一件事，本条一个字都不说。
/// 这两格要的是类型层的真检查（`ts-rs` 导出面对拍），今天**判不了** ——
/// 缺的证据是通信层的第一版类型定义。
#[test]
fn c3_the_word_transport_never_crosses_the_boundary() {
    let pop = boundary();
    let mut offenders: Vec<String> = Vec::new();
    for m in &pop {
        if m.rel.ends_with(".ts") {
            if guard_core::contains_word(&m.prod, "transport") {
                offenders.push(format!("  {} —— TS 那一侧整段不许出现这个词", m.rel));
            }
            continue;
        }
        for (i, line) in m.prod.lines().enumerate() {
            if public_line_leaks_transport(line) {
                offenders.push(format!("  {}:{} —— `{}`", m.rel, i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "`transport` 漏到了通信层的公开面上：\n{}\n\n\
         `设计/05 §3.2`：**`transport` 是通信层的内部选择，前端不知道。**\n\
         前端只给 `origin`（`§3.1`：本机也带值，不是 `null`），选哪条路是本层的事。",
        offenders.join("\n")
    );
    assert!(
        public_line_leaks_transport("    pub transport: Transport,"),
        "公开面上的 `transport` 认不出来 —— 上面那条在空转"
    );
    assert!(
        public_line_leaks_transport("export type Req = { transport: string };"),
        "TS 导出面上的 `transport` 认不出来 —— 上面那条在空转"
    );
    assert!(
        !public_line_leaks_transport("    let transport = pick_transport(&origin);"),
        "层**内部**选传输被判成违例 —— `§3.2` 逐字说那是它的本职，假红比不查更坏"
    );
    // `X6` 的人群也从入口表来，这里顺手钉住「入口表与它的读者同源」那一格不成立：
    assert_eq!(
        ENTRIES.len(),
        ENTRIES
            .iter()
            .map(|(n, _)| *n)
            .collect::<BTreeSet<_>>()
            .len(),
        "入口表里有重名 —— 人群会被数两遍"
    );
}

/// `C4` 的形状表 —— `(串, 出处)`。
///
/// 前三条是 `设计/05 §2` 逐字点名的（`read_to_string` / `File::open` / `env::var`）；
/// 后三条是同族的别名，本件补的 —— 补的理由：只挡三种写法等于给第四种写法留门，
/// 而「换个写法就过」在本仓有记录（`readonly_guard` 的前身栽过）。
/// 🔴 **串一律运行时拼**：写成字面量的话，本文件自己就是一处「读盘点」，
/// 而 `local_read_surface_registry` / `write_site_registry` 那几张表会把它数进去。
fn disk_and_env_needles() -> Vec<(String, &'static str)> {
    vec![
        (format!("read_to_{}(", "string"), "§2 C4 逐字"),
        (format!("File::{}(", "open"), "§2 C4 逐字"),
        (format!("env::{}(", "var"), "§2 C4 逐字"),
        (format!("env::{}_os(", "var"), "同族别名（本件补）"),
        (format!("OpenOptions::{}", "new"), "同族别名（本件补）"),
        (format!("fs::{}(", "read"), "同族别名（本件补）"),
    ]
}

/// ★ `C4` —— 不许读盘、不许读环境变量。
///
/// `设计/05 §2.1`：这是用户那句「**key 什么的这些应该要归后端管，通信只负责流量**」
/// 的操作化 —— 凭据不是"它去拿"，是"后端给它"。
///
/// **买不到**：① `include_str!` 那种**编译期**读盘；② 经由别的 crate 间接读盘；
/// ③ 「它拿到的那张表对不对」。本条只买「这一层自己不伸手」。
#[test]
fn c4_nothing_inside_the_boundary_reads_disk_or_environment() {
    let pop = boundary();
    let needles = disk_and_env_needles();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            needles
                .iter()
                .filter(|(n, _)| m.prod.contains(n.as_str()))
                .map(|(n, why)| format!("  {} —— `{n}`（{why}）", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "通信层成员自己去读盘 / 读环境变量了：\n{}\n\n\
         `设计/05 §2.1` 把用户那句话操作化成这一条：\n\
         「key 什么的这些应该要归后端管，**通信只负责流量**」\n\
         ⇒ 凭据、配置、账号映射全部由后端**交给它**（步 4：`creds.rs` ＋ `table.rs` 搬去后端）。",
        offenders.join("\n")
    );
    for (n, why) in &needles {
        let synthetic = format!("pub fn boot() {{ let _ = {n}\"x\"); }}\n");
        assert!(
            needles.iter().any(|(m, _)| synthetic.contains(m.as_str())),
            "形状表里写着 `{n}`（{why}），识别器却认不出自己造的那处 —— 表与识别器脱钩了"
        );
    }
    assert!(
        !needles
            .iter()
            .any(|(n, _)| "pub fn feed(table: Table) { use_it(table) }\n".contains(n.as_str())),
        "一段**由后端喂进来**的干净代码被判成读盘 —— 假红比不查更坏"
    );
}

/// `C5` 的形状表 —— 前两条是 `设计/05 §2` 逐字点名的，其余是同族别名。
fn spawn_and_bind_needles() -> Vec<(String, &'static str)> {
    vec![
        (format!("Command::{}(", "new"), "§2 C5 逐字"),
        (format!("TcpListener::{}(", "bind"), "§2 C5 逐字"),
        (format!("UnixListener::{}(", "bind"), "同族别名（本件补）"),
        (format!("UdpSocket::{}(", "bind"), "同族别名（本件补）"),
    ]
}

/// ★ `C5` —— 不许起进程、不许绑端口。
///
/// `设计/05 §2.1`：用户那句「**我不希望一个 app 占用三个端口、三个进程**」的操作化 ——
/// 谁起进程、谁绑端口是**后端生命周期**的事，通信层无权。
/// 它只使用别人交给它的通道（一个 `AsyncRead + AsyncWrite`）。
///
/// **买不到**：① 经由别的 crate 间接起进程 / 绑端口；② **发起连接**（`connect`）——
/// 那是本层的本职，刻意不在名单里；③ 「交给它的那条通道是谁开的」。
#[test]
fn c5_nothing_inside_the_boundary_spawns_a_process_or_binds_a_port() {
    let pop = boundary();
    let needles = spawn_and_bind_needles();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            needles
                .iter()
                .filter(|(n, _)| m.prod.contains(n.as_str()))
                .map(|(n, why)| format!("  {} —— `{n}`（{why}）", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "通信层成员自己起进程 / 绑端口了：\n{}\n\n\
         `设计/05 §2.1`：「我不希望一个 app 占用三个端口、三个进程」\n\
         ⇒ 生命周期归后端，本层只**使用**别人交给它的那条通道。",
        offenders.join("\n")
    );
    for (n, why) in &needles {
        let synthetic = format!("pub fn boot() {{ let _ = {n}\"x\"); }}\n");
        assert!(
            needles.iter().any(|(m, _)| synthetic.contains(m.as_str())),
            "形状表里写着 `{n}`（{why}），识别器却认不出自己造的那处 —— 表与识别器脱钩了"
        );
    }
    assert!(
        !needles.iter().any(
            |(n, _)| "pub async fn run(io: impl AsyncRead + AsyncWrite) {}\n".contains(n.as_str())
        ),
        "一段**只用交给它的通道**的干净代码被判成起进程 —— 假红比不查更坏"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  七、X1–X6：`设计/05 §3.3.6` 的签名判据（与 C1–C5 共用上面那张表）
// ════════════════════════════════════════════════════════════════════════════

/// 从 `at` 起第一对配平花括号里的内容。
///
/// ⚠ **粗尺子，如实登记**：它不解析字符串与字符字面量 ——
/// 一个写在字符串里的 `}` 会让它提前收尾。本仓的判据语料里满是合成源码串，
/// 精确解析一次失步就整段跟着错，而错的方向是**静默的绿**；
/// 宁可用一把粗而稳的尺子（`scanning_guard_registry::guard_fn_item` 的头注同此论证）。
fn braced_block(text: &str, at: usize) -> Option<&str> {
    let open = at + text[at..].find('{')?;
    let mut depth = 0usize;
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open + 1..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// `设计/05 §3.3.0` 那三个线上类型 —— `X1` 的人群按它们认。
const WIRE_TYPES: &[&str] = &["CallError", "Item", "Reach"];

/// 生产段里「对那三个类型的 `match`」中带 `_ =>` 的那些（返回每处的片段）。
fn inexhaustive_wire_matches(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mark = format!("{} ", "match");
    for (at, _) in prod.match_indices(mark.as_str()) {
        let Some(block) = braced_block(prod, at) else {
            continue;
        };
        if !WIRE_TYPES
            .iter()
            .any(|t| guard_core::contains_word(block, t))
        {
            continue;
        }
        let wildcard = format!("_ {}", "=>");
        if block.contains(wildcard.as_str()) {
            out.push(block.trim().chars().take(120).collect::<String>());
        }
    }
    out
}

/// ★ `X1` —— 对 `CallError` / `Item` / `Reach` 的 `match` 必须穷尽，零 `_ =>`。
///
/// **买不到**：① 类型对不对（本条按**名字**认，不做类型检查）；
/// ② `_ =>` 之外的兜底写法（`other =>`、`e if true =>`）；
/// ③ 写在字符串里的花括号会让块提前收尾（见 [`braced_block`]）。
#[test]
fn x1_every_match_on_the_three_wire_types_is_exhaustive() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            inexhaustive_wire_matches(&m.prod)
                .into_iter()
                .map(|blk| format!("  {} —— `{blk}`", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "这几处对线上类型的 `match` 用了通配臂：\n{}\n\n\
         `设计/05 §3.3.6` `X1`：三个类型的每一处 `match` 必须**穷尽、零 `_ =>`**。\n\
         ⇒ 通配臂的代价是：错误面加一个变体时，**没有任何地方会红** —— \n\
         新错误被悄悄归进旧分支，而那正是这一层最不能出的事。",
        offenders.join("\n")
    );
    for t in WIRE_TYPES {
        let bad = format!("    match e {{ {t}::A => 1, _ => 0 }}\n");
        assert_eq!(
            inexhaustive_wire_matches(&bad).len(),
            1,
            "识别器认不出 `{t}` 上的通配臂 —— `X1` 此刻在空转"
        );
        let good = format!("    match e {{ {t}::A => 1, {t}::B => 0 }}\n");
        assert!(
            inexhaustive_wire_matches(&good).is_empty(),
            "穷尽的 `{t}` match 被判成违例 —— 假红比不查更坏"
        );
    }
    assert!(
        inexhaustive_wire_matches("    match kind { Foo::A => 1, _ => 0 }\n").is_empty(),
        "**别的**类型上的通配臂被算进了 `X1` 的人群 —— 人群要等于它真正证明的那件事"
    );
}

/// 生产段里的期限字面量。
fn deadline_literals(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    let dur = concat!("Duration", "::from_");
    for (at, _) in prod.match_indices(dur) {
        out.push(prod[at..].chars().take(40).collect::<String>());
    }
    // 裸秒常量：`const 名字里带 SEC/MS/TIMEOUT/DEADLINE 的 = 数字`。
    for line in prod.lines() {
        let t = guard_core::strip_visibility(line.trim_start());
        if !t.starts_with("const ") {
            continue;
        }
        let deadlineish = ["SEC", "MS", "TIMEOUT", "DEADLINE", "INTERVAL"]
            .iter()
            .any(|k| t.contains(k));
        if deadlineish && t.chars().any(|c| c.is_ascii_digit()) {
            out.push(t.trim().to_string());
        }
    }
    out
}

/// ★ `X2` —— 通信层生产段**零期限字面量**。
///
/// `设计/05 §3.3.2`：**值归后端 · 执行归通信层 · 说法归调用方**。
/// 期限的**值**一个字都不许写在这一层里 —— 它是后端交下来的。
///
/// **买不到**：① 从别处 `use` 进来的常量（本条只看这一层自己的文本）；
/// ② 名字不带那五个关键词的裸秒常量（`const GRACE: u64 = 30;`）；
/// ③ **`§9` 逐字**：具体该给多少秒**判不了** —— 缺的证据是各跳端到端时延的 p50/p99。
///    本条守的是「值不住在这里」，不是「值定对了」。
#[test]
fn x2_no_deadline_literal_lives_inside_the_boundary() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            deadline_literals(&m.prod)
                .into_iter()
                .map(|s| format!("  {} —— `{s}`", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "通信层生产段里出现了期限字面量：\n{}\n\n\
         `设计/05 §3.3.2`：**值归后端**。期限从 `Budget` 里进来，不在这一层里写死。",
        offenders.join("\n")
    );
    assert_eq!(
        deadline_literals(&format!(
            "let d = {}(30);\n",
            concat!("Duration", "::from_secs")
        ))
        .len(),
        1,
        "识别器认不出期限字面量 —— `X2` 此刻在空转"
    );
    assert_eq!(
        deadline_literals("const CALL_TIMEOUT_SECS: u64 = 30;\n").len(),
        1,
        "识别器认不出裸秒常量 —— `X2` 此刻在空转"
    );
    assert!(
        deadline_literals("pub fn call(budget: Budget) -> Reach { budget.until }\n").is_empty(),
        "一段**只收 `Budget`** 的干净代码被判成写死期限 —— 假红比不查更坏"
    );
}

/// `Hop` 的构造点里**没给 `reach`** 的那些。
fn hop_sites_without_reach(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    let hop = format!("{} {{", "Hop");
    for (at, _) in prod.match_indices(hop.as_str()) {
        let Some(block) = braced_block(prod, at) else {
            continue;
        };
        let defaulted = block.contains(concat!("..Default::", "default()"));
        if !guard_core::contains_word(block, "reach") || defaulted {
            out.push(block.trim().chars().take(120).collect::<String>());
        }
    }
    out
}

/// ★ `X3` —— `CallError::Hop` 的每一个构造点都**显式给 `reach`**。
///
/// `设计/05 §3.3.1`：错误分三层（传输错 · 对端错 · 我们自己错），
/// 而 `reach`（这一跳到底走到哪儿了）是调用方唯一能据以决定"要不要重试"的东西。
/// 给它默认值 = 把"不知道"伪装成"知道"。
///
/// **买不到**：① 构造点写成 `Hop::new(...)` 那种函数形（本条只认结构体字面量）；
/// ② `reach` 填得**对不对** —— `§3.3.6` 逐字：那一格「只能靠真机实验」。
#[test]
fn x3_every_hop_construction_names_its_reach() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            hop_sites_without_reach(&m.prod)
                .into_iter()
                .map(|s| format!("  {} —— `{s}`", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "这几处 `Hop` 构造点没有显式给 `reach`：\n{}\n\n\
         `设计/05 §3.3.6` `X3`：**无默认值、无 `..Default::default()`**。\n\
         ⇒ `reach` 是调用方判断「能不能重试」的唯一依据；给它默认值 =\n\
         把「不知道走到哪儿了」伪装成「知道」。",
        offenders.join("\n")
    );
    assert_eq!(
        hop_sites_without_reach("let e = Hop { code: 1 };\n").len(),
        1,
        "识别器认不出缺 `reach` 的构造点 —— `X3` 此刻在空转"
    );
    assert_eq!(
        hop_sites_without_reach(&format!(
            "let e = Hop {{ reach: r, {} }};\n",
            concat!("..Default::", "default()")
        ))
        .len(),
        1,
        "带默认填充的构造点没被逮住 —— 那正是 `X3` 点名禁的写法"
    );
    assert!(
        hop_sites_without_reach("let e = Hop { reach: Reach::Sent, code: 1 };\n").is_empty(),
        "显式给了 `reach` 的构造点被判成违例 —— 假红比不查更坏"
    );
}

/// 静默丢弃的两种形状。
fn silent_drop_sites(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    let try_send = concat!("try_", "send");
    for (at, _) in prod.match_indices(try_send) {
        out.push(prod[at..].chars().take(60).collect::<String>());
    }
    for line in prod.lines() {
        let t = line.trim();
        if t.starts_with("let _ =") && guard_core::contains_word(t, "send") {
            out.push(t.to_string());
        }
    }
    out
}

/// ★ `X4` —— 丢弃只能经 `Item::Gap` 表达。
///
/// `设计/05 §3.3.4`：**回推优先 · 推不动才丢 · 丢必须说**。
/// `try_send` 与 `let _ = …send(…)` 都是「推不动就当没发生」——
/// 订阅方**看不出**中间少了东西，而流的语义整个塌在这一点上。
///
/// **买不到**：① 别的丢弃写法（`if ch.capacity() == 0 { return; }`）；
/// ② **`§9` 逐字**：SSH 那条路上回推到底推不推得回去**判不了** ——
///    缺的证据是一次真机的慢消费者实验。本条只买「丢的时候有没有说」。
#[test]
fn x4_the_only_way_to_drop_is_to_say_gap() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            silent_drop_sites(&m.prod)
                .into_iter()
                .map(|s| format!("  {} —— `{s}`", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "这几处在**静默地**丢东西：\n{}\n\n\
         `设计/05 §3.3.4`：**回推优先 · 推不动才丢 · 丢必须说**。\n\
         ⇒ 丢了就发一个 `Item::Gap`，让订阅方知道中间缺了东西。",
        offenders.join("\n")
    );
    assert_eq!(
        silent_drop_sites(&format!("ch.{}(item);\n", concat!("try_", "send"))).len(),
        1,
        "识别器认不出那种「推不动就算了」的发法 —— `X4` 此刻在空转"
    );
    assert_eq!(
        silent_drop_sites("    let _ = ch.send(item);\n").len(),
        1,
        "识别器认不出被丢掉的发送结果 —— `X4` 此刻在空转"
    );
    assert!(
        silent_drop_sites("    ch.send(item).await?;\n    ch.send(Item::Gap(n)).await?;\n")
            .is_empty(),
        "一段**回推 ＋ 报 Gap** 的干净代码被判成静默丢弃 —— 假红比不查更坏"
    );
}

/// `until` 的派生点里**不是只收紧**的那些。
fn loose_until_derivations(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in prod.lines() {
        let t = line.trim();
        if !guard_core::contains_word(t, "until") {
            continue;
        }
        let Some(eq) = t.find('=') else { continue };
        if t[eq..].starts_with("==") {
            continue;
        }
        let rhs = &t[eq + 1..];
        let tightening = rhs.contains(".min(") || rhs.contains("min(");
        let loosening = rhs.contains('+')
            || rhs.contains(".max(")
            || rhs.contains("max(")
            || rhs.contains("now(");
        if loosening || !tightening {
            out.push(t.to_string());
        }
    }
    out
}

/// ★ `X5` —— `Budget.until` 的每一处派生都是 `min`。
///
/// `设计/05 §3.3.2`：期限沿着调用链**只许越来越紧**。
/// 一处 `+` 或一次重新 `now() + …`，就把上游给的那个期限放宽了 ——
/// 而放宽之后没有任何人会发现：请求只是"慢了一点"。
///
/// **买不到**：① 跨行的派生（本条按行判）；② `min` 之外语义等价的收紧写法
/// （`if a < b { a } else { b }`）会被判成违例 —— 那是**假红**，
/// 而假红在本仓的账上比漏判更危险（它会训练人绕过判据）。
///    ⇒ 真撞上了，**改写代码去用 `min`**，别来放宽本条。
#[test]
fn x5_every_budget_until_derivation_only_tightens() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            loose_until_derivations(&m.prod)
                .into_iter()
                .map(|s| format!("  {} —— `{s}`", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "`until` 的这几处派生不是「只收紧」：\n{}\n\n\
         `设计/05 §3.3.6` `X5`：**每一处派生都是 `min` —— 零 `+` / `max` / 重新 `now() + …`**。\n\
         ⇒ 放宽上游给的期限之后，症状只是「慢了一点」，没有任何人会发现。",
        offenders.join("\n")
    );
    assert_eq!(
        loose_until_derivations("let until = now() + step;\n").len(),
        1,
        "识别器认不出重新起算的期限 —— `X5` 此刻在空转"
    );
    assert_eq!(
        loose_until_derivations("let until = parent.until.max(mine);\n").len(),
        1,
        "识别器认不出被放宽的期限 —— `X5` 此刻在空转"
    );
    assert!(
        loose_until_derivations("let until = parent.until.min(mine);\n").is_empty(),
        "一处**收紧**的派生被判成违例 —— 假红比不查更坏"
    );
}

/// 前端对某个入口的调用点里，**没显式给 `Budget`** 的那些。
fn call_sites_without_budget(text: &str, entry: &str) -> Vec<String> {
    let mut out = Vec::new();
    let head = format!("{entry}(");
    for (at, _) in text.match_indices(head.as_str()) {
        // 边界：`recall(` 不算 `call(`。
        let before_is_ident = text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if before_is_ident {
            continue;
        }
        let tail = &text[at + head.len()..];
        let args: String = tail
            .chars()
            .take_while(|c| *c != ')' && *c != ';')
            .collect();
        if !(args.contains("Budget") || guard_core::contains_word(&args, "budget")) {
            out.push(format!("{head}{args})"));
        }
    }
    out
}

/// ★ `X6` —— 前端侧的调用点**一律显式给 `Budget`**，零处"用库里的默认"。
///
/// `设计/05 §3.3.2`：**说法归调用方**。一个藏在库里的默认期限意味着
/// 「这条路该等多久」没有任何调用方想过，而 `§9` 逐字记着：
/// 那 8 条无期限路径今天**连实测分布都没有**。
///
/// 人群 = [`ENTRIES`] × 前端语料。两张表今天都空 ⇒ 0 个调用点。
///
/// **买不到**：① 跨行的实参表（本条只取到第一个 `)` 或 `;`）；
/// ② 把 `Budget` 藏进一个变量再传（`call(o, op, p, b)`）——
///    那一格要类型检查，今天**判不了**（缺的证据：通信层的第一版类型定义）。
#[test]
fn x6_every_frontend_call_site_passes_an_explicit_budget() {
    let pop = boundary();
    let member_paths: BTreeSet<&str> = pop.iter().map(|m| m.rel.as_str()).collect();
    let front: Vec<(String, String)> = corpus()
        .into_iter()
        .filter(|(rel, _)| rel.ends_with(".ts") && !member_paths.contains(rel.as_str()))
        .collect();
    // 抽取器自检：前端语料真的在（人群为空不等于语料为空）。
    assert!(
        front.iter().any(|(rel, _)| rel == "src/tabs.ts"),
        "前端语料里找不到 `src/tabs.ts` —— 语料面坏了，本条此刻在空转"
    );
    let mut offenders: Vec<String> = Vec::new();
    let mut sites = 0usize;
    for (entry, _) in ENTRIES {
        for (rel, text) in &front {
            let prod = production_of(rel, text);
            let head = format!("{entry}(");
            sites += prod.matches(head.as_str()).count();
            for s in call_sites_without_budget(&prod, entry) {
                offenders.push(format!("  {rel} —— `{s}`"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "前端这几处调用通信层入口时没有显式给 `Budget`（本趟共扫到 {sites} 个调用点）：\n{}\n\n\
         `设计/05 §3.3.2`：**说法归调用方**。库里的默认期限 =\n\
         「这条路该等多久」没有任何调用方想过（`§9`：那 8 条无期限路径连实测分布都没有）。",
        offenders.join("\n")
    );
    assert_eq!(
        call_sites_without_budget("await call(origin, op, payload);\n", "call").len(),
        1,
        "识别器认不出缺 `Budget` 的调用点 —— `X6` 此刻在空转"
    );
    assert!(
        call_sites_without_budget("await call(origin, op, payload, budget);\n", "call").is_empty(),
        "显式给了 `budget` 的调用点被判成违例 —— 假红比不查更坏"
    );
    assert!(
        call_sites_without_budget("await recall(origin);\n", "call").is_empty(),
        "`recall(` 被当成了 `call(` —— 匹配单位比事实小（`needle_anchor_registry` 那一族）"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  八、元判据：这十五条真的在跑
// ════════════════════════════════════════════════════════════════════════════

/// 一份 Rust 源码里所有 `#[test] fn <名字>` 的名字。
fn test_fn_names(src: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = src.lines().collect();
    let attr = format!("#[{}]", "test");
    let mut out = BTreeSet::new();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() != attr {
            continue;
        }
        for next in lines.iter().skip(i + 1) {
            let t = next.trim();
            if t.is_empty() || t.starts_with("//") || t.starts_with('#') {
                continue;
            }
            if let Some(rest) = guard_core::strip_visibility(t).strip_prefix("fn ") {
                if let Some(name) = rest.split('(').next() {
                    out.insert(name.trim().to_string());
                }
            }
            break;
        }
    }
    out
}

/// ★★ **「判据不在执行链上就等于不存在」的机检形态**。
///
/// [`CRITERIA`] 那张表与本文件里**真实的** `#[test]` 两向集合相等，
/// 并且编号那一列恰好覆盖 `C1`–`C5` 与 `X1`–`X6`。
///
/// # 它治什么
///
/// 删掉一条判据而把表留着（读的人以为那条性质有人守着）· 加一条判据而不登记
/// （`设计/05` 的编号与盘上的东西对不上）· 把 `X4` 悄悄改名（闭集按名字认）。
///
/// # ⚠ 它**接不住**的那一格（如实登记，这是本件已知的洞）
///
/// **整个模块被从 `lib.rs` 摘掉**。那时本文件一条都不跑，本条也不跑 ——
/// 「摘掉了」与「全绿」在终端上一模一样。挡这一形要在 `tests/scripts/gate.sh` 上
/// 给这十五条开一个**自己的格**（形状照 `f3-copy`：pin · declared · ran 三方相等），
/// 而那份文件**不在本件写区** ⇒ 已在交回里点名报备，不在这里偷偷改。
#[test]
fn every_criterion_is_on_the_execution_chain() {
    let me = include_str!("comm_boundary_registry_tests.rs");
    let actual = test_fn_names(me);
    // 抽取器自检：真的抠到了测试名（否则两个空集相等，恒绿）。
    assert!(
        actual.len() >= 10,
        "只抠出 {} 个 `#[test]` —— 抽取器坏了，下面那条会拿两个空集比出绿：{actual:?}",
        actual.len()
    );
    let declared: BTreeSet<String> = CRITERIA.iter().map(|(_, f, _)| (*f).to_string()).collect();
    let undeclared: Vec<&String> = actual.difference(&declared).collect();
    let phantom: Vec<&String> = declared.difference(&actual).collect();
    assert!(
        undeclared.is_empty(),
        "本文件里有 `#[test]` 没登记进 `CRITERIA`：{undeclared:?}\n\
         ⇒ 它守的是哪一条判据？写进表里，否则下一个人读表会以为它不存在。"
    );
    assert!(
        phantom.is_empty(),
        "`CRITERIA` 里登记着这几条，而本文件里**没有**对应的 `#[test]`：{phantom:?}\n\
         ⇒ 判据被删了 / 改名了 / 被 `#[ignore]` 挡住了。\n\
         🔴 表还在而判据没了 —— 读表的人会以为那条性质有人守着。那正是本仓治的那一族。"
    );
    assert_eq!(
        actual.len(),
        CRITERIA.len(),
        "两向都查过还对不上 —— 表里有重名（{} 条登记 vs {} 个去重后的名字）",
        CRITERIA.len(),
        declared.len()
    );
    let ids: BTreeSet<&str> = CRITERIA.iter().map(|(id, _, _)| *id).collect();
    let want_c: BTreeSet<&str> = ["C1", "C2", "C3", "C4", "C5"].into_iter().collect();
    let want_x: BTreeSet<&str> = ["X1", "X2", "X3", "X4", "X5", "X6"].into_iter().collect();
    let got_c: BTreeSet<&str> = ids.iter().filter(|i| i.starts_with('C')).copied().collect();
    let got_x: BTreeSet<&str> = ids.iter().filter(|i| i.starts_with('X')).copied().collect();
    assert_eq!(
        got_c, want_c,
        "`设计/05 §2` 的铁律恰好五条（C1–C5），盘上是 {got_c:?}"
    );
    assert_eq!(
        got_x, want_x,
        "`设计/05 §3.3.6` 的签名判据恰好六条（X1–X6），盘上是 {got_x:?}"
    );
}
