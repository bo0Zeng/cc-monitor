//! [`super`] 的判据：本机后端没连着那几样照同一张字表出点 · 语气 · 词 · 句（期望逐格手写，不从被测那几张表拼）。

use super::*;

#[test]
fn every_local_case_lands_on_its_dot_tone_word_and_sentence() {
    let m = "这台电脑";
    let want = |dot, tone, word: &str, said: String| LocalLook {
        dot,
        tone,
        word: copy_text(word, &[]),
        said,
    };
    let cases = [
        (
            Local::Starting,
            want(
                MachineDot::Pending,
                Tone::Busy,
                "machineState.word.connecting",
                copy_text(
                    "machineState.busy.connecting",
                    &[
                        ("machine", m),
                        ("stage", &copy_text("machineState.stage.attach", &[])),
                    ],
                ),
            ),
        ),
        (
            Local::Older,
            want(
                MachineDot::Warn,
                Tone::Warn,
                "machineState.word.needsUpdate",
                copy_text("front.title.update", &[("machine", m)]),
            ),
        ),
        // 口上那一份比这一版新也是没连着：点画成没连着（远端 `Newer` 那一态是连着的点），与出错那一句对上。
        (
            Local::Newer,
            want(
                MachineDot::Failed,
                Tone::Fail,
                "machineState.word.newer",
                copy_text("machineState.problem.newer", &[("machine", m)]),
            ),
        ),
        (
            Local::Down { failed: None },
            want(
                MachineDot::Failed,
                Tone::Fail,
                "machinePage.state.down",
                copy_text("machineState.down.local", &[]),
            ),
        ),
        (
            Local::Down {
                failed: Some("起不来的那一句"),
            },
            want(
                MachineDot::Failed,
                Tone::Fail,
                "machinePage.state.down",
                "起不来的那一句".to_string(),
            ),
        ),
    ];
    for (l, w) in cases {
        assert_eq!(local(l, m), w, "{l:?}");
    }
}

/// 本机正在起 · 口上旧与远端同一态那一格逐格一样（同一张表，不另写一份）。
#[test]
fn starting_and_older_are_the_remote_states_cell_for_cell() {
    let m = "这台电脑";
    for (l, k, stage) in [
        (Local::Starting, K::Connecting, Some(Stage::Attach)),
        (Local::Older, K::NeedsUpdate, None),
    ] {
        let got = local(l, m);
        assert_eq!(got.dot, dot(k));
        assert_eq!(Some(got.tone), strip(k, None, true).map(|s| s.tone));
        assert_eq!(got.word, word(k, None));
        assert_eq!(got.said, said(k, stage, None, m));
    }
}
