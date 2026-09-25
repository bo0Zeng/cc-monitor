/// ★ **每个带 `*title` 字段的 `JsonlRecord` 变体都必须被标题抽取吃到**〔audit-0805 08-06〕。
///
/// # 它钉的是一个**有历史先例**的缺口
///
/// 标题抽取那段 `match` 只认两种记录，其余走 `_ => {}` —— 静默忽略。
/// 对绝大多数记录类型这是对的（它们与标题无关），
/// **但对「又冒出一种承载标题的记录」就不是** ——
/// 而那件事**真的发生过**：Claude Code v2.1.x 把 `ai-title` 改名成 `custom-title`，
/// 仓里因此多了 `JsonlRecord::CustomTitle` 这一支。
/// 那次是**靠人发现的**：改名之后标题会静默消失，没有任何判据会红。
///
/// ⇒ 本条把「谁承载标题」这件事变成可机检的：
/// 人群 = `messages.rs` 的 `JsonlRecord` 里**带 `*title` 字段**的变体（今天 2 个）；
/// 性质 = 它必须被标题抽取段接住 ——〔C4d · 第四波 4B〕那一段今天住后端 `observe/history_query.rs::analyze_session`
/// （本机与远端同一个函数），按记录的线上类型名分臂。
/// 加第三种标题记录而忘了接 ⇒ 红。
///
/// ⚠ 人群刻意**不按变体名**取（`*Title` 结尾那种）——名字是可以随便起的，
/// 而「带一个叫 `xxx_title` 的字段」才是它承载标题的实据。
#[test]
fn every_title_bearing_record_is_consumed_by_the_extractor() {
    let msgs = include_str!("../../src/bridge/src/messages.rs");
    let b = msgs
        .find("enum JsonlRecord")
        .expect("找不到 `JsonlRecord` 定义 —— 记录类型搬家了，本条要跟着改");
    let e = msgs[b..].find("\nfn ").map_or(msgs.len(), |k| b + k);
    let block = &msgs[b..e];

    let mut cur = String::new();
    let mut bearers: Vec<String> = Vec::new();
    // 〔C4d〕每个变体在线上的记录类型名（它上面那行 `#[serde(rename = "…")]`）—— 后端按这个名字认记录。
    let mut pending_tag: Option<String> = None;
    let mut tags: std::collections::BTreeMap<String, String> = Default::default();
    for line in block.lines() {
        let ind = line.len() - line.trim_start().len();
        let s = line.trim_start();
        if ind == 4 {
            if let Some(rest) = s.strip_prefix("#[serde(rename = \"") {
                pending_tag = rest.split('"').next().map(str::to_string);
            }
        }
        if ind == 4 && s.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            cur = s
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>();
            if let Some(t) = pending_tag.take() {
                tags.insert(cur.clone(), t);
            }
        }
        if !cur.is_empty()
            && s.contains("title")
            && s.contains(':')
            && !bearers.contains(&cur)
            && s.split(':')
                .next()
                .is_some_and(|f| f.trim().ends_with("title"))
        {
            bearers.push(cur.clone());
        }
    }
    // 抽取器自检：一个都没抠到 ⇒ 下面的对拍会零命中地绿。
    assert!(
        bearers.len() >= 2,
        "只抠到 {} 个带 `*title` 字段的变体（08-06 实测 2：AiTitle / CustomTitle）\
             —— 抽取坏了，本条此刻是空转的：{bearers:?}",
        bearers.len()
    );

    // 〔C4d · 第四波 4B〕标题抽取**搬家了**：本机历史清单从 monitor 进程内（`history·rs::analyze_jsonl`〔散文墓碑〕，
    //   按 `JsonlRecord::<变体>` 分臂）搬进本机常驻后端，本机与远端同一个函数
    //   （`src/backend/observe/history_query.rs::analyze_session`，按记录的**线上类型名**分臂）。
    //   ⇒ 本条的「被接住」从「`history.rs` 里有 `JsonlRecord::<变体>`」换成「后端那个函数里有 `Some("<线上类型名>")` 那一臂」；
    //   人群（带 `*title` 字段的变体）照旧从 `messages.rs` 抠，线上类型名从它上面那行 `serde(rename)` 抠 —— 两份源码异源。
    let backend = include_str!("../../src/backend/observe/history_query.rs");
    let at = guard_core::find_pinned(
        backend,
        "fn analyze_session(p: &Path) -> serde_json::Value {",
    )
    .unwrap_or_else(|e| panic!("后端那个会话行函数不是恰好一处：{e}"));
    let body_end = backend[at + 1..]
        .find("\nfn ")
        .map_or(backend.len(), |k| at + 1 + k);
    let here = &backend[at..body_end];
    let missing: Vec<String> = bearers
        .iter()
        .filter(|v| {
            let tag = tags.get(*v).unwrap_or_else(|| {
                panic!("变体 {v} 上面没有 `serde(rename)` —— 认不出它的线上类型名")
            });
            !here.contains(&format!("Some(\"{tag}\")"))
        })
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "这些记录类型带 `*title` 字段，却没被后端会话行（`analyze_session`）的标题抽取接住：{missing:?}\n\
             ⚠ 后果不是报错，是**标题静默消失** —— 会话列表上那一行变回默认名，\n\
             而没有任何判据会红。这件事真发生过一次（`ai-title` 改名成 `custom-title`），\n\
             那次是靠人发现的。\n\
             ⇒ 在后端标题抽取的 `match` 里给它加一条具名臂。"
    );
    // 正控：每个承载标题的变体都真抠到了线上类型名（上面那一步 `unwrap_or_else` 已逐个核；这里再钉住人群没塌）。
    assert!(
        bearers.iter().all(|b| tags.contains_key(b)),
        "有承载标题的变体没抠到线上类型名：{bearers:?} / {tags:?}"
    );
}
