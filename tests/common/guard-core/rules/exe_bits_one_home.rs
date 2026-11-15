//! 测试源码里经 std 直接给文件设可执行位只在 `test_home::write_exe` 一处。
//!
//! 为什么：判据在本进程里直接写一份小程序、紧接着 exec 它，旁边别的判据线程正 fork 时，子进程在 fork 到 exec 之间也攥着那份写 fd
//! ⇒ exec ＝ ETXTBSY，偶发红。`write_exe` 由 `cp` 子进程落盘，本进程从没有过它的写端。目录改权限位（只读 · 进不去 · 改回来）
//! 经 `test_home::set_dir_mode`，它不收文件 —— 所以「带 x 位的权限」在别处出现，就是又有人自己给文件上了执行位。
//!
//! 认法（注释与串的内容先抹掉）：`fs::set_permissions(…)` 或 `set_mode(…)` 的实参里有带 x 位（`& 0o111 != 0`）的八进制字面量。
//! 看不见的：权限值经变量 / 闭包 / 别的助手传进去的、十进制或十六进制写的、方法形与裸名的 `set_permissions(…)`
//! （`File::set_permissions` · 产品的 `face.set_permissions` —— 后者测的是产品自己改权限，不算违规）、`OpenOptions` · `DirBuilder` 的 `.mode(…)`、
//! 起 `chmod` 子进程的、`fs::copy` 连权限一起抄的。会误认的：实参里带 x 位字面量却是在摘掉 x 位的（`m & !0o111`）—— 今天没有这一形。
//! 人群：`tests/` 下的 `.rs`（含本目录），摘掉 `tests/devsched/`：开发调度是独立工作区、按设计不链 `test-home`，
//! 它那一份 `write_exe` 由它自己的 `support.rs` 收着。
//!
//! 5.1.5 立。可以删它的时候：本仓的判据不再自己起子进程 exec 现写的文件；或测试源码里 `write_exe` 以外再没有设可执行位的写法
//! （那时连 `write_exe` 一并删）。

use super::{equal, none, rule, Repo};

/// 唯一设可执行位的住处。
const HOME: &str = "src/common/test-home/src/lib.rs";

/// 摘掉的子树（理由见头注）。
const OUTSIDE: &str = "tests/devsched/";

/// 八进制字面量（`0o` 起头，去下划线、去类型后缀）的值；不是八进制 ⇒ `None`。
fn octal_at(b: &[u8], i: usize) -> Option<(u32, usize)> {
    let starts = b.get(i) == Some(&b'0')
        && b.get(i + 1) == Some(&b'o')
        && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'));
    if !starts {
        return None;
    }
    let mut j = i + 2;
    let mut digits = String::new();
    while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'_') {
        if b[j] != b'_' {
            digits.push(char::from(b[j]));
        }
        j += 1;
    }
    u32::from_str_radix(&digits, 8).ok().map(|v| (v, j))
}

/// `b[open]` 是 `(`：到配对的 `)` 为止那一段（不含两头）。配不上 ⇒ 到尾。
fn args_from(b: &[u8], open: usize) -> &[u8] {
    let mut depth = 0usize;
    for (k, &c) in b.iter().enumerate().skip(open) {
        match c {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return &b[open + 1..k];
                }
            }
            _ => {}
        }
    }
    &b[open + 1..]
}

fn has_exec_bits(seg: &[u8]) -> bool {
    (0..seg.len()).any(|i| octal_at(seg, i).is_some_and(|(v, _)| v & 0o111 != 0))
}

/// 一段源码里设出带 x 位权限的那几处：各给起点的字节偏移。注释与串先抹掉（[`crate::code_only`]）。
fn hits(src: &str) -> Vec<usize> {
    let code = crate::code_only(src);
    let b = code.as_bytes();
    let mut out = Vec::new();
    for needle in ["fs::set_permissions(", "set_mode("] {
        let mut from = 0;
        while let Some(at) = code[from..].find(needle).map(|k| from + k) {
            let open = at + needle.len() - 1;
            let bounded = at == 0 || !(b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_');
            if bounded && has_exec_bits(args_from(b, open)) {
                out.push(at);
            }
            from = open + 1;
        }
    }
    out.sort_unstable();
    out
}

/// `at` 落在哪个函数里（往前找最近的 `fn <名>`）。
fn fn_around(code: &str, at: usize) -> String {
    let mut head = &code[..at];
    while let Some(k) = head.rfind("fn ") {
        let prev = head[..k].chars().next_back();
        if prev.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) {
            return head[k + 3..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
        }
        head = &head[..k];
    }
    String::new()
}

/// `tests/`（摘掉 [`OUTSIDE`]）里设出带 x 位权限的那几处：`<住址> · fn <函数名>`。
fn found(repo: &Repo) -> Vec<String> {
    repo.with_rules()
        .under(&["tests"], &["rs"])
        .into_iter()
        .filter(|f| !super::is_under(&f.rel, OUTSIDE))
        .flat_map(|f| {
            let text = f.text();
            let code = crate::code_only(text);
            hits(text)
                .into_iter()
                .map(|at| format!("{} · fn {}", f.rel, fn_around(&code, at)))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn only_write_exe_sets_exec_bits_in_tests() {
    rule(|repo| {
        let home = repo.file(HOME).text();
        let code = crate::code_only(home);
        let homes: Vec<String> = hits(home)
            .into_iter()
            .map(|at| fn_around(&code, at))
            .collect();
        equal(
            "test-home 里设可执行位的函数",
            homes,
            ["write_exe".to_string()],
        );

        // 正控：在别处直写一份（写完自己 chmod 755 · 改 Permissions 补上 x 位再设回去）当场认得出；
        // 经 write_exe / set_dir_mode 的、不带 x 位的、写在串里 · 注释里的、devsched 那棵里的不算。
        let planted = Repo::synthetic(&[
            (
                "tests/backend/planted_tests.rs",
                "fn a(p: &Path) {\n\
                 \x20   std::fs::write(p, \"#!/bin/sh\\n\").unwrap();\n\
                 \x20   std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();\n\
                 }\n\
                 fn b(p: &Path) {\n\
                 \x20   let mut perm = std::fs::metadata(p).unwrap().permissions();\n\
                 \x20   perm.set_mode(perm.mode() | 0o100);\n\
                 \x20   std::fs::set_permissions(p, perm).unwrap();\n\
                 }\n",
            ),
            (
                "tests/backend/clean_tests.rs",
                "fn c(p: &Path, d: &Path) {\n\
                 \x20   test_home::write_exe(p, \"#!/bin/sh\\n\");\n\
                 \x20   test_home::set_dir_mode(d, 0o755);\n\
                 \x20   std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o644)).unwrap();\n\
                 \x20   let s = \"std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))\";\n\
                 \x20   // std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755));\n\
                 \x20   reset_mode(0o755);\n\
                 }\n",
            ),
            (
                "tests/devsched/support.rs",
                "fn w(p: &Path) { std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap(); }\n",
            ),
        ]);
        let control = found(&planted);
        assert_eq!(
            control,
            [
                "tests/backend/planted_tests.rs · fn a",
                "tests/backend/planted_tests.rs · fn b"
            ],
            "正控认错了：直写的两处该认出、别的一处都不该认"
        );
        none(
            "测试源码在 `test_home::write_exe` 之外给文件设了可执行位（现写现 exec 的改经 `write_exe`；目录改经 `test_home::set_dir_mode`）",
            found(repo),
            control.len(),
        );
    });
}
