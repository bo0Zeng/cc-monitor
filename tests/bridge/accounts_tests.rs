use super::*;

// 〔C4c · 第四波 4B〕本文件原有的解析 / 降级说明 / trust 拼参那几组判据〔散文墓碑〕随被测函数一起退役：
//   账号清单与信任预检改成前端经通道问、那台机器的后端出成品（`accounts-list` / `accounts-trust`）。
//   它们守的性质各自搬到了新家：
//   - 行形状 · 账号 0（configDir 缺席 = null）· 缺账号 0 那一句 ⇒ 后端 `tests/backend/observe/accounts_query_tests.rs`
//     的 C4c 那一节（成品 == CLI 臂逐行 · 缺账号 0 出那一句）＋ 跨语言金样 `tests/__fixtures__/accounts.golden.json`；
//   - 「坏形状不许悄悄变成一个能选的号 / 老后端不许编一个值」⇒ TS 解码器严格收（`tests/accounts-decode.vitest.ts`）；
//   - 账号 0 的信任预检不传路径 ⇒ 帧面 `configDir: null`（后端 `read_face_tests` 入参闸 ＋ CLI 那一臂原有判据）。

// ---- K-A1：鉴权方式这一维（这一侧只剩**形状**：TS 生成物的来源） ----

/// ★ 枚举的 **serde 名**必须逐字等于 `acct-core` 的那两个契约常量。
///
/// 这是本枚举与共享 crate 之间**唯一**的双写点（`kebab-case` 是 derive 算出来的，
/// 不是我写的字面量）⇒ 改了任一侧，本条红。生成物 `src/generated/AuthKind.ts`
/// 也是从这两个名字来的，所以 TS 那个字面量联合一并被钉住。
#[test]
fn the_wire_names_match_the_shared_contract() {
    assert_eq!(
        serde_json::to_string(&AuthKind::Subscription).unwrap(),
        format!("\"{}\"", acct_core::AUTH_KIND_SUBSCRIPTION)
    );
    assert_eq!(
        serde_json::to_string(&AuthKind::ApiKey).unwrap(),
        format!("\"{}\"", acct_core::AUTH_KIND_API_KEY)
    );
    // 反向：契约里的每个字面量都要能反序列化回来（闭集两头都得通）。
    for k in acct_core::AUTH_KINDS {
        let v: AuthKind = serde_json::from_str(&format!("\"{k}\"")).unwrap();
        assert_eq!(v.as_contract_str(), k);
    }
    // 默认档是订阅（`KA6d`：缺席 ⇒ 订阅，不是 api-key）。
    assert_eq!(AuthKind::default(), AuthKind::Subscription);
    assert_eq!(AuthKind::from_manifest(None), AuthKind::Subscription);
    assert_eq!(
        AuthKind::from_manifest(Some(acct_core::AUTH_KIND_API_KEY)),
        AuthKind::ApiKey
    );
}
