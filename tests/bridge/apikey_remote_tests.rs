//! 〔RM1a · 第四波〕`apikey_remote.rs` 的判据 —— 今天那一份只剩发送口 `call`（起会话那一侧问 `launch-endpoint` 走它）。
//!
//! 〔HX2 · 第四波 4D〕墓碑：写 key 那一半（`write_key_on`〔散文墓碑〕· `send_key`〔散文墓碑〕）随界面改走通道 `apikey-key-set` 一起删了，
//! 它的几条判据跟着走 ——
//! `gp1_the_read_arms_stay_local_and_the_write_arm_goes_to_that_machines_backend`〔散文墓碑〕（写臂两台同一条路）·
//! `gp1_the_local_write_checks_the_file_then_hands_the_key_to_the_backend`〔散文墓碑〕与 `gp1_a_backend_writing_another_file_gets_no_key`〔散文墓碑〕
//! （本机那一臂先核路径：那一问今天由连接本身答 —— `local_backend_host_tests::hx2_a_backend_started_for_another_data_dir_is_refused_out_loud`）·
//! `the_remote_arm_says_what_went_wrong_without_the_plaintext`〔散文墓碑〕（今天界面那一侧说话：`tests/apikey-reads.vitest.ts`）·
//! `the_command_names_are_the_ones_the_backend_registers`〔散文墓碑〕（命令名常量随写臂一起删；monitor 里零处叫得出那条帧命令，
//! 由 `creds_store_tests::hx2_the_monitor_names_no_plaintext_key_on_the_way_to_the_backend` 钉零命中）。
//! 更早退掉的两条照留：RM1a `the_local_arm_never_sends_the_key_to_a_backend`〔散文墓碑〕· US1 `the_read_answer_is_parsed_from_the_fields_the_backend_declares`〔散文墓碑〕。

use super::*;

/// 没有通道的那台 ⇒ 发送口说得出是哪台（不 panic、不装作成了）。
#[test]
fn a_host_without_a_channel_is_named_in_the_refusal() {
    let err = tauri::async_runtime::block_on(call(
        "hx2-no-such-host-for-tests",
        "launch-endpoint",
        serde_json::json!({}),
    ))
    .expect_err("没有通道还说问成了");
    assert!(
        err.contains("hx2-no-such-host-for-tests"),
        "报错里说不出是哪台机器：{err}"
    );
}
