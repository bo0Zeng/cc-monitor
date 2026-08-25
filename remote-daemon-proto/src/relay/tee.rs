//! tee：把响应体里的 SSE 事件抄一份出去。**只抄响应体，永不抄请求头。**
//!
//! # 为什么落 stdout 而不是文件
//!
//! 见 `super` 的头注㈠：只读护栏的默认层把标准库里「新建文件」的每一种写法都禁掉了，
//! 而绕开它的三条路都是「让护栏变瞎」。⇒ 本轮 tee 写 stdout，要落文件由启动方重定向。
//!
//! # 行格式（契约是**这份文件**，不是代码）
//!
//! 每个响应先一行 meta，其后每个 SSE 事件一行：
//!
//! ```text
//! {"__meta__":{"source":"relay","proto":"passthrough-v0","agent":"…","key":"…","seq":N}}
//! {"agent":"…","key":"…","event":"<上游 data: 后面那段，转义成一个 JSON 串>"}
//! ```
//!
//! **没有 `t_ns`** —— 理由与它丢掉了什么，见 `super` 的头注㈡。
//!
//! ## ⚠ 订正〔回修轮之四 08-25，D1 `重要-7` / D2 `阻-1(D2)`〕：`event` 是**串**，不是裸值
//!
//! 先前这里写的是 `"event":<上游 data: 后面那段，**原样**>` —— 那句话把「原样」落在了
//! **JSON 结构**这一层，而上游的字节是**敌手可控**的：
//! - 上游发 `data: 1,"agent":"evil"` ⇒ 整行成了
//!   `{"agent":"realA","key":"sid-AAA","event":1,"agent":"evil"}`，
//!   而重复键在 `serde_json` / `json.loads` 两侧都是 **last-wins** ⇒ **路由键被上游改写**。
//! - 上游发一段不是 JSON 的文本（SSE 的 `data:` 本来就允许任意文本）⇒ 整行**不可解析**，
//!   而件计划 `DoD-3㈠` 的 acceptor 逐字要「其后**每行可解析**」。
//!
//! ⇒ 今天 `event` 的值是**一个 JSON 串**：上游那一段逐字节保住（`K9` 裁定二「内容一律原样
//! 透传」在**内容**这一层照旧成立，中转仍然不解析它的任何字段），但它**不再参与本行的结构**。
//! 下游要拿里面的字段，自己对这个串再解一次。
//! 死值验与射程见 `an_upstream_payload_cannot_break_out_of_the_event_field` 与件文件 §8.18.1。

use std::io::Write;

/// SSE 拆行器 —— 增量喂字节，吐出 `data:` 行的载荷。
///
/// 它只认 SSE 的**分帧**（行、`data:` 前缀），不认里面是什么。
#[derive(Default)]
pub(crate) struct SseSplitter {
    partial: Vec<u8>,
}

impl SseSplitter {
    pub(crate) fn feed(&mut self, decoded: &[u8]) -> Vec<String> {
        self.partial.extend_from_slice(decoded);
        let mut out = Vec::new();
        loop {
            let Some(pos) = self.partial.iter().position(|b| *b == b'\n') else {
                break;
            };
            let line: Vec<u8> = self.partial.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            let Some(payload) = line.strip_prefix("data:") else {
                continue;
            };
            let payload = payload.trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            out.push(payload.to_string());
        }
        out
    }
}

/// tee 的落点。一个进程只有一个，**跨连接共享**，靠锁保证一行不被另一行劈开。
pub(crate) struct TeeSink {
    inner: std::sync::Mutex<Box<dyn Write + Send>>,
    seq: std::sync::atomic::AtomicU64,
}

impl TeeSink {
    pub(crate) fn new(w: Box<dyn Write + Send>) -> Self {
        Self {
            inner: std::sync::Mutex::new(w),
            seq: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub(crate) fn to_stdout() -> Self {
        Self::new(Box::new(std::io::stdout()))
    }

    /// 一个响应开头写一行 meta，返回这一响应的序号。
    pub(crate) fn open(&self, agent: &str, key: &str) -> u64 {
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let line = format!(
            "{{\"__meta__\":{{\"source\":\"relay\",\"proto\":\"passthrough-v0\",\"agent\":{},\"key\":{},\"seq\":{}}}}}\n",
            json_str(agent),
            json_str(key),
            seq
        );
        self.write_line(&line);
        seq
    }

    /// 写一行事件。
    ///
    /// ★★ **`payload` 必须过 `json_str`**（回修轮之四 08-25，D1 `重要-7` / D2 `阻-1(D2)`）：
    /// 它是**上游**给的字节，敌手可控。先前这里把它**原样**拼进 JSON，实测两条后果 ——
    /// 上游发 `data: 1,"agent":"evil"` ⇒ 整行变成
    /// `{"agent":"realA","key":"sid-AAA","event":1,"agent":"evil"}`，而 `serde_json` /
    /// `json.loads` 两侧解重复键都是 **last-wins** ⇒ **这一行的路由键被上游改写**；
    /// 上游发一段不是 JSON 的文本 ⇒ 整行**不可解析**，而 `DoD-3㈠` 的 acceptor
    /// 逐字要「其后每行可解析」。判据见 `an_upstream_payload_cannot_break_out_of_the_event_field`。
    pub(crate) fn event(&self, agent: &str, key: &str, payload: &str) {
        let line = format!(
            "{{\"agent\":{},\"key\":{},\"event\":{}}}\n",
            json_str(agent),
            json_str(key),
            json_str(payload)
        );
        self.write_line(&line);
    }

    fn write_line(&self, line: &str) {
        let Ok(mut w) = self.inner.lock() else {
            return;
        };
        // tee 坏了**不许**拖垮转发：中转的首要职责是把字节送到 CLI。
        let _ = w.write_all(line.as_bytes());
        let _ = w.flush();
    }
}

/// 最小 JSON 串转义 —— 路由键已被白名单收窄过，这里是第二道。
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sse_data_lines_across_arbitrary_split_points() {
        let wire = b"event: message_start\ndata: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: [DONE]\n\n";
        let mut s = SseSplitter::default();
        let mut got = Vec::new();
        for b in wire.iter() {
            got.extend(s.feed(&[*b]));
        }
        // 期望值手写：两个事件，`[DONE]` 与非 data 行都不算。
        assert_eq!(got, vec!["{\"a\":1}".to_string(), "{\"b\":2}".to_string()]);
    }

    #[test]
    fn a_half_delivered_line_is_not_emitted_until_it_completes() {
        let mut s = SseSplitter::default();
        // ⚠ 这里刻意用中括号载荷：只读护栏的剥法按**大括号配平**，
        // 测试串里出现不配对的大括号会把剥除边界带偏（该护栏头注逐字警告过这个形状）。
        assert!(s.feed(b"data: [1,").is_empty(), "半行不许吐");
        assert_eq!(s.feed(b"2]\n"), vec!["[1,2]".to_string()]);
    }

    /// 收集到内存里的 tee，用来在测试里读回写了什么。
    #[derive(Clone, Default)]
    struct MemSink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl Write for MemSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("lock").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn meta_line_then_event_lines() {
        let mem = MemSink::default();
        let sink = TeeSink::new(Box::new(mem.clone()));
        let seq = sink.open("agentA", "sid-AAA");
        assert_eq!(seq, 0);
        sink.event("agentA", "sid-AAA", "{\"type\":\"x\"}");
        let raw = mem.0.lock().expect("lock").clone();
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

    #[test]
    fn seq_is_per_process_and_monotonic() {
        let sink = TeeSink::new(Box::new(MemSink::default()));
        assert_eq!(sink.open("a", "k1"), 0);
        assert_eq!(sink.open("b", "k2"), 1);
        assert_eq!(sink.open("a", "k1"), 2);
    }

    /// ★★ **上游内容是敌手可控的** —— `data:` 后面那一段原样进这一行。
    ///
    /// 本条钉的是 `DoD-3㈠` acceptor 逐字那半句：「其后**每行可解析**」；
    /// 顺带钉住**路由键** —— `agent` / `key` 是下游按会话分流的唯一依据，
    /// 而 `serde_json` / `json.loads` 两侧解重复键都是 **last-wins**
    /// ⇒ 上游只要把 `,"agent":"…"` 拼进来，就能改写这一行的落点。
    ///
    /// 分母 = 我列出的这 **4** 形，每一形都是上游**一行 `data:`** 就发得出来的。
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
        ];
        for payload in hostile {
            let mem = MemSink::default();
            let sink = TeeSink::new(Box::new(mem.clone()));
            sink.event("realA", "sid-AAA", payload);
            let raw = mem.0.lock().expect("lock").clone();
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

    #[test]
    fn json_escaping_closes_the_second_door() {
        assert_eq!(json_str("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_str("a\nb"), "\"a\\u000ab\"");
    }
}
