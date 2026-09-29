/// 一段**生产代码**里有没有「建 tmux 会话」的形态。
///
/// 两种都算：shell 命令串 `tmux new-session …`，与 argv 元素 `"new-session", "-d"`。
/// ⚠ 只写 `new-session` 这个词会命中测试夹具与 UI 动作 id（摸底实测：宽模式 10 个
/// 文件、收窄后 4 个），所以两种形态都带上下文。
pub(crate) fn creates_a_session(prod: &str) -> bool {
    let verb = format!("new-{}", "session");
    let wide = format!("tmux {verb}");
    let argv = format!("\"{verb}\", \"-d\"");
    prod.lines().any(|l| {
        let t = l.trim_start();
        !t.starts_with("//")
            && !t.starts_with('#')
            && !t.starts_with('*')
            && (l.contains(wide.as_str()) || l.contains(argv.as_str()))
    })
}
