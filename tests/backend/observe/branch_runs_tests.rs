//! 流上的 `session_branch` 帧带分好的段：冷接那一帧整份带段；之后又回退、新来的接进了前一段 ⇒ 那一帧从那一段的段首起整段重发，
//! 框头那一句照新条数写好。夹具只造结构（uuid · 父子链），正文是占位字。
use super::*;

fn said(n: usize) -> crate::common::cells::Words {
    crate::common::cells::Words(copy_core::copy_text(
        "branchFold.wrapRun.summary",
        &[("n", &n.to_string())],
    ))
}

fn run(at: u64, n: usize) -> crate::wire::BranchRun {
    crate::wire::BranchRun {
        at,
        n: n as u64,
        said: said(n),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn branch_frames_carry_runs_and_resend_the_run_that_grew_from_its_head() {
    let dir = test_home::scratch("branch-runs");
    let proj = dir.join("projects").join("proj-r");
    std::fs::create_dir_all(&proj).unwrap();
    let pid = std::process::id();
    let ticks = proc_starttime(pid).expect("own starttime");
    let jsonl = proj.join("runs-sid.jsonl");
    let line = |id: &str, parent: &str, at: &str, ty: &str| {
        format!(
            r#"{{"type":"{ty}","uuid":"{id}","parentUuid":{p},"timestamp":"{at}","message":{{"role":"{ty}","content":"w"}}}}"#,
            p = if parent.is_empty() {
                "null".to_string()
            } else {
                format!("\"{parent}\"")
            }
        ) + "\n"
    };
    // u2 回退掉（u3 接在同一个 a1 下面、更新）。
    let mut body = line("u1", "", "t1", "user")
        + line("a1", "u1", "t2", "assistant").as_str()
        + line("u2", "a1", "t3", "user").as_str()
        + line("u3", "a1", "t4", "user").as_str();
    std::fs::write(&jsonl, &body).unwrap();
    let (tx, mut rx) = frame_channel(CHANNEL_MAX_BYTES.as_len());
    let mut sink = FrameSink::wanting_branches(tx, &["runs-sid"]);
    let mut state = ReaderState::new(dir.join("projects"));
    let pidfile = dir.join(format!("{pid}.json"));
    std::fs::write(
        &pidfile,
        format!(r#"{{"pid":{pid},"sessionId":"runs-sid","cwd":"/p","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    process_session_added(&pidfile, &mut state, &mut sink);
    let drain = |rx: &mut FrameRx| {
        let mut v = Vec::new();
        while let Ok(f) = rx.try_recv() {
            if let Frame::SessionBranch(crate::wire::SessionBranch { at, off, runs, .. }) = f {
                v.push((at, off, runs));
            }
        }
        v
    };
    assert_eq!(
        drain(&mut rx),
        [(0, vec!["u2".to_string()], vec![run(0, 1)])]
    );
    // 又在 a1 下回退重发（u4 更新）⇒ u3 也回退：它与 u2 之间没有进界面的主线 ⇒ 接进 u2 那一段（×2），
    // 从那一段的段首（第 0 条）起整段重发，不是「前 1 条照旧」。
    body += &line("u4", "a1", "t5", "user");
    std::fs::write(&jsonl, &body).unwrap();
    process_jsonl(&jsonl, &mut state, &mut sink);
    assert_eq!(
        drain(&mut rx),
        [(0, vec!["u2".to_string(), "u3".to_string()], vec![run(0, 2)])]
    );
    std::fs::remove_dir_all(&dir).ok();
}
