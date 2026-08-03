//! **前端 + `shared/ccm` 的周期唤醒清账**（用户 2026-08-03：「现在又轮询吗? 尽量不要轮询，
//! 既然都收编了那就尽量在内部进行事件驱动」）。
//!
//! # 为什么是本模块，而不是把 daemon 那条护栏扩过来
//!
//! daemon 侧的 `no_timer_guard`（§41）是**零容忍**的：那边一个自己醒过来的构件都不许有。
//! 它的头注自己写着范围：「**只钉 daemon crate。** monitor 侧另有自己的轮询纪律
//! （那边有 UI 刷新、重连退避等**正当**周期行为），把本护栏扩过去会立刻变成噪音
//! ⇒ **要钉那半得单独论证**。」
//!
//! **那个论证到今天没人做过** —— 于是前端与 `shared/ccm` 这半**一条机检都没有**，
//! 而两个文件的头注里写着「本文件里不得出现 setInterval / setTimeout 轮询」这类**散文纪律**
//! （`account-usage.ts` · `settings/cc-bus-section.ts`）。散文纪律 = 没有纪律，
//! 这正是本工作区一直在治的病。
//!
//! # 论证：这一半为什么是**登记表**而不是禁令
//!
//! 前端确实有**正当**的周期行为（UI 重绘时钟、退避），一刀禁掉就是 daemon 那条护栏
//! 预言的噪音。所以本模块钉的不是「不许有」，而是「**每一处都得说清它是哪一类、
//! 谁来退役它**」：
//!
//! | 类别 | 含义 | 要求 |
//! |---|---|---|
//! | `ui-clock` | 只重绘已有状态、**不取数** | 说清它不取数 |
//! | `data-poll` | **真轮询** —— 周期性去取数据 | **必须写明事件源在哪 + 谁退役它** |
//! | `wait-for-condition` | 等一个一次性条件（有上限，不是节拍器） | 说清没有内核事件源可用 |
//!
//! # 它查什么、查不了什么
//!
//! 查的是 TS 生产段（注释剥掉）里的 `setInterval`、**`poll` 命名的递归 `setTimeout`**、
//! 以及 `shared/ccm` 里的 `sleep`。
//!
//! ⚠ **查不了「没按 poll 命名的递归 `setTimeout`」** —— 那种形态在语法上与一次性延时
//! 无法区分（普查时正是靠 `pollTimer = setTimeout(…, pollMs)` 这个命名才抓到 `tabs.ts`
//! 那一处；裸 grep `setInterval` 看不见它）。**比没有强，别读成证明。**
//!
//! ⚠ **monitor 的 Rust 侧刻意不在范围内**：那边的 `thread::sleep` 大多是「等一个一次性条件」
//! 而不是节拍（`bind.rs` 的重试、`launch.rs` 的窗口等待…），逐条论证是另一件事。
//! **如实登记为未做，不假装覆盖了。**

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// 周期唤醒的**登记表**：`(相对仓根的路径, 类别, 为什么 + 谁退役它)`。
    ///
    /// 多一处没登记的 ⇒ 下面那条红。**登记表不是豁免清单**，是「这些我看过、
    /// 而且知道它归谁」的账。
    const REGISTERED: &[(&str, &str, &str)] = &[
        (
            "src/main.ts",
            "data-poll",
            "10s 拉一次 `refreshSessionAccounts`（会话↔账号映射）。**事件源已经存在** ——\
             daemon 起会话时就知道账号（§1.5「daemon 起的就记账」）；退役归 **U7e**，\
             而 U7e 按 §1.5 必须排在 U8 之后（U7↔U8 之间 Windows 上会话账号全未知的那个窗口）。",
        ),
        (
            "src/views/grid-monitor.ts",
            "ui-clock",
            "1s 重绘一次网格。**不取数** —— 只把已有状态（相对时间等）重画；\
             取数走事件（`events.ts` 的帧）。这一类是 daemon 那条护栏头注说的「正当周期行为」。",
        ),
        (
            "src/tabs.ts",
            "data-poll",
            "`awaitExitFor`：等 claude 退出时每 1s 拉一次 `list_remote_tmux`（有 timeout 上限）。\
             **事件源已经存在** —— daemon 的会话/判活帧（U7d 已交付本机判活）；\
             退役归 **U10**（停/接搬进 `control/` 时，「等它真的退了」应当由帧推回来）。\
             ⚠ 这一处是本轮普查**新发现**的：它用递归 `setTimeout` 而不是 `setInterval`，\
             此前任何地方都没登记过。",
        ),
        (
            "shared/ccm",
            "wait-for-condition",
            "两处：① 预信任对话框等待（6 × 0.5s，**§1.3 登记在案的例外** —— 那个对话框没有\
             内核事件源，只能看屏）；② 1s 身份轮询（`sleep 1`）。**②是真 data-poll**，\
             退役归 **U9b**（thin ccm 变零决策执行臂）。⚠ 一个文件两类，故按文件登记。",
        ),
    ];

    /// **明令不许有周期唤醒**的文件（把两处散文纪律变成机检）。
    const NO_PERIODIC_WAKE: &[(&str, &str)] = &[
        (
            "src/account-usage.ts",
            "头注写着「没有 `setInterval`，没有后台…」—— 用量探测是重操作（起隐藏会话+网络查询），\
             周期化会把它变成后台负载",
        ),
        (
            "src/settings/cc-bus-section.ts",
            "头注写着「本文件里不得出现 setInterval / setTimeout 轮询 / 后台定时任务」",
        ),
    ];

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    /// 剥掉整行注释（`//` / `*` / `/*`）。
    ///
    /// ⚠ 必须剥：`account-usage.ts` 与 `cc-bus-section.ts` 的头注里**就写着**
    /// `setInterval` 这个词（写的是「不许有」）。不剥的话它们会被自己的纪律说明命中 ——
    /// 与 `launch-cli-wire.vitest.ts` 那次「文档注释里就写着 `deny_unknown_fields`」同一个坑。
    fn strip_line_comments(src: &str) -> String {
        src.lines()
            .filter(|l| {
                let t = l.trim_start();
                !(t.starts_with("//") || t.starts_with("*") || t.starts_with("/*"))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 一行里有没有周期唤醒的形态。
    fn is_periodic(line: &str, is_shell: bool) -> bool {
        if is_shell {
            return line.contains("sleep ");
        }
        if line.contains("setInterval") {
            return true;
        }
        // `poll` 命名的递归 setTimeout —— 见模块头注的诚实边界。
        line.contains("setTimeout") && line.to_lowercase().contains("poll")
    }

    /// 扫描面：`src/**/*.ts`（排除测试）+ `shared/ccm`。
    fn scan() -> Vec<(String, usize)> {
        let root = repo_root();
        let mut files: Vec<PathBuf> = Vec::new();
        collect_ts(&root.join("src"), &mut files);
        files.sort();
        files.push(root.join("shared/ccm"));
        let mut out = Vec::new();
        for f in files {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            let is_shell = rel == "shared/ccm";
            let src = strip_line_comments(&fs::read_to_string(&f).unwrap_or_default());
            let n = src.lines().filter(|l| is_periodic(l, is_shell)).count();
            if n > 0 {
                out.push((rel, n));
            }
        }
        out
    }

    fn collect_ts(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_ts(&p, out);
                continue;
            }
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.ends_with(".ts")
                && !name.contains(".vitest.")
                && !name.contains(".test.")
                && !name.ends_with(".d.ts")
            {
                out.push(p);
            }
        }
    }

    /// ★ 抽取器自检：扫不到东西时下面几条会零命中变绿。
    #[test]
    fn the_scan_actually_reads_the_frontend_and_ccm() {
        let root = repo_root();
        let mut ts = Vec::new();
        collect_ts(&root.join("src"), &mut ts);
        assert!(
            ts.len() >= 60,
            "只扫到 {} 个前端 .ts（实测应约 90+）—— 遍历器坏了",
            ts.len()
        );
        assert!(
            fs::read_to_string(root.join("shared/ccm"))
                .map(|s| s.len())
                .unwrap_or(0)
                > 10_000,
            "shared/ccm 读不到或太短 —— 路径变了？"
        );
        // 剥注释不能把整份文件剥空。
        let ccm = fs::read_to_string(root.join("shared/ccm")).unwrap_or_default();
        assert!(
            strip_line_comments(&ccm).len() * 2 > ccm.len(),
            "剥注释后 shared/ccm 只剩不到一半 —— 剥法太狠"
        );
    }

    /// ★ 正题：**每一处周期唤醒都得在登记表里**。
    #[test]
    fn every_periodic_wake_is_registered_with_an_owner() {
        let found = scan();
        let registered: Vec<&str> = REGISTERED.iter().map(|(f, _, _)| *f).collect();
        let mut unregistered: Vec<String> = Vec::new();
        for (f, n) in &found {
            if !registered.contains(&f.as_str()) {
                unregistered.push(format!("  {f}（{n} 处）"));
            }
        }
        assert!(
            unregistered.is_empty(),
            "有周期唤醒没登记。**登记表不是豁免清单** —— 请写明它是哪一类\
             （ui-clock / data-poll / wait-for-condition），\
             `data-poll` 还要写明事件源在哪、谁退役它：\n{}",
            unregistered.join("\n")
        );
        // 反向：登记表里的文件必须**真的还有**周期唤醒（搬走/退役了就该删条目）。
        for (f, _, _) in REGISTERED {
            assert!(
                found.iter().any(|(g, _)| g == f),
                "登记表里的 `{f}` 已经没有周期唤醒了 —— 退役了就把这条删掉（别留成僵尸账）"
            );
        }
    }

    /// ★ `data-poll` 这一类**必须**写明事件源与退役去处 —— 那是它与「正当周期行为」的分界。
    #[test]
    fn every_data_poll_names_its_event_source_and_owner() {
        let mut polls = 0usize;
        for (f, kind, why) in REGISTERED {
            assert!(
                matches!(*kind, "ui-clock" | "data-poll" | "wait-for-condition"),
                "{f} 的类别 {kind:?} 不在三类里"
            );
            if *kind == "data-poll" || why.contains("data-poll") {
                polls += 1;
                assert!(
                    why.contains("事件源"),
                    "{f} 记成 data-poll 却没说事件源在哪"
                );
                assert!(why.contains("退役归"), "{f} 记成 data-poll 却没说谁退役它");
            }
        }
        assert!(polls >= 3, "只认出 {polls} 条 data-poll —— 分类抽取坏了");
    }

    /// ★ 把两处**散文纪律**变成机检：这两个文件里一处周期唤醒都不许有。
    #[test]
    fn the_files_that_forbid_polling_really_have_none() {
        let root = repo_root();
        for (f, why) in NO_PERIODIC_WAKE {
            let raw = fs::read_to_string(root.join(f))
                .unwrap_or_else(|e| panic!("{f} 读不到：{e} —— 文件搬了就把这条一起改"));
            assert!(raw.len() > 500, "{f} 只有 {} 字节，像是抽错了", raw.len());
            let prod = strip_line_comments(&raw);
            let hits: Vec<&str> = prod
                .lines()
                .filter(|l| is_periodic(l, false))
                .map(|l| l.trim())
                .collect();
            assert!(
                hits.is_empty(),
                "`{f}` 的纪律是「不许有周期唤醒」（{why}），却出现了：{hits:?}"
            );
        }
    }
}
