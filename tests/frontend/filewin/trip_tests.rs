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

/// 新的一趟一格新的：结局与趟数不带过去（上一趟的结局不许落在下一趟那一行上）。
#[test]
fn a_fresh_lane_starts_with_no_ending() {
    let lane: Lane<u32> = Lane::default();
    lane.finish(1);
    let next = lane.fresh_like();
    assert_eq!((next.rounds(), next.last()), (0, None));
    next.finish(2);
    assert_eq!(lane.last(), Some(1), "新的一格写到了旧的那一格上");
}
