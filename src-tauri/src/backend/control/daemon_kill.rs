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
        (
            "shared/ccm",
            CreationVerdict::ValidatesItself,
            "显式 `--tmux=<名>` 那条创建路径的 `case` 校验 —— **F15 给它加的 `=`**",
        ),
        (
            "remote-daemon-proto/src/control/launch.rs",
            CreationVerdict::UpstreamValidated,
            "名字来自入方向 `parse_request`，它自己就拒 `:`/`=`（那正是本判据的字符集来源）",
        ),
        (
            "src-tauri/src/account_usage.rs",
            CreationVerdict::UpstreamValidated,
            "探针会话名是 `ccm-usage-<slug>`，`slug` 由账号名 sanitize 而来、**不是用户自由输入**；\
             且它自己 `kill-session` 收尾、不经 daemon 的 kill 主路",
        ),
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
        (
            "shared/ccm",
            "*[*?.:=]*)",
            "显式 `--tmux=<名>` 的 `case` 校验（**F15 给它加的 `=`**）—— 它既创建也自校验，两张表都在",
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
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级");

        // ── ① 反向锚点：daemon 那条形状门还在（它没了本判据就在空转）──────────
        let kill_prod = guard_core::production_code(include_str!(
            "../../../../remote-daemon-proto/src/control/kill.rs"
        ));
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
            ["src-tauri/src", "remote-daemon-proto/src", "src", "shared"]
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
                let hit = body.lines().any(|l| {
                    let t = l.trim_start();
                    !t.starts_with("//")
                        && !t.starts_with('#')
                        && !t.starts_with('*')
                        && (l.contains(wide.as_str()) || l.contains(argv.as_str()))
                });
                if hit {
                    found.push(
                        p.strip_prefix(root)
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
        assert_eq!(
            found, registered,
            "\n真正产 `tmux new-session` 的文件与创建路径登记表对不上。\n\
             **多一处** = 新增了一条创建路径没表态 ⇒ 要么让它自己校验禁字集，\n\
             要么写清「名字来自哪个已校验的上游」。\n\
             **少一处** = 那条路没了 ⇒ 删登记。\n\
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
    #[test]
    fn the_doc_sentence_about_the_transitional_fallback_cannot_outlive_the_code() {
        let tmux_rs = guard_core::production_code(include_str!("../../tmux.rs"));
        let at = tmux_rs
            .find("pub async fn kill_remote_tmux(")
            .expect("找不到 kill 命令 —— 签名变了就把本条一起改");
        let body = &tmux_rs[at..];
        let end = body.find("\n}\n").map(|k| k + 3).unwrap_or(body.len());
        let fallback_alive = body[..end].contains("connect_and_exec_cmd");
        let doc = include_str!("../../../../doc/IPC-PROTOCOL.md");
        let doc_says_transitional = doc.contains("过渡期回落");
        assert_eq!(
            fallback_alive, doc_says_transitional,
            "代码与文档对不上了：\n\
             · `kill_remote_tmux` 里还有一次性 SSH 回落吗 = {fallback_alive}\n\
             · `doc/IPC-PROTOCOL.md` 还写着「过渡期回落」吗 = {doc_says_transitional}\n\
             ⚠ 如果是**删掉了回落**（F11 的活）：那句话要一起改，否则下一个读者会以为\n\
             「没有 daemon 的远端」还有一条路可走 —— 而那正是 C7 说的过渡期已经结束。\n\
             ⚠ 如果是**改了文档措辞**：本条判据跟着改（它钉的是两者一致，不是某个字面量）。"
        );
    }

    /// 两条路的拒绝文案必须说同一件事 —— 同一个拒绝在两条路上说两种话，
    /// 用户会以为是两个不同的问题。
    #[test]
    fn the_refusal_wording_matches_the_ssh_path() {
        let ssh = include_str!("../../tmux.rs");
        for (code, needle) in [
            ("no_tmux", "远端未安装 tmux"),
            ("no_such_session", "远端会话已不存在（可能已被终止）"),
            ("wrong_owner", "可能不是本工具管理的会话"),
            ("too_many_windows", "请到该 tmux 里自行处理"),
        ] {
            let mine = refusal_text(code, "m");
            assert!(
                mine.contains(needle),
                "`{code}` 的文案里没有 {needle:?}：{mine}"
            );
            assert!(
                ssh.contains(needle),
                "SSH 那条路里已经没有 {needle:?} 了 —— 两条路的文案漂了，\
                 要么一起改，要么本条判据该跟着改"
            );
        }
    }
}
