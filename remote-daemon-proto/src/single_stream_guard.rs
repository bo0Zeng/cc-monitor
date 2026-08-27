//! `K-P1 KPY8`：**「多客户端的流」本件明确不做 —— 留一个触发器，不留一句话。**
//!
//! # 它是什么形状
//!
//! **今天绿、那天故意红**（形状抄 `wire::tests::production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen`
//! 的那一条：「它会在那天**故意变红**，那是提醒，不是障碍」）。
//!
//! 「写成『以后记得』= **没有触发器**」这一族本仓治过一次，逐字住
//! `src-tauri/src/backend/mod.rs:22-26`：「那是个**没有触发器的等待**…
//! **别再把这句读成「等某个人想起来」**」。⇒ 本件明确不做的那一半必须有东西钉着。
//!
//! # 它钉的是哪三处「恰好一个客户端」
//!
//! `K-P1 §0b-2㈢` 逐条点了名，本模块把它们做成**带次数的锚点**（`brief` 第 7 条：
//! 每一刀先断言锚点恰好命中 N 次）：
//!
//! 1. **`HelloFlushed` 是类型级见证** —— 换载体那一格是 drop-in（两边本来就是泛型），
//!    但**多连接那天要么每连接一份见证，要么把见证降级 —— 降级是放宽，不许**。
//!    那一格由 `wire::tests::the_hello_witness_has_exactly_one_way_to_exist` 钉着，
//!    本模块**不重复钉它**，只在这里指住它的住址。
//! 2. **观测帧只有一个消费者** —— `observe::watcher::spawn` 返回**一个**
//!    `mpsc::Receiver<Frame>`。N 个连接要 fan-out，而 `Overflow.lost` 的丢帧账是
//!    **按那一个通道**记的（`wire.rs` 的 `LostFrame` 头注逐字：状态增量帧
//!    「是一次差分的结果，**别处不存在**」）⇒ 分流之后「丢了哪几帧」这本账要**重新定义**。
//!    ★ **那是语义变更，不是搬运。**
//! 3. **`writer_task` 的让位预算是「两条通道对一个 writer」** —— `REPLY_BURST = 8`，
//!    那个数是 D 审计实测「500ms 内应答 70789 条 / 实时行 4 条」之后加的。
//!    每连接一个 writer 之后，这条预算要**按连接算**。
//!
//! # 它**挡不住**什么（说清楚，别让人以为它是证明）
//!
//! 它认的是下面那张表里的**字面锚点与次数**。有人把 fan-out 写成一个新模块、
//! 而这几处字面一个都不动 ⇒ 本条照样绿。⇒ 它买的是「**动到这三处时有人会被拦一下**」，
//! 不是「fan-out 不可能被实现」。这一句是照 `relay/bind_guard.rs` 头注那条订正写的：
//! 那条守卫**只认 4 个拼写，别的一律看不见**，而它先前把自己写得比这更强。
//!
//! ⚠ **本模块刻意与被扫的代码不同住一个文件**（`relay/bind_guard.rs` 头注逐字给的理由：
//! 判据在自己的登记表 / 注释 / 常量里找到自己 ⇒ 恒绿）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;

    /// `(文件, 锚点, 恰好几处, 它一变就意味着什么)`。**次数钉死**（`KP4` 第四轮买牙的两个价钱之一）。
    ///
    /// ⚠ 加一行 / 改一个数之前先答：**这是不是在做「多客户端的流」**？
    /// 是 ⇒ 那件事本件明确不做（`K-P1 §2`），要做请先立件，把 `Overflow.lost`
    /// 那本账重新定义清楚；不是 ⇒ 把新的数与理由一起写进来。
    const PINS: &[(&str, &str, usize, &str)] = &[
        (
            "main.rs",
            "writer_task(",
            2,
            "出方向的写者**一条载体一个**：stdio 那条 + 常驻那条。\
             第三处意味着同一个进程里同时有两条流在写 —— 那正是 fan-out 的入口，\
             而两条流各自记各自的 `Overflow.lost` 之后，那本账就不再是「这台机丢了几帧」。",
        ),
        (
            "main.rs",
            "inbound::spawn(",
            2,
            "入方向 reader 同样**一条载体一个**。多一处 = 多一个能发 `launch`/`kill` 的对端，\
             而 `inbound::REGISTRY` 的取消登记表是**一份**（按 id 去重），两个对端会撞 id。",
        ),
        (
            "main.rs",
            "busy.swap(true",
            1,
            "「谁拿到那一条流」由**一次原子操作**决出来，全 crate 只此一处。\
             改成「先查后写」就有窗口（两条连接同时握手都拿到流）；\
             多一处则意味着有第二个地方在发牌。",
        ),
        (
            "observe/watcher.rs",
            "mpsc::Receiver<Frame>",
            1,
            "观测帧**只有一个消费者**。第二个 `Receiver` = fan-out 落地，\
             而 `Overflow.lost` 的丢帧账是按那**一个**通道记的 ⇒ 语义变更，不是搬运。",
        ),
        (
            "listen.rs",
            "Admit::Stream",
            1,
            "「交出流」这一档全 crate 只此一处产出。多一处 = 有第二条路能绕过 `admit` 拿到流。",
        ),
        (
            "main.rs",
            "REPLY_BURST",
            2,
            "让位预算是「两条通道对**一个** writer」（常量 1 处 + 比较 1 处）。\
             每连接一个 writer 之后这条预算要按连接算 —— 那时本行的数会变，正好逼人回来重判。",
        ),
    ];

    fn source_of(rel: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(rel);
        production_code(&std::fs::read_to_string(&p).unwrap_or_else(|e| {
            panic!("读不到 {} —— 文件搬家了就来改本表：{e}", p.display())
        }))
    }

    /// ★ 正题：三处「恰好一个客户端」的锚点**逐个按次数**对上。
    #[test]
    fn the_single_stream_shape_is_still_exactly_one_client() {
        for (file, needle, want, why) in PINS {
            let src = source_of(file);
            // 反空真：切出来的必须是真代码，不是一个空串。
            assert!(
                src.len() > 500,
                "{file} 的生产段只有 {} 字节 —— 抽取坏了，本条此刻在空转",
                src.len()
            );
            let got = src.matches(needle).count();
            assert_eq!(
                got, *want,
                "\n`{file}` 里 `{needle}` 有 {got} 处，登记的是 {want} 处。\n\
                 登记说法：{why}\n\
                 ★ **这多半不是 bug，是提醒**：`K-P1 §2` 明确不做「多客户端的流」——\n\
                 fan-out 与 `Overflow.lost` 丢帧账的重新定义留给**第二个要「流」的客户端**出现那天。\n\
                 真到了那天：先立件，把那本账重新定义清楚，再回来改这张表；\n\
                 只是重构动到了这几处：把新的数与理由一起写进来。"
            );
        }
    }

    /// ★ 反向锚点：**每个锚点今天都真的找得到**。
    ///
    /// 少了这条，把 `PINS` 里某一行的 `want` 改成 0 就能让它「绿着通过」——
    /// 而 0 处恰恰是「抽取器坏了」与「代码被删了」共同的样子。
    #[test]
    fn every_pin_is_a_live_anchor_not_a_zero() {
        for (file, needle, want, _) in PINS {
            assert!(
                *want > 0,
                "`{file}` / `{needle}` 登记成 0 处 —— 零命中的锚点钉不住任何东西"
            );
            assert!(
                source_of(file).contains(needle),
                "`{file}` 里找不到锚点 `{needle}` —— 锚点腐烂了，本表在假绿"
            );
        }
    }

    /// ★ 那条**不由本模块钉**的（`HelloFlushed` 见证）要指得住住址。
    ///
    /// `§0b-2㈢`-1 逐字：多连接那天「要么每连接一份见证，**要么把见证降级 —— 降级是放宽，不许**」。
    /// 那一格的判据住 `wire.rs`；本条只保证**那条判据还在**，不重复实现它
    /// （两份实现迟早分叉 —— 本仓最贵的那一族）。
    #[test]
    fn the_hello_witness_pin_still_lives_where_this_module_says_it_does() {
        let wire = include_str!("wire.rs");
        assert!(
            wire.contains("fn the_hello_witness_has_exactly_one_way_to_exist"),
            "本模块头注指着 `wire.rs` 的那条见证判据，而它已经不在了 ——\n\
             ⇒ 「hello 先于 reader」在换了载体之后就只剩一句散文。"
        );
    }
}
