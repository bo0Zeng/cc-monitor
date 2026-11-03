//! ccm 行判据的共用夹具：没有配置层的那一形解析 · 旧写法夹具的换排列。本包判据与上层判据（经 `test-support`）共用。

use super::{flag, parse_layered, Die, Parsed};

/// 没有配置层的那一形（命令行当场给的就是全部）：判据各处喂解析器都经它。
pub fn parse(args: &[String]) -> Result<Parsed, Die> {
    parse_layered(&[], args).map(|(p, _)| p)
}

/// 〔只给测试〕把旧写法（ccm 选项与交给 agent 的词混写、`--` 之后全交 agent）改写成
/// `<交给 agent 的…> -- <ccm 的…>`。给沿用旧写法写夹具的那几份单测用 —— 意图逐词不变，只换排列。
pub fn mixed_to_split(old: &[String]) -> Vec<String> {
    const WITH_VALUE: [&str; 9] = [
        flag::TMUX_BASE,
        flag::TMUX_SIZE,
        flag::ACCOUNT,
        flag::CWD,
        flag::AGENT,
        flag::LAUNCHER,
        flag::ATTACH,
        flag::CCM_SID,
        flag::END,
    ];
    const BARE: [&str; 9] = [
        flag::TMUX,
        flag::HOSTED,
        flag::DETACH,
        flag::BASE,
        flag::CCM_PRINT,
        flag::CCM_HELP,
        flag::CCM_VERSION,
        flag::CCM_PROBE,
        flag::END,
    ];
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < old.len() {
        let a = &old[i];
        if a == flag::END {
            left.extend_from_slice(&old[i + 1..]);
            break;
        }
        let key = a.split_once('=').map_or(a.as_str(), |(k, _)| k);
        if BARE.contains(&key) || (WITH_VALUE.contains(&key) && a.contains('=')) {
            right.push(a.clone());
        } else if WITH_VALUE.contains(&key) {
            right.push(a.clone());
            if let Some(v) = old.get(i + 1) {
                right.push(v.clone());
                i += 1;
            }
        } else {
            left.push(a.clone());
        }
        i += 1;
    }
    if right.is_empty() && !left.iter().any(|a| a == flag::END) {
        return left;
    }
    left.push(flag::END.to_string());
    left.extend(right);
    left
}
