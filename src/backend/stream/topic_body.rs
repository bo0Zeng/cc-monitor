//! 帧里的小成品现算（`changed.body`）：照主题表的 `reask` 那条命令、用它在命令表里登记的同一个处理器跑一次。
//!
//! 只由 `main` 装进 [`super::topic_hook`]（发端经那一层调）：这一份够得着命令表、也就够得着每一条命令的处理器，
//! 发端（watcher · tap）被几乎整棵树引用，直接引它会让「这条命令够不够得着 X」那几张按文件引用图算的判据把每条命令都连上命令表。

use super::topic::Topic;

/// 现算一个主题的小成品：照主题表的 `reask` 那条命令，用它登记的那个处理器跑一次（与客户端重问拿到的是同一份，实现只一处），回它的应答。
/// 不现算的主题 · 要 `key` 却没给 · 那条命令答不成 ⇒ `None`（帧里不带，客户端重问；上限在 [`super::wire::Frame::changed`] 判）。
/// 同步跑（读那台自己的盘，毫秒级）：发端在一阵变化之后调一次。
pub fn body_now(topic: Topic, key: Option<&str>) -> Option<serde_json::Value> {
    use crate::stream::inbound::spec::Run;
    let s = topic.spec();
    let args = match s.ask {
        super::topic::Ask::None => return None,
        super::topic::Ask::NoArgs => serde_json::json!({}),
        super::topic::Ask::Sid => serde_json::json!({ "sid": key? }),
        super::topic::Ask::Sids => serde_json::json!({ "sids": [key?] }),
    };
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|c| c.name == s.reask)?;
    let req = super::wire::Request {
        id: String::new(),
        cmd: s.reask.to_string(),
        args,
        within_ms: None,
        // 不带声明 ＝ 全量：帧里的小成品与客户端不带 view 重问拿到的是同一份
        view: serde_json::Value::Null,
        until: None,
    };
    match spec.run {
        Run::Blocking(f) => f(req).ok().flatten(),
        Run::BlockingData(f) => f(req).ok().flatten(),
        Run::Async(_) | Run::AsyncData(_) | Run::Builtin => None,
    }
}
