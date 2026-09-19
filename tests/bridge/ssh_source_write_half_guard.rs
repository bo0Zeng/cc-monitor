use crate::structural_scan::ScanReport;

/// 生产段。**用共享的 `guard_core::production_code`，不自己写便宜近似。**
///
/// 这不是洁癖：本文件第一个 `#[cfg(test)]` 模块在 800 行附近，而本护栏要扫的
/// `parse_frame` / `stream_loop` / `probe_daemon` 全在它**后面**。
/// monitor 侧此前流行的那个近似（`split("\n#[cfg(test)]").next()`）会把扫描面
/// 砍掉三分之二 —— 下面第一条测试把这个差距**实测**出来，免得它变成一句口号。
fn prod() -> String {
    let p = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    guard_core::assert_no_test_code("ssh_source.rs", &p);
    p
}

/// ★ 扫描面自检：共享剥法留住了要扫的部分，而便宜近似留不住。
#[test]
fn the_shared_stripper_keeps_the_part_this_guard_must_scan() {
    let me = include_str!("../../src/bridge/src/ssh_source.rs");
    let cheap = me.split("\n#[cfg(test)]").next().unwrap_or(me);
    let good = prod();
    for anchor in [
        "fn parse_frame",
        "async fn stream_loop",
        "async fn probe_daemon",
    ] {
        assert!(
            good.contains(anchor),
            "共享剥法把 `{anchor}` 剥掉了 —— 本护栏此刻扫不到它"
        );
        assert!(
            !cheap.contains(anchor),
            "便宜近似居然也留住了 `{anchor}` —— 这条对照失去意义，重新确认文件结构"
        );
    }
    assert!(
        good.len() > cheap.len() * 2,
        "共享剥法只留了 {} 字节，便宜近似 {} 字节 —— 差距没有预期那么大，检查剥法",
        good.len(),
        cheap.len()
    );
}

/// ★ 本文件**自己不许切流** —— 切与停必须由 `inbound_client::split_and_park` 一步做完。
///
/// # 判据形状换过一次（D 审计打的）
///
/// 上一版是「每处 `tokio::io::split(` 后面 240 字符内要出现 `park`」。审计用两种**普通写法**
/// 绕过，两次全绿：
/// - 在那一行加一句**尾随注释**提 `park`，写半边交给别的函数
///   （`guard_core::production_code` 只剥**行首**注释，行尾的留在扫描面里）；
/// - `split()` 之后先 `w.write(b"early\n").await` 再 `park(w)` —— 那就是一次
///   **Hello 之前的写**，而窗口判据看不见。
///
/// 处置按第 6 条纪律：不往判据上加正则，让违规不可表示。切分入口收成一个函数之后，
/// 这条判据变成**零命中型** —— 尾随注释再怎么写都改变不了「这里出现了 `tokio::io::split(`」。
#[test]
fn ssh_source_never_splits_a_stream_itself() {
    // 运行时拼，避免命中本行自己。
    //
    // ⚠⚠ **08-07：原来只有这一个 needle，而它只认「一种切法」。**
    // `TcpStream` 自带 `into_split()` / `split()`，那才是更常用的写法 ——
    // 实测往生产段加一处 `stream.into_split()`，全仓 **976 条判据一条不红**。
    // ⇒ 与上一件（写流判据漏 `tokio::io::copy`）同一族：
    // **动作类判据锚在「这个动作长什么样」上，就会漏掉别的做法。**
    //
    // 补两路，两路都不是「枚举拼写」：
    // · **切法**是 tokio 的封闭上游集合（自由函数 + `TcpStream` 的两个方法）；
    // · **持有物**才是这条判据真正关心的东西 —— 它的诊断自己写着
    //   「中间留 `WriteHalf` 就等于留了一个『Hello 之前能写』的窗口」。
    //   类型名这一路能接住「用 `let (r, w) = …` 推导、一个切法名都不写」的情形。
    let split_apis: Vec<String> = vec![
        format!("tokio::io::spl{}", "it("),
        format!(".into_spl{}", "it("),
        format!("TcpStream::spl{}", "it("),
    ];
    let half_types: Vec<String> = vec![
        format!("Owned{}Half", "Write"),
        format!("Owned{}Half", "Read"),
    ];
    let prod = prod();
    // 匹配器自检：**独立手写**的样本，不用 needle 自己拼。
    for (sample, why) in [
        ("let (r, w) = tokio::io::split(stream);", "自由函数切法"),
        (
            "let (r, w) = stream.into_split();",
            "TcpStream 的 owned 切法",
        ),
        (
            "let (r, w) = tokio::net::TcpStream::split(&mut s);",
            "TcpStream 的借用切法",
        ),
    ] {
        assert!(
            split_apis.iter().any(|n| sample.contains(n.as_str())),
            "切法匹配器漏了「{why}」：{sample:?} —— 那条路照旧能切出 WriteHalf"
        );
    }
    for (sample, why) in [
        ("fn f(w: OwnedWriteHalf) {}", "持有 owned 写半边"),
        ("let r: OwnedReadHalf = x;", "持有 owned 读半边"),
    ] {
        assert!(
            half_types.iter().any(|n| sample.contains(n.as_str())),
            "持有物匹配器漏了「{why}」：{sample:?}"
        );
    }
    let mut split_hits: Vec<String> = split_apis
        .iter()
        .chain(half_types.iter())
        .filter(|n| prod.contains(n.as_str()))
        .cloned()
        .collect();
    split_hits.dedup();
    assert!(
        split_hits.is_empty(),
        "ssh_source 的生产段自己切流 / 自己持有流的一半了（{split_hits:?}）。\n\
             切分与停放必须是同一步（`inbound_client::split_and_park`）—— 中间留一个 \
             写半边就等于留了一个「Hello 之前能写」的窗口，而那正是那一步要消掉的东西。\n\
             ⚠ 08-07 起本条同时认**切法**与**持有物**：只堵一种切法挡不住 `into_split()`。"
    );
    assert!(
        !prod.contains(split_apis[0].as_str()),
        "ssh_source 的生产段自己切流了。\n\
             切分与停放必须是同一步（`inbound_client::split_and_park`）—— 中间留一个裸\n\
             `WriteHalf` 就等于留了一个「Hello 之前能写」的窗口，而那正是本轮要消灭的东西。"
    );
    // 反面：切分入口必须真的被用着（否则上面那条零命中是因为**根本没有双工流**）。
    let mut checked = 0usize;
    for (_i, _) in prod.match_indices("inbound_client::split_and_park(") {
        checked += 1;
    }
    ScanReport {
        checked,
        violations: Vec::new(),
    }
    .require(
        2,
        "本文件应有两处双工切分（stream_loop 的长连接 + probe_daemon 的一次性探测）",
    )
    .expect("split_and_park 用量");
}

/// ★ 本文件的生产段**一个字节都不许自己往流里写**。
///
/// 写的能力整个交给了 `inbound_client`。这条是白名单的反面（「这里一处都不该有」），
/// 所以它必须自带**匹配器自检** —— 否则「零命中」既可能是干净，也可能是名单漏了写法。
#[test]
fn ssh_source_never_writes_to_a_stream_itself() {
    // 运行时拼，避免命中本文件里这几行自己。
    let needles: Vec<String> = [
        "write", // 覆盖 .write( / .write_all( / .write_buf( / .write_all_buf( / .write_vectored( / .write_u8(
        "shutdown",
    ]
    .iter()
    .map(|s| format!(".{s}"))
    .collect();
    let ufcs = format!("AsyncWrite{}::", "Ext");
    // ★★ **把流交给别人写**也算自己写〔audit-0805 08-07〕。
    //
    // 上面两个 needle 认的是「点调用」，`ufcs` 认的是 UFCS —— 三者都盯着
    // **写这个动作长什么样**。而 `tokio::io::copy(&mut src, stream)` 一个字都不沾，
    // 却实实在在把字节写进了流。08-07 实测：加这么一处，全仓 **976 条判据一条不红**。
    //
    // ⚠ 这次为什么可以枚举：`copy` 家族是 **tokio 的一个封闭上游 API 集合**
    // （`copy` / `copy_buf` / `copy_bidirectional` / `copy_bidirectional_with_sizes`），
    // 版本升级才会变，而且变了会**编译期**报出来。
    // 这与「枚举写法拼写」不是一回事 —— 后者的空间是无限的，这个是有边界的。
    let copy_family: Vec<String> = ["copy", "copy_buf", "copy_bidirectional"]
        .iter()
        .map(|f| format!("io::{f}("))
        .collect();

    // ── 匹配器自检 ────────────────────────────────────────────────────────
    //
    // 上一版的自检是 `let planted = format!("…{}…", needles[0]); assert!(needles.any(…))`
    // —— 用 needle 自己拼出来的样本，**数学上不可能失败**（D 审计点名）。
    // 现在样本是**独立手写**的真实写法清单，名单漏一种写法这里就红。
    let samples = [
        "let _ = w.write(b\"x\").await;",
        "let _ = w.write_all(b\"x\").await;",
        "let _ = w.write_all_buf(&mut b).await;",
        "let _ = w.write_vectored(&bufs).await;",
        "let _ = w.write_u8(1).await;",
        "let _ = w.shutdown().await;",
    ];
    // copy 家族的独立样本（同样**手写**，不用 needle 自己拼 —— 那种自检数学上不会失败）。
    let copy_samples = [
        "let n = tokio::io::copy(&mut src, stream).await?;",
        "tokio::io::copy_buf(&mut r, &mut w).await?;",
        "let (a, b) = tokio::io::copy_bidirectional(&mut x, &mut y).await?;",
    ];
    for sample in copy_samples {
        assert!(
            copy_family.iter().any(|n| sample.contains(n.as_str())),
            "copy 家族的匹配器漏了这种写法：{sample:?} —— 那条路照旧能把流写满"
        );
    }
    for sample in samples {
        assert!(
            needles.iter().any(|n| sample.contains(n.as_str())),
            "匹配器漏了这种写法：{sample:?} —— 下面那句「零命中」对它毫无意义"
        );
    }
    // 反面：不该被这些 needle 命中的普通代码（防止 needle 宽到人人都红、逼人删护栏）。
    for innocent in ["let n = buf.len();", "tracing::warn!(\"x\");"] {
        assert!(
            !needles.iter().any(|n| innocent.contains(n.as_str())),
            "needle 宽过头了，普通代码 {innocent:?} 也命中"
        );
    }

    let prod = prod();
    let mut hits: Vec<String> = needles
        .iter()
        .filter(|n| prod.contains(n.as_str()))
        .cloned()
        .collect();
    // UFCS 写法（`AsyncWriteExt::write_all(&mut w, …)`）绕开点调用，单列。
    if prod.contains(ufcs.as_str()) {
        hits.push(ufcs);
    }
    // 把流交给 copy 家族去写，同样算。
    hits.extend(
        copy_family
            .iter()
            .filter(|n| prod.contains(n.as_str()))
            .cloned(),
    );
    // ★ 堵逃生口：`use tokio::io::copy;` 之后裸写 `copy(..)`，上面三路都认不出。
    // 今天全文件只有一处 `use tokio::io::{AsyncBufReadExt, BufReader};`（都不是写能力）
    // ⇒ 这条零误红。放行清单写死在这里，新导入必须有人看一眼。
    // ★ 堵逃生口：`use tokio::io::copy;` 之后裸写 `copy(..)`，上面三路都认不出。
    //
    // ⚠ 放行清单是**用判据自己的 `prod()` 量出来的**。我第一版用「首个 `#[cfg(test)]`
    //   切生产段」去量，只得到一条 —— 本文件有多个测试模块，那个切法在本工作区
    //   已记过四次。**量具要用被测者那一套**，这是第五次。
    const ALLOWED_IO_IMPORTS: &[&str] = &[
        "use tokio::io::{AsyncBufReadExt, BufReader};",
        "use tokio::io::AsyncBufReadExt;",
        "use tokio::io::AsyncReadExt;",
    ];
    let io_imports: Vec<&str> = prod
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("use ") && l.contains("tokio::io"))
        .collect();
    // 自检：一条都没扫到 ⇒ 下面那句「不许有没登记的」是空转。
    assert!(
        !io_imports.is_empty(),
        "生产段里一条 `tokio::io` 导入都没扫到 —— 剥法坏了，本段此刻无效"
    );
    let bad_imports: Vec<&&str> = io_imports
        .iter()
        .filter(|l| !ALLOWED_IO_IMPORTS.contains(l))
        .collect();
    assert!(
        bad_imports.is_empty(),
        "本文件新增/改动了 `tokio::io` 导入：{bad_imports:?}\n\
             ⚠ 条目导入会让写能力变成**裸名字**（`use tokio::io::copy;` 之后 `copy(..)`），\n\
             上面那三路匹配器一个都认不出。要么用全路径 `tokio::io::xxx(`，\n\
             要么确认这条导入不带写能力之后把它加进 `ALLOWED_IO_IMPORTS`。"
    );
    // 反向锚点：放行清单不许留死行 —— 腐掉的清单看着像有人守，实则没有。
    for allowed in ALLOWED_IO_IMPORTS {
        assert!(
            io_imports.contains(allowed),
            "放行清单里的 {allowed:?} 在生产段里已经不存在了 —— 删掉它，\
                 否则下次有人写出同名导入会被静默放行。"
        );
    }

    assert!(
        hits.is_empty(),
        "ssh_source 的生产段又开始自己写流了（{hits:?}）。\n\
             写的能力在 U8a-2a 整个交给了 `inbound_client`：`ParkedWriter` 拿不到 Hello 见证\n\
             就换不出能写的东西。在这里直接写 = **静默绕过**那条类型保证。\n\
             真要发命令，走 `inbound_client::InboundClient::call`。"
    );
}
