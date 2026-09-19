use super::{nudge_should_skip, pack_nudge_state};

#[test]
fn same_size_same_state_skips() {
    let a = pack_nudge_state(2560, 1400, false);
    assert!(nudge_should_skip(a, pack_nudge_state(2560, 1400, false)));
}

#[test]
fn first_nudge_never_skips() {
    assert!(!nudge_should_skip(0, pack_nudge_state(800, 600, false)));
}

#[test]
fn size_change_runs() {
    let a = pack_nudge_state(2560, 1400, false);
    assert!(!nudge_should_skip(a, pack_nudge_state(2560, 1399, false)));
    assert!(!nudge_should_skip(a, pack_nudge_state(2559, 1400, false)));
}

#[test]
fn fullscreen_bit_distinguishes_same_inner_size() {
    // F11 无边框全屏与 maximize 在自动隐藏任务栏下 inner 尺寸可能相同——
    // 全屏态翻转必须照跑三板斧（#4095 高危过渡）
    let maxed = pack_nudge_state(2560, 1440, false);
    let fs = pack_nudge_state(2560, 1440, true);
    assert_ne!(maxed, fs);
    assert!(!nudge_should_skip(maxed, fs));
    assert!(!nudge_should_skip(fs, maxed));
}

#[test]
fn pack_no_collision_between_dimensions() {
    // w/h 位域独立：宽高互换、进位不串位
    assert_ne!(pack_nudge_state(1, 2, false), pack_nudge_state(2, 1, false));
    assert_ne!(
        pack_nudge_state(0x10000, 0, false),
        pack_nudge_state(0, 0x10000, false)
    );
}
