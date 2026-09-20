//! 〔步 24f · 2026-09-20〕**路径的原始字节** —— `files-read` 这一族的地基。
//!
//! # 它解掉的是 `设计/60 §2 档②` 那一条
//!
//! 那张表逐字：「非 UTF-8 文件名 —— 库层有损解码，**寻址不到**」。
//! 「寻址不到」不是显示难看：一旦文件名在某一跳被有损解码过，回程拿着那串
//! 替换字符去找那个文件，**找的是一个不存在的名字**。
//! ⇒ 本族从遍历到回送，路径**一路走字节**，中间没有任何一跳把它当文本读。
//!
//! # 三条实现纪律（都是判据在钉，不是自律）
//!
//! 1. **取字节走 [`std::ffi::OsStr::as_encoded_bytes`]** —— 它是跨平台的、无损的，
//!    而且**不需要任何平台条件编译**。
//!    ⚠ 这一条不是偏好：`platform/` 之外一写平台原语，`cfgless_guard` 的 `A1` 当场红
//!    （`std::os::…` 那一族在它的信号表上），而本族不住 `platform/`。
//! 2. **一处有损解码都不许有**。把路径转成人话的那几个调用（连 `Path` 的 `Display`
//!    一起）在本族目录下**零命中**，由 `tests/backend/files/capability_guard.rs`
//!    的 `no_lossy_decode_anywhere_in_the_family` 逐份扫源码钉住。
//! 3. **回送时不许「顺手解一下」**：[`to_json`] 对有效 UTF-8 才交字符串，
//!    其余一律交 `{"b16": "<十六进制>"}`。两条路**都是双向无损的**，
//!    由一条往返恒等判据钉住（含一份合成的、含非 UTF-8 字节的夹具）。
//!
//! # ⚠ 它**没有**买到什么
//!
//! - **Windows 那一侧的字节是 WTF-8，不是 UTF-16。** `as_encoded_bytes` 给的是
//!   标准库内部那套编码；同一个文件名在 Linux 与 Windows 上取出来的字节**不保证相同**。
//!   本族只承诺「**同一台机器上**取出去的字节，原样送回来能再指到同一个名字」，
//!   **不承诺跨平台的字节对等**。
//! - **不做归一化**（大小写 / Unicode NFC-NFD / 路径分隔符）。匹配是字节级的。

/// 一个路径的**原始字节**。
///
/// ⚠ 这里刻意不做成 newtype 包 `Vec<u8>`：本族内部一路传 `&[u8]`，
/// 多一层包装只会多一处「什么时候拆包」的判断，而那正是有损解码溜进来的缝。
pub fn path_bytes(p: &std::path::Path) -> &[u8] {
    p.as_os_str().as_encoded_bytes()
}

/// 十六进制小写。
fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let nib = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        }
    };
    let mut i = 0usize;
    while i < b.len() {
        out.push((nib(b[i])? << 4) | nib(b[i + 1])?);
        i += 2;
    }
    Some(out)
}

/// 线上那个键名。**只有一处住址** —— 判据按它对拍。
pub const HEX_KEY: &str = "b16";

/// 把一串路径字节交给线上，**双向无损**。
///
/// - 有效 UTF-8 ⇒ 一个 JSON 字符串（常见情形，界面直接能显示）。
/// - 其余 ⇒ `{"b16": "<十六进制>"}`。
///
/// ⚠ 为什么不一律走十六进制：回送的是**命中**，而命中数可以上万
///（`真相源/98 §3.3` 那一趟现打过一次 52 666 条命中）⇒ 一律双倍体积是白花的钱。
/// ⚠ 为什么不一律走字符串：那就是 `档②` 那条有损解码，本族存在的理由之一。
pub fn to_json(bytes: &[u8]) -> serde_json::Value {
    match std::str::from_utf8(bytes) {
        Ok(s) => serde_json::Value::String(s.to_string()),
        Err(_) => {
            let mut m = serde_json::Map::new();
            m.insert(
                HEX_KEY.to_string(),
                serde_json::Value::String(to_hex(bytes)),
            );
            serde_json::Value::Object(m)
        }
    }
}

/// [`to_json`] 的**逆** —— 入方向的路径参数也走这两种形。
///
/// `None` = 这个 JSON 值不是一个路径（形状不对 / 十六进制坏了）。
/// 🔴 **不许在这里「尽力而为」**：猜错一个字节就是去动另一个文件。
pub fn from_json(v: &serde_json::Value) -> Option<Vec<u8>> {
    match v {
        serde_json::Value::String(s) => Some(s.as_bytes().to_vec()),
        serde_json::Value::Object(m) => match m.get(HEX_KEY)? {
            serde_json::Value::String(h) => from_hex(h),
            _ => None,
        },
        _ => None,
    }
}

/// 把一串路径字节还原成能交给标准库的 `PathBuf`。
///
/// 🔴 **这是本族唯一一处 `unsafe`，理由写死在这里：**
/// `as_encoded_bytes` 与 `from_encoded_bytes_unchecked` 是标准库同一份编码的两侧，
/// 文档逐字要求喂进去的字节**必须来自同一平台的 `as_encoded_bytes`**。
/// 本族满足它：字节要么由 [`path_bytes`] 在**这台机器上**取出，要么由
/// [`from_json`] 从我们自己刚发出去的那两种形里读回来（往返恒等判据钉着）。
///
/// ⚠ 拿一串**外来**字节（比如别的平台发过来的）喂它是未定义行为 ——
/// 所以入方向的路径参数只许从**本机刚发出去的那一份**回流，
/// 不许由第三方凭空构造。这一条只能靠协议纪律，机器钉不住，**如实登记**。
pub fn to_path_buf(bytes: &[u8]) -> std::path::PathBuf {
    // SAFETY: 见上面那段 —— 字节来自同一平台的 `as_encoded_bytes`。
    let os: &std::ffi::OsStr = unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(bytes) };
    std::path::PathBuf::from(os)
}

/// 字节级子串匹配 —— `files.find` 的判词。
///
/// `ignore_ascii_case = true` 时只对 **ASCII** 大小写不敏感；
/// ⚠ **非 ASCII 一律按字节比**（那是 `设计/60 §3.5.3` 里「模糊匹配一条都没设计」
/// 那条边界的内侧：这不是「大小写不敏感的搜索」，是「ASCII 段的大小写不敏感」）。
///
/// # 为什么分成两条循环，而不是在内层判一次大小写标志
///
/// 🔴 **现打逼出来的**（秤 `F2 ③` 第一趟）：第一版把标志判在**最内层**
///（每比一个字节就分支一次），20 220 条的语料上一次查询 **17–39 毫秒**
/// ——外推到 `真相源/98 §3.3` 那个 64 万条量纲就是**半秒**级，
/// 而设计正文那句「**打字即出结果，不等**」（`设计/60 §3.5`）在半秒上不成立。
/// ⇒ 标志提到外层判一次，精确匹配那一支走整块 `==`（release 下落到 `memcmp`）。
/// ⚠ **改法不改语义**：两支的判词与第一版逐字等价，由
/// `raw_tests::the_substring_predicate_has_both_sides` 两侧各喂一遍钉着。
pub fn contains(hay: &[u8], needle: &[u8], ignore_ascii_case: bool) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    if ignore_ascii_case {
        contains_ascii_ci(hay, needle)
    } else {
        contains_exact(hay, needle)
    }
}

/// 精确那一支：**先按首字节跳**，命中了才整块比。
///
/// 🔴 为什么不是一句 `windows(n).any(|w| w == needle)` —— 秤 `F2 ③` 现打过：
/// 那个写法在**每一个**偏移上都调一次比较（2.9 MB 的索引 ⇒ 近三百万次），
/// release 档下 20 220 条要 **5–7 毫秒**，外推到 64 万条量纲是 **150–220 毫秒**。
/// 而 `真相源/98 §3.3` 那个对照（`grep -F` 在 64 万条上 **0.01 秒**）之所以快，
/// 靠的正是「先跳到可能的起点」这一步。
/// ⇒ 加一层首字节筛：把整块比较的次数从「每个偏移一次」降到
/// 「每个首字节命中一次」（随机字节上约 1/256）。
///
/// ⚠ **它仍然不是 `grep -F`**：那一份走的是 SIMD ＋ Boyer-Moore 一族。
/// 本族与它的差距逐趟报在秤 `F2 ③` 的读数里，**不在这里写一个结论**。
fn contains_exact(hay: &[u8], needle: &[u8]) -> bool {
    let first = needle[0];
    let last = hay.len() - needle.len();
    let mut i = 0usize;
    while i <= last {
        match hay[i..=last].iter().position(|&b| b == first) {
            None => return false,
            Some(k) => {
                let at = i + k;
                if &hay[at..at + needle.len()] == needle {
                    return true;
                }
                i = at + 1;
            }
        }
    }
    false
}

/// ASCII 段大小写不敏感的那一支。**先按首字节筛，再逐字节折**。
fn contains_ascii_ci(hay: &[u8], needle: &[u8]) -> bool {
    let first = needle[0];
    let last = hay.len() - needle.len();
    let mut i = 0usize;
    while i <= last {
        if hay[i].eq_ignore_ascii_case(&first)
            && hay[i..i + needle.len()]
                .iter()
                .zip(needle)
                .all(|(h, n)| h.eq_ignore_ascii_case(n))
        {
            return true;
        }
        i += 1;
    }
    false
}

/// 一个条目是不是 `dir` 的**直接子项**。
///
/// 口径写死（别读宽）：`entry` 以 `dir` 开头、紧跟一个分隔符、之后**再没有分隔符**。
/// 分隔符两种都认（`/` 与 `\`）—— 这样同一份判词在两个平台上是同一件事。
///
/// ⚠ 它**不解 symlink、不消 `..`**：本族拿到的路径全部由自己的遍历拼出来
/// （从根一层层 `push`），里面不会有 `.` / `..` 段。外来路径不走这条判词。
pub fn is_direct_child(entry: &[u8], dir: &[u8]) -> bool {
    let sep = |c: u8| c == b'/' || c == b'\\';
    if dir.is_empty() || entry.len() <= dir.len() + 1 {
        return false;
    }
    if &entry[..dir.len()] != dir {
        return false;
    }
    if !sep(entry[dir.len()]) {
        return false;
    }
    !entry[dir.len() + 1..].iter().copied().any(sep)
}

#[cfg(test)]
#[path = "../../../tests/backend/files/raw_tests.rs"]
mod tests;
