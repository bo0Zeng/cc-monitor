use super::*;

// 〔C4d · 第四波 4B〕本机 manifest 参照实现（`list_from_dir`〔散文墓碑〕一族）删了，驱动它的八条判据随之退役；
//   它们钉的性质各自改指现存实现（后端 `observe/accounts_query.rs` ＋ `common/fs.rs`），住
//   `tests/backend/observe/accounts_query_tests.rs` 的 C4d 那一节：
//   - 读上限三种失败分得开 · 正好等于上限放行 ⇒ `read_regular_capped_keeps_its_three_failures_distinguishable`；
//   - 欺骗字符逐组拒 ⇒ `every_group_of_deceptive_characters_is_rejected_in_a_config_dir`（码位表逐字搬过去）；
//   - 账号库目录名是契约 ⇒ `the_accounts_library_lives_under_the_contract_directory_name`；
//   - 缺 manifest / 坏 JSON / 坏 schema · 账号 0 是缺席的键 · 登录态只是 stat · 不安全的 configDir 丢单条
//     ⇒ 后端早有同形判据（`list_accounts_degrades_gracefully` · `account_zero_is_kept_and_probes_shared_store` ·
//     `empty_config_dir_is_not_account_zero` · `unsafe_config_dirs_are_dropped` · `one_bad_account_does_not_kill_the_list`）；
//   - `authKind` 跨生产者对拍 ⇒ 生产者只剩后端一个，`auth_kind_parity_backend_side` 照旧对 `acct-core` 的金样；
//   - 「这一维只有一条计算路径」⇒ 后端同名判据 `the_auth_dimension_has_exactly_one_computation_path`。

// 〔C4c · 第四波 4B〕`N-F1c` 读口那几组（已知假清单 · 三档结局 · 读口真去问了后端）与 `A3` 本机信任预检那几组
//   〔散文墓碑〕随被测函数一起退役：本机账号清单与信任预检改走通道（`<local>` 那条长连接的 `accounts-list` /
//   `accounts-trust`），后端出成品。「够不着 ≠ 你没有账号」那一格由通道的失败层级接住（`ipc/chan-caller.ts::saidOf`），
//   判据在 `tests/accounts-decode.vitest.ts`。

// ─────────────────────────────────────────────────────────────────────────────
// 〔`A3` 第二波〕`acct-iso.check` / `acct-iso.shellinit` 的本机对侧
// ─────────────────────────────────────────────────────────────────────────────

/// 「没装」是 `Ok(installed:false)`（后端 exit 0 那一支），不是错误；
/// 「问不出来」三档（后端不在 / 查询失败 / 读不懂）是 `Err`，且**不许**说成「没装」。
#[test]
fn the_local_acct_iso_status_tells_not_installed_from_cannot_ask() {
    let yes = classify_local_acct_iso(QueryOutcome::Ok(
        "{\"installed\":true,\"path\":\"/h/.local/bin/cc-acct-iso\",\"looked\":null}\n".into(),
    ))
    .unwrap();
    assert!(yes.installed);
    assert_eq!(yes.path.as_deref(), Some("/h/.local/bin/cc-acct-iso"));
    // vendor 指纹与远端那条同一个来源（前端比「有没有更新」用的是同一个值）。
    assert_eq!(yes.vendor_id, crate::acct_iso_deploy::vendor_id());

    let no = classify_local_acct_iso(QueryOutcome::Ok(
        "{\"installed\":false,\"path\":null,\"looked\":\"找不到\"}".into(),
    ))
    .unwrap();
    assert!(!no.installed && no.path.is_none());

    let errs: Vec<String> = [
        QueryOutcome::NoBackend("找过 /x".into()),
        QueryOutcome::Failed {
            code: Some(2),
            stderr: "{\"code\":\"bad_args\",\"message\":\"unknown argument\"}".into(),
        },
        QueryOutcome::Ok("not json".into()),
        QueryOutcome::Ok("{\"path\":null}".into()),
    ]
    .into_iter()
    .map(|o| classify_local_acct_iso(o).expect_err("问不出来的那几档必须是 Err"))
    .collect();
    assert!(errs[0].contains("本机后端不在") && errs[0].contains("找过 /x"));
    assert!(
        errs[1].contains("unknown argument") && !errs[1].contains("\"code\""),
        "后端的 {{code,message}} 要取出 message 说人话：{}",
        errs[1]
    );
    for e in &errs {
        assert!(!e.contains("远端"), "本机那条路的话里出现了「远端」：{e}");
    }
}

/// 片段：围栏齐 ⇒ 原样；缺 END ⇒ 截断那句；缺 BEGIN ⇒ 没产出那句；后端失败 ⇒ 带后端原话。
/// 四种失败两两不同，都不说「远端」、都不指「先在『维护』里部署」（本机没有那个口）。
#[test]
fn the_local_shellinit_uses_the_same_fence_judgment_with_local_words() {
    use crate::acct_iso_deploy::{SHELLINIT_FENCE_BEGIN as B, SHELLINIT_FENCE_END as E};
    let whole = format!("{B}\nzcc() {{ :; }}\n{E}\n");
    assert_eq!(
        classify_local_shellinit(QueryOutcome::Ok(whole.clone())),
        Ok(whole)
    );
    let errs: Vec<String> = [
        QueryOutcome::Ok(format!("{B}\nzcc() {{")),
        QueryOutcome::Ok("warn only".into()),
        QueryOutcome::NoBackend("找过 /x".into()),
        QueryOutcome::Failed {
            code: Some(2),
            stderr: "{\"code\":\"not_installed\",\"message\":\"找不到 `cc-acct-iso`\"}".into(),
        },
    ]
    .into_iter()
    .map(|o| classify_local_shellinit(o).expect_err("失败档必须是 Err"))
    .collect();
    assert!(errs[0].contains("不完整"));
    assert!(errs[1].contains("没能产出"));
    assert!(errs[3].contains("找不到 `cc-acct-iso`"));
    assert_eq!(
        errs.iter().collect::<std::collections::BTreeSet<_>>().len(),
        4,
        "四种失败说成了同一句：{errs:?}"
    );
    for e in &errs {
        assert!(
            !e.contains("远端") && !e.contains("维护"),
            "本机那条路指了走不通的路：{e}"
        );
    }
    // 同一个判定：远端那条对同样的截断输出也判不完整（话不同、判法同源）。
    assert!(
        crate::acct_iso_deploy::validate_shellinit_output(format!("{B}\nx"))
            .unwrap_err()
            .contains("不完整")
    );
}

/// ★ 走**生产入口本体**：单测环境没有本机后端 ⇒ 两条都是带理由的 `Err`（真去问了），
/// 不是「空但成功」（被短路的形状：`Ok(installed:false)` / `Ok("")`）。
#[tokio::test]
async fn the_two_local_acct_iso_commands_really_ask_the_backend() {
    let probe = run_query(
        env!("CCM_TARGET_TRIPLE"),
        &["--acct-iso-status"],
        &*crate::spawn_managed::local_backend_one_shot_query(),
    );
    assert!(
        matches!(probe, QueryOutcome::NoBackend(_)),
        "测试环境里居然找得到 local_backend —— 本条的前提不成立"
    );
    let st = check_local_acct_iso()
        .await
        .expect_err("没有本机后端却答出了装没装");
    assert!(st.contains("本机后端不在"), "{st}");
    let sn = local_acct_iso_shellinit()
        .await
        .expect_err("没有本机后端却拿到了片段");
    assert!(sn.contains("本机后端不在"), "{sn}");
}

// 〔C4c · 第四波 4B〕S3 那一节（本机 apikey 表并进清单）〔散文墓碑〕随 `with_apikey_table` 一起退役：并表挪进了后端
//   （`observe/accounts_query.rs::list_product`，规则 `acct_core::apikey_routed_subset`），判据在后端
//   `tests/backend/observe/accounts_query_tests.rs` 的 C4c 那一节（表里有行 ⇒ api-key 且可选 · 别家 / 空表 / 对不上 ⇒ 一格不动）。
