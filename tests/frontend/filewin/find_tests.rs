//! [`super`] 的判据 —— **搜索真的接到窗口上了吗**。
//!
//! 设计住。
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
    let root = tree.remote_root();
    let wired = testing::wire_up(
        "b1-find-e2e",
        FakeBackend::new(COMMANDS, Declared::default()).homed(&tree.root),
    )
    .await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, &root);

    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, NEEDLE);
    assert_eq!(
        w.query(),
        NEEDLE,
        "合成事件没落进搜索框 —— 那一帧上这个框没拿到焦点（量具坏了，下面的读数不许用）"
    );
    testing::settle(&w.search, before, "端到端那一趟").await;

    let painted = testing::frame_text(&ctx, &mut w, Vec::new());
    // 目录那几行后面带 `/`（命中那一摞的画法）⇒ 比之前摘掉。
    let got: std::collections::BTreeSet<String> = painted
        .iter()
        .filter(|t| t.starts_with(&root))
        .map(|t| t.trim_end_matches('/').to_string())
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
    let root = tree.remote_root();
    // 🔴 它**不声明** `files-index-rebuild` ⇒ 客户端连发都发不出去
    //    （`InboundClient::call` 第一行的能力协商），索引永远建不起来。
    //    这正是本刀之前真机上的那一态（「零生产调用方 ⇒ 恒回 index_missing」）。
    let wired = testing::wire_up(
        "b1-find-noindex",
        FakeBackend::new(
            &[CMD_FIND, CMD_INDEX_STATUS, CMD_BROWSE],
            Declared::default(),
        ),
    )
    .await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, &root);

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
// 🔴 那条节奏缺口：调用方**真的会发**那条重走命令
// ═══════════════════════════════════════════════════════════════════

/// 🔴🔴 **`§3.5.2a` 欠的那条判据就是这一条。**
///
/// 它逐字登记的缺口是：「调用方不发那条重走命令，索引就永远不会自己变新 ——
/// 而『调用方到底发不发』后端那棵树的判据钉不住（它在另一棵树上）。
/// ⇒ 欠一条判据，住址在 `src/frontend/shell` 那一侧。」
///
/// ⇒ 这里数的是**线上真的出现过几条** `files-index-rebuild`：**恰好 1 条**。
/// 而且顺序也钉：先问状态（拿 `index_missing`）、再重走、最后才查。
#[tokio::test]
async fn the_window_sends_a_rebuild_when_the_backend_says_the_index_is_missing() {
    let tree = testing::plant("cadence", 40, TREE_SEED).expect("造不出那棵树");
    let root = tree.remote_root();
    let wired = testing::wire_up(
        "b1-find-cadence",
        FakeBackend::new(COMMANDS, Declared::default()).homed(&tree.root),
    )
    .await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, &root);

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
    // 顺序：查（后端说要不要走、走哪个根）→ 重走 → 再查。少了后一趟查，用户看到的就是**重走之前**那份索引的答案。
    let cmds = wired.cmds();
    let i_first = cmds.iter().position(|c| c == CMD_FIND);
    let i_rebuild = cmds.iter().position(|c| c == CMD_INDEX_REBUILD);
    let i_last = cmds.iter().rposition(|c| c == CMD_FIND);
    assert!(
        matches!((i_first, i_rebuild, i_last), (Some(a), Some(b), Some(c)) if a < b && b < c),
        "顺序不对（查 {i_first:?} / 重走 {i_rebuild:?} / 再查 {i_last:?}）：{cmds:?}"
    );
    assert!(
        cmds.iter().any(|c| c == CMD_INDEX_STATUS),
        "这一趟没问状态：{cmds:?}"
    );
    // 重走的根是后端给的那个（这台合成后端的家目录），不是这一侧编的。
    let rebuilt = wired
        .log
        .lock()
        .unwrap()
        .iter()
        .find(|r| r["cmd"] == CMD_INDEX_REBUILD)
        .map(|r| r["args"]["path"].clone());
    assert_eq!(rebuilt, Some(serde_json::Value::String(root.clone())));
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
    let root = tree.remote_root();
    let declared = Declared {
        age_secs: 7,
        rewalk_interval_secs: 4242,
        stale: Some(false),
        ..Declared::default()
    };
    let wired = testing::wire_up(
        "b1-find-fresh",
        FakeBackend::new(COMMANDS, declared)
            .homed(&tree.root)
            .preindexed(&tree.root),
    )
    .await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, &root);

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
    let root = tree.remote_root();
    let declared = Declared {
        age_secs: 9_999,
        rewalk_interval_secs: 4242,
        stale: Some(true),
        ..Declared::default()
    };
    let wired = testing::wire_up(
        "b1-find-stale",
        FakeBackend::new(COMMANDS, declared)
            .homed(&tree.root)
            .preindexed(&tree.root),
    )
    .await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, &root);

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
    let root = tree.remote_root();
    let wired = testing::wire_up(
        "b1-find-burst",
        FakeBackend::new(COMMANDS, Declared::default()).homed(&tree.root),
    )
    .await;
    let w = testing::window_on(&wired, &root);
    // 直接连发五趟（不经 UI —— 这一条问的是编排，不是焦点）。
    for _ in 0..5 {
        let mine = w.search.start();
        let b = w.search.clone();
        let (o, r) = (
            chan_core::origin::Origin(wired.origin.clone()),
            root.clone(),
        );
        let line = wired.line.clone();
        let asked = Asked {
            query: NEEDLE.to_string(),
            under: None,
        };
        tokio::spawn(async move {
            let cwd = crate::source::RemotePath::plain(&r);
            run_search_at(b, line, o, cwd, asked, mine, false).await;
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
// 只要一屏 · 滚到底再要 · 只搜当前目录 · 点一条跳过去
// ═══════════════════════════════════════════════════════════════════

/// 这一帧上命中那一摞的每一行（摘掉目录后面那个 `/`）。
fn painted_hits(painted: &[String], root: &str) -> Vec<String> {
    painted
        .iter()
        .filter(|t| t.starts_with(root))
        .map(|t| t.trim_end_matches('/').to_string())
        .collect()
}

/// 线上每一趟 `files-find` 的入参。
fn find_calls(wired: &testing::Wired) -> Vec<serde_json::Value> {
    wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == CMD_FIND)
        .map(|r| r["args"].clone())
        .collect()
}

/// 最后一行露在帧上、后端说后面还有 ⇒ 同号、`offset` ＝ 手上的条数再要一屏，回来的接在后面；
/// 后面没有了 ⇒ 一趟都不多发（阴性对照）。
#[tokio::test]
async fn the_last_row_on_screen_pulls_the_next_page_with_the_same_seq() {
    let tree = testing::plant("pages", TREE_N, TREE_SEED).expect("造不出那棵树");
    let root = tree.remote_root();
    let wired = testing::wire_up(
        "b1-find-pages",
        FakeBackend::new(COMMANDS, Declared::default())
            .homed(&tree.root)
            .preindexed(&tree.root)
            .paging_by(8),
    )
    .await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, &root);
    // 根那一段在每条路径里 ⇒ 全树都中，远多于一屏。
    let q = root.rsplit('/').next().unwrap().to_string();
    let want = tree.expected(&q);
    assert!(want.len() > 16, "语料不够两屏，本条说明不了什么");
    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, &q);
    testing::settle(&w.search, before, "第一屏").await;
    assert_eq!(w.search.shown().outcome.unwrap().hits.len(), 8);

    let before = w.search.rounds();
    let first = painted_hits(&testing::frame_text(&ctx, &mut w, Vec::new()), &root);
    assert_eq!(first.len(), 8, "第一屏该整屏画出来");
    testing::settle(&w.search, before, "第二屏").await;
    let o = w.search.shown().outcome.unwrap();
    let got: Vec<String> = o.hits.iter().map(|h| h.display()).collect();
    assert_eq!(got.len(), 16, "第二屏没接在后面");
    assert_eq!(got[..8], first[..], "接上之后前一屏变了");
    let distinct: std::collections::BTreeSet<&String> = got.iter().collect();
    assert_eq!(distinct.len(), 16, "两屏有重的 —— 偏移没对上");
    assert!(got.iter().all(|g| want.contains(g)));
    let calls = find_calls(&wired);
    assert_eq!(calls.len(), 2, "只该多发一趟：{calls:?}");
    assert_eq!(calls[1]["offset"], 8, "下一屏该从第 8 条起");
    assert_eq!(
        calls[1]["seq"], calls[0]["seq"],
        "翻页换了号 —— 后端会把它当成新的一问"
    );
    assert_eq!(calls[1]["query"], calls[0]["query"]);

    // 阴性对照：全都在手上了（这一问只中几条）⇒ 画多少帧都不再要。
    let wired2 = testing::wire_up(
        "b1-find-pages-done",
        FakeBackend::new(COMMANDS, Declared::default())
            .homed(&tree.root)
            .preindexed(&tree.root),
    )
    .await;
    let mut w2 = testing::window_on(&wired2, &root);
    let before = w2.search.rounds();
    testing::type_into_search(&ctx, &mut w2, NEEDLE);
    testing::settle(&w2.search, before, "一屏就完").await;
    for _ in 0..3 {
        testing::frame_text(&ctx, &mut w2, Vec::new());
    }
    assert_eq!(find_calls(&wired2).len(), 1, "后面没有了还在要下一屏");
}

/// 「只搜当前目录」开着 ⇒ 带上当前目录；换了目录 ⇒ 自动按新目录再搜；点一条命中 ⇒ 进它所在的目录。
#[tokio::test]
async fn search_here_follows_the_directory_and_a_hit_takes_you_there() {
    let tree = testing::plant("here", TREE_N, TREE_SEED).expect("造不出那棵树");
    let root = tree.remote_root();
    let wired = testing::wire_up(
        "b1-find-here",
        FakeBackend::new(COMMANDS, Declared::default())
            .homed(&tree.root)
            .preindexed(&tree.root),
    )
    .await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, &root);
    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, NEEDLE);
    testing::settle(&w.search, before, "家目录那一趟").await;
    assert!(
        find_calls(&wired)[0].get("under").is_none(),
        "开关关着却带了范围"
    );

    w.set_search_here(true);
    let before = w.search.rounds();
    testing::frame_text(&ctx, &mut w, Vec::new());
    testing::settle(&w.search, before, "开了开关那一趟").await;
    assert_eq!(find_calls(&wired).last().unwrap()["under"], root.as_str());

    // 点第 0 条命中 ⇒ 进它所在的目录、高亮它、框清空。
    let hit = w.search.shown().outcome.unwrap().hits[0].display();
    let (dir, name) = hit.rsplit_once('/').unwrap();
    assert!(w.jump_to_find_hit(0));
    assert_eq!(w.cwd, dir, "点了命中没进它所在的目录");
    assert_eq!(w.query(), "", "跳过去之后框该清空（屏幕换回目录列表）");
    assert!(!name.is_empty());

    // 框里重新打字、目录已经换了 ⇒ 范围跟着新目录。
    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, NEEDLE);
    testing::settle(&w.search, before, "换了目录那一趟").await;
    assert_eq!(find_calls(&wired).last().unwrap()["under"], dir);

    // 框里有字时换目录（上一级）⇒ 下一帧自动按新目录再搜一趟。
    w.navigate_up();
    let up = w.cwd.clone();
    assert_ne!(up, dir);
    let before = w.search.rounds();
    testing::frame_text(&ctx, &mut w, Vec::new());
    testing::settle(&w.search, before, "上一级那一趟").await;
    assert_eq!(find_calls(&wired).last().unwrap()["under"], up.as_str());
}

// ═══════════════════════════════════════════════════════════════════
// 🔴 新鲜度那几个数真的走到了界面上
// ═══════════════════════════════════════════════════════════════════

/// 🔴🔴 ** 那条 ⬜ 的兑现判据。**
///
/// 它要的逐字是「那个数**显示在界面上**」。而记着同族的教训：
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
    // 多一个数：后端没走进去的挂载点（两组各不相同，跨组缺席断言照样罩着它）。
    for (tag, interval, age, unreadable, mounts, entries_hint) in [
        ("b1-fresh-a", 4242u64, 1234u64, 41u64, 3u64, 'a'),
        ("b1-fresh-b", 8765u64, 5678u64, 13u64, 29u64, 'b'),
    ] {
        let tree =
            testing::plant(&format!("fresh-{entries_hint}"), 30, TREE_SEED).expect("造不出那棵树");
        let root = tree.remote_root();
        let declared = Declared {
            rewalk_interval_secs: interval,
            age_secs: age,
            stale: Some(false),
            unreadable_dirs: unreadable,
            skipped_mounts: mounts,
            browse_watch_cap: 64,
            ..Declared::default()
        };
        let wired = testing::wire_up(
            tag,
            FakeBackend::new(COMMANDS, declared)
                .homed(&tree.root)
                .preindexed(&tree.root),
        )
        .await;
        let ctx = ctx_ready();
        let mut w = testing::window_on(&wired, &root);
        let before = w.search.rounds();
        // 按那颗「重建索引」以外的路：直接发一趟（子串留空 ⇒ 只问状态）。
        w.fire_search(None, true);
        testing::settle(&w.search, before, tag).await;
        let painted = testing::frame_text(&ctx, &mut w, Vec::new());
        let line = painted
            .iter()
            // 禁档词换掉（重走周期 → 扫描间隔，terms.json「重走」那一条）⇒ 按「扫描间隔」认那一行。
            .find(|t| t.starts_with("索引 ") && t.contains("扫描间隔"))
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
            // 禁档词换掉（重走周期 → 扫描间隔 · 走完 → 扫完），数一个不少。
            format!("扫描间隔 {interval} 秒"),
            format!("{age} 秒前扫完"),
            format!("有 {unreadable} 个目录读不进去"),
            format!("有 {mounts} 个目录在另一个盘上"),
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

/// 等到那块板子挂上「冷启动首建正在走」。**带上限，绝不挂死**（同 [`testing::settle`]）。
async fn until_first_build_shows(board: &SearchBoard, who: &str) -> u64 {
    for _ in 0..600 {
        if let Some(n) = board.first_build() {
            return n;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("{who}：等了 3 秒板子上还没挂「首建正在走」—— 首建那一趟没挂上它");
}

/// **冷启动首建那一趟正在走时，帧上恰有「正在建索引（首次约 N 秒）」，
/// 走完就没了；N 是后端报的那个数。**
///
/// 用户裁「单列一个数并在搜索界面显示」。三向钉：
/// - **在**：合成后端把重走的应答扣住，这时跑一帧，帧上恰好一段 == [`first_build_line`]`(N)`；
/// - **走**：放行、等答案落地，再跑一帧 —— 「正在建索引」零命中（挂上不摘 = 永远在「建」）；
/// - **跟着后端变**：两组喂不同的 N（都**不是**后端今天声明的 10），跨组缺席 ——
///   写死一个数只会在一组上碰巧对（同 [`the_freshness_numbers_the_backend_reports_really_reach_the_frame`]）。
///
/// ⚠ 按「措辞 ＋ 数」找，不按裸数字找（那一条的教训：`13247 字节` 里有 `13`）。
#[tokio::test]
async fn the_cold_first_build_line_is_on_the_frame_while_it_runs_and_gone_after() {
    let groups = [("b1-cold-a", 37u64), ("b1-cold-b", 58u64)];
    for (i, (tag, secs)) in groups.iter().enumerate() {
        let other = groups[1 - i].1;
        let tree = testing::plant(tag, 30, TREE_SEED).expect("造不出那棵树");
        let root = tree.remote_root();
        let gate = std::sync::Arc::new(tokio::sync::Notify::new());
        let declared = Declared {
            cold_first_build_secs: *secs,
            ..Declared::default()
        };
        let wired = testing::wire_up(
            tag,
            FakeBackend::new(COMMANDS, declared)
                .homed(&tree.root)
                .holding_rebuild(gate.clone()),
        )
        .await;
        let ctx = ctx_ready();
        let mut w = testing::window_on(&wired, &root);
        let before = w.search.rounds();
        testing::type_into_search(&ctx, &mut w, NEEDLE);
        let got = until_first_build_shows(&w.search, tag).await;
        assert_eq!(got, *secs, "板子上挂的不是后端报的那个数");

        // ── 在：重走还扣着 ──
        let painted = testing::frame_text(&ctx, &mut w, Vec::new());
        let want = first_build_line(*secs);
        let hits: Vec<&String> = painted.iter().filter(|t| **t == want).collect();
        assert_eq!(
            hits.len(),
            1,
            "首建正在走，帧上该**恰好一段** {want:?}，这一帧画的是：{painted:?}"
        );
        // 🔴 **异源那一侧**：上面那条相等的两侧都过 `first_build_line` —— 它写死一个数，两侧一起写死
        //    （死值验刀 K4 现打：把函数体写死成 10，上面那条照绿）。⇒ 再用**本文件自己拼**的措辞 ＋ 后端那个数找一遍。
        let mine = format!("正在建索引（首次约 {secs} 秒）");
        assert!(
            painted.iter().any(|t| *t == mine),
            "帧上没有 {mine:?} —— 画出来的不是后端报的那个数：{painted:?}"
        );
        let foreign = format!("首次约 {other} 秒");
        assert!(
            !painted.iter().any(|t| t.contains(foreign.as_str())),
            "这一组帧上出现了另一组的数 {foreign:?} —— 那一行画的不是后端报的数：{painted:?}"
        );
        assert!(
            !painted.iter().any(|t| t.contains("索引还没建过")),
            "首建那一行该**顶替**新鲜度那一行，两句同时在：{painted:?}"
        );

        // ── 走：放行，等答案落地 ──
        gate.notify_one();
        testing::settle(&w.search, before, tag).await;
        assert_eq!(
            w.search.first_build(),
            None,
            "重走回来了，板子上还挂着「首建正在走」"
        );
        let after = testing::frame_text(&ctx, &mut w, Vec::new());
        assert!(
            !after.iter().any(|t| t.contains("正在建索引")),
            "首建走完了，帧上还说「正在建索引」：{after:?}"
        );
        assert!(
            after
                .iter()
                .any(|t| t.starts_with("索引 ") && t.contains("扫描间隔")),
            "首建走完之后新鲜度那一行该回来：{after:?}"
        );
        assert_eq!(
            wired.count(CMD_INDEX_REBUILD),
            1,
            "首建那一趟只该发一条重走"
        );
    }
}

/// 🔴 **阴性对照**：不是首建（索引在、只是 `stale`）的那一趟重走，帧上**没有**「正在建索引」。
///
/// 没有这一条，上面那条可以靠「**每一趟**重走都挂」全绿 —— 而周期性重走是热的，
/// 冷启动那个数套在它头上就是在说假话（两个数分开钉）。
#[tokio::test]
async fn a_warm_rewalk_never_claims_to_be_the_cold_first_build() {
    let tree = testing::plant("warm", 30, TREE_SEED).expect("造不出那棵树");
    let root = tree.remote_root();
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let declared = Declared {
        age_secs: 9_999,
        stale: Some(true),
        cold_first_build_secs: 37,
        ..Declared::default()
    };
    let wired = testing::wire_up(
        "b1-find-warm",
        FakeBackend::new(COMMANDS, declared)
            .homed(&tree.root)
            .preindexed(&tree.root)
            .holding_rebuild(gate.clone()),
    )
    .await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, &root);
    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, NEEDLE);
    // 等到重走**真的发出去、而且还扣着**（线上记录先于应答进账）。
    let mut seen = false;
    for _ in 0..600 {
        if wired.count(CMD_INDEX_REBUILD) == 1 {
            seen = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        seen,
        "stale 那一趟没发重走 —— 本条的前提没成立，下面的零命中不作数"
    );
    let painted = testing::frame_text(&ctx, &mut w, Vec::new());
    assert_eq!(w.search.first_build(), None, "热的重走挂上了「首建正在走」");
    assert!(
        !painted.iter().any(|t| t.contains("正在建索引")),
        "热的重走在帧上说「正在建索引（首次…）」：{painted:?}"
    );
    gate.notify_one();
    testing::settle(&w.search, before, "warm").await;
}

/// 🔴 **零命中型 ＋ 唯一住址**：重走周期那个数在客户端这一侧**一处都没有**，
/// 而它在后端**恰好一处**。
///
/// # 🔴用户拍板了，而这一条要买的东西**没变**
///
/// 上一版这段头注逐字写着「：周期该定多少**还没拍板**」。
/// **那句话今天过期了**：用户 2026-09-22 同一轮裁「按推荐来」，而推荐原文逐字是
/// 「先按现值 **300 秒**发，界面上把它**显示出来**」⇒ 那个数**定了**。
///
/// 而「定了」并**不**削弱本条 —— 恰恰相反：不许前后端各写一份，
/// 一个**已经定下来的**数比一个待定的数更容易被人顺手抄到界面这一侧
/// （「反正就是 300」）。⇒ 本条这一刀**加宽**了，见下面 §射程。
///
/// # 射程：从一份文件扩到**整棵客户端树**，另加一条后端侧的唯一性
///
/// 上一版只扫 `filewin/find.rs`。那是个洞：把 `300` 写进 `shell.rs` / `source.rs`
/// 的任何一处，上一版**看不见**。今天扫的是 `filewin/` 整棵树的生产段。
///
/// 窗口独立成包之后「`filewin/` 整棵树」是两棵：窗口包 `src/frontend/filewin/src/` ＋ monitor 那一侧 `src/frontend/shell/src/filewin/`
/// （开窗入口 · 起进程），人群与搬家前逐份相同；键写相对 `src/frontend/` 的路径（两棵里都有 `proc.rs`）。
///
/// 加宽之后撞到一个**真的、合法的** `300`，逐条登记在 [`MILLIS_NOT_SECONDS`] 里：
/// monitor 那一侧 `proc.rs` 的 `EARLY_FAILURE_BUDGET` 是 **300 毫秒**（开窗那一跳的预算）——
/// 与重走周期**不同单位、不同量纲、不同用途**。
/// ⚠ 本仓为「裸数字不是一把尺子」栽过（`13247 字节` 里有 `13`）⇒ 例外**明写**，
/// 而且那张表自己也被两向钉着：登记了而盘上没有 ⇒ 也红。
///
/// # ⚠ 它买不到什么
///
/// - **买不到「界面上显示的就是后端报的那个数」** —— 那由
///   [`the_freshness_numbers_the_backend_reports_really_reach_the_frame`] 买
///   （真跑一帧、从 galley 里读回来、而且喂两组不同的数）。
/// - **买不到那个值是多少对不对**（那是产品判断，用户已裁）。本条刻意**不**断言
///   后端那个常量等于 300 —— 断言它就等于**在这一侧又造了一个知道那个值的地方**，
///   而那正是 `D2` 禁的事。本条只数「住址有几个」。
/// - **买不到「后端那一处真的被送上线」**（那由 `files-index-status` 的契约判据买：
///   `STATUS_FIELDS` 里有它、少了它就解析失败）。
#[test]
fn no_rewalk_period_literal_lives_on_this_side() {
    /// 客户端树里**合法**的那几处 `300`：`(文件, 它是什么)`。
    ///
    /// 🔴 默认拒绝：不在这张表里的一处 `300` 就是红。
    /// 而这张表**反向也钉**：登记了而盘上没有 ⇒ 说明那一处改了/没了，得回来看一眼。
    const MILLIS_NOT_SECONDS: &[(&str, &str)] = &[(
        "shell/src/filewin/proc.rs",
        "`EARLY_FAILURE_BUDGET` = 300 **毫秒** —— 开窗那一跳的预算（人感觉不到 /          毫秒级失败一定抓得到之间的取值）。与重走周期不同单位、不同量纲、不同用途。",
    )];

    // 运行时拼，免得命中本行自己。
    let needle = format!("{}{}", 30, 0);
    let repo = crate::guard_support::repo_root();
    let frontend = repo.join("src/frontend");
    // 🔴 走 `guard_core` 而不是裸 `read_dir`（`scanning_guard_registry` 那条纪律）。
    let files: Vec<(String, String)> = [
        crate::guard_support::crate_src_root(),
        repo.join("src/frontend/shell/src/filewin"),
    ]
    .iter()
    .flat_map(|dir| guard_core::scan_tree!(dir, &["rs"]))
    .map(|(p, raw)| {
        let rel = p
            .strip_prefix(&frontend)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        (rel, raw)
    })
    .collect();
    // ★ 抽取器自检①：语料塌了 ⇒ 下面那条零命中恒真。
    assert!(
        files.len() >= 14,
        "`filewin/` 只扫到 {} 份 `.rs` —— 语料面坏了，本条此刻是空转的",
        files.len()
    );
    let mut hits: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for (stem, raw) in &files {
        let prod = guard_core::production_code(raw);
        scanned += prod.len();
        for line in prod.lines() {
            // 只看代码，不看注释（注释里当然会提到那个数）。
            let code = line.trim_start();
            if code.starts_with("//") || !code.contains(needle.as_str()) {
                continue;
            }
            if MILLIS_NOT_SECONDS.iter().any(|(f, _)| *f == stem.as_str()) {
                continue;
            }
            hits.push(format!("  {stem}: {}", code.trim()));
        }
    }
    // ★ 抽取器自检②：剥完还得有东西可扫。
    assert!(
        scanned > 100_000,
        "剥掉测试段之后只剩 {scanned} 字节可扫 —— 本条此刻是空转的"
    );
    assert!(
        hits.is_empty(),
        "客户端这一侧出现了 `{needle}`：\n{}\n\n\
         ⚠ 那是后端声明的重走周期（用户 2026-09-22 裁「先按现值 300 秒发」）——\n\
         `D2`：它只许有一个家，而那个家在后端。\n\
         这一侧要用它，走 `IndexStatus::rewalk_interval_secs`（后端报过来的那一份）。\n\
         真的是另一个意思的 `300`（比如毫秒）⇒ 往 `MILLIS_NOT_SECONDS` 里加一行并写清它是什么。",
        hits.join("\n")
    );
    // ★ 例外表的**反向**那一半：登记了而盘上没有 ⇒ 也红。
    for (f, what) in MILLIS_NOT_SECONDS {
        let raw = files
            .iter()
            .find(|(p, _)| p.as_str() == *f)
            .map(|(_, raw)| guard_core::production_code(raw))
            .unwrap_or_else(|| panic!("例外表点着 `{f}`，而语料里没有这份文件"));
        assert!(
            raw.contains(needle.as_str()),
            "例外表给 `{f}` 开了口子（{what}），而它的生产段里**没有** `{needle}` ——\n\
             那一处改了或没了 ⇒ 回来把这一行删掉（留着就是一个白开的口子）"
        );
    }

    // ══════════════════════════════════════════════════════════════
    // 🔴 另一半：那个数在**后端恰好一个住址**
    // ══════════════════════════════════════════════════════════════
    //
    // 只数「零命中」是半条：客户端一处都没有，而后端长出第二份（比如
    // `browse_watch.rs` 自己也写一个）时，本条照旧全绿 —— 而那时那个数就有两个家了。
    let backend = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../backend");
    let btree = guard_core::scan_tree!(&backend, &["rs"]);
    assert!(
        btree.len() >= 40,
        "后端树只扫到 {} 份 `.rs` —— 语料面坏了，下面那条相等此刻不可信",
        btree.len()
    );
    let decl = format!("pub const {}", "REWALK_INTERVAL_SECS");
    let homes: Vec<String> = btree
        .iter()
        .filter(|(_, raw)| guard_core::production_code(raw).contains(&decl))
        .map(|(p, _)| {
            p.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(
        homes,
        vec!["index.rs".to_string()],
        "重走周期那个数的住址现打是 {homes:?}，而它只许有一个（`files/index.rs`）。\n\
         多一处 = `D2` 破了；少一处 = 它改名/搬家了，而这一侧那条零命中随之变成空转。"
    );
    // 冷启动首建那个数同形：后端恰好一个住址（它与周期**分开**住，各是各的常量）。
    //   ⚠ 客户端侧那个值（今天 10）**不做**零命中扫描 —— `10` 这种裸数字满树都是、扫它是一把假尺子；
    //   「界面上画的是后端报的那个数」由 `the_cold_first_build_line_is_on_the_frame_while_it_runs_and_gone_after`
    //   喂两组数、跨组缺席买。
    let cold_decl = format!("pub const {}", "COLD_FIRST_BUILD_SECS");
    let cold_homes: Vec<String> = btree
        .iter()
        .filter(|(_, raw)| guard_core::production_code(raw).contains(&cold_decl))
        .map(|(p, _)| {
            p.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(
        cold_homes,
        vec!["index.rs".to_string()],
        "冷启动首建那个数的住址现打是 {cold_homes:?}，而它只许有一个（`files/index.rs`）"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 契约面：名字与字段名对着那份冻结的线上契约
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **线上名里一个 `.` 都不许有。**
///
/// 本仓三个取词器的字符集都是「字母数字 / `_` / `-`」，
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
             线上与 CLI 一律连字符，理由是取词器的字符集。"
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
        "browse_watches": 1, "browse_watch_cap": 64, "cold_first_build_secs": 47,
        "skipped_mounts": 0,
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
/// 文件名被有损解码过之后，拿着那串替换字符回去找，
/// **找的是一个不存在的名字**。⇒ 这一侧把字节留着，只在画的时候才有损。
#[test]
fn a_non_utf8_hit_keeps_its_bytes_and_says_it_is_lossy() {
    let bytes = vec![0x2fu8, 0x74, 0x6d, 0x70, 0x2f, 0xff, 0xfe, 0x2e, 0x72, 0x73];
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let tail = serde_json::json!({
        "total_hits": 1, "truncated": false, "scanned": 9,
        "index_age_secs": 3, "index_missing": false, "stale": false,
        "index_root": "/tmp", "out_of_index": false, "cover_root": "/tmp", "seq": 1, "offset": 0,
    });
    let with_hits = |hits: serde_json::Value| {
        let mut v = tail.clone();
        v["hits"] = hits;
        v
    };
    let data = with_hits(serde_json::json!([{ "path": { "b16": hex }, "kind": "file" }]));
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
    let plain = with_hits(serde_json::json!([{ "path": "/tmp/a", "kind": "dir" }]));
    let p = decode_find(&plain).expect("该解析得动");
    assert!(!p.hits[0].lossy(), "普通路径被误判成有损");
    assert_eq!(p.hits[0].display(), "/tmp/a");
    assert_eq!(p.hits[0].row_text(), "/tmp/a/", "目录那一行后面该带 `/`");
    assert!(!o.hits[0].dir);
}

/// 命中那一行**一定**带着 `scanned`：「没命中」与「索引是空的」在屏幕上本来一样。
#[test]
fn the_hits_line_always_carries_the_scanned_count() {
    let empty_index = FindOutcome::default();
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

/// 入参形状与那份契约一致：搜索词原样发（这一侧不读语法）、号与搜索框名跟着走、范围只在开关开着时带。
#[test]
fn the_arg_builders_use_the_documented_keys() {
    let asked = Asked {
        query: "  report ext:pdf|txt !old ".to_string(),
        under: None,
    };
    assert_eq!(
        find_args(&asked, 7, "s-1", 0),
        serde_json::json!({
            "query": "  report ext:pdf|txt !old ",
            "seq": 7,
            "stream": "s-1",
            "offset": 0,
            "limit": PAGE,
        }),
        "搜索词该原样发（一个字都不动），不带范围 ⇒ 后端搜家目录"
    );
    let here = Asked {
        query: "x".into(),
        under: Some(crate::source::RemotePath::plain("/home/u/p")),
    };
    assert_eq!(find_args(&here, 8, "s-1", 100)["under"], "/home/u/p");
    assert_eq!(
        rebuild_args_at(Some(b"/home/u")),
        serde_json::json!({ "path": "/home/u" })
    );
    assert_eq!(rebuild_args_at(None), serde_json::json!({}));
    assert_eq!(
        browse_args_at(&[
            crate::source::RemotePath::plain("/a"),
            crate::source::RemotePath::plain("/b")
        ]),
        serde_json::json!({ "dirs": ["/a", "/b"] })
    );
}

// ═══════════════════════════════════════════════════════════════════
// 只读铁律
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **零命中型**：这一族纯读（边界①）⇒ 这一侧一个写动词都没有。
///
/// `tests/backend/readonly_guard.rs` 那 4200 多行因此一行都不用改，
/// 而**这一侧不许把它变成假的**：客户端偷偷写一笔，那条铁律看不见
/// （它的射程逐字只钉 backend crate）。
#[test]
fn this_module_never_touches_the_disk() {
    let src = std::fs::read_to_string(crate::guard_support::crate_src_root().join("find.rs"))
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
             要动盘上的东西，先去边界① 把那条改掉（那是放宽一条红线）。"
        );
    }
}
