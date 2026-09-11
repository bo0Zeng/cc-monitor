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
//! `sidecars/codepicture/acquire.rs::land` 引了 `std::os::unix::fs` 里那个给 `mode(…)` 的扩展
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
//! - **等价改写绕得过**：把 `std::os::unix::fs::…` 那一串写成 `use std::os as o;`
//!   再 `o::unix::…`，本模块看不见。这条**刻意不追** —— 完备性在这里做不到，
//!   而本模块挡的是「顺手写一行平台代码」这个真实且高频的形态。
//! - **不认空白变体**：`#[cfg(` 与 `std :: os :: unix` 这类插了空格的写法认不出来。
//!   本 crate 全量过 `cargo fmt --check`（CI 与沙箱门禁各一道）⇒ 今天不会出现；
//!   哪天 fmt 那道门没了，这一条同时失效。
//! - 🔴 **它不是跨 target 编译的替代品。** 真判据一直是
//!   `cargo check --all-targets --target x86_64-pc-windows-msvc`（CI 的 daemon job 那一步，
//!   带 zig 的三个环境变量）。本模块买到的是**在那道门跑不到的地方也能出声**
//!   —— 本机那道 `scripts/verify-committed-state.sh` 的 `daemon-win` 今天卡在 `ring`
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
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
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
            "Command::new(\"sh\")",
            "起 POSIX shell —— Windows 上没有 `sh`（编得过、跑不对）",
        ),
        (
            "posix-shell-bash",
            "A2",
            Where::Literal,
            false,
            "Command::new(\"bash\")",
            "起 POSIX shell（编得过、跑不对）",
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
    /// ⚠ **它买到的是「下不为例」，不是「今天干净」** —— 表里那三条 `真漏` 今天还在门外。
    ///
    /// 形状 = `(文件相对路径, 那一行的逐字锚点, 堆, 理由)`
    const REGISTERED: &[(&str, &str, &str, &str)] = &[
        (
            "observe/watcher.rs",
            "Command::new(\"sh\")",
            "真漏",
            "`run_tmux_ls` / `run_tmux_probe` 那两跳起的是 POSIX shell。\
             该进适配层（`K33` 裁定二），今天没进。\
             ⚠ `K-R52` 的写区不含 `observe/` ⇒ **本件不改，已走上报口交回 PM**。",
        ),
        (
            "observe/watcher.rs",
            "PathBuf::from(\"/tmp\")",
            "真漏",
            "`tmux_socket_dir` 的兜底根目录写死成 POSIX 的 `/tmp`。同一行上方的 `uid` \
             那半**已经**有两条 `#[cfg]` 臂了，而这一半没有 —— 典型的「只修一半」。\
             该和 `platform/paths.rs` 住一起。\
             ⚠ `K-R52` 的写区不含 `observe/` ⇒ **本件不改，已走上报口交回 PM**。",
        ),
        (
            "agents/fake/mod.rs",
            "cmdline(\"/usr/bin/vim\")",
            "合法线外",
            "它是喂给判定器的**反例数据**（断言这条 cmdline **不**被判成 agent），\
             不是本机要去走的路 ⇒ 在任何平台上行为相同，不是平台代码。",
        ),
    ];

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
        assert!(
            scanned >= 60,
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
        assert!(
            files >= 70,
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

    /// ★★ 反空真②：**拿合成夹具证明这把尺子真有牙** —— 不靠真树上碰巧有没有病灶。
    ///
    /// 四刀，两正两反。反的那两刀是本模块最容易坏的两处：
    /// 剥注释坏掉 ⇒ 散文被数成代码（这棵树的注释里**大量逐字写着代码片段**）；
    /// 抹字符串坏掉 ⇒ 日志文案被数成平台符号（`watcher.rs` 那句 `SIGUSR1` 实打命中过一次）。
    #[test]
    fn the_detector_bites_on_synthetic_defects_and_not_on_prose() {
        let naked_a1 = |src: &str| {
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
        };
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
