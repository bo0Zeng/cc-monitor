fn screen(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| l.to_string()).collect()
}

/// ★ 认档：屏底那一行写的是哪一档就是哪一档（几种写法都认、大小写不论）；自下而上找，正文里引了同样的字盖不过屏底；
/// 一行档标都没有 ⇒ 认不出（不按「没有标就是默认」猜）。
#[test]
fn the_mode_is_read_from_the_bottom_line_and_never_guessed() {
    let read = super::mode_on_screen;
    assert_eq!(
        read(&screen(&[
            "> ",
            "  ⏵⏵ accept edits on (shift+tab to cycle)"
        ])),
        Some("acceptEdits")
    );
    assert_eq!(
        read(&screen(&["> ", "  ⏸ plan mode on (shift+tab to cycle)"])),
        Some("plan")
    );
    assert_eq!(
        read(&screen(&["> ", "  ⏸ manual mode on"])),
        Some("default")
    );
    assert_eq!(read(&screen(&["> ", "  ⏵⏵ Auto mode on"])), Some("auto"));
    assert_eq!(
        read(&screen(&["> ", "  ⏵⏵ bypass permissions on"])),
        Some("bypassPermissions")
    );
    assert_eq!(
        read(&screen(&[
            "正文里说 accept edits on",
            "> ",
            "  ⏸ plan mode on"
        ])),
        Some("plan"),
        "正文里那一句盖过了屏底"
    );
    assert_eq!(read(&screen(&["> ", "  ? for shortcuts"])), None);
    assert_eq!(read(&[]), None);
}

/// 一条本地命令的输出记录（只放那一句；别的格合成）。
fn stdout_line(said: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "system",
        "subtype": "local_command",
        "content": format!("<local-command-stdout>{said}</local-command-stdout>"),
        "uuid": "u-1"
    })
}

/// ★ 换模型读回：认 `/model` 之后那条本地命令输出里的 `Set model to …` / `Kept model as …`，取反引号里的显示名（带括号的照原样），
/// 后面接的 effort 那一截不进显示名；别的记录、别的输出 ⇒ 认不出（不猜）。原文照 Claude Code 实际写的那几句。
#[test]
fn the_model_is_read_back_from_the_local_command_output() {
    let read = super::model_said;
    let set = read(&stdout_line(
        "Set model to `Opus 5.5` and saved as your default for new sessions",
    ))
    .unwrap();
    assert_eq!((set.name.as_str(), set.kept), ("Opus 5.5", false));
    let set = read(&stdout_line(
        "Set model to `Opus 5.5 (1M context)` and saved as your default for new sessions with `xhigh` effort",
    ))
    .unwrap();
    assert_eq!(set.name, "Opus 5.5 (1M context)");
    let set = read(&stdout_line(
        "Set model to `Opus 5.5 (1M context) (default)` and saved as your default for new sessions with `high` effort",
    ))
    .unwrap();
    assert_eq!(set.name, "Opus 5.5 (1M context) (default)");
    assert_eq!(
        read(&stdout_line(
            "Set model to `Fable 5.1` and saved as your default for new sessions"
        ))
        .unwrap()
        .name,
        "Fable 5.1"
    );
    let kept = read(&stdout_line("Kept model as `Opus 5 (1M context)`")).unwrap();
    assert_eq!(
        (kept.name.as_str(), kept.kept),
        ("Opus 5 (1M context)", true)
    );
    // 别的本地命令输出 · 不是本地命令输出的记录 ⇒ 认不出。
    assert!(read(&stdout_line("Conversation compacted")).is_none());
    assert!(read(&serde_json::json!({"type": "user", "message": {"role": "user", "content": "Set model to `X`"}})).is_none());
    assert!(read(&stdout_line("Set model to Opus")).is_none());
}

/// ★ 换模型送的那一行：`/model <值>`。
#[test]
fn the_model_command_is_slash_model_and_the_value() {
    assert_eq!(super::model_command("opus", None), "/model opus");
    assert_eq!(
        super::model_command("sonnet[1m]", None),
        "/model sonnet[1m]"
    );
}

/// ★ 每一档都有字：起会话可选的 ∪ 只会显示的那几档 == 记录里会写的那七个值（`recognize::facts::tests` 那七个：六个真值 ＋ 别名 `manual`），
/// 两向；每一档经这一家的权限档那一处（[`crate::agents::PermissionFace::text_of`]）给字，就是用户看到的那几个字；屏底认得出的档都在里面。认不出的档不猜字。
#[test]
fn every_mode_the_session_can_be_in_has_its_words() {
    use std::collections::BTreeSet;
    let permission = super::super::LAUNCH.permission.unwrap();
    let text = |m: &str| permission.text_of(m);
    let known: BTreeSet<&str> = super::PERMISSION_MODES
        .iter()
        .map(|m| m.value)
        .chain(super::SHOWN_MODES.iter().map(|c| c.value))
        .collect();
    let in_records = BTreeSet::from([
        "default",
        "plan",
        "acceptEdits",
        "bypassPermissions",
        "dontAsk",
        "auto",
        "manual",
    ]);
    assert_eq!(known, in_records, "有字的档与记录里会写的档不是同一组");
    // 期望的字照键直接从文案表取（不经这一家给字的那条链）。
    let table: serde_json::Value = serde_json::from_str(copy_core::TABLE_JSON).unwrap();
    let said = |key: &str| table["entries"][key]["zh"].as_str().map(str::to_owned);
    for (m, key) in [
        ("default", "beAsk.choice.planManual"),
        ("manual", "beAsk.choice.planManual"),
        ("acceptEdits", "beAsk.choice.planEdits"),
        ("plan", "beSessionNew.mode.plan"),
        ("auto", "beAsk.choice.planAuto"),
        ("bypassPermissions", "beSessionMode.mode.bypassPermissions"),
        ("dontAsk", "beSessionMode.mode.dontAsk"),
    ] {
        assert!(said(key).is_some(), "表里没有 `{key}`");
        assert_eq!(text(m), said(key), "`{m}` 的字");
    }
    for (_, m) in super::MODE_MARKS {
        assert!(known.contains(m), "屏底认得出的 `{m}` 没有字");
    }
    assert_eq!(text("noSuchMode"), None);
}

/// 记录里的模型原值 ⇒ 写给人看的名字：系列名同起会话那张表的字，版本取一两位的数段（日期段不算）；认不出 ⇒ 没有字，不猜。
#[test]
fn the_model_written_in_the_record_reads_as_its_name() {
    let text = super::super::LAUNCH.model_text.unwrap();
    assert_eq!(
        text("claude-opus-4-5-20251101").as_deref(),
        Some("Opus 4.5")
    );
    assert_eq!(
        text("claude-sonnet-4-20250514").as_deref(),
        Some("Sonnet 4")
    );
    assert_eq!(
        text("claude-3-5-haiku-20241022").as_deref(),
        Some("Haiku 3.5")
    );
    for m in [
        "<synthetic>",
        "claude-opus",
        "claude-mystery-4-5",
        "gpt-5",
        "",
    ] {
        assert_eq!(text(m), None, "`{m}` 不该有字");
    }
}

/// ★ 记录里的模型原值认成换模型清单里唯一对得上的那一项：`claude-opus-…` ⇒ `opus`；`claude-sonnet-…` 在清单里有 `sonnet` 与
/// `sonnet[1m]` 两项、记录里分不出 ⇒ 不认；不是这一家的值 ⇒ 不认。
#[test]
fn the_record_model_is_picked_only_when_one_item_matches() {
    let pick = super::super::LAUNCH.model_pick.unwrap();
    assert_eq!(pick("claude-opus-4-1-20250805"), Some("opus"));
    assert_eq!(pick("claude-haiku-4-5-20251001"), Some("haiku"));
    assert_eq!(pick("claude-sonnet-4-5"), None);
    assert_eq!(pick("gpt-5"), None);
}

/// 真 Claude Code 2.1.302 上、对话里已有回复时 `/model sonnet` 之后那一屏（工作目录与第一句换成了中性的）。
const SWITCH_MODEL_BOX: &str =
    include_str!("../../../__fixtures__/claude-switch-model-box.screen.txt");

/// ★ 换模型的确认框：认得出真机那一屏（光标停在「是」上，按回车就是确认）；光标不在「是」上 · 别的框 · 正文里引了框的标题 · 平常的一屏 ⇒ 认不出（不乱按）。
#[test]
fn the_switch_model_box_is_recognized_and_nothing_else_is() {
    let seen = |s: &str| super::model_confirm_on_screen(&screen(&s.lines().collect::<Vec<_>>()));
    assert!(seen(SWITCH_MODEL_BOX), "真机那一屏认不出");
    let cursor_on_no = SWITCH_MODEL_BOX
        .replace("❯ 1. Yes, switch to", "  1. Yes, switch to")
        .replace("  2. No, go back", "❯ 2. No, go back");
    assert!(!seen(&cursor_on_no), "光标在「否」上也认了");
    assert!(!seen(
        "   Do you want to proceed?\n\n   ❯ 1. Yes\n     2. No"
    ));
    assert!(!seen("● 换模型时会弹 Switch model? 那个框\n\n> "));
    assert!(!seen("● 好\n\n> \n  ⏸ plan mode on"));
    let face = crate::agents::claudecode::LAUNCH.model_switch.unwrap();
    let bx = face.confirm.expect("Claude 换模型声明了确认框");
    // 终端认不认得这个键，挂在主 crate 那一条（`cross/agents.rs`）。
    assert_eq!(bx.key, "enter");
}
