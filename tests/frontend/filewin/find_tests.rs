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
//! 3. [`the_status_line_shows_the_numbers_the_backend_reports`] 喂**两组
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
    // 结果表上一行 ＝ 名字一格 ＋ 位置一格（相对搜索起点）⇒ 两格拼回全路径再比。
    let got: std::collections::BTreeSet<String> =
        painted_hits(&painted, &w.search.hit_rows(), &root)
            .into_iter()
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
    // 「没建过」要在状态行上说出来（它与「这台机器上没有这个文件」是两件事，混了用户会删错东西），
    // 而且这一形**不许**画「无匹配」那一句。
    let st = w.search.shown().status.expect("这一趟该拿到状态");
    assert!(
        st.index_missing,
        "这台合成后端该报「没建过」—— 夹具坏了，下面的读数不许用"
    );
    let o = w.search.shown().outcome.expect("这一趟该有一份答案");
    assert!(o.index_missing, "`{CMD_FIND}` 该回 `index_missing: true`");
    assert!(
        painted.contains(&not_built_line()),
        "状态行没说「文件清单未建」。\n这一帧画的是：{painted:?}"
    );
    assert!(
        !painted.contains(&no_match_line(NEEDLE)),
        "清单没建过却说「无匹配」：{painted:?}"
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
            comms_inward::origin::Origin(wired.origin.clone()),
            root.clone(),
        );
        let line = wired.line.clone();
        let asked = Asked {
            query: NEEDLE.to_string(),
            ..Asked::default()
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

/// 这一帧上结果表的每一行，拼回全路径：表上一行先画名字、紧跟着画位置（相对搜索起点 `root`，直接在起点里的不画位置）。
/// 只认帧上**真画出来**的那几对（名字 · 位置）—— 后端那一份只用来认「哪一对是一行」。
fn painted_hits(painted: &[String], rows: &[crate::rows::HitRow], root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < painted.len() {
        let hit = rows.iter().find(|r| {
            painted[i] == r.name
                && (r.location.is_empty() || painted.get(i + 1) == Some(&r.location))
        });
        match hit {
            Some(r) if r.location.is_empty() => {
                out.push(format!("{root}/{}", r.name));
                i += 1;
            }
            Some(r) => {
                out.push(format!("{root}/{}/{}", r.location, r.name));
                i += 2;
            }
            None => i += 1,
        }
    }
    out
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
    let first = painted_hits(
        &testing::frame_text(&ctx, &mut w, Vec::new()),
        &w.search.hit_rows(),
        &root,
    );
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

/// 范围默认「当前目录以下」⇒ 带上当前目录；换了目录 ⇒ 自动按新目录再搜；「整台机器」⇒ 只发 `scope`、不带目录；
/// 打开一条文件命中 ⇒ 进它所在的目录，打开一条目录命中 ⇒ 进它自己。
#[tokio::test]
async fn the_scope_follows_the_directory_and_a_hit_takes_you_there() {
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
    testing::settle(&w.search, before, "当前目录那一趟").await;
    assert_eq!(
        find_calls(&wired)[0]["under"],
        root.as_str(),
        "默认范围不是当前目录"
    );
    assert!(find_calls(&wired)[0].get("scope").is_none());

    w.set_search_whole(true);
    let before = w.search.rounds();
    w.fire_search(None, false);
    testing::settle(&w.search, before, "整台机器那一趟").await;
    let last = find_calls(&wired).last().unwrap().clone();
    assert_eq!(last["scope"], "machine");
    assert!(last.get("under").is_none(), "整台机器还带了目录：{last}");
    w.set_search_whole(false);

    // 打开第一条文件命中 ⇒ 进它所在的目录、高亮它、框清空。
    let before = w.search.rounds();
    w.fire_search(None, false);
    testing::settle(&w.search, before, "回到当前目录以下").await;
    let o = w.search.shown().outcome.unwrap();
    let i = o.hits.iter().position(|h| !h.dir).expect("命中里该有文件");
    let hit = o.hits[i].display();
    let (dir, name) = hit.rsplit_once('/').unwrap();
    assert!(w.jump_to_find_hit(i));
    assert_eq!(w.cwd, dir, "打开文件命中没进它所在的目录");
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

    // 目录命中 ⇒ 进它自己。
    let o = w.search.shown().outcome.unwrap();
    if let Some(j) = o.hits.iter().position(|h| h.dir) {
        let d = o.hits[j].display();
        assert!(w.jump_to_find_hit(j));
        assert_eq!(w.cwd, d, "打开目录命中没进它自己");
    }
}

/// 点表头 ⇒ 按那一列从头再问一遍（排是后端排的，这一侧只发列名与正反）；翻页带着同一个序。
#[tokio::test]
async fn a_header_click_asks_the_backend_for_that_order_and_pages_keep_it() {
    let tree = testing::plant("sort", TREE_N, TREE_SEED).expect("造不出那棵树");
    let root = tree.remote_root();
    let wired = testing::wire_up(
        "b1-find-sort",
        FakeBackend::new(COMMANDS, Declared::default())
            .homed(&tree.root)
            .preindexed(&tree.root)
            .paging_by(8),
    )
    .await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, &root);
    let q = root.rsplit('/').next().unwrap().to_string();
    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, &q);
    testing::settle(&w.search, before, "相关度那一趟").await;
    assert_eq!(find_calls(&wired)[0]["sort"], "relevance");
    for (col, sort, desc) in [
        (SortCol::Name, "name", false),
        (SortCol::Name, "name", true),
        (SortCol::Name, "relevance", false),
        (SortCol::Location, "location", false),
        (SortCol::Location, "location", true),
        (SortCol::Mtime, "mtime", true),
        (SortCol::Size, "size", true),
        (SortCol::Size, "size", false),
    ] {
        let before = w.search.rounds();
        assert!(w.click_hit_header(col, None));
        testing::settle(&w.search, before, sort).await;
        let last = find_calls(&wired).last().unwrap().clone();
        assert_eq!(
            (last["sort"].as_str(), last["desc"].as_bool()),
            (Some(sort), Some(desc)),
            "点了 {col:?}"
        );
        assert_eq!(last["offset"], 0, "换了序该从头问");
    }
    // 按位置倒序：帧上画的那几行就是后端回的那个序；滚到底要的下一屏带着同一个序。
    w.click_hit_header(SortCol::Location, None);
    let before = w.search.rounds();
    w.click_hit_header(SortCol::Location, None);
    testing::settle(&w.search, before, "位置倒序").await;
    let rows = w.search.hit_rows();
    let painted = testing::frame_text(&ctx, &mut w, Vec::new());
    let shown = painted_hits(&painted, &rows, &root);
    let from_backend: Vec<String> = w
        .search
        .shown()
        .outcome
        .unwrap()
        .hits
        .iter()
        .map(|h| h.display())
        .collect();
    assert_eq!(
        shown,
        from_backend[..shown.len()].to_vec(),
        "帧上的顺序不是后端回的顺序"
    );
    let mut more = serde_json::Value::Null;
    for _ in 0..600 {
        more = find_calls(&wired).last().unwrap().clone();
        if more["offset"] != 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        (
            more["sort"].as_str(),
            more["desc"].as_bool(),
            more["offset"].as_u64()
        ),
        (Some("location"), Some(true), Some(8)),
        "下一屏没带着同一个序"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 🔴 新鲜度那几个数真的走到了界面上
// ═══════════════════════════════════════════════════════════════════

/// 状态行上的数都是后端报的：文件清单多久前（`index_age_secs`）· 几个目录无权限 · 其他盘几个目录没扫。
/// 喂两组不同的数、跨组缺席 —— 写死一个数只会在一组上碰巧对。按「措辞 ＋ 数」找，不按裸数字找。
#[tokio::test]
async fn the_status_line_shows_the_numbers_the_backend_reports() {
    let mut lines: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    for (tag, age, said, unreadable, mounts, hint) in [
        ("b1-fresh-a", 1234u64, "20m", 41u64, 3u64, 'a'),
        ("b1-fresh-b", 7300u64, "2h", 13u64, 29u64, 'b'),
    ] {
        let tree = testing::plant(&format!("fresh-{hint}"), 30, TREE_SEED).expect("造不出那棵树");
        let root = tree.remote_root();
        let declared = Declared {
            rewalk_interval_secs: 99_999,
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
        testing::type_into_search(&ctx, &mut w, NEEDLE);
        testing::settle(&w.search, before, tag).await;
        let painted = testing::frame_text(&ctx, &mut w, Vec::new());
        let o = w.search.shown().outcome.expect("该有答案");
        assert_eq!(o.index_age_secs, age, "解出来的年龄不是后端报的那个数");
        let frags = vec![
            age_line(&o),
            format!("{unreadable} 个目录无权限"),
            format!("其他盘 {mounts} 个目录未扫"),
            scope_line(&o),
        ];
        for f in &frags {
            assert!(painted.contains(f), "状态行上没有 {f:?}：{painted:?}");
        }
        // 异源那一侧：本文件自己拼的措辞 ＋ 后端那个数。
        let mine = format!("文件清单 · {said} 前");
        assert!(painted.contains(&mine), "帧上没有 {mine:?}：{painted:?}");
        lines.push((frags, painted));
    }
    let (a, b) = (&lines[0], &lines[1]);
    for f in &a.0[..3] {
        assert!(!b.1.contains(f), "第二组帧上出现了第一组的读数 {f:?}");
    }
    for f in &b.0[..3] {
        assert!(!a.1.contains(f), "第一组帧上出现了第二组的读数 {f:?}");
    }
}

/// 状态行「n 个目录无权限［查看］」：点［查看］⇒ 一层浮层列后端交的那几个目录（相对搜索起点；多于交来的 ⇒「另外 n 个」）；
/// 点一行 ⇒ 复制那条**绝对**路径、右下角回执「路径已复制」，**不**跳过去（跳过去只会落到「无权限」那一条）。数与名单都是后端报的。
#[tokio::test]
async fn the_unreadable_dirs_can_be_listed_and_a_click_copies_the_path() {
    let tree = testing::plant("unreadable-list", 30, TREE_SEED).expect("造不出那棵树");
    let root = tree.remote_root();
    let declared = Declared {
        unreadable_dirs: 3,
        unreadable_paths: vec![format!("{root}/secret"), format!("{root}/a/locked")],
        ..Declared::default()
    };
    let wired = testing::wire_up(
        "unreadable-list",
        FakeBackend::new(COMMANDS, declared)
            .homed(&tree.root)
            .preindexed(&tree.root),
    )
    .await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, &root);
    let before = w.search.rounds();
    testing::type_into_search(&ctx, &mut w, NEEDLE);
    testing::settle(&w.search, before, "unreadable-list").await;
    let screen = egui::vec2(1280.0, 800.0);
    let mut t = 10.0;
    let mut paint = |w: &mut crate::shell::FileWindow, ev: Vec<egui::Event>| {
        t += 0.5;
        crate::copy::testing::painted_text(&ctx, screen, t, ev, |ui| w.frame_body(ui))
    };
    let _ = paint(&mut w, Vec::new());
    let painted = paint(&mut w, Vec::new());
    let look =
        crate::copy::testing::rects_of(&painted, &copy_text("rsFilewinFind.status.look", &[]));
    assert_eq!(look.len(), 1, "状态行上没有［查看］：{painted:?}");
    assert!(crate::copy::testing::painted_contains(
        &painted,
        "3 个目录无权限"
    ));
    let rel = "a/locked";
    assert!(
        crate::copy::testing::rects_of(&painted, rel).is_empty(),
        "没点［查看］就把名单摆出来了"
    );
    let _ = paint(&mut w, crate::rows::testing::click_at(look[0].center()));
    let _ = paint(&mut w, Vec::new());
    let painted = paint(&mut w, Vec::new());
    let row = crate::copy::testing::rects_of(&painted, rel);
    assert_eq!(
        row.len(),
        1,
        "点了［查看］没列出那几个目录（相对搜索起点）：{painted:?}"
    );
    assert_eq!(crate::copy::testing::rects_of(&painted, "secret").len(), 1);
    assert!(
        crate::copy::testing::painted_contains(
            &painted,
            &copy_text("rsFilewinFind.status.holesMore", &[("n", "1")])
        ),
        "后端说 3 个、交了 2 个，末行没说「另外 1 个」"
    );
    let cwd = w.cwd.clone();
    let out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
            time: Some(t + 0.5),
            events: crate::rows::testing::click_at(row[0].center()),
            ..Default::default()
        },
        |ui| w.frame_body(ui),
    );
    let copied = out.platform_output.commands.iter().find_map(|c| match c {
        egui::OutputCommand::CopyText(s) => Some(s.clone()),
        _ => None,
    });
    out.drop_without_applying_deltas();
    assert_eq!(
        copied,
        Some(format!("{root}/a/locked")),
        "点了一行，复制的不是那条绝对路径"
    );
    assert_eq!(
        w.receipt.as_deref(),
        Some(copy_text("rsFilewinShell.receipt.pathCopied", &[]).as_str()),
        "点了一行没给回执"
    );
    assert_eq!(w.cwd, cwd, "点了一行却跳过去了");
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

/// **冷启动首建那一趟正在走时，帧上恰有「首建文件清单 · 机器 · 约 Ns」，
/// 走完就没了；N 是后端报的那个数。**
///
/// 要求「单列一个数并在搜索界面显示」。三向钉：
/// - **在**：合成后端把重走的应答扣住，这时跑一帧，帧上恰好一段 == [`first_build_line`]`(N)`；
/// - **走**：放行、等答案落地，再跑一帧 —— 「首建文件清单」零命中（挂上不摘 = 永远在「建」）；
/// - **跟着后端变**：两组喂不同的 N（都**不是**后端今天声明的 10），跨组缺席 ——
///   写死一个数只会在一组上碰巧对（同 [`the_status_line_shows_the_numbers_the_backend_reports`]）。
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
        let machine = w.source.label();
        let want = first_build_line(&machine, *secs);
        let hits: Vec<&String> = painted.iter().filter(|t| **t == want).collect();
        assert_eq!(
            hits.len(),
            1,
            "首建正在走，帧上该**恰好一段** {want:?}，这一帧画的是：{painted:?}"
        );
        // 🔴 **异源那一侧**：上面那条相等的两侧都过 `first_build_line` —— 它写死一个数，两侧一起写死
        //    （死值验刀 K4 现打：把函数体写死成 10，上面那条照绿）。⇒ 再用**本文件自己拼**的措辞 ＋ 后端那个数找一遍。
        let mine = format!("首建文件清单 · {machine} · 约 {secs}s");
        assert!(
            painted.iter().any(|t| *t == mine),
            "帧上没有 {mine:?} —— 画出来的不是后端报的那个数：{painted:?}"
        );
        let foreign = format!("约 {other}s");
        assert!(
            !painted.iter().any(|t| t.contains(foreign.as_str())),
            "这一组帧上出现了另一组的数 {foreign:?} —— 那一行画的不是后端报的数：{painted:?}"
        );
        assert!(
            !painted.contains(&not_built_line()),
            "首建那一行该**顶替**「文件清单未建」，两句同时在：{painted:?}"
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
            !after.iter().any(|t| t.contains("首建文件清单")),
            "首建走完了，帧上还说「首建文件清单」：{after:?}"
        );
        assert!(
            after.iter().any(|t| t.starts_with("文件清单 · ")),
            "首建走完之后「文件清单 · 多久前」该回来：{after:?}"
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
        !painted.iter().any(|t| t.contains("首建文件清单")),
        "热的重走在帧上说「首建文件清单」：{painted:?}"
    );
    gate.notify_one();
    testing::settle(&w.search, before, "warm").await;
}

/// 🔴 **零命中型 ＋ 唯一住址**：重走周期那个数在客户端这一侧**一处都没有**，
/// 而它在后端**恰好一处**。
///
/// # 🔴 周期定了，而这一条要买的东西**没变**
///
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
///   [`the_status_line_shows_the_numbers_the_backend_reports`] 买
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
         ⚠ 那是后端声明的重走周期（「先按现值 300 秒发」）——\n\
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
/// 那一侧现读协议参考 `src/doc/IPC-COMMANDS.md` 那两张表（由后端的命令登记生成）。
/// ⇒ 后端改了字段名而文档跟着改，本条会红，逼这一侧一起改。
///
/// ⚠ **它买不到「后端真的发这几个字段」** —— 后端那棵树不在本 crate 的依赖图里
/// （`src/backend` 是独立 crate）。它买到的是「这一侧与那份文档不漂」。
#[test]
fn the_reply_shapes_match_the_frozen_wire_contract() {
    let doc = include_str!("../../../src/doc/IPC-COMMANDS.md");
    for (cmd, declared) in [(CMD_FIND, FIND_FIELDS), (CMD_INDEX_STATUS, STATUS_FIELDS)] {
        let heading = format!("#### `{cmd}`");
        let start = doc
            .find(&heading)
            .unwrap_or_else(|| panic!("`src/doc/IPC-COMMANDS.md` 里没有 `{heading}` 那一小节"));
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
    let doc = include_str!("../../../src/doc/IPC-COMMANDS.md");
    for c in COMMANDS {
        assert!(
            doc.contains(&format!("#### `{c}`")),
            "`src/doc/IPC-COMMANDS.md` 里没有 `{c}` 那一小节 —— \n\
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
        "skipped_mounts": 0, "unreadable_paths": [],
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
    assert_eq!(p.hits[0].table_row().name, "a");
    assert!(p.hits[0].dir);
    assert!(!o.hits[0].dir);
}

/// 入参形状与那份契约一致：搜索词原样发（这一侧不读语法）、号与搜索框名跟着走、范围只在开关开着时带。
#[test]
fn the_arg_builders_use_the_documented_keys() {
    let asked = Asked {
        query: "  report ext:pdf|txt !old ".to_string(),
        ..Asked::default()
    };
    assert_eq!(
        find_args(&asked, 7, "s-1", 0),
        serde_json::json!({
            "query": "  report ext:pdf|txt !old ",
            "seq": 7,
            "stream": "s-1",
            "offset": 0,
            "limit": PAGE,
            "sort": "relevance",
            "desc": false,
        }),
        "搜索词该原样发（一个字都不动），不带范围 ⇒ 后端搜家目录"
    );
    let here = Asked {
        query: "x".into(),
        under: Some(crate::source::RemotePath::plain("/home/u/p")),
        sort: FindSort {
            col: SortCol::Mtime,
            desc: true,
        },
        ..Asked::default()
    };
    let a = find_args(&here, 8, "s-1", 100);
    assert_eq!(
        (a["under"].as_str(), a["sort"].as_str(), a["desc"].as_bool()),
        (Some("/home/u/p"), Some("mtime"), Some(true))
    );
    let whole = Asked {
        machine: true,
        ..here
    };
    let a = find_args(&whole, 8, "s-1", 0);
    assert_eq!(a["scope"], "machine");
    assert!(a.get("under").is_none(), "整台机器不该带目录");
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

/// 往下翻那一问失败（通道断了 / 回包解不出）⇒ 这一问不再自动往下翻（此前命中滚在底部时每帧再发一趟、每趟失败又敲一次重画）；
/// 换一问（再敲字 / 重建）才放开。
#[test]
fn a_failed_next_page_is_not_retried_every_frame() {
    let b = SearchBoard::default();
    let asked = Asked {
        query: "x".into(),
        ..Asked::default()
    };
    b.invalidate(&asked);
    let mine = b.start();
    store_if_current(
        &b,
        mine,
        &asked,
        Round {
            outcome: Some(FindOutcome {
                hits: vec![Hit {
                    path: b"/h/x".to_vec(),
                    ..Hit::default()
                }],
                total_hits: 5,
                truncated: true,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    let (m, _, offset) = b.claim_more().expect("后面还有，却不往下翻");
    assert_eq!(offset, 1);
    assert!(append_if_current(&b, m, Err("通道断了".into())));
    assert_eq!(b.claim_more(), None, "翻页失败之后每帧又发一趟");
    assert_eq!(b.shown().notice.as_deref(), Some("通道断了"));
    b.invalidate(&asked);
    assert!(!b.page_failed(), "换一问之后还闩着");
}
