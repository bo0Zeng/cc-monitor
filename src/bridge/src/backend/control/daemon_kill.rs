//! F04b：**daemon `kill` 的 monitor 侧发送端** —— 「控制搬进 daemon」的第二条通道。
//!
//! # 它在定框里的位置
//!
//! C5 逐字写着「任何**改状态**的 tmux 命令一律归 `control/`」，C6 写着
//! 「**先搬 Gate 2，再切 kill / send-keys —— 顺序不可反**」。
//! F03 搬了 Gate 2、F04a 搬了 Gate 3 + daemon 侧的 `control/kill.rs`，
//! **本模块是那条顺序的最后一步**：让 monitor 真的走过去。
//!
//! # 切过去换来的是什么（不是「架构更整齐」这种空话）
//!
//! 今天 `tmux.rs::kill_remote_tmux` 拼一条穿过 ssh + shell 的原子命令，最后
//! `tmux kill-session -t '=name:'` —— **对名字下手**。daemon 侧那条
//! （`control/kill.rs`）先 `admit_destructive` 拿到 `#{session_id}` 句柄，
//! 再 `kill-session -t '$3'` —— **对句柄下手**。
//! tmux 的 `$N` 在 server 生命周期内唯一且不复用 ⇒ 名字在探测与执行之间被重新绑定
//! 也杀不到别人身上。**破坏性动作尤其不能对名字下手**（`control/gate` 头注那段 TOCTOU 分析）。
//! ⇒ 切路由本身就是**安全性的净改善**，不只是搬家。
//!
//! # ★★ 三态而不是两态：为什么「过门被拒」不许回落
//!
//! C7 允许过渡期的回落，但回落有一个**危险的错法**：把 daemon 的一次**拒绝**
//! （`wrong_owner` / `too_many_windows`）当成「daemon 不可用」，转头用 SSH 那条路再杀一次。
//! 那等于**把一次被门拒绝洗成另一条路的成功** —— 今天两条路的门恰好等价，所以看不出问题；
//! 哪天有一侧漂了，这就是一个静默的权限旁路。
//!
//! ⚠ **分流规则本身不在这里** —— F04c 把它搬进了 [`super::daemon_route`]，
//! 与 `send-keys` 那条命令共用**一份**（两份必漂，而漂开的后果就是上面那条）。
//! 本模块只负责「拒绝该怎么对用户说」。

use std::time::Duration;

use super::daemon_route::{no_channel, route_call_error, Routed};

/// 一次 `kill` 的往返上限。同 `daemon_launch::CALL_TIMEOUT_SECS` 的理由：
/// §41「零定时器」管的是 daemon 侧不许等，客户端侧的等待本来就归客户端。
const CALL_TIMEOUT_SECS: u64 = 10;

/// 从 daemon 的 `kill` 应答里读出 `killed`。
///
/// 三态都要有说法（同 `daemon_launch::typed_from_reply`）：字段在且是 bool ⇒ 照抄；
/// 字段缺 / 类型不对 / 整个 body 缺 ⇒ **不当成 false**，而是诚实报「应答形状不认识」——
/// 那是协议漂移，不是「没杀成」。⚠ 而且对 kill 尤其重要：形状不认识时我们**不知道它杀没杀**，
/// 所以调用方必须把它当成 `Refused`（不回落），否则就是在未知状态上再做一次破坏性动作。
pub(crate) fn killed_from_reply(reply: Option<&serde_json::Value>) -> Result<bool, String> {
    let Some(v) = reply else {
        return Err("daemon 的 kill 应答没有 body（协议漂移？）".into());
    };
    match v.get("killed") {
        Some(serde_json::Value::Bool(b)) => Ok(*b),
        Some(other) => Err(format!("kill 应答里的 killed 不是 bool：{other}")),
        None => Err(format!("kill 应答里没有 killed 字段：{v}")),
    }
}

/// daemon 的错误码 → 用户看的话。**与今天那条 SSH 路的文案逐条对齐**，
/// 否则同一个拒绝在两条路上说两种话，用户会以为是两个不同的问题。
fn refusal_text(code: &str, message: &str) -> String {
    match code {
        "no_tmux" => "远端未安装 tmux".to_string(),
        "no_such_session" => "远端会话已不存在（可能已被终止）".to_string(),
        "wrong_owner" => format!(
            "拒绝 kill：目标未通过身份守卫（{message}）——可能不是本工具管理的会话\
             （避免误杀你自己的 tmux 会话）"
        ),
        "too_many_windows" => format!(
            "拒绝 kill：目标未通过窗口守卫（{message}）——它已被扩展出额外窗口\
             （请到该 tmux 里自行处理）"
        ),
        _ => format!("远端 kill 失败（{code}）：{message}"),
    }
}

/// **F04b：杀一个远端 tmux 会话（走 daemon `control/kill.rs`）。**
///
/// 不是 `#[tauri::command]` —— 前端**够不着才对**（C9：frontend 只剩开窗）。
/// 唯一调用方是 `tmux.rs::kill_remote_tmux`，它按三态分流。
pub(crate) async fn daemon_kill(origin: &str, name: &str) -> Routed {
    let Some(client) = crate::inbound_client::client_for(origin) else {
        return no_channel(origin);
    };
    let args = serde_json::json!({ "name": name });
    match client
        .call("kill", args, Duration::from_secs(CALL_TIMEOUT_SECS))
        .await
    {
        Ok(reply) => match killed_from_reply(reply.as_ref()) {
            // daemon 只在真杀掉时回 `killed:true`（`kill.rs::kill_for_inbound`）。
            Ok(true) => Routed::Done,
            Ok(false) => Routed::Refused(
                "daemon 回报未杀掉，但也没给错误码 —— 协议漂移，不再用另一条路重试".into(),
            ),
            Err(e) => Routed::Refused(format!(
                "{e} —— ⚠ 应答形状不认识时无法判断它杀没杀，因此不再用另一条路重杀"
            )),
        },
        Err(e) => route_call_error(&e, refusal_text),
    }
}

/// ★ **「怎么算一处 tmux 建会话」只有一个家**〔audit-0805 08-08，E3〕。
///
/// 本文件的创建路径登记表与 `account_usage.rs` 的 D3 例外表问的是同一件事，
/// 而 08-08 实测它们的**发现口径不一样**：这里同时认命令串与 argv 两种形态，
/// 那边只认命令串。于是用 argv 形态（`Command::new("tmux").args(["new-session","-d",…])`）
/// 新建一处会话时，**这里红、那边不红** —— 那边的表可以静默变得不完整。
///
/// ⇒ 与其在两处各写一份近似的口径（那是下一个漂移源），不如让它们问同一个函数。
#[cfg(test)]
pub(crate) mod creation_detect {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn killed_from_reply_reads_the_flag_and_refuses_to_guess() {
        assert_eq!(
            killed_from_reply(Some(&serde_json::json!({ "killed": true }))),
            Ok(true)
        );
        assert_eq!(
            killed_from_reply(Some(&serde_json::json!({ "killed": false }))),
            Ok(false)
        );
        // ★ 缺字段 / 类型不对 / 无 body 一律是**协议漂移**，不是「没杀成」。
        for bad in [
            serde_json::json!({}),
            serde_json::json!({ "killed": "yes" }),
            serde_json::json!({ "session": "x-cc" }),
        ] {
            let r = killed_from_reply(Some(&bad));
            assert!(r.is_err(), "{bad} 应当报协议漂移，而不是被读成 false");
        }
        assert!(killed_from_reply(None).is_err());
    }

    /// 「创建路径」的登记表：`(路径, 判定, 理由)`。
    ///
    /// # ⚠ 发现机制是**遍历**，这张表只用来**表态**
    ///
    /// F04b 建这条判据时**只读了 TS 那一份**（`isValidNewTmuxName`）—— 于是 F12 的
    /// `/full-audit` 逮到 `shared/ccm` 那条创建路径**也允许 `=`**：`--tmux=*` 的取值是
    /// `${1#*=}`（剥到第一个 `=`）⇒ `ccm --tmux=proj=x` 建得出 `proj=x`，通道 B 还给它写真
    /// `@ccm_sid` ⇒ Gate 2 通过、正常出现在列表里，而「结束会话」永远 `invalid_args`
    /// ⇒ **那个会话在 UI 上杀不掉**。（改之前实测：`ccm new --tmux=proj=x --print` 产的就是
    /// `new-session -d -s 'proj=x'`。）
    ///
    /// ⇒ F15 把发现机制换成**遍历**：扫全仓生产段里真正产 `tmux new-session` 的文件
    /// （摸底实测 **4 个**；收窄前 `new-session` 这个词还会命中测试夹具与 UI 动作 id ——
    /// **扫描面画大了会被噪音填满，与画小了一样失去意义**）。
    /// 每个都必须在下表里表态：要么**自己校验**禁字集，要么**名字来自已校验的上游**并说清是谁。
    #[cfg(test)]
    const CREATION_PATHS: &[(&str, CreationVerdict, &str)] = &[
        // 🔴 〔`K-R48` 第二拍 09-11〕原来这里第一条是 `shared/ccm`（bash 的 `case` 校验，
        //    `F15` 给它加的 `=`）。〔用@09-11 `K33`〕那个脚本删了 ⇒ **这条路没有第二个实现了**，
        //    它的原生副本就是下面那条 `control/ccm/plan.rs`。表从 5 条回到 4 条。
        (
            "src/backend/control/launch.rs",
            CreationVerdict::UpstreamValidated,
            "名字来自入方向 `parse_request`，它自己就拒 `:`/`=`（那正是本判据的字符集来源）",
        ),
        (
            // 〔`K-R48` 09-11〕**`ccm` 那条创建路径今天唯一的实现**：`ccm` 变成后端二进制
            // 自己的命令之后，`--print` 吐的那条 tmux 编排与真跑读的是同一个 `Plan`。
            //（第一拍它与 `shared/ccm` 并存、表是 5 条；第二拍脚本删了，回到 4 条。）
            "src/backend/control/ccm/plan.rs",
            CreationVerdict::ValidatesItself,
            "显式 `--tmux=<名>` / `--tmux-base=<基名>` 两条都先过 `validate_tmux_name`，\
             它逐字拒 `* ? . : =` 与控制字符（禁字集自 `shared/ccm` 那条 bash `case` 逐字承接）；\
             派生名那条走 `derive_tmux_name`，它的字符集只放行 `[A-Za-z0-9_-]`，\
             **构造上产不出禁字**。三条入口都由              `control::ccm::plan::tests::a_session_name_that_would_confuse_tmux_is_refused` 钉住",
        ),
        // 🔴 **`K-R104`（09-13）：`src/bridge/src/account_usage.rs` 这一行删了。**
        //    它原来的理由是「探针会话名是 `ccm-usage-<slug>`……且它自己 `kill-session` 收尾」。
        //    今天那两句都不成立了：编排搬上 daemon 帧面之后，**monitor 不再建任何 tmux 会话**
        //    （会话由 `oneshot-session` 原语铸并建，收尾发帧面的 `kill`）。
        //    ⇒ 它不再是一个「创建路径」⇒ 留着就是幽灵条目，而本表的遍历会当场逮住。
        //    ★ 同 `K-R72` 那次逐字：这一改是**结构性强制的随动**，不是顺手删记录。
        (
            "src/session-backend.ts",
            CreationVerdict::UpstreamValidated,
            "它只是**渲染器**：名字由上游 `mintTmuxName` 产、由 `src/shell-quote.ts::isValidNewTmuxName` 校验（见 `VALIDATORS`）",
        ),
    ];

    /// **校验器**登记表：`(路径, 它是谁)`。
    ///
    /// # ⚠ 为什么是两张表
    ///
    /// 第一版我把 `src/shell-quote.ts` 塞进了 `CREATION_PATHS` —— 而**它不产 `new-session`**，
    /// 它是**校验器**。判据当场红（遍历只找到 4 个产出方，登记表却有 5 条）。
    /// ⇒ 两张表各司其职：
    ///
    /// - `CREATION_PATHS`：**谁在创建**（发现机制 = 遍历 `tmux new-session`）；
    /// - `VALIDATORS`：**谁在校验**（这些文件必须真的拒 daemon 拒的每个字符）。
    ///
    /// ★ 一般化：**「一张表混装两种角色」是它自己会红的那种错** ——
    /// 因为两种角色的**发现机制不同**（一个能遍历，一个不能），混在一张表里必然对不上。
    /// `(路径, **禁字集表达式的字面量**, 它是谁)`。
    ///
    /// # ⚠ 第二列不是装饰 —— 没有它这条判据是恒真的
    ///
    /// 第一版我写的是 `src.contains('=')`（整个文件里有没有那个字符）。
    /// **变异 P1（把 `=` 从 `shared/ccm` 的禁字集里拿掉）当场存活** ——
    /// 因为一个 shell 脚本里到处都是 `=`（变量赋值、`--tmux=*`…）⇒ 那个断言**恒真**。
    ///
    /// ★ 「判据自己会不会错」那一问的教科书形态：**它匹配到了别处**。
    /// ⇒ 改成钉**禁字集表达式本身**：字面量必须逐字出现在文件里，且它必须含 daemon 拒的每个字符。
    /// 两个方向都活：拿掉 `=` ⇒ 字面量不再出现 ⇒ 红；daemon 新增禁字 ⇒ 字面量缺它 ⇒ 红。
    #[cfg(test)]
    const VALIDATORS: &[(&str, &str, &str)] = &[
        (
            "src/shell-quote.ts",
            "[*?=]",
            "`isValidNewTmuxName` 的 glob/目标语法禁字集（F04b 给它加的 `=`）；\
             `:` 由它调的 `isValidTmuxName` 那条字符类禁，本判据单独查",
        ),
        // 🔴 〔`K-R48` 第二拍 09-11〕原来这里有一条 `shared/ccm` 的 `*[*?.:=]*)`
        //    （bash `case` 校验，`F15` 给它加的 `=`）。脚本删了 ⇒ 只剩下面那条原生的。
        (
            // 〔`K-R48` 09-11〕原 `shared/ccm` 那条校验的**原生副本**：同一串禁字，换了语言。
            "src/backend/control/ccm/plan.rs",
            "\"*?.:=\"",
            "`validate_tmux_name` 的禁字集 —— 与上面那条 bash `case` **逐字同一串字符**，\
             刻意写成一个字符串字面量而不是 `matches!(c, '*' | '?' | …)`，\
             就是为了让本判据的第二列钉得住它（钉表达式本身、不钉「文件里有没有那个字符」）",
        ),
    ];

    #[cfg(test)]
    #[derive(PartialEq, Eq, Debug)]
    enum CreationVerdict {
        /// 这条路径**自己**校验禁字集。
        ValidatesItself,
        /// 名字来自已校验的上游 ⇒ 本路径不必再校验，但**必须说清上游是谁**。
        UpstreamValidated,
    }

    /// ★★ **创建路径不许铸出主路杀不掉的名字** —— 发现机制是遍历，不是手写清单。
    #[test]
    fn no_creation_path_can_mint_a_name_the_main_path_cannot_kill() {
        let root = crate::guard_support::repo_root();

        // ── ① 反向锚点：daemon 那条形状门还在（它没了本判据就在空转）──────────
        let kill_prod =
            guard_core::production_code(include_str!("../../../../backend/control/kill.rs"));
        let forbidden: Vec<char> = [':', '=']
            .into_iter()
            .filter(|c| kill_prod.contains(&format!("name.contains('{c}')")))
            .collect();
        assert_eq!(
            forbidden,
            vec![':', '='],
            "daemon 的 `parse_name` 不再同时拒 `:` 与 `=` 了 —— 本判据的字符集来源变了，回来重裁"
        );

        // ── ② 遍历：谁在生产段真正产 `tmux new-session` ────────────────────────
        // ⚠ 模式收窄到 `tmux new-session` 与 argv 形态；**只写 `new-session` 会命中
        //   测试夹具与 UI 动作 id**（摸底实测：宽模式 10 个文件，收窄后 4 个）。
        let verb = format!("new-{}", "session");
        let wide = format!("tmux {verb}");
        let argv = format!("\"{verb}\", \"-d\"");
        let mut found: Vec<String> = Vec::new();
        let mut scanned = 0usize;
        let mut stack: Vec<std::path::PathBuf> =
            // 〔搬树 2026-09-17〕**这里没有 `"src/backend"`，不是漏了**：后端树搬到
            // `<repo>/src/backend` 之后它已经是 `"src"` 的**子目录**，两个都列会把
            // 后端的每个文件数两遍（搬家前 `src/backend` 与 `src` 是互斥的）。
            // 〔搬 src-tauri 2026-09-17〕**这里没有 `"src/bridge/src"`，不是漏了**：
            // 它已经是 `"src"` 的子目录，两个都列会把每个文件数两遍。
            ["src", "shared"]
                .iter()
                .map(|d| root.join(d))
                .collect();
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if name.contains(".test.") || name.contains(".vitest.") {
                    continue;
                }
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
                if !matches!(ext, "rs" | "ts" | "sh" | "") {
                    continue;
                }
                let Ok(raw) = std::fs::read_to_string(&p) else {
                    continue;
                };
                scanned += 1;
                let body = if ext == "rs" {
                    guard_core::production_code(&raw)
                } else {
                    raw
                };
                let hit = super::creation_detect::creates_a_session(&body);
                if hit {
                    found.push(
                        p.strip_prefix(&root)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        found.sort();
        // ★ 抽取器自检：遍历坏了下面几条会零命中地绿。
        assert!(
            scanned >= 300,
            "只扫到 {scanned} 个文件 —— 遍历坏了（四个根目录实测远超 300）"
        );
        let mut registered: Vec<String> = CREATION_PATHS
            .iter()
            .map(|(f, _, _)| (*f).to_string())
            .collect();
        registered.sort();
        // ★★ **「少一处」有两种成因，而它们的处置相反** 〔audit-0805 08-07〕。
        //
        // 发现口径是 `"new-session", "-d"` 这个 argv 形态 —— 它把 `-d` 这个**可选旗标**
        // 当成了识别特征。08-07 实测：把 `launch.rs` 的 `-d` 去掉，本条当场红，
        // 而诊断说的是「那条路没了 ⇒ 删登记」。**照它做就是把一条真实创建路径移出人群**，
        // 于是判据回绿、盲区永久化 —— 一个讲错成因的红灯，比不红更坏。
        // ⇒ 登记在册却掉出人群时，先看它**是不是还在产 new-session**，再给处置。
        let verb_only = registered
            .iter()
            .filter(|r| !found.contains(r))
            .filter(|r| {
                // ⚠ **必须与发现口径读同一段源码**：第一版读整份文件，于是测试夹具里的
                // `new-session` 让「已经改名、真的不再创建」的文件被判成「还在产」——
                // 一条讲错成因的诊断，被它自己的变异当场逮出来（08-07）。
                std::fs::read_to_string(root.join(r))
                    .map(|s| {
                        let body = if r.ends_with(".rs") {
                            guard_core::production_code(&s)
                        } else {
                            s
                        };
                        body.contains(&verb)
                    })
                    .unwrap_or(false)
            })
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            verb_only.is_empty(),
            "\n这些文件**还在产 `{verb}`，只是不再匹配发现口径** `{argv}`：{verb_only:?}\n\
             ⚠ **别删登记** —— 那条路还在。多半是 argv 形态被改了（例如 `-d` 被去掉，\n\
             而 `-d` 正是「后台建会话」本身：没有它 tmux 会去 attach 当前终端，\n\
             daemon 那条路上根本没有终端）。\n\
             先确认那个改动是不是本意；是本意就同步改这里的发现口径，不是就改回去。"
        );
        assert_eq!(
            found, registered,
            "\n真正产 `tmux new-session` 的文件与创建路径登记表对不上。\n\
             **多一处** = 新增了一条创建路径没表态 ⇒ 要么让它自己校验禁字集，\n\
             要么写清「名字来自哪个已校验的上游」。\n\
             **少一处** = 那条路没了 ⇒ 删登记。⚠ 但先读上面那条：**还在产 `{verb}` 的**\n\
             属于「形态变了」不是「路没了」，处置相反。\n\
             ⚠ F04b 那版是**手写两个文件名**，于是 `shared/ccm` 整条路径逃出了扫描面（F12 逮到）。"
        );

        // ── ③ 每条创建路径都要有非空理由；自己校验的那条必须在 `VALIDATORS` 里 ──
        for (f, verdict, why) in CREATION_PATHS {
            assert!(!why.trim().is_empty(), "{f} 的理由是空的");
            if *verdict == CreationVerdict::ValidatesItself {
                assert!(
                    VALIDATORS.iter().any(|(v, _, _)| v == f),
                    "`{f}` 记成「自己校验」，却不在 `VALIDATORS` 表里 —— \
                     那下面那条「真的拒了那些字符」就不会查它"
                );
            }
        }

        // ── ③b 🔴 **反方向**：每条校验器都得有一条创建路径指着它 ────────────
        //
        // 〔`K-R105` 09-13，`KR105D4`〕**删创建路径要连它的校验器一起看。**
        // 上面 ③ 走的是「创建路径 → 校验器」那一向：记成「自己校验」就必须在下表里。
        // **反过来没人查** —— 而这两张表的**发现机制不对称**正是漏洞所在：
        // `CREATION_PATHS` 是遍历出来的（少一条会红），`VALIDATORS` 是**手写**的，
        // 少一条不红、**多一条也不红**。
        //
        // ⇒ 活体形状（现打就摆在盘上）：`src/shell-quote.ts` 这一行今天靠
        // `src/session-backend.ts` 那条创建路径的理由把它引进来。哪天
        // `session-backend.ts` 真被删掉（`U8c-3` 的正题），`CREATION_PATHS` 少一行、
        // 遍历那条断言照样绿，而 `VALIDATORS` 里 `shell-quote.ts` 这一行
        // **变成一条守着「没有任何创建路径在用的校验器」的判据** ——
        // 它仍然会逐字检查那个禁字集，仍然全绿，**而它守的东西已经不在人群里了**。
        // 那不是红，是**一格静默的空真**：正是本文件头注那句「一张表混装两种角色」的反面
        //（两张表分了角色，却没人看着它们的**边**）。
        //
        // 量法：一条校验器合法，当且仅当 —— 它**自己就是**一条创建路径（`ValidatesItself`），
        // 或者**某条创建路径的理由里点了它的名**（`UpstreamValidated` 那一族）。
        // ⚠ 「点名」= 那条理由里出现它的仓相对路径。这要求写理由的人把住址写全，
        //   而那本来就是 ③ 那句「必须说清上游是谁」的字面要求。
        for (v, _, who) in VALIDATORS {
            let is_path = CREATION_PATHS.iter().any(|(f, ..)| f == v);
            let named_by: Vec<&str> = CREATION_PATHS
                .iter()
                .filter(|(_, _, why)| why.contains(*v))
                .map(|(f, ..)| *f)
                .collect();
            assert!(
                is_path || !named_by.is_empty(),
                "\n校验器 `{v}` **今天没有任何创建路径指着它**（它是谁：{who}）。\n\
                 ⇒ 这一行还会跑、还会绿，而它守的那条路已经不在 `CREATION_PATHS` 里了 ——\n\
                 **一条守着不存在的人群的判据，比没有判据更坏**（它让人以为这一格有人看着）。\n\
                 · 是**刚删掉一条创建路径**？那就同一拍把这条校验器一起处置：\n\
                   还有别人在用 ⇒ 把那个用它的创建路径的理由写全（点它的名）；\n\
                   没人用了 ⇒ 连这一行一起撤，并回 `INVARIANTS §33b` / `U8c-3` 说明。\n\
                 · 是**新加了一条校验器**？那它守的创建路径是哪一条 —— 先把那条登记上。\n\
                 ⚠ 本条**不**替你判「该留还是该删」，它只保证那个决定**必须被做一次**。"
            );
        }

        // ── ④ 校验器必须真的拒 daemon 拒的每个字符 ───────────────────────────
        assert!(
            !VALIDATORS.is_empty(),
            "校验器表空了 —— 下面这段会零命中地绿"
        );
        for (f, class_expr, who) in VALIDATORS {
            assert!(!who.trim().is_empty(), "{f} 没说它是谁");
            let src = std::fs::read_to_string(root.join(f))
                .unwrap_or_else(|_| panic!("读不到 {f} —— 读不到的文件只会静默返回空串"));
            // ★ 钉**禁字集表达式本身**，不是「文件里有没有那个字符」——
            //   后者对 shell 脚本恒真（变异 P1 当场存活，见 `VALIDATORS` 头注）。
            assert!(
                src.contains(class_expr),
                "校验器 `{f}` 里找不到禁字集表达式 `{class_expr}` ——\n\
                 要么它被改了（那就同时改这张表，并想清新表达式还拒不拒 {forbidden:?}），\n\
                 要么**某个禁字被拿掉了** ⇒ 这条路径能铸出一个**建得出来、主路杀不掉**的名字\n\
                 （daemon 的 kill 形状门拒它，且**按设计不回落**）。"
            );
            for c in &forbidden {
                // `:` 在 TS 那条由 `isValidTmuxName` 的另一条正则禁（不在本表达式里）⇒ 单独查。
                if *c == ':' && *f == "src/shell-quote.ts" {
                    assert!(
                        src.contains("[.:"),
                        "`{f}` 里找不到 `isValidTmuxName` 那条禁 `.`/`:` 的字符类"
                    );
                    continue;
                }
                assert!(
                    class_expr.contains(*c),
                    "`{f}` 的禁字集表达式 `{class_expr}` 里没有 `{c}` —— \
                     daemon 的 kill 形状门拒它，而这条创建路径放它进来"
                );
            }
        }
    }

    /// ★ **前提触发器：耐久文档里那句「过渡期回落」不许比代码活得久。**
    ///
    /// # 为什么专门给一句文档配一条判据
    ///
    /// F07 顺出的一般化：**「状态列」与「实测答案」是耐久文档里最易腐的两种字段** ——
    /// 它们描述**当下**，而文档寿命比「当下」长。F04b 自己就撞到四处：
    /// `IPC-PROTOCOL` 说 kill 的 shell 路是主路（已降为回落）·
    /// `INVARIANTS §A5` 说 kill「无此白名单」（**自 F04 起就假了**）·
    /// `INVARIANTS §34` 说三道门住 `tmux.rs`（主路那份已在 daemon）·
    /// 用量方案文档说 kill「daemon 不参与」。
    ///
    /// 处置不是「以后记得更新」，是**配一条触发器**：本条把那句话与
    /// 「回落这段代码到底还在不在」绑在一起。F11 删回落时它会主动红，
    /// 逼人回来把那句话一起改掉。
    ///
    /// 🔴 **`K-R72`（09-12）：它真的响了，而且响得对。**
    /// 那一刀删掉 `kill_remote_tmux` 的一次性 SSH 回落，本条**当场红**，
    /// 逼着把 `src/doc/IPC-PROTOCOL.md` 那两处「过渡期」的说法一起改成「已删」。
    /// ⇒ 今天两侧都是 `false`：代码里没有回落，文档里也不再说有。
    /// **本条不因此作废** —— 它两个方向都咬：谁把回落加回来不改文档、
    /// 或谁把那句话写回文档而代码里没有，都会红。
    ///
    /// # 🔴🔴 `K-R106`（09-13）`KR106D3`：**人群从一份文档扩到整棵 `doc/`**
    ///
    /// 本条此前只 `include_str!` **一份** `src/doc/IPC-PROTOCOL.md` ——
    /// 而同一句话当时在盘上还有**另外三份副本**，它们**结构上够不着**：
    /// `src/doc/CONTRIBUTING.md`（正文 ＋ 同节表格两处）· `src/doc/ARCHITECTURE.md` ·
    /// `doc/账号用量-usage抓取方案.md`。`K-R72` 那次「响得对」只响到了它看得见的那一份，
    /// 于是它逼人改的也只有那一份 —— **一条判据挡住的，只有它人群里的那些**。
    ///
    /// ⇒ 发现机制从**一个 `include_str!`** 换成**遍历 `doc/`**（同本文件
    /// `CREATION_PATHS` 那条的做法：人群靠遍历发现，不靠手写清单）。
    /// ⚠ 这是**扩扫描面 = 更严**，不是放宽闸：两个方向都还咬，只是够得着的人多了。
    ///
    /// # ⚠ 诚实边界（两侧都写出来，别读大）
    ///
    /// - **人群是 `doc/` 这棵树**，按「耐久文档的家」这条语义划，不是「碰巧只有它们长这样」。
    ///   仓根那几份 `.md`（`README*` / `CHANGELOG` / 复盘报告）与 `evidence/` **不在人群里**：
    ///   前者不是耐久设计文档；后者是**死值验留档**，逐字记着历史上那一刀砍的是什么，
    ///   它**本来就该**提到那句话（现打 09-13：`tests/evidence/K-R72-deathvalue.md` 正是这一形）。
    ///   ⇒ 把它们扫进来买到的不是更严，是一条必然误报的闸。
    /// - **它按整串 `contains` 判** ⇒ 想在耐久文档里给这句话立一块**墓碑**（「历史上有过、
    ///   已经删了」）就会被它拦下。今天的出路是**换一种说法**（本轮三份副本都是这么改的）。
    ///   这是它已知的代价，不是没看见。
    /// - **它不判那三份副本说得对不对** —— 只判「那句话在不在」与「代码里那条路在不在」一致。
    #[test]
    fn the_doc_sentence_about_the_transitional_fallback_cannot_outlive_the_code() {
        let tmux_rs = guard_core::production_code(include_str!("../../tmux.rs"));
        let at = tmux_rs
            .find("pub async fn kill_remote_tmux(")
            .expect("找不到 kill 命令 —— 签名变了就把本条一起改");
        let body = &tmux_rs[at..];
        let end = body.find("\n}\n").map(|k| k + 3).unwrap_or(body.len());
        let fallback_alive = body[..end].contains("connect_and_exec_cmd");

        // ── 人群：遍历 `doc/`（递归），**不是**一张手写清单 ────────────────
        let root = crate::guard_support::repo_root();
        let needle = format!("过渡期{}", "回落");
        let mut scanned: Vec<String> = Vec::new();
        let mut said: Vec<String> = Vec::new();
        let mut stack = vec![root.join("src/doc")];
        while let Some(d) = stack.pop() {
            let rd = std::fs::read_dir(&d).unwrap_or_else(|e| {
                panic!("读不到 {} —— 人群空了本条会零命中地绿：{e}", d.display())
            });
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("md") {
                    continue;
                }
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                let Ok(text) = std::fs::read_to_string(&p) else {
                    panic!("{rel} 读不出来 —— 读不到的文件只会静默返回空串");
                };
                scanned.push(rel.clone());
                if text.contains(needle.as_str()) {
                    said.push(rel);
                }
            }
        }
        // ★ 抽取器自检：人群塌成 0 时，下面那条相等断言会**空真地**绿。
        assert!(
            scanned.len() >= 8,
            "只扫到 {} 份耐久文档（`src/doc/**/*.md`）—— 遍历坏了，本条此刻在空转：{scanned:?}",
            scanned.len()
        );
        // ★ 地板的第二半：人群里必须**真的有**那份 `K-R72` 逼着改过的文档，
        //   否则「扫到 8 份」也可能扫的是另外八份。
        assert!(
            scanned.iter().any(|f| f.ends_with("IPC-PROTOCOL.md")),
            "人群里没有 `src/doc/IPC-PROTOCOL.md` —— 本条原来唯一看得见的那一份掉出去了：{scanned:?}"
        );
        said.sort();
        assert_eq!(
            !said.is_empty(),
            fallback_alive,
            "代码与耐久文档对不上了：\n\
             · `kill_remote_tmux` 里还有一次性 SSH 的第二条路吗 = {fallback_alive}\n\
             · `doc/` 里还写着那句话的（分母 = 遍历到的 {} 份 `.md`）= {said:?}\n\
             ⚠ 如果是**删掉了那条路**：那句话要一起改，否则下一个读者会以为\n\
             「没有后端的远端」还有一条路可走 —— 而那正是 C7 说的过渡期已经结束。\n\
             ⚠ 如果是**改了措辞**：本条判据跟着改（它钉的是两者一致，不是某个字面量）。\n\
             ⚠ 〔`K-R106` 09-13〕人群是**整棵 `doc/`**，不再只有 `IPC-PROTOCOL.md` ——\n\
             在**任何一份**耐久文档里把那句话写回来，本条都会红。",
            scanned.len()
        );
    }

    /// 两条路的拒绝文案必须说同一件事 —— 同一个拒绝在两条路上说两种话，
    /// 用户会以为是两个不同的问题。
    ///
    /// # `K-R72`（09-12）：**换了对照面，不是删掉判据**
    ///
    /// 本条原名 `the_refusal_wording_matches_the_ssh_path`，反向锚点断的是  〔散文墓碑〕
    /// 「monitor 侧那条一次性 SSH 回落里这几句话还在」。**那条 SSH 路整块删了** ⇒
    /// 「两条路」若还指它就是一句假话，而它守的性质（**同一个拒绝只许有一种说法**）没有消失：
    /// 今天用户碰得到的两条路是 **kill 与 send-keys 这两条后端命令**，
    /// 各有一份 `refusal_text` ⇒ 对照面换成兄弟命令那一份。
    ///
    /// ⚠ 顺带收紧了一格：对照面过 [`guard_core::production_code`]，
    /// **兄弟文件自己的测试里抄一份同样的串糊弄不过去**（原来那半是整份源码 `contains`）。
    ///
    /// ⚠ `too_many_windows` **不参与对照** —— 它是 kill 独有的一档
    /// （send-keys 不删除任何东西，窗口数与它无关，`admit` / `admit_destructive` 是两个入口）。
    /// 它自己那句「下一步该干什么」单独钉。
    #[test]
    fn the_refusal_wording_matches_the_sibling_command() {
        let sibling = guard_core::production_code(include_str!("daemon_send_keys.rs"));
        for (code, needle) in [
            ("no_tmux", "远端未安装 tmux"),
            ("no_such_session", "远端会话已不存在（可能已被终止）"),
            ("wrong_owner", "可能不是本工具管理的会话"),
        ] {
            let mine = refusal_text(code, "m");
            assert!(
                mine.contains(needle),
                "`{code}` 的文案里没有 {needle:?}：{mine}"
            );
            assert!(
                sibling.contains(needle),
                "`daemon_send_keys.rs` 的生产段里已经没有 {needle:?} 了 —— \
                 两条后端命令的文案漂了，要么一起改，要么本条判据该跟着改"
            );
        }
        assert!(
            refusal_text("too_many_windows", "m").contains("请到该 tmux 里自行处理"),
            "`too_many_windows` 少了「下一步该干什么」那半句 —— \
             它是 kill 独有的一档，没有兄弟命令替它兜"
        );
    }
}
