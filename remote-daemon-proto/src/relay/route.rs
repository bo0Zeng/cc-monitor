//! 路由：从请求路径里切出**路由键**，把其余部分原样交给上游。
//!
//! 形状 `/s/<agent>/<key>/<真路径>` —— `<agent>` 那一段是 `K9` 裁定二最后一句要的
//! 「agent-aware 的路由键」：**今天就用**，不是预留接口。中转把这两段当**不透明串**，
//! 它不认识任何 agent 叫什么，也不解释 `<key>` 是会话 id 还是别的什么。

/// 切出来的路由。`rest` 逐字保留原请求的 `路径 + 查询串`，中转不重写它。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Route {
    pub(crate) agent: String,
    pub(crate) key: String,
    pub(crate) rest: String,
}

/// 一段路由键里允许的字符 —— 白名单，不是黑名单。
///
/// 收窄到这几类是**故意的**：路由键要参与日志与 tee 行，放开任意字节等于给
/// 「把控制字符 / 换行塞进 tee 流」开一条路。`.` 与 `/` **不在**白名单里 ⇒
/// `..` 这种段根本构造不出来。
fn segment_is_safe(seg: &str) -> bool {
    !seg.is_empty()
        && seg.len() <= 128
        && seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 解析 `/s/<agent>/<key>/<rest>`。不是这个形状就返回 `None`（调用方回 404）。
pub(crate) fn parse(target: &str) -> Option<Route> {
    let after = target.strip_prefix("/s/")?;
    let (agent, after) = after.split_once('/')?;
    let (key, rest) = after.split_once('/')?;
    if !segment_is_safe(agent) || !segment_is_safe(key) {
        return None;
    }
    Some(Route {
        agent: agent.to_string(),
        key: key.to_string(),
        rest: format!("/{rest}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_prefix_and_keeps_the_rest_verbatim() {
        let r = parse("/s/agentA/sid-AAA/v1/messages?beta=true").expect("应当解析成功");
        assert_eq!(r.agent, "agentA");
        assert_eq!(r.key, "sid-AAA");
        // ★ 期望值是**手写字面量**，不是拿被测函数算出来的（否则本断言自证、恒绿）。
        assert_eq!(r.rest, "/v1/messages?beta=true");
    }

    #[test]
    fn two_keys_do_not_collide() {
        let a = parse("/s/agentA/sid-AAA/v1/messages").expect("A");
        let b = parse("/s/agentB/sid-BBB/v1/messages").expect("B");
        assert_ne!(a.key, b.key);
        assert_ne!(a.agent, b.agent);
        assert_eq!(a.rest, b.rest, "两条路由的真路径相同，区别只在键");
    }

    #[test]
    fn rejects_everything_that_is_not_the_shape() {
        // 分母 = 我列出的这 7 形；不是「所有不合法输入」。
        for bad in [
            "/v1/messages",
            "/s/agentA",
            "/s/agentA/",
            "/s/agentA/sid-AAA",
            "/s//sid-AAA/v1",
            "/s/agentA//v1",
            "/s/../../etc/v1",
        ] {
            assert!(parse(bad).is_none(), "这一形不该被接受：{bad}");
        }
    }

    /// ⚠ **分母**〔回修轮之四 08-25 复扫补的〕：名字是个**全称**句（「cannot be smuggled」），
    /// 而它量的是**我列出的这 2 形**，**不是**「所有穿越写法」。真正承重的是白名单本身
    /// （段里只许出现白名单字符）—— 这两形是那条白名单的**样例**，不是它的证明。
    /// 〔隔壁 `rejects_everything_that_is_not_the_shape` 逐字写了「分母 = 我列出的这 7 形」，
    ///  本条先前一个字都没写 —— 同一族的话，同一份纪律。〕
    #[test]
    fn path_traversal_cannot_be_smuggled_through_a_segment() {
        // 分母 = 我列出的这 2 形。
        assert!(parse("/s/agentA/..%2f..%2fetc/v1").is_none());
        assert!(parse("/s/a.b/sid/v1").is_none(), "点号不在白名单里");
    }
}
