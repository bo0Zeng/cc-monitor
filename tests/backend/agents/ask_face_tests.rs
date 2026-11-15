//! 答题口那几个注册表上的读法：插件环境变量清单（`plugin_envs_among`）。
//! 一律拿替身家（`fake::ASK` · `fake::ASK_PLUGIN`）拼注册表喂，期望逐字手写。

use super::*;
use crate::agents::fake;

/// 报 `env` 那个插件变量的一家替身。
fn row_with_plugin(kind: &'static str, env: &'static str) -> Adapter {
    Adapter {
        kind,
        asks: Some(AskFace {
            plugin: Some(PluginFace {
                env,
                ..fake::ASK_PLUGIN
            }),
            ..fake::ASK
        }),
        ..fake::bare_row(fake::home)
    }
}

/// ★ 两家各报同一个变量、中间隔着别的一家 ⇒ 清单里只剩一个；按注册表顺序、留第一次出现的位置。
/// 没有插件那一路的家不进清单。
#[test]
fn the_same_plugin_var_from_two_families_apart_is_listed_once_in_registry_order() {
    let reg = [
        row_with_plugin("fake-a", "FAKE_SHARED_PLUGINS"),
        row_with_plugin("fake-b", "FAKE_B_PLUGINS"),
        fake::bare_row(fake::home),
        row_with_plugin("fake-c", "FAKE_SHARED_PLUGINS"),
    ];
    assert_eq!(
        plugin_envs_among(&reg),
        vec!["FAKE_SHARED_PLUGINS", "FAKE_B_PLUGINS"]
    );
}
