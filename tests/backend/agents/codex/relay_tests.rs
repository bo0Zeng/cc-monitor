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
