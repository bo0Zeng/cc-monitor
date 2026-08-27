//! 那份文件**长什么样**，以及「读一份 / 改一个键 / 再写回去」这三步的**纯逻辑**。
//!
//! # ⚠ 本模块**一个文件系统调用都没有**，那是刻意的（不是「还没写」）
//!
//! 三条理由，每条都指向本仓一条真实存在的判据：
//!
//! 1. **daemon 只许读，不许写**〔`K-H2a` 裁四〕：`readonly_guard.rs` 扫 daemon 生产段
//!    （剥掉 `#[cfg(test)]` 块之后）断言**不含任何文件系统变更调用**，白名单**恰好一个模块**
//!    `control/fork_write.rs`。本 crate 被 daemon depend ⇒ 写调用放进来是在给那道护栏挖洞。
//! 2. **写盘落点必须被登记表看见**：`src-tauri/src/write_site_registry.rs` 与
//!    `atomic_replace_registry.rs` 的扫描根**都是 `src-tauri/src`**
//!    （两者的 `src_root()` 逐字是 `CARGO_MANIFEST_DIR/src`）——
//!    `src-tauri/crates/` **不在它们的人群里**。
//!    ⇒ 把写盘搬进本 crate，等于**让它从两张登记表底下溜出去**，而那正是本工作区在治的那族病
//!    （守卫的人群对不上它守的性质）。⇒ 真正的写盘留在 `src-tauri/src`，在人群里、要申报。
//! 3. 纯函数好测：`KS10` 要测的是**交错**（人改了 A，程序写 B，A 还在不在），
//!    而交错的关键是「程序在**写的那一刻**才去读」——那条纪律由调用方兑现，
//!    本模块只保证「给我旧内容 + 新 key，我还你一份没吃掉任何东西的新内容」。
//!
//! # 格式（`KS9`：人能读能改，改完就生效）
//!
//! 一份**明文 JSON 对象**。今天本件只认一个键 [`KEY_FIELD`]，**其余键一律原样留着**。
//! 文件不存在时给 [`TEMPLATE`] —— 一个「能手编但没人知道格式」的文件等于不能手编。

use crate::SecretKey;
use serde_json::{Map, Value};

/// key 住在哪个字段。**这个名字是契约**：人手编的时候写的就是它。
pub const KEY_FIELD: &str = "api_key";

/// 那份文件相对 claude 家目录的位置。**两侧共用的唯一契约。**
///
/// # 为什么住这里，而不是两边各写一份字面量
///
/// monitor 与 daemon 各有自己的「家目录」解析（前者 `paths::resolve_monitor_data_dir`，
/// 后者 `agents::claudecode::paths::resolve_home`），但**落点的相对路径必须是同一个** ——
/// 两边各写一份字符串，漂开的那天没有任何东西会说，而症状是
/// 「界面上配好了，中转说没配」这种**查不出来**的形状。
///
/// ⚠ 它**不**跟随 `claudeDir` 覆盖（monitor 自己的数据目录本来就不跟随，见
/// `src-tauri/src/config.rs` 头注逐字：「monitor 自己的设置永远在默认
/// `~/.claude/claudecode-frontend/` 下，不跟随 `claudeDir` 字段变化」）。
pub const FILE_NAME: &str = "relay-credentials.json";

/// `<claude 家目录>/claudecode-frontend/relay-credentials.json`。
pub fn path_under_claude_home(home: &std::path::Path) -> std::path::PathBuf {
    home.join("claudecode-frontend").join(FILE_NAME)
}

/// 文件不存在 / 是空的时候给出去的模板。
///
/// ⚠ 它是**注释 + 一个空字段**，不是一份能直接用的配置 —— 目的是让人一眼看出该填哪儿。
/// JSON 没有注释语法，所以说明写成一个**未知键**（`_note`），而
/// 「未知键原样保留」正是 [`merge_key`] 的性质 ⇒ 这份模板**自己就是那条性质的用例**。
pub const TEMPLATE: &str = r#"{
  "_note": "把第三方 API key 填进 api_key。这份文件可以直接用编辑器改，改完下次读就生效；也可以整份换成另一份 JSON（导入）。本文件之外的键不会被程序动。",
  "api_key": ""
}
"#;

/// 读不动那份文件时的说法。**三态，不是两态** —— 「不存在」与「读坏了」必须分开：
/// 前者是正常的（还没配），后者要出声（人手编时打错了一个逗号，不该被当成「没配」）。
#[derive(Debug, PartialEq, Eq)]
pub enum StoreError {
    /// 不是合法 JSON。带上解析器的话，人手编打错时能看懂。
    NotJson(String),
    /// 是合法 JSON，但顶层不是对象（例如整份是个数组）。
    NotAnObject,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotJson(e) => write!(
                f,
                "凭据文件不是合法 JSON（{e}）。它是人能手编的明文 JSON —— \
                 多半是少了逗号或引号。**没有动它**，修好再试。"
            ),
            StoreError::NotAnObject => write!(
                f,
                "凭据文件的顶层不是一个 JSON 对象。正确形状见模板：{TEMPLATE}"
            ),
        }
    }
}

/// 解析一份盘上的内容。**空内容 = 空对象**（还没配，不是错）。
pub fn parse(raw: &str) -> Result<Map<String, Value>, StoreError> {
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    let v: Value = serde_json::from_str(raw).map_err(|e| StoreError::NotJson(e.to_string()))?;
    match v {
        Value::Object(m) => Ok(m),
        _ => Err(StoreError::NotAnObject),
    }
}

/// 从一份已解析的文档里取 key。没有 / 不是字符串 / 是空串 ⇒ `None`（都当成「还没配」）。
///
/// ⚠ **不许在这里对 key 做任何「顺手清理」**（去引号、剥前缀…）：
/// 人手编写进去什么，上游就该收到什么。猜错一次的代价是一条查不出来的 401。
/// 只 `trim` 首尾空白 —— 那是编辑器留下的，不是人的意思。
pub fn read_key(doc: &Map<String, Value>) -> Option<SecretKey> {
    let s = doc.get(KEY_FIELD)?.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    Some(SecretKey::new(s))
}

/// **`KS10` 的正主**：把 key 并进一份**刚从盘上读回来的**文档，其余键一个不动。
///
/// # 它为什么收 `current` 而不是收一个 `&mut self`
///
/// `daemon_policy.rs` 头注逐字记着本仓踩过的那个形状：同一个文件有两个写者时，
/// 「前端『读—改—写』整份的那一刻，会把 Rust 刚写进去的键按一份**陈旧副本**覆盖掉」。
/// **本件是同一个形状换了两个当事人（程序 vs 人手）。**
/// ⇒ 签名逼着调用方在**写的那一刻**把盘上的当前内容递进来，
/// 而不是递一份「界面打开时读的那一份」。这条约束写在类型上，不写在注释里。
pub fn merge_key(current: &Map<String, Value>, key: &SecretKey) -> Map<String, Value> {
    let mut out = current.clone();
    out.insert(
        KEY_FIELD.to_string(),
        // ★ 用的是**落盘那个出口**，不是换头那个。两个出口的人群刻意不相交，
        //   见 `SecretKey::expose_for_persisting` 的头注。
        Value::String(key.expose_for_persisting().to_string()),
    );
    out
}

/// 键的落盘顺序。**抽成一个纯函数，理由是一次实测证伪。**
///
/// # ⚠ 它为什么不能留在 `to_pretty_json` 里（`MU9`，08-27 实测）
///
/// 排序那一步原来内联在 `to_pretty_json` 里，判据是
/// `the_field_order_does_not_depend_on_the_map_implementation`：
/// 造两份**内容相同、插入顺序相反**的文档，断言落盘文本相同。
/// **变异实测：把 `keys.sort()` 整条删掉，19 条判据全绿。**
///
/// 根子是**判据的人群与它守的性质对不上**：`serde_json::Map` 在**这台机器今天这份构建**里
/// 是 `BTreeMap`（默认 feature），插进去就已经有序 ⇒ 「两份插入顺序相反的文档」这个夹具
/// **根本造不出来**，两边喂进去的本来就是同一个有序结构。
/// 而这条判据要守的恰恰是「**哪天 `preserve_order` 被依赖图里任何一个 crate 打开、
/// `Map` 变成 `IndexMap`（插入序）**，输出仍然稳定」——那一天今天造不出来。
///
/// ⇒ 把排序抽成一个**收迭代器**的纯函数：判据可以直接喂它一个乱序的序列，
/// 与 `Map` 今天是哪种实现**无关**。`MU9` 重打后被 `ordering_is_by_name_not_by_arrival` 逮住。
pub fn ordered_keys<'a>(keys: impl Iterator<Item = &'a String>) -> Vec<&'a String> {
    let mut v: Vec<&'a String> = keys.collect();
    v.sort();
    v
}

/// 序列化成盘上那份文本。
///
/// # `KS10②` 字段顺序稳定 —— 而且**不靠 `serde_json` 的默认行为**
///
/// `serde_json::Map` 默认是 `BTreeMap`（有序），但开了 `preserve_order` feature 之后
/// 它变成 `IndexMap`（插入序）。**那个 feature 由依赖图里任何一个 crate 打开都算数**，
/// 而 monitor 那棵树很大 ⇒ 「今天是有序的」不是一条能靠的性质。
/// ⇒ 这里**自己排一次序**，两种情况下输出都一样。
/// 由 `the_field_order_does_not_depend_on_the_map_implementation` 钉住。
pub fn to_pretty_json(doc: &Map<String, Value>) -> String {
    let mut ordered = Map::new();
    for k in ordered_keys(doc.keys()) {
        ordered.insert(k.clone(), doc[k].clone());
    }
    let mut s = serde_json::to_string_pretty(&Value::Object(ordered))
        .unwrap_or_else(|_| TEMPLATE.to_string());
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
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
        let hand = parse(
            r#"{"_note":"别删我","api_key":"OLD","zzz_last":1,"aaa_first":[1,2]}"#,
        )
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
        let got: Vec<&str> = ordered_keys(arrival.iter()).into_iter().map(|s| s.as_str()).collect();
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
    /// ⚠ **它单独存在时是安慰剂**，如实记：`serde_json::Map` 在今天这份构建里是
    /// `BTreeMap`（插进去就有序）⇒ 「两份插入顺序相反的文档」这个夹具**造不出来**，
    /// 把排序整条删掉它照样绿（`MU9` 实测 19/19）。有牙的那一格是
    /// `ordering_is_by_name_not_by_arrival`。**两条配着用，别只读这一条的名字。**
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
        let fresh = parse(r#"{"_note":"人刚改成这样","my_own":"别动我","api_key":"OLD"}"#)
            .expect("fresh");
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
        assert!(matches!(parse(r#"{"api_key": }"#), Err(StoreError::NotJson(_))));
        assert_eq!(parse("[1,2,3]"), Err(StoreError::NotAnObject));
        // 说法里要带得走「怎么修」——人手编打错时看得懂。
        let msg = parse(r#"{"a":}"#).unwrap_err().to_string();
        assert!(msg.contains("手编"), "错误说法没告诉人这文件是手编的：{msg}");
        assert!(msg.contains("没有动它"), "错误说法没说清程序没破坏文件：{msg}");
    }

    /// key 字段的几种「等于没配」的写法。
    #[test]
    fn several_shapes_all_mean_not_configured() {
        // 分母 = 我列出的这 4 形，**不是**「所有写法」。
        for raw in [r#"{}"#, r#"{"api_key":""}"#, r#"{"api_key":"   "}"#, r#"{"api_key":null}"#] {
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
}
