//! `trip.rs`：一趟的那一格（[`Lane`]）。

use super::*;

/// 落地那一下：结局在、趟数加一、「在做哪一项」清掉；拿到的每一份（`Clone`）看的是同一格。
#[test]
fn a_lane_lands_once_and_every_copy_sees_the_same_ending() {
    let lane: Lane<u32> = Lane::default();
    let seen = lane.clone();
    assert_eq!((seen.rounds(), seen.last()), (0, None), "还没起就有结局");
    lane.begin("a.txt");
    assert_eq!(seen.running().as_deref(), Some("a.txt"));
    lane.finish(7);
    assert_eq!(
        (seen.rounds(), seen.last(), seen.running()),
        (1, Some(7), None)
    );
}
