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
//! - 生产调用方今天零个（见 [`super`] 的头注）。判据钉的是机制。
//! - 判据的对拍语料读的是那个插件的**源码**（`include_str!`）⇒ 它挡得住「改源码」，
//!   **挡不住**「同名的另一份装在 `PATH` 上」。真跑那条命令的判据住 e2e，
//!   而出货门禁一套 e2e 都不跑（`KY7`）。这一档由谁跑、什么时候跑，写在件文件里。

use copy_core::copy_text;

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
            Rejected::NotThePlugin { want, saw } => copy_text(
                "beProbe.message.wrongProgram",
                &[("want", &want.to_string()), ("saw", &saw.to_string())],
            ),
            Rejected::MissingCapability {
                plugin,
                token,
                version,
            } => copy_text(
                "beProbe.message.missingAbility",
                &[
                    ("plugin", &plugin.to_string()),
                    ("token", &token.to_string()),
                    (
                        "version",
                        &(version
                            .as_deref()
                            .unwrap_or(&copy_text("beProbe.message.versionUnknown", &[])))
                        .to_string(),
                    ),
                ],
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
            "capabilities" => capabilities = tokens(v),
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
            saw: copy_text("beProbe.parse.emptyOutput", &[]),
        }),
    }
}

/// 逗号列表 → token（去空白、去空项）。
fn tokens(v: &str) -> Vec<String> {
    v.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
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

/// 一步走完：认身份 → 逐个查必需清单。
pub(crate) fn negotiate(text: &str, want_name: &str, required: &[&str]) -> Result<Answer, Rejected> {
    let answer = parse(text, want_name)?;
    require(&answer, required)?;
    Ok(answer)
}

#[cfg(test)]
#[path = "../../../tests/backend/plugin/probe_tests.rs"]
mod tests;
