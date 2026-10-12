//! known_hosts 那一行的规矩（OpenSSH 的格式）：主机名怎么写 · 一台一行、换了钥匙就换掉那一行。
//! 记到哪份文件、怎么锁、怎么落盘是宿主的事（后端 `dial/known_hosts.rs`）；这里只有纯规则。

/// OpenSSH 认的主机名写法：口 22 只写主机，别的口写 `[主机]:口`（IPv6 字面量的方括号先剥掉）。
pub fn host_pattern(host: &str, port: u16) -> String {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if port == 22 {
        bare.to_string()
    } else {
        format!("[{bare}]:{port}")
    }
}

/// 现有内容 ＋ 这台的一行（`<主机名> <算法> <base64>`）⇒ 新内容；同一台已是这一把 ⇒ `None`（不写）。
/// 同一主机名的旧行（别的钥匙）去掉；别的行原样留着。
pub fn merged(existing: &str, pattern: &str, key: &str) -> Option<String> {
    let line = format!("{pattern} {key}");
    let mut placed = false;
    let mut lines: Vec<&str> = Vec::new();
    for l in existing.lines().filter(|l| !l.trim().is_empty()) {
        if l.split_whitespace().next() != Some(pattern) {
            lines.push(l);
        } else if !placed {
            lines.push(&line);
            placed = true;
        }
    }
    if !placed {
        lines.push(&line);
    }
    let body = lines.join("\n") + "\n";
    (body != existing).then_some(body)
}
