use super::*;

#[path = "support/scripted_backend.rs"]
mod scripted;

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

/// 「没装」是 `Ok(installed:false)`（后端答了「没有」），不是错误；
/// 「问不出来」（够不着 / 对端说不行 / 应答读不懂）是 `Err`，且**不许**说成「没装」。
#[test]
fn the_local_acct_iso_status_tells_not_installed_from_cannot_ask() {
    let yes = classify_local_acct_iso(Ok(serde_json::json!(
        {"installed": true, "path": "/h/.local/bin/cc-acct-iso", "looked": null}
    )))
    .unwrap();
    assert!(yes.installed);
    assert_eq!(yes.path.as_deref(), Some("/h/.local/bin/cc-acct-iso"));
    // vendor 指纹与远端那条同一个来源（前端比「有没有更新」用的是同一个值）。
    assert_eq!(yes.vendor_id, crate::acct_iso_deploy::vendor_id());

    let no = classify_local_acct_iso(Ok(serde_json::json!(
        {"installed": false, "path": null, "looked": "找不到"}
    )))
    .unwrap();
    assert!(!no.installed && no.path.is_none());

    let errs: Vec<String> = [
        Err("[<local>] 没有可用的控制通道（backend 未在场或长连接未握手）".to_string()),
        Ok(serde_json::json!({"path": null})),
        Ok(serde_json::json!("not an object")),
    ]
    .into_iter()
    .map(|o| classify_local_acct_iso(o).expect_err("问不出来的那几档必须是 Err"))
    .collect();
    assert!(errs[0].contains("没有可用的控制通道"), "{}", errs[0]);
    assert!(errs[1].contains("两端契约对不上") && errs[2].contains("两端契约对不上"));
    for e in &errs {
        // 问不出来 ⇒ 句子以「查不出」或契约那句开头，绝不是一句「没装」的结论。
        assert!(
            e.starts_with("查不出") || e.contains("两端契约对不上"),
            "问不出来被说成了别的：{e}"
        );
    }
}

/// 片段：围栏齐 ⇒ 原样；缺 END ⇒ 截断那句；缺 BEGIN ⇒ 没产出那句；对端失败 ⇒ 带后端原话；应答缺 `snippet` ⇒ 契约那句。
/// 五种失败两两不同，都不说「远端」、都不指「先在『维护』里部署」（本机没有那个口）。
#[test]
fn the_local_shellinit_uses_the_same_fence_judgment_with_local_words() {
    use crate::acct_iso_deploy::{SHELLINIT_FENCE_BEGIN as B, SHELLINIT_FENCE_END as E};
    let whole = format!("{B}\nzcc() {{ :; }}\n{E}\n");
    assert_eq!(
        classify_local_shellinit(Ok(serde_json::json!({ "snippet": whole.clone() }))),
        Ok(whole)
    );
    let errs: Vec<String> = [
        Ok(serde_json::json!({ "snippet": format!("{B}\nzcc() {{") })),
        Ok(serde_json::json!({ "snippet": "warn only" })),
        Err("[<local>] 没有可用的控制通道（backend 未在场或长连接未握手）".to_string()),
        Err("本机 查询失败（not_installed）：找不到 `cc-acct-iso`".to_string()),
        Ok(serde_json::json!({})),
    ]
    .into_iter()
    .map(|o| classify_local_shellinit(o).expect_err("失败档必须是 Err"))
    .collect();
    assert!(errs[0].contains("不完整"));
    assert!(errs[1].contains("没能产出"));
    assert!(errs[3].contains("找不到 `cc-acct-iso`"));
    assert!(errs[4].contains("两端契约对不上"));
    assert_eq!(
        errs.iter().collect::<std::collections::BTreeSet<_>>().len(),
        5,
        "五种失败说成了同一句：{errs:?}"
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

/// ★ 走**生产入口本体**，〔LOC1a〕两条都真走 `<local>` 长连接、问的是那两条帧命令（异源：数假后端收到的）；
/// 通道不在 ⇒ 带理由的 `Err`（不是「空但成功」那种被短路的形状：`Ok(installed:false)` / `Ok("")`）。
#[test]
fn the_two_local_acct_iso_commands_really_ask_the_backend() {
    use crate::acct_iso_deploy::{SHELLINIT_FENCE_BEGIN as B, SHELLINIT_FENCE_END as E};
    let _guard = crate::backend::control::inbound_client::local_origin_test_lock();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let _enter = rt.enter();
    // 通道不在
    let st = rt
        .block_on(check_local_acct_iso())
        .expect_err("没有本机后端通道却答出了装没装");
    assert!(st.contains("没有可用的控制通道"), "{st}");
    let sn = rt
        .block_on(local_acct_iso_shellinit())
        .expect_err("没有本机后端通道却拿到了片段");
    assert!(sn.contains("没有可用的控制通道"), "{sn}");
    // 通道在：真问了那两条
    let whole = format!("{B}\nzcc() {{ :; }}\n{E}\n");
    let rig = scripted::rig(
        crate::backend::control::inbound_client::LOCAL_ORIGIN,
        &["acct-iso-status", "acct-iso-shellinit"],
        vec![
            (
                "acct-iso-status",
                Ok(serde_json::json!({"installed": true, "path": "/h/x", "looked": null})),
            ),
            (
                "acct-iso-shellinit",
                Ok(serde_json::json!({ "snippet": whole.clone() })),
            ),
        ],
    );
    assert!(rt.block_on(check_local_acct_iso()).expect("装了").installed);
    assert_eq!(
        rt.block_on(local_acct_iso_shellinit()).expect("片段"),
        whole
    );
    assert_eq!(
        rig.cmds(),
        vec![
            "acct-iso-status".to_string(),
            "acct-iso-shellinit".to_string()
        ]
    );
}

// 〔C4c · 第四波 4B〕S3 那一节（本机 apikey 表并进清单）〔散文墓碑〕随 `with_apikey_table` 一起退役：并表挪进了后端
//   （`observe/accounts_query.rs::list_product`，规则 `acct_core::apikey_routed_subset`），判据在后端
//   `tests/backend/observe/accounts_query_tests.rs` 的 C4c 那一节（表里有行 ⇒ api-key 且可选 · 别家 / 空表 / 对不上 ⇒ 一格不动）。
