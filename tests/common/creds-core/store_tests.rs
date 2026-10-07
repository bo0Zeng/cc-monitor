//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md` 的 `apikey-key-set` / `apikey-read` 两节（改一个键不许吃掉人手编的内容）
//!
//! 核原文：`apikey-key-set` 节逐字「别的行与未知键一个不动」·「`bad_file`（现有文件解析不了 ⇒ **不覆盖**，人手编的内容不许被抹掉）」；
//! `apikey-read` 节逐字「解析不了不退化成「没配」」。写者今天只剩一种 —— 每台机器那台后端的
//! `accounts/upstream_select/file_face.rs`（本机也是；monitor 那侧的写口删了）—— 它用本模块的纯逻辑兑现这几句 —— 本族判的就是那份共用逻辑。手编 JSON 原样即用、模板自带字段名那几条守
//! 逐字「只放一个凭据文件、一次界面都不开，中转就能用那把 key」。
//! ⚠ 落盘键序（按名 · 递归 · 数组保序）与 `auth_style` 往返那几条今天没有逐字原文。
//! ⚠ `INVARIANTS §42` 的机检只核字段名落节、不核行为；契约里行为句不漂靠的是本族。

use super::*;

/// `KS9①`：格式是**明文 JSON**，不是二进制、不是密文块。
///
/// 量法：把一份带 key 的文档序列化出来，断言**用一个通用 JSON 解析器读得回来**，
/// 且 key 的明文**逐字**出现在文本里（这一条正是「明文」的定义，不是缺陷）。
#[test]
fn what_lands_on_disk_is_plain_json_a_human_can_read() {
    let doc = merge_key(&Map::new(), &SecretKey::new("sk-ant-PLAINTEXT-ON-PURPOSE"));
    let text = to_pretty_json(&doc);
    let back: Value = serde_json::from_str(&text).expect("落盘的东西必须是合法 JSON");
    assert_eq!(back[KEY_FIELD], "sk-ant-PLAINTEXT-ON-PURPOSE");
    // 明文就是明文 —— 这一档**刻意**不加密（理由见 crate 头注：单机加密是安全剧场）。
    assert!(text.contains("sk-ant-PLAINTEXT-ON-PURPOSE"));
    // 非空对照：它确实是一份**对象**，不是被塞进一个字符串里的转义 JSON。
    assert!(back.is_object());
}

/// `KS9③`：**导入** —— 把一份现成 JSON 放进去就算配好，不需要任何迁移步骤。
#[test]
fn a_hand_authored_json_is_accepted_as_is_with_no_migration_step() {
    // 一份**人手写**的文件：有未知键、有注释性字段、键的顺序是人的顺序。
    let hand = r#"{
            "_note": "这是我自己写的",
            "api_key": "sk-ant-HAND-WRITTEN",
            "endpoint_hint": "https://example.invalid"
        }"#;
    let doc = parse(hand).expect("人手写的 JSON 应当直接被接受");
    let k = read_key(&doc).expect("应当读得到 key");
    assert_eq!(k.expose_for_auth_header(), "sk-ant-HAND-WRITTEN");
    // 没有任何「版本号 / schema 迁移」这一步：读出来就能用。
    assert!(doc.get("_schema_version").is_none());
}

/// `KS9`：文件不存在时要给一份模板 —— 否则「导入」这条要靠猜。
#[test]
fn the_template_is_itself_a_valid_store_and_names_the_field() {
    let doc = parse(&template()).expect("模板自己必须是合法的一份 store");
    assert!(doc.contains_key(KEY_FIELD), "模板里没点名那个字段");
    // 模板里的 key 是空的 ⇒ 读出来是「还没配」，不是一把假 key。
    assert!(read_key(&doc).is_none(), "模板不该看起来像已经配好了");
    // 模板自己就带一个未知键（那句说明），它是「未知键会被保留」的活用例。
    assert!(doc.contains_key("_note"));
}

/// `KS10①`：保留未知键。**人加的字段不许被抹掉。**
#[test]
fn merging_a_key_never_swallows_anything_the_human_wrote() {
    let hand = parse(r#"{"_note":"别删我","api_key":"OLD","zzz_last":1,"aaa_first":[1,2]}"#)
        .expect("夹具应当可解析");
    let merged = merge_key(&hand, &SecretKey::new("NEW"));
    assert_eq!(merged[KEY_FIELD], "NEW", "key 应当被换成新的");
    // 分母 = 夹具里除 key 外的这 3 个键，逐个查。
    assert_eq!(merged["_note"], "别删我");
    assert_eq!(merged["zzz_last"], 1);
    assert_eq!(merged["aaa_first"], serde_json::json!([1, 2]));
    assert_eq!(merged.len(), 4, "键数变了 —— 有东西被吃掉或凭空多出来了");
}

/// ★★ `KS10②` 的**正主**：顺序按**名字**定，不按**到达先后**定。
///
/// 它直接喂 `ordered_keys` 一个乱序序列 ⇒ 与 `serde_json::Map` 今天是
/// `BTreeMap` 还是 `IndexMap` **无关**。
/// 〔立项理由：`MU9` 实测把排序删掉，隔壁那条端到端判据 19 全绿 —— 详见 `ordered_keys` 头注。〕
#[test]
fn ordering_is_by_name_not_by_arrival() {
    let arrival: Vec<String> = ["zzz", "mmm", "aaa", "bbb"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let got: Vec<&str> = ordered_keys(arrival.iter())
        .into_iter()
        .map(|s| s.as_str())
        .collect();
    // 期望值是**手写字面量**，不是拿被测函数算出来的（否则本断言自证、恒绿）。
    assert_eq!(got, vec!["aaa", "bbb", "mmm", "zzz"]);
    // 非空对照：到达顺序**确实**不是这个顺序（夹具真的是乱的）。
    let as_arrived: Vec<&str> = arrival.iter().map(|s| s.as_str()).collect();
    assert_ne!(
        as_arrived, got,
        "夹具的到达顺序本来就等于排序结果 —— 这条判据在空转"
    );
}

/// `KS10②` 的**端到端那一半**（整份进、整份出）：落盘顺序由渲染时的排法定，不由 `Map` 的迭代顺序定。
/// 注入倒序排法 ⇒ 输出跟着倒，证明顺序出自排法这一处（与 `Map` 是 `BTreeMap` 还是 `IndexMap` 无关）。
#[test]
fn the_field_order_does_not_depend_on_the_map_implementation() {
    // 两份**内容相同、插入顺序相反**的文档。
    let a = parse(r#"{"aaa":1,"mmm":2,"zzz":3}"#).expect("a");
    let b = parse(r#"{"zzz":3,"mmm":2,"aaa":1}"#).expect("b");
    assert_eq!(
        to_pretty_json(&a),
        to_pretty_json(&b),
        "同样的内容按不同顺序插入，落盘文本就不一样 —— \
             那样每次界面一动整份文件的 diff 全变，人就不敢手编了"
    );
    // 非空对照：输出确实有多行、确实含那三个键（不是空串恒等）。
    let t = to_pretty_json(&a);
    assert!(t.lines().count() >= 5, "输出只有 {} 行", t.lines().count());
    for k in ["aaa", "mmm", "zzz"] {
        assert!(t.contains(k), "输出里没有 {k}");
    }
    // 顺序就是排好的那一个（不是「碰巧一样」）。
    let ia = guard_core::find_pinned(&t, "aaa").expect("aaa 应当恰好出现一处");
    let im = guard_core::find_pinned(&t, "mmm").expect("mmm 应当恰好出现一处");
    let iz = guard_core::find_pinned(&t, "zzz").expect("zzz 应当恰好出现一处");
    assert!(ia < im && im < iz, "输出不是按键名排序的：{t}");
    let rev = render(&Value::Object(a.clone()), |k| {
        k.sort();
        k.reverse();
    });
    let (ra, rz) = (
        guard_core::find_pinned(&rev, "aaa").expect("aaa"),
        guard_core::find_pinned(&rev, "zzz").expect("zzz"),
    );
    assert!(
        rz < ra,
        "注入倒序排法，输出没跟着倒 —— 顺序不是渲染时排出来的：{rev}"
    );
}

/// ★★ **`KS10` 的交错那一格**（PM 08-27 点名：别测成「写完能读回来」，那测不到覆盖）。
///
/// 场景逐字：**人改了 A，程序写 B，A 还在不在。**
/// 这里模拟的是那条真实的时间线 ——
/// ① 界面打开时程序读了一份（`stale`）；② 人在编辑器里改了另一个键；
/// ③ 程序这时候才去写 key。
/// **`merge_key` 收的是「写的那一刻盘上的内容」**，所以第三步喂的必须是 `fresh` 而不是 `stale`。
#[test]
fn a_human_edit_between_read_and_write_survives_the_program_write() {
    // ① 界面打开时的样子
    let stale = parse(r#"{"_note":"旧的","api_key":"OLD"}"#).expect("stale");
    // ② 人在编辑器里把 `_note` 改了，并且自己加了一个新键
    let fresh =
        parse(r#"{"_note":"人刚改成这样","my_own":"别动我","api_key":"OLD"}"#).expect("fresh");
    // ③ 程序写 key —— 喂的是**写的那一刻**的内容
    let written = merge_key(&fresh, &SecretKey::new("NEW"));

    assert_eq!(written["_note"], "人刚改成这样", "人的编辑被陈旧副本盖掉了");
    assert_eq!(written["my_own"], "别动我", "人新加的键被吃掉了");
    assert_eq!(written[KEY_FIELD], "NEW");

    // ★ **非空对照：喂陈旧副本就是会盖掉** —— 没有这一格，上面三条可能是恒真的。
    let wrong = merge_key(&stale, &SecretKey::new("NEW"));
    assert_eq!(
        wrong["_note"], "旧的",
        "拿陈旧副本合并**应当**丢掉人的编辑；它没丢 ⇒ 上面那三条断言证不了什么"
    );
    assert!(
        wrong.get("my_own").is_none(),
        "拿陈旧副本合并**应当**丢掉人新加的键；没丢 ⇒ 本条在空转"
    );
}

/// 三态：不存在（空）· 读坏了 · 顶层不是对象。**「读坏了」不许被当成「没配」。**
#[test]
fn a_broken_file_is_reported_instead_of_being_read_as_not_configured() {
    assert_eq!(parse(""), Ok(Map::new()));
    assert_eq!(parse("   \n "), Ok(Map::new()));
    assert!(matches!(
        parse(r#"{"api_key": }"#),
        Err(StoreError::NotJson(_))
    ));
    assert_eq!(parse("[1,2,3]"), Err(StoreError::NotAnObject));
    // 说法里要带得走「怎么修」——人手编打错时看得懂。
    let msg = parse(r#"{"a":}"#)
        .unwrap_err()
        .said(std::path::Path::new("/p"));
    assert!(
        copy_core::copy_matches("credsStore.error.notJson", &msg),
        "错误说法没告诉人这文件是手编的：{msg}"
    );
}

/// key 字段的几种「等于没配」的写法。
#[test]
fn several_shapes_all_mean_not_configured() {
    // 分母 = 我列出的这 4 形，**不是**「所有写法」。
    for raw in [
        r#"{}"#,
        r#"{"api_key":""}"#,
        r#"{"api_key":"   "}"#,
        r#"{"api_key":null}"#,
    ] {
        let doc = parse(raw).expect(raw);
        assert!(read_key(&doc).is_none(), "这一形该被当成没配：{raw}");
    }
    // 非空对照：真配了的读得出来。
    let doc = parse(r#"{"api_key":" sk-x "}"#).expect("ok");
    assert_eq!(
        read_key(&doc).expect("应当读得到").expose_for_auth_header(),
        "sk-x",
        "首尾空白该被 trim（那是编辑器留下的），中间的内容一个字节都不许动"
    );
}

// ================================================================ `K-H2` `KH5`：多条

/// ★★ **`KH5a`**：`KS9③`（导入）在**多条**形状下重验 —— 一份人手写的多账号 JSON
/// 放进去就算配好，**两层的未知键都还在**，不需要任何迁移步骤。
///
/// ⚠ `KS9` 那几条原来只在**一条**的形状上验过（件计划 `§0c` 第 3 问逐字）。
#[test]
fn a_hand_authored_multi_account_file_is_read_as_a_table_with_both_layers_intact() {
    let hand = r#"{
            "_note": "顶层未知键：别删我",
            "accounts": {
                "zzz-last": { "api_key": "KEY-Z", "why": "我自己加的注释" },
                "aaa-first": { "api_key": "KEY-A", "base_url": "https://api.example.invalid" }
            }
        }"#;
    let doc = parse(hand).expect("人手写的多账号 JSON 应当直接被接受");
    let table = read_accounts(&doc);

    // 分母 = 夹具里这 2 条，逐条查。
    assert_eq!(table.len(), 2, "读出来的条数不对");
    // ★ 顺序按**键名**，不按人写的先后（`aaa-first` 写在后面）。
    assert_eq!(table[0].id, "aaa-first");
    assert_eq!(table[1].id, "zzz-last");
    assert_eq!(
        table[0]
            .key
            .as_ref()
            .expect("A 应当有 key")
            .expose_for_auth_header(),
        "KEY-A"
    );
    assert_eq!(
        table[0].base_url.as_deref(),
        Some("https://api.example.invalid")
    );
    assert_eq!(
        table[1]
            .key
            .as_ref()
            .expect("Z 应当有 key")
            .expose_for_auth_header(),
        "KEY-Z"
    );
    // 没写 `base_url` 的那条是 `None`（= 用默认上游），**不是空串**。
    assert_eq!(table[1].base_url, None);

    // 两层的未知键都还在（这是「导入不吃东西」的另一半）。
    assert_eq!(doc["_note"], "顶层未知键：别删我");
    assert_eq!(doc["accounts"]["zzz-last"]["why"], "我自己加的注释");
    // 没有任何「版本号 / schema 迁移」这一步。
    assert!(doc.get("_schema_version").is_none());
}

/// **`KH5a` 的另一半**：`K-H2a` 交付的那份**旧文件**（顶层一把 key）升级之后照常能用，
/// 它变成一条 id 逐字是 [`LEGACY_ACCOUNT_ID`] 的**有名字的行**。
///
/// ⚠⚠ 「有名字的行」与「默认行」的分界就在这一条判据上：
/// 本条只证「`default` 查得到」，**不证也不许证**「别的 id 也能拿到它」——
/// 那一半由 `K-H2` `KH2` 的 404 判据反向钉住。
#[test]
fn the_legacy_top_level_key_becomes_one_named_row_not_a_default_row() {
    let doc = parse(r#"{"_note":"旧文件","api_key":"LEGACY-KEY"}"#).expect("旧形状");
    let table = read_accounts(&doc);
    assert_eq!(table.len(), 1, "旧文件应当读成**恰好一条**");
    assert_eq!(table[0].id, LEGACY_ACCOUNT_ID);
    assert_eq!(
        table[0]
            .key
            .as_ref()
            .expect("应当有 key")
            .expose_for_auth_header(),
        "LEGACY-KEY"
    );

    // ★ `accounts` 里已经有同名的那一条 ⇒ **人写的那条赢**，顶层那半不再加。
    let both =
        parse(r#"{"api_key":"LEGACY-KEY","accounts":{"default":{"api_key":"HAND-WRITTEN"}}}"#)
            .expect("两处都有");
    let t2 = read_accounts(&both);
    assert_eq!(t2.len(), 1, "同名的两处应当合成一条，不是两条");
    assert_eq!(
        t2[0].key.as_ref().expect("key").expose_for_auth_header(),
        "HAND-WRITTEN",
        "程序拿历史形状盖掉了人明确写下的那条"
    );

    // 非空对照：顶层**没有**这两个字段时，一条都不加（不是加一条空的）。
    let empty = parse(r#"{"_note":"什么都没配"}"#).expect("空");
    assert!(read_accounts(&empty).is_empty());
}

/// ★★★ **「没配」就是零条 —— 一条「默认行」都不许有。**
///
/// 这是 `K-H2` `KH2` 在存储那一层的那一半：**零条 ⇒ `/s/` 全部 404**（上游选择查不到行）。
/// 只要这里凭空多出一条什么都没配的行，「查不到就用它」马上就写得出来了，
/// 而那正是件计划逐字禁的**回落**。
///
/// ⚠ 模板里 `accounts` 是**空对象**、`api_key` 是**空串** ——
/// 例子写在 `_note_accounts` 那句**散文**里，不是一条活的行。
/// 这是刻意的：模板里放一条活的示例行，等于每个刚装好的人都白得一条路。
#[test]
fn an_unconfigured_file_yields_no_rows_at_all() {
    // ★ **非空对照排最前**〔`D1` 一并修，08-28〕：先证明这把尺子读得出行，
    //   否则下面整个循环可能只是因为 `read_accounts` 恒返回空而全绿。
    let filled = parse(r#"{"accounts":{"my-account":{"api_key":"K"}}}"#).expect("填上");
    assert_eq!(read_accounts(&filled).len(), 1, "这把尺子是瞎的");

    // 分母 = 我列出的这 4 种「没配」的写法。
    let tpl = template();
    for raw in [
        tpl.as_str(),
        r#"{}"#,
        r#"{"_note":"什么都没写"}"#,
        r#"{"accounts":{},"api_key":""}"#,
    ] {
        let doc = parse(raw).expect("夹具应当可解析");
        assert!(
            read_accounts(&doc).is_empty(),
            "这一形凭空多出了行 —— 那就是一条默认行：{raw}"
        );
    }
}

/// ★★ **一条什么都没填的账号是合法的一条**：`base_url` 空 = 用默认上游，
/// `api_key` 空 = 原样转发客户端自己那份鉴权头。
///
/// # 它买回来的是 `K-H1` 甲半那条被多账号「顺手弄没了」的性质
///
/// 先前「不配凭据 ⇒ 中转仍是一条能用的透传路」是一条**隐式的全局行为**
/// （`render_upstream_request` 的头注逐字写着它）。改成按表路由之后，
/// 那条隐式行为**必然消失** —— 因为不再有「全局」这个东西。
/// ⇒ 它没有被删掉，是被**改成显式的一条路**：写一条空账号就有了。
/// **这两句话差得很远，所以要有一条判据钉着后一句。**
#[test]
fn an_account_with_nothing_filled_in_is_still_a_row() {
    let doc = parse(r#"{"accounts":{"passthrough":{}}}"#).expect("夹具");
    let rows = read_accounts(&doc);
    assert_eq!(rows.len(), 1, "空账号应当算一条");
    assert_eq!(rows[0].id, "passthrough");
    assert!(rows[0].key.is_none(), "它不该有 key");
    assert!(rows[0].base_url.is_none(), "它不该有自己的上游");

    // ⚠ 但值**不是对象**的那一条要跳过（有人写成 `"acct": "sk-..."`）。
    let wrong = parse(r#"{"accounts":{"acct":"sk-not-an-object"}}"#).expect("夹具");
    assert!(read_accounts(&wrong).is_empty(), "非对象的那一条不该进表");
    // ★ `K-R1`：没写 `auth_style` 的那一条读回来是 `Absent`，**不是**默认值 ——
    //   把它折成默认值是调用方的事，本模块只报「文件里写了什么」。
    assert_eq!(rows[0].auth_style, AuthStyleSetting::Absent);
}

/// ★★ `K-R1`：那个闭集**只有一个住址**，而且两头对得上。
///
/// # 它买的是什么
///
/// [`AuthStyle::from_field_value`] 刻意从 [`AuthStyle::ALL`] 派生，不是第二个 `match`。
/// 本条把这句话变成读数：`ALL` 里每一个成员，`field_value` 出去再进得回来，
/// **而且回到的是同一个成员**。写成第二个 `match` 而漏一支的话，那一支在这里当场红。
///
/// ⚠ 它**不**证明「这三个词就是现实里所有的鉴权头形状」—— 那是一个没人给得出分母的
/// 全称句。它证的是「我登记的这几个，代码两头一致」。
#[test]
fn every_registered_auth_style_survives_the_round_trip_through_its_field_value() {
    // 反空真：`ALL` 不是空的（空的话下面整个循环恒真）。
    assert!(
        AuthStyle::ALL.len() >= 2,
        "闭集只有 {} 个成员 —— 少于两个的话「按形状切」这句话没有内容",
        AuthStyle::ALL.len()
    );
    // 每个成员的词**互不相同**（重了的话 `from_field_value` 会把两个折成一个）。
    let mut words: Vec<&str> = AuthStyle::ALL.iter().map(|s| s.field_value()).collect();
    let n = words.len();
    words.sort();
    words.dedup();
    assert_eq!(words.len(), n, "有两个成员的 field_value 撞了：{words:?}");

    for want in AuthStyle::ALL {
        let w = want.field_value();
        assert_eq!(
            AuthStyle::from_field_value(w),
            Some(*want),
            "`{w}` 进不回它自己 —— 两头漂了"
        );
        // 大小写与首尾空白都要认（人手编时会带）。
        assert_eq!(
            AuthStyle::from_field_value(&format!("  {}  ", w.to_ascii_uppercase())),
            Some(*want)
        );
    }
    // ★ 非空对照：认不出的词就是认不出，**不许**回落到默认值。
    //   分母 = 我列出的这 3 形（不是「所有不合法输入」）。
    for bad in ["Bearer token", "x_api_key", "openai"] {
        assert_eq!(
            AuthStyle::from_field_value(bad),
            None,
            "`{bad}` 被认成了某个合法值 —— 静默回落正是本件在治的那一形"
        );
    }
}

/// ★★★ `K-R1`：`auth_style` 那一格读出来是**三态**，
/// 「缺席」与「写了个认不出的词」**不许挤进同一态**。
///
/// 合成一态的唯一去处是回落成默认 ⇒ 一个打错字母的 `auth_style` 静默走默认头，
/// 症状是**一条查不出来的 401**，与「key 打错了」同形。〔`K-R21` 那一族〕
#[test]
fn a_typo_in_auth_style_is_not_the_same_state_as_leaving_it_out() {
    // 分母 = 下面这 6 形，逐形手写期望值。
    let cases: &[(&str, AuthStyleSetting)] = &[
        (r#"{"accounts":{"a":{}}}"#, AuthStyleSetting::Absent),
        (
            r#"{"accounts":{"a":{"auth_style":""}}}"#,
            AuthStyleSetting::Absent,
        ),
        (
            r#"{"accounts":{"a":{"auth_style":"   "}}}"#,
            AuthStyleSetting::Absent,
        ),
        (
            r#"{"accounts":{"a":{"auth_style":"bearerr"}}}"#,
            AuthStyleSetting::Unknown,
        ),
        // 值不是字符串（有人写成数字）也归 `Unknown` —— 同样是「写了但读不懂」。
        (
            r#"{"accounts":{"a":{"auth_style":7}}}"#,
            AuthStyleSetting::Unknown,
        ),
        (
            r#"{"accounts":{"a":{"auth_style":"none"}}}"#,
            AuthStyleSetting::Known(AuthStyle::NoAuth),
        ),
    ];
    for (raw, want) in cases {
        let doc = parse(raw).expect("夹具应当可解析");
        let rows = read_accounts(&doc);
        assert_eq!(rows.len(), 1, "夹具该读出恰好一条：{raw}");
        assert_eq!(&rows[0].auth_style, want, "这一形读错了：{raw}");
    }
    // ★ 非空对照：这把尺子分得出三态里的**两两不同**（不是恒答一张脸）。
    assert_ne!(AuthStyleSetting::Absent, AuthStyleSetting::Unknown);
    assert_ne!(
        AuthStyleSetting::Known(AuthStyle::Bearer),
        AuthStyleSetting::Known(AuthStyle::NoAuth)
    );
}

/// ★★ `K-R1`：顶层那一格 `auth_style` **单独在，造不出一条行**。
///
/// 加进「要不要造这一行」的条件里就等于「只写了 `auth_style` 的文件也有一条
/// 什么都没配的 `default` 行」，而那正是 [`read_accounts`] 头注逐字禁的**回落**。
#[test]
fn a_top_level_auth_style_on_its_own_does_not_conjure_a_row() {
    let alone = parse(r#"{"auth_style":"none"}"#).expect("夹具");
    assert!(
        read_accounts(&alone).is_empty(),
        "只写了 auth_style 就凭空多出一条行 —— 那是一条默认行"
    );
    // ★ 非空对照：同一格与 `api_key` 一起写时，它**被读进那一行**（不是被忽略）。
    let with_key = parse(r#"{"api_key":"K","auth_style":"none"}"#).expect("夹具");
    let rows = read_accounts(&with_key);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, LEGACY_ACCOUNT_ID);
    assert_eq!(
        rows[0].auth_style,
        AuthStyleSetting::Known(AuthStyle::NoAuth),
        "顶层那一格没被读进这一行"
    );
}

/// ★★★ **`KH5b` 的正主**〔件计划 `§0c` 第 3 问逐字点名的那个**新**形状〕：
/// **程序只改其中一条，别的条不许被动。**`K-H2a` 没验过这一格。
///
/// # 量法：**B 那一条序列化出来的文本逐字节相等**
///
/// ⚠ 刻意**不**逐字段查 —— 逐字段查的分母是「我列出的这几个字段」，
/// 而这里要的是「**整条**没被动」，那个全称句只有逐字节比得起。
#[test]
fn writing_one_account_leaves_every_other_account_byte_for_byte_untouched() {
    let hand = parse(
        r#"{
                "_note": "顶层未知键",
                "accounts": {
                    "A": { "api_key": "A-OLD", "note_a": "A 自己的注释", "base_url": "https://a.invalid" },
                    "B": { "api_key": "B-KEY", "note_b": "别动我", "nested": { "z": 1, "a": 2 } }
                }
            }"#,
    )
    .expect("夹具应当可解析");
    let before_b = serde_json::to_string(&hand["accounts"]["B"]).expect("序列化 B");

    let merged = merge_account_key(&hand, "A", &SecretKey::new("A-NEW"));
    let merged_b = merge_account_key(&hand, "B", &SecretKey::new("B-NEW"));

    // ★★ **非空对照排最前**〔`D1` 一并修，08-28〕：同一把尺子，改 **B** 的时候 B **应当**变。
    //    没有这一格，下面 ㈡ 那条可能只是因为这把尺子看不见任何变化。
    //    ⚠ 先前它排在整条判据的**最后** —— 与 `D1-M6` 逮到的那一形同族：
    //    前面任何一条先炸，它就一次都没被求值。
    let b_after_writing_b = serde_json::to_string(&merged_b["accounts"]["B"]).expect("序列化 B");
    assert_ne!(
        b_after_writing_b, before_b,
        "改 B 的时候 B 没变 —— 这把尺子是瞎的，下面那条「没被动」证不了什么"
    );

    // ㈡ **别的条逐字节没动**（本条判据的正题，排在对照之后、别的之前）。
    let after_b = serde_json::to_string(&merged["accounts"]["B"]).expect("序列化 B");
    assert_eq!(after_b, before_b, "改 A 的时候 B 那一条被动了");

    // ㈠ 被改的那一条：key 换了，**它自己的未知键**还在。
    assert_eq!(merged["accounts"]["A"][KEY_FIELD], "A-NEW");
    assert_eq!(merged["accounts"]["A"]["note_a"], "A 自己的注释");
    assert_eq!(merged["accounts"]["A"][BASE_URL_FIELD], "https://a.invalid");
    // ㈢ 顶层的未知键也还在。
    assert_eq!(merged["_note"], "顶层未知键");
    // 改 B 的时候，B 自己的未知键与嵌套结构也都留着。
    assert_eq!(merged_b["accounts"]["B"]["note_b"], "别动我");
    assert_eq!(merged_b["accounts"]["B"]["nested"]["z"], 1);
}

/// **`KH5b` 的第二半**：往一份**还没有 `accounts` 段**的文件里写一条账号，
/// 顶层原有的东西（含 `K-H2a` 那把顶层 key）一个都不许被吃掉。
#[test]
fn adding_the_first_account_to_a_legacy_file_keeps_the_legacy_half() {
    let legacy = parse(r#"{"_note":"旧的","api_key":"LEGACY"}"#).expect("旧文件");
    let merged = merge_account_key(&legacy, "newone", &SecretKey::new("NEW"));
    assert_eq!(merged["accounts"]["newone"][KEY_FIELD], "NEW");
    assert_eq!(merged[KEY_FIELD], "LEGACY", "顶层那把 key 被吃掉了");
    assert_eq!(merged["_note"], "旧的");
    // 读回来是**两条**（`default` + `newone`）。
    let rows = read_accounts(&merged);
    let ids: Vec<&str> = rows.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, vec![LEGACY_ACCOUNT_ID, "newone"]);
}

/// **`KH5c`**：落盘文本在**深度 ≥2** 上也按键名排；注入倒序排法 ⇒ 深度 2 跟着倒，证明每一层都经过排法。
#[test]
fn nested_objects_are_also_ordered_by_name_in_what_lands_on_disk() {
    let doc = parse(r#"{"accounts":{"b":{"zzz":1,"aaa":2},"a":{"mmm":3}}}"#).expect("夹具");
    let text = to_pretty_json(&doc);

    // ★ **反空真排最前**〔`D1` 一并修〕：真的落到了嵌套那一层（不是整份被压成一行），
    //   且落盘的仍然是合法 JSON。先前这两条排在最后。
    assert!(
        text.lines().count() >= 8,
        "输出只有 {} 行",
        text.lines().count()
    );
    let back: Value = serde_json::from_str(&text).expect("落盘的东西必须是合法 JSON");
    assert_eq!(back["accounts"]["b"]["aaa"], 2);

    // 深度 1：`a` 排在 `b` 前面。
    let ia = guard_core::find_pinned(&text, "\"a\": {").expect("`a` 应当恰好出现一处");
    let ib = guard_core::find_pinned(&text, "\"b\": {").expect("`b` 应当恰好出现一处");
    assert!(ia < ib, "深度 1 没按键名排：{text}");
    // 深度 2：`b` 那条里 `aaa` 排在 `zzz` 前面。
    let iaaa = guard_core::find_pinned(&text, "\"aaa\"").expect("`aaa` 应当恰好出现一处");
    let izzz = guard_core::find_pinned(&text, "\"zzz\"").expect("`zzz` 应当恰好出现一处");
    assert!(iaaa < izzz, "深度 2 没按键名排：{text}");
    let rev = render(&Value::Object(doc.clone()), |k| {
        k.sort();
        k.reverse();
    });
    let (ra, rz) = (
        guard_core::find_pinned(&rev, "\"aaa\"").expect("aaa"),
        guard_core::find_pinned(&rev, "\"zzz\"").expect("zzz"),
    );
    assert!(
        rz < ra,
        "注入倒序排法，深度 2 没跟着倒 —— 那一层没经过排法：{rev}"
    );
}

/// **数组的顺序是数据，不许排；但数组里的对象要递归进去**（注入倒序排法 ⇒ 数组里那个对象跟着倒）。
#[test]
fn arrays_keep_their_order_because_that_order_is_data() {
    let doc = parse(r#"{"list":["zzz","aaa","mmm"]}"#).expect("夹具");
    let text = to_pretty_json(&doc);
    let back: Value = serde_json::from_str(&text).expect("合法 JSON");
    // 期望值是**手写字面量**，不是拿被测函数算出来的。
    assert_eq!(back["list"], serde_json::json!(["zzz", "aaa", "mmm"]));
    // 非空对照：同一把尺子看得见「排过」的样子长什么样（证明它不是恒等）。
    assert_ne!(back["list"], serde_json::json!(["aaa", "mmm", "zzz"]));

    // ── 另一半：**数组里套的对象要递归进去** ──────────────────
    let nested = parse(r#"{"list":[{"zzz":1,"aaa":2}]}"#).expect("嵌套夹具");
    let nested_text = to_pretty_json(&nested);
    // 非空对照：落盘的仍是合法 JSON，而且数组那一层**还在**（不是被压没了）。
    let nested_back: Value = serde_json::from_str(&nested_text).expect("合法 JSON");
    assert_eq!(nested_back["list"][0]["aaa"], 2);
    let iaaa = guard_core::find_pinned(&nested_text, "\"aaa\"").expect("`aaa` 应当恰好出现一处");
    let izzz = guard_core::find_pinned(&nested_text, "\"zzz\"").expect("`zzz` 应当恰好出现一处");
    assert!(iaaa < izzz, "数组里那个对象没被递归排序：{nested_text}");
    let rev = render(&Value::Object(nested.clone()), |k| {
        k.sort();
        k.reverse();
    });
    let (ra, rz) = (
        guard_core::find_pinned(&rev, "\"aaa\"").expect("aaa"),
        guard_core::find_pinned(&rev, "\"zzz\"").expect("zzz"),
    );
    assert!(
        rz < ra,
        "注入倒序排法，数组里那个对象没跟着倒 —— 渲染没递归进数组：{rev}"
    );
}

/// 手写的渲染与 `serde_json::to_string_pretty` 逐字节相同（嵌套 · 数组 · 空容器 · 转义 · 各种标量）。
#[test]
fn the_rendering_is_byte_for_byte_serde_pretty() {
    let doc = parse(
        r#"{"b":{"y":[1,2.5,-3,{"q":null,"p":true}],"x":{}},"a":[],"c":"引号\"与\\反斜杠\n换行\u0001","d":false}"#,
    )
    .expect("夹具");
    let want = serde_json::to_string_pretty(&Value::Object(doc.clone())).expect("序列化") + "\n";
    assert_eq!(to_pretty_json(&doc), want);
}

/// ★ 模板的三句说明住文案表、骨架住源码：拼出来的那份键序与今天逐字同
/// （三句 note 在前、`accounts` · `api_key` 在后）、每句 == 文案表那一条；读坏那一句只说「顶层要是对象」＋ 路径，不再贴整份模板。
#[test]
fn the_template_notes_come_from_the_copy_table_and_the_error_names_the_file() {
    let t = template();
    let keys: Vec<usize> = [
        "\"_note\"",
        "\"_note_accounts\"",
        "\"_note_auth_style\"",
        "\"accounts\": {}",
        "\"api_key\": \"\"",
    ]
    .iter()
    .map(|k| t.find(k).unwrap_or_else(|| panic!("模板里没有 {k}：{t}")))
    .collect();
    assert!(keys.windows(2).all(|w| w[0] < w[1]), "键序变了：{t}");
    let doc = parse(&t).expect("模板读得动");
    assert_eq!(
        doc["_note"],
        copy_core::copy_text("credsStore.template.note", &[("keyField", KEY_FIELD)])
    );
    assert_eq!(
        doc["_note_auth_style"],
        copy_core::copy_text("credsStore.template.noteAuthStyle", &[])
    );
    // 例子里点的就是骨架里那两个字段名（名字只住常量一处）。
    let acc = doc["_note_accounts"].as_str().expect("说明是字符串");
    assert!(
        acc.contains(&format!(
            "\"{ACCOUNTS_FIELD}\": {{ \"my-account\": {{ \"{KEY_FIELD}\""
        )),
        "{acc}"
    );
    assert_eq!(doc.len(), 5, "骨架多了 / 少了键：{t}");
    let said = StoreError::NotAnObject.said(std::path::Path::new("/h/c/apikey-credentials.json"));
    assert!(said.contains("/h/c/apikey-credentials.json"), "{said}");
    assert!(!said.contains("_note"), "报错里又贴了整份模板：{said}");
}

// ═══════ 家目录：哪个环境变量算家，两侧一条规矩 ═══════════════════

/// 注入的环境（`(变量, 值)`；不在表里 ⇒ 没设）。
fn env_of(
    pairs: &'static [(&'static str, &'static str)],
) -> impl Fn(&str) -> Option<std::ffi::OsString> {
    move |k| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| (*v).into())
    }
}

/// ★ 要求：「规则按平台惯例 —— **Windows：`USERPROFILE` → `HOME`；其余：`HOME` → `USERPROFILE`**…
/// 都空 ⇒ `None`、调用方明说」。两臂各喂同一组注入环境，期望手写；空串当没设。
#[test]
fn the_home_is_picked_by_the_platform_convention() {
    let p = |s: &str| Some(std::path::PathBuf::from(s));
    let both: &[(&str, &str)] = &[("USERPROFILE", r"C:\Users\u"), ("HOME", "/home/u")];
    let only_home: &[(&str, &str)] = &[("HOME", "/home/u")];
    let only_profile: &[(&str, &str)] = &[("USERPROFILE", r"C:\Users\u")];
    let empty_first_win: &[(&str, &str)] = &[("USERPROFILE", ""), ("HOME", "/home/u")];
    let empty_first_other: &[(&str, &str)] = &[("HOME", ""), ("USERPROFILE", r"C:\Users\u")];
    let blank: &[(&str, &str)] = &[("HOME", ""), ("USERPROFILE", "")];
    let cases: [(&'static [(&str, &str)], bool, Option<std::path::PathBuf>); 10] = [
        (both, true, p(r"C:\Users\u")),
        (only_home, true, p("/home/u")),
        (empty_first_win, true, p("/home/u")),
        (blank, true, None),
        (&[], true, None),
        (both, false, p("/home/u")),
        (only_profile, false, p(r"C:\Users\u")),
        (empty_first_other, false, p(r"C:\Users\u")),
        (blank, false, None),
        (&[], false, None),
    ];
    for (env, windows, want) in cases {
        assert_eq!(
            home_dir_on(&env_of(env), windows),
            want,
            "env {env:?} · windows={windows}"
        );
    }
    // 本进程这一臂：`home_dir_from` 取的是本平台那一格（门禁在 Linux 上跑 ⇒ 非 Windows 那一臂）。
    #[cfg(not(windows))]
    assert_eq!(home_dir_from(&env_of(both)), p("/home/u"));
}

/// 两棵生产树（monitor 前端树 · 后端）里 `dirs::home_dir` 的调用：允许的只有下面这几处，逐条说理由（其余一律改调 [`home_dir`]）。
const DIRS_HOME_ELSEWHERE: &[(&str, &str)] = &[
    // `ccm_probe.rs` 那一行摘了：`local_ccm_entry_status` 改调 `home_dir`。
    // `src/frontend/filewin/src/shell.rs` 那一行（`local_home`）摘了：改调本函数（文件窗口包链 `creds-core`，契约类）。
];

/// 一份源码（原文）的生产段里 `dirs::home_dir(` 几处。
fn dirs_home_calls(src: &str) -> usize {
    guard_core::production_code(src)
        .matches("dirs::home_dir(")
        .count()
}

/// ★ 要求：「「哪个环境变量算家」是两侧必须对上的**契约** ⇒ 收成 `creds-core` 里唯一一个函数」·
/// 「monitor 生产段为数据目录 / 家目录用 `dirs::home_dir` 零命中（有别的正当用途就登记理由）」。
/// 调用点现扫（异源：读两棵树的源码，不信任何一侧自报）：① monitor 那份数据目录（`config.rs::resolve_monitor_data_dir`）与
/// 后端那一处家目录（`platform/paths.rs` 的 `home_dir` · `home_dir_from`）都调本模块这一个函数；② 两棵生产树里 `dirs::home_dir(` ==
/// [`DIRS_HOME_ELSEWHERE`]（两向）。正控：往 `config.rs` 副本里塞回一处数得出。
#[test]
fn both_halves_read_the_home_through_this_one_function() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let read = |rel: &str| {
        std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读不到 `{rel}`：{e}"))
    };
    let body_of = |src: &str, head: &str| -> String {
        let prod = guard_core::production_code(src);
        let at = prod
            .find(head)
            .unwrap_or_else(|| panic!("找不到 `{head}` —— 改名了就回来改本条"));
        let tail = &prod[at..];
        tail[..tail.find("\n}").expect("函数体没收尾")].to_string()
    };
    let config = read("src/frontend/shell/src/config.rs");
    assert!(
        body_of(&config, "pub fn resolve_monitor_data_dir()")
            .contains("creds_core::store::home_dir()"),
        "monitor 的数据目录不经 `creds_core::store::home_dir` 取家 —— 两侧会认两个家"
    );
    let paths = read("src/backend/platform/paths.rs");
    assert!(body_of(&paths, "pub(crate) fn home_dir()").contains("creds_core::store::home_dir()"));
    assert!(body_of(&paths, "pub(crate) fn home_dir_from(")
        .contains("creds_core::store::home_dir_from(get)"));
    let mut got: Vec<String> = Vec::new();
    for tree in ["src/frontend", "src/backend"] {
        let mut files = guard_core::scan_tree_excluding(&root.join(tree), &["rs"], &[]);
        // 前端那一侧 ＋ monitor 人群声明里住在它外面的兄弟包（通道 · 宿主原语 · 开窗契约）：通道成员从前经壳 `#[path]` 挂在前端树的人群里。
        let frontend = root.join("src/frontend");
        if tree == "src/frontend" {
            for (_, t) in guard_core::population_trees(&frontend.join("shell/src")) {
                if !t.starts_with(&frontend) {
                    files.extend(guard_core::scan_tree_excluding(&t, &["rs"], &[]));
                }
            }
            files.sort_by(|a, b| a.0.cmp(&b.0));
            files.dedup_by(|a, b| a.0 == b.0);
        }
        assert!(
            files.len() > 50,
            "`{tree}` 只扫到 {} 份 —— 遍历坏了",
            files.len()
        );
        for (p, src) in files {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            for _ in 0..dirs_home_calls(&src) {
                got.push(rel.clone());
            }
        }
    }
    got.sort();
    let want: Vec<String> = DIRS_HOME_ELSEWHERE
        .iter()
        .map(|(f, _)| f.to_string())
        .collect();
    assert_eq!(got, want, "`dirs::home_dir` 的调用点变了 ⇒ 改调 `creds_core::store::home_dir`；确有别的正当用途的写进表里说清");
    let planted = config.replace("creds_core::store::home_dir()", "dirs::home_dir()");
    assert_ne!(planted, config, "正控的锚没落在靶上");
    assert!(dirs_home_calls(&planted) >= 1);
}

/// 删号 · 回滚那两步：只摘 / 只放回点名的那一条，别的条与两层的未知键一个不动；那一条不在 ⇒ 原样。
#[test]
fn removing_and_restoring_one_account_touches_only_that_account() {
    let doc = parse(
        r#"{"_note":"keep","accounts":{"a":{"api_key":"sk-a","x":1},"b":{"api_key":"sk-b","base_url":"https://b.example"}}}"#,
    )
    .unwrap();
    let gone = remove_account(&doc, "a");
    assert_eq!(gone["_note"], "keep");
    assert!(gone["accounts"].get("a").is_none(), "{gone:?}");
    assert_eq!(gone["accounts"]["b"], doc["accounts"]["b"]);
    assert_eq!(remove_account(&doc, "nope"), doc, "不在的那一条也改了文档");
    // 删之后又有人写进来一条 c：放回 a 时 c 留着。
    let mut later = gone.clone();
    later["accounts"]
        .as_object_mut()
        .unwrap()
        .insert("c".into(), serde_json::json!({"api_key": "sk-c"}));
    let back = restore_account(&later, "a", &doc);
    assert_eq!(back["accounts"]["a"], doc["accounts"]["a"]);
    assert_eq!(back["accounts"]["c"]["api_key"], "sk-c");
    assert_eq!(restore_account(&later, "nope", &doc), later);
}
