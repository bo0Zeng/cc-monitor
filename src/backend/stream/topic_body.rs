//! 帧里的小成品现算（`changed.body`）：照主题表的 `reask` 那条命令、跑它自己的那个处理器（与客户端重问拿到的是同一份，实现只一处）。
//!
//! 只由 `main` 装进 [`crate::wire::topic_hook`]（发端经那一层调）：这一份够得着每一条命令的处理器，
//! 发端（watcher · tap）被几乎整棵树引用，直接引它会让「这条命令够不够得着 X」那几张按文件引用图算的判据把每条命令都连上。

use crate::wire::topic::Topic;
use crate::wire::typed::{Cx, Handle as _};
use crate::wire::Changed;

/// 现算一个主题带体的那一臂：体就是重问那条命令的回包（类型照那一臂）。
/// 不现算的主题 · 要 `key` 却没给 ⇒ `None`（发端自己造那一臂）；那条命令答不成 ⇒ 那一臂不带体（客户端重问；上限在 [`crate::wire::Frame::changed`] 判）。
/// 同步跑（读那台自己的盘，毫秒级）：发端在一阵变化之后调一次。
/// `tz` ＝ 这条连接看的那一台的时区（attach 带来的）：成品里的时刻字按它写，与客户端带同一个 `tz` 重问拿到的是同一份。
pub fn body_now(topic: Topic, key: Option<&str>, tz: &crate::common::time::Tz) -> Option<Changed> {
    let cx = || Cx {
        tz: tz.clone(),
        until: None,
        needs: (),
    };
    Some(match topic {
        Topic::Quota => Changed::Quota {
            body: crate::wire::spec::finish(crate::faces::rotation_face::QuotaReadCmd::run(
                cmd_wire::quota::QuotaReadReq {},
                cx(),
            ))
            .ok(),
        },
        Topic::RotationStrategies => Changed::RotationStrategies {
            body: crate::wire::spec::finish(
                crate::faces::rotation_view_face::RotationStrategies::run(Default::default(), cx()),
            )
            .ok(),
        },
        Topic::Tasks => {
            let sid = key?.to_string();
            Changed::Tasks {
                body: crate::wire::spec::finish(crate::faces::feature_face::TasksListCmd::run(
                    crate::faces::feature_face::TasksListReq { sid: sid.clone() },
                    cx(),
                ))
                .ok(),
                key: sid,
            }
        }
        _ => return None,
    })
}
