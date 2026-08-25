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
//! {"agent":"…","key":"…","event":<上游 data: 后面那段，原样>}
//! ```
//!
//! **没有 `t_ns`** —— 理由与它丢掉了什么，见 `super` 的头注㈡。
//! `event` 的值是上游 `data:` 后面那段**原样**，中转不解析它的任何字段
//! （`K9` 裁定二：内容一律原样透传，下游拿到的就是上游方言）。

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
        let seq = self
            .seq
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let line = format!(
            "{{\"__meta__\":{{\"source\":\"relay\",\"proto\":\"passthrough-v0\",\"agent\":{},\"key\":{},\"seq\":{}}}}}\n",
            json_str(agent),
            json_str(key),
            seq
        );
        self.write_line(&line);
        seq
    }

    pub(crate) fn event(&self, agent: &str, key: &str, payload: &str) {
        let line = format!(
            "{{\"agent\":{},\"key\":{},\"event\":{}}}\n",
            json_str(agent),
            json_str(key),
            payload
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
        assert!(lines[1].contains("\"event\":{\"type\":\"x\"}"));
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

    #[test]
    fn json_escaping_closes_the_second_door() {
        assert_eq!(json_str("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_str("a\nb"), "\"a\\u000ab\"");
    }
}
