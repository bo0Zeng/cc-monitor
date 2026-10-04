//! 超长行整行丢弃时，从行首那一小段里抠出信封的 `id`（好让 `line_too_long` 带回请求方的 id）。

/// 超长行只看行首这么多字节去找 `id`。
///
/// 整行已经不进内存（[`MAX_LINE_BYTES`] 头注那条「读的时候就生效」），这里多留的只有这一小段，
/// 与行长无关。4 KiB 远够：monitor 发号最长 75 字节、且 `id` 是信封的第一个键
/// （`inbound_client::encode_request` 的字段顺序）；排在它前面的键再长也只是「抠不出 ⇒ 空串」，回到旧行为。
pub const ID_SNIFF_BYTES: usize = 4 * 1024;

/// 从一行**开头的一段**里尽力抠出信封的 `id`（顶层对象里、值是字符串的那一个）。
///
/// 为什么要它：超长行整行丢弃，此前回的 `line_too_long` 带**空** `id` ⇒ 发这一行的调用方等不到
/// 自己的应答，要熬满它自己的预算才超时，看到的是「超时」而不是真原因。
///
/// ⚠ 只认**顶层**的 `"id"`：排在前面的键值原样跳过（字符串 / 数 / 嵌套对象与数组都认得），
/// 嵌套对象里的 `"id"` 不算。段不完整、形状不对、`id` 不是字符串 ⇒ `None`（调用方回空串，即旧行为）。
/// ⚠ 它不是 JSON 解析器，也不校验这一行别处合不合法 —— 这一行本来就要被丢弃，只借它的 `id` 回话。
pub(super) fn sniff_id(head: &[u8]) -> Option<String> {
    // 结构字节按值写（不写成字符字面量）：本仓有几条按文本扫源码的判据，引号与大括号的字面量会搅乱它们的配平。
    const QUOTE: u8 = 0x22;
    const BACKSLASH: u8 = 0x5C;
    const OPEN_OBJ: u8 = 0x7B;
    const CLOSE_OBJ: u8 = 0x7D;
    const OPEN_ARR: u8 = 0x5B;
    const CLOSE_ARR: u8 = 0x5D;
    const COMMA: u8 = 0x2C;
    const COLON: u8 = 0x3A;
    let at = |k: usize| head.get(k).copied();
    let ws = |mut i: usize| {
        while head.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        i
    };
    // 一个字符串（`i` 指在开头那个引号上）⇒ 回收尾引号之后的位置。
    let string_end = |i: usize| -> Option<usize> {
        if at(i) != Some(QUOTE) {
            return None;
        }
        let mut k = i + 1;
        loop {
            match at(k)? {
                BACKSLASH => k += 2,
                QUOTE => return Some(k + 1),
                _ => k += 1,
            }
        }
    };
    // 跳过一个值 ⇒ 回它之后的位置。
    let value_end = |i: usize| -> Option<usize> {
        match at(i)? {
            QUOTE => string_end(i),
            OPEN_OBJ | OPEN_ARR => {
                let mut depth = 0usize;
                let mut k = i;
                loop {
                    match at(k)? {
                        QUOTE => {
                            k = string_end(k)?;
                            continue;
                        }
                        OPEN_OBJ | OPEN_ARR => depth += 1,
                        CLOSE_OBJ | CLOSE_ARR => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(k + 1);
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
            }
            _ => {
                let mut k = i;
                while !matches!(at(k)?, COMMA | CLOSE_OBJ | CLOSE_ARR)
                    && !head[k].is_ascii_whitespace()
                {
                    k += 1;
                }
                Some(k)
            }
        }
    };
    let mut i = ws(0);
    if at(i) != Some(OPEN_OBJ) {
        return None;
    }
    i += 1;
    loop {
        i = ws(i);
        let key_end = string_end(i)?;
        let key: String = serde_json::from_slice(&head[i..key_end]).ok()?;
        i = ws(key_end);
        if at(i) != Some(COLON) {
            return None;
        }
        i = ws(i + 1);
        if key == "id" {
            let end = string_end(i)?;
            return serde_json::from_slice(&head[i..end]).ok();
        }
        i = ws(value_end(i)?);
        if at(i) != Some(COMMA) {
            return None;
        }
        i += 1;
    }
}
