//! 判据用：把一家的默认上游换成给的那一份（真上游换成假上游）。

use super::{UpstreamPick, Upstreams};

pub(crate) fn with_pick(mut u: Upstreams, agent: &'static str, pick: UpstreamPick) -> Upstreams {
    u.by_agent.insert(agent, pick);
    u
}
