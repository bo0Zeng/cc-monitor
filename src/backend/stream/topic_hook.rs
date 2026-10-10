//! 帧里的小成品怎么现算 —— 进程里装的那一个函数（生产：`main` 起来时装 [`super::topic_body::body_now`]；没装 ⇒ 不带，客户端重问）。
//! 隔这一层的理由住 [`super::topic_body`] 头注。

use super::topic::Topic;

/// 现算一个主题的小成品（`key` ＝ 帧里那一格）。
pub type Bodies = fn(Topic, Option<&str>) -> Option<serde_json::Value>;

static HOOK: std::sync::OnceLock<Bodies> = std::sync::OnceLock::new();

/// 装上（进程里第一次装的那一个算数）。
pub fn install(f: Bodies) {
    HOOK.get_or_init(|| f);
}

/// 现算；没装 ⇒ `None`。
pub(crate) fn body(topic: Topic, key: Option<&str>) -> Option<serde_json::Value> {
    HOOK.get().and_then(|f| f(topic, key))
}
