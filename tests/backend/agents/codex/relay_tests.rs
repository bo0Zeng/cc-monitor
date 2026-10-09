//! Codex 经中转那一份：起会话垫的参数里只有不带钥匙的地址、钥匙走钥匙头的环境变量；直接敲的 Codex 那份配置的顶层地址读得出、读不懂的说读不懂。
//! 期望值手写（地址形状照中转构造口的产物）。

use super::*;

#[test]
fn launch_args_point_a_provider_at_the_relay_and_carry_no_key() {
    let url = "http://127.0.0.1:8788/t/codex/_";
    let got = launch_args(url);
    let want: Vec<String> = [
        "-c",
        "model_provider=\"ccm\"",
        "-c",
        "model_providers.ccm.name=\"cc-monitor\"",
        "-c",
        "model_providers.ccm.base_url=\"http://127.0.0.1:8788/t/codex/_\"",
        "-c",
        "model_providers.ccm.wire_api=\"responses\"",
        "-c",
        "model_providers.ccm.requires_openai_auth=true",
        "-c",
        "model_providers.ccm.env_http_headers.X-Cc-Monitor-Key=\"CCM_RELAY_KEY\"",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(got, want);
}

#[test]
fn the_top_level_base_url_is_read_and_odd_shapes_are_unreadable() {
    use crate::agents::{SettingsBaseUrl as S, SettingsUnreadable as W};
    let bad = S::Unreadable(W::BadShape);
    let set = |u: &str| S::Set(u.to_string());
    for (raw, want) in [
        ("", S::Unset),
        ("model = \"x\"\n", S::Unset),
        ("openai_base_url = \"\"\n", S::Unset),
        (
            "openai_base_url = \"http://a/t/codex/_\"\n",
            set("http://a/t/codex/_"),
        ),
        (
            "\u{feff}  openai_base_url='http://a' # 注\n",
            set("http://a"),
        ),
        (
            "\"openai_base_url\" = \"http://a\\\\b\"\n",
            set("http://a\\b"),
        ),
        // 表头之后的同名键不是顶层那一格。
        ("[profiles.x]\nopenai_base_url = \"http://a\"\n", S::Unset),
        ("openai_base_url = 3\n", bad.clone()),
        ("openai_base_url = \"http://a\n", bad.clone()),
        (
            "openai_base_url = \"a\"\nopenai_base_url = \"b\"\n",
            bad.clone(),
        ),
        ("openai_base_url = \"\"\"x\"\"\"\n", bad.clone()),
    ] {
        assert_eq!(config_base_url(raw), want, "{raw:?}");
    }
    assert_eq!(
        config_snippet("http://127.0.0.1:8788/k/t/codex/_"),
        "openai_base_url = \"http://127.0.0.1:8788/k/t/codex/_\""
    );
}

/// 「要你动手」那一件的合法：顶层那一行在 ⇒ 就地换那一行；不在 ⇒ 放到文件最前面（表头之前才是顶层）；
/// 合好的那份再读一遍就是那条地址，别的行一个字不动；现在的内容读不懂 ⇒ 不给改法。
#[test]
fn merging_the_base_url_line_keeps_everything_else_and_reads_back() {
    let url = "http://127.0.0.1:8788/k/t/codex/_";
    let line = format!("openai_base_url = \"{url}\"");
    for (now, want) in [
        (String::new(), format!("{line}\n")),
        (
            "model = \"m\"\n".to_string(),
            format!("{line}\nmodel = \"m\"\n"),
        ),
        (
            "model = \"m\"\nopenai_base_url = \"http://old\" # 注\n[tui]\nx = 1\n".to_string(),
            format!("model = \"m\"\n{line}\n[tui]\nx = 1\n"),
        ),
        (
            "[profiles.p]\nopenai_base_url = \"http://p\"\n".to_string(),
            format!("{line}\n[profiles.p]\nopenai_base_url = \"http://p\"\n"),
        ),
    ] {
        let got = config_merge(&now, url).expect("合得出");
        assert_eq!(got, want, "{now:?}");
        assert_eq!(
            config_base_url(&got),
            crate::agents::SettingsBaseUrl::Set(url.into())
        );
    }
    assert_eq!(config_merge("openai_base_url = 3\n", url), None);
}
