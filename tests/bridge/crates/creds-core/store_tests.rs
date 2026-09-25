//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md` 的 `apikey-key-set` / `apikey-read` 两节（改一个键不许吃掉人手编的内容）
//!
//! 核原文：`apikey-key-set` 节逐字「别的行与未知键一个不动」·「`bad_file`（现有文件解析不了 ⇒ **不覆盖**，人手编的内容不许被抹掉）」；
//! `apikey-read` 节逐字「解析不了不退化成「没配」」。〔GP1 · 第四波〕写者今天只剩一种 —— 每台机器那台后端的
//! `accounts/upstream/file_face.rs`（本机也是；monitor 那侧的写口删了）—— 它用本模块的纯逻辑兑现这几句 —— 本族判的就是那份共用逻辑。手编 JSON 原样即用、模板自带字段名那几条守 `设计/05 §4.4`
//! 逐字「只放一个凭据文件、一次界面都不开，中转就能用那把 key」。
//! ⚠ 落盘键序（按名 · 递归 · 数组保序）与 `auth_style` 往返那几条今天没有逐字原文。
//! ⚠ `INVARIANTS §42` 的机检只核字段名落节、不核行为；契约里行为句不漂靠的是本族（射程待主会话确认，见 `JA1.md`）。〔JA1 点址 2026-09-24〕

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
    let doc = parse(TEMPLATE).expect("模板自己必须是合法的一份 store");
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

/// `KS10②` 的**端到端那一半**（整份进、整份出）。
///
/// ⚠ **订正〔`K-H2` `D1` 阻-3 回修，08-28〕：本段那句「它单独存在时是安慰剂」今天不成立了。**
/// 先前逐字写的是：「`serde_json::Map` 在今天这份构建里是 `BTreeMap`（插进去就有序）
/// ⇒『两份插入顺序相反的文档』这个夹具**造不出来**，把排序整条删掉它照样绿（`MU9` 实测 19/19）」。
/// `K-H2` 给本 crate 的 `[dev-dependencies]` 加了 `serde_json` 的 `preserve_order`
/// ⇒ **判据构建里 `Map` 就是 `IndexMap`**，那个夹具造得出来了。
/// 实测：删掉 [`ordered_value`] 的递归 ⇒ **本条与 `nested_objects_…` 两条一起红**（30 passed / 2 failed）。
/// ⇒ **`MU9` 当年那条「造不出来」是对当时那份构建说的，不是一条永久事实。**
/// ⚠ 但**牙的来源变了要说清**：本条今天有牙靠的是那一行 dev-dependency；删了它就退回安慰剂。
/// `ordering_is_by_name_not_by_arrival` 仍然是**不依赖任何 feature** 的那一格，两条配着用。
///
/// ⚠⚠ **而「那一行被删掉」这件事，先前没有任何东西会说**〔`D2` `§五-3`，`C-补` 08-28 补上〕。
/// `D2` `P6` 实测：删掉那行 dev-dependency、**生产码一个字不动** ⇒ 全量门禁 `GATE: OK`、
/// 三个数逐个不变 —— **本条与 `nested_objects_…` 一起悄悄退回安慰剂，而没人会知道。**
/// ⇒ 判据体第一段加了**反空真自检**（直接量「`Map` 是不是插入序」）：
/// 那一行没了 ⇒ **本条当场红**，报文逐字点出是哪一行依赖没了，不再是「悄悄退回」。
#[test]
fn the_field_order_does_not_depend_on_the_map_implementation() {
    // ★★ **反空真自检排最前**〔`C-补` 08-28〕：本条的牙**整个**架在
    //    `[dev-dependencies]` 里 `serde_json` 那行 `preserve_order` 上 —— 没有它
    //    `Map` 是 `BTreeMap`，下面那两份「插入顺序相反」的夹具**根本造不出来**
    //    （两边喂进去的本来就是同一个有序结构，删掉排序也照绿）。
    //    这两行就是「那一行没了会说话的东西」。
    let mut probe = Map::new();
    probe.insert("z".into(), Value::from(1));
    probe.insert("a".into(), Value::from(2));
    assert_eq!(
        probe.keys().next().map(String::as_str),
        Some("z"),
        "判据构建里 Map 不是插入序 —— `preserve_order` 那行 dev-dependency 没了，本条已退回安慰剂"
    );

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
    let msg = parse(r#"{"a":}"#).unwrap_err().to_string();
    assert!(
        msg.contains("手编"),
        "错误说法没告诉人这文件是手编的：{msg}"
    );
    assert!(
        msg.contains("没有动它"),
        "错误说法没说清程序没破坏文件：{msg}"
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
    for raw in [
        TEMPLATE,
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

/// **`KH5c`**：落盘文本在**深度 ≥2** 上也按键名排。
///
/// # ★★ 它**有牙**（`D1` 阻-3 回修之后）
///
/// ⚠⚠ **本段先前写的是「这一条今天没有牙，它是一条给明天用的绊线」，那句话已被实测证伪。**
/// 病灶是我把一个**关于分母的全称句**（「`BTreeMap` ⇒ 嵌套层乱序的夹具造不出来」）
/// 当成前提直接写进了头注，而**没有去打它**。
/// `D1` 只加了一行 dev-dependency（`serde_json` 开 `preserve_order`）就把夹具造了出来。
///
/// 今天的读数（我自己复打）：原码 **32 passed / 0 failed**；
/// 把 [`ordered_value`] 的 `Value::Object` 那一支删成 `other.clone()`
/// ⇒ **30 passed / 2 failed**，红的是本条 + 隔壁 `the_field_order_does_not_depend_on_the_map_implementation`。
/// ⇒ **深度 ≥2 今天真有人守着。**
///
/// ⚠ 它仍然要配着 `ordering_is_by_name_not_by_arrival` 读：那一条收迭代器，
/// 与 `Map` 是哪种实现**无关**；本条则是**靠 `preserve_order` 把 `Map` 换成 `IndexMap`** 才有牙的
/// —— 哪天那一行 dev-dependency 被删掉，本条**先前会悄悄退回**成安慰剂。
/// 那一行的理由与读数写在 `crates/creds-core/Cargo.toml` 里，**别当成可有可无的依赖**。
///
/// ⚠⚠ **「悄悄」那一格已经补上了**〔`D2` `§五-3`，`C-补` 08-28〕：判据体第一段是
/// **反空真自检**（直接量「`Map` 是不是插入序」）⇒ 那一行没了本条**当场红**。
/// 立项读数：`D2` `P6` 删掉那行、生产码不动 ⇒ 全量门禁 `GATE: OK`、三个数逐个不变，
/// **没有任何东西说话**；`P6b`（再删递归）⇒ 32 passed / 0 failed ⇒ 确实退回了安慰剂。
#[test]
fn nested_objects_are_also_ordered_by_name_in_what_lands_on_disk() {
    // ★★ **反空真自检排最前**〔`C-补` 08-28〕：见本条头注最后一段。
    //    没有 `preserve_order`，下面这个「嵌套层乱序」的夹具造不出来（`parse` 出来就已经排好），
    //    整条判据会变成「排过的东西还是排过的」——恒真。
    let mut probe = Map::new();
    probe.insert("z".into(), Value::from(1));
    probe.insert("a".into(), Value::from(2));
    assert_eq!(
        probe.keys().next().map(String::as_str),
        Some("z"),
        "判据构建里 Map 不是插入序 —— `preserve_order` 那行 dev-dependency 没了，本条已退回安慰剂"
    );

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
}

/// **数组的顺序是数据，不许排；但数组里的对象要递归进去。**
///
/// 这一条守的是**方向**：把 [`ordered_value`] 的 `Value::Array` 那一支改成「排数组」会当场红。
///
/// # ⚠⚠ 「递归进去」那一半先前**没有牙**，本轮取到了〔`D2` `§五-4`，`C-补` 08-28〕
///
/// 本条先前逐字承认：「把那一支改成 `other.clone()`（不递归进数组里的对象）本条**看不出来**
/// —— 夹具是一个**标量数组**，人群里没有『数组里套对象』那一形」。**那句自陈属实**
/// （`D2` `P7`：不递归 + 旧夹具 ⇒ **32 passed / 0 failed**，一声不吭）。
/// ⇒ 本轮把缺的那一形**加进夹具**：`{"list":[{"zzz":1,"aaa":2}]}`，断 `aaa` 排在 `zzz` 前。
/// 今天这一支**真有人守**：不递归 ⇒ 本条红，报文逐字说「数组里那个对象没被递归排序」。
///
/// ⚠ **牙的来源要说清**：新那一段与 `nested_objects_…` 同源 —— 它靠
/// `[dev-dependencies]` 里 `serde_json` 那行 `preserve_order` 把 `Map` 换成 `IndexMap`，
/// 否则 `parse` 出来的内层对象**本来就已经排好**，「有没有递归进去」看不出来。
/// ⇒ 那一段前面同样立着**反空真自检**。**数组本身那一半（顺序是数据、不许排）不依赖它。**
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
    // ★★ 反空真自检排在这一段最前（同 `nested_objects_…`）：没有 `preserve_order`
    //    下面那个内层对象 `parse` 出来就已经排好，这一段会恒真。
    let mut probe = Map::new();
    probe.insert("z".into(), Value::from(1));
    probe.insert("a".into(), Value::from(2));
    assert_eq!(
        probe.keys().next().map(String::as_str),
        Some("z"),
        "判据构建里 Map 不是插入序 —— `preserve_order` 那行 dev-dependency 没了，下面这一段已退回安慰剂"
    );

    let nested = parse(r#"{"list":[{"zzz":1,"aaa":2}]}"#).expect("嵌套夹具");
    let nested_text = to_pretty_json(&nested);
    // 非空对照：落盘的仍是合法 JSON，而且数组那一层**还在**（不是被压没了）。
    let nested_back: Value = serde_json::from_str(&nested_text).expect("合法 JSON");
    assert_eq!(nested_back["list"][0]["aaa"], 2);
    let iaaa = guard_core::find_pinned(&nested_text, "\"aaa\"").expect("`aaa` 应当恰好出现一处");
    let izzz = guard_core::find_pinned(&nested_text, "\"zzz\"").expect("`zzz` 应当恰好出现一处");
    assert!(
        iaaa < izzz,
        "数组里那个对象没被递归排序 —— `ordered_value` 的 `Value::Array` 那一支没有往下走：{nested_text}"
    );
}
