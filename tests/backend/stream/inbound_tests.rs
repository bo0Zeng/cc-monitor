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
/// 裸键那一支〔RST 续：已删〕/ `CreateOrAttach`）· `kill.rs` 的门在 kill 之前 · `payload.rs` 的
/// `cd` 位次（逐字节 golden 对拍抓过一次）· `sanitize` 先于 `wrap`（F54 已钉接线）·
/// `fs.rs` 先看长度再读（F06）——**都已经有判据**。
/// **只有这一条没有**：实测把两行对调，backend 294 条一条不红。
#[test]
fn the_handle_is_deregistered_before_the_reply_goes_out() {
    let src =
        guard_core::production_code(include_str!("../../../src/backend/stream/inbound/mod.rs"));
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
    let src = include_str!("../../../src/backend/main.rs");
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
use crate::wire::spec::{invoke, Prepared};

/// 分派要的那两样：`stream-watch` 那一口 ＋ 一份指到临时目录的写句柄。
fn given_for_tests(watch: WatchDesk) -> Given {
    Given {
        watch,
        own: crate::platform::fs::Own::for_tests(),
    }
}

fn chan() -> (mpsc::Sender<Frame>, mpsc::Receiver<Frame>) {
    mpsc::channel(REPLY_CHANNEL_CAPACITY)
}

async fn one_line(input: &str) -> Vec<String> {
    let (tx, mut rx) = chan();
    let h = spawn(
        std::io::Cursor::new(input.as_bytes().to_vec()),
        tx,
        crate::stream::inbound::WatchDesk::new(|_| {
            tokio::sync::oneshot::channel::<Vec<crate::wire::WatchFrom>>().1
        }),
        crate::platform::fs::Own::for_tests(),
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

/// 🔴 **超长行的应答带回请求的 `id`**（此前回空串，调用方熬满预算才超时）。
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

/// [`sniff_id`] 的边：只看给它的那一段，顶层之外的 `id` 不认，形状不对就不编。
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
            crate::stream::inbound::WatchDesk::new(|_| {
                tokio::sync::oneshot::channel::<Vec<crate::wire::WatchFrom>>().1
            }),
            crate::platform::fs::Own::for_tests(),
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
        within_ms: None,
        view: serde_json::Value::Null,
        tz: Default::default(),
        until: None,
    }
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
    let xfers = crate::control::transfer::Desk::new(tx.clone(), crate::platform::fs::Face::issue());
    let follows = crate::control::terminal_follow::Desk::new(tx.clone());
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
                dispatch(req("x", name), &tx, &running, &links, &xfers, &follows, &given_for_tests(crate::stream::inbound::WatchDesk::new(|_| tokio::sync::oneshot::channel::<Vec<crate::wire::WatchFrom>>().1)), 0),
                Disposition::Reply(Frame::Reply(crate::wire::Reply { code: Some(ref c), .. }))
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
            dispatch(req("x", "no-such-command-kr104"), &tx, &running, &links, &xfers, &follows, &given_for_tests(crate::stream::inbound::WatchDesk::new(|_| tokio::sync::oneshot::channel::<Vec<crate::wire::WatchFrom>>().1)), 0),
            Disposition::Reply(Frame::Reply(crate::wire::Reply { code: Some(ref c), .. })) if c == "unknown_command"
        ),
        "喂一个根本不存在的命令进去，本条居然认为它够得到 —— 那上面那一批证不了任何事"
    );
}

/// 指纹表的「撤不动」名单 == 真 `dispatch` 会送进阻塞档（开跑之后撤不动）的那几条，两向。
/// 两侧异源：左边是生成物 `cmd_wire::shapes::UNCANCELLABLE`（照登记的档位写），右边是真调一次 `dispatch` 看它回哪种 `Disposition`。
#[test]
fn the_uncancellable_list_is_exactly_what_dispatch_runs_blocking() {
    let (tx, _rx) = mpsc::channel::<Frame>(64);
    let running: Running = Arc::new(Mutex::new(HashMap::new()));
    let links = crate::dial::link::Table::new(tx.clone());
    let xfers = crate::control::transfer::Desk::new(tx.clone(), crate::platform::fs::Face::issue());
    let follows = crate::control::terminal_follow::Desk::new(tx.clone());
    let blocking: Vec<String> = REGISTRY
        .iter()
        .map(|spec| spec.name)
        .filter(|name| {
            matches!(
                dispatch(
                    req("x", name),
                    &tx,
                    &running,
                    &links,
                    &xfers,
                    &follows,
                    &given_for_tests(crate::stream::inbound::WatchDesk::new(|_| {
                        tokio::sync::oneshot::channel::<Vec<crate::wire::WatchFrom>>().1
                    })),
                    0,
                ),
                Disposition::SpawnBlocking(..)
            )
        })
        .map(str::to_string)
        .collect();
    assert!(!blocking.is_empty(), "一条阻塞档都没有 —— 本条在空转");
    // 指纹表那份「撤不动」名单（客户端读它本侧编进来的那一份）与分派那一侧两向相等。
    let mut all: Vec<String> = cmd_wire::shapes::UNCANCELLABLE
        .iter()
        .map(|s| s.to_string())
        .collect();
    all.sort();
    let mut blocking = blocking;
    blocking.sort();
    assert_eq!(all, blocking);
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
        views::Plan::default(),
    )
    .await;

    // 对它发 cancel。
    let links = crate::dial::link::Table::new(tx.clone());
    let xfers = crate::control::transfer::Desk::new(tx.clone(), crate::platform::fs::Face::issue());
    let follows = crate::control::terminal_follow::Desk::new(tx.clone());
    handle_line(
        br#"{"id":"c1","cmd":"cancel","args":{"target":"blk"}}"#,
        &tx,
        &running,
        &links,
        &xfers,
        &follows,
        &given_for_tests(crate::stream::inbound::WatchDesk::new(|_| {
            tokio::sync::oneshot::channel::<Vec<crate::wire::WatchFrom>>().1
        })),
        0,
    )
    .await;

    let f = rx.recv().await.expect("应当有应答");
    match f {
        Frame::Reply(crate::wire::Reply {
            id, ok, ref code, ..
        }) => {
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
            views::Plan::default(),
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
        views::Plan::default(),
    )
    .await;
    // 第二条：同一个 id。
    spawn_handler(
        req("dup", "ping"),
        tx.clone(),
        running.clone(),
        |_r| async move { Ok(None) },
        true,
        views::Plan::default(),
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
        views::Plan::default(),
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

/// 复制详情：失败应答多一格 `detail`（句子下面那几行「项名：值」），句子那一格不变。
/// 协议级失败（命令认不出）带时刻 · 机器 · 码；处理器回的失败另带命令名。
#[tokio::test]
async fn a_failed_reply_carries_a_detail_with_the_facts_below_the_sentence() {
    let out =
        one_line("{\"id\":\"a\",\"cmd\":\"rm-rf\"}\n{\"id\":\"b\",\"cmd\":\"kill\",\"args\":{}}\n")
            .await;
    let reply = |id: &str| -> serde_json::Value {
        let l = out
            .iter()
            .find(|l| l.contains(&format!("\"id\":\"{id}\"")))
            .unwrap_or_else(|| panic!("{id} 没回：{out:?}"));
        serde_json::from_str(l).unwrap()
    };
    for (id, code) in [("a", "unknown_command"), ("b", "bad_args")] {
        let r = reply(id);
        let detail = r["detail"]
            .as_str()
            .unwrap_or_else(|| panic!("{id} 无 detail：{r}"));
        let label = |k: &str| copy_core::copy_text(k, &[]);
        assert!(
            detail.starts_with(&format!("{}：", label("detail.label.at"))),
            "{detail}"
        );
        assert!(
            detail.contains(&format!("\n{}：{code}", label("detail.label.code"))),
            "{detail}"
        );
        assert!(
            detail.contains(&format!("\n{}：", label("detail.label.machine"))),
            "{detail}"
        );
        assert!(
            !r["message"].as_str().unwrap().contains(code),
            "码进了句子：{r}"
        );
    }
    let b = reply("b");
    assert!(
        b["detail"].as_str().unwrap().contains(&format!(
            "{}：kill",
            copy_core::copy_text("detail.label.command", &[])
        )),
        "{b}"
    );
}

/// 硬臂那几条（链路 · 传输 · 终端订阅：`Run::Builtin`，就地回应答）被拒：复制详情里也有「命令」那一项（同处理器回的失败）。
#[tokio::test]
async fn a_builtin_refusal_names_its_command_in_the_detail() {
    let cmds = [
        "link-open",
        "link-credit",
        "link-close",
        "transfer-start",
        "terminal-follow-ack",
        "terminal-unfollow",
    ];
    let input: String = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "{}\n",
                serde_json::json!({"id": format!("b{i}"), "cmd": c, "args": {}})
            )
        })
        .collect();
    let out = one_line(&input).await;
    let label = copy_core::copy_text("detail.label.command", &[]);
    for (i, c) in cmds.iter().enumerate() {
        let r: serde_json::Value = serde_json::from_str(
            out.iter()
                .find(|l| l.contains(&format!("\"id\":\"b{i}\"")))
                .unwrap_or_else(|| panic!("{c} 没回：{out:?}")),
        )
        .unwrap();
        assert_eq!(r["ok"], false, "{c}：{r}");
        let detail = r["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains(&format!("{label}：{c}")),
            "{c} 的详情里没有命令：{detail}"
        );
    }
}

/// 文件读那一族读不了：句子只带原因词，系统原话进复制详情的「原话」那一行（不进句子）。
#[tokio::test]
async fn a_files_read_failure_puts_the_system_words_into_the_detail() {
    let base = std::env::temp_dir().join(format!("ccm-inb-files-raw-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let f = base.join("f.txt");
    std::fs::write(&f, b"x").unwrap();
    let line = format!(
        "{}\n",
        serde_json::json!({"id": "l", "cmd": "files-ls", "args": {"path": f.to_str().unwrap()}})
    );
    let out = one_line(&line).await;
    std::fs::remove_dir_all(&base).ok();
    let r: serde_json::Value = serde_json::from_str(
        out.iter()
            .find(|l| l.contains("\"id\":\"l\""))
            .unwrap_or_else(|| panic!("没回：{out:?}")),
    )
    .unwrap();
    assert_eq!(r["code"], "not_dir", "{r}");
    let message = r["message"].as_str().unwrap();
    let detail = r["detail"].as_str().unwrap();
    assert_eq!(
        message,
        copy_core::copy_text(
            "beFilesRead.ls.unreadable",
            &[(
                "kind",
                &copy_core::io_reason(std::io::ErrorKind::NotADirectory)
            )]
        )
    );
    let raw_label = format!("{}：", copy_core::copy_text("detail.label.raw", &[]));
    let raw = detail
        .lines()
        .find_map(|l| l.strip_prefix(raw_label.as_str()))
        .unwrap_or_else(|| panic!("详情里没有原话那一行：{detail}"));
    assert!(
        !raw.trim().is_empty() && !message.contains(raw.trim()),
        "{r}"
    );
}

/// `stream-watch`：名单（连同要哪几个的主线外清单、手上各有什么）原样交给这条连接的 watcher（整份），它答的「从第几行起」原样回；
/// `sids` 不是串数组 · `branch` 形状不对 ⇒ `bad_args`、不交。`branch` 可缺（＝ 一份都不要）。
#[tokio::test]
async fn stream_watch_hands_the_list_to_this_connections_watcher_and_returns_its_answer() {
    use crate::wire::{BranchHeld, WatchAsk};
    let asked: Arc<Mutex<Vec<WatchAsk>>> = Arc::default();
    let seen = asked.clone();
    let desk = WatchDesk::new(move |ask| {
        seen.lock().unwrap().push(ask);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(vec![crate::wire::WatchFrom {
            sid: "a".into(),
            path: "/p/a.jsonl".into(),
            seq: 7,
        }]);
        rx
    });
    let input = concat!(
        r#"{"id":"w1","cmd":"stream-watch","args":{"sids":["a","b"]}}"#,
        "\n",
        r#"{"id":"w2","cmd":"stream-watch","args":{"sids":"a"}}"#,
        "\n",
        r#"{"id":"w3","cmd":"stream-watch","args":{}}"#,
        "\n",
        r#"{"id":"w4","cmd":"stream-watch","args":{"sids":["a"],"branch":[{"sid":"a","path":"/p/a.jsonl","at":3,"sum":"00ff"},{"sid":"b"}]}}"#,
        "\n",
        r#"{"id":"w5","cmd":"stream-watch","args":{"sids":["a"],"branch":[{"at":3}]}}"#,
        "\n",
    );
    let (tx, mut rx) = chan();
    let h = spawn(
        std::io::Cursor::new(input.as_bytes().to_vec()),
        tx,
        desk,
        crate::platform::fs::Own::for_tests(),
        crate::wire::HelloFlushed::for_tests(),
    );
    h.await.expect("reader task");
    let mut got = std::collections::BTreeMap::new();
    while let Some(f) = rx.recv().await {
        let v: serde_json::Value =
            serde_json::from_str(crate::wire::to_line(&f).unwrap().trim()).unwrap();
        got.insert(v["id"].as_str().unwrap().to_string(), v);
    }
    assert_eq!(
        got["w1"]["data"],
        serde_json::json!({"from": [{"sid": "a", "path": "/p/a.jsonl", "seq": 7}]}),
        "{}",
        got["w1"]
    );
    for id in ["w2", "w3", "w5"] {
        assert_eq!(got[id]["code"], "bad_args", "{id}: {}", got[id]);
    }
    assert_eq!(
        *asked.lock().unwrap(),
        vec![
            WatchAsk {
                sids: vec!["a".to_string(), "b".to_string()],
                branch: vec![]
            },
            WatchAsk {
                sids: vec!["a".to_string()],
                branch: vec![
                    BranchHeld {
                        sid: "a".into(),
                        path: Some("/p/a.jsonl".into()),
                        at: 3,
                        sum: Some("00ff".into())
                    },
                    BranchHeld {
                        sid: "b".into(),
                        ..Default::default()
                    },
                ],
            },
        ]
    );
}

/// 答题 hook 经流挂一条、阻塞等：出口答了 ⇒ hook 那条连接收到决定；hook 那一头走了（连接写半边关掉）⇒ 挂着的那条撤掉。
#[tokio::test]
async fn needs_ask_blocks_until_answered_and_is_withdrawn_when_the_hook_side_leaves() {
    use tokio::io::AsyncWriteExt;
    let watch = || {
        crate::stream::inbound::WatchDesk::new(|_| {
            tokio::sync::oneshot::channel::<Vec<crate::wire::WatchFrom>>().1
        })
    };
    let hook = |sid: &str| {
        serde_json::json!({"id": "h", "cmd": "needs-ask", "args": {"agent": "claude", "hook": {
            "session_id": sid, "hook_event_name": "PermissionRequest",
            "tool_name": "Bash", "tool_input": {"command": "true"}
        }}})
        .to_string()
            + "\n"
    };
    let until_pending = |sid: &'static str| async move {
        for _ in 0..10_000 {
            if let Some(a) = crate::control::asks::pending(sid).pop() {
                return a;
            }
            tokio::task::yield_now().await;
        }
        panic!("没挂上");
    };

    // 答了 ⇒ 决定回到 hook 那条连接。
    let (mut client, server) = tokio::io::duplex(1 << 16);
    let (tx, mut rx) = chan();
    let h = spawn(
        server,
        tx,
        watch(),
        crate::platform::fs::Own::for_tests(),
        crate::wire::HelloFlushed::for_tests(),
    );
    client
        .write_all(hook("t-inbound-1").as_bytes())
        .await
        .unwrap();
    let a = until_pending("t-inbound-1").await;
    crate::control::asks::answer(
        crate::wire::typed::take(
            &serde_json::json!({"sid": "t-inbound-1", "ask": a.id, "choice": "allow"}),
        )
        .unwrap(),
    )
    .unwrap();
    let f = rx.recv().await.expect("应答");
    let line: serde_json::Value = serde_json::from_str(&crate::wire::to_line(&f).unwrap()).unwrap();
    assert_eq!(line["id"], "h");
    assert_eq!(
        line["data"]["decision"]["hookSpecificOutput"]["decision"]["behavior"],
        "allow"
    );
    drop(client);
    h.await.unwrap();

    // hook 那一头走了 ⇒ 撤掉，之后再答回 closed_elsewhere。
    let (mut client, server) = tokio::io::duplex(1 << 16);
    let (tx, _rx) = chan();
    let h = spawn(
        server,
        tx,
        watch(),
        crate::platform::fs::Own::for_tests(),
        crate::wire::HelloFlushed::for_tests(),
    );
    client
        .write_all(hook("t-inbound-2").as_bytes())
        .await
        .unwrap();
    let a = until_pending("t-inbound-2").await;
    drop(client);
    h.await.unwrap();
    assert!(crate::control::asks::pending("t-inbound-2").is_empty());
    assert_eq!(
        crate::control::asks::answer(
            crate::wire::typed::take(
                &serde_json::json!({"sid": "t-inbound-2", "ask": a.id, "choice": "allow"})
            )
            .unwrap()
        )
        .unwrap_err()
        .0,
        "closed_elsewhere"
    );
}

/// 两台之间的枢纽问**这台自己**那一跳：`HUB_INNER` 放行的每一条都要真到那条命令（入参不对回它自己的码），
/// 不能因为它住在哪一档（`BlockingData` · `BlockingOwn`）就回 `unknown_command`。
/// 一条一判：哪条没到，失败里点它的名字。
#[test]
fn the_hub_local_hop_reaches_every_inner_command() {
    let here = super::doors::hub_here(
        &crate::Tz::default(),
        &crate::platform::fs::Own::for_tests(),
    );
    let mut missed = Vec::new();
    for cmd in super::doors::HUB_INNER {
        // 空入参：多数条按自己的码拒；`cc-bus-install` 不看入参，会装进测试档的沙箱家（`src/common/test-home`）。只看它到没到。
        if let Err((code, said)) = here.ask(cmd, serde_json::json!({})) {
            if code == "unknown_command" {
                missed.push(format!("{cmd}（{said}）"));
            }
        }
    }
    assert!(
        missed.is_empty(),
        "枢纽问本机那一跳到不了这几条：{missed:?}"
    );
}

/// 走登记表跑一条阻塞档的命令（同分派那一层）。
fn run_registered(cmd: &str, args: serde_json::Value) -> Outcome {
    let spec = lookup(cmd).unwrap_or_else(|| panic!("`{cmd}` 不在登记表里"));
    let req = Request {
        id: "t".to_string(),
        cmd: cmd.to_string(),
        args,
        within_ms: None,
        view: serde_json::Value::Null,
        tz: Default::default(),
        until: None,
    };
    match invoke(
        spec.run,
        &crate::stream::inbound::hands(Some(&crate::platform::fs::Own::for_tests())),
    ) {
        Some(Prepared::Blocking(f)) => f(req),
        _ => panic!("`{cmd}` 不在阻塞档"),
    }
}

/// 同 [`run_registered`]，异步档的也跑（在一个当场起的运行时上等它）。
fn run_any(cmd: &str, args: serde_json::Value) -> Outcome {
    let spec = lookup(cmd).unwrap_or_else(|| panic!("`{cmd}` 不在登记表里"));
    let req = Request {
        id: "t".to_string(),
        cmd: cmd.to_string(),
        args,
        within_ms: None,
        view: serde_json::Value::Null,
        tz: Default::default(),
        until: None,
    };
    match invoke(
        spec.run,
        &crate::stream::inbound::hands(Some(&crate::platform::fs::Own::for_tests())),
    ) {
        Some(Prepared::Blocking(f)) => f(req),
        Some(Prepared::Async(f)) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f(req)),
        None => panic!("`{cmd}` 是内建档"),
    }
}

/// `history-search` 的入参按类型收：认不得的 `scope` · 多出来的键 · 类型不对 ⇒ `bad_args`（不再悄悄当没给、照搜全部）。
#[test]
fn history_search_refuses_what_its_request_type_does_not_have() {
    for args in [
        serde_json::json!({ "query": "x", "scope": "users" }),
        serde_json::json!({ "query": "x", "limt": 5 }),
        serde_json::json!({ "query": "x", "include_tools": "yes" }),
        serde_json::json!({ "scope": "user" }),
    ] {
        let got = run_registered("history-search", args.clone());
        assert_eq!(
            got.as_ref().err().map(|f| f.code.as_str()),
            Some("bad_args"),
            "入参 {args} 该回 bad_args：{got:?}"
        );
    }
}

/// 帧面 `history-search` 不再把 JSON 摊成选项词交给命令行那一套解：`SUBCOMMAND_OPTIONS` 里不剩那四个词。
#[test]
fn history_search_options_are_not_subcommand_words() {
    for gone in ["--include-tools", "--scope", "--after-ms", "--limit"] {
        assert!(
            !crate::SUBCOMMAND_OPTIONS.contains(&gone),
            "`{gone}` 还在 SUBCOMMAND_OPTIONS 里（帧面入参该直接从类型解）"
        );
    }
}

/// 账号那几条换成 typed 之前钉的线上字节（沙箱里的空家：没有会话 · 没有转发）。
#[test]
fn the_account_reads_keep_their_wire_bytes() {
    let wire = |cmd: &str, args: serde_json::Value| {
        serde_json::to_string(&run_registered(cmd, args).unwrap()).unwrap()
    };
    assert_eq!(
        wire("accounts-sessions", serde_json::json!({})),
        r#"{"lines":[]}"#
    );
    assert_eq!(
        wire("machine-interrupts", serde_json::json!({})),
        r#"{"forwards":0,"liveStreams":0,"relayedMaybe":0,"relayedSessions":0}"#
    );
    assert_eq!(
        wire(
            "machine-interrupts",
            serde_json::json!({ "machine": "box", "appExit": true })
        ),
        r#"{"forwards":0,"liveStreams":0,"relayedMaybe":0,"relayedSessions":0}"#
    );
}

/// 手机也在问的那几条换成 typed 之后，回包的线上字节不变：经登记表跑（同分派那一层），回包按帧里那一份写法（`serde_json::to_string`）
/// 逐字节比手机那一侧收的形状（入参照抄手机发的那一份：`core-claude/.../link/Terminals.kt`）。
#[test]
fn the_replies_the_phone_reads_keep_their_wire_bytes() {
    let entry = crate::control::launch_render::ccm_invocation::argv(
        &crate::platform::paths::installed_ccm_entry().unwrap(),
    );
    let attach = serde_json::json!({
        "agent": "claude",
        "action": { "kind": "attach", "name": "demo-cc" },
        "container": { "kind": "tmux", "name": "demo-cc", "send_into": false },
        "cwd": null,
        "account": { "kind": "base" },
        "ccmSid": null,
        "model": null,
    });
    let got = run_registered("launch-render-cli", attach).expect("launch-render-cli 渲得出");
    assert_eq!(
        serde_json::to_string(&got).unwrap(),
        format!(
            r#"{{"account":null,"cmd":{}}}"#,
            serde_json::to_string(&format!("{entry} -- --attach demo-cc")).unwrap()
        )
    );
    // 终端那三条：经登记表跑一趟手机发的那一份（这台测试档的 tmux 是本进程自己那个空 socket ⇒ 空名单 · 不在名单），
    //   再拿纯构造器喂金样原料 ＋ 几格「有才出」的（没标 agent · 没声明 client · 别的着色属性 · 不带颜色），逐字节比换形之前那一版写出来的。
    let wire = |v: &dyn erased_wire::Wire| v.bytes();
    assert_eq!(
        serde_json::to_string(
            &run_registered("terminals-list", serde_json::json!({ "client": "mobile" })).unwrap()
        )
        .unwrap(),
        r##"{"complete":true,"terminals":[]}"##
    );
    assert_eq!(
        run_registered(
            "terminal-preview",
            serde_json::json!({ "terminal": "tmux-1", "color": false })
        )
        .unwrap_err()
        .code,
        "not_known"
    );
    assert_eq!(
        serde_json::to_string(
            &run_registered(
                "terminal-input",
                serde_json::json!({ "sid": "sid-a", "text": "hi" })
            )
            .unwrap()
        )
        .unwrap(),
        r#"{"result":"refused","said":"终端已不在","why":"not_known"}"#
    );
    use crate::control::terminals as t;
    use crate::control::terminals_wire::SentResult;
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/terminals.golden.json")).unwrap();
    let (mut rows, _) = t::parse_rows(g["terminals-list"]["panes"].as_str().unwrap());
    let clients = t::parse_clients(g["terminals-list"]["clients"].as_str().unwrap());
    rows.extend(t::parse_rows("$4\t%4\t1\t1\tbare\t1\t400\tsid-d\t\t\tclaude\t/s\t\n").0);
    assert_eq!(
        wire(&t::list_reply(&rows, &clients, false, Some("mobile"))),
        r##"{"complete":false,"terminals":[{"can":{"end":true,"input":true},"clients":[{"kind":"terminal-window","last_activity":90,"since":50}],"cwd":"/p","host":"tmux","input":"shared","last_activity":100,"program":"claude","session":{"agent":"claude","sid":"sid-a"},"started_by":{"client":"ccm","mine":true},"state":"running","terminal":"tmux-1-1","title":"demo title","tmux_name":"demo-cc"},{"can":{"end":{"no":"not_yours","said":"手机端内部会话"},"input":{"no":"not_yours","said":"手机端内部会话"}},"clients":[],"cwd":"/r","host":"tmux","input":"shared","last_activity":300,"program":"vim","started_by":{"client":"desktop","mine":false},"state":"running","terminal":"tmux-3","title":"vim","tmux_name":"other"},{"can":{"end":true,"input":true},"clients":[],"cwd":"/q","host":"tmux","input":"shared","last_activity":200,"program":"bash","session":{"agent":"claude","sid":"sid-b"},"started_by":{"client":"mobile","mine":true},"state":"program_exited","terminal":"tmux-2-2","title":"bash","tmux_name":"pipe-x"},{"can":{"end":true,"input":true},"clients":[],"cwd":"/s","host":"tmux","input":"shared","last_activity":400,"program":"claude","session":{"sid":"sid-d"},"started_by":{"client":null,"mine":true},"state":"running","terminal":"tmux-4-4","title":"claude","tmux_name":"bare"}]}"##
    );
    assert_eq!(
        wire(&t::list_reply(&rows, &clients, false, None)),
        r##"{"complete":false,"terminals":[{"can":{"end":true,"input":true},"clients":[{"kind":"terminal-window","last_activity":90,"since":50}],"cwd":"/p","host":"tmux","input":"shared","last_activity":100,"program":"claude","session":{"agent":"claude","sid":"sid-a"},"started_by":{"client":"ccm","mine":true},"state":"running","terminal":"tmux-1-1","title":"demo title","tmux_name":"demo-cc"},{"can":{"end":{"no":"not_yours","said":"手机端内部会话"},"input":{"no":"not_yours","said":"手机端内部会话"}},"clients":[],"cwd":"/r","host":"tmux","input":"shared","last_activity":300,"program":"vim","started_by":{"client":"desktop","mine":false},"state":"running","terminal":"tmux-3","title":"vim","tmux_name":"other"},{"can":{"end":{"no":"not_yours","said":"手机端内部会话"},"input":{"no":"not_yours","said":"手机端内部会话"}},"clients":[],"cwd":"/q","host":"tmux","input":"shared","last_activity":200,"program":"bash","session":{"agent":"claude","sid":"sid-b"},"started_by":{"client":"mobile","mine":false},"state":"program_exited","terminal":"tmux-2-2","title":"bash","tmux_name":"pipe-x"},{"can":{"end":true,"input":true},"clients":[],"cwd":"/s","host":"tmux","input":"shared","last_activity":400,"program":"claude","session":{"sid":"sid-d"},"started_by":{"client":null,"mine":true},"state":"running","terminal":"tmux-4-4","title":"claude","tmux_name":"bare"}]}"##
    );
    let view = t::View {
        text: "\u{1b}[1m\u{1b}[31mline\u{1b}[0m one\n\u{1b}[2;3;4;7;48;5;200;38;2;1;2;3mx\u{1b}[22;23;24;27;39;49my\u{1b}[44;92mz\u{1b}[0m\nplain\n".into(),
        cols: 80,
        rows: 2,
        cursor_x: 3,
        cursor_y: 1,
        cursor_visible: false,
    };
    assert_eq!(
        wire(&t::preview_reply(
            &view,
            true,
            true,
            1_700_000_000,
            "07:13:20"
        )),
        r##"{"capped":true,"captured_at":1700000000,"captured_at_text":"07:13:20","cols":80,"cursor":{"visible":false,"x":3,"y":1},"lines":[{"spans":[{"bold":true,"fg":"red","from":0,"to":4}],"text":"line one"},{"spans":[{"bg":"#ff00d7","dim":true,"fg":"#010203","from":0,"inverse":true,"italic":true,"to":1,"underline":true},{"bg":"blue","fg":"bright-green","from":2,"to":3}],"text":"xyz"},{"spans":[],"text":"plain"}],"rows":2,"screen":"057751a6837d184a","scrollback_lines":1}"##
    );
    assert_eq!(
        wire(&t::preview_reply(
            &view,
            false,
            false,
            1_700_000_000,
            "07:13:20"
        )),
        r##"{"capped":false,"captured_at":1700000000,"captured_at_text":"07:13:20","cols":80,"cursor":{"visible":false,"x":3,"y":1},"lines":[{"text":"line one"},{"text":"xyz"},{"text":"plain"}],"rows":2,"screen":"057751a6837d184a","scrollback_lines":1}"##
    );
    assert_eq!(
        wire(&t::input_reply(SentResult::Delivered, None, None)),
        r##"{"result":"delivered"}"##
    );
    assert_eq!(
        wire(&t::input_reply(SentResult::Unsure, None, None)),
        r##"{"result":"unsure"}"##
    );
    assert_eq!(
        wire(&t::input_reply(
            SentResult::Refused,
            Some(t::Refused::ScreenChanged),
            Some("0000000000000000")
        )),
        r#"{"result":"refused","said":"画面已变","screen":"0000000000000000","why":"screen_changed"}"#
    );
    // kill · launch：经登记表跑一趟（这台测试档的 tmux 是空 socket ⇒ 没有这个会话），再拿纯构造器逐字节比换形之前那一版写出来的。
    assert_eq!(
        run_registered(
            "kill",
            serde_json::json!({ "name": "demo-cc", "client": "mobile" })
        )
        .unwrap_err()
        .code,
        "no_such_session"
    );
    assert_eq!(
        wire(&crate::control::kill::reply("demo-cc", &Default::default())),
        r##"{"bus":{"detail":"","failed":[],"removed":[],"said":null,"unread":null},"killed":true,"session":"demo-cc"}"##
    );
    let removed = crate::control::cc_bus::BusCleanup {
        removed: vec!["p_cc".into(), "q_cc".into()],
        failed: vec![],
        unread: None,
    };
    assert_eq!(
        wire(&crate::control::kill::reply("demo-cc", &removed)),
        r##"{"bus":{"detail":"","failed":[],"removed":["p_cc","q_cc"],"said":"已从 cc-bus 名单注销 p_cc · q_cc","unread":null},"killed":true,"session":"demo-cc"}"##
    );
    assert_eq!(
        run_registered(
            "launch",
            serde_json::json!({ "mode": "send-into", "name": "nope-x", "payload": "true" })
        )
        .unwrap_err()
        .code,
        "no_such_session"
    );
    assert_eq!(
        wire(&crate::control::launch::reply("demo-cc", true, false)),
        r##"{"created":true,"session":"demo-cc","typed":false}"##
    );
    // session-new：手机那一份入参（`core-claude/.../link/Sessions.kt`）经登记表跑；目录不在 ⇒ 失败信封的 `data` 照旧（哪一格 ＋ 选不了号那一形）。
    let e = run_registered(
        "session-new",
        serde_json::json!({ "agent": "claude", "cwd": "/definitely/not/here", "place": "tmux", "local": false, "ticket": "t-1", "prompt": "hi" }),
    )
    .unwrap_err();
    assert_eq!(
        (e.code.as_str(), serde_json::to_string(&e.data).unwrap()),
        (
            "no_dir",
            r##"{"field":"cwd","unavailable":null}"##.to_string()
        )
    );
    let started = crate::control::session_new::SessionNew {
        outcome: crate::control::session_new::SessionNewOutcome::Started,
        session: Some("demo-cc".into()),
        sid: None,
        cmd: None,
        account: None,
        agent: "claude".into(),
        cwd: "/srv/p".into(),
        prompt: Some(crate::control::session_new::FirstSay {
            result: "delivered".into(),
            why: None,
            said: None,
        }),
    };
    assert_eq!(
        wire(&started),
        r##"{"account":null,"agent":"claude","cmd":null,"cwd":"/srv/p","outcome":"started","prompt":{"result":"delivered","said":null,"why":null},"session":"demo-cc","sid":null}"##
    );
}

/// 回包写上线的那一步（帧里同此：先 `to_value` 再 `to_string` ⇒ 每一层的键排好序）。
mod erased_wire {
    pub(super) trait Wire {
        fn bytes(&self) -> String;
    }
    impl<T: serde::Serialize> Wire for T {
        fn bytes(&self) -> String {
            serde_json::to_string(&serde_json::to_value(self).unwrap()).unwrap()
        }
    }
}

/// ★ 终端那三条只收它们真用得上的那几格（`terminal-input` 原先还收一格 `take`「先接管输入」：收了、校验了、然后丢掉，
/// 第二个前端照字段表画了一颗按钮、按下去什么都不变 ⇒ 删了）。多送一格 · 类型不对 ⇒ `bad_args`，在解入参那一步就回（碰 tmux 之前），
/// 多出来的那一格诊断里点名（类型不对的那一种，解码器那一句不带格名，只比码）。
#[test]
fn the_terminal_trio_refuses_fields_it_does_not_use() {
    for (cmd, args, named) in [
        (
            "terminal-input",
            serde_json::json!({ "terminal": "tmux-1", "text": "x", "take": true }),
            "take",
        ),
        (
            "terminal-input",
            serde_json::json!({ "terminal": "tmux-1", "text": "x", "nonsense": 1 }),
            "nonsense",
        ),
        (
            "terminal-input",
            serde_json::json!({ "terminal": "tmux-1", "text": "x", "enter": "yes" }),
            "",
        ),
        (
            "terminal-input",
            serde_json::json!({ "terminal": "tmux-1", "text": "x", "seen_screen": 1 }),
            "",
        ),
        (
            "terminal-preview",
            serde_json::json!({ "terminal": "tmux-1", "scrollback": -1 }),
            "",
        ),
        (
            "terminal-preview",
            serde_json::json!({ "terminal": "tmux-1", "color": "no" }),
            "",
        ),
        ("terminals-list", serde_json::json!({ "client": 1 }), ""),
        (
            "terminals-list",
            serde_json::json!({ "who": "mobile" }),
            "who",
        ),
    ] {
        let (e, diag) = crate::common::contract::tests::diag(|| {
            run_registered(cmd, args.clone()).expect_err("多送 / 类型不对的那一格该被拒")
        });
        assert_eq!(e.code, "bad_args", "{cmd} {args}：{e:?}");
        assert!(
            diag.contains(named),
            "{cmd} {args}：诊断没点名那一格：{diag}"
        );
    }
}

/// 会话族换成 typed 的那几条：多一个键 · 少一个必填 · 类型不对 ⇒ `bad_args`，在解入参那一步就回（不往下读 tmux / 盘）。
#[test]
fn the_session_family_refuses_what_its_request_types_do_not_have() {
    for (cmd, args) in [
        (
            "session-new-dir",
            serde_json::json!({ "cwd": "/x", "extra": 1 }),
        ),
        ("session-new-dir", serde_json::json!({ "forkOf": "s-1" })),
        (
            "sessions-where",
            serde_json::json!({ "sids": ["s-1"], "extra": 1 }),
        ),
        ("sessions-stop", serde_json::json!({ "sids": "s-1" })),
        (
            "sessions-start",
            serde_json::json!({ "mode": "tmux", "local": true, "agent": "claude", "items": [{ "sid": "s-1", "cwd": "/w", "extra": 1 }] }),
        ),
        (
            "sessions-start",
            serde_json::json!({ "mode": "screen", "local": true, "agent": "claude", "items": [] }),
        ),
        (
            "session-terminals",
            serde_json::json!({ "sid": "s-1", "extra": 1 }),
        ),
        (
            "terminal-processes",
            serde_json::json!({ "terminals": [{ "ssh": null, "activity": null, "extra": 1 }] }),
        ),
        (
            "launch-local",
            serde_json::json!({ "agent": "claude", "action": { "kind": "new" }, "x": 1 }),
        ),
        (
            "session-say",
            serde_json::json!({ "sid": "s-1", "text": "hi", "enter": false }),
        ),
        ("session-say", serde_json::json!({ "sid": "s-1" })),
        (
            "session-permission",
            serde_json::json!({ "sid": "s-1", "mode": "plan", "key": "shift-tab" }),
        ),
        ("session-permission", serde_json::json!({ "sid": "s-1" })),
        (
            "session-commands",
            serde_json::json!({ "sid": "s-1", "cwd": "/w" }),
        ),
        ("session-commands", serde_json::json!({})),
        (
            "session-model",
            serde_json::json!({ "sid": "s-1", "model": "opus", "effort": "high" }),
        ),
        ("session-model", serde_json::json!({ "sid": "s-1" })),
        ("kill", serde_json::json!({ "name": "demo-cc", "extra": 1 })),
        ("kill", serde_json::json!({ "name": 1 })),
        (
            "launch",
            serde_json::json!({ "mode": "send-into", "name": "x", "payload": "y", "extra": 1 }),
        ),
        (
            "launch",
            serde_json::json!({ "mode": "send-into", "name": "x", "cwd": 5, "payload": "y" }),
        ),
        (
            "session-new",
            serde_json::json!({ "agent": "claude", "cwd": "/x", "place": "tmux", "local": false, "extra": 1 }),
        ),
        (
            "session-new",
            serde_json::json!({ "agent": "claude", "cwd": "/x", "place": "tmux", "local": false, "rotation": "c1" }),
        ),
        (
            "session-new-facts",
            serde_json::json!({ "forkOf": "s-1", "extra": 1 }),
        ),
        ("session-new-facts", serde_json::json!({ "at": 1 })),
        (
            "session-fork",
            serde_json::json!({ "sid": "s-1", "uuid": "u-1", "extra": 1 }),
        ),
        (
            "session-fork",
            serde_json::json!({ "sid": 1, "uuid": "u-1" }),
        ),
        (
            "session-restart",
            serde_json::json!({ "sid": "s-1", "cwd": "/w", "compact_first": false, "compact_within_ms": 0, "arrive_within_ms": 0, "local": false, "agent": "claude", }),
        ),
        (
            "session-restart",
            serde_json::json!({ "sid": "s-1", "cwd": "/w", "account": "work", "compact_first": "no", "compact_within_ms": 0, "arrive_within_ms": 0, "local": false, "agent": "claude", }),
        ),
    ] {
        let got = run_any(cmd, args.clone());
        assert_eq!(
            got.as_ref().err().map(|f| f.code.as_str()),
            Some("bad_args"),
            "{cmd} 入参 {args} 该回 bad_args：{got:?}"
        );
    }
}

/// 账号库那一族的入参按类型收：多一个键 · 缺必填 · 类型不对 · 不收入参的那两条带了键 ⇒ `bad_args`，下层一个字节不碰。
#[test]
fn the_account_library_refuses_what_its_request_types_do_not_have() {
    for (cmd, args) in [
        (
            "accounts-init",
            serde_json::json!({ "name": "z", "extra": 1 }),
        ),
        ("accounts-init", serde_json::Value::Null),
        (
            "accounts-add",
            serde_json::json!({ "name": "z", "kind": "other" }),
        ),
        (
            "accounts-add",
            serde_json::json!({ "name": "z", "kind": "subscription", "loginCmd": "x" }),
        ),
        ("accounts-remove", serde_json::json!({ "name": 3 })),
        ("accounts-set-default", serde_json::json!({})),
        ("accounts-repair", serde_json::json!({ "dryRun": "yes" })),
        ("accounts-rollback", serde_json::json!({ "backup": 1 })),
        ("accounts-verify", serde_json::json!({ "x": 1 })),
        (
            "accounts-login-cmd",
            serde_json::json!({ "name": "z", "extra": 1 }),
        ),
        ("accounts-mcp-read", serde_json::json!({ "x": 1 })),
        ("accounts-mcp-remove", serde_json::json!({})),
        (
            "accounts-mcp-pick",
            serde_json::json!({ "name": "s", "from": 1 }),
        ),
        ("accounts-mcp-sync", serde_json::json!({ "on": "yes" })),
        ("login-finish", serde_json::json!({ "job": "j" })),
        (
            "login-finish",
            serde_json::json!({ "job": "j", "pasted": "x", "extra": 1 }),
        ),
        ("login-cancel", serde_json::json!({})),
        ("manual-list", serde_json::json!({ "x": 1 })),
    ] {
        let got = run_registered(cmd, args.clone()).expect_err(&format!("{cmd} {args} 本该被拒"));
        assert_eq!(got.code, "bad_args", "{cmd} {args}：{got:?}");
    }
}
