//! `K-R52`（2026-09-11）：**「不带 cfg 的平台代码」这一族，今天起有尺子。**
//!
//! # `K-G6` `KG62`：性质与人群，两行逐字（**各自只许有一句**）
//!
//! - **它守的性质是**：只在某些 target 上成立的代码，必须住在一道平台门后面 —— 要么被 `#[cfg(平台)]` 罩着，要么整份文件由 `#[cfg(平台)] mod x;` 选进来；而门后那一臂不许凭空返回一个「成功」值。
//! - **它扫的人群是**：本 crate `src/` 递归全部 `.rs`（`scan_tree!` 自动摘除本文件）的生产段 —— 剥掉 `#[cfg(test)]` 模块与全部注释之后 —— 里，[`SIGNALS`] 那张具名表逐条命中的每一处；回退臂那一半（`B 族`）的人群再扣掉 `platform/`，那一格归 [`super::fallback_guard`]。
//!
//! # 为什么非有它不可：**盘上那条机检是安慰剂，而它自己写着这句话**
//!
//! [`super`] 的模块头注逐字：
//!
//! > 「`platform/` 之外出现平台 cfg 就红」这条机检**是安慰剂** ——
//! > 本 crate 在 Windows 上编不过的 12 个错里，头号的 `pidfd_open` **根本没有 cfg**。
//!
//! ⇒ 判据只看得见**带 cfg** 的平台代码，看不见**不带 cfg** 的，
//! 而后者才是真正编不过的那一类。09-10 `K-W2D` 接线那一拍就从这条缝里漏了一处：
//! 已删的那条按需拉取路里「落一个可执行文件」那一跳，引了 `std::os::unix::fs` 里那个给 `mode(…)` 的扩展
//! trait，**一个 cfg 都没有**，daemon 从那一刻起在 Windows 上名字解析就过不了 ——
//! ⚠ 这里刻意**不把那个 trait 的名字逐字写出来**：`readonly_guard` 的默认层扫本 crate
//!   生产段（**连注释一起扫**，那是它 fail-closed 的设计），而那个名字在它的禁词表上
//!   —— 本文件不在写面白名单里，写出来当场红。实测过一次，如实记在这里。
//! 而当天**门禁全绿**：沙箱门禁的 `winchk` 射程是 `-p monitor`，**不含 daemon**。
//!
//! # 两族判据，一张人群
//!
//! - **A 族（本模块的正题）**：一处平台原语，头上没有任何平台 cfg，它所在的文件也不是被
//!   `#[cfg(平台)] mod x;` 整份选进来的。按后果分两档，**刻意不合成一条**
//!   （合成之后「A1 清零」这件事就报不出来，而那才是「搬得动搬不动」的分界）：
//!   - `A1` **编不过**：换个 target 名字解析就过不了（`cargo check --target` 当场红）。
//!   - `A2` **跑不对**：编得过，但那条路在别的平台上根本不存在（写死的 POSIX 路径、POSIX shell）。
//! - **B 族**：门后那一臂编了个乐观答案。判红条件与 [`super::fallback_guard`] **同源**，
//!   本份只补它够不着的那部分人群 —— 所以本份的人群**扣掉 `platform/`**：
//!   同一条判红条件不许在两处各写一份（一个闭集只许有一个住址）。
//!   ⚠ 那一份自己的头注早就承认过这一格：它要挡的那个形状**曾经就在
//!   `plugin/discover.rs` 活着，而人群够不着它**。本模块就是去把那个人群补上的。
//!   〔`K-R52` 同轮把那个活体也修了 ⇒ 那一行今天是**过去时**，两边同轮改完，
//!    别照抄成现在时。〕
//!
//! # 它挡不住什么（如实登记，别再宣称完备）
//!
//! - **信号表是黑名单**，列不全。本仓的偏好是白名单，但「枚举式白名单要求人群同质」这条前提
//!   在这里不成立（`fallback_guard` 头注 08-06 已经量过同一件事）：平台原语没有一个可枚举的全集。
//!   ⇒ 表里每一条都带**为什么它算平台**，新增要连理由一起加。
//! - **等价改写绕得过 —— 但门槛比上一版说的高一格**〔`K-R55` 09-11 订正，别再引旧说法〕。
//!   上一版逐字写着「把 `std::os::unix::fs::…` 那一串写成 `use std::os as o;` 再
//!   `o::unix::…`，本模块看不见」。🔴 **PM 09-11 切刀现打证伪：那个例子逃不掉** ——
//!   它被 [`SIGNALS`] 里**扩展 trait 名**那一族（`unix-ext-*`）接住，当场红。
//!   ⇒ 真实的说法是：本模块认的是**一组字面 needle**，一份改写要逃掉，得**同时**避开
//!   落在同一处代码上的**每一条** —— 路径（`std-os-*`）· 扩展 trait 名（`unix-ext-*`）·
//!   以及那几个字面量信号（`unix-mode-bits` 的 `.mode(0o` 那一族）。
//!   避开一条不够，避开两条也不够。
//!   ★ 这个说法**自己切过一刀验**：一份三样全避开的样本逐字住
//!   [`tests::a_rewrite_only_escapes_when_it_dodges_every_needle_that_lands_on_it`]，
//!   而同一份样本**只要把其中任意一样放回去就被逮住** —— 三格逐格断在那一条里。
//!   这条仍然**刻意不追**：完备性在这里做不到，而本模块挡的是
//!   「顺手写一行平台代码」这个真实且高频的形态。
//! - **`sh` / `bash` 那两条认的是「整条字面量恰好是它」，不是「出现过 `sh`」**〔`K-R103` 09-13〕。
//!   放宽的是**形状**不是**宽度**：`Command::new("sh")` 与 `const POSIX_SHELL: &str = "sh"`
//!   都进人群；而渲进一条更长的命令串里的 `sh`（`"tmux run-shell 'sh -c …'"`）、
//!   以及 `ssh` / `shell` / 变量名 `sh` 这类标识符**一律不进**。
//!   🔴 **刻意不做「凡出现 `sh` 就红」**：那会把正当用法一并扫进来 ⇒ **净变宽，人会绕开它**。
//!   现打（09-13，本 crate `src/` 生产段）：整条字面量这一口径命中 **3** 处
//!   （`control/ccm/mod.rs` 已签字 · `platform/shell.rs` 在 `#[cfg(unix)]` 门后 ·
//!   `control/oneshot_session.rs` 本轮新签），**假红 0**。
//! - **同一趟普查补上的 `posix-setsid`**〔`K-R103` 09-13〕：那条 argv 路是
//!   `setsid` ＋ `sh` ＋ `sleep` **三件**，而上一版只有 `sh` 那一件有针。
//!   `"setsid"` 整条字面量现打命中 **1** 处（`control/oneshot_session.rs`，本轮签字），假红 0。
//!   ⚠ `sleep` 那一件**刻意不加针**：它渲在一条更长的串里（`"sleep \"$1\"; shift; …"`），
//!   按子串认会把日志文案一并扫进来 —— **如实登记为没做**，不是「顺便也守住了」。
//!   ⚠ `tmux` 同样**刻意不加**：它也不在 Windows 上，但 `C12`〔用 08-11〕「windows不要tmux」
//!   已经把整个命令面裁成 POSIX-only，现打 **10** 处，加针买到的是 10 行签字、不是一条新事实。
//!   ⚠ 它仍然挡不住**运行期算出来的**程序名 —— 现打 **2** 处
//!   （`control/oneshot_session.rs` 的 `Command::new(launcher)` ·
//!   `control/ccm/mod.rs` 的 `Command::new(prog)`）。那一形无解，如实登记。
//! - **不认空白变体**：`#[cfg(` 与 `std :: os :: unix` 这类插了空格的写法认不出来。
//!   本 crate 全量过 `cargo fmt --check`（CI 与沙箱门禁各一道）⇒ 今天不会出现；
//!   哪天 fmt 那道门没了，这一条同时失效。
//! - 🔴 **它不是跨 target 编译的替代品。** 真判据一直是
//!   `cargo check --all-targets --target x86_64-pc-windows-msvc`（CI 的 daemon job 那一步，
//!   带 zig 的三个环境变量）。本模块买到的是**在那道门跑不到的地方也能出声**
//!   —— 本机那道 `tests/scripts/verify-committed-state.sh` 的 `daemon-win` 今天卡在 `ring`
//!   的构建脚本上（现打：EXIT=101、`failed to find tool "lib.exe"`、
//!   `Checking cc-monitor-remote` 命中 **0** ⇒ 根本走不到我们的代码），
//!   而 CI 那道只看 `origin/main`，本仓的红线是**不 push**。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销、不改 daemon 行为。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;
    use std::path::{Path, PathBuf};

    // ══════════════════════════ 人群 ══════════════════════════

    fn src_root() -> PathBuf {
        crate::guard_support::src_root()
    }

    fn rel_of(p: &Path, root: &Path) -> String {
        p.strip_prefix(root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    }

    // ══════════════════════════ 匹配单位 ══════════════════════════

    fn ident_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }

    /// `needle` 在 `hay` 里的全部命中。`word = true` ⇒ 两侧都要有标识符边界。
    ///
    /// 边界那一半不是装饰：没有它，一个 needle 会命中**任何把它整个包在里面的更长标识符**
    /// —— 那是 F05/F16 那一族「匹配单位比事实小」，本仓量到过三次，三次都不是被判据逮到的。
    fn hits(hay: &str, needle: &str, word: bool) -> Vec<usize> {
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = hay[from..].find(needle) {
            let at = from + rel;
            let end = at + needle.len();
            let ok = !word || {
                let before = hay[..at].chars().next_back().is_none_or(|c| !ident_char(c));
                let after = hay[end..].chars().next().is_none_or(|c| !ident_char(c));
                before && after
            };
            if ok {
                out.push(at);
            }
            from = at + 1;
        }
        out
    }

    fn contains_word(hay: &str, needle: &str) -> bool {
        !hits(hay, needle, true).is_empty()
    }

    /// 把**字符串字面量的内容**换成等长空格（引号留着），字节偏移不变。
    ///
    /// 这一份是回测第一版逼出来的：不分文本时，`observe/watcher.rs` 那句
    /// `tracing::info!("… → SIGUSR1 → 立刻重探")` 会被数成一处平台符号 —— 那是**日志文案**。
    /// ⇒ 代码类信号一律在这份文本上找；字面量类信号（写死的 POSIX 路径）才走原文。
    fn mask_strings(src: &str) -> String {
        let b = src.as_bytes();
        let mut out = b.to_vec();
        let mut i = 0usize;
        let blank = |out: &mut Vec<u8>, from: usize, to: usize| {
            for q in from..to.min(out.len()) {
                if out[q] != b'\n' {
                    out[q] = b' ';
                }
            }
        };
        while i < b.len() {
            // 原始字符串 `r"…"` / `r#"…"#`
            if b[i] == b'r' && i + 1 < b.len() && (b[i + 1] == b'"' || b[i + 1] == b'#') {
                let mut j = i + 1;
                let mut hashes = 0usize;
                while j < b.len() && b[j] == b'#' {
                    hashes += 1;
                    j += 1;
                }
                if j < b.len() && b[j] == b'"' {
                    let term = format!("\"{}", "#".repeat(hashes));
                    let start = j + 1;
                    let end = src[start..]
                        .find(term.as_str())
                        .map(|k| start + k)
                        .unwrap_or(b.len());
                    blank(&mut out, start, end);
                    i = (end + term.len()).min(b.len());
                    continue;
                }
            }
            if b[i] == b'"' {
                let mut j = i + 1;
                while j < b.len() {
                    if b[j] == b'\\' {
                        blank(&mut out, j, j + 2);
                        j += 2;
                        continue;
                    }
                    if b[j] == b'"' {
                        break;
                    }
                    blank(&mut out, j, j + 1);
                    j += 1;
                }
                i = (j + 1).min(b.len());
                continue;
            }
            i += 1;
        }
        String::from_utf8(out).expect("只把整段字节一起换成空格，不会切碎多字节序列")
    }

    // ══════════════════════════ 平台门 ══════════════════════════

    /// 一条 cfg 谓词里出现这些**独立标识符**就算「平台条件」。
    const PLATFORM_PREDS: &[&str] = &[
        "unix",
        "windows",
        "target_os",
        "target_family",
        "target_env",
        "target_vendor",
        "target_arch",
        "target_pointer_width",
    ];

    /// `s[i]` 是 `(` `[` `{` 之一 ⇒ 返回配对收尾的下标。
    ///
    /// ASCII 分隔符在 UTF-8 里不可能是多字节序列的一部分（续字节一律 ≥ 0x80）
    /// ⇒ 按字节扫、按字节切都安全。
    fn match_bracket(s: &str, i: usize) -> Option<usize> {
        let b = s.as_bytes();
        let closer = |c: u8| match c {
            b'(' => Some(b')'),
            b'[' => Some(b']'),
            b'{' => Some(b'}'),
            _ => None,
        };
        let mut stack = vec![closer(*b.get(i)?)?];
        let mut j = i + 1;
        while j < b.len() {
            let c = b[j];
            if let Some(cl) = closer(c) {
                stack.push(cl);
            } else if c == b')' || c == b']' || c == b'}' {
                if *stack.last()? != c {
                    return None;
                }
                stack.pop();
                if stack.is_empty() {
                    return Some(j);
                }
            }
            j += 1;
        }
        None
    }

    /// 一处平台 cfg 属性 + 它罩住的那一段。
    struct Gate {
        attr: String,
        /// 罩住的字节区间（含属性本身）。
        span: (usize, usize),
        /// 紧跟的那个 item / 块（不含属性）。
        body: (usize, usize),
        /// 主分支（本 crate 今天的原生平台）。其余一律按**回退臂**处理。
        primary: bool,
    }

    /// 扫出一份生产文本里全部**平台** cfg 属性，以及各自罩住的那一段。
    ///
    /// 罩住的那一段 = 属性之后（跳过紧跟的其它属性）的**那一个 item 或块**：
    /// 先到 depth-0 的 `{` ⇒ 罩到它的配对 `}`；先到 depth-0 的 `;` ⇒ 罩到那个 `;`。
    /// 这两形覆盖 `mod x;` · `use …;` · `let x = …;` · `fn f() {}` · 裸块 `{ … }`
    /// · `if cond { }`（条件在 `{` 之前，一并罩住）。
    fn platform_gates(prod: &str) -> Vec<Gate> {
        let b = prod.as_bytes();
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = prod[from..].find("#[cfg(") {
            let at = from + rel;
            from = at + 1;
            let (Some(rb), Some(rp)) = (match_bracket(prod, at + 1), match_bracket(prod, at + 5))
            else {
                continue;
            };
            let pred = &prod[at + 6..rp];
            // `#[cfg(all(test, target_os = "linux"))]` —— 测试期专属，不进生产人群。
            if !PLATFORM_PREDS.iter().any(|p| contains_word(pred, p)) || contains_word(pred, "test")
            {
                continue;
            }
            // 跳过紧跟着的其它属性（`#[allow(dead_code)]` 这类）。
            let mut k = rb + 1;
            loop {
                while k < b.len() && (b[k] as char).is_ascii_whitespace() {
                    k += 1;
                }
                if k < b.len() && b[k] == b'#' {
                    match prod[k..]
                        .find('[')
                        .map(|x| k + x)
                        .and_then(|nb| match_bracket(prod, nb))
                    {
                        Some(ne) => {
                            k = ne + 1;
                            continue;
                        }
                        None => break,
                    }
                }
                break;
            }
            let body_start = k;
            let mut j = k;
            let mut end = None;
            while j < b.len() {
                match b[j] {
                    b'(' | b'[' => match match_bracket(prod, j) {
                        Some(e) => j = e + 1,
                        None => break,
                    },
                    b'{' => {
                        end = match_bracket(prod, j);
                        break;
                    }
                    b';' => {
                        end = Some(j);
                        break;
                    }
                    _ => j += 1,
                }
            }
            let Some(end) = end else { continue };
            let norm: String = pred.chars().filter(|c| !c.is_whitespace()).collect();
            out.push(Gate {
                attr: prod[at..rb + 1]
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
                span: (at, end + 1),
                body: (body_start, end + 1),
                primary: norm == "unix" || norm == "target_os=\"linux\"",
            });
        }
        out
    }

    // ══════════════════════════ 信号表 ══════════════════════════

    /// `(名字, 档, 在哪份文本上找, 是否要求标识符边界, needle, 为什么它算平台)`
    ///
    /// 🔴 **每加一条都要把最后那一栏一起写上。** 一条说不出「为什么它算平台」的信号，
    /// 下一轮就没人敢删、也没人说得清它在守什么。
    const SIGNALS: &[(&str, &str, Where, bool, &str, &str)] = &[
        (
            "std-os-unix",
            "A1",
            Where::Code,
            false,
            "std::os::unix",
            "`std::os::unix` 整个模块在 Windows target 上不存在 ⇒ 名字解析就过不了",
        ),
        (
            "std-os-windows",
            "A1",
            Where::Code,
            false,
            "std::os::windows",
            "`std::os::windows` 在 unix target 上不存在",
        ),
        (
            "std-os-linux",
            "A1",
            Where::Code,
            false,
            "std::os::linux",
            "`std::os::linux` 只在 Linux 上存在",
        ),
        (
            "std-os-fd",
            "A1",
            Where::Code,
            false,
            "std::os::fd",
            "`std::os::fd` 只在 unix / wasi 上存在（Windows 那一侧叫 `os::windows::io`）",
        ),
        (
            "libc",
            "A1",
            Where::Code,
            false,
            "libc::",
            "`libc` 的符号绝大多数是 POSIX，Windows 上没有",
        ),
        (
            "windows-sys",
            "A1",
            Where::Code,
            false,
            "windows_sys::",
            "Windows 专属 crate",
        ),
        (
            "winapi",
            "A1",
            Where::Code,
            false,
            "winapi::",
            "Windows 专属 crate",
        ),
        (
            "tokio-signal-unix",
            "A1",
            Where::Code,
            false,
            "tokio::signal::unix",
            "`tokio::signal::unix` 是 unix-only 子模块",
        ),
        (
            "unix-ext-PermissionsExt",
            "A1",
            Where::Code,
            true,
            "PermissionsExt",
            "平台扩展 trait 的名字 —— 换 target 后这个名字不存在",
        ),
        (
            "unix-ext-OpenOptionsExt",
            "A1",
            Where::Code,
            true,
            "OpenOptionsExt",
            "平台扩展 trait 的名字 —— 换 target 后这个名字不存在（Windows 那一份语义也不同）",
        ),
        (
            "unix-ext-MetadataExt",
            "A1",
            Where::Code,
            true,
            "MetadataExt",
            "平台扩展 trait 的名字",
        ),
        (
            "unix-ext-CommandExt",
            "A1",
            Where::Code,
            true,
            "CommandExt",
            "两个平台各有一个同名 trait，方法集完全不同 ⇒ 换 target 就不是同一件事",
        ),
        (
            "unix-ext-ExitStatusExt",
            "A1",
            Where::Code,
            true,
            "ExitStatusExt",
            "平台扩展 trait 的名字",
        ),
        (
            "raw-fd",
            "A1",
            Where::Code,
            true,
            "AsRawFd",
            "fd 是 unix 的概念（Windows 那一侧是 HANDLE / SOCKET）",
        ),
        (
            "raw-fd-from",
            "A1",
            Where::Code,
            true,
            "FromRawFd",
            "fd 是 unix 的概念",
        ),
        (
            "owned-fd",
            "A1",
            Where::Code,
            true,
            "OwnedFd",
            "fd 是 unix 的概念",
        ),
        (
            "posix-SIGUSR1",
            "A1",
            Where::Code,
            true,
            "SIGUSR1",
            "POSIX 信号常量 —— Windows 上没有信号这套东西",
        ),
        (
            "posix-pollfd",
            "A1",
            Where::Code,
            true,
            "pollfd",
            "`poll(2)` 的结构体 —— POSIX",
        ),
        (
            "posix-pid_t",
            "A1",
            Where::Code,
            true,
            "pid_t",
            "POSIX 的进程号类型名 —— Windows 上没有这个类型",
        ),
        (
            "posix-syscall-no",
            "A1",
            Where::Code,
            false,
            "SYS_",
            "Linux 系统调用号常量（`SYS_pidfd_open` 那一族 —— 正是本模块头注点名的那个）",
        ),
        (
            "unix-mode-bits",
            "A1",
            Where::Code,
            false,
            ".mode(0o",
            "unix 权限位 —— `OpenOptions::mode` / `set_mode` 只长在 unix 扩展 trait 上，             换 target 后那个方法不存在",
        ),
        (
            "posix-errno",
            "A1",
            Where::Code,
            true,
            "E2BIG",
            "POSIX errno 常量",
        ),
        (
            "posix-abs-proc",
            "A2",
            Where::Literal,
            false,
            "\"/proc/",
            "写死的 POSIX 绝对路径 —— Windows 上这条路根本不存在（编得过、跑不对）",
        ),
        (
            "posix-abs-tmp",
            "A2",
            Where::Literal,
            false,
            "\"/tmp\"",
            "写死的 POSIX 绝对路径（编得过、跑不对）",
        ),
        (
            "posix-abs-tmp-sub",
            "A2",
            Where::Literal,
            false,
            "\"/tmp/",
            "写死的 POSIX 绝对路径（编得过、跑不对）",
        ),
        (
            "posix-abs-dev",
            "A2",
            Where::Literal,
            false,
            "\"/dev/",
            "写死的 POSIX 绝对路径（编得过、跑不对）",
        ),
        (
            "posix-abs-usr",
            "A2",
            Where::Literal,
            false,
            "\"/usr/",
            "写死的 POSIX 绝对路径（编得过、跑不对）",
        ),
        (
            "posix-abs-etc",
            "A2",
            Where::Literal,
            false,
            "\"/etc/",
            "写死的 POSIX 绝对路径（编得过、跑不对）",
        ),
        (
            "posix-shell-sh",
            "A2",
            Where::Literal,
            false,
            "\"sh\"",
            "起 POSIX shell —— Windows 上没有 `sh`（编得过、跑不对）。\
             🔴 `K-R103`（09-13）把 needle 从 `Command::new(\"sh\")` 放宽成\
             **整条字面量恰好是 `sh`**：那一形之外还有 **argv 形**（`sh` 是一个 argv 元素、\
             或一个具名常量的值），上一版一个字都看不见 —— 活体就是\
             `control/oneshot_session.rs` 的看门狗",
        ),
        (
            "posix-setsid",
            "A2",
            Where::Literal,
            false,
            "\"setsid\"",
            "`setsid(1)`（util-linux）—— Windows 上没有这个程序，也没有「会话」这套概念\
             （编得过、跑不对）。🔴 它是 `K-R103` 现打补上的：`posix-shell-*` 那两根针只看得见\
             **起 shell** 那一半，而同一条 argv 路上**把进程放进新会话**的那一半（launcher），\
             上一版一根针都没落在它身上",
        ),
        (
            "posix-shell-bash",
            "A2",
            Where::Literal,
            false,
            "\"bash\"",
            "起 POSIX shell（编得过、跑不对）。放宽同上一条：认的是**整条字面量恰好是 `bash`**",
        ),
    ];

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Where {
        /// 抹掉字符串内容之后的文本 —— 代码类信号走这里。
        Code,
        /// 原文 —— 找的就是字面量本身的那几条走这里。
        Literal,
    }

    // ══════════════════════════ A2 登记表 ══════════════════════════

    /// **A2（跑不对那一档）今天在门外的每一处，逐条签字。**
    ///
    /// 🔴 **这不是白名单** —— 每一条都要说清它**是哪一堆**（`真漏` / `合法线外`），
    /// 而不是「免检」。`真漏` 那几条写明**归谁、什么时候搬**。
    /// 一处没登记的 A2 ⇒ 当场红；一条登记了却**再也匹配不上**的 ⇒ 也当场红
    /// （不许留过期条目 —— 那会让这张表慢慢变成一张谁也不敢动的免检名单）。
    ///
    /// ⚠ **它买到的是「下不为例」，不是「今天干净」** —— 表里剩下的 `真漏` 今天还在门外。
    ///
    /// # 🔴 `K-R55`（09-11）：`observe/watcher.rs` 那两条**不是删掉，是搬走了**
    ///
    /// 两条 `真漏`（`Command::new("sh")` ×2 · `PathBuf::from("/tmp")`）今天住进了
    /// [`super::shell::posix_shell`] 与 [`super::paths::temp_root`] / [`super::paths::current_uid`]。
    /// ⇒ 它们在门外的命中降到 0，本表里那两行随之成为**过期条目**（留着会被
    /// [`platform_assumptions_outside_a_gate_are_each_signed_for`] 的第二个方向判红）⇒ 删。
    /// 🔴 而「搬走」与「签个字了事」在这张表上长得一模一样（两种做法这张表都会变绿）
    /// ⇒ 分得开它们的是 [`CLOSED_FOR_GOOD`] 那道棘轮，不是本表。
    ///
    /// 形状 = `(文件相对路径, 那一行的逐字锚点, 堆, 理由)`
    const REGISTERED: &[(&str, &str, &str, &str)] = &[
        (
            "control/ccm/mod.rs",
            "Command::new(\"sh\")",
            "真漏",
            "`K-R48`：一次性 `ccm` 模式把渲好的命令串交给 POSIX shell。\
             `ccm` 这套命令面今天**只有 POSIX 一支**（`C12`「windows不要tmux」；\
             `payload.rs` 逐字「Windows 那条腿不在人群里」），而件文件 `§0-Bx-7` 第 6 条\
             已经把「一次性模式在 Windows 上是什么形状」登记成**判不了**。\
             ⇒ 这里不假装它跨平台：`exec_or_spawn` 里那条 `#[cfg(unix)]` 是真门，\
             而这一处**没有门**，归 PM（要么进适配层，要么随那一格一起裁）。",
        ),
        (
            "agents/fake/mod.rs",
            "cmdline(\"/usr/bin/vim\")",
            "合法线外",
            "它是喂给判定器的**反例数据**（断言这条 cmdline **不**被判成 agent），\
             不是本机要去走的路 ⇒ 在任何平台上行为相同，不是平台代码。",
        ),
    ];

    /// 🔴 `K-R55`（09-11）：**搬走过一次的文件，不许再靠签字回到这张表上。**
    ///
    /// # 它治的是一个「两种做法长得一样」的口子
    ///
    /// [`REGISTERED`] 那两个方向（没登记 ⇒ 红 · 登记了匹配不上 ⇒ 红）买到的是
    /// 「每一处门外的 A2 都有人签过字」。它**分不出**这两件事：
    /// ① 把那条平台原语搬进 `platform/`（`K33` 裁定二要的那件事）；
    /// ② 在 [`REGISTERED`] 上补一行字（那张表自己写着，它买的是「下不为例」）。
    /// 两种做法它都会变绿，而 `KR55D1` 逐字要的是**前者**
    /// （「且**不是**靠把它们塞进 `REGISTERED` 签字表兑现的」）。
    ///
    /// ⇒ 这张表记「已经搬完、从此不许再签字」的那几个文件。一处回到门外 ⇒ 当场红，
    /// 而且红的是**这一条**（诊断直接说「它又回来了」），不是那条泛泛的「没签字」。
    ///
    /// ⚠ **它买不到什么（如实写）**：
    /// - 它按**文件**认，不按那一处认 ⇒ 同一个文件里长出**别的**平台原语，
    ///   报出来的诊断仍然是这一条（措辞会误导一格）。今天这个分母是 1，不值得再切细。
    /// - 它**不**保证那条原语搬进 `platform/` 之后写对了 —— 那归行为判据
    ///   （[`super::shell`] / [`super::paths`] 各自的单测）与跨 target 编译。
    /// - **只许变长**：一处修好了就把它的文件加进来；**从这张表里删名字**等于允许回退，
    ///   要删得先说清那条平台原语今天住在哪儿。
    const CLOSED_FOR_GOOD: &[(&str, &str)] = &[(
        "observe/watcher.rs",
        "`K-R55` 09-11：两跳 `sh -c` 搬进 `platform/shell.rs`，\
         `/tmp` 与 `uid` 搬进 `platform/paths.rs` ⇒ 门外命中 0",
    )];

    // ══════════════════════════ 扫 ══════════════════════════

    struct Finding {
        rel: String,
        signal: &'static str,
        tier: &'static str,
        line: String,
        gated_by: Option<String>,
    }

    /// 一份生产文本里，被 `#[cfg(平台)] mod x;` 整份选进来的**模块名**。
    fn file_level_gated(prod: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for g in platform_gates(prod) {
            let body = prod[g.body.0..g.body.1].trim();
            let Some(rest) = body.strip_suffix(';') else {
                continue;
            };
            let rest = rest.trim();
            // `mod x` / `pub mod x` / `pub(crate) mod x`
            let Some(name) = rest.rsplit_once("mod ").map(|(_, n)| n.trim()) else {
                continue;
            };
            if !name.is_empty() && name.chars().all(ident_char) {
                out.push((name.to_string(), g.attr.clone()));
            }
        }
        out
    }

    /// 那一处所在的整行（在**生产文本**里）。
    ///
    /// ⚠ 刻意不报行号：生产文本是剥过测试段的，行号与文件对不上。
    /// 报**逐字那一行**既是住址也是校验位（`brief` 13c：指进本树的行号要么带校验位、要么不写）。
    fn line_at(prod: &str, at: usize) -> String {
        let start = prod[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let end = prod[at..].find('\n').map(|i| at + i).unwrap_or(prod.len());
        prod[start..end].trim().to_string()
    }

    /// 全树一趟，出 A 族的全部命中（有门的与无门的都在里面）。
    fn scan_tree() -> (Vec<Finding>, usize) {
        let root = src_root();
        let files = guard_core::scan_tree!(&root, &["rs"]);
        let n = files.len();
        // 先把「哪些文件整份住在门后」算出来 —— 它是跨文件的事实。
        let mut gated_files: Vec<(String, String)> = Vec::new();
        let mut prods: Vec<(String, PathBuf, String)> = Vec::new();
        for (f, src) in &files {
            let prod = production_code(src);
            let rel = rel_of(f, &root);
            for (name, attr) in file_level_gated(&prod) {
                let dir = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
                let base = if dir.is_empty() {
                    name.clone()
                } else {
                    format!("{dir}/{name}")
                };
                gated_files.push((format!("{base}.rs"), attr.clone()));
                gated_files.push((format!("{base}/"), attr));
            }
            prods.push((rel, f.clone(), prod));
        }
        let mut out = Vec::new();
        for (rel, _f, prod) in &prods {
            let file_gate = gated_files
                .iter()
                .find(|(p, _)| rel == p || (p.ends_with('/') && rel.starts_with(p.as_str())))
                .map(|(p, a)| format!("整份文件由 `{a}`（{p}）选进来"));
            let gates = platform_gates(prod);
            let code = mask_strings(prod);
            for (name, tier, wh, word, needle, _why) in SIGNALS {
                let hay = if *wh == Where::Code { &code } else { prod };
                for at in hits(hay, needle, *word) {
                    let inside = gates
                        .iter()
                        .find(|g| g.span.0 <= at && at < g.span.1)
                        .map(|g| format!("`{}`", g.attr));
                    out.push(Finding {
                        rel: rel.clone(),
                        signal: name,
                        tier,
                        line: line_at(prod, at),
                        gated_by: inside.or_else(|| file_gate.clone()),
                    });
                }
            }
        }
        (out, n)
    }

    /// B 族：门后那一臂编了个乐观答案。判红条件与 [`super::super::fallback_guard`] 同源。
    fn fabricated_success(prod: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for g in platform_gates(prod) {
            if g.primary {
                continue;
            }
            let body = &prod[g.body.0..g.body.1];
            // `mod x;` 没有块体 ⇒ 判不了（整份文件绕过），如实跳过、在别处登记。
            if body.trim_end().ends_with(';') {
                continue;
            }
            let why = if !hits(body, "true", true).is_empty() {
                "块体里出现裸 `true`"
            } else {
                let inner = body.trim().trim_start_matches('{').trim_end_matches('}');
                let last = inner
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .next_back()
                    .unwrap_or("");
                let tail = last.trim_end_matches([',', ';']);
                if tail.starts_with("Some(") || tail.starts_with("Ok(") {
                    "块的最后一个表达式以 `Some(` / `Ok(` 打头"
                } else {
                    continue;
                }
            };
            out.push((
                format!(
                    "{} → {}",
                    g.attr,
                    body.split_whitespace().collect::<Vec<_>>().join(" ")
                ),
                why.to_string(),
            ));
        }
        out
    }

    // ══════════════════════════ 判据 ══════════════════════════

    /// ★★ 正题：**A1（换个 target 就编不过的那一档）一处都不许在门外。**
    ///
    /// 这一条就是 `platform/mod.rs` 头注说「今天没有判据」的那一格。
    #[test]
    fn platform_primitives_that_break_the_build_must_all_sit_behind_a_gate() {
        let (all, _) = scan_tree();
        let naked: Vec<String> = all
            .iter()
            .filter(|f| f.tier == "A1" && f.gated_by.is_none())
            .map(|f| format!("  {} [{}]  {}", f.rel, f.signal, f.line))
            .collect();
        assert_eq!(
            naked,
            Vec::<String>::new(),
            "这几处平台原语**头上一个平台 cfg 都没有**，所在文件也不是被 \
             `#[cfg(平台)] mod x;` 整份选进来的 ⇒ 换个 target 名字解析就过不了：\n{}\n\
             `K33` 裁定二：平台差异只许住适配层。两条出路 ——\n\
             ① 给它一道 `#[cfg(平台)]` 门，且**非目标平台那一臂要诚实**\
             （`false` / `None` / `unimplemented!()`，不许编一个乐观答案）；\n\
             ② 把那条原语搬进 `platform/`。\n\
             🔴 **不许**把它加进 `REGISTERED` —— 那张表只收 A2（跑不对那一档）。",
            naked.join("\n")
        );
    }

    /// ★ A2（编得过、跑不对那一档）：门外的每一处都要在 [`REGISTERED`] 里签过字。
    ///
    /// 两个方向都断，**缺一不可**：
    /// · 有命中而没登记 ⇒ 新长出来的平台假设，红；
    /// · 有登记而匹配不上 ⇒ 过期条目，也红（否则这张表会慢慢变成一张没人敢动的免检名单）。
    #[test]
    fn platform_assumptions_outside_a_gate_are_each_signed_for() {
        let (all, _) = scan_tree();
        let naked: Vec<&Finding> = all
            .iter()
            .filter(|f| f.tier == "A2" && f.gated_by.is_none())
            .collect();
        let unsigned: Vec<String> = naked
            .iter()
            .filter(|f| {
                !REGISTERED
                    .iter()
                    .any(|(p, anchor, _, _)| *p == f.rel && f.line.contains(anchor))
            })
            .map(|f| format!("  {} [{}]  {}", f.rel, f.signal, f.line))
            .collect();
        assert_eq!(
            unsigned,
            Vec::<String>::new(),
            "这几处平台假设在门外，而 `REGISTERED` 里没有它：\n{}\n\
             要么给它一道门 / 搬进 `platform/`，要么在那张表里签一行字\
             （写清它是`真漏`还是`合法线外`，`真漏` 要写明归谁）。",
            unsigned.join("\n")
        );
        let stale: Vec<String> = REGISTERED
            .iter()
            .filter(|(p, anchor, _, _)| {
                !naked.iter().any(|f| f.rel == *p && f.line.contains(anchor))
            })
            .map(|(p, anchor, _, _)| format!("  {p} :: {anchor}"))
            .collect();
        assert_eq!(
            stale,
            Vec::<String>::new(),
            "`REGISTERED` 里这几条今天**一处都匹配不上**了：\n{}\n\
             多半是那一处已经修好 / 挪走了 ⇒ 把这一行删掉。\n\
             留着它等于让这张表越长越松，而每一行都说得出理由的表才拦得住人。",
            stale.join("\n")
        );
    }

    /// ★★★ `KR55D1`：**搬完的那几个文件，门外的 A2 保持 0，而且不许靠签字回来。**
    ///
    /// 上一条只买到「每一处都有人签过字」—— 而「搬进适配层」与「在表上补一行」
    /// 在它眼里一模一样（整段理由住 [`CLOSED_FOR_GOOD`] 头注）。本条把那一格补上，
    /// 两个方向都断：
    /// · 那个文件在门外又有 A2 命中 ⇒ 红（回退了）；
    /// · 那个文件又出现在 [`REGISTERED`] 上 ⇒ 红（**签字了事**，正是 `KR55D1` 排除的那条路）。
    #[test]
    fn the_files_that_were_moved_into_the_adapter_layer_stay_clean() {
        assert!(
            !CLOSED_FOR_GOOD.is_empty(),
            "`CLOSED_FOR_GOOD` 空了 —— 本条会变成「对空集全称成立」，恒绿"
        );
        let (all, _) = scan_tree();
        // 反空真：整棵树上 A2 命中本来就该是非空的（门里门外都算）。
        // 全空时下面那条「这几个文件没有命中」是空真 —— 那多半是扫坏了，不是干净了。
        assert!(
            all.iter().any(|f| f.tier == "A2"),
            "整棵树一处 A2 都没扫到 —— 扫坏了，本条在空转"
        );
        let mut bad: Vec<String> = Vec::new();
        for (rel, _why) in CLOSED_FOR_GOOD {
            for f in all
                .iter()
                .filter(|f| f.rel == *rel && f.tier == "A2" && f.gated_by.is_none())
            {
                bad.push(format!("  回到门外：{} [{}]  {}", f.rel, f.signal, f.line));
            }
            for (p, anchor, ..) in REGISTERED {
                if p == rel {
                    bad.push(format!("  又签了字：{p} :: {anchor}"));
                }
            }
        }
        assert_eq!(
            bad,
            Vec::<String>::new(),
            "这几处破了 `KR55D1` 的棘轮：\n{}\n\
             `CLOSED_FOR_GOOD` 里的文件已经把平台原语搬进 `platform/` 了 ——\n\
             回退的出路只有一条：把那条原语**再搬进适配层**，\n\
             **不是**在 `REGISTERED` 上补一行字（那张表买的是「下不为例」，不是「今天干净」）。\n\
             真要让某个文件退出这张表，先说清它那条平台原语今天住在哪儿。",
            bad.join("\n")
        );
    }

    /// ★ B 族补人群：`platform/` **之外**的回退臂，不许凭空返回一个「成功」值。
    ///
    /// 判红条件与 [`super::super::fallback_guard`] 同源，本条只补它够不着的那部分人群 ——
    /// 那一份自己的头注承认过：它要挡的那个形状**曾经**就在 `plugin/discover.rs` 活着，
    /// **而人群够不着它** —— `K-R52` 同轮把那个活体修掉了，人群这一格由本条接手。
    #[test]
    fn fallback_arms_outside_the_adapter_layer_must_not_fabricate_success() {
        let root = src_root();
        let mut bad: Vec<String> = Vec::new();
        let mut scanned = 0usize;
        for (f, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = rel_of(&f, &root);
            // `platform/` 那一格归 `fallback_guard` —— 同一条判红条件不许两处各写一份。
            if rel.starts_with("platform/") {
                continue;
            }
            scanned += 1;
            for (what, why) in fabricated_success(&production_code(&src)) {
                bad.push(format!("  {rel}：{why}\n      {what}"));
            }
        }
        // 🔴 〔条 67 · 2026-09-18〕地板 57 → **53**（现打 55，留 2 份余量，与立表时同 margin）：
        // 用户逐字「**不在现在设计里的全部删掉**」⇒ 删了 `sidecars/` 那四份 `.rs`。
        // ⚠ 同拍删的 `platform/landing.rs` **不在本条人群里**（本条把 `platform/` 整个 continue 掉了）
        // ⇒ 本条只少 4，上一条少 5。**两个数不一样是对的**，别照抄。
        // 〔上一次：`设计/50` 删用量 09-18 地板 60 → 57（现打 59），删的是 `observe/usage_query.rs` ·
        //   `control/oneshot_session.rs` · `agents/codex/usage.rs` 三份。〕
        assert!(
            scanned >= 53,
            "`platform/` 之外只扫到 {scanned} 份 `.rs` —— 人群塌了，本条此刻在空转"
        );
        assert_eq!(
            bad,
            Vec::<String>::new(),
            "这几条非主分支的平台臂凭空返回了一个「成功」值：\n{}\n\
             答不上来的问题要诚实地说不知道（`false` / `None` / `unimplemented!()`）。\n\
             一个编出来的 `true` / `Some(..)` / `Ok(..)` 会让上层以为拿到了事实 ——\
             `platform/fallback_guard.rs` 头注里那个 `pid_alive` 的地雷就是这么来的。",
            bad.join("\n")
        );
    }

    /// ★ 反空真①：**人群不许静默塌掉。**
    ///
    /// 上面三条都是「某个集合是空的」形状的断言 —— 而闸死了 `[] == []` 照样成立。
    /// 这一条钉住「今天确实扫到了东西」：文件数地板 + 一处**已知在门后**的 A1 命中。
    #[test]
    fn the_population_is_not_silently_empty() {
        let (all, files) = scan_tree();
        // 🔴 〔条 67 · 2026-09-18〕地板 67 → **62**（现打 64）：用户逐字「**不在现在设计里的全部删掉**」
        // ⇒ `sidecars/` 整棵树四份 `.rs`（2 008 行）＋ `platform/landing.rs`（唯一消费者没了）一起走。
        // 人群**真的**小了 5。**这不是遍历坏了** —— 两者读数长得一样，所以降地板必须逐份点名。
        // 〔上一次：`设计/50` 09-18 地板 70 → 67（现打 69），那一刀删了三份 `.rs`。〕
        assert!(
            files >= 62,
            "只扫到 {files} 份 `.rs`（`scan_tree!` 已摘除本文件）—— 遍历坏了，\
             上面那几条此刻全在空转"
        );
        let gated_a1 = all
            .iter()
            .filter(|f| f.tier == "A1" && f.gated_by.is_some())
            .count();
        assert!(
            gated_a1 >= 10,
            "全树只找到 {gated_a1} 处**有门的** A1 平台原语 —— \
             信号表或剥法坏了（这棵树光 `platform/pidwatch/linux.rs` 一份就不止这个数）"
        );
        let pidwatch = all
            .iter()
            .filter(|f| f.rel == "platform/pidwatch/linux.rs")
            .count();
        assert!(
            pidwatch >= 5,
            "`platform/pidwatch/linux.rs` 只贡献 {pidwatch} 处命中 —— \
             它是这棵树上平台原语最密的一份（`SYS_pidfd_open` / `std::os::fd` / `libc::poll` …）；\
             数不出来说明**文件级门**那一段把整份文件漏掉了，而不是它变干净了"
        );
    }

    /// 一份合成源码里**门外的 A2** 命中了哪几条信号 —— 与 [`naked_a1`] 同源，只换档。
    ///
    /// ⚠ 它回**信号名**而不是个数：`KR103D2` 要断的是「argv 形那一条被 `posix-shell-sh`
    /// 接住了」，只数个数分不出是哪根针在响。
    fn naked_a2(src: &str) -> Vec<&'static str> {
        let prod = production_code(src);
        let code = mask_strings(&prod);
        let gates = platform_gates(&prod);
        let mut out: Vec<&'static str> = Vec::new();
        for (name, tier, wh, word, needle, _) in SIGNALS {
            if *tier != "A2" {
                continue;
            }
            let hay = if *wh == Where::Code { &code } else { &prod };
            for at in hits(hay, needle, *word) {
                if !gates.iter().any(|g| g.span.0 <= at && at < g.span.1) {
                    out.push(name);
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// ★★ `KR103D2`：**argv 形起 shell 逮得到，而「出现过 `sh`」不算。**
    ///
    /// 五刀。反的那三刀就是「🔴 别把针扩成『凡出现 `sh` 就红』」那条红线的**活体** ——
    /// 没有它们，把 needle 换成裸 `sh` 这一步在盘上与本轮的改法长得一模一样。
    #[test]
    fn an_argv_shaped_shell_start_is_caught_and_a_mere_mention_of_sh_is_not() {
        // ★ 老形态不许丢。
        let ctor = "fn f() {\n    let c = std::process::Command::new(\"sh\");\n}\n";
        assert_eq!(
            naked_a2(ctor),
            vec!["posix-shell-sh"],
            "`Command::new(…)` 那一形不逮了 —— 放宽把老形态弄丢了"
        );
        // ★ 正题：argv 形 —— `sh` 是一个具名常量的值，`Command::new` 一个字都没有。
        let argv = "pub const POSIX_SHELL: &str = \"sh\";\n";
        assert_eq!(
            naked_a2(argv),
            vec!["posix-shell-sh"],
            "argv 形起 shell 没被逮到 —— 那正是 `K-R103` 要关的那条盲区"
        );
        // ★ 反 ①：`sh` 渲在一条更长的命令串里（**正当用法**：那条串是交给别人执行的）。
        let inside = "fn f() {\n    let s = \"tmux run-shell 'sh -c echo'\";\n}\n";
        assert_eq!(
            naked_a2(inside),
            Vec::<&str>::new(),
            "把渲在长串里的 `sh` 也判红了 —— 那就是「凡出现 sh 就红」，净变宽"
        );
        // ★ 反 ②：标识符里的 `sh`（`ssh` / `shell` / 变量名 `sh`）。
        let ident = "fn f() {\n    let sh = ssh_shell();\n}\n";
        assert_eq!(
            naked_a2(ident),
            Vec::<&str>::new(),
            "标识符里的 `sh` 被判红了 —— 匹配单位比事实大"
        );
        // ★ 反 ③：门后那一臂仍然不算门外，否则适配层自己会被打红。
        let gated = "fn f() {\n    #[cfg(unix)]\n    {\n        \
                     let c = Command::new(\"sh\");\n    }\n}\n";
        assert_eq!(
            naked_a2(gated),
            Vec::<&str>::new(),
            "有门的那一处被判成门外 —— 粗刀：把合法的适配层代码一起打红"
        );
    }

    /// 一份合成源码里**门外的 A1** 有几处 —— 判据本体喂字符串的那一半。
    ///
    /// ⚠ 它是从 [`the_detector_bites_on_synthetic_defects_and_not_on_prose`] 里**抬出来的
    /// 同一份**，不是复刻：两条判据（那一条与
    /// [`a_rewrite_only_escapes_when_it_dodges_every_needle_that_lands_on_it`]）必须量同一把尺子，
    /// 否则副本一分叉，红灯就开始骗人（`fallback_guard` 头注里逐字记过这一形）。
    fn naked_a1(src: &str) -> usize {
        let prod = production_code(src);
        let code = mask_strings(&prod);
        let gates = platform_gates(&prod);
        SIGNALS
            .iter()
            .filter(|(_, tier, ..)| *tier == "A1")
            .flat_map(|(_, _, wh, word, needle, _)| {
                let hay = if *wh == Where::Code { &code } else { &prod };
                hits(hay, needle, *word)
            })
            .filter(|at| !gates.iter().any(|g| g.span.0 <= *at && *at < g.span.1))
            .count()
    }

    /// ★★ 反空真②：**拿合成夹具证明这把尺子真有牙** —— 不靠真树上碰巧有没有病灶。
    ///
    /// 四刀，两正两反。反的那两刀是本模块最容易坏的两处：
    /// 剥注释坏掉 ⇒ 散文被数成代码（这棵树的注释里**大量逐字写着代码片段**）；
    /// 抹字符串坏掉 ⇒ 日志文案被数成平台符号（`watcher.rs` 那句 `SIGUSR1` 实打命中过一次）。
    #[test]
    fn the_detector_bites_on_synthetic_defects_and_not_on_prose() {
        // ① 无门的平台原语 ⇒ 逮到。这一形就是 09-10 真漏进来的那一处。
        let naked = "pub fn land() {\n    use std::os::unix::fs::OpenOptionsExt;\n}\n";
        assert_eq!(naked_a1(naked), 2, "无门的 `std::os::unix` 竟然没被逮到");
        // ② 同一份代码加上门 ⇒ 不逮。（否则本模块把合法的适配层代码一起打红，
        //    那是「粗刀」—— 红得像有牙，其实是把漏洞和实现一起打红。）
        let gated = "pub fn land() {\n    #[cfg(unix)]\n    {\n        \
                     use std::os::unix::fs::OpenOptionsExt;\n    }\n}\n";
        assert_eq!(naked_a1(gated), 0, "有门的那一处被误判成无门");
        // ③ 注释里逐字写着同一句 ⇒ 不逮（剥注释那一格）。
        let prose = "/// 曾经这里写着 use std::os::unix::fs::OpenOptionsExt;\n\
                     pub fn land() {}\n";
        assert_eq!(naked_a1(prose), 0, "注释里的散文被数成了代码 —— 剥法坏了");
        // ④ 日志文案里出现 `SIGUSR1` ⇒ 不逮（抹字符串那一格）。
        let log = "pub fn f() { tracing::info!(\"tmux hook 装好了 → SIGUSR1 → 立刻重探\"); }\n";
        assert_eq!(
            naked_a1(log),
            0,
            "日志文案被数成了平台符号 —— 抹字符串那一段坏了"
        );
        // ⑤ B 族：门后一个裸 `true` ⇒ 逮到；换成 `false` ⇒ 不逮。
        let fab = "pub fn e() -> bool {\n    #[cfg(not(unix))]\n    {\n        true\n    }\n}\n";
        assert_eq!(fabricated_success(&production_code(fab)).len(), 1);
        let honest = fab.replace("true", "false");
        assert_eq!(fabricated_success(&production_code(&honest)).len(), 0);
    }

    /// ★★★ `KR55D3`：**头注那句「挡不住等价改写」说的是哪一种改写** —— 逐格切一刀。
    ///
    /// # 它治的是一句**说窄了**的话
    ///
    /// 上一版头注逐字举了个例子：「把 `std::os::unix::fs::…` 写成 `use std::os as o;`
    /// 再 `o::unix::…`，本模块看不见」。🔴 PM 09-11 切刀现打：**那个例子逃不掉** ——
    /// 路径躲开了，而**扩展 trait 的名字还在**，`unix-ext-*` 那一族当场接住它。
    /// ⇒ 一句「它挡不住 X」如果举的 X 其实挡得住，那句话就是**自陈比实情弱**，
    /// 而自陈弱的判据下一轮会被人当成「反正它不管用」而绕开。
    ///
    /// # 本条断的四格（三个单断 + 一个全断，`brief` 第 9 条那个形状）
    ///
    /// 同一段代码的四个版本，只差「避开了哪几条 needle」：
    /// ① 路径 · ② 扩展 trait 名 · ③ `.mode(0o` 那个字面量 —— 各**只**放回一条 ⇒ 必须被逮到；
    /// 三条全避开 ⇒ **逃得掉**（那就是头注那句话今天的准确射程）。
    ///
    /// ⚠ **它不是在教人怎么绕过本模块**：这四格钉的是「头注那句话说得准不准」。
    /// 真判据一直是跨 target 编译（头注最后一条），本模块从来只是它够不着时的那一声。
    #[test]
    fn a_rewrite_only_escapes_when_it_dodges_every_needle_that_lands_on_it() {
        // 三样各自的「避开写法」与「原样写法」，其余字节逐字相同。
        const ESCAPE: &str = "use std::os as o;\n\
                              use self::o::unix::prelude::*;\n\
                              pub fn land(p: &std::path::Path) {\n    \
                              let mut opts = std::fs::OpenOptions::new();\n    \
                              opts.write(true).create_new(true).mode(493);\n    \
                              let _ = opts.open(p);\n}\n";
        // ① 只把**路径**放回去（trait 名与字面量仍然避开着）。
        let with_path = ESCAPE.replace(
            "use self::o::unix::prelude::*;",
            "use std::os::unix::prelude::*;",
        );
        // ② 只把**扩展 trait 名**放回去。
        let with_trait = ESCAPE.replace(
            "use self::o::unix::prelude::*;",
            "use self::o::unix::fs::OpenOptionsExt;",
        );
        // ③ 只把**那个字面量**放回去（八进制权限位）。
        let with_mode = ESCAPE.replace(".mode(493)", ".mode(0o755)");

        // 反空真排最前：三份「只放回一条」的样本必须**互不相同**，也都不等于 ESCAPE，
        // 否则下面三条单断里有的是拿同一份文本断了三遍。
        for (name, s) in [
            ("路径", &with_path),
            ("trait 名", &with_trait),
            ("字面量", &with_mode),
        ] {
            assert_ne!(
                *s, ESCAPE,
                "「只放回{name}」那一份与全避开那一份逐字相同 —— 替换没落地"
            );
        }

        // 单断三格：每一格都**只**放回一条，仍然被逮到 ⇒ 那一条 needle 各自真有牙。
        assert_eq!(
            naked_a1(&with_path),
            1,
            "只把**路径**写回去就逃掉了 —— `std-os-*` 那一族没牙"
        );
        assert_eq!(
            naked_a1(&with_trait),
            1,
            "只把**扩展 trait 名**写回去就逃掉了 —— 而 PM 09-11 现打的正是这一格：\n\
             上一版头注举的那个「`use std::os as o;`」例子就是被它接住的"
        );
        assert_eq!(
            naked_a1(&with_mode),
            1,
            "只把 `.mode(0o…)` 那个字面量写回去就逃掉了 —— `unix-mode-bits` 没牙"
        );

        // 全断那一格：三样同时避开 ⇒ **真的逃得掉**。
        // 🔴 这一格红（数不是 0）= 头注那句话**又说窄了一次**：它列的三样不是全部，
        //    还有第四条 needle 落在这段代码上 ⇒ 回去把那一条也写进头注，别把本条改宽。
        assert_eq!(
            naked_a1(ESCAPE),
            0,
            "三样全避开的那份样本竟然被逮到了 —— 头注那句「挡不住同时避开这三样的改写」\n\
             说窄了一次：本模块今天比它自陈的更严。回去把接住它的那条 needle 一起写进头注。"
        );
    }

    /// ★ [`SIGNALS`] 的每一条都要说得出「为什么它算平台」，且 needle 不许是空串。
    ///
    /// 空 needle 会匹配到任何地方 ⇒ 等于把判据关掉；说不出理由的信号下一轮没人敢动。
    #[test]
    fn every_signal_carries_its_reason() {
        assert!(
            SIGNALS.len() >= 20,
            "信号表只剩 {} 条 —— 它在缩水",
            SIGNALS.len()
        );
        for (name, tier, _, _, needle, why) in SIGNALS {
            assert!(!needle.is_empty(), "信号 `{name}` 的 needle 是空串");
            assert!(
                *tier == "A1" || *tier == "A2",
                "信号 `{name}` 的档不认识：{tier}"
            );
            assert!(
                why.chars().count() >= 10,
                "信号 `{name}` 说不出为什么它算平台（只有 {} 字）",
                why.chars().count()
            );
        }
        for (p, anchor, pile, why) in REGISTERED {
            assert!(
                *pile == "真漏" || *pile == "合法线外",
                "`REGISTERED` 里 {p} :: {anchor} 的堆不认识：{pile}"
            );
            assert!(
                why.chars().count() >= 20,
                "`REGISTERED` 里 {p} :: {anchor} 的理由只有 {} 字 —— \
                 一条说不出理由的登记就是一条免检",
                why.chars().count()
            );
        }
    }
}
