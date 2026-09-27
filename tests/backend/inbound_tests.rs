/// ★★ **backend 的 runtime 必须是多 worker 的**〔audit-0805 08-08，Phase G 第 63 件〕。
///
/// `Disposition::SpawnBlocking` 的头注逐字写着：`main` 是**裸** `#[tokio::main]`
/// （worker 数 = 可用核数），所以「一条在跑的阻塞命令占住一个 worker」这件事
/// 只在**单核机器**（Pi 那一档）上才会饿死 `writer_task`；症状是
/// 「远端还活着但一句话不说」，而观测 watcher 在 `std::thread` 上不受影响，
/// 所以看起来更像网络问题 —— **极难排查**。
///
/// ⇒ 那整段论证压在「裸 `#[tokio::main]`」这五个字上，而**没人钉它**。
/// 08-08 实测：改成 `#[tokio::main(flavor = "current_thread")]`，
/// **backend 293 条判据一条不红** —— 而那一改会把单核才有的饿死**推广到所有机器**。
///
/// ⚠ 只钉「不是单 worker」，**不钉具体 worker 数**：那由机器决定，钉了就是把
/// 环境写进判据（本工作区反复在治的「把会腐的当前值抄进来」）。
/// ★ **「先摘登记再回应答」是一条真的时序约束**〔audit-0805 08-08，Phase G 第 79 件〕。
///
/// # 它不只是「白 abort 一个空壳」
///
/// 那行注释写的是：反过来的话，客户端收到应答后立刻发 `cancel`，
/// 可能命中一个已经跑完但还没摘掉的句柄。而**上面那段「拒重复 `id`」把后果抬高了一档**：
/// `dispatch` 见到登记表里已有同名 `id` 就**直接拒绝**。
/// ⇒ 「收到应答 ⇒ 这个 id 可以再用」这句话，正是靠 `remove` 排在 `send` 前面才成立的。
/// 调换两行，客户端**按应答办事**地复用 id 会被后端拒掉 —— 一个只在时序上出现、
/// 客户端侧无从解释的失败。
///
/// # 这条透镜（08-08「顺序」类声称）的结果一并记在这里
///
/// 全仓生产段扫出 31 处「顺序」声称，逐个查过：`launch.rs` 三条（`SendInto` /
/// `SendKeysRaw` / `CreateOrAttach`）· `kill.rs` 的门在 kill 之前 · `payload.rs` 的
/// `cd` 位次（逐字节 golden 对拍抓过一次）· `sanitize` 先于 `wrap`（F54 已钉接线）·
/// `fs.rs` 先看长度再读（F06）——**都已经有判据**。
/// **只有这一条没有**：实测把两行对调，backend 294 条一条不红。
#[test]
fn the_handle_is_deregistered_before_the_reply_goes_out() {
    let src = crate::guard_support::production_code(include_str!("../../src/backend/inbound.rs"));
    // 两个锚点各自的唯一性先量过：`remove` 那句只有一处；`replies.send(frame)` 有两处
    // （另一处在下面的监督臂里），所以**取第一处**并断言它就在 `remove` 之后。
    let remove_at = src
        .find("lock(&running_for_task).remove(")
        .expect("找不到摘登记那一句 —— 改写了就把本条一起改（本条会零命中地绿）");
    let send_at = src
        .find("replies.send(frame)")
        .expect("找不到回应答那一句 —— 同上");
    assert!(
        remove_at < send_at,
        "回应答排在摘登记之前了。\n\
             ★ 后果不是「白 abort 一个空壳」那么轻：本文件上面那段**拒重复 `id`** 意味着\n\
             「收到应答 ⇒ 这个 id 可以再用」，而那句话正是靠 `remove` 排在 `send` 前面才成立。\n\
             调换之后，客户端**按应答办事**地复用 id 会被后端直接拒掉 ——\n\
             一个只在时序上出现、客户端侧无从解释的失败。\n\
             ⚠ 真要先回应答（比如为了延迟），得先把「拒重复 id」那条规则一起重新设计。"
    );
    // 反向自检：两句必须在**同一个块**里，否则「谁在前」这个问题本身就没意义。
    //
    // ⚠ 08-08 第一版查的是「两句之间有没有 `tokio::spawn`」，被变异证伪：
    // 把 `remove` 搬进一个新 task 之后，那个 `spawn` 落在 `remove` **之前**、
    // 不在两句之间 ⇒ 自检一声不吭，而「先摘再回」已经名存实亡
    // （新 task 什么时候跑没人保证）。**锚错了边**：要找的是它们之间有没有**块边界**。
    // ⚠ **不写含大括号的字面量**：本文件被 `readonly_guard` 的括号配平扫描读，
    // 一个不配对的右大括号会把配平提前收尾、让它误报「测试段泄漏进生产段」——
    // F27 就是这个形状（警告文本自己触发了它警告的那件事），08-08 我又踩了一次。
    // 改判「摘登记那句的分号之后、回应答之前，除空白外什么都不许有」：
    // 同一段顺序代码里这两句是紧挨着的，一旦中间插进块结尾或别的语句，它就非空。
    let between = &src[remove_at..send_at];
    // 判「紧挨着」用**行数**：两个锚点必须落在相邻两行上。
    // （08-08 先写成「分号之后除空白外什么都不许有」，红在 `let _ =` 那半个绑定上 ——
    //  那是把「同一段代码」这个事实说窄了。）
    let gap = between.matches('\n').count();
    assert!(
        gap <= 1 && !between.contains("spawn("),
        "摘登记与回应答之间夹进了别的东西（跨了 {gap} 行）：{between:?}\n\
             ⇒ 它们已经不在同一段顺序执行的代码里，「谁在前」这个问题失去意义 ——\
             比如把 `remove` 丢进一个新 task，文本上它仍在前面，实际什么时候跑没人保证。\
             先修锚点/结构，再谈顺序。"
    );
}

#[test]
fn the_backend_runtime_keeps_more_than_one_worker() {
    let src = include_str!("../../src/backend/main.rs");
    let prod = guard_core::production_code(src);
    // 运行时拼：写成字面量会命中本条自己的诊断文案（F58/F62 记过）。
    // ⚠ **两种单 worker 写法都要认**。第一版只认 `current_thread`，
    //   而 `#[tokio::main(worker_threads = 1)]` 是等价的另一种 —— 变异当场证伪
    //   （本会话反复的那个病：判据锚在**一种写法**上）。
    //   这次能枚举是因为 tokio 的 runtime 属性是**封闭上游 API**：
    //   限成单 worker 只有这两条路，版本升级才会变、变了编译期就报（同 F43 的理由）。
    let single_forms = [
        format!("current{}thread", "_"),
        format!("worker_threads{}= 1", " "),
    ];
    // ⚠ 自检只问「有没有起 runtime 这件事」，**不问它是不是裸的** ——
    //   后者是主断言的活。第一版自检写成 `contains("#[tokio::main]")`，
    //   于是「改成单 worker」这一刀先撞上它，红出来的话是
    //   「找不到裸 attr，抽取器可能坏了」 —— **红对了位置、讲错了成因**
    //   （F42/F53 记过两次的同一形态：两条判据抢同一个变异，先响的讲错话）。
    assert!(
        prod.contains("tokio::main"),
        "`main.rs` 生产段里找不到 `tokio::main` —— runtime 的起法整个变了，\
             或者抽取器坏了。两种都要人来看一眼，本条此刻无效。"
    );
    let offenders: Vec<&str> = prod
        .lines()
        .filter(|l| {
            l.contains("tokio::main") && single_forms.iter().any(|f| l.contains(f.as_str()))
        })
        .collect();
    // 匹配器自检：**独立手写**的样本，不用 needle 自己拼（F43 的纪律）。
    for sample in [
        "#[tokio::main(flavor = \"current_thread\")]",
        "#[tokio::main(worker_threads = 1)]",
    ] {
        assert!(
            single_forms.iter().any(|f| sample.contains(f.as_str())),
            "单 worker 的匹配器漏了这种写法：{sample:?} —— 那条路照旧能把 writer_task 饿死"
        );
    }
    assert!(
        offenders.is_empty(),
        "backend 的 runtime 被改成了单 worker：{offenders:?}\n\
             ⚠ `SpawnBlocking` 那一档的整段论证前提是「worker 数 = 可用核数」——\n\
             单 worker 之下，**一条在跑的阻塞命令就占住唯一的 worker**，\n\
             `writer_task`（出方向帧的唯一出口）随即饿死：远端还活着但一句话不说，\n\
             而观测 watcher 在 `std::thread` 上照常工作 ⇒ 看起来像网络问题，极难排查。\n\
             真要改 runtime 形态，先把 `Disposition::SpawnBlocking` 的头注一起改。"
    );
}
use super::*;

fn chan() -> (mpsc::Sender<Frame>, mpsc::Receiver<Frame>) {
    mpsc::channel(REPLY_CHANNEL_CAPACITY)
}

async fn one_line(input: &str) -> Vec<String> {
    let (tx, mut rx) = chan();
    let h = spawn(
        std::io::Cursor::new(input.as_bytes().to_vec()),
        tx,
        crate::wire::HelloFlushed::for_tests(),
    );
    h.await.expect("reader task");
    let mut out = Vec::new();
    // 用 `recv().await` 直到 `None`，不用 `try_recv()`：处理器跑在**独立 task** 上
    // （那正是本模块的纪律），`try_recv` 会在它还没跑完时读到空。
    // sender 只有 reader task 与它 spawn 的处理器两处，都结束后 `recv()` 返回 `None`
    // ⇒ 确定性收尾，不需要 sleep、也不需要「空了就重跑一次」那种凑合法
    // （那种测试**可能因为错误的原因通过**）。
    while let Some(f) = rx.recv().await {
        out.push(
            crate::wire::to_line(&f)
                .expect("serialize")
                .trim_end()
                .to_string(),
        );
    }
    out
}

#[tokio::test]
async fn ping_replies_ok() {
    let out = one_line("{\"id\":\"x\",\"cmd\":\"ping\"}\n").await;
    // 逐字节钉死：顺带把「`code`/`message` 为 None 时不上线」也钉住了。
    assert_eq!(out, vec![r#"{"kind":"reply","id":"x","ok":true}"#]);
}

/// ★ `id` 是**不透明**的：backend 不解析、不规范化、只回显。
#[tokio::test]
async fn id_is_echoed_back_byte_for_byte() {
    for id in ["🌊-emoji", "0123456789", &"z".repeat(500)] {
        let line = format!(
            "{{\"id\":{},\"cmd\":\"nope\"}}\n",
            serde_json::to_string(id).unwrap()
        );
        let out = one_line(&line).await;
        let want = format!("\"id\":{}", serde_json::to_string(id).unwrap());
        assert!(
            out.iter().any(|l| l.contains(&want)),
            "id {id:?} 没被逐字回显：{out:?}"
        );
    }
}

#[tokio::test]
async fn unknown_command_is_an_error_reply_not_a_crash() {
    let out = one_line("{\"id\":\"a\",\"cmd\":\"rm-rf\"}\n").await;
    assert!(
        out.iter()
            .any(|l| l.contains("unknown_command") && l.contains("\"ok\":false")),
        "{out:?}"
    );
}

/// ★ 坏行**不许**拖垮读循环 —— 后面那条好行必须照常处理。
#[tokio::test]
async fn a_bad_line_does_not_stop_the_reader() {
    let out = one_line("not json at all\n{\"id\":\"after\",\"cmd\":\"nope\"}\n").await;
    assert!(
        out.iter().any(|l| l.contains("bad_request")),
        "坏行没回 bad_request：{out:?}"
    );
    assert!(
        out.iter().any(|l| l.contains("\"id\":\"after\"")),
        "坏行之后的好行没被处理 —— 读循环被拖垮了：{out:?}"
    );
}

/// ★ 超长行只丢它自己，进程活着、后面照常。
#[tokio::test]
async fn an_oversized_line_is_rejected_without_killing_the_reader() {
    let huge = format!(
        "{{\"id\":\"big\",\"cmd\":\"ping\",\"pad\":\"{}\"}}\n",
        "p".repeat(MAX_LINE_BYTES)
    );
    let out = one_line(&(huge + "{\"id\":\"after\",\"cmd\":\"nope\"}\n")).await;
    assert!(
        out.iter().any(|l| l.contains("line_too_long")),
        "超长行没被拒：{out:?}"
    );
    assert!(
        out.iter().any(|l| l.contains("\"id\":\"after\"")),
        "超长行之后的好行没被处理：{out:?}"
    );
}

/// 〔F9c · 第四波〕🔴 **超长行的应答带回请求的 `id`**（`设计/60 §9c.2`：此前回空串，调用方熬满预算才超时）。
///
/// 真读循环、真超长行（行长 > [`MAX_LINE_BYTES`]）。四形：`id` 在第一个键 · `id` 排在一个
/// 含假 `"id"` 的嵌套对象之后 · `id` 带转义 · 两条超长行挨着（第二条不许沾上第一条的 `id`）。
/// 阴性对照：信封里没有 `id` ⇒ 仍回空串（抠不出就不编）。
#[tokio::test]
async fn an_oversized_line_answers_with_the_id_it_carried() {
    let pad = "p".repeat(MAX_LINE_BYTES);
    let lines = [
        format!("{{\"id\":\"first\",\"cmd\":\"files-write-text\",\"args\":{{\"content\":\"{pad}\"}}}}\n"),
        format!(
            "{{\"cmd\":\"x\",\"args\":{{\"id\":\"inner\",\"n\":[1,{{\"id\":\"deeper\"}}]}},\"id\":\"outer\",\"pad\":\"{pad}\"}}\n"
        ),
        format!("{{\"id\":\"q\\\"uo\\\\te\",\"pad\":\"{pad}\"}}\n"),
        format!("{{\"cmd\":\"no-id\",\"pad\":\"{pad}\"}}\n"),
    ];
    let out = one_line(&lines.concat()).await;
    let got: Vec<(String, String)> = out
        .iter()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).expect("应答是一行 JSON");
            (
                v["id"].as_str().unwrap_or("<非字符串>").to_string(),
                v["code"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let want: Vec<(String, String)> = ["first", "outer", "q\"uo\\te", ""]
        .iter()
        .map(|id| (id.to_string(), "line_too_long".to_string()))
        .collect();
    assert_eq!(
        got, want,
        "超长行的应答没带回（或带错了）请求的 id —— 调用方会熬满自己的预算才超时"
    );
}

/// 〔F9c〕[`sniff_id`] 的边：只看给它的那一段，顶层之外的 `id` 不认，形状不对就不编。
#[test]
fn the_id_sniffer_only_believes_a_top_level_string_id() {
    let cases: &[(&str, Option<&str>)] = &[
        (r#"{"id":"a"}"#, Some("a")),
        (r#"  { "id" : "a b" , "cmd":"x"}"#, Some("a b")),
        (
            r#"{"cmd":"x","args":{"id":"inner"},"id":"outer"}"#,
            Some("outer"),
        ),
        (
            r#"{"n":-1.5e3,"t":true,"z":null,"s":"}\"{","id":"k"}"#,
            Some("k"),
        ),
        (r#"{"id":"\u4e2d"}"#, Some("中")),
        // 阴性：没有顶层 id · id 不是字符串 · 不是对象 · 段在 id 之前就断了
        (r#"{"args":{"id":"inner"}}"#, None),
        (r#"{"id":7}"#, None),
        (r#"["id","a"]"#, None),
        (r#"{"cmd":"xxxxxxxx"#, None),
        (r#"{"id":"unterminated"#, None),
        ("", None),
    ];
    for (head, want) in cases {
        assert_eq!(
            sniff_id(head.as_bytes()).as_deref(),
            *want,
            "嗅 {head:?} 嗅错了"
        );
    }
    // 读循环只交给它行首 `ID_SNIFF_BYTES` 那么多：排在 id 前面的键把 id 挤出这一段 ⇒ 抠不出（回旧行为）。
    let far = format!(r#"{{"pad":"{}","id":"late"}}"#, "p".repeat(ID_SNIFF_BYTES));
    assert_eq!(sniff_id(&far.as_bytes()[..ID_SNIFF_BYTES]), None);
    assert_eq!(
        sniff_id(far.as_bytes()).as_deref(),
        Some("late"),
        "正控：给全了就认得"
    );
}

/// ★ 喂一条**远超上限**的无换行流，进程内存不许跟着涨。
///
/// 这条测的是 D 审计抓到的那件事：上限如果是「读完再判」，`line_too_long`
/// 照样回、看起来全对，而 RSS 已经涨了一整条行那么多（实测 512 MiB）。
/// 判据只能是**内存**，不能是应答内容 —— 应答内容在两种实现下一模一样。
#[tokio::test]
async fn an_oversized_line_does_not_grow_memory() {
    /// 懒生成的无换行流：自己不占内存，读多少造多少。
    struct Flood {
        left: usize,
        nl_sent: bool,
    }
    impl tokio::io::AsyncRead for Flood {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            if self.left == 0 {
                if !self.nl_sent {
                    // 末尾补一个换行：不补的话这一整段永远不构成「一行」，
                    // reader 读到 EOF 直接收尾、什么都不回 —— 那是对的行为
                    // （对端没发完整行就走了），但那样就测不到超限应答。
                    self.nl_sent = true;
                    buf.put_slice(b"\n");
                }
                return std::task::Poll::Ready(Ok(())); // EOF
            }
            let n = buf.remaining().min(self.left).min(64 * 1024);
            buf.initialize_unfilled_to(n);
            buf.advance(n);
            self.left -= n;
            std::task::Poll::Ready(Ok(()))
        }
    }
    const FLOOD: usize = 256 * 1024 * 1024; // 256 MiB，是上限的 256 倍

    // 跑一次洪流。
    async fn flood_once() -> tokio::sync::mpsc::Receiver<crate::wire::Frame> {
        let (tx, rx) = chan();
        let h = spawn(
            Flood {
                left: FLOOD,
                nl_sent: false,
            },
            tx,
            crate::wire::HelloFlushed::for_tests(),
        );
        h.await.expect("reader task");
        rx
    }

    // 量具：**本线程**「已分配未释放」字节数的高水位，见 `crate::alloc_probe`。
    //
    // ⚠ 这里曾经读 `/proc/self/status` 的 `VmHWM`，那是**进程级**的，
    //   而 `cargo test` 在同一进程内并行跑测试 ⇒ 邻居的一次性大分配被算进来
    //   ⇒ 本条在 F07/F09/F11/F18 四件里各制造过一次假红。换线程级量具后成因消失。
    //   走过的弯路（「跑两遍只判第二遍」为什么是哑的）记在 `alloc_probe` 头注里。
    //
    // ⚠ 必须量**峰值**不是终值：那块 buffer 在 reader 返回前就 drop 了，
    //   终值看不见它。这也是最初不用当前 RSS 的同一个理由。
    let base = crate::alloc_probe::reset_peak();
    let mut rx = flood_once().await;
    let grew = crate::alloc_probe::peak_since(base);
    // 上限 2 MiB。实测峰值 **1_066_728 字节**（≈1.02 MiB，就是 `MAX_LINE_BYTES` 那块 buf
    // 本身加杂项），连跑三次**一字节不差** —— 线程级量具是确定的，所以余量可以收得很紧。
    // **余量意味着什么**：剩下的不到 1 MiB **放不下第二块整行**（`MAX_LINE_BYTES` = 1 MiB），
    // 所以任何「多留了一份整行」的回归都会顶穿它，不用等到 256 MiB 那种极端形状。
    assert!(
        grew < 2 * 1024 * 1024,
        "喂 {} MiB 无换行的流，本线程分配峰值涨了 {} MiB —— 上限是「读完再判」的，\n\
             它在整行进内存之后才生效（D 审计实测涨满 512 MiB）。",
        FLOOD / 1024 / 1024,
        grew / 1024 / 1024
    );
    // 顺带确认它确实**报了**超限，而不是悄悄吞掉。
    let mut saw = false;
    while let Ok(f) = rx.try_recv() {
        if crate::wire::to_line(&f)
            .unwrap_or_default()
            .contains("line_too_long")
        {
            saw = true;
        }
    }
    assert!(saw || FLOOD == 0, "超长流没回 line_too_long");
}

fn req(id: &str, cmd: &str) -> Request {
    Request {
        id: id.into(),
        cmd: cmd.into(),
        args: serde_json::Value::Null,
    }
}

/// ★ **接缝**：`dispatch` 必须把阻塞命令放到阻塞那一档上。
///
/// 下面那条 `cancelling_a_blocking_command_says_not_cancellable_instead_of_lying`
/// 验的是**机制**（`spawn_handler(..., cancellable=false)` 的行为）。
/// 变异实测：把 `dispatch` 里 `launch` 那一档的 `false` 改成 `true`，那条**照样绿** ——
/// 因为它直接调 `spawn_handler`，根本不经过 `dispatch`。
/// 这就是本区第 10 条纪律：**「两端各自有测试」≠「接起来是对的」，接缝要单独有判据。**
///
/// 这条走**真的 `dispatch`**，按数据（返回的 `Disposition` 变体）判，不是扫文本。
/// 变异复验：把 `launch` 那档改回普通 `spawn` ⇒ 本条红。
#[test]
fn the_dispatch_table_puts_blocking_commands_on_the_blocking_arm() {
    let (tx, _rx) = mpsc::channel::<Frame>(4);
    let running: Running = Arc::new(Mutex::new(HashMap::new()));
    let links = crate::dial::link::Table::new(tx.clone());
    let xfers = crate::control::transfer::Desk::new(tx.clone());
    let d = |cmd: &str| dispatch(req("x", cmd), &tx, &running, &links, &xfers);

    // `launch` 起进程、同步阻塞 ⇒ 必须是 SpawnBlocking（不占 tokio worker + 不可取消）。
    assert!(
        matches!(d("launch"), Disposition::SpawnBlocking(..)),
        "`launch` 不在阻塞档上 —— 它会占住 tokio worker（单核机器上把出方向也一起卡死），\n\
             而且 `cancel` 会对它撒谎（abort 对 spawn_blocking 是空操作）"
    );
    // 纯计算的两条留在普通 spawn 上（它们能在 await 点被真取消）。
    // 〔AS2 · 第四波 4B〕`assets-sync`：等拨号 / 等远端 capture —— 真异步，也在普通 spawn 上。
    // 〔RM1f〕`panorama` 起进程，但**异步等**（`plugin::invoke::run_abortable`）⇒ 同在这一档：
    //   不占 worker（等的是子进程退出，不是一段同步计算），`cancel` 命中时 future 被丢、子进程组被杀。
    // 〔C4d · 第四波 4B〕`remote-reach`：纯内存登记（一把锁、插一行），同 `ping` 在普通 spawn 上。
    // 〔C4d · 第四波 4B〕历史两条出成品：远端那一支等 `remote_ask`（真异步），本机扫盘那段自己挪到阻塞线程池。
    // 〔DUP2 · J4〕`acct-iso-cmd`：纯函数（校验 ＋ 唯一的 quote，不起进程不碰盘），同 `ping` / `resolve` 在普通 spawn 上。
    for c in [
        "ping",
        "resolve",
        "acct-iso-cmd",
        "ccm-probe", // 〔E2〕纯函数，普通 spawn
        "assets-sync",
        "panorama",
        "remote-reach",
        "history-projects",
        "history-sessions",
    ] {
        assert!(
            matches!(d(c), Disposition::Spawn(..)),
            "`{c}` 不该在阻塞档上 —— 那会让它白白变成不可取消"
        );
    }
    assert!(matches!(d("cancel"), Disposition::Done));
    assert!(matches!(d("nope"), Disposition::Reply(..)));
    // 〔SR1a〕链路四条是硬臂、**就地**做完（不进任何 spawn 档）：`link-data` 要保序，
    // 另三条只碰本连接的链路表。空 `args` ⇒ 当场回一条 `invalid_args` 应答（不起任务）。
    for c in ["link-open", "link-data", "link-credit", "link-close"] {
        assert!(
            matches!(
                d(c),
                Disposition::Reply(Frame::Reply { code: Some(ref code), .. }) if code == "invalid_args"
            ),
            "`{c}` 没有就地回应答 —— 它该是硬臂，不该进 spawn 档"
        );
    }

    // P4f：两条 cc-bus 命令**要起子进程并等它退出** ⇒ 与 `launch`/`kill` 同档。
    // `K-R104`：那两条 tmux 原语同理（抓一屏 / 建会话都要起 tmux 并等它退出）。
    // `K-R113`：`bus-state` 起**两个**子进程（`cc-list` ＋ `cc-agents`）⇒ 更是阻塞档。
    for c in [
        "bus-list",
        "bus-send",
        "bus-broadcast",
        "bus-kill",
        "bus-spawn",
        "bus-state",
        "bus-inbox",
        "capture-pane",
        // 〔LOC1a · 第四波 4D〕起插件进程 / 读写整份 jsonl。
        "acct-iso-status",
        "acct-iso-shellinit",
        "session-fork",
    ] {
        assert!(
            matches!(d(c), Disposition::SpawnBlocking(..)),
            "`{c}` 不在阻塞档上 —— 它要起子进程并等它退出，会占住 tokio worker"
        );
    }

    // 〔步 `24f` 第二刀〕`files-read` 四条**不起子进程**，但同样在阻塞档上：
    // `files::answer` 是**同步**函数 —— 前两条真做文件系统 I/O（`read_dir` / 取元数据），
    // `files-find` 在 64 万条量纲上的现打外推是 20–50 ms（`设计/60 §3.5.3`）。
    // 走 `Run::Async` 就是把这些跑在 tokio worker 上，而 `main` 是裸 `#[tokio::main]`
    //（worker 数 = 可用并行度）⇒ 单核机上一条查询就占住唯一的 worker，
    // 而出方向 writer 与入方向 reader 都在同一个 runtime 上。
    // 代价如实写：这一档**开跑之后打不断** ⇒ `cancel` 命中时回 `not_cancellable`，不撒谎。
    // 〔步 `24f` 第三刀〕新那两条同档，而且这一档对它们**更承重**：
    // `files-index-rebuild` 走一整棵树（64 万条现打 0.99 秒，**热缓存**；冷缓存没量过），
    // `files-browse` 把名单上那几个目录各 `read_dir` 一遍。
    // 放 tokio worker 上就是拿唯一那条 runtime 去跑一趟秒级遍历。
    // 〔波 5 ㈠ 09-23〕`files-create` 也在这一档：它开句柄 ＋ `write_all` 一遍，
    // 是同步阻塞 I/O，而且围栏② 还要 `canonicalize` 一次（真实路径解析）。
    // 〔波 5 ㈡ 09-23〕写面另外五条同档，理由同 `files-create`。
    for c in [
        "files-create",
        "files-commit-upload",
        "files-stage-chunk",
        "files-commit-text",
        "files-chmod",
        "files-delete",
        "files-mkdir",
        "files-rename",
        "files-write-text",
        "files-copy",
        "files-ls",
        "files-stat",
        "files-find",
        "files-index-status",
        "files-index-rebuild",
        "files-browse",
        // 〔F7a · 第三波 09-24〕同族第七、第八条：`files-read-text` 读一整份文件（同步 I/O）。
        "files-read-text",
        "files-home",
        // 〔W5-FILES〕读族第九条：走一整棵树（同步 I/O）。
        "files-size",
        // 〔`C1` · 09-24〕只读查询面八条：全做文件 I/O（`history-search` 扫全库）。
        // 〔C4d · 第四波 4B〕`history-projects` / `history-sessions` 出列：它们出成品、远端那一支要等 ⇒ 真异步（见上面那一档）。
        "history-index",
        "history-user-inputs",
        "history-find",
        "backend-log",   // 〔GAP1〕
        "history-facts", // 〔STC〕
        "history-read",
        "history-lines",  // 〔CF2〕
        "history-record", // 〔U4b〕
        "history-search",
        "history-subagents",
        "history-tail",
        "accounts-list",
        "accounts-sessions",
        // 〔C4c · 第四波 4B〕信任预检：读一份 manifest ＋ 一份 `.claude.json`（同步文件 I/O），同族同档。
        "accounts-trust",
        // 〔B2 · 条 66〕「退出行为」那两条：同步文件 I/O（读 / 原子写 `~/.cc-monitor` 下那一份）。
        "exit-policy-read",
        "exit-policy-set",
        // 〔RM1b · 第四波〕功能侧只读查询：读一个目录 ＋ 每个文件各一次（同步文件 I/O）。
        "plugins-marketplaces",
        "tasks-list",
        "mcp-read",
        "tmux-list",
        // 〔RM1f〕`panorama` 从这里挪走了：起进程改走 `invoke::run_abortable`（异步等子进程），
        //   上面「纯计算留在普通 spawn」那一格里单列它（可取消档）。
        // 〔RM1a · 第四波〕上游选择那份凭据文件的两条：同步文件 I/O（读 / 原子写那一份）。
        "apikey-key-set",
        "apikey-read",
        // 〔US1 · 第四波 4D〕上游选择出的两份成品：读一份凭据文件 ＋ 装一次表 ＋（要注入时）回环上探一次中转，同步阻塞。
        "apikey-routing",
        "launch-endpoint",
        // 〔RM1a · 第四波〕中转那两条：回环连一次 / 起一个进程。
        "relay-ensure",
        "relay-status",
        // 〔RM1a · 第四波〕足迹那一条：一批 stat / 读几份小文件。
        "footprint-probe",
        // 〔W5-ALIAS〕别名预览：读账号库 manifest ＋ 问会话快照。
        "ccm-print",
        // 〔AS2 · 第四波 4B〕资产目录两条：扫盘 ＋ 原子写目录文件。
        "assets-catalog",
        "assets-catalog-merge",
        // 〔AS2〕skill「装到这台」两条：走目录 ＋ 读原文 ＋ stat。
        "skill-read",
        "skill-install-plan",
        // 〔SU1 · 第四波 4C〕skill 卸三条：原子写装记录 · 读装记录 · 逐个读盘比摘要。
        "skill-install-record",
        "skill-installs",
        "skill-uninstall-plan",
        // 〔C4d · 第四波 4B〕历史注解三条：读 / 原子写一份小文件（同步文件 I/O）。
        "history-annotate",
        "history-forget",
        "history-last-accounts",
        // 〔RW1 · 第四波 09-24〕读改写两条 ＋ 删历史会话：同步文件 I/O（围栏 ＋ 读 / 写满换名 / 删）。
        "files-peek",
        "files-put",
        "files-delete-session",
        // 〔AS1 · 第四波 4B〕MCP 同步的判定：逐条 stat ＋ PATH 上找名字。
        "mcp-sync-plan",
    ] {
        assert!(
            matches!(d(c), Disposition::SpawnBlocking(..)),
            "`{c}` 不在阻塞档上 —— 它是同步处理器，放 tokio worker 上会把读循环那条 runtime 占住"
        );
    }

    // 计数自检：每条已声明的命令都被上面覆盖到了（新增命令必须来这里表态）。
    let covered = [
        "launch",
        "kill",
        "ping",
        "resolve",
        // 〔DUP2 · J4〕纯函数，普通 spawn。
        "acct-iso-cmd",
        // 〔E2〕`ccm-probe`：纯函数，普通 spawn。
        "ccm-probe",
        "assets-sync",
        // 〔C4d · 第四波 4B〕可达表登记（纯内存，普通 spawn）。
        "remote-reach",
        "cancel",
        "link-open",
        "link-data",
        "link-credit",
        "link-close",
        "bus-list",
        "bus-send",
        "bus-broadcast",
        "bus-kill",
        "bus-spawn",
        "bus-state",
        "bus-inbox",
        "capture-pane",
        "acct-iso-status",
        "acct-iso-shellinit",
        "session-fork",
        "files-create",
        "files-commit-upload",
        "files-stage-chunk",
        "files-commit-text",
        "files-chmod",
        "files-delete",
        "files-mkdir",
        "files-rename",
        "files-write-text",
        "files-copy",
        "files-ls",
        "files-stat",
        "files-find",
        "files-index-status",
        "files-index-rebuild",
        "files-browse",
        "files-read-text",
        "files-home",
        // 〔W5-FILES〕读族第九条：走一整棵树（同步 I/O）。
        "files-size",
        "history-projects",
        "history-index",
        "history-user-inputs",
        "history-find",
        "backend-log",   // 〔GAP1〕
        "history-facts", // 〔STC〕
        "history-read",
        "history-lines",  // 〔CF2〕
        "history-record", // 〔U4b〕
        "history-search",
        "history-sessions",
        "history-subagents",
        "history-tail",
        "accounts-list",
        "accounts-sessions",
        "accounts-trust", // 〔C4c〕
        "exit-policy-read",
        "exit-policy-set",
        "plugins-marketplaces",
        "tasks-list",
        "mcp-read",
        "tmux-list",
        "panorama",
        "apikey-key-set",
        "apikey-read",
        "apikey-routing",  // 〔US1〕
        "launch-endpoint", // 〔US1〕
        "relay-ensure",
        "relay-status",
        "footprint-probe",
        // 〔W5-ALIAS〕别名预览，阻塞档。
        "ccm-print",
        // 〔AS2 · 第四波 4B〕资产目录两条，阻塞档。
        "assets-catalog",
        "assets-catalog-merge",
        "skill-read",
        "skill-install-plan",
        // 〔SU1 · 第四波 4C〕skill 卸三条，阻塞档。
        "skill-install-record",
        "skill-installs",
        "skill-uninstall-plan",
        // 〔C4d · 第四波 4B〕历史注解三条，阻塞档。
        "history-annotate",
        "history-forget",
        "history-last-accounts",
        // 〔RW1 · 第四波 09-24〕读改写两条 ＋ 删历史会话：同步文件 I/O，阻塞档。
        "files-peek",
        "files-put",
        "files-delete-session",
        // 〔AS1 · 第四波 4B〕MCP 同步的判定，阻塞档。
        "mcp-sync-plan",
        // 〔SR1b〕传输四条：硬臂，就地记账（起跑那一下只 `spawn`、不 await）⇒ 不阻塞。
        "transfer-upload",
        "transfer-download",
        "transfer-start",
        "transfer-stop",
    ];
    let missing: Vec<&&str> = COMMANDS.iter().filter(|c| !covered.contains(c)).collect();
    assert!(
        missing.is_empty(),
        "这些命令没在本条里表态「阻塞还是不阻塞」：{missing:?}\n\
             新增命令时必须回答这个问题 —— 放错档的代价是「占住 worker」或「假装能取消」。"
    );
}

/// ★★ `KR104D1` ③ 的**总伞**：注册表里每一条命令，走**真的 `dispatch`**
/// 都必须落到它自己的处理器上 —— 一条都不许落进 `unknown_command`。
///
/// # 它买到什么，而上面那条买不到
///
/// 上面那条按**手写清单**（`covered`）逐条表态，清单漏一条它就漏一条；
/// 本条的发现机制是**遍历 `REGISTRY`** ⇒ 加一条命令，本条**自动**盖到它，
/// 不需要任何人来这里写名字。这正是 `K-R102` 问的那句「要不要有一把总的」——
/// 在帧面上答案是**要，而且很便宜**：注册表是数据，遍历一遍就是全集。
///
/// # 它逮的那一形（`K-R102` 记的那个盲区）
///
/// 「表里有、分派到不了」。CLI 那面之所以逮不住，是因为它的判据**数字面量**，
/// 而表自己就住在生产段里 ⇒ 同一次扫描里「表里有」与「源码里有」同时成立。
/// 本条不数字面量：它**真的调一次 `dispatch`**，按返回的 `Disposition` 判。
///
/// ⚠ **它不证明处理器做得对** —— 只证明「够得到」。做得对是各命令自己的判据。
#[test]
fn every_registered_command_is_reachable_through_the_real_dispatch() {
    let (tx, _rx) = mpsc::channel::<Frame>(4);
    let running: Running = Arc::new(Mutex::new(HashMap::new()));
    let links = crate::dial::link::Table::new(tx.clone());
    let xfers = crate::control::transfer::Desk::new(tx.clone());
    assert!(
        REGISTRY.len() >= 4,
        "注册表只有 {} 条 —— 本条在空转",
        REGISTRY.len()
    );
    let unreachable: Vec<&str> = REGISTRY
        .iter()
        .map(|spec| spec.name)
        .filter(|name| {
            matches!(
                dispatch(req("x", name), &tx, &running, &links, &xfers),
                Disposition::Reply(Frame::Reply { code: Some(ref c), .. })
                    if c == "unknown_command"
            )
        })
        .collect();
    assert!(
        unreachable.is_empty(),
        "这些命令在注册表里，而 `dispatch` 到不了它们的处理器：{unreachable:?}\n\
             ⇒ `hello.commands` 会把它们报给客户端，客户端照报的发过来，收到的是 \
             `unknown_command` —— 两边各自看都「对」。\n\
             今天这一形只有两种成因：① 有人给它写了 `Run::Builtin` 却没在 `dispatch` 里\n\
             加那条硬臂；② 有人收窄了 `lookup`。两种都得回来重判，别改本条。"
    );
    // ★ 反向自检：这把尺子真的会说「够不到」—— 不然上面那一批是空真。
    assert!(
        matches!(
            dispatch(req("x", "no-such-command-kr104"), &tx, &running, &links, &xfers),
            Disposition::Reply(Frame::Reply { code: Some(ref c), .. }) if c == "unknown_command"
        ),
        "喂一个根本不存在的命令进去，本条居然认为它够得到 —— 那上面那一批证不了任何事"
    );
}

/// 〔NET2 · `设计/05 §3.3.3`〕hello 的 `uncancellable` == 真 `dispatch` 会送进阻塞档（开跑之后撤不动）的那几条，两向。
/// 两侧异源：左边是 `uncancellable()`（读表的档位），右边是真调一次 `dispatch` 看它回哪种 `Disposition`。
#[test]
fn the_uncancellable_list_is_exactly_what_dispatch_runs_blocking() {
    let (tx, _rx) = mpsc::channel::<Frame>(64);
    let running: Running = Arc::new(Mutex::new(HashMap::new()));
    let links = crate::dial::link::Table::new(tx.clone());
    let xfers = crate::control::transfer::Desk::new(tx.clone());
    let blocking: Vec<String> = REGISTRY
        .iter()
        .map(|spec| spec.name)
        .filter(|name| {
            matches!(
                dispatch(req("x", name), &tx, &running, &links, &xfers),
                Disposition::SpawnBlocking(..)
            )
        })
        .map(str::to_string)
        .collect();
    assert!(!blocking.is_empty(), "一条阻塞档都没有 —— 本条在空转");
    assert_eq!(super::uncancellable(), blocking);
}

/// ★ **不可取消的命令不许回一条撒谎的 `cancelled`。**
///
/// D 设计审计（视角 A · P4）：`launch` 的处理器是同步阻塞的，`abort()` 对
/// `spawn_blocking` 起的活是空操作 ⇒ 旧实现下客户端收到 `Cancelled`，
/// 而远端的 tmux 会话照样建出来、载荷照样键入。**控制面在骗调用方。**
#[tokio::test]
async fn cancelling_a_blocking_command_says_not_cancellable_instead_of_lying() {
    let (tx, mut rx) = mpsc::channel::<Frame>(16);
    let running: Running = Arc::new(Mutex::new(HashMap::new()));
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();

    // 一条「在跑、且不可取消」的命令。
    spawn_handler(
        req("blk", "launch"),
        tx.clone(),
        running.clone(),
        move |_r| async move {
            let _ = release_rx.await;
            Ok(None)
        },
        false, // ← 不可取消
    )
    .await;

    // 对它发 cancel。
    let links = crate::dial::link::Table::new(tx.clone());
    let xfers = crate::control::transfer::Desk::new(tx.clone());
    handle_line(
        br#"{"id":"c1","cmd":"cancel","args":{"target":"blk"}}"#,
        &tx,
        &running,
        &links,
        &xfers,
    )
    .await;

    let f = rx.recv().await.expect("应当有应答");
    match f {
        Frame::Reply {
            id, ok, ref code, ..
        } => {
            assert_eq!(id, "c1");
            assert!(!ok, "对不可取消的命令发 cancel 不该回 ok");
            assert_eq!(
                code.as_deref(),
                Some("not_cancellable"),
                "应当明说停不下来，而不是撒一条 cancelled 的谎"
            );
        }
        other => panic!("应答形状不对：{other:?}"),
    }
    // **绝不许**出现 `Cancelled` 帧。
    assert!(
        rx.try_recv().is_err(),
        "除了 not_cancellable 之外还发了别的帧 —— 那多半就是那条谎"
    );
    // 登记还在（命令真的还在跑）。
    assert!(lock(&running).contains_key("blk"), "不可取消的命令被摘掉了");
    let _ = release_tx.send(());
}

/// ★ 登记表不许残留空壳。
///
/// 复现 D 审计那条：旧版先 `spawn` 后 `insert`，multi_thread 下 task 可以在 `insert`
/// 之前就跑完自己的 `remove` ⇒ 那条 `insert` 把已完成任务的句柄永久留在表里。
/// 实测 20 000 条命令回完应答后**还剩 1964 个**。
///
/// 后果不止泄漏：空壳让 `cancel` **撒谎** —— 对一条早就成功回过 `ok` 的命令发 cancel，
/// 会收到一条 `cancelled`，客户端把它记成「被取消了」。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_running_table_never_leaks_finished_handles() {
    const N: usize = 5_000;
    let (tx, mut rx) = chan();
    let running: Running = Arc::new(Mutex::new(HashMap::new()));
    for i in 0..N {
        spawn_handler(
            req(&format!("id-{i}"), "ping"),
            tx.clone(),
            running.clone(),
            |_r| async move { Ok(None) },
            true,
        )
        .await;
    }
    drop(tx);
    let mut got = 0usize;
    while rx.recv().await.is_some() {
        got += 1;
    }
    assert_eq!(got, N, "应答条数不对 —— 本断言在空转");
    let left = lock(&running).len();
    assert_eq!(
        left, 0,
        "{N} 条命令全部回完应答后，登记表里还剩 {left} 个句柄（空壳）"
    );
}

/// ★ 重复 `id` 必须**明确拒绝**，不许静默覆盖。
///
/// 旧版 `insert` 覆盖 ⇒ 前一条永远取消不掉，而再发 cancel 照样回 `ok:true`
/// （被「取消不存在的 id 是幂等的」那条规则盖住）；且先跑完的那条会把另一条的句柄摘走。
#[tokio::test]
async fn a_duplicate_id_is_rejected_instead_of_silently_overwriting() {
    let (tx, mut rx) = chan();
    let running: Running = Arc::new(Mutex::new(HashMap::new()));
    // 第一条：占住 id，直到测试收尾时放行。
    //
    // **不用 `std::future::pending()`**：那样 handler 与它的监督 task 永不结束，
    // 测试断言过了之后进程还挂着不退（实测卡 60s+，整个 test binary 收不了尾）。
    let (release, held) = tokio::sync::oneshot::channel::<()>();
    spawn_handler(
        req("dup", "ping"),
        tx.clone(),
        running.clone(),
        |_r| async move {
            let _ = held.await;
            Ok(None)
        },
        true,
    )
    .await;
    // 第二条：同一个 id。
    spawn_handler(
        req("dup", "ping"),
        tx.clone(),
        running.clone(),
        |_r| async move { Ok(None) },
        true,
    )
    .await;
    drop(tx);
    // **不能排空到 `None`**：占位那条与它的监督 task 各持一个 sender，未放行前不落地。
    // 拒绝那条是**同步**回进通道的（`send` 在未满通道上立即完成），此刻 `try_recv` 必有。
    // 不用 `timeout`：本仓有零定时器纪律，确定性写法本来也比等超时好。
    let first = rx
        .try_recv()
        .expect("通道里没有应答 —— 重复 id 被静默吞了？");
    let line = crate::wire::to_line(&first).expect("serialize");
    assert!(
        line.contains("duplicate_id"),
        "重复 id 没被拒绝，被静默覆盖了：{line}"
    );
    assert_eq!(
        lock(&running).len(),
        1,
        "第一条应当仍在表里、仍可取消（重复的那条不该动它）"
    );
    let _ = release.send(()); // 放行占位那条，让它与监督 task 都收尾
}

/// ★ 处理器 panic ⇒ 客户端拿得到错误应答，登记表不泄漏。
#[tokio::test]
async fn a_panicking_handler_still_answers_the_client() {
    let (tx, mut rx) = chan();
    let running: Running = Arc::new(Mutex::new(HashMap::new()));
    spawn_handler(
        req("boom", "ping"),
        tx.clone(),
        running.clone(),
        |_r| async {
            panic!("处理器炸了");
        },
        true,
    )
    .await;
    drop(tx);
    let mut lines = Vec::new();
    while let Some(f) = rx.recv().await {
        lines.push(crate::wire::to_line(&f).expect("serialize"));
    }
    assert!(
        lines.iter().any(|l| l.contains("handler_panicked")),
        "处理器 panic 之后客户端什么都没收到 —— 它会永远挂着：{lines:?}"
    );
    assert_eq!(lock(&running).len(), 0, "panic 之后登记表泄漏了");
}

/// 取消一个不存在的 id 是**幂等**的，不是错误。
#[tokio::test]
async fn cancelling_an_unknown_id_is_idempotent_not_an_error() {
    let out = one_line("{\"id\":\"c\",\"cmd\":\"cancel\",\"args\":{\"target\":\"ghost\"}}\n").await;
    assert!(
        out.iter()
            .any(|l| l.contains("\"id\":\"c\"") && l.contains("\"ok\":true")),
        "{out:?}"
    );
}

/// ★〔RM1f · C3〕**`cancel` 打得断在飞的 `panorama`**：回 `cancelled`（不是 `not_cancellable`），
/// 小程序那一组子进程没了。
///
/// 处理器走**真的** `control::panorama::answer_with`（找它 · 问它会什么 · 起它全是真进程：
/// 一个说小程序那套方言的 sh 替身，`index` 那一问睡着、把自己的 pid 写进文件），
/// 登记走真的 `spawn_handler`、`cancel` 走真的 `handle_line`。
/// 档位那一格（`dispatch` 把 `panorama` 放进可取消档）由上面 `the_dispatch_table_puts_…` 钉。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_really_stops_an_in_flight_panorama_index() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("ccm-be-inbound-pano-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pidf = dir.join("pid");
    let bin = dir.join(crate::control::panorama::PLUGIN_NAME);
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--probe\" ]; then printf 'name=cc-monitor-panorama\\nversion=t\\ncapabilities=index\\n'; exit 0; fi\n\
             echo $$ > '{p}.tmp'; mv '{p}.tmp' '{p}'\nexec sleep 300\n",
            p = pidf.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

    let (tx, mut rx) = chan();
    let running: Running = Arc::new(Mutex::new(HashMap::new()));
    let (fixed, store) = (vec![bin.clone()], dir.join("store"));
    spawn_handler(
        Request {
            id: "pano".into(),
            cmd: "panorama".into(),
            args: serde_json::json!({"op": "index", "repo": "/r"}),
        },
        tx.clone(),
        running.clone(),
        move |r: Request| async move {
            crate::control::panorama::answer_with(&fixed, &store, &r.args)
                .await
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        },
        true,
    )
    .await;
    // 等替身把 pid 写出来（= 它真的在跑了）。
    let mut pid = None;
    for _ in 0..200 {
        if let Ok(s) = std::fs::read_to_string(&pidf) {
            pid = s.trim().parse::<u32>().ok();
            if pid.is_some() {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let pid = pid.expect("替身小程序 10 秒内没起来 —— 本条判不了");
    let alive = |p: u32| {
        std::fs::read_to_string(format!("/proc/{p}/stat")).is_ok_and(|s| {
            s.rsplit_once(')')
                .and_then(|(_, r)| r.split_whitespace().next())
                .is_some_and(|st| st != "Z" && st != "X")
        })
    };
    assert!(alive(pid), "正控：取消之前它在跑");

    let links = crate::dial::link::Table::new(tx.clone());
    let xfers = crate::control::transfer::Desk::new(tx.clone());
    handle_line(
        br#"{"id":"c1","cmd":"cancel","args":{"target":"pano"}}"#,
        &tx,
        &running,
        &links,
        &xfers,
    )
    .await;
    let mut frames = Vec::new();
    for _ in 0..2 {
        frames.push(
            tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
                .await
                .expect("5 秒内没等到应答")
                .expect("通道关了"),
        );
    }
    assert!(
        frames
            .iter()
            .any(|f| matches!(f, Frame::Cancelled { id } if id == "pano")),
        "没有 `cancelled{{pano}}` 帧：{frames:?}"
    );
    assert!(
        !frames.iter().any(|f| matches!(
            f,
            Frame::Reply { code: Some(c), .. } if c == "not_cancellable"
        )),
        "`panorama` 又回了 `not_cancellable`：{frames:?}"
    );
    let mut dead = false;
    for _ in 0..100 {
        if !alive(pid) {
            dead = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status();
    assert!(
        dead,
        "`cancelled` 回了，小程序（{pid}）5 秒后还在跑 —— 那是一条撒谎的 `cancelled`"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
