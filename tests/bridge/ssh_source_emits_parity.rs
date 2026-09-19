/// daemon `main.rs` 的 `EMITS` 常量（编译期内嵌 daemon 源码，同 `build_id` 那套单源思路）。
const DAEMON_MAIN: &str = include_str!("../../src/backend/main.rs");

fn daemon_emits() -> Vec<String> {
    let i = DAEMON_MAIN
        .find("const EMITS")
        .expect("daemon main.rs 里找不到 EMITS —— 抽取坏了，本断言在空转");
    let j = DAEMON_MAIN[i..]
        .find("];")
        .map(|k| i + k)
        .expect("EMITS 没有收尾");
    let mut out: Vec<String> = DAEMON_MAIN[i..j]
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// ★ daemon 承诺发的每个 kind，monitor 都必须**认识**（消费或刻意不消费）。
#[test]
fn every_kind_the_daemon_emits_is_known_to_the_monitor() {
    let emits = daemon_emits();
    assert!(
        emits.len() >= 6,
        "只抽到 {} 个 EMITS —— 抽取坏了，本断言在空转：{emits:?}",
        emits.len()
    );
    let unknown: Vec<&String> = emits
        .iter()
        .filter(|k| !super::KNOWN_FRAME_KINDS.contains(&k.as_str()))
        .collect();
    assert!(
        unknown.is_empty(),
        "daemon 的 `EMITS` 承诺发这些 kind，但 monitor 的 `parse_frame` 不认：{unknown:?}\n\
             后果不是「忽略」，是**每帧刷一条 warn** 并丢弃 —— 真正的坏帧会淹没在里面。\n\
             要么加一条消费臂，要么加进那条「认识但刻意不消费」的臂**并写明理由**。"
    );
}

/// ★ `KNOWN_FRAME_KINDS` 必须与 `parse_frame` 的 match 臂**完全一致**。
///
/// 两份名单必然漂移 —— 这条让它们只能是同一份（同 daemon 侧
/// `inbound.rs::the_commands_mirror_matches_the_registry` 的思路）。
///
/// 〔`K-R19` 订正 09-03〕这一句原先点的是 `hello_commands_match_the_dispatch_table`，
/// **全仓零定义**——那是 daemon 侧那条判据的**上一版**名字（`U8a-2d` 换掉的），
/// 而这一句仍当成现状在说。
#[test]
fn known_kinds_matches_parse_frame() {
    let src = include_str!("../../src/bridge/src/ssh_source.rs");
    let at = src
        .find("fn parse_frame")
        .expect("找不到 parse_frame —— 抽取坏了");
    let end = src[at..]
        .find("\n/// 本 monitor **认识**的全部帧 kind")
        .map(|k| at + k)
        .expect("找不到 parse_frame 的收尾锚点");
    let body = &src[at..end];
    let mut arms: Vec<String> = body
        .match_indices("\" =>")
        .filter_map(|(i, _)| {
            let head = &body[..i];
            let q = head.rfind('"')?;
            let name = &head[q + 1..];
            (!name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
                .then(|| name.to_string())
        })
        .collect();
    // 多 pattern 合并的臂（`"a" | "b" =>`）上面只会抓到最后一个，补齐。
    // U8a-2a：`reply`/`cancelled` 已各自独立成臂并**真消费**，只剩 `turn_end` 是
    // 「认识但刻意不消费」的那一条。
    for extra in ["turn_end"] {
        if body.contains(&format!("\"{extra}\"")) && !arms.contains(&extra.to_string()) {
            arms.push(extra.to_string());
        }
    }
    arms.sort();
    arms.dedup();
    assert!(
        arms.len() >= 8,
        "只切出 {} 条 kind 分支 —— 抽取坏了，本断言在空转：{arms:?}",
        arms.len()
    );
    let mut known: Vec<String> = super::KNOWN_FRAME_KINDS
        .iter()
        .map(|s| s.to_string())
        .collect();
    known.sort();
    assert_eq!(
        arms, known,
        "\n`KNOWN_FRAME_KINDS` 与 `parse_frame` 的 match 臂对不上 —— 两份名单已经漂了。"
    );
}
