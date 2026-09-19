use super::*;

fn hs(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn never_bound_sid_never_retires() {
    // 从未在后端自报里出现（bg/无 wrapper）→ 缺失多少轮都不 retire。
    let mut st = ReconcileState::default();
    let announced = hs(&["bg1"]);
    let backend = hs(&[]); // 后端一直不报它
    for _ in 0..10 {
        assert!(reconcile_step(&mut st, &announced, &backend, &hs(&[]), 2).is_empty());
    }
}

#[test]
fn bound_then_missing_retires_at_threshold_once() {
    let mut st = ReconcileState::default();
    let announced = hs(&["s"]);
    // 第 1 轮：后端自报在跑 → ever_bound。
    assert!(reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 2).is_empty());
    // 后端消失：第 1 次缺失（miss=1 < 2）→ 不 retire。
    assert!(reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 2).is_empty());
    // 第 2 次缺失（miss=2 >= 2）→ retire 一次。
    assert_eq!(
        reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 2),
        vec!["s".to_string()]
    );
}

#[test]
fn retire_is_idempotent() {
    let mut st = ReconcileState::default();
    let announced = hs(&["s"]);
    reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 1); // bound
    assert_eq!(
        reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 1),
        vec!["s".to_string()]
    );
    // 之后继续缺失也不重复 emit（retired 已置）。
    assert!(reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 1).is_empty());
    assert!(reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 1).is_empty());
}

#[test]
fn rebind_resets_miss() {
    let mut st = ReconcileState::default();
    let announced = hs(&["s"]);
    reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 3); // bound
    reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 3); // miss=1
    reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 3); // miss=2
    reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 3); // 复绑 → miss=0
                                                                   // 再缺 2 轮仍不到阈值 3。
    assert!(reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 3).is_empty());
    assert!(reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 3).is_empty());
}

#[test]
fn sid_leaving_announced_drops_tracking() {
    let mut st = ReconcileState::default();
    reconcile_step(&mut st, &hs(&["s"]), &hs(&["s"]), &hs(&[]), 1); // bound
    reconcile_step(&mut st, &hs(&["s"]), &hs(&[]), &hs(&[]), 1); // retire s（miss>=1）
                                                                 // s 经 daemon 正常结束 → 离开 announced_live → 追踪被剔除。
    reconcile_step(&mut st, &hs(&[]), &hs(&[]), &hs(&[]), 1);
    // s 又出现且后端在跑（复用同名）→ 重新 ever_bound、不带旧 retired 状态。
    assert!(reconcile_step(&mut st, &hs(&["s"]), &hs(&["s"]), &hs(&[]), 1).is_empty());
}

#[test]
fn branch_drift_does_not_retire_old_sid() {
    // /branch：daemon 先退旧 A 宣告新 B。对账里 A 离开 announced_live → 不 retire；B 新绑。
    let mut st = ReconcileState::default();
    reconcile_step(&mut st, &hs(&["A"]), &hs(&["A"]), &hs(&[]), 2); // A bound
                                                                    // 漂移后一轮：announced_live 只剩 B（daemon 已退 A），后端自报 B。
    let retire = reconcile_step(&mut st, &hs(&["B"]), &hs(&["B"]), &hs(&[]), 2);
    assert!(
        retire.is_empty(),
        "A 不该被 retire（它是正常漂移离开、非带外杀）"
    );
}

#[test]
fn branch_drift_lag_round_does_not_retire_with_threshold_2() {
    // 审计发现的滞后竞态：daemon 退旧 sid A 晚一拍——某轮 A 仍 announced、backend 已是 B（A 缺失）。
    // threshold=2 下这单轮 miss=1 < 2 → 不 retire；下一轮 daemon 退 A、A 离开 announced → 剔除。
    // （这正是 threshold ≥ 2 编译期兜死要保的性质。）
    let mut st = ReconcileState::default();
    reconcile_step(&mut st, &hs(&["A"]), &hs(&["A"]), &hs(&[]), 2); // A bound
                                                                    // 滞后 1 轮：A 仍 announced、backend=B → A miss=1 < 2 → 不 retire。
    assert!(reconcile_step(&mut st, &hs(&["A", "B"]), &hs(&["B"]), &hs(&[]), 2).is_empty());
    // 下一轮 daemon 退 A → A 离开 announced_live → 剔除追踪、不 retire。
    assert!(reconcile_step(&mut st, &hs(&["B"]), &hs(&["B"]), &hs(&[]), 2).is_empty());
}

#[test]
fn empty_backend_increments_ever_bound_sid() {
    // 注：纯函数对空 backend 会累计缺失（source-agnostic、F90 期或有更好的空 vs 错区分）；
    // **poller 侧对空 backend 保守跳过**（见 run_tmux_reconcile_poller）——本测只锁纯函数语义。
    let mut st = ReconcileState::default();
    let announced = hs(&["s"]);
    reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 5); // ever_bound
                                                                   // 后端空集（tmux 全没了但会话还宣告活）→ 累计缺失。
    for i in 1..5 {
        assert!(
            reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 5).is_empty(),
            "miss={i} 未到阈值 5"
        );
    }
    assert_eq!(
        reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 5),
        vec!["s".to_string()]
    );
}

#[test]
fn threshold_one_retires_immediately_on_first_miss() {
    let mut st = ReconcileState::default();
    let announced = hs(&["s"]);
    reconcile_step(&mut st, &announced, &hs(&["s"]), &hs(&[]), 1); // bound
    assert_eq!(
        reconcile_step(&mut st, &announced, &hs(&[]), &hs(&[]), 1),
        vec!["s".to_string()]
    );
}

#[test]
fn multiple_sids_mixed() {
    // a: bound→missing 到阈值 retire；b: 一直在跑不动；c: never-bound 不判。
    let mut st = ReconcileState::default();
    let announced = hs(&["a", "b", "c"]);
    reconcile_step(&mut st, &announced, &hs(&["a", "b"]), &hs(&[]), 2); // a,b bound; c never
    let retire = reconcile_step(&mut st, &announced, &hs(&["b"]), &hs(&[]), 2); // a miss=1
    assert!(retire.is_empty());
    let mut retire = reconcile_step(&mut st, &announced, &hs(&["b"]), &hs(&[]), 2); // a miss=2 → retire
    retire.sort();
    assert_eq!(retire, vec!["a".to_string()]);
}

#[test]
fn pre_bound_idle_sid_retires_even_if_never_in_backend() {
    // audit-fixes F03.2（D 审计②修 + 覆盖缺口）：复现「卡灰」竞态——idle sid 因跨线程缝从未在
    // 「tracked ∩ backend」的帧里被置 ever_bound（这里 backend 全程不含 s，模拟那帧已过）。
    // pre_bound（idle 集）播种 ever_bound 后，s 仍应累计缺失并在阈值 retire，不会永久卡灰。
    // **变异锚点**：删掉函数体里 `if pre_bound.contains(sid) { t.ever_bound = true; }` 则本测红
    //（s 恒走 never-bound 分支、miss 永不累计、retire 永空）。
    let mut st = ReconcileState::default();
    let announced = hs(&["s"]);
    let idle = hs(&["s"]);
    // backend 从不含 s；s 是 idle（pre_bound）→ 第 1 轮 miss=1 < 2。
    assert!(reconcile_step(&mut st, &announced, &hs(&[]), &idle, 2).is_empty());
    // 第 2 轮 miss=2 >= 2 → retire（tmux 真没了 → 归档，灰关得掉）。
    assert_eq!(
        reconcile_step(&mut st, &announced, &hs(&[]), &idle, 2),
        vec!["s".to_string()]
    );
}

#[test]
fn pre_bound_idle_still_in_backend_does_not_retire() {
    // idle sid 但 tmux 仍在（backend 含它）→ pre_bound 播种 ever_bound、backend 分支清 miss →
    // 不 retire（灰灯继续，正确：claude 死但 tmux 活=可复用）。
    let mut st = ReconcileState::default();
    let announced = hs(&["s"]);
    let idle = hs(&["s"]);
    for _ in 0..5 {
        assert!(reconcile_step(&mut st, &announced, &hs(&["s"]), &idle, 2).is_empty());
    }
}
