use super::*;

/// 判据用：跑 `f`，回它的结果 ＋ 期间契约错记进日志的那几条诊断（句子里不带诊断，判据要分「哪一格错」就读这里）。
pub(crate) fn diag<T>(f: impl FnOnce() -> T) -> (T, String) {
    let mut out = None;
    let lines = crate::stream::run_route::tests::heard(|| out = Some(f()));
    let said = lines
        .iter()
        .filter(|l| l.contains("contract: malformed request:"))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    (out.expect("f 跑完了"), said)
}

/// 契约错那一句只说「请求格式不对」：英文诊断不上句子（记一行 warn 日志），任何诊断都回同一句。
#[test]
fn the_contract_sentence_carries_no_diagnostic() {
    let (said, heard) = diag(|| malformed("missing `to` (string)"));
    assert_eq!(said, copy_core::copy_text("beContract.malformed.say", &[]));
    assert!(!said.contains("missing"), "诊断进了句子：{said}");
    assert!(
        heard.contains("missing `to` (string)"),
        "诊断没记进日志：{heard}"
    );
    assert_eq!(malformed("another diag"), said);
}
