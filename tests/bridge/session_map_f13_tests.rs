use super::liveness_from_stat;

/// 一份形状正常的 `/proc/<pid>/stat`：第 22 字段是 starttime。
fn stat_with_starttime(start: &str) -> String {
    let mut f: Vec<String> = (1..=51).map(|i| i.to_string()).collect();
    f[1] = "(claude)".into(); // comm 带括号，解析要能跨过去
    f[21] = start.into(); // 第 22 字段（0-based 21）
    f.join(" ")
}

/// ★★ **「字段解析不出」不等于「进程不在」**〔audit-0805 F13 / 报告 I-14〕。
///
/// 原来最后一行是 `proc_stat_starttime(&raw).is_some_and(|got| got == want)` ——
/// 解析不出时给 `false` ⇒ **判死**。而后端侧 `platform/liveness.rs` 对同一格逐字写着
/// 「Do not archive a still-existing PID on missing start info.」⇒ **同一件事两个方向**。
///
/// `/proc/<pid>/stat` 都读到了就说明那个 pid **还在**；解析不出只是「我认不出它是不是
/// 同一个进程」。判死会让一个**活着的会话被归档** —— 用户看到它无故变灰。
#[test]
fn an_unparseable_starttime_does_not_archive_a_living_process() {
    // 读到了 /proc，但内容不成形 ⇒ 解析不出 starttime。
    assert!(
        liveness_from_stat("garbage without fields", Some("12345")),
        "★ 解析不出 starttime 就判死 —— 那会把一个活着的会话归档。\n\
             /proc 都读到了就说明进程还在；解析不出只是「认不出是不是同一个」。\n\
             backend 侧 platform/liveness.rs 逐字：Do not archive a still-existing PID on \n\
             missing start info. 两边必须同向。"
    );
}

/// 反向三条：**别把上面写成恒真**。
#[test]
fn the_starttime_comparison_still_discriminates() {
    assert!(
        liveness_from_stat(&stat_with_starttime("999"), Some("999")),
        "抽取器自检：连正常匹配都判死，说明夹具或解析器不对"
    );
    assert!(
        !liveness_from_stat(&stat_with_starttime("111"), Some("999")),
        "starttime 对不上 = pid 被复用给了别的进程 ⇒ 必须判死，否则 PID 复用检测形同虚设"
    );
    assert!(
        liveness_from_stat(&stat_with_starttime("111"), None),
        "没有 baseline ⇒ 退到存在性（同 Windows 侧与后端侧）"
    );
}
