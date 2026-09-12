//! ② **问它会什么** —— 能力协商。
//!
//! # 这一段是**新造**的，不是抽出来的
//!
//! 反推的两个真实实现里，宿主侧只有**一处**在做这件事（另一处一行都没有）。
//! ⇒ 与 ①③④ 不同：那三段是把已有形状收上来，这一段是给宿主侧**加**一件今天没有的东西。
//! 两者的风险不同，写在这里免得下一个人以为它有同样的实测底子。
//!
//! # 形状照仓里唯一在说这套方言的那个插件（`E7`：由现有能力反推，不发明第二套）
//!
//! 探测输出是**一行一个 `key=value`**：
//!
//! | key | 干什么用 | 谁在管 |
//! |---|---|---|
//! | `name` | ★ **身份行，必须是第一行、必须逐字对上** | 防 `PATH` 上同名的无关程序被当成插件 |
//! | `version` | **只进诊断文案**，不参与「能不能用」的判断 | `E7` 逐字排除了比版本号大小 |
//! | `capabilities` | 逗号列表，**集合语义**，做**子集检查** | 加 token 安全，删/改名才危险 |
//! | 其余 | 该插件自己的域枚举（它支持哪些东西） | 插件自己定，本层原样带回 |
//!
//! # ★★ 为什么不比版本号
//!
//! 集成方按**能力**兼容，不按版本号 —— 版本号是一条一维的线，而「装的这份会不会做 X」
//! 是一张集合。三个真实消费者今天全是**逐 token 子集检查 + 缺谁报谁**，一个都不比大小。
//!
//! # ★★ 「缺能力」必须说得出**缺哪一个**
//!
//! 这是本段存在的全部理由。原形逐字记着不检能力的后果：
//! **「版本太旧」会被说成「建会话失败」** —— 调用方于是去查一个根本不存在的故障。
//! ⇒ [`Rejected::message`] 里放的是**那一个 token 的名字**，不是「缺能力」这四个字，
//! 也不是把整张必需清单都打出来（那等于没说）。
//!
//! # 诚实边界
//!
//! - 本模块今天**零生产调用方**（见 [`super`] 的头注）。判据钉的是机制。
//! - 判据的对拍语料读的是那个插件的**源码**（`include_str!`）⇒ 它挡得住「改源码」，
//!   **挡不住**「同名的另一份装在 `PATH` 上」。真跑那条命令的判据住 e2e，
//!   而出货门禁一套 e2e 都不跑（`KY7`）。这一档由谁跑、什么时候跑，写在件文件里。

/// 插件对「你会什么」这一问的回答。
pub(crate) struct Answer {
    /// 身份行的值（已经与调用方期望的名字对上过）。
    pub(crate) name: String,
    /// 只进诊断文案。**不参与判断**。
    pub(crate) version: Option<String>,
    /// 能力 token 集合。
    pub(crate) capabilities: Vec<String>,
    /// 该插件自己的其它键（域枚举之类），原样带回，本层不解释。
    pub(crate) extras: Vec<(String, String)>,
}

impl Answer {
    /// 会不会做这一件事。**子集检查，不比版本号。**
    pub(crate) fn can(&self, token: &str) -> bool {
        self.capabilities.iter().any(|c| c == token)
    }
}

/// 协商没过 —— 两种，**刻意分开**。
pub(crate) enum Rejected {
    /// 首行的身份对不上：找到的那个程序不是我们要的插件。
    NotThePlugin { want: String, saw: String },
    /// 装的这份**缺一个**必需能力。★ 名字在这里，诊断文案要用它。
    MissingCapability {
        plugin: String,
        token: String,
        version: Option<String>,
    },
}

impl Rejected {
    /// 给调用方看的那句话。
    ///
    /// ⚠ 「缺能力」那一支里**只提缺的那一个 token** —— 把整张必需清单打出来等于没说，
    /// 人还得自己去比对哪个不在。
    pub(crate) fn message(&self) -> String {
        match self {
            Rejected::NotThePlugin { want, saw } => format!(
                "找到的这个程序不是 `{want}`：它的身份行报的是 `{saw}`。\
                 ⚠ 这不是「它太旧」，是**找错了程序** —— 多半是 PATH 上有个同名的无关命令。"
            ),
            Rejected::MissingCapability {
                plugin,
                token,
                version,
            } => format!(
                "`{plugin}` 装的这份缺能力 `{token}`（它自己报的版本：{}）。\
                 ⚠ 这**不是**「调用失败」—— 那个程序好好的，是它这一版还不会做这一件事，\
                 换一份新的就行。",
                version.as_deref().unwrap_or("<没报>")
            ),
        }
    }
}

/// 把探测输出切成 `key=value`。
///
/// 首行必须逐字是 `name=<期望的名字>`，否则**当场拒**（身份行的全部用途）。
pub(crate) fn parse(text: &str, want_name: &str) -> Result<Answer, Rejected> {
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut capabilities: Vec<String> = Vec::new();
    let mut extras: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            // 不成对的行**不是**协商内容；身份行还没确认之前一律拒。
            if name.is_none() {
                return Err(Rejected::NotThePlugin {
                    want: want_name.to_string(),
                    saw: line.to_string(),
                });
            }
            continue;
        };
        if name.is_none() {
            if k != "name" || v != want_name {
                return Err(Rejected::NotThePlugin {
                    want: want_name.to_string(),
                    saw: line.to_string(),
                });
            }
            name = Some(v.to_string());
            continue;
        }
        match k {
            "version" => version = Some(v.to_string()),
            "capabilities" => {
                capabilities = v
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            }
            _ => extras.push((k.to_string(), v.to_string())),
        }
    }
    match name {
        Some(name) => Ok(Answer {
            name,
            version,
            capabilities,
            extras,
        }),
        None => Err(Rejected::NotThePlugin {
            want: want_name.to_string(),
            saw: "<空的探测输出>".to_string(),
        }),
    }
}

/// 宿主把**「我要哪些 token」**写成一份显式清单，逐个查。
///
/// ⚠ 不许写成「探测成功就当能用」—— 三个真实消费者全是这个形状，一个都不是那样。
/// 报的是**第一个**缺的那一个：一次说一件事，人才修得动。
pub(crate) fn require(answer: &Answer, required: &[&str]) -> Result<(), Rejected> {
    for token in required {
        if !answer.can(token) {
            return Err(Rejected::MissingCapability {
                plugin: answer.name.clone(),
                token: (*token).to_string(),
                version: answer.version.clone(),
            });
        }
    }
    Ok(())
}

/// 一步走完：认身份 → 读能力 → 逐个查必需清单。
pub(crate) fn negotiate(
    text: &str,
    want_name: &str,
    required: &[&str],
) -> Result<Answer, Rejected> {
    let answer = parse(text, want_name)?;
    require(&answer, required)?;
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 仓里唯一一个真在说这套方言的插件 —— **`K-R48` 第二拍起它是本二进制自己**。
    ///
    /// # 语料换了，而且换得更硬
    ///
    /// 从前这里是一句 `include_str!` 指着那份 bash `ccm` 脚本 ——**读它的源码**，
    /// 再从里面抠 `capabilities=` 那一行。头注当时逐字写着这么做的代价：
    /// 「它挡得住『改源码』，挡不住『PATH 上换了一份』」。
    ///
    /// 〔用@09-11 `K33`〕那个脚本删了。今天说这套方言的是**本二进制的一次性模式**
    /// （`control::ccm::probe_output`）⇒ 语料改成**真调那个产出函数**：
    /// 不再是「读一份源码再抠」，是**拿生产那条路真吐出来的那份**。
    /// ⚠ 仍然挡不住「PATH 上换了一份」（那要真跑一条命令，住 e2e）—— 这一格没有变好也没有变坏。
    fn real_plugin_probe_text() -> String {
        crate::control::ccm::probe_output("/usr/local/bin/ccm")
    }

    /// 那个插件**自己声明**的能力 token（从它真吐出来的那份里抠）。
    ///
    /// ⚠ 锚点必须带**它前面那个换行**：光找 `capabilities=` 会先命中散文里的同一个词。
    /// ⇒ 锚点唯一性也一并断言（这条纪律从 bash 语料那一版逐字保留 —— 它当年真逮到过一次）。
    fn declared_capabilities() -> Vec<String> {
        let src = real_plugin_probe_text();
        let key = format!("\ncapabi{}=", "lities");
        assert_eq!(
            src.matches(&key).count(),
            1,
            "对拍语料里 `{key}` 出现的次数不是 1 —— 锚点不唯一，抠到的可能不是那一行"
        );
        let at = src
            .find(&key)
            .expect("对拍语料里找不到能力声明 —— 抠法坏了，下面几条会空转");
        let tail = &src[at + key.len()..];
        let end = tail.find('\n').unwrap_or(tail.len());
        tail[..end]
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// 那个插件的名字（身份行里的值）。运行时拼，免得判据文本自己成为语料。
    fn plugin_name() -> String {
        format!("cc{}", "m")
    }

    /// **宿主侧的必需清单** —— 抄的是仓里那个真实消费者今天逐个查的那几个 token。
    ///
    /// ⚠ 它不是我编的：那处逐字对这几个做子集检查，缺哪个就把**那个名字**打进 stderr。
    fn required_today() -> Vec<String> {
        vec![
            format!("deta{}", "ch"),
            format!("tmux-{}", "size"),
            format!("tmux-{}", "base"),
            format!("bus-{}", "register"),
        ]
    }

    /// 按那个插件**今天真声明**的能力，合成一份探测输出。
    fn live_probe_text(caps: &[String]) -> String {
        format!(
            "name={}\nversion=3\nself=/opt/x\ncapabilities={}\n",
            plugin_name(),
            caps.join(",")
        )
    }

    /// ★★ **活体对拍**：语料是那个插件源码里今天真写着的那一串，不是手抄的死 fixture。
    ///
    /// 它同时钉两件事：① 抠法没坏（token 数是实测值，多一个少一个都要有人看一眼）；
    /// ② 宿主的必需清单**每一个都在**那串里 —— 清单里冒出一个插件根本没有的 token
    /// （编出来的、或者改名之后没跟）会当场红。
    #[test]
    fn the_required_list_is_checked_against_what_the_real_plugin_declares() {
        let caps = declared_capabilities();
        assert_eq!(
            caps.len(),
            18,
            "对拍语料里的能力 token 从 18 个变成 {}：{caps:?}\n\
             加 token 是好事（消费者全是子集检查）；**删/改名才危险** —— \
             那会让下面那份必需清单里的某一条对不上。这个数变了就顺手看一眼消费者。\n\
             〔`K-R61` 09-11：17 → 18，加的是 `base-url-across-tmux`。\
             **本条是『谁在数它』那张表上的第五处**，而 `K-R61` 派工时那张表只登记了四处 —— \
             已点名交回 PM。〕",
            caps.len()
        );
        for want in required_today() {
            assert!(
                caps.contains(&want),
                "必需清单里的 `{want}` 不在那个插件今天声明的能力里：{caps:?}\n\
                 ⇒ 要么清单抄错了，要么那个 token 被改名/删掉了。"
            );
        }
    }

    /// ② **全断**：token 齐全的那一份必须**通过** —— 防「恒红」那种假判据。
    #[test]
    fn a_complete_answer_passes_negotiation() {
        let caps = declared_capabilities();
        let text = live_probe_text(&caps);
        let owned = required_today();
        let required: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        let answer = negotiate(&text, &plugin_name(), &required)
            .unwrap_or_else(|e| panic!("齐全的探测输出被拒了：{}", e.message()));
        assert_eq!(answer.name, plugin_name());
        assert_eq!(answer.version.as_deref(), Some("3"));
        assert_eq!(answer.capabilities.len(), caps.len());
        assert!(
            answer.extras.iter().any(|(k, _)| k == "self"),
            "插件自己的其它键被吞了：{:?}",
            answer.extras.iter().map(|(k, _)| k).collect::<Vec<_>>()
        );
    }

    /// ★ ① **逐 token 单断**：必需清单里的每一个 token，各造一份「**只少这一个**」的
    /// 探测输出，断言那句话**含那一个名字**、**且不含清单里的其他名字**。
    ///
    /// 后半句是承重的：把整张清单原样打出来也能让「含那一个名字」通过，
    /// 而那种诊断等于没说（人还得自己去比对哪个不在）。
    #[test]
    fn a_missing_capability_is_named_and_only_it_is_named() {
        let all = declared_capabilities();
        let required = required_today();
        let refs: Vec<&str> = required.iter().map(|s| s.as_str()).collect();
        for drop_me in &required {
            let thinned: Vec<String> = all.iter().filter(|c| *c != drop_me).cloned().collect();
            assert_eq!(
                thinned.len(),
                all.len() - 1,
                "夹具没造对：`{drop_me}` 本来就不在那串里"
            );
            let text = live_probe_text(&thinned);
            let err = negotiate(&text, &plugin_name(), &refs)
                .err()
                .unwrap_or_else(|| panic!("少了 `{drop_me}` 却放行了"));
            let msg = err.message();
            assert!(
                msg.contains(drop_me.as_str()),
                "少的是 `{drop_me}`，而那句话里没有它：{msg}"
            );
            for other in &required {
                if other == drop_me {
                    continue;
                }
                assert!(
                    !msg.contains(other.as_str()),
                    "那句话把没缺的 `{other}` 也打出来了 —— 整张清单一起报等于没说：{msg}"
                );
            }
        }
    }

    /// ★ 报的必须是**能力**这件事，不是笼统的「调用失败」。
    ///
    /// 原形逐字记着不检能力的后果：「版本太旧」被说成「建会话失败」，
    /// 于是人去查一个根本不存在的故障。
    #[test]
    fn the_message_does_not_blame_the_call() {
        let all = declared_capabilities();
        let required = required_today();
        let refs: Vec<&str> = required.iter().map(|s| s.as_str()).collect();
        let thinned: Vec<String> = all.iter().filter(|c| **c != required[0]).cloned().collect();
        let err = negotiate(&live_probe_text(&thinned), &plugin_name(), &refs)
            .err()
            .expect("该拒的没拒");
        let msg = err.message();
        assert!(
            msg.contains(&format!("`{}`", required[0])),
            "缺的那个 token 没被引起来单独点名：{msg}"
        );
        assert!(
            msg.contains("不是"),
            "没把「这不是调用失败」说出来，调用方会去查错方向：{msg}"
        );
    }

    /// ★ 身份行：首行对不上就**当场拒**，并说出看到的是什么。
    ///
    /// 用途是防 `PATH` 上同名的无关程序被当成插件 —— 那种程序也可能 exit 0、也可能有输出。
    #[test]
    fn a_stranger_that_happens_to_have_the_same_name_is_rejected() {
        let stranger = "usage: something-else [options]\n";
        let err = parse(stranger, &plugin_name())
            .err()
            .expect("陌生程序没被拒");
        let msg = err.message();
        assert!(msg.contains("something-else"), "没说看到的是什么：{msg}");
        assert!(msg.contains("找错了程序"), "归因说反了：{msg}");

        // 键值成对、但名字不对的那种，也要拒。
        let other = format!(
            "name=some-other-tool\ncapabilities={}\n",
            required_today()[0]
        );
        assert!(
            parse(&other, &plugin_name()).is_err(),
            "名字不对却认了 —— 身份行就白设了"
        );
        // 空输出也是「不是它」。
        assert!(
            parse("", &plugin_name()).is_err(),
            "空输出被当成了合格的回答"
        );
    }

    /// 版本号**不参与判断**：版本再老，只要能力齐就放行。
    #[test]
    fn the_version_number_never_decides_anything() {
        let caps = declared_capabilities();
        let ancient = format!(
            "name={}\nversion=0.0.1-ancient\ncapabilities={}\n",
            plugin_name(),
            caps.join(",")
        );
        let owned = required_today();
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        let answer = negotiate(&ancient, &plugin_name(), &refs)
            .unwrap_or_else(|e| panic!("按版本号把人拒了：{}", e.message()));
        assert_eq!(answer.version.as_deref(), Some("0.0.1-ancient"));
    }

    /// 空的必需清单**不是**「随便什么都行」的借口 —— 它就是「我不要求任何 token」，
    /// 但身份行照样要对上。
    #[test]
    fn an_empty_requirement_list_still_checks_the_identity() {
        let text = live_probe_text(&declared_capabilities());
        assert!(negotiate(&text, &plugin_name(), &[]).is_ok());
        assert!(negotiate(&text, "not-that-plugin", &[]).is_err());
    }

    /// 逗号列表的边角：多余空白、末尾逗号不许变出空 token。
    #[test]
    fn whitespace_and_trailing_commas_do_not_become_tokens() {
        let text = format!("name={}\ncapabilities= a , b ,,c,\n", plugin_name());
        let a = parse(&text, &plugin_name()).ok().expect("该解析得出来");
        assert_eq!(a.capabilities, vec!["a", "b", "c"]);
        assert!(a.can("b"));
        assert!(!a.can(""));
    }
}
