//! 收事实那几样里能离线判的：PATH 上的 ccm 是不是我们那一份 · 启动文件里确证失效的行。

use super::*;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "st8-gather-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn path_上先找到的是我们那一份_不报_别的那一份_报() {
    let h = tmp("ccm");
    let ours = h.join(".cc-monitor/bin");
    let other = h.join("other");
    std::fs::create_dir_all(&ours).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(ours.join("ccm"), "x").unwrap();
    std::fs::write(other.join("ccm"), "x").unwrap();
    let p = std::env::join_paths([&ours, &other]).unwrap();
    assert!(stale_ccm(&h, p.to_str(), false).is_none());
    let p = std::env::join_paths([&other, &ours]).unwrap();
    let s = stale_ccm(&h, p.to_str(), false).unwrap();
    assert_eq!(s.path, other.join("ccm").display().to_string());
    assert!(!s.needs_root, "家目录底下的不用 sudo");
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn 失效行_只报_source_的文件不在的_围栏里与注释与说不清的不报() {
    let h = tmp("dead");
    std::fs::write(h.join("here.sh"), "").unwrap();
    let text = "source ~/here.sh\nsource ~/gone.sh\n. $HOME/gone2\n# source ~/gone3\nsource $FOO/x\n# === cc-monitor BEGIN ===\nsource ~/gone4\n# === cc-monitor END ===\nsource \"/nope/abs\"\n";
    let got = crate::platform::shell::posix::dead_source_lines(text, Some(&h));
    let nums: Vec<usize> = got.iter().map(|(n, _)| *n).collect();
    assert_eq!(nums, vec![2, 3, 9]);
    let _ = std::fs::remove_dir_all(&h);
}

/// 被「文件在才读」守着的 source（`[ -f X ]` · `[ -r X ]` · `test -f X`；同一行 `&&`、或包在 `if … then … fi` 里）——
/// 文件不在是正常的，不是失效；守的是别的文件、或裸 source 一个不在的文件 ⇒ 照报。
#[test]
fn 失效行_文件在才读的不报_守着别的文件的与裸的照报() {
    let h = tmp("guarded");
    let text = [
        "if [ -f ~/.extra_aliases ]; then",                   // 1
        "    . ~/.extra_aliases",                             // 2 守着
        "fi",                                                 // 3
        "[ -f ~/gone-a ] && . ~/gone-a",                      // 4 守着
        "[ -r \"$HOME/gone-b\" ] && source \"$HOME/gone-b\"", // 5 守着
        "test -f ~/gone-c && . ~/gone-c",                     // 6 守着
        "if test -r ~/gone-d; then source ~/gone-d; fi",      // 7 守着
        "if [ -f ~/gone-e ]",                                 // 8
        "then",                                               // 9
        "  echo x",                                           // 10
        "  . $HOME/gone-e",                                   // 11 守着（同一个文件的另一种写法）
        "fi",                                                 // 12
        ". ~/gone-bare",                                      // 13 裸的 ⇒ 报
        "if [ -f ~/other ]; then",                            // 14
        "  . ~/gone-f",                                       // 15 守的是别的文件 ⇒ 报
        "fi",                                                 // 16
        "[ -f ~/other ] && . ~/gone-g",                       // 17 同上 ⇒ 报
        ". ~/gone-h",                                         // 18 if 已经关了 ⇒ 报
    ]
    .join("\n");
    let got = crate::platform::shell::posix::dead_source_lines(&text, Some(&h));
    let nums: Vec<usize> = got.iter().map(|(n, _)| *n).collect();
    assert_eq!(nums, vec![13, 15, 17, 18]);
    let _ = std::fs::remove_dir_all(&h);
}

/// 根钥匙派生出的只许直通那一把（照中转的派生法在这里现算：HMAC-SHA256(根, "ccm-relay/t") 的小写十六进制）。
fn pass_hex(root: &str) -> String {
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, root.as_bytes());
    ring::hmac::sign(&k, b"ccm-relay/t")
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 一段文本里所有恰好 64 位的小写十六进制串（钥匙的形状）。
fn hex64(s: &str) -> std::collections::BTreeSet<String> {
    s.split(|c: char| !matches!(c, '0'..='9' | 'a'..='f'))
        .filter(|w| w.len() == 64)
        .map(str::to_string)
        .collect()
}

/// ★ 中转钥匙不进界面：这台收事实 → 出「待办」→ 序列化成线上那一份，里面所有钥匙形的串 == {只许直通那一把}。
/// 两份设置文件：没写 · 照旧贴过全权那一把（根钥匙本身，贴过的那一行在 diff 里显示成 `…`、整份里换成新的那一条）。
/// 同一份文件上还有收信那一件（它的 diff 上下文与整份也从那份文件来）。正控：直通那一把确实出现了。
#[test]
fn 待办里的钥匙只有只许直通那一把() {
    let root = "0123456789abcdef".repeat(4);
    let old = format!(
        "{{\n  \"model\": \"x\",\n  \"env\": {{\n    \"ANTHROPIC_BASE_URL\": \"http://127.0.0.1:8788/{root}/t/claude-code/_\"\n  }}\n}}\n"
    );
    for settings in [None, Some(old)] {
        let h = tmp("relay-key");
        std::fs::create_dir_all(h.join(".cc-monitor")).unwrap();
        std::fs::create_dir_all(h.join(".claude")).unwrap();
        std::fs::write(h.join(".cc-monitor/relay-key"), &root).unwrap();
        let file = h.join(".claude/settings.json");
        if let Some(s) = &settings {
            std::fs::write(&file, s).unwrap();
        }
        let relay = relay_at(&h, &[], &|_| true);
        assert!(!relay.is_empty(), "该有「直接敲的也走中转」那几件");
        let f = Facts {
            windows: false,
            needs_install: vec![],
            stale_ccm: None,
            self_paste: None,
            clashes: vec![],
            dead: vec![],
            relay,
            hooks: Some(Hooks {
                ccbus: true,
                installed: false,
                path: file.display().to_string(),
                text: settings.clone(),
                items: vec![("Stop".into(), serde_json::json!({"hooks": []}))],
            }),
            mcp_login: vec![],
            declined: vec![],
        };
        let wire = serde_json::to_string(&super::super::chores(&f)).unwrap();
        assert_eq!(
            hex64(&wire),
            std::collections::BTreeSet::from([pass_hex(&root)]),
            "设置文件 {settings:?} ⇒ {wire}"
        );
        let _ = std::fs::remove_dir_all(&h);
    }
}
