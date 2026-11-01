//! 契约错的诊断怎么读：本包的判据与上层各族的测试档（经 `test-support`）共用；发布构建里没有。

// 听日志那一份与量具包 `test-meters` 挂的是同一个文件（量具包只许在 dev 侧，这里够不到它）。
#[path = "../heard.rs"]
mod heard;

/// 判据用：跑 `f`，回它的结果 ＋ 期间契约错记进日志的那几条诊断（句子里不带诊断，判据要分「哪一格错」就读这里）。
pub fn diag<T>(f: impl FnOnce() -> T) -> (T, String) {
    let mut out = None;
    let lines = heard::heard(|| out = Some(f()));
    let said = lines
        .iter()
        .filter(|l| l.contains("contract: malformed request:"))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    (out.expect("f 跑完了"), said)
}
