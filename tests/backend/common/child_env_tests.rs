//! 要求：常驻后端读完自有的那几格环境就不再往下传（监听口的钥匙「常驻后端读完即从自己的环境里删」，体检 S3-I2 ＋ 主会话 10-02 追加：
//! 后端起的 tmux / ccm / claude 都带着常驻后端的环境）。

use super::*;

/// 名单 == 起常驻后端的那一方只交给它自己用的那几格（手写期望，异源：逐个现取各模块的常量）。
#[test]
fn own_envs_are_exactly_what_the_host_hands_the_backend_for_itself() {
    let got: std::collections::BTreeSet<&str> = OWN_ENVS.into_iter().collect();
    let want: std::collections::BTreeSet<&str> = [
        "CCM_LISTEN_PORT",
        "CCM_LISTEN_TOKEN_FILE",
        "CCM_BACKEND_STDERR_LOG",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
    // 远端 `--resident-ensure` 交给常驻载体的那几格 == 这一份 ＋ 中转口（中转口要往下传给窗格里的 ccm）。
    let handed: std::collections::BTreeSet<String> = crate::control::resident::child_env(
        1,
        std::path::Path::new("/h/.cc-monitor/listen-token"),
        std::path::Path::new("/h"),
        &[
            (crate::stream::listen::RELAY_PORT_ENV, "8788".into()),
            (crate::stderr_log::ENV, "~/x.log".into()),
        ],
    )
    .into_iter()
    .map(|(k, _)| k)
    .collect();
    let mut want_handed: std::collections::BTreeSet<String> =
        want.iter().map(|s| s.to_string()).collect();
    want_handed.insert(crate::stream::listen::RELAY_PORT_ENV.to_string());
    assert_eq!(
        handed, want_handed,
        "起常驻载体交的 ≠ 清掉的那一份 ＋ 中转口"
    );
    assert!(
        !OWN_ENVS.contains(&crate::stream::listen::RELAY_PORT_ENV),
        "中转口不许清：窗格里的 ccm 靠它找同机中转，清掉就去连默认口"
    );
}

/// 行为：过了它的 `Command` 起出来的子进程环境里一格自有的都没有（即使在它之前显式设过），故意交的（家）与别的照旧。
#[cfg(unix)]
#[test]
fn a_child_started_through_it_carries_none_of_the_backend_own_envs() {
    let mut cmd = std::process::Command::new("env");
    for k in OWN_ENVS {
        cmd.env(k, "leak");
    }
    cmd.env("CCM_DATA_DIR", "/iso/home").env("KEEP_ME", "1");
    let out = cmd.without_own_env().output().expect("起 env");
    let text = String::from_utf8_lossy(&out.stdout);
    let names: std::collections::BTreeSet<&str> = text
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, _)| k))
        .collect();
    for k in OWN_ENVS {
        assert!(!names.contains(k), "子进程拿到了 {k}：{text}");
    }
    assert!(
        names.contains("CCM_DATA_DIR") && names.contains("KEEP_ME"),
        "正控：故意交的 / 无关的那几格该在：{text}"
    );
}

/// 结构：生产段每一处 `Command::new(…)` 紧跟 `.without_own_env()` —— 例外恰好是 `control/ccm/mod.rs` 那两处
/// （`ccm` 那一趟往下起，沿用调用者的环境；常驻后端起它那一下已经清过）。人群与 `readonly_guard` 那张起进程登记同一把抽取法。
#[test]
fn every_production_spawn_goes_through_it() {
    let src = crate::guard_support::src_root();
    let mut sites: Vec<(String, bool)> = Vec::new();
    for (p, raw) in guard_core::scan_tree_excluding(&src, &["rs"], &[]) {
        let rel = p
            .strip_prefix(&src)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        let prod = guard_core::production_code(&raw);
        for (at, _) in prod.match_indices("Command::new(") {
            let tail = &prod[at + "Command::new(".len()..];
            // 实参到配平的右括号为止（实参都是单行的简单式子）。
            let mut depth = 1usize;
            let close = tail
                .char_indices()
                .find(|(_, c)| {
                    match c {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    depth == 0
                })
                .map(|(i, _)| i)
                .expect("`Command::new(` 没配平");
            let next = tail[close + 1..].trim_start();
            sites.push((rel.clone(), next.starts_with(".without_own_env()")));
        }
    }
    let bare: Vec<&str> = sites
        .iter()
        .filter(|(_, ok)| !ok)
        .map(|(f, _)| f.as_str())
        .collect();
    assert_eq!(
        bare,
        vec!["control/ccm/mod.rs", "control/ccm/mod.rs"],
        "这几处起子进程没过 `without_own_env`：{sites:?}"
    );
    // 反空真：人群与起进程登记那张表数的是同一个数（`readonly_guard::SPAWN_SITES_TODAY`；终端管理那一处 18 → 19）。
    assert_eq!(
        sites.len(),
        19,
        "抽到的起进程处数不对 —— 抽取坏了：{sites:?}"
    );
}
