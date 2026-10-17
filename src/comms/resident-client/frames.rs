//! **帧门**：流上的一行进不进 —— 只看行首的 `kind` 与握手那一刻比出的那张表（[`Offer::frame_differs`]），不解整行。
//! 远端的常驻流（[`crate::feed`]）与本机两条读循环同一个函数。`hello` 不在帧表里、照过（流中间来的那一帧由宿主照实说）。

use comms_inward::chan::wire::Offer;

/// 一行的去向。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admit {
    /// 形状对得上：照解。
    Pass(String),
    /// 本侧认这种帧、那台那一版形状不同：丢，只记账。
    Differs(String),
    /// 本侧不认这种帧（或这一行读不出 `kind`，带空串）：丢，只记账。
    Unknown(String),
}

/// 这一行进不进。
pub fn admit(offer: &Offer, line: &str) -> Admit {
    let Ok(kind) = cmd_wire::frame::kind_of(line) else {
        return Admit::Unknown(String::new());
    };
    if kind == HELLO {
        Admit::Pass(kind)
    } else if !cmd_wire::frame_shapes::FRAMES
        .iter()
        .any(|(k, _)| *k == kind)
    {
        Admit::Unknown(kind)
    } else if offer.frame_differs(&kind) {
        Admit::Differs(kind)
    } else {
        Admit::Pass(kind)
    }
}

const HELLO: &str = "hello";

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/frames_tests.rs"]
mod tests;
