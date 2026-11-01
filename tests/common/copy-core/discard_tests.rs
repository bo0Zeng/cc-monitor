//! `copy_core::discard`：丢掉一个 `Result` 只经 `.discard(Why::…)`，理由住在调用处；丢的那一下什么也不做。

use super::*;

#[test]
fn discarding_an_err_compiles_and_does_nothing() {
    Err::<(), _>("夹具错误").discard(Why::Diag);
    Ok::<u8, ()>(1).discard(Why::InfallibleWrite);
}

#[test]
fn discarding_runs_the_expression_exactly_once() {
    let mut runs = 0;
    let mut attempt = || -> Result<(), ()> {
        runs += 1;
        Err(())
    };
    attempt().discard(Why::CleanupAfterFailure);
    assert_eq!(runs, 1);
}
