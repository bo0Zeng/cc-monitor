use super::*;

/// 「这条判据不测上限那一格」的写法：给一个**永远触发不了**的上限
/// （理由同 `http1.rs` 那份 `NO_CAP`：期望值不许拿被测常量算）。
const NO_CAP: usize = usize::MAX;

#[test]
fn splits_sse_data_lines_across_arbitrary_split_points() {
    let wire = b"event: message_start\ndata: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: [DONE]\n\n";
    let mut s = SseSplitter::default();
    let mut got = Vec::new();
    for b in wire.iter() {
        got.extend(s.feed(&[*b], NO_CAP));
    }
    // 期望值手写：两个事件，`[DONE]` 与非 data 行都不算。
    assert_eq!(got, vec!["{\"a\":1}".to_string(), "{\"b\":2}".to_string()]);
}

#[test]
fn a_half_delivered_line_is_not_emitted_until_it_completes() {
    let mut s = SseSplitter::default();
    // ⚠ 这里刻意用中括号载荷：只读护栏的剥法按**大括号配平**，
    // 测试串里出现不配对的大括号会把剥除边界带偏（该护栏头注逐字警告过这个形状）。
    assert!(s.feed(b"data: [1,", NO_CAP).is_empty(), "半行不许吐");
    assert_eq!(s.feed(b"2]\n", NO_CAP), vec!["[1,2]".to_string()]);
}

/// ★ `TEE_DECODE_CAP` 在 `SseSplitter` 这一侧的那一格〔回修轮之五 08-25，`阻-1(D3)` 同职面〕。
///
/// 分母 = 我列出的这 **3** 形：①超长半行被丢掉且**计数**；②它**不连坐**后面的完整行；
/// ③没超上限时**一个字节都不丢**（非空对照 —— 没有它，「丢了 N 字节」可能只是它见谁丢谁）。
#[test]
fn an_endless_sse_line_is_dropped_and_counted_instead_of_growing_forever() {
    const CAP: usize = 64; // 手写字面量，不引生产常量
    let mut s = SseSplitter::default();

    // ③ 非空对照先做：正常的行一个字节都不许丢。
    assert_eq!(
        s.feed(b"data: {\"a\":1}\n", CAP),
        vec!["{\"a\":1}".to_string()]
    );
    assert_eq!(s.take_dropped(), 0, "正常的行不许丢");

    // ① 一条永不换行的超长行：吐不出东西，且**丢的字节数被记下来**。
    let long = vec![b'x'; CAP * 2];
    assert!(s.feed(&long, CAP).is_empty(), "超长的半行不许吐出来");
    let dropped = s.take_dropped();
    assert!(
        dropped >= CAP as u64,
        "丢了 {dropped} 字节 —— 超上限的半行必须被丢掉**并计数**（不许静默）"
    );
    assert_eq!(s.take_dropped(), 0, "取走之后账要清零，不许重复报");

    // ② 不连坐：紧跟其后的完整行照样拆得出来。
    assert_eq!(
        s.feed(b"tail\ndata: {\"b\":2}\n", CAP),
        vec!["{\"b\":2}".to_string()],
        "丢掉超长那一行之后，后面的完整行必须照常吐"
    );
}

/// 收集到内存里的 tee，用来在测试里读回写了什么。
///
/// ★★ **它必须「能等」**〔回修轮之五 08-25，`阻-2(D3)` 的连带〕：
/// 今天 tee 的写落在**另一条线程**上（那正是 `阻-2` 的修法）⇒ `event()` 返回**不代表已经写完**。
/// 判据写完就读缓冲区 = 在读一个还没写完的东西，而「tee 是空的」与「还没写完」
/// 在断言里**长得一模一样** ⇒ 那会是一条会随机说谎的判据。
/// ⇒ 每写一行往通道投一条，判据用 `wait_lines` 等够行数；**等不到就当红**，不许当绿。
///
/// ⚠ 等待逻辑刻意住在 `mod tests` 里、**不做成 `TeeSink` 的 `#[cfg(test)]` 方法**：
/// `no_timer_guard` 的 `production_code()` 只剥 `#[cfg(test)] mod`，**不剥单个 `#[cfg(test)]` 函数**
/// —— 我第一版就是那么写的，当场被它逮到两条红
///（`daemon_production_code_has_no_periodic_wakeups` 点名 `relay/tee.rs` 里的 `sleep(`，
/// 以及 `every_duration_use_is_registered_as_non_timer` 说「生产段 `Duration::from_*` 有 2 处」）。
/// 那两条红是**对的**：按那把尺子，我那个方法确实算生产段。
#[derive(Clone)]
struct MemSink {
    buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    tick: std::sync::mpsc::Sender<()>,
}
impl Write for MemSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.lock().expect("lock").extend_from_slice(buf);
        let _ = self.tick.send(());
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 造一个「能等」的 tee 落点：返回 `(TeeSink, 读回字节的句柄, 等行数用的接收端)`。
fn waitable_sink() -> (
    TeeSink,
    std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    std::sync::mpsc::Receiver<()>,
) {
    let (tick, rx) = std::sync::mpsc::channel();
    let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = TeeSink::new(Box::new(MemSink {
        buf: std::sync::Arc::clone(&buf),
        tick,
    }));
    (sink, buf, rx)
}

/// 等写线程写够 `n` 行。**等不到就 panic**（不许把「还没写完」读成「tee 是空的」）。
///
/// 4 秒对回环内存写来说宽得离谱（实测这几条都在毫秒量级收工）⇒ 它只把**挂住**换成**红**。
fn wait_lines(rx: &std::sync::mpsc::Receiver<()>, n: usize) {
    for i in 0..n {
        rx.recv_timeout(std::time::Duration::from_secs(4))
            .unwrap_or_else(|e| panic!("等 tee 的第 {} 行没等到：{e}", i + 1));
    }
}

#[test]
fn meta_line_then_event_lines() {
    let (sink, buf, rx) = waitable_sink();
    let seq = sink.open("agentA", "acctA", "sid-AAA");
    assert_eq!(seq, 0);
    sink.event("agentA", "acctA", "sid-AAA", "{\"type\":\"x\"}");
    wait_lines(&rx, 2);
    let raw = buf.lock().expect("lock").clone();
    let text = String::from_utf8(raw).expect("utf8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("\"__meta__\""), "首行必须是 meta");
    assert!(lines[0].contains("\"seq\":0"));
    // `event` 是**串**（订正见本文件头注）：上游那一段逐字保住，但不参与本行结构。
    assert!(
        lines[1].contains("\"event\":\"{\\\"type\\\":\\\"x\\\"}\""),
        "event 必须是转义过的串：{}",
        lines[1]
    );
    // ★ 这一条钉的是 `裁-3`：第一刀**不带** t_ns。带上它 = 动了零定时器护栏。
    assert!(!text.contains("t_ns"), "本刀不带 t_ns，见 super 头注㈡");
}

/// ⚠ **改名**〔回修轮之四 08-25，承接 D2 `建议-8`〕：旧名 `seq_is_**per_process**_and_monotonic`
/// 里的「**per process**」是假的 —— `seq` 是 `TeeSink` 的**实例字段**，本条自己
/// `TeeSink::new(...)` 造**一个**再断 0/1/2 ⇒ 它只证了**实例内**单调。
/// 「一个进程一份」今天靠的是生产段 `run_with` 里那**唯一一个** `TeeSink::to_stdout()`
/// 调用点，而**那一点没有任何判据钉着**（登记住址件文件 §8.18.9 `判不了-单例`）。
/// ⇒ 名字只说它证得了的那一半。
#[test]
fn seq_is_monotonic_within_one_sink() {
    let (sink, _buf, _rx) = waitable_sink();
    assert_eq!(sink.open("a", "acctA", "k1"), 0);
    assert_eq!(sink.open("b", "acctB", "k2"), 1);
    assert_eq!(sink.open("a", "acctA", "k1"), 2);
}

/// ★★ **上游内容是敌手可控的** —— `data:` 后面那一段原样进这一行。
///
/// 本条钉的是 `DoD-3㈠` acceptor 逐字那半句：「其后**每行可解析**」；
/// 顺带钉住**路由键** —— `agent` / `key` 是下游按会话分流的唯一依据，
/// 而 `serde_json` / `json.loads` 两侧解重复键都是 **last-wins**
/// ⇒ 上游只要把 `,"agent":"…"` 拼进来，就能改写这一行的落点。
///
/// 分母 = 我列出的这 **5** 形，每一形都是上游**一行 `data:`** 就发得出来的。
/// 〔来历：D1 `重要-7` → D2 `阻-1(D2)`。这条判据是它今天的落点，登记住址见件文件 §8.18.1。〕
#[test]
fn an_upstream_payload_cannot_break_out_of_the_event_field() {
    // 每一形 = 上游 `data:` 后面那一段的**原样字节**。期望值全是手写字面量。
    let hostile = [
        // ㈠ 改写路由键：拼一段合法 JSON 片段，把 `agent` 顶掉
        "1,\"agent\":\"evil\"",
        // ㈡ 整行不再是合法 JSON：上游发的根本不是 JSON（SSE 的 data: 允许任意文本）
        "oops",
        // ㈢ 行尾多出垃圾（⚠ 刻意用**中括号**：只读护栏的剥法按大括号配平，
        //    测试串里写不配对的大括号会把剥除边界带偏 —— 上面 `:141` 那条注释
        //    逐字警告过这一形，而我这条夹具的第一版就是这么把它打红的）
        "[1,2]]",
        // ㈣ 裸控制字符：SSE 拆行器只吃掉 \r 与 \n，别的控制字符原样穿过来
        "{\"a\":\"x\u{1}y\"}",
        // ㈤ 以**反斜杠**结尾：不转义就会把这一行的结束引号吃掉
        //（这一形是回修轮之四复扫补的 —— 前四形都不含 `\`）
        "tail\\",
    ];
    for payload in hostile {
        let (sink, buf, rx) = waitable_sink();
        sink.event("realA", "realAcct", "sid-AAA", payload);
        wait_lines(&rx, 1);
        let raw = buf.lock().expect("lock").clone();
        let text = String::from_utf8(raw).expect("utf8");
        let line = text.trim_end_matches('\n');
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("tee 行必须可解析（DoD-3㈠）：{line:?} ⇒ {e}"));
        assert_eq!(v["agent"], "realA", "上游内容改写了路由键 agent：{line:?}");
        assert_eq!(v["key"], "sid-AAA", "上游内容改写了路由键 key：{line:?}");
        // ★ 原样透传**没丢**：`event` 的值逐字节还是上游那一段（`K9` 裁定二）。
        assert_eq!(v["event"], payload, "上游那一段必须逐字保住：{line:?}");
    }
}

/// ★★ **队列满了要丢，但丢必须说**〔回修轮之五 08-25，`阻-2(D3)` 的代价那一面〕。
///
/// # 为什么这一条是承重的
///
/// `阻-2` 的修法把「阻塞的写」换成了「有界队列」，代价是**队列满时要丢行**。
/// 而 `DoD-3㈠` acceptor 要的是「`event` 数 == 上游事件数」——
/// **静默地丢**会让那笔账永远对不上，还查不出是谁丢的。
/// ⇒ 这一条钉的就是那句代价：丢了要在流里留下 `__dropped__`，**说清丢了几行**。
///
/// # 量法
///
/// 落点先卡住（写第一行时挡在门闩上），趁它卡着灌 `TEE_QUEUE_LINES + 64` 行 ——
/// 队列必然满、必然丢。然后放行，再写一行，读回全文。
///
/// 非空对照：`__dropped__` 里的 `lines` 必须 **> 0**（不是「有这个词就算」）。
#[test]
fn a_full_queue_drops_lines_but_says_so() {
    let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
    let (tick, rx) = std::sync::mpsc::channel();
    let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    struct Gated {
        buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
        tick: std::sync::mpsc::Sender<()>,
        gate: Option<std::sync::mpsc::Receiver<()>>,
    }
    impl Write for Gated {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            // 第一行卡在门闩上；放行之后正常写。
            if let Some(g) = self.gate.take() {
                let _ = g.recv();
            }
            self.buf.lock().expect("lock").extend_from_slice(b);
            let _ = self.tick.send(());
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let sink = TeeSink::new(Box::new(Gated {
        buf: std::sync::Arc::clone(&buf),
        tick,
        gate: Some(gate_rx),
    }));

    // 灌到必然溢出。**期望值不拿 `TEE_QUEUE_LINES` 算**，只断「丢了 > 0 行」。
    for i in 0..(TEE_QUEUE_LINES + 64) {
        sink.event("agentA", "acctA", "sid-AAA", &format!("{{\"i\":{i}}}"));
    }
    gate_tx.send(()).expect("放行");
    // 等到那行补报出来（等不到就红，不许把「还没写完」读成「没有报」）。
    // `__dropped__` 由**写线程**在写完一行之后补 ⇒ 放行之后它自己会出来。
    let mut text = String::new();
    while rx.recv_timeout(std::time::Duration::from_secs(4)).is_ok() {
        text = String::from_utf8_lossy(&buf.lock().expect("lock").clone()).to_string();
        if text.contains("__dropped__") {
            break;
        }
    }
    assert!(
        text.contains("__dropped__"),
        "队列溢出必须在流里留下 `__dropped__`，**不许静默丢**：{}",
        &text[..text.len().min(400)]
    );
    let note = text
        .lines()
        .find(|l| l.contains("__dropped__"))
        .expect("那一行");
    let v: serde_json::Value = serde_json::from_str(note)
        .unwrap_or_else(|e| panic!("`__dropped__` 行必须可解析（DoD-3㈠）：{note:?} ⇒ {e}"));
    let lines = v["__dropped__"]["lines"].as_u64().expect("lines 是个数");
    assert!(lines > 0, "非空对照：报出来的丢行数必须 > 0：{note}");
}

/// `json_str` 的三条转义规则各一格。**期望值全是手写字面量。**
///
/// ⚠ **补一格**〔回修轮之四 08-25 复扫补的〕：先前只有 `"` 与控制字符两格，
/// **反斜杠那一格没有** —— 而 `\` 恰恰是最要命的一形：一段以 `\` 结尾的内容
/// 不转义就会把**结束引号**吃掉，整行结构塌掉。本轮起 `event` 的载荷（**上游给的、
/// 敌手可控的字节**）正是走这个函数 ⇒ 这一格从「顺手补的」变成了承重的。
/// 分母 = `json_str` 的 `match` 里那 **3** 条规则（`"` · `\` · `< 0x20`）。
#[test]
fn json_escaping_closes_the_second_door() {
    assert_eq!(json_str("a\"b"), "\"a\\\"b\"");
    assert_eq!(json_str("a\nb"), "\"a\\u000ab\"");
    assert_eq!(json_str("a\\b"), "\"a\\\\b\"", "反斜杠必须转义");
    assert_eq!(
        json_str("x\\"),
        "\"x\\\\\"",
        "结尾的反斜杠不许把结束引号吃掉"
    );
}
