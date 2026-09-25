//! 〔RM1b · 第四波〕功能侧帧面宿主的判据。夹具只造结构，不采任何真会话正文。

use super::*;

/// 本族的帧命令 —— **题面给的**那几样（任务列表 · 插件市场），写成帧面名。
/// 它是判据的**异源**那一侧：下面那条从 `inbound.rs` 源码里数「谁把活交给了
/// `feature_face::answer`」，两边必须相等。
const FAMILY: &[&str] = &["plugins-marketplaces", "tasks-list"];

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("rm1b-face-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// ★ 两向相等：`inbound.rs` 生产段里把活交给本宿主的帧命令 == [`FAMILY`]。
#[test]
fn the_registry_hands_exactly_this_family_to_this_host() {
    let src = include_str!("../../src/backend/inbound.rs");
    let prod = crate::guard_support::production_code(src);
    let mut got: Vec<String> = prod
        .split("CommandSpec {")
        .skip(1)
        .filter(|blk| blk.contains("feature_face::answer"))
        .filter_map(|blk| {
            let at = blk.find("name: \"")? + "name: \"".len();
            Some(blk[at..].split('"').next()?.to_string())
        })
        .collect();
    got.sort();
    let mut want: Vec<String> = FAMILY.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(
        got, want,
        "交给 `feature_face::answer` 的帧命令与本族清单不相等"
    );
    // 反空真：本族每一条都真在 `COMMANDS` 镜子里（不只是在某段注释里出现过）。
    for c in FAMILY {
        assert!(
            crate::inbound::COMMANDS.contains(c),
            "`{c}` 不在 hello.commands 里"
        );
    }
}

#[test]
fn tasks_list_answers_in_lines_and_refuses_a_missing_sid() {
    let h = scratch("tasks");
    let dir = crate::observe::tasks_query::tasks_root(&h).join("s");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("1.json"),
        r#"{"id":"1","subject":"占位","status":"pending"}"#,
    )
    .unwrap();
    let v = answer_at(&h, "tasks-list", &json!({"sid": "s"})).unwrap();
    let rows = v["lines"].as_array().expect("lines");
    assert_eq!(rows.len(), 1);
    // 反向：缺 sid ⇒ bad_args；sid 走出任务根 ⇒ bad_args（围栏在本体里，这里只证它透得上来）。
    assert_eq!(
        answer_at(&h, "tasks-list", &json!({})).unwrap_err().0,
        "bad_args"
    );
    assert_eq!(
        answer_at(&h, "tasks-list", &json!({"sid": "../s"}))
            .unwrap_err()
            .0,
        "bad_args"
    );
    assert_eq!(
        answer_at(&h, "no-such", &json!({})).unwrap_err().0,
        "bad_args"
    );
}

#[test]
fn plugins_marketplaces_answers_the_survey_itself_and_keeps_the_three_exits_apart() {
    // ① 文件不在 ⇒ 〔C4b〕应答就是 survey（成品），`file_absent: true`、键集合恒等。
    let h = scratch("mk-absent");
    let survey = answer_at(&h, "plugins-marketplaces", &json!({})).unwrap();
    let mut keys: Vec<&String> = survey.as_object().expect("成品是一个对象").keys().collect();
    keys.sort();
    assert_eq!(keys, ["entries", "file_absent"], "survey 的成品形状变了");
    assert_eq!(survey["file_absent"], json!(true));
    // ② 读 / 解析失败 ⇒ `failed`，**不是**空表。
    let h = scratch("mk-broken");
    let p = crate::observe::plugins_query::plugins_root(&h).join("known_marketplaces.json");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "{ 不是 json").unwrap();
    assert_eq!(
        answer_at(&h, "plugins-marketplaces", &json!({}))
            .unwrap_err()
            .0,
        "failed"
    );
}

/// ★★〔C4b · 第四波 4B〕**跨语言金样**：`plugins-marketplaces` 对一份夹具家目录的成品 ==
/// `tests/__fixtures__/plugins-survey.golden.json`（夹具临时目录那一截换成 `<home>` 再比）。
///
/// 那份金样的另一个读者是界面的解码器（`tests/settings/plugins-section.vitest.ts` 读同一份文件、逐字段断言）
/// ⇒ 两侧**异源**：后端改一个键名 ⇒ 本条红；界面解码器改一个键名 ⇒ 那边红。夹具只造结构（占位名、占位时间）。
#[test]
fn the_survey_product_matches_the_cross_language_golden() {
    let h = scratch("c4b-golden");
    let plugins = crate::observe::plugins_query::plugins_root(&h);
    let good = h.join("mk-good");
    std::fs::create_dir_all(good.join(".claude-plugin")).unwrap();
    std::fs::write(
        good.join(".claude-plugin").join("marketplace.json"),
        r#"{"plugins":[{"name":"a"},{"name":"b"}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(
        plugins.join("known_marketplaces.json"),
        format!(
            r#"{{"a-good":{{"source":{{"source":"github","repo":"o/r"}},"installLocation":{:?},"lastUpdated":"2026-01-01T00:00:00Z"}},"b-bad":{{"installLocation":"/nonexistent/c4b"}}}}"#,
            good.to_string_lossy()
        ),
    )
    .unwrap();
    let v = answer_at(&h, "plugins-marketplaces", &json!({})).unwrap();
    let got: serde_json::Value =
        serde_json::from_str(&v.to_string().replace(&*h.to_string_lossy(), "<home>")).unwrap();
    let want: serde_json::Value =
        serde_json::from_str(include_str!("../__fixtures__/plugins-survey.golden.json"))
            .expect("金样不是合法 JSON");
    let _ = std::fs::remove_dir_all(&h);
    assert_eq!(
        got,
        want,
        "帧面成品与跨语言金样不一致。现打：\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}
