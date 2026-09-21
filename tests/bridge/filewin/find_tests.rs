//! [`super`] 的判据 —— **搜索真的接到窗口上了吗**。
//!
//! 设计住 `调研/设计/60 §3.5` / `§3.5.2a` / `§3.5.3` 与 `调研/设计/96 §2.9`。
//!
//! # 🔴 这一摞里哪几条是**反空真**的锚
//!
//! 1. [`a_find_against_a_backend_that_never_built_an_index_comes_back_empty_and_says_so`]
//!    ＋ [`typing_into_the_box_puts_exactly_the_expected_hits_on_the_frame`] 是**一对**：
//!    同一棵树、同一个子串，**没建过索引**那一侧拿到空集 ＋ 一句「还没建过」，
//!    **建过**那一侧拿到**恰好等于**预期集的命中。
//!    🔴 少了前一条，后一条可以靠「永远回空集」全绿；
//!    少了后一条，前一条可以靠「永远回空集」全绿。**两条一起才是判据。**
//! 2. [`a_fresh_index_is_never_rebuilt_behind_the_users_back`] 是
//!    [`the_window_sends_a_rebuild_when_the_backend_says_the_index_is_missing`] 的阴性对照：
//!    没有它，「会发那条重走命令」可以靠**每次都发**全绿，而那是一条真缺陷
//!    （每敲一个字走一整棵树）。
//! 3. [`the_freshness_numbers_the_backend_reports_really_reach_the_frame`] 喂**两组
//!    不同的数**并断言画出来的跟着变 —— 一个写死的 `300` 只在一组上碰巧对。
//!
//! # ⚠ 这一摞买不到什么
//!
//! 逐条住 [`super::testing`] 的头注（**它不是后端** · 帧的反序列化不在射程里 ·
//! 时延一个读数都没有 · 真机上看得见买不到）。别把「这一摞全绿」读成「搜索在真机上好了」。

use super::testing::{self, Declared, FakeBackend};
use super::*;

/// 这几条用例共用的子串。**两个字符**：一个字符会命中太多（近乎全集 ⇒ 相等断言
/// 退化成「全都中」），四个字符在 38 个字母的语料上常常一条都不中（退化成空集）。
/// 人群自检（[`the_synthetic_tree_is_not_a_trivial_shape`]）钉住它两头都不塌。
const NEEDLE: &str = "7q";

/// 语料规模。**够让预期集里有好几条、又不至于让一条用例花几秒造文件。**
const TREE_N: usize = 220;
const TREE_SEED: u64 = 0x24F4;

fn ctx_ready() -> egui::Context {
    let ctx = egui::Context::default();
    // 先跑一帧空的 —— 之后碰字体/命中测试才不会踩 `fonts_mut` 那一形（`fonts.rs §四`）。
    let out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.drop_without_applying_deltas();
    ctx
}

// ═══════════════════════════════════════════════════════════════════
// 人群自检：语料先得是个**不平凡**的形状
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **量具自检。** 这棵树与这个子串要同时满足三件事，否则下面每一条相等断言
/// 都可能是**退化**的（全中 / 全不中都会让「相等」变得毫无信息）。
#[test]
fn the_synthetic_tree_is_not_a_trivial_shape() {
    let t = testing::plant("shape", TREE_N, TREE_SEED).expect("造不出那棵树");
    let all = t.entries.len();
    let hit = t.expected(NEEDLE).len();
    assert!(
        all >= 200,
        "这棵树只有 {all} 条条目 —— 语料塌了（`corpus::synth_paths` 的行为变了？）"
    );
    assert!(
        hit >= 2,
        "子串 `{NEEDLE}` 在这棵树上只命中 {hit} 条 —— 相等断言会退化成「几乎空集」。\n\
         换子串或换种子，**别放宽断言**"
    );
    assert!(
        hit * 4 < all,
        "子串 `{NEEDLE}` 命中了 {hit}/{all} 条 —— 太多了，相等断言会退化成「几乎全集」"
    );
    // 夹紧过的段长与深度：仍然要有多级目录，否则「树」这个字就名不副实。
    let depth = t
        .entries
        .iter()
        .map(|p| p.matches(std::path::MAIN_SEPARATOR).count())
        .max()
        .unwrap_or(0);
    let root_depth = t
        .root
        .to_string_lossy()
        .matches(std::path::MAIN_SEPARATOR)
        .count();
    assert!(
        depth >= root_depth + 3,
        "最深那条只比根深 {} 级 —— 夹得太狠了，这不再是一棵树",
        depth - root_depth
    );

    // 🔴 **Windows 那两格：这棵树在 `windows-latest` 上也会真跑。**
    //
    // `.github/workflows/ci.yml` 里跑 `cargo test --workspace` 的那个 job 逐字
    // `runs-on: windows-latest` ⇒ 下面两条坑必须在**这台 Linux 机器上**就出声，
    // 而不是等云端红（而且种子是**定死的** ⇒ 要么永远不碰、要么永远碰）。
    //
    // ① **保留设备名**：`con` / `aux` / `nul` / `prn` / `com1..9` / `lpt1..9`
    //    在 Windows 上压根**建不出文件**。语料的字母表是 `[a-z0-9-_]`
    //    ⇒ 一个三字符段碰巧等于它们是可能的。
    // ② **`MAX_PATH` = 260**：根住临时目录（云端那台约 40 字符）＋
    //    `ccm-filewin-find-<tag>-<pid>-<seed>` 那一段（约 35）⇒ 相对那一段的预算约 180。
    //    ⚠ 钉的是**相对**长度，不是绝对 —— 绝对长度逐平台不同，
    //    钉它等于把本机的临时目录路径烤进判据。
    //    🔴 **这个上限是量出来的，而且它真的是一道门**（第二轮死值验的刀 20 逼出来的）：
    //    · `SEG_CAP = 24`（今天的值）现打 **86** 字符；
    //    · 把 `SEG_CAP` 放宽到 **200** 现打 **207** 字符 ⇒ 超过下面这个数、当场红。
    //    ⇒ 钉 **100**（86 ＋ 一点余量）。上一版钉的是 170，而 170 与 86 之间那 84 的空档
    //    正好把刀 20 吞掉了 —— **一个宽出一倍的上限，长得和一道门一模一样**。
    const WIN_RESERVED: &[&str] = &[
        "con", "aux", "nul", "prn", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    for p in &t.entries {
        for seg in p.split(std::path::MAIN_SEPARATOR) {
            let low = seg.to_ascii_lowercase();
            assert!(
                !WIN_RESERVED.contains(&low.as_str()),
                "合成出来的路径段 `{seg}` 是 Windows 的保留设备名（整条：{p}）——\n\
                 那条路径在 `windows-latest` 上**建不出来**，而种子是定死的 ⇒ 云端会一直红。\n\
                 ⇒ 换种子 / 换 `TREE_N`，**别在这里加豁免**。"
            );
        }
    }
    let worst = t
        .entries
        .iter()
        .map(|p| p.chars().count())
        .max()
        .unwrap_or(0)
        - t.root.to_string_lossy().chars().count();
    assert!(
        worst <= 100,
        "最长那条相对路径 {worst} 个字符 —— 超了 Windows `MAX_PATH` 的预算（现打 86，钉 100；见上面 ②）。\n\
         ⇒ 把 `testing::SEG_CAP` / `testing::DEPTH_CAP` 夹小一点，别改这个上限。"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 🔴 帧面端到端：打字 → 命中恰好等于预期集
// ═══════════════════════════════════════════════════════════════════

/// 🔴🔴 **本摞的主判据**：在那一个原生窗口里**打字**，帧面上出**恰好**那几条命中。
///
/// 相等断言，双向差集：预期集由**路径算术**算出来（`testing::SynthTree::expected`），
/// 实得集从**egui 这一帧交出去的 galley**里读回来。两侧不同源 ——
/// 一侧是路径字符串的过滤，另一侧走了「造文件 → 后端走盘 → 线上 JSON →
/// `decode_find` → `Hit::display` → `show_hit_rows` → galley」整条链。
#[tokio::test]
async fn typing_into_the_box_puts_exactly_the_expected_hits_on_the_frame() {
    let tree = testing::plant("e2e", TREE_N, TREE_SEED).expect("造不出那棵树");
    let root = tree.root.to_string_lossy().to_string();
    let wired = testing::wire_up(
        "b1-find-e2e",
        FakeBackend::new(COMMANDS, Declared::default()),
    );
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired.origin, &root);

    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, NEEDLE);
    assert_eq!(
        w.query(),
        NEEDLE,
        "合成事件没落进搜索框 —— 那一帧上这个框没拿到焦点（量具坏了，下面的读数不许用）"
    );
    testing::settle(&w.search, before, "端到端那一趟").await;

    let painted = testing::frame_text(&ctx, &mut w, Vec::new());
    let got: std::collections::BTreeSet<String> = painted
        .iter()
        .filter(|t| t.starts_with(&root))
        .cloned()
        .collect();
    let want = tree.expected(NEEDLE);
    assert!(
        !want.is_empty(),
        "预期集是空的 —— 人群自检那条应该先红（它没红说明它坏了）"
    );
    assert_eq!(
        got,
        want,
        "帧面上的命中集与预期集对不上。\n  \
         帧上多出来的：{:?}\n  帧上漏掉的：{:?}\n\
         ⚠ 两侧都不是地板：预期是路径算术算的，实得是从这一帧的 galley 里读回来的。",
        got.difference(&want).take(5).collect::<Vec<_>>(),
        want.difference(&got).take(5).collect::<Vec<_>>()
    );
    // 线上真的跑过那几条命令 —— 不是本地算出来的。
    let cmds = wired.cmds();
    assert!(
        cmds.iter().any(|c| c == CMD_FIND),
        "线上一条 `{CMD_FIND}` 都没有 —— 那这些命中不知道从哪来的：{cmds:?}"
    );
}

/// 🔴 **反空真的锚（前半）**：后端**没建过**索引时，帧面上是空的，而且**说出来**。
///
/// 它与上面那条是**一对**：同一棵树、同一个子串、同一条链，唯一的差别是
/// 那台后端认不认 `files-index-rebuild`。
/// ⇒ 「命中集恰好等于预期集」不可能靠「永远回空集」蒙过去。
#[tokio::test]
async fn a_find_against_a_backend_that_never_built_an_index_comes_back_empty_and_says_so() {
    let tree = testing::plant("nonull", TREE_N, TREE_SEED).expect("造不出那棵树");
    let root = tree.root.to_string_lossy().to_string();
    // 🔴 它**不声明** `files-index-rebuild` ⇒ 客户端连发都发不出去
    //    （`InboundClient::call` 第一行的能力协商），索引永远建不起来。
    //    这正是本刀之前真机上的那一态（「零生产调用方 ⇒ 恒回 index_missing」）。
    let wired = testing::wire_up(
        "b1-find-noindex",
        FakeBackend::new(
            &[CMD_FIND, CMD_INDEX_STATUS, CMD_BROWSE],
            Declared::default(),
        ),
    );
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired.origin, &root);

    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, NEEDLE);
    testing::settle(&w.search, before, "没索引那一趟").await;

    let painted = testing::frame_text(&ctx, &mut w, Vec::new());
    let got: Vec<&String> = painted.iter().filter(|t| t.starts_with(&root)).collect();
    assert!(
        got.is_empty(),
        "没建过索引却在帧面上画出了命中：{got:?} —— 那些是哪来的？"
    );
    // 🔴 **〔死值验第 2 刀的订正，2026-09-21〕这一段原来是一条
    //    `painted.any(|t| t.contains("索引还没建过"))`，而那把「没建过」这件事
    //    在界面上说出来的**两个地方**合成了一个断言 —— 把
    //    `freshness_line` 那句话整句换掉，本条**照样绿**（`hits_line` 里也有那几个字）。
    //    ⇒ 刀没红逮到的是真东西：断言的单位（「帧上某处有这几个字」）
    //    比事实的单位（「那两行**各自**说了这件事」）**粗一级**。
    //    ⇒ 拆成两条**相等**断言 ＋ 两条措辞断言，四条各挡一把刀。
    let st = w.search.shown().status.expect("这一趟该拿到状态");
    assert!(
        st.index_missing,
        "这台合成后端该报「没建过」—— 夹具坏了，下面的读数不许用"
    );
    let o = w.search.shown().outcome.expect("这一趟该有一份答案");
    assert!(o.index_missing, "`{CMD_FIND}` 该回 `index_missing: true`");
    let fresh = freshness_line(&st);
    let hl = hits_line(&o);
    assert!(
        painted.contains(&fresh),
        "**新鲜度那一行**没画到帧上（该是 {fresh:?}）。\n这一帧画的是：{painted:?}"
    );
    assert!(
        painted.contains(&hl),
        "**命中那一行**没画到帧上（该是 {hl:?}）。\n这一帧画的是：{painted:?}"
    );
    assert!(
        fresh.contains("索引还没建过"),
        "新鲜度那一行没把「索引还没建过」说出来：{fresh:?}\n\
         ⚠ 它与「这台机器上没有这个文件」是两件事，混了用户会删错东西"
    );
    assert!(
        hl.contains("索引还没建过"),
        "命中那一行没把「索引还没建过」说出来：{hl:?}\n\
         ⚠ 同上 —— 两行**各自**都要说得出，不许只靠另一行兜着"
    );
    // 而预期集是**非空**的 ⇒ 上面那个空集是「索引没建」造成的，不是「树里没有」。
    assert!(
        !tree.expected(NEEDLE).is_empty(),
        "这棵树里压根没有含 `{NEEDLE}` 的路径 —— 那这条对照说明不了任何事"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 🔴 `设计/60 §3.5.2a` 那条节奏缺口：调用方**真的会发**那条重走命令
// ═══════════════════════════════════════════════════════════════════

/// 🔴🔴 **`§3.5.2a` 欠的那条判据就是这一条。**
///
/// 它逐字登记的缺口是：「调用方不发那条重走命令，索引就永远不会自己变新 ——
/// 而『调用方到底发不发』后端那棵树的判据钉不住（它在另一棵树上）。
/// ⇒ 欠一条判据，住址在 `src/bridge` 那一侧。」
///
/// ⇒ 这里数的是**线上真的出现过几条** `files-index-rebuild`：**恰好 1 条**。
/// 而且顺序也钉：先问状态（拿 `index_missing`）、再重走、最后才查。
#[tokio::test]
async fn the_window_sends_a_rebuild_when_the_backend_says_the_index_is_missing() {
    let tree = testing::plant("cadence", 40, TREE_SEED).expect("造不出那棵树");
    let root = tree.root.to_string_lossy().to_string();
    let wired = testing::wire_up(
        "b1-find-cadence",
        FakeBackend::new(COMMANDS, Declared::default()),
    );
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired.origin, &root);

    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, NEEDLE);
    testing::settle(&w.search, before, "节奏那一趟").await;

    assert_eq!(
        wired.count(CMD_INDEX_REBUILD),
        1,
        "线上出现了 {} 条 `{CMD_INDEX_REBUILD}` —— 该是**恰好 1** 条。\n\
         这一趟线上跑的是：{:?}\n\
         ⚠ 0 条 = `§3.5.2a` 那个缺口还敞着（索引永远不会变新）；\n\
         ⚠ >1 条 = 客户端自己造雪崩（那条命令没有并发保护，一条就是一整棵树）。",
        wired.count(CMD_INDEX_REBUILD),
        wired.cmds()
    );
    assert_eq!(
        w.search.rebuilds_sent(),
        1,
        "那块板子自己记的重走数与线上数不一致 —— 两个数有一个在撒谎"
    );
    // 顺序：状态 → 重走 → 查。**先问再走**，否则「要不要走」这件事没有依据。
    let cmds = wired.cmds();
    let i_status = cmds.iter().position(|c| c == CMD_INDEX_STATUS);
    let i_rebuild = cmds.iter().position(|c| c == CMD_INDEX_REBUILD);
    let i_find = cmds.iter().position(|c| c == CMD_FIND);
    assert!(
        matches!((i_status, i_rebuild, i_find), (Some(a), Some(b), Some(c)) if a < b && b < c),
        "顺序不对（状态 {i_status:?} / 重走 {i_rebuild:?} / 查 {i_find:?}）——\n\
         先查后走的话，用户看到的永远是**上一趟**那份索引的答案：{cmds:?}"
    );
    // 而且这一趟真的建起来了 —— `files-find` 不再回 `index_missing`。
    let o = w.search.shown().outcome.expect("这一趟该有答案");
    assert!(
        !o.index_missing,
        "重走命令发出去了，`{CMD_FIND}` 却还是回 `index_missing` —— 那条命令白发了"
    );
    assert!(
        o.scanned > 0,
        "`scanned` 是 0 —— 「没命中」与「索引是空的」在界面上一模一样（`§3.5.3` 那条）"
    );
}

/// 🔴 **阴性对照**：后端说「建过了、不 stale」⇒ 那条重走命令**一条都不发**。
///
/// 没有这一条，上面那条可以靠「**每次都发**」全绿 —— 而那是一条真缺陷：
/// 每敲一个字走一整棵树（64 万条热缓存 0.99 秒，冷缓存没量过）。
#[tokio::test]
async fn a_fresh_index_is_never_rebuilt_behind_the_users_back() {
    let tree = testing::plant("fresh", 40, TREE_SEED).expect("造不出那棵树");
    let root = tree.root.to_string_lossy().to_string();
    let declared = Declared {
        age_secs: 7,
        rewalk_interval_secs: 4242,
        stale: Some(false),
        ..Declared::default()
    };
    let wired = testing::wire_up(
        "b1-find-fresh",
        FakeBackend::new(COMMANDS, declared).preindexed(&tree.root),
    );
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired.origin, &root);

    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, NEEDLE);
    testing::settle(&w.search, before, "新鲜那一趟").await;

    assert_eq!(
        wired.count(CMD_INDEX_REBUILD),
        0,
        "索引是新的，却还是发了 {} 条 `{CMD_INDEX_REBUILD}`：{:?}\n\
         ⚠ 那不是「多花一点」—— 一条就是一整棵树的遍历，而且它是每敲一个字发一次。",
        wired.count(CMD_INDEX_REBUILD),
        wired.cmds()
    );
    // 🔴 **但保鲜的另一半照样发了。** `files-browse` 刻意**不**放在重走那一支里面
    //    （理由住 `find.rs::one_round` 的 ②）：重走难得一次，而「眼前是哪个目录」
    //    用户每换一次就变一次 —— 放进去等于这条命令只在重走那一刻有效。
    //    ⇒ 这一条就是那个取舍的判据：**一趟不重走的查询里它仍然恰好发了 1 条**。
    assert_eq!(
        wired.count(CMD_BROWSE),
        1,
        "这一趟没重走，`{CMD_BROWSE}` 也跟着一条都没发（线上：{:?}）——\n\
         那等于把「告诉后端用户在看哪个目录」这件事绑在了重走那一刻上。",
        wired.cmds()
    );

    // 但它**照样查到了** —— 「不重走」不等于「不干活」。
    let got: std::collections::BTreeSet<String> = w
        .search
        .shown()
        .outcome
        .expect("这一趟该有答案")
        .hits
        .iter()
        .map(|h| h.display())
        .collect();
    assert_eq!(
        got,
        tree.expected(NEEDLE),
        "不重走那一支查出来的命中集不对 —— 那说明「跳过重走」把查询也跳过了"
    );
}

/// 🔴 后端说 `stale` ⇒ 也要发那条重走命令（`stale` 是**后端算的**，不是这一侧）。
///
/// 这一条与上面那条只差 `stale` 一个布尔 ⇒ 它证明那个布尔**真的在驱动**这条命令。
#[tokio::test]
async fn a_stale_index_the_backend_flagged_gets_rebuilt() {
    let tree = testing::plant("stale", 40, TREE_SEED).expect("造不出那棵树");
    let root = tree.root.to_string_lossy().to_string();
    let declared = Declared {
        age_secs: 9_999,
        rewalk_interval_secs: 4242,
        stale: Some(true),
        ..Declared::default()
    };
    let wired = testing::wire_up(
        "b1-find-stale",
        FakeBackend::new(COMMANDS, declared).preindexed(&tree.root),
    );
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired.origin, &root);

    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, NEEDLE);
    testing::settle(&w.search, before, "stale 那一趟").await;

    assert_eq!(
        wired.count(CMD_INDEX_REBUILD),
        1,
        "后端说 `stale: true`，客户端却没发重走（线上：{:?}）——\n\
         那等于把后端声明的那个周期当空气：`stale` 逐字是 `age_secs > rewalk_interval_secs`。",
        wired.cmds()
    );
}

/// 🔴 连打几趟查询，重走**只发一趟**。
///
/// `src/doc/IPC-PROTOCOL.md §10` 逐字：`files-index-rebuild`「**没有并发保护**：
/// 两个调用方同时发，后完成的那一趟胜出」。而这一侧**没有去抖** ⇒
/// 不拦一下就是「连打五个字 ⇒ 五趟全树遍历」。
#[tokio::test]
async fn many_keystrokes_in_flight_still_only_trigger_one_rebuild() {
    let tree = testing::plant("burst", 40, TREE_SEED).expect("造不出那棵树");
    let root = tree.root.to_string_lossy().to_string();
    let wired = testing::wire_up(
        "b1-find-burst",
        FakeBackend::new(COMMANDS, Declared::default()),
    );
    let w = testing::window_on(&wired.origin, &root);
    // 直接连发五趟（不经 UI —— 这一条问的是编排，不是焦点）。
    for _ in 0..5 {
        let mine = w.search.start();
        let b = w.search.clone();
        let (o, r) = (crate::origin::Origin(wired.origin.clone()), root.clone());
        tokio::spawn(async move {
            run_search(b, o, r, NEEDLE.to_string(), mine, false).await;
        });
    }
    testing::settle(&w.search, 0, "连打那一趟").await;
    // 等到五趟全落地（`rounds` 会数到 5：`store_if_current` 只对号对得上的那一趟写，
    // 但 `rounds` 那个数是**落地过几份**，号不对的那几趟不加）。
    for _ in 0..600 {
        if w.search.rebuilds_sent() > 0 && !w.search.is_running() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let n = wired.count(CMD_INDEX_REBUILD);
    assert!(
        (1..=1).contains(&n),
        "五趟并发查询触发了 {n} 条 `{CMD_INDEX_REBUILD}` —— 该是 1 条。\n\
         线上：{:?}\n\
         ⚠ 0 条 = 那条抢占写反了（索引永远建不起来）；>1 条 = 客户端自己造雪崩。",
        wired.cmds()
    );
}

// ═══════════════════════════════════════════════════════════════════
// 🔴 新鲜度那几个数真的走到了界面上
// ═══════════════════════════════════════════════════════════════════

/// 🔴🔴 **`设计/60 §3.5.3` 那条 ⬜ 的兑现判据。**
///
/// 它要的逐字是「那个数**显示在界面上**」。而 `真相源/99 §9.1` 记着同族的教训：
/// **源码扫描买不到「显示出来了」**（「调了那个看起来对的 API 只证明盘上有」）。
/// ⇒ 这一条真跑一帧生产那个 `frame_body`，从 galley 里把数字读回来。
///
/// 🔴 **而且它喂两组不同的数。** 只喂一组的话，界面上写死一个 `300`
/// （后端今天声明的值）也会绿 —— 那正是「显示的是你编的数」那一形。
#[tokio::test]
async fn the_freshness_numbers_the_backend_reports_really_reach_the_frame() {
    // 🔴 **每一条断言都按「措辞 ＋ 那个数」一起找，不按裸数字找。**
    //    现打逼出来的（本条自己第一版就栽在这）：拿裸数字做包含判断时，
    //    同一行里的 `entries` / `resident_bytes` 随便哪个数字都可能把它**碰巧**命中
    //    —— 第一版的跨组缺席断言就是这么假红的（`13247 字节` 里有 `13`）。
    //    ⇒ 裸数字不是一把尺子；带着它所在那一格的措辞才是。
    let mut lines: Vec<(Vec<String>, String)> = Vec::new();
    for (tag, interval, age, unreadable, entries_hint) in [
        ("b1-fresh-a", 4242u64, 1234u64, 41u64, 'a'),
        ("b1-fresh-b", 8765u64, 5678u64, 13u64, 'b'),
    ] {
        let tree =
            testing::plant(&format!("fresh-{entries_hint}"), 30, TREE_SEED).expect("造不出那棵树");
        let root = tree.root.to_string_lossy().to_string();
        let declared = Declared {
            rewalk_interval_secs: interval,
            age_secs: age,
            stale: Some(false),
            unreadable_dirs: unreadable,
            browse_watch_cap: 64,
            ..Declared::default()
        };
        let wired = testing::wire_up(
            tag,
            FakeBackend::new(COMMANDS, declared).preindexed(&tree.root),
        );
        let ctx = ctx_ready();
        let mut w = testing::window_on(&wired.origin, &root);
        let before = w.search.rounds();
        // 按那颗「重建索引」以外的路：直接发一趟（子串留空 ⇒ 只问状态）。
        w.fire_search(None, true);
        testing::settle(&w.search, before, tag).await;
        let painted = testing::frame_text(&ctx, &mut w, Vec::new());
        let line = painted
            .iter()
            .find(|t| t.contains("后端声明的重走周期"))
            .unwrap_or_else(|| {
                panic!("这一帧上没有新鲜度那一行 —— `§3.5.3` 那条 ⬜ 还是 ⬜。\n这一帧画的是：{painted:?}")
            })
            .clone();
        // 🔴 相等断言：那一行**恰好**等于纯函数拿后端那份状态渲染出来的样子。
        let st = w.search.shown().status.expect("这一趟该拿到状态");
        assert_eq!(
            line,
            freshness_line(&st),
            "帧面上那一行与 `freshness_line` 对不上 —— 界面上那句话另有一份实现"
        );
        // 🔴 而后端报的那几个数**逐个**在那一行上，而且是**带着措辞**找的
        //    （光找一个数字容易撞上同一行里别的数）。
        let frags = vec![
            format!("后端声明的重走周期 {interval} 秒"),
            format!("{age} 秒前走完"),
            format!("有 {unreadable} 个目录读不进去"),
        ];
        for frag in &frags {
            assert!(
                line.contains(frag.as_str()),
                "新鲜度那一行里找不到 {frag:?}：{line:?}\n\
                 ⚠ 后端报什么就画什么 —— 这一侧不许换算、不许补默认值。"
            );
        }
        assert_eq!(
            st.rewalk_interval_secs, interval,
            "解析出来的周期不是后端报的那个数"
        );
        lines.push((frags, line));
    }
    // 🔴 **跨组缺席断言 —— 这一条才是「不是你编的」那句话的真锚。**
    //    两组各自的 `contains` 只证明「那个数在那一行上」；一个**写死**的
    //    `4242`（或写死任何一个值）会在它自己那一组上碰巧全绿。
    //    ⇒ 再断言：第二组那一行里**一段**第一组的读数都没有，反之亦然。
    assert_eq!(lines.len(), 2, "两组都得跑到");
    let (a, b) = (&lines[0], &lines[1]);
    for frag in &a.0 {
        assert!(
            !b.1.contains(frag.as_str()),
            "第二组那一行里出现了第一组的读数 {frag:?}：{:?}\n\
             ⇒ 那一格画的不是后端报的数，是这一侧写死的。",
            b.1
        );
    }
    for frag in &b.0 {
        assert!(
            !a.1.contains(frag.as_str()),
            "第一组那一行里出现了第二组的读数 {frag:?}：{:?}",
            a.1
        );
    }
}

/// 🔴 **零命中型**：这一侧**没有**任何一个重走周期的字面量。
///
/// `设计/99 §2 Q4` 逐字：周期该定多少**还没拍板** ⇒ 这一侧不许有第二个家。
/// 后端今天声明的是 `300`，所以 `300` 出现在生产段里就是一个假的第二住址。
#[test]
fn no_rewalk_period_literal_lives_on_this_side() {
    let src =
        std::fs::read_to_string(crate::guard_support::crate_src_root().join("filewin/find.rs"))
            .expect("find.rs 读不动");
    let prod = guard_core::production_code(&src);
    // 运行时拼，免得命中本行自己。
    let needle = format!("{}{}", 30, 0);
    for line in prod.lines() {
        // 只看代码，不看注释（注释里当然会提到那个数）。
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        assert!(
            !code.contains(needle.as_str()),
            "`find.rs` 的生产段里出现了 `{needle}`：{line:?}\n\
             ⚠ 那是后端今天声明的重走周期 —— 它只许有一个家，而那个家在后端。\n\
             这一侧要用它，走 `IndexStatus::rewalk_interval_secs`（后端报过来的那一份）。"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// 契约面：名字与字段名对着那份冻结的线上契约
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **线上名里一个 `.` 都不许有。**
///
/// `设计/96 §2.9` 逐字：本仓三个取词器的字符集都是「字母数字 / `_` / `-`」，
/// 带 `.` 的字面量会被它们**静默丢弃** —— 那等于把一条命令从判据底下抽走，
/// 而判据照常报绿。⇒ 这一侧只许持有连字符那一套。
#[test]
fn the_wire_names_never_carry_a_dot() {
    assert_eq!(
        COMMANDS.len(),
        4,
        "这一侧用到的线上命令数变了，连理由一起改"
    );
    for c in COMMANDS {
        assert!(
            !c.contains('.'),
            "`{c}` 里有一个点 —— 那是**能力名**的写法。\n\
             线上与 CLI 一律连字符（`设计/96 §2.9`），理由是取词器的字符集。"
        );
        assert!(
            c.starts_with("files-"),
            "`{c}` 不像 `files-read` 这一族的线上名"
        );
    }
}

/// 🔴 出方向那几个字段名与**那份冻结的线上契约**逐个对得上。
///
/// 两侧不同源：这一侧是 `find.rs` 里手写的 [`FIND_FIELDS`] / [`STATUS_FIELDS`]，
/// 那一侧现读 `src/doc/IPC-PROTOCOL.md §10` 那两张表（读者在仓外的那份契约）。
/// ⇒ 后端改了字段名而文档跟着改，本条会红，逼这一侧一起改。
///
/// ⚠ **它买不到「后端真的发这几个字段」** —— 后端那棵树不在本 crate 的依赖图里
/// （`src/backend` 是独立 crate）。它买到的是「这一侧与那份文档不漂」。
#[test]
fn the_reply_shapes_match_the_frozen_wire_contract() {
    let doc = include_str!("../../../src/doc/IPC-PROTOCOL.md");
    for (cmd, declared) in [(CMD_FIND, FIND_FIELDS), (CMD_INDEX_STATUS, STATUS_FIELDS)] {
        let heading = format!("#### `{cmd}`");
        let start = doc
            .find(&heading)
            .unwrap_or_else(|| panic!("`src/doc/IPC-PROTOCOL.md` 里没有 `{heading}` 那一小节"));
        let rest = &doc[start + heading.len()..];
        let end = rest.find("\n#### ").unwrap_or(rest.len());
        let section = &rest[..end];
        // 那张表里「向」那一栏是 `←` 的行 = 出方向字段。
        let mut documented: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for line in section.lines() {
            let cols: Vec<&str> = line.split('|').map(str::trim).collect();
            if cols.len() < 4 {
                continue;
            }
            if !cols[2].contains('←') {
                continue;
            }
            let name = cols[1].trim_matches('`');
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                documented.insert(name.to_string());
            }
        }
        assert!(
            documented.len() >= 5,
            "从 `{cmd}` 那一小节里只抠出 {} 个出方向字段 —— 抽取器坏了（表的形状变了？）：{documented:?}",
            documented.len()
        );
        let mine: std::collections::BTreeSet<String> =
            declared.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            mine,
            documented,
            "`{cmd}` 的出方向字段两侧对不上。\n  \
             这一侧多出来的：{:?}\n  文档里有而这一侧没解析的：{:?}\n\
             ⚠ 少解析一个字段的症状是「界面上那一格永远是默认值」，**而且不报错**。",
            mine.difference(&documented).collect::<Vec<_>>(),
            documented.difference(&mine).collect::<Vec<_>>()
        );
    }
}

/// 🔴 那份契约文档里**真的有**这四条命令的小节（名字不许漂）。
#[test]
fn every_wire_name_this_side_sends_has_a_section_in_the_contract() {
    let doc = include_str!("../../../src/doc/IPC-PROTOCOL.md");
    for c in COMMANDS {
        assert!(
            doc.contains(&format!("#### `{c}`")),
            "`src/doc/IPC-PROTOCOL.md` 里没有 `{c}` 那一小节 —— \n\
             要么名字漂了，要么这一侧在发一条契约里不存在的命令（后端会回 `unknown_command`）。"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// 纯函数那几格
// ═══════════════════════════════════════════════════════════════════

/// 少一个字段就是**解析失败**，不是悄悄当 0。
///
/// 🔴 这一条挡的是本仓治过很多次的「静默缩水」：契约漂了之后界面上那一格
/// 显示 `0 秒前走完`，而用户会把它读成「刚刚更新过」—— **恰好反了**。
#[test]
fn a_missing_field_is_a_loud_failure_not_a_silent_zero() {
    let full = serde_json::json!({
        "index_missing": false, "entries": 5, "resident_bytes": 100,
        "unreadable_dirs": 0, "truncated": false, "age_secs": 7,
        "rewalk_interval_secs": 4242, "stale": false,
        "browse_watches": 1, "browse_watch_cap": 64,
    });
    assert!(decode_status(&full).is_ok(), "完整那一份该解析得动");
    for k in STATUS_FIELDS {
        let mut m = full.clone();
        m.as_object_mut().unwrap().remove(*k);
        let r = decode_status(&m);
        assert!(
            r.is_err(),
            "少了 `{k}` 却照样解析成功 —— 那一格会在界面上显示成默认值，而且不报错"
        );
        assert!(
            r.unwrap_err().contains(k),
            "报错里没点名是哪个字段缺了（`{k}`）—— 那句话就帮不上任何人"
        );
    }
}

/// 非 UTF-8 路径走 `{"b16": …}` 那一形，**双向无损**。
///
/// `设计/60 §2 档②` 逐字：文件名被有损解码过之后，拿着那串替换字符回去找，
/// **找的是一个不存在的名字**。⇒ 这一侧把字节留着，只在画的时候才有损。
#[test]
fn a_non_utf8_hit_keeps_its_bytes_and_says_it_is_lossy() {
    let bytes = vec![0x2fu8, 0x74, 0x6d, 0x70, 0x2f, 0xff, 0xfe, 0x2e, 0x72, 0x73];
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let data = serde_json::json!({
        "hits": [{ "b16": hex }],
        "total_hits": 1, "truncated": false, "scanned": 9,
        "index_age_secs": 3, "index_missing": false,
    });
    let o = decode_find(&data).expect("该解析得动");
    assert_eq!(o.hits.len(), 1);
    assert_eq!(
        o.hits[0].path, bytes,
        "十六进制那一形没有原样解回来 —— 回程拿这串字节去寻址就指到别的文件了"
    );
    assert!(o.hits[0].lossy(), "非 UTF-8 的命中要认出自己是有损的");
    assert!(
        o.hits[0].display().contains('\u{FFFD}'),
        "画出来那一份该带替换字符（让用户看得见这个名字显示不全）"
    );
    // 而有效 UTF-8 那一形不许被当成有损。
    let plain = serde_json::json!({
        "hits": ["/tmp/a.rs"],
        "total_hits": 1, "truncated": false, "scanned": 9,
        "index_age_secs": 3, "index_missing": false,
    });
    let p = decode_find(&plain).expect("该解析得动");
    assert!(!p.hits[0].lossy(), "普通路径被误判成有损");
    assert_eq!(p.hits[0].display(), "/tmp/a.rs");
}

/// 命中那一行**一定**带着 `scanned`：「没命中」与「索引是空的」在屏幕上本来一样。
#[test]
fn the_hits_line_always_carries_the_scanned_count() {
    let empty_index = FindOutcome {
        hits: vec![],
        total_hits: 0,
        truncated: false,
        scanned: 0,
        index_age_secs: 0,
        index_missing: false,
    };
    let big_index = FindOutcome {
        scanned: 640_413,
        ..empty_index.clone()
    };
    let a = hits_line(&empty_index);
    let b = hits_line(&big_index);
    assert_ne!(
        a, b,
        "「扫了 0 条一条没中」与「扫了 64 万条一条没中」画出来是同一句话 ——\n\
         那正是 `src/doc/IPC-PROTOCOL.md §10` 说 `scanned` 是反空真用的那个理由"
    );
    assert!(b.contains("640413"), "那一行里没有 `scanned` 那个数：{b:?}");
}

/// `files-index-rebuild` 的入参形状与那份契约一致（`path` 那一个键）。
#[test]
fn the_arg_builders_use_the_documented_keys() {
    assert_eq!(find_args("a.rs"), serde_json::json!({ "needle": "a.rs" }));
    assert_eq!(
        rebuild_args("/home/u"),
        serde_json::json!({ "path": "/home/u" })
    );
    assert_eq!(
        browse_args(&["/a".to_string(), "/b".to_string()]),
        serde_json::json!({ "dirs": ["/a", "/b"] })
    );
    // 🔴 `limit` / `ignore_ascii_case` **刻意不发** —— 默认值住后端那一侧
    //    （`设计/01 §5 D3`：诚实的默认 ＝ 沿用调用者已有状态）。
    let a = find_args("x");
    assert!(
        a.get("limit").is_none() && a.get("ignore_ascii_case").is_none(),
        "这一侧给 `limit` / `ignore_ascii_case` 编了一个值：{a} —— \n\
         那两个默认值住 `src/doc/IPC-PROTOCOL.md §10`（1000 / false），\n\
         在这里写一份就是给它们造第二个家。"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 只读铁律
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **零命中型**：这一族纯读（`设计/96 §2.9` 边界①）⇒ 这一侧一个写动词都没有。
///
/// `tests/backend/readonly_guard.rs` 那 4200 多行因此一行都不用改，
/// 而**这一侧不许把它变成假的**：客户端偷偷写一笔，那条铁律看不见
/// （它的射程逐字只钉 backend crate）。
#[test]
fn this_module_never_touches_the_disk() {
    let src =
        std::fs::read_to_string(crate::guard_support::crate_src_root().join("filewin/find.rs"))
            .expect("find.rs 读不动");
    let prod = guard_core::production_code(&src);
    // 运行时拼，免得命中本行自己。
    let verbs = [
        format!("fs::{}", "write"),
        format!("fs::{}", "create_dir"),
        format!("fs::{}", "remove_file"),
        format!("fs::{}", "remove_dir"),
        format!("fs::{}", "rename"),
        format!("fs::{}", "copy"),
        format!("fs::{}", "OpenOptions"),
        format!("{}::new", "Command"),
    ];
    for v in &verbs {
        assert!(
            !prod.contains(v.as_str()),
            "`find.rs` 的生产段里出现了 `{v}` —— 这一族逐字是**纯读**。\n\
             要动盘上的东西，先去 `设计/96 §2.9` 边界① 把那条改掉（那是放宽一条红线）。"
        );
    }
}
