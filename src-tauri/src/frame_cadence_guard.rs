//! **`TmuxSessions` 帧的节奏说法不许回到「每 ~8s」**（F01，定框 C8/C12）。
//!
//! # 病史：同一句假话在四个地方活了下来
//!
//! P5（`zero-poll-liveness`）删掉了 daemon 的 8s ticker 线程 —— 之后每一拍由
//! **tmux hook → `--tmux-notify` → SIGUSR1 → `WatchEvent::Poke`** 驱动，
//! server 生死由 pidfd / socket inotify 管，**daemon 零定时器**（`no_timer_guard` 钉着）。
//! monitor 侧那个 8s 对账 poller 也早已删（`run_tmux_reconcile_poller`，改收帧驱动）。
//!
//! **可是「daemon 每 ~8s 推帧」「帧最长 8s 陈旧」这两句仍留在四处注释里**，
//! 一句在 `lib.rs`（两处）、一句在 `tmux_reconcile.rs`、一句在 `ssh_source.rs`。
//! 它们是**理由过期而结论仍对**那一类 —— 结论（收帧驱动、零轮询）没错，
//! 而给出的**数量级是假的**，读的人会据此做设计判断。
//!
//! ⚠ **它真的造成过一次错判**：一次裁决里「把 1s 的 SSH 轮询换成等帧 = 拿延迟换负载，
//! 最长要等 8s」这个理由，整条建立在这句过期注释上。P5 之后真实的新鲜度**不是 8s** ——
//! 是「**取决于 hook 覆不覆盖那个变化**」：覆盖到的近乎即时，**覆盖不到的可能永不刷新**
//! （`classify_removed` 头注记着的 `/branch` 洞就是后者）。两种画像给出的设计结论完全不同。
//!
//! # 它查什么、查不了什么
//!
//! 零命中：生产段里不许再出现那两句**当下时态**的说法。
//! **历史叙述不受影响** —— 「P5 删掉 8s ticker 之后…」「已删的 8s poller」这类带限定词的
//! 写法不含被禁子串，而且它们是对的、应当留着。
//!
//! ⚠ **查不了「换个说法的同一句假话」**（比如「每八秒」「大约 8 秒一次」）——
//! 与本仓其它约定型守卫同一档。**比没有强，别读成证明。**
//!
//! ⚠ **本文件自己被排除在扫描面外** —— 上面那几段**逐字引用了被禁的说法**，
//! 不排除的话守卫基线就是红的（首跑实测：它咬了自己）。
//! 这正是本仓记过三次的那个坑：**判据要找的关键字，很可能就写在它自己的纪律说明里**。
//! ⚠ 而且它当时还**掩盖了另一条判据的变异复验** —— 基线红会让后续变异「看起来红」，
//! 分不清是谁红的。**基线必须先绿，变异复验才有意义。**

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// 被禁的**当下时态**说法。判据字符串运行时拼，避免命中本文件自己。
    fn forbidden() -> Vec<String> {
        let eight = "8s";
        vec![
            format!("每 ~{eight} 推"),
            format!("每 {eight} 推"),
            format!("帧最长 {eight} 陈旧"),
            format!("帧 ≤{eight} 陈旧"),
        ]
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    fn rust_sources(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for base in ["src-tauri/src", "remote-daemon-proto/src"] {
            walk(&root.join(base), &mut out);
        }
        out
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                // 排除本文件：它的头注逐字引用了被禁说法（见头注最后一段）。
                if p.file_name().is_some_and(|n| n == "frame_cadence_guard.rs") {
                    continue;
                }
                out.push(p);
            }
        }
    }

    /// ★ 抽取器自检：扫不到文件时下面那条会零命中地绿。
    #[test]
    fn the_scan_actually_reads_both_crates() {
        let n = rust_sources(&repo_root()).len();
        // 地板按「排除本文件后」的实测值定。
        assert!(n >= 90, "只扫到 {n} 个 .rs（实测应约 100）—— 扫描器坏了");
    }

    /// ★ 反向锚点：**真机制必须还写在某处** —— 否则本条退化成「谁都没提过 8s」的空守卫。
    ///
    /// 锚点选 `initial_tmux_probe` 的头注：它是 P5 留下的那一拍，
    /// 头注里逐字解释了「节拍没了，但首轮这一拍要留」。那段没了 = 真相源没了。
    #[test]
    fn the_real_cadence_is_still_documented_somewhere() {
        let w = fs::read_to_string(repo_root().join("remote-daemon-proto/src/observe/watcher.rs"))
            .expect("读不到 daemon watcher.rs");
        assert!(
            w.len() > 10_000,
            "watcher.rs 只有 {} 字节，像是抽错了",
            w.len()
        );
        for needle in ["initial_tmux_probe", "零定时器"] {
            assert!(
                w.contains(needle),
                "daemon watcher.rs 里找不到 `{needle}` —— P5 留下的那一拍与「零定时器」的说明没了，\
                 那本条就成了「谁都没提过 8s」的空守卫"
            );
        }
    }

    /// ★ 正题：不许再出现「每 ~8s 推帧」「帧最长 8s 陈旧」这类**当下时态**的假陈述。
    #[test]
    fn no_production_comment_claims_an_eight_second_frame_cadence() {
        let root = repo_root();
        let pats = forbidden();
        let mut offenders = Vec::new();
        for f in rust_sources(&root) {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            // ⚠ 这里**刻意不剥注释** —— 被禁的正是注释里的说法。
            let src = fs::read_to_string(&f).unwrap_or_default();
            for p in &pats {
                if src.contains(p.as_str()) {
                    offenders.push(format!("  {rel}: `{p}`"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "又出现了「daemon 每 ~8s 推帧」这类说法 —— **P5 之后它是假的**：\n\
             daemon 零定时器，帧由 tmux hook → SIGUSR1 → Poke 驱动 ⇒ 新鲜度**取决于 hook 覆不覆盖**\n\
             （覆盖到的近乎即时，覆盖不到的**可能永不刷新**）。这两种画像给出的设计结论完全不同 ——\n\
             实测有过一次裁决整条建立在这句过期注释上。\n\
             历史叙述请带限定词（「P5 前…」「已删的 8s poller」），那样不会命中本条。\n{}",
            offenders.join("\n")
        );
    }
}
