/// ★ P8c：**收帧那条路真的记了观测分类**，且只记变化、只作观测。
///
/// 钉源码形状而不是跑一条真流：那条臂住在需要真远端连接的 `async fn` 里
///（`classify_tmux_observation` 当年被提成纯函数正是因为这个）。
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
/// 帧级套件 `graylight-daemon-frames` 同期 **12 过 / 0 败**（daemon 那侧发得对），
/// 前端 `建卡 rendered=0`（一条都没收到）⇒ 断点就在这两者之间，而这里是那段路上的分叉点。
///
/// ⚠ **位置性质**：日志要排在 `app.emit` **之前**。排在后面的话，emit 那一跳若卡住/panic，
/// 就连「收到了」这件事都没留下 —— 而那正是最需要知道的一格。
#[test]
fn the_session_added_arm_is_not_silent() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
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
        guard_core::find_pinned(
            window,
            "app.emit(crate::bridge::events::REMOTE_SESSION_ADDED, &payload)"
        )
        .is_ok(),
        "那行日志与它要守的那次 `app.emit` 之间隔太远（或被排到了后面）——\
             排在 emit 后面的话，emit 卡住时就连「收到了」都没留下"
    );
}

/// ★ `P0b-Y2` 第十七拍：摘的必须是**那一格**，不许误伤前缀相同的邻居。
#[test]
fn remove_tmux_line_takes_out_exactly_that_session() {
    // `tmux ls` 的形状：6 列、制表符分隔、以 \n 结尾。
    let raw = "cc-a\t/x\tclaude\t0\t1\tsid-a\ncc-ab\t/y\tbash\t0\t1\tsid-ab\n";
    let out = super::remove_tmux_line(raw, "cc-a");
    assert!(
        !out.lines().any(|l| l.split('\t').next() == Some("cc-a")),
        "该摘的那一格还在：{out:?}"
    );
    // ⚠ **不许误伤前缀相同的邻居** —— `cc-a` 与 `cc-ab` 是两个会话。
    assert!(
        out.lines().any(|l| l.split('\t').next() == Some("cc-ab")),
        "把前缀相同的邻居一起摘掉了：{out:?}"
    );
    assert!(out.ends_with('\n'), "尾部换行没保住：{out:?}");
    // 摘不存在的名字 = 原样返回（幂等：同一 sid 两条路都可能到）。
    assert_eq!(super::remove_tmux_line(raw, "cc-zzz"), raw);
}

/// ★ `P0b-Y2` 第十七拍：**先摘账、再 retire** 的顺序不许反。
///
/// 反了的话下游 `classify_removed` 查到的还是「tmux 还在」⇒ 判灰不判归档
/// ⇒ **永久灰点**（`#60` 现象 2）。这条钉的是**位置**，不是「那行代码在」。
///
/// ⚠ 针要钉**当下的形状**：首版钉 `*raw = remove_tmux_line(...)`（内联 `get_mut` 那版），
/// 而实现随后改成走 `record_tmux_raw` ⇒ 针过期，两条变异**双双存活**而判据照绿。
/// ★「判据的针跟不上实现」与「判据钉得太小」是同一族的两面。
#[test]
fn the_ledger_is_pruned_before_the_retire_is_sent() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let at = guard_core::find_pinned(
        &prod,
        "record_tmux_raw(&host_label, remove_tmux_line(raw, &name))",
    )
    .expect("会话关闭那一臂必须先把这一格从 tmux 账本里摘掉（且走既有写口）");
    let send_at = prod[at..]
        .find("removed: vec![RemovedSid::gone(sid)]")
        .expect("找不到那次 retire —— 抽取面画错了");
    assert!(
        send_at < 900,
        "摘账与 retire 隔了 {send_at} 字符 —— 中间插了别的东西？\n             \
             这两件事必须紧挨着且**摘账在前**，否则下游查到的还是「tmux 还在」。"
    );
}

/// ★ 与上一条**成对**：`SessionRemoved` 那一臂也不许静默〔`P0b-Y2` 第十拍 08-13〕。
///
/// # 为什么成对才有用
///
/// `#60` 问的是**灰灯**，而灰灯是「死亡」这件事的 UI 表现。只给 `added` 加日志，
/// 全链失败时能回答「上线那跳到没到」，**答不出「死亡那跳到没到」** ——
/// 08-13 实测当场撞上：daemon 侧 tap 里明明有 `session_removed`，monitor 日志里
/// 一个字都没有，于是「收到了没转发」与「根本没收到」分不开。补上这一行才分得开
/// （补完立刻量到：`cause=Gone`，两跳都通）。
///
/// ⚠ 差点漏掉 `#[test]`：搬动代码块时属性留在了上一条身上，`cargo test` **一声不吭地
/// 少跑一条**（`4 passed` 里没有它）。⇒ 加判据之后要核**名字出现在跑的清单里**，
/// 不是只看「全绿」——本仓「0 passed 不是绿」的同一族。
#[test]
fn the_session_removed_arm_is_not_silent() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let at = guard_core::find_pinned(&prod, "session-removed: [{host_label}] sid={sid}")
        .expect(
            "`SessionRemoved` 那一臂必须留下一行「收到了」——`#60`（灰灯不出现）问的正是\n             \
                 「死亡这件事走到哪一步丢了」，而 08-13 全链实测时 daemon 侧 tap 里明明有\n             \
                 `session_removed`、monitor 日志里一个字都没有 ⇒ 分不清「收到了没转发」与「根本没收到」。",
        );
    // `cause` 必须一起打：灰灯与归档走**不同的 cause**（`Superseded` 直接归档，
    // `Gone` 才是灰灯那条）。少了它，看见一行「removed」仍答不出 UI 该变成什么样。
    // ⚠ 窗口内也**不许裸 `contains`**：`needle_anchor_registry` 那条递减棘轮当场拦下
    //   第一版（与 added 那条判据 08-13 早些时候踩的是同一个坑，头注里逐字记着）。
    let window = &prod[at..(at + 200).min(prod.len())];
    assert!(
        guard_core::find_pinned(window, "cause={cause:?}").is_ok(),
        "那行日志没带 `cause` —— 灰灯与归档是两条路，只报 sid 分不出该走哪条。"
    );
    // 与 added 那条同样的时序要求：日志排在转发**之前**。
    let fwd = &prod[at..(at + 700).min(prod.len())];
    assert!(
        guard_core::find_pinned(fwd, "session_changes.send(SessionChange {").is_ok(),
        "那行日志与它要守的那次转发之间隔太远（或被排到了后面）——\
             排在后面的话，转发卡住时就连「收到了」都没留下"
    );
}

#[test]
fn the_frame_arm_logs_the_observation_kind() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let log_at =
        guard_core::find_pinned(&prod, "\"tmux-observation: [{host_label}] {} → {kind}\"")
            .unwrap_or_else(|e| {
                panic!(
                "{e}\n收 `TmuxSessions` 帧时不再记观测分类 —— `U3` 裁定的那一行可观测性没了，\n\
                     而它存在的全部理由是：不记就分不清『0 次』与『记不下来』。"
            )
            });
    // 只记**变化** —— 帧由 tmux hook 驱动，逐帧记会把日志淹掉（淹掉的日志与没有一样不可读）。
    let gate_at =
        guard_core::find_pinned(&prod, "if last_observation_kind.as_deref() != Some(kind) {")
            .unwrap_or_else(|e| panic!("{e}\n变成逐帧记了 —— 那会把日志淹掉"));
    assert!(gate_at < log_at, "那条日志不在「变了才记」的门里面");
    // 它是**纯观测**：决策仍只看 `classify_tmux_observation` 的结果（`verdict`）。
    let decide_at = guard_core::find_pinned(
        &prod,
        "if let crate::tmux::TmuxObservation::Backend(backend) = verdict {",
    )
    .unwrap_or_else(|e| panic!("{e}\n对账不再直接吃分类结果 —— 观测与决策的界线糊了"));
    assert!(
        log_at < decide_at,
        "记日志排在决策**之后** —— 那样决策路径提前 return 时这一维就丢了"
    );
}

use super::{read_capped_line, CappedLine};
use tokio::io::BufReader;

/// 正常一行：内容不含行尾 `\n`。
#[tokio::test]
async fn a_normal_line_comes_back_without_its_newline() {
    let data: &[u8] = b"hello\n";
    let mut rd = BufReader::new(data);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, 64).await.unwrap(),
        CappedLine::Line
    ));
    assert_eq!(buf, b"hello");
}

/// **恰好等于上限**的一行必须过 —— 边界差一格就是「合法数据被丢」。
#[tokio::test]
async fn a_line_exactly_at_the_cap_is_still_accepted() {
    let line = vec![b'x'; 16];
    let mut data = line.clone();
    data.push(b'\n');
    let mut rd = BufReader::new(&data[..]);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, 16).await.unwrap(),
        CappedLine::Line
    ));
    assert_eq!(buf.len(), 16);
}

/// 超一个字节就该丢，且**报出真实字节数**。
#[tokio::test]
async fn one_byte_over_the_cap_is_dropped_and_counted() {
    let mut data = vec![b'x'; 17];
    data.push(b'\n');
    let mut rd = BufReader::new(&data[..]);
    let mut buf = Vec::new();
    match read_capped_line(&mut rd, &mut buf, 16).await.unwrap() {
        CappedLine::TooLong(n) => assert_eq!(n, 17, "报出的字节数要是真实长度"),
        other => panic!("该判超限，实得 {other:?}"),
    }
    assert!(buf.is_empty(), "超限的行不许留在 buf 里被当数据用");
}

/// ★ **超限之后不许继续往 buf 里塞字节**（daemon 那次 518 MiB 的正题）。
///
/// 喂一条 100_000 字节的行、上限 16。若机制是「读完再判」，
/// `buf` 的容量会涨到 10 万量级；正确实现下它应当在超限那一刻就被清掉并归还。
#[tokio::test]
async fn over_limit_stops_growing_the_buffer() {
    let mut data = vec![b'x'; 100_000];
    data.push(b'\n');
    let mut rd = BufReader::new(&data[..]);
    let mut buf = Vec::new();
    match read_capped_line(&mut rd, &mut buf, 16).await.unwrap() {
        CappedLine::TooLong(n) => assert_eq!(n, 100_000),
        other => panic!("该判超限，实得 {other:?}"),
    }
    assert!(
        buf.capacity() < 1024,
        "超限之后 buf 容量涨到了 {} —— 说明字节还在往里塞（那正是 daemon 侧 518 MiB 的形状）",
        buf.capacity()
    );
}

/// ★ **超限之后的下一行必须还读得到** —— 丢一行不许连带丢掉整条流。
///
/// ⚠ 超长那一行**必须跨多个 `fill_buf` 块**（这里 20_000 > `BufReader` 默认 8 KiB）。
/// 第一版用 50 字节，一次 `fill_buf` 就连着 `\n` 一起读完了 ——
/// 于是「超限之后继续找换行」那条**续读路径压根没被走到**：
/// 实测把机制改成「超限就立刻返回」，本条照样绿，只有 100_000 那条抓住了。
/// ⇒ 短数据让本条退化成了 `one_byte_over_the_cap_is_dropped_and_counted` 的副本。
#[tokio::test]
async fn the_line_after_an_over_long_one_is_still_delivered() {
    let mut data = vec![b'x'; 20_000];
    data.push(b'\n');
    data.extend_from_slice(b"after\n");
    let mut rd = BufReader::new(&data[..]);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, 16).await.unwrap(),
        CappedLine::TooLong(20_000)
    ));
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, 16).await.unwrap(),
        CappedLine::Line
    ));
    assert_eq!(buf, b"after");
}

/// 流尽而无残行 ⇒ `Eof`；有残行（末尾没 `\n`）⇒ 当一行交出去（`read_line` 旧行为）。
#[tokio::test]
async fn eof_and_trailing_partial_keep_the_old_read_line_behaviour() {
    let empty: &[u8] = b"";
    let mut rd = BufReader::new(empty);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, 16).await.unwrap(),
        CappedLine::Eof
    ));

    let partial: &[u8] = b"tail";
    let mut rd = BufReader::new(partial);
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, 16).await.unwrap(),
        CappedLine::Line
    ));
    assert_eq!(buf, b"tail");
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, 16).await.unwrap(),
        CappedLine::Eof
    ));
}

/// 空行（连着两个 `\n`）不该被当成 EOF。
#[tokio::test]
async fn an_empty_line_is_a_line_not_an_eof() {
    let data: &[u8] = b"\nx\n";
    let mut rd = BufReader::new(data);
    let mut buf = Vec::new();
    assert!(matches!(
        read_capped_line(&mut rd, &mut buf, 16).await.unwrap(),
        CappedLine::Line
    ));
    assert!(buf.is_empty());
}
