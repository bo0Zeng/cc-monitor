use super::*;
use crate::history::{EntryMetadata, HistoryMetadata};

/// daemon 那一行的夹具。**字段名从生产常量取** —— 见 [`REMOTE_SESSION_IDS_FIELD`]
/// 的头注：名字写死在测试里，改名就会让判据红，那等于把「判能力」偷换成「判名字」，
/// 正是 `KR83D1` 第 ③ 刀要治的。
fn row(dir: &str, sids: &[&str]) -> serde_json::Value {
    let mut v = serde_json::json!({
        "dirName": dir,
        "projectPath": format!("/home/u/{dir}"),
        "sessionCount": sids.len(),
        "lastActivityMs": 1_700_000_000_000i64,
    });
    v[REMOTE_SESSION_IDS_FIELD] = serde_json::json!(sids);
    v
}

/// 旧 daemon（`K-R83` 之前）的那一行：四个字段，**没有**会话 sid 清单。
fn row_without_ids(dir: &str, session_count: u64) -> serde_json::Value {
    serde_json::json!({
        "dirName": dir,
        "projectPath": format!("/home/u/{dir}"),
        "sessionCount": session_count,
        "lastActivityMs": 1_700_000_000_000i64,
    })
}

fn metadata_with(starred: &[&str], hidden: &[&str]) -> HistoryMetadata {
    let mut m = HistoryMetadata::default();
    for sid in starred {
        m.entries.entry((*sid).to_string()).or_default().starred = true;
    }
    for sid in hidden {
        m.entries.entry((*sid).to_string()).or_default().hidden = true;
    }
    m
}

fn cfg(label: &str) -> RemoteConfig {
    serde_json::from_value(serde_json::json!({
        "host": format!("{label}.example"),
        "label": label,
        "user": "u",
        "daemonPath": "/opt/cc-monitor-remote",
    }))
    .expect("夹具配置")
}

// ── `K-R92` 的假真相源：判据用它喂「答得出 / 答不出」两侧 ──────────────────
//
// ⚠ 它是**夹具**，不是第二份实现：生产那份是 [`NoLivenessOracleYet`]，
// 两者的差别正是本件要判的东西（这条路端不端得动一个真值）。

/// 点名哪几个 sid 活着，其余**确定没活**（答得出）。
struct Oracle(&'static [&'static str]);
impl LivenessOracle for Oracle {
    fn is_live(&self, _origin: &str, sid: &str) -> Counted<bool> {
        Counted::Known(self.0.contains(&sid))
    }
}

/// 只对点名的那几个 sid 答不出，其余确定没活 —— 用来验「一行里混着答得出与答不出」。
struct OracleBlindTo(&'static [&'static str]);
impl LivenessOracle for OracleBlindTo {
    fn is_live(&self, _origin: &str, sid: &str) -> Counted<bool> {
        if self.0.contains(&sid) {
            Counted::Unknown(WhyUnknown::NoRemoteLivenessOracle)
        } else {
            Counted::Known(false)
        }
    }
}

// ───────────────────────────── `KR83D1` ─────────────────────────────

/// ★★ `KR83D1`：**daemon 那一行带得出算这三个数所需的东西。**
///
/// 判的是**性质**：拿到那一行 ＋ 本机 metadata，`starred_count` / `hidden_count`
/// 就地算得出**真值**。
///
/// ⚠ 本条**不判字段叫什么名字**：夹具的那个 key 是从生产常量取的，
/// 只改名、内容等价 ⇒ 本条照常绿（第 ③ 刀）。
#[test]
fn a_row_that_carries_session_ids_lets_us_compute_the_real_numbers() {
    let md = metadata_with(&["s2"], &["s1", "s3"]);
    let c = project_counts(
        &row("-home-u-p", &["s1", "s2", "s3", "s4"]),
        &md,
        "pi",
        &NoLivenessOracleYet,
    );

    assert_eq!(
        c.starred,
        Counted::Known(1),
        "★ 这个项目下 s2 是星标的 —— 算得出来就该是 1。\n\
             读到别的值 = 要么没在算（写死），要么 sid 对不上号。"
    );
    assert_eq!(
        c.hidden,
        Counted::Known(2),
        "★ s1 / s3 隐藏 ⇒ 2。这一格与上一格分开写是有意的：\
             两个数用同一份清单算，一起错和分别错要能分辨。"
    );
}

/// ★ `KR83D1` 第 ① 刀的正向：**把清单从那一行摘掉 ⇒ 算不出**（而不是算出 0）。
///
/// 这一条同时是 `KR83D2` 第 ③ 刀的落点，两条判据在这里是**同一格**：
/// 「算不出的那一档」必须与「真的是 0」区分得开。
#[test]
fn a_row_without_the_list_yields_unknown_and_unknown_is_not_zero() {
    let md = metadata_with(&["s1"], &[]);
    let c = project_counts(
        &row_without_ids("-home-u-p", 3),
        &md,
        "pi",
        &NoLivenessOracleYet,
    );

    assert_eq!(
        c.starred,
        Counted::Unknown(WhyUnknown::NoSessionIdList),
        "★ 旧 daemon 不带清单 ⇒ 这三个数是**不知道**"
    );
    assert_ne!(
        c.starred,
        Counted::Known(0),
        "🔴 **本件的全部题面就在这一行断言上。**\n\
             「不知道」退化成 `Known(0)` ⇒ 界面上「查过了，一个星标都没有」与「压根没查」\n\
             重新变成同一个值 —— 那正是 09-12 之前 `fanout_list_projects` 里那三处写死的病。\n\
             ⚠ 只把写死的 `0` 换成另一个写死值也治不了它：那还是一个值装两件事。"
    );
    assert_ne!(c.hidden, Counted::Known(0), "同上，hidden 那一格");
    assert_ne!(c.has_live, Counted::Known(false), "同上，has_live 那一格");
}

/// ★ `KR83D1` 第 ② 刀：**清单是空的、而这个项目下确实有会话 ⇒ 不许当成 0。**
///
/// 「空清单」与「没有会话」是两件事：后者在 daemon 侧**根本不会出这一行**
///（`project_row` 返回 `None`）。所以出了行还空 = 这一行坏了 ⇒ 报「不知道」。
#[test]
fn an_empty_list_on_a_project_that_has_sessions_is_a_broken_row_not_a_zero() {
    let md = metadata_with(&["s1"], &["s1"]);
    let mut broken = row("-home-u-p", &[]);
    broken["sessionCount"] = serde_json::json!(3); // 有 3 个会话，清单却是空的

    let c = project_counts(&broken, &md, "pi", &NoLivenessOracleYet);
    assert_eq!(
        c.starred,
        Counted::Unknown(WhyUnknown::ListDisagreesWithCount),
        "★ 空清单 ≠ 没有 —— 拿它算出 0 就是把一行坏数据当成了真值"
    );
    assert_ne!(
        c.starred,
        Counted::Known(0),
        "🔴 空清单不许退化成「真的是 0」"
    );

    // 少一个也一样（不是只有「全空」才算坏）。⚠ 刻意**不**退化成
    // 「按拿得到的那两个算」：那会给出一个看起来像真值的少数，比明说不知道更糟。
    let mut short = row("-home-u-p", &["s1", "s2"]);
    short["sessionCount"] = serde_json::json!(3);
    assert_eq!(
        project_counts(&short, &md, "pi", &NoLivenessOracleYet).hidden,
        Counted::Unknown(WhyUnknown::ListDisagreesWithCount),
        "★ 短清单同理"
    );
}

/// ★ `KR83D1` 第 ③ 刀的**对照组**：真的把字段改名，判据必须还是绿的。
///
/// 这里逐字模拟「只改字段名、内容等价」：拿生产常量以外的另一个名字造一行，
/// 再用**同一个常量**去读 —— 于是「改名」这件事在判据眼里只是改了一个字符串常量。
#[test]
fn renaming_the_field_does_not_break_the_criterion() {
    let md = metadata_with(&["x1"], &[]);
    // 等价字段：换个名字、内容一字不改。
    let renamed = serde_json::json!({
        "dirName": "p",
        "projectPath": "/home/u/p",
        "sessionCount": 2,
        "lastActivityMs": 1i64,
        "sessionUuids": ["x1", "x2"],
    });
    // 判据读的是**常量指到的那个 key**：把常量指过去，算出来的数一字不变。
    let mut as_if_renamed = renamed.clone();
    as_if_renamed[REMOTE_SESSION_IDS_FIELD] = renamed["sessionUuids"].clone();
    assert_eq!(
        project_counts(&as_if_renamed, &md, "pi", &NoLivenessOracleYet).starred,
        Counted::Known(1),
        "★ 「或等价字段」那个口子要留着：内容等价的一行，算出来的数必须一样。"
    );
}

// ───────────────────────────── `KR83D2` ─────────────────────────────

/// ★★ `KR83D2` 第 ① 刀：**恢复成今天的 `0 / 0 / false` 写死 ⇒ 必须红。**
///
/// 走的是完整那条路（fan-out → 解析 → 算 → 压成线上形状），
/// 所以「把 `starred_count:` 改回字面量 0」在这里当场红。
#[tokio::test]
async fn the_wire_row_carries_the_real_star_and_hide_counts_now() {
    let md = metadata_with(&["s1", "s2"], &["s3"]);
    let lines = vec![row("-home-u-p", &["s1", "s2", "s3"]).to_string()];
    let out = fanout_list_projects(&[cfg("pi")], &md, &NoLivenessOracleYet, |_| {
        let lines = lines.clone();
        async move { Ok(lines) }
    })
    .await
    .expect("一台成功");

    let wire = out.rows[0].0.clone();
    assert_eq!(
        (wire.starred_count, wire.hidden_count),
        (Some(2), Some(1)),
        "★ 09-12 之前这两格是字面量 `0` —— 改回去这一条当场红。\n\
             `K-R92` 起线上那一格是三态：`Some(n)` = 算过了，`None` = 不知道。"
    );
    assert_eq!(wire.session_count, 3);
    assert_eq!(wire.origin.as_deref(), Some("pi"));
}

/// ★★ `KR83D2` 第 ③ 刀 ＋ `KR92D1`：**算不出的那一档，必须与「真的是 0」区分得开，
/// 而且这一次它要一路走到线上那一格。**
///
/// 〔`K-R92` 改写〕上一版最后一条断言逐字写着「线上那一格今天仍是 `false`」，
/// 并把那个有损投影记成「已知的暗账」。**那笔账现在平了** —— 线上那一格是 `Option`。
#[test]
fn liveness_has_no_oracle_here_so_it_says_so_instead_of_saying_false() {
    let md = metadata_with(&[], &[]);
    let c = project_counts(&row("-home-u-p", &["s1"]), &md, "pi", &NoLivenessOracleYet);
    assert_eq!(
        c.has_live,
        Counted::Unknown(WhyUnknown::NoRemoteLivenessOracle),
        "★ 「本机答不了」要说出口"
    );
    assert_ne!(
        c.has_live,
        Counted::Known(false),
        "🔴 `false` 是一句断言：它说「这台机器上这个项目此刻没有活会话」。\n\
             而本机根本没有远端的判活真相源 —— 说不出口的话不许说。"
    );
    assert_eq!(
        c.has_live.known(),
        None,
        "🔴 `KR92D1`：这一维**要过得了线**。上一版这里是 `wire_placeholder()` ⇒ `false`，\n\
             刚算出来的「不知道」在过线那一步被自己丢掉，而丢的那处没有任何东西说话。"
    );
}

/// ★★ `KR92D1` 第 ① 刀 ＋ 第 ③ 刀，**打在整条路上**：一行算不出的数
/// 走完 fan-out 之后，线上那一格拿到的是「不知道」，**不是 `0`/`false`**。
///
/// # 为什么这一条与 `history.rs` 那条不是同一条
///
/// `history.rs::the_three_counts_can_say_i_do_not_know` 判的是**类型装不装得下**
/// 第三态（它直接造一个 `HistoryProject`）；本条判的是**这条路会不会在中途把它压掉** ——
/// 09-12 那半修就正是「类型分得开、过线那一步自己丢掉」。两条缺一条都有一整类漏网。
///
/// 🔴 三格**逐格断**（第 ③ 刀：只修 star/hide 不修 `has_live` ⇒ 必须红）。
#[tokio::test]
async fn an_unknown_count_reaches_the_wire_as_unknown_not_as_zero() {
    let md = metadata_with(&["s1"], &["s1"]);
    // 旧 daemon 那一行（`K-R83` 之前的版本）：没有会话 sid 清单 ⇒ 三个数都算不出。
    let lines = vec![row_without_ids("-home-u-p", 3).to_string()];
    let out = fanout_list_projects(&[cfg("pi")], &md, &NoLivenessOracleYet, |_| {
        let lines = lines.clone();
        async move { Ok(lines) }
    })
    .await
    .expect("一台成功");
    let wire = out.rows[0].0.clone();

    assert_eq!(
        wire.starred_count, None,
        "🔴 本件的题面：这个数**算不出来**，线上那一格却收到了一个具体的数。\n\
             那个数会被人当成「查过了，一个星标都没有」。"
    );
    assert_eq!(wire.hidden_count, None, "🔴 同上，hidden 那一格");
    assert_eq!(
        wire.has_live, None,
        "🔴 第 ③ 刀就钉在这一行：三格是一族，不许挑软的做。\n\
             star/hide 治了而 `has_live` 还压着 ⇒ 本条红。"
    );

    // 对照组：**算得出**的那一行，线上那一格必须是实打实的数（不是「一律 None」）。
    let good = vec![row("-home-u-p", &["s1"]).to_string()];
    let ok = fanout_list_projects(&[cfg("pi")], &md, &NoLivenessOracleYet, |_| {
        let good = good.clone();
        async move { Ok(good) }
    })
    .await
    .expect("一台成功");
    assert_eq!(
        (ok.rows[0].0.starred_count, ok.rows[0].0.hidden_count),
        (Some(1), Some(1)),
        "★ 对照组：算得出就要给出真值 —— 否则本条可以靠「一律说不知道」作弊过关"
    );
}

// ─────────────── `KR92D2`：这条路上「活没活」不许写死 ───────────────

/// ★★ `KR92D2` 第 ② 刀：**给一个真会答话的真相源，这条路端得动真值。**
///
/// 这一条同时把 `project_counts` 的汇总口径钉住：
/// 一个确定活着 ⇒ 整行 `Known(true)`（其余答不答得出都不改这个答案）。
#[test]
fn a_real_oracle_makes_liveness_a_real_value_all_the_way_to_the_wire() {
    let md = metadata_with(&[], &[]);
    let r = row("-home-u-p", &["s1", "s2"]);

    let live = project_counts(&r, &md, "pi", &Oracle(&["s2"]));
    assert_eq!(
        live.has_live,
        Counted::Known(true),
        "★ s2 活着 ⇒ 这一行有活的"
    );
    assert_eq!(live.has_live.known(), Some(true), "★ 真值要过得了线");

    let dead = project_counts(&r, &md, "pi", &Oracle(&[]));
    assert_eq!(
        dead.has_live,
        Counted::Known(false),
        "★ 真相源说两个都没活 ⇒ 这是**查过了的** `false`，与「不知道」不是同一个值"
    );
    assert_eq!(dead.has_live.known(), Some(false));
    assert_ne!(
        dead.has_live.known(),
        project_counts(&r, &md, "pi", &NoLivenessOracleYet)
            .has_live
            .known(),
        "🔴 本条的牙：「查过了，没活」与「没人查过」过线之后必须不同"
    );
}

/// ★★ `KR92D2` 第 ③ 刀：**一行里只要有一个会话答不出，整行就不许说「没有活会话」。**
///
/// 失效方向：把答不出的那几个当成「没活」跳过去 —— 那样整行退化成一个
/// **看起来像真值**的 `false`，比明说不知道更糟（同 `ListDisagreesWithCount` 那一档的道理）。
#[test]
fn one_session_we_cannot_answer_for_makes_the_whole_row_unknown() {
    let md = metadata_with(&[], &[]);
    let c = project_counts(
        &row("-home-u-p", &["s1", "s2"]),
        &md,
        "pi",
        &OracleBlindTo(&["s2"]),
    );
    assert_eq!(
        c.has_live,
        Counted::Unknown(WhyUnknown::NoRemoteLivenessOracle),
        "★ s1 确定没活、s2 答不出 ⇒ 整行是**不知道**"
    );
    assert_ne!(
        c.has_live,
        Counted::Known(false),
        "🔴 退化成 `false` = 拿「查得到的那几个」编出一个整行的断言"
    );
    // 反过来：答不出的那个之外还有一个**确定活着**的 ⇒ 整行确定活着（不确定的不影响）。
    assert_eq!(
        project_counts(
            &row("-home-u-p", &["s1", "s2"]),
            &md,
            "pi",
            &Oracle(&["s1"])
        )
        .has_live,
        Counted::Known(true)
    );
}

/// ★★ `KR92D2` 第 ① 刀：**`stream_remote_history_sessions` 那条路上的 `is_live`
/// 不许写死** —— 恢复 `〔R83c〕` 那处写死当场红。
#[test]
fn the_streamed_remote_entry_does_not_hardcode_its_liveness() {
    let md = metadata_with(&["s1"], &[]);
    let line = serde_json::json!({
        "sessionId": "s1",
        "cwd": "/home/u/p",
        "startedAtMs": 1,
        "updatedAtMs": 2,
        "jsonlPath": "/home/u/.claude/projects/p/s1.jsonl",
        "messageCountApprox": 7,
    })
    .to_string();

    let unknown = remote_session_entry(&line, "p", &md, "pi", &NoLivenessOracleYet)
        .expect("这一行是好的");
    assert_eq!(
        unknown.is_live, None,
        "🔴 09-12 之前这一格是字面量 `false`（住 `remote_history.rs::stream_remote_history_sessions` \
             那个循环里）——\n\
             本机没有远端判活的真相源，说不出口的话不许说。\n\
             ⚠ 本条判的是**性质**（这条路上有没有写死的活状态），不钉那一行的行号。"
    );

    let live =
        remote_session_entry(&line, "p", &md, "pi", &Oracle(&["s1"])).expect("这一行是好的");
    assert_eq!(live.is_live, Some(true), "★ 第 ② 刀：真值要端得动");
    let dead = remote_session_entry(&line, "p", &md, "pi", &Oracle(&[])).expect("这一行是好的");
    assert_eq!(
        dead.is_live,
        Some(false),
        "★ 「查过了，没活」也是一个真值 —— 它与 `None` 是两个不同的答案"
    );
    assert_ne!(
        dead.is_live, unknown.is_live,
        "🔴 第 ③ 刀：答不出时退化成 `false` ⇒ 这一行与「查过了没活」同形，本条红"
    );
    // 顺带钉住：这条路仍旧合本机 metadata（star 是本机的事，不受判活影响）。
    assert!(
        unknown.starred,
        "★ star 按 sid 合本机 metadata，没被本件改坏"
    );
    assert_eq!(unknown.origin.as_deref(), Some("pi"));
}

/// ★ 每一档「不知道」都说得出**为什么** —— 定框 `E4`：静默失败一律给身份。
#[test]
fn every_unknown_can_say_why() {
    for w in [
        WhyUnknown::NoSessionIdList,
        WhyUnknown::ListDisagreesWithCount,
        WhyUnknown::NoRemoteLivenessOracle,
    ] {
        let r = w.reason();
        assert!(!r.is_empty(), "{w:?} 说不出理由");
        assert!(
            !r.contains("unknown") && !r.contains("None"),
            "★ 理由要是**人话**，不是把类型名抄一遍：{r}"
        );
    }
}

/// ★★ **把「不知道」压成线上那个值的地方，现在一处都没有了。**
///
/// 〔`K-R92` 改写；上一版的名字里写着「**只有一处**」，钉的就是那句话
///（那一处是 `Counted` 上那个 `wire_placeholder`，`Unknown` ⇒ `0`/`false`）。
/// `K-R92` 的裁定是**唯一一处也是一处** ⇒ 本条改钉「**零处**」。〕
///
/// 行为判据看不见这条：谁在别处再写一句 `starred_count: Some(0)`，上面那些测试用的夹具
/// 走的是有清单那条路，照样绿。这一条钉的是**代码里有没有把这四格写成常量**。
///
/// ⚠ 本条同时是 `KR92D2` 的**性质**那一半：逐字**不钉那一行的行号**（挪个位置就瞎），
/// 钉的是「这条路上不存在写死的活状态」。
///
/// 对照组自带（F23 那一族本区已犯过三次）：needle 在未剥测试段里比生产段多，
/// 剥不掉就说明 `production_source` 没在起作用、下面在读自己。
#[test]
fn there_is_no_place_left_that_flattens_unknown_into_a_wire_value() {
    let raw = include_str!("../../src/bridge/src/remote_history.rs");
    let prod = guard_core::production_source(raw);
    // 对照组：这个名字只住在测试段里（`K-R92` 的假真相源），生产段必须一个都没有。
    const ONLY_IN_TESTS: &str = "OracleBlindTo";
    assert!(
        raw.matches(ONLY_IN_TESTS).count() > 0 && prod.matches(ONLY_IN_TESTS).count() == 0,
        "★ 对照组：剥掉测试段后 needle 应当归零（现打 raw {} / prod {}）。\n\
             没归零 ⇒ `production_source` 没在起作用，下面那几条是在读自己、恒绿。",
        raw.matches(ONLY_IN_TESTS).count(),
        prod.matches(ONLY_IN_TESTS).count()
    );
    // ⚠ **再剥一层行注释**：`production_source` 剥的是 `#[cfg(test)]` 段，注释照留。
    // 而本条要判的是「**代码里**有没有把这四格写成常量」——
    // 讲这段历史的**散文里必然出现那几个字面量**（上面 `Counted` 的头注就是），
    // 不剥的话判据会被自己的文档喂饱（反过来：为了绕开判据而不敢把历史写清楚，更糟）。
    // 本仓已有先例逐字记着这一族：`readonly_guard` 头注「护栏是子串扫描、**不剥注释**」。
    let code: String = prod
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(
        code.matches("wire_placeholder").count(),
        0,
        "★ 那个压点回来了。`K-R92` 起「不知道」一路走到线上那一格（`Option`），\n\
             不再有任何一处在过线时把它说成 `0`/`false`。"
    );

    // 过线点：`Counted` ⇒ `Option` 只发生在这几处（star / hide / has_live / is_live）。
    // 多一处 = 又开了一个线上面，要说清那一格是什么、谁读它。
    assert_eq!(
        code.matches(".known()").count(),
        4,
        "★ 过线点个数变了。现打生产代码：\n{}",
        code.lines()
            .filter(|l| l.contains(".known()"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // 🔴 这四格**任何一种写死的写法**都不许回来：`0`/`false` 是 09-12 之前的原样，
    // `Some(0)`/`Some(false)` 是同一句谎话换了个类型说一遍（`K-R92` 之后才可能出现的形状）。
    for hardcoded in [
        "starred_count: 0",
        "hidden_count: 0",
        "has_live: false",
        "is_live: false",
        "starred_count: Some(0)",
        "hidden_count: Some(0)",
        "has_live: Some(false)",
        "is_live: Some(false)",
        "has_live: Some(true)",
        "is_live: Some(true)",
    ] {
        assert!(
            !code.contains(hardcoded),
            "🔴 生产代码里又出现了 `{hardcoded}` —— 这条路上没有判活的真相源、\n\
                 也不该把 star/hide 写成常量。写死的活状态是一句没人查过的断言。"
        );
    }
}

// ───────────────────────────── `KR83D3` ─────────────────────────────

/// ★★ `KR83D3`：**算这三个数的路径上，进程 spawn 次数不随项目数增长。**
///
/// 判的是**「一次调用里 spawn 了几次」这个可数的事实** —— 逐字**不判**
/// 「代码里有没有 for 循环」（那判的是写法不是复杂度）。
///
/// 做法：把「去问远端」作为参数传进 [`fanout_list_projects`]，喂一个会计数的假查询。
/// 一旦有人为了拿 star/hide 而在每个项目上补一句 `--list-sessions`，
/// 计数当场从 `台数` 涨成 `台数 + 项目数`，本条红。
#[tokio::test]
async fn the_number_of_remote_execs_does_not_grow_with_the_number_of_projects() {
    let md = metadata_with(&["p7-s0"], &[]);

    async fn run_with(n_projects: usize, md: &HistoryMetadata) -> (usize, usize) {
        let lines: Vec<String> = (0..n_projects)
            .map(|i| row(&format!("p{i}"), &[&format!("p{i}-s0")]).to_string())
            .collect();
        let calls = std::cell::Cell::new(0usize);
        let out = fanout_list_projects(&[cfg("pi")], md, &NoLivenessOracleYet, |_| {
            calls.set(calls.get() + 1);
            let lines = lines.clone();
            async move { Ok(lines) }
        })
        .await
        .expect("一台成功");
        (calls.get(), out.rows.len())
    }

    let (few_calls, few_rows) = run_with(3, &md).await;
    let (many_calls, many_rows) = run_with(200, &md).await;

    assert_eq!((few_rows, many_rows), (3, 200), "夹具本身要真的变多");
    assert_eq!(
        few_calls, 1,
        "★ 1 台 3 个项目 ⇒ **1 次**远端 exec（现打 {few_calls}）"
    );
    assert_eq!(
        many_calls, 1,
        "🔴 1 台 200 个项目 ⇒ 仍然**1 次**（现打 {many_calls}）。\n\
             变成 201 = 有人给每个项目补了一次 `--list-sessions` ——\n\
             那是 N 次进程 spawn，而这是用户常开的界面（失效方向逐字记在\n\
             `local_read_surface_registry.rs` 那条退役条件里）。"
    );
}

/// ★ 多台时的口径：spawn 次数 = **台数**，与项目数无关（不是「恒为 1」）。
#[tokio::test]
async fn the_number_of_remote_execs_equals_the_number_of_hosts() {
    let md = metadata_with(&[], &[]);
    let calls = std::cell::Cell::new(0usize);
    let hosts = [cfg("pi"), cfg("nas"), cfg("box")];
    let out = fanout_list_projects(&hosts, &md, &NoLivenessOracleYet, |_| {
        calls.set(calls.get() + 1);
        async move {
            Ok((0..50)
                .map(|i| row(&format!("p{i}"), &[&format!("p{i}-s0")]).to_string())
                .collect())
        }
    })
    .await
    .expect("三台都成功");
    assert_eq!(out.rows.len(), 150, "3 台 × 50 个项目");
    assert_eq!(
        calls.get(),
        hosts.len(),
        "★ 3 台 ⇒ 3 次；150 个项目一次都不额外加"
    );
}

/// 逐台失败仍旧隔离（F76 的老性质，本件重构了这条路 ⇒ 顺手钉住没被改坏）。
#[tokio::test]
async fn a_failing_host_is_still_skipped_and_reported_not_fatal() {
    let md = metadata_with(&[], &[]);
    let out =
        fanout_list_projects(&[cfg("good"), cfg("bad")], &md, &NoLivenessOracleYet, |c| {
            let bad = c.origin_label() == "bad";
            async move {
                if bad {
                    Err("连不上".to_string())
                } else {
                    Ok(vec![row("p", &["s1"]).to_string()])
                }
            }
        })
        .await
        .expect("有一台成功 ⇒ 整体 Ok");
    assert_eq!(out.failed_hosts, vec!["bad".to_string()]);
    assert_eq!(out.rows.len(), 1);
}
