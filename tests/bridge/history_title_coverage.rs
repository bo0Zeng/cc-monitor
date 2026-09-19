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
/// 性质 = 它必须出现在 `history.rs` 的标题抽取段里。
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
    for line in block.lines() {
        let ind = line.len() - line.trim_start().len();
        let s = line.trim_start();
        if ind == 4 && s.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            cur = s
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>();
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

    let here = include_str!("../../src/bridge/src/history.rs");
    let missing: Vec<&String> = bearers
        .iter()
        .filter(|v| !here.contains(&format!("JsonlRecord::{v}")))
        .collect();
    assert!(
        missing.is_empty(),
        "这些记录类型带 `*title` 字段，却没被 `history.rs` 的标题抽取接住：{missing:?}\n\
             ⚠ 后果不是报错，是**标题静默消失** —— 会话列表上那一行变回默认名，\n\
             而没有任何判据会红。这件事真发生过一次（`ai-title` 改名成 `custom-title`），\n\
             那次是靠人发现的。\n\
             ⇒ 在标题抽取的 `match` 里给它加一条具名臂。"
    );
}
