//! SSH 公钥的 SHA256 指纹只由核心写（`ssh_dial::known_hosts::fingerprint`，桌面与手机同一份）：手机 Kotlin 的生产代码里
//! 不许自己摘要（`MessageDigest`）、不许自己拼 `"SHA256:` 那一串。指纹要什么就问核心（`Core.publicKeyOf` · `Core.publicKeyOfPoint`）。
//!
//! 认法：`src/mobile/**/src/main/**/*.kt` 原文里这两个记号。换个名字调摘要的、拼出来的，这里看不见。

use super::{none, rule};

/// 两个记号：摘要那个类 · 指纹那一串的开头（带引号：字面量）。
const MARKS: [&str; 2] = ["MessageDigest", "\"SHA256:"];

/// 一段 Kotlin 里命中的记号（按 [`MARKS`] 的先后，各算一次）。
fn hits(code: &str) -> Vec<&'static str> {
    MARKS.into_iter().filter(|m| code.contains(m)).collect()
}

#[test]
fn phone_kotlin_never_computes_an_ssh_fingerprint_itself() {
    rule(|repo| {
        let found: Vec<String> = repo
            .under(&["src/mobile"], &["kt"])
            .into_iter()
            .filter(|f| f.rel.contains("/src/main/"))
            .filter_map(|f| {
                let h = hits(f.text());
                (!h.is_empty()).then(|| format!("{}（{}）", f.rel, h.join(" · ")))
            })
            .collect();
        let control = hits(
            "val d = MessageDigest.getInstance(\"SHA-256\").digest(blob)\nreturn \"SHA256:$b64\"",
        )
        .len();
        none(
            "手机 Kotlin 生产代码自己算 SSH 指纹（改问核心：`Core.publicKeyOf` · `Core.publicKeyOfPoint`）",
            found,
            control,
        );
    });
}
