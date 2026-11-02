/// ★ P8c：**收帧那条路真的记了观测分类**，且只记变化、只作观测。
///
/// 钉源码形状而不是跑一条真流：那条臂住在需要真远端连接的 `async fn` 里。
///
/// ⚠ 全程用 `find_pinned`（恰好一处 + 两侧有边界），**不用裸 `contains`** ——
/// `needle_anchor_registry` 是条**递减棘轮**，它逐字写着「不许把上限调上去让今天好过」。
/// `P0b`：`SessionAdded` 那一跳**不许再是静默的**。
///
/// # 它为什么值得一条判据
///
/// 08-13 全链台架终于可信（连续两跑、四格全绿、读数一致），报的是
/// 「30s 内未见灰灯 tab-state」。而当时日志**回答不了最基本的那一问：帧到 monitor 了吗？**
/// —— 因为这一臂只有 emit **失败**才打日志，成功一个字不留。
/// 帧级套件 `graylight-backend-frames` 同期 **12 过 / 0 败**（backend 那侧发得对），
/// 前端 `建卡 rendered=0`（一条都没收到）⇒ 断点就在这两者之间，而这里是那段路上的分叉点。
///
/// ⚠ **位置性质**：日志要排在 `app.emit` **之前**。排在后面的话，emit 那一跳若卡住/panic，
/// 就连「收到了」这件事都没留下 —— 而那正是最需要知道的一格。
#[test]
fn the_session_added_arm_is_not_silent() {
    let prod = guard_core::addr::stream_source_production();
    let at = guard_core::find_pinned(&prod, "session-added: [{host_label}] sid={sid}")
        .expect("`SessionAdded` 那一臂必须留下一行「收到了」——否则全链失败时问不出帧到没到");
    // ⚠ **改成局部窗口，不求全局唯一**：那句 emit 生产段有 2 处（另一处是**重宣告**那条路），
    //   `find_pinned` 当场拒收并逐字提醒「把 needle 扩到能唯一确定那个事实的大小」。
    //   而本条要钉的性质本来就是**局部**的（「这行日志的紧后面就是那次 emit」）
    //   ⇒ 用窗口比硬造一个全局唯一的针更贴事实。
    // ⚠ 窗口 400 字符是启发式：够覆盖日志与 emit 之间那几行，又不至于跨到别的臂。
    // ⚠⚠ 窗口内也**不许裸 `contains`** —— `needle_anchor_registry` 那条递减棘轮当场拦下了
    //   第一版（它逐字：「不许把上限调上去让今天好过」）。⇒ 在窗口这个**小语料**上
    //   仍走 `find_pinned`：恰好一处 + 两侧有边界。窗口小到只含本臂 ⇒ 唯一性天然成立。
    let window = &prod[at..(at + 400).min(prod.len())];
    assert!(
        guard_core::find_pinned(window, "told(at, Frame::SessionAdded(meta));").is_ok(),
        "那行日志与它要守的那次 `app.emit` 之间隔太远（或被排到了后面）——\
             排在 emit 后面的话，emit 卡住时就连「收到了」都没留下"
    );
}

// monitor 不存 tmux 原文；收割在后端会话账本里（`tests/backend/observe/session_ledger_tests.rs` 的「tmux 会话关了当场已结束」）。

/// ★ 与上一条**成对**：离开那一帧（`SessionLeft`）那一臂也不许静默。
///
/// # 为什么成对才有用
///
/// `#60` 问的是**灰灯**，而灰灯是「死亡」这件事的 UI 表现。只给 `added` 加日志，
/// 全链失败时能回答「上线那跳到没到」，**答不出「死亡那跳到没到」** ——
/// 08-13 实测当场撞上：backend 侧 tap 里明明有 `session_removed`，monitor 日志里
/// 一个字都没有，于是「收到了没转发」与「根本没收到」分不开。补上这一行才分得开
/// （补完立刻量到：`cause=Gone`，两跳都通）。
///
/// ⚠ 差点漏掉 `#[test]`：搬动代码块时属性留在了上一条身上，`cargo test` **一声不吭地
/// 少跑一条**（`4 passed` 里没有它）。⇒ 加判据之后要核**名字出现在跑的清单里**，
/// 不是只看「全绿」——本仓「0 passed 不是绿」的同一族。
#[test]
fn the_session_left_arm_is_not_silent() {
    // 「死亡那跳到没到」只一帧：`session_left`（内容流收口 ＋ 后端裁的去向在 `face.life`）。
    //   那一行排在转交**之前**（转交卡住时也留下「收到了、去向是什么」）。
    let prod = guard_core::addr::stream_source_production();
    let at = guard_core::find_pinned(&prod, "session-left: [{host_label}] sid={sid} → {:?}").expect(
        "离开那一臂必须留下一行「收到了、去向是什么」——`#60`（灰灯不出现）问的正是「死亡这件事走到哪一步丢了」。",
    );
    let fwd = &prod[at..(at + 400).min(prod.len())];
    assert!(
        guard_core::find_pinned(fwd, "told(at, Frame::SessionLeft(s));").is_ok(),
        "那行日志与它要守的那次转交之间隔太远（或被排到了后面）"
    );
}

// 「收帧那条路记观测分类」那条（P8c）随收割搬进后端：日志今天住 `src/backend/observe/session_ledger.rs::SessionLedger::observe`
//   （`tmux-observation: … → {kind}`，只记变化）。

// 有界读行的行为对拍（超限 · 内存不涨 · 超长之后照读下一行 · 残行与空行）住 `tests/comms/inward/line_tests.rs`，与读行同一个包。

// ── P28：给这条源码扫描型守卫立**负对照** ──
//
// 判的不是产品性质，是「**剥法没把我要扫的那一段剥掉**」。
// 失效形状是现打过的：便宜近似 `src.split("\n#[cfg(test)]").next()` 只在
// 「第一个测试模块之后再没有生产代码」时才对。`stream_source/` 今天 4490 行，
// 第一个测试模块在 **909** 行 ⇒ 那个近似把扫描面砍到前 908 行，
// 而本文件要扫的东西全在它**后面**（逐针行号写在下面）。
// ⇒ 扫描面一旦静默缩水，本文件的判据会**零命中地绿**。
//
// 原语与它买不到什么：`guard_core::assert_stripper_keeps` 的头注。
// 一句话：它不买「针还是那个针」—— 下面这张表必须从本文件真正用的针里抄。

/// ★ 扫描面自检：共享剥法留住了本文件要扫的那几段，而便宜近似留不住。
#[test]
fn the_shared_stripper_keeps_the_arms_this_guard_must_scan() {
    // 三条臂的日志/账本针，今天分别在 3515 / 3605 / 3684 行 —— 都在 909 之后。
    // ⚠ 刻意用**整句**日志格式串而不是 `session-added`：本文件的判据本来就走
    //    `find_pinned`（恰好一处 + 两侧有边界），针的大小要和它一致。
    guard_core::assert_stripper_keeps(
        "stream_source::capped_line_tests",
        &guard_core::addr::stream_source_raw(),
        &[
            "session-added: [{host_label}] sid={sid}",
            "session-left: [{host_label}] sid={sid} → {:?}",
        ],
    );
}
