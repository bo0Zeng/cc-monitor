use super::*;

/// 🔴 **语料分布自检 —— 这条会红，而且它红过就要停下来看。**
///
/// ／`§4.2` 记过它两次真红（一次逮住生成器偏 16%，
/// 一次逮住闸自己比量具的分辨率还细）。**两次都红对了。**
///
/// **九项全判，一项都不豁免**（`99` 那条 `段长 p90` 的豁免本实现用不上，
/// 理由见模块头注）。九个数每趟都印出来。
#[test]
fn the_synthetic_corpus_matches_the_measured_distribution() {
    let paths = synth_paths(200_000, 0xC0FFEE);
    let r = measure(&paths);

    let checks: [(&str, f64, f64); 9] = [
        ("路径长 中位", r.path_len.median, target::PATH_LEN.0),
        ("路径长 均值", r.path_len.mean, target::PATH_LEN.1),
        ("路径长 p90", r.path_len.p90, target::PATH_LEN.2),
        ("深度 中位", r.depth.median, target::DEPTH.0),
        ("深度 均值", r.depth.mean, target::DEPTH.1),
        ("深度 p90", r.depth.p90, target::DEPTH.2),
        ("段长 中位", r.seg_len.median, target::SEG_LEN.0),
        ("段长 均值", r.seg_len.mean, target::SEG_LEN.1),
        ("段长 p90", r.seg_len.p90, target::SEG_LEN.2),
    ];

    let mut bad: Vec<String> = Vec::new();
    for (name, got, want) in checks {
        let rel = ((got - want) / want).abs() * 100.0;
        let ok = within_tolerance(got, want);
        println!(
            "  {name:<12} 合成 {got:>7.1}  目标 {want:>7.1}  偏差 {rel:>5.1}%  {}",
            if ok { "ok" } else { "🔴 RED" }
        );
        if !ok {
            bad.push(format!(
                "{name}: 合成 {got:.1} / 目标 {want:.1}（偏 {rel:.1}%）"
            ));
        }
    }
    assert_eq!(
        checks.len(),
        9,
        "九项一项都不许少 —— 少一项就是挖瞎一个维度"
    );
    assert!(
        bad.is_empty(),
        "语料分布对不上现打目标 —— 本次读数作废：\n  {}",
        bad.join("\n  ")
    );
}

/// 反空真：自检那把尺子**认得出坏语料**。
/// 把生成器换成「所有路径一样长」，上面九项里必须有东西红。
#[test]
fn the_distribution_self_check_rejects_a_degenerate_corpus() {
    let degenerate: Vec<String> = (0..10_000).map(|i| format!("/a/b/{i:03}")).collect();
    let r = measure(&degenerate);
    let any_red = !within_tolerance(r.path_len.median, target::PATH_LEN.0)
        || !within_tolerance(r.depth.median, target::DEPTH.0)
        || !within_tolerance(r.seg_len.mean, target::SEG_LEN.1);
    assert!(any_red, "一份明显不像真分布的语料竟然全过了 —— 自检是空的");
}

#[test]
fn the_generator_is_deterministic_for_a_given_seed() {
    let a = synth_paths(500, 42);
    let b = synth_paths(500, 42);
    let c = synth_paths(500, 43);
    assert_eq!(a, b, "同一个 seed 必须复算出同一份语料");
    assert_ne!(a, c, "不同 seed 不该产出同一份语料");
}

#[test]
fn tolerance_takes_the_wider_of_the_two_rules() {
    // 差一个量化单位，相对偏差 16.7% > 15% —— 按绝对差那一支放行（§4.2）。
    assert!(within_tolerance(5.0, 6.0));
    // 差 5 个单位、18.5% —— 两支都不放行（§4.3 那一项靠点名豁免，不靠这条）。
    assert!(!within_tolerance(22.0, 27.0));
    // 大数上的 10% 照旧放行。
    assert!(within_tolerance(110.0, 121.0));
    // 大数上的 30% 不放行。
    assert!(!within_tolerance(85.0, 121.0));
}
