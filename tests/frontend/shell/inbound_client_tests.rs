use super::*;
// 〔.5，2026-09-21〕三样参数构造器搬去了 `command_args`
// （`C1` 在 `inbound_client.rs` 上咬的 `sid` / `agent` 两处全在它们身上）。
// 下面那三条判据**刻意留在这里** —— 理由（跨半边 include 被别人的登记表按文件路径钉着）
// 写在 `command_args` 的头注里，不在这里抄第二份。
use tokio::io::AsyncBufReadExt;

fn hello_frame(commands: &[&str]) -> InboundFrame {
    InboundFrame::Hello {
        v: 1,
        build_id: "test".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/tmp".into(),
        // backend-split `S4`：`hello.homes` 与本用例无关（它测的是入方向命令协商），
        // 空表 = 今天所有已部署后端的形态。
        homes: vec![],
        capabilities: vec![],
        commands: commands.iter().map(|s| s.to_string()).collect(),
        unavailable: vec![],
        uncancellable: vec![],
    }
}

/// ★ **一律带超时地读**。D 审计变异 MU16（`encode_request` 不 push `\n`）时，
/// 两条断言确实 FAILED，**但整个 `cargo test --lib` 900s 没返回** —— 5 处裸
/// `read_line().await` 收不到换行就永远悬着。CI 上那表现为 job 超时，不是失败列表，
/// 排查成本天差地别。
async fn next_line(peer: &mut tokio::io::BufReader<tokio::io::DuplexStream>) -> String {
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(5), peer.read_line(&mut line))
        .await
        .expect("等对端读一行超时（5s）—— 让它干净变红，别让 cargo test 挂死")
        .expect("读一行");
    line
}

fn client_on_duplex(
    commands: &[&str],
) -> (
    Arc<InboundClient>,
    tokio::io::BufReader<tokio::io::DuplexStream>,
) {
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let hello = BackendHello::from_hello_frame(&hello_frame(commands)).expect("是 Hello 帧");
    (
        park(mine).into_client(hello),
        tokio::io::BufReader::new(theirs),
    )
}

#[test]
/// P2s：**`<local>` 在两侧必须是同一个串**。
///
/// 漂了**不会报错** —— 前端的本机开关会去操作一个谁都没登记过的 origin：
/// 问 / 改「退出行为」那一格会发到一个谁都没登记过的 origin（`backend_exit_policy("<localhost>")` 恒回「没有控制通道」），
/// `backend_status` 永远回 `channel: false`。**设了没反应，且不报错。**
///
/// 照仓里现成的跨语言对拍形状写（`payload.rs` 的 `REFUSE_TAG` 那条 / `launch.rs` 的
/// POSIX marker 那条）：`include_str!` 读前端那份、抠出字面量、逐字比。
fn the_local_origin_is_the_same_string_on_both_sides() {
    let ts = include_str!("../../../src/frontend/ui/backend-policy.ts");
    let line = ts
        .lines()
        .find(|l| l.trim_start().starts_with("export const LOCAL_ORIGIN"))
        .expect("前端那份里找不到 `export const LOCAL_ORIGIN` —— 名字改了就来改这条");
    let lit = line
        .split('"')
        .nth(1)
        .expect("那一行不是 `export const LOCAL_ORIGIN = \"…\";` 的形状");
    assert_eq!(
        lit, LOCAL_ORIGIN,
        "两侧的本机 origin 漂了：前端 {lit:?} / 后端 {:?}。\n\
             ⚠ 这种漂**不会有任何东西报错** —— 本机开关会去操作一个谁都没登记过的 origin。",
        LOCAL_ORIGIN
    );
}

#[test]
/// P2-Y2：**造一个 `InboundClient` 的路只有 `into_client` 一条**。
///
/// # 为什么钉构造点而不是数 `register(` 的调用点
///
/// 件里的 DoD 原写「`register(` 的生产调用方只许有远端那处 + 本机那处」。
/// 那条判据数的是**调用点**，而调用点数量随功能增长天然会变（P3 之后可能有第三条传输）
/// ⇒ 它会退化成一条要人反复放宽的白名单（铁律 16 骂的正是这个）。
///
/// 真正保证「本机与远端拿到的是同一种 client」的性质是：**两边都经 `into_client`**。
/// 而 `into_client` 要求交出 `BackendHello` 见证（`the_hello_witness_can_only_come_from_a_hello_frame`
/// 守着见证只能来自真 hello 帧）⇒ 钉住构造点唯一，整条链就闭合了。
///
/// # 这不是抽样，是完备的
///
/// `InboundClient` 的字段**全部私有** ⇒ 本文件之外的代码**编译期就构造不出**它。
/// 所以只扫本文件不是「取样」，是把全部可能的构造点都覆盖了。
/// （不另加一条「别的文件不许出现 `InboundClient {`」——那条恒绿，铁律 16 不许留。）
fn the_only_way_to_build_an_inbound_client_is_into_client() {
    let src = include_str!("../../../src/frontend/shell/src/inbound_client.rs");
    // ⚠ 边界**不能**自己手搓。第一版取「第一个 `#[cfg(test)]`」—— 而 `park()` 本身就挂着
    // 那个属性、且住在 `into_client` **之前** ⇒ 那样切会把构造点整个切掉，判据扫了个空
    // （人群自检当场逮到；没有自检它会绿着挂在这里）。第二版改扫 `mod tests` 的位置，
    // 那是**语料上的裸 `.find`**，`needle_anchor_registry` 的递减棘轮不许再长。
    // ⇒ 用仓里共享的剥法，它自己有反向自检（剥完不许再出现测试属性）。
    let prod = guard_core::production_code(src);
    let prod = prod.as_str();

    // 人群：构造点 = `InboundClient {` 的字面量出现（排掉 `pub struct InboundClient {` 那处定义）。
    let sites: Vec<usize> = prod
        .match_indices("InboundClient {")
        .map(|(i, _)| i)
        // 排掉**声明**（`pub struct InboundClient {` / `impl InboundClient {`）——
        // 它们与构造长得一样，但不是构造。第一版只排了 `struct`，`impl` 那处混进人群把计数顶成 2。
        .filter(|i| {
            let before = prod[..*i].trim_end();
            !before.ends_with("struct") && !before.ends_with("impl")
        })
        .collect();

    assert!(
        !sites.is_empty(),
        "人群塌了：生产段一个 `InboundClient {{` 构造点都没扫到。\n\
             要么构造被搬走了，要么写法变了（比如改用 `Self {{`）—— 无论哪种，本判据都已失守。"
    );
    assert_eq!(
        sites.len(),
        1,
        "`InboundClient` 的构造点不止一处（实得 {} 处）。\n\
             多一处 = 多一条不经 `BackendHello` 见证就能造出 client 的路 ⇒\n\
             本机与远端拿到的 client 可能语义不同，而没有任何东西会响。",
        sites.len()
    );

    // 那唯一一处必须住在 `into_client` 里 —— 往前找最近的 `fn `。
    let before = &prod[..sites[0]];
    let fn_at = before.rfind("fn ").expect("构造点之前总有一个 fn");
    let name: String = prod[fn_at + 3..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    assert_eq!(
        name, "into_client",
        "唯一的构造点跑到了 `{name}` 里，而不是 `into_client`。\n\
             `into_client` 的签名要 `BackendHello`（那是「换写能力必须交出见证」的门）；\n\
             构造搬到别的函数里 = 那道门被绕开了。"
    );
}

/// ★★ **那两句「唯一入口 / 唯一出口」必须有人读**〔audit-0805 08-07，Phase G 第 45 件〕。
///
/// 本模块头注逐字写着「`BackendHello` 的**唯一构造入口**是 `from_hello_frame`」
/// 与「`ParkedWriter` 的**唯一出口**是 `into_client`，而它要一个 `BackendHello`」。
/// 整条「Hello 之前不许写」的类型保证就压在这两句上 ——
/// `stream_source` 那两条判据的诊断也是这么写的（「在这里直接写 = 静默绕过那条类型保证」）。
///
/// # 而它们是散文
///
/// 08-07 实测：给 `BackendHello` 加 `pub fn forged(commands) -> Self`（凭空造见证）、
/// 给 `ParkedWriter` 加 `pub fn into_inner(self) -> W`（不要见证就把写半边取回来），
/// **全仓 976 条判据一条不红**。旁边那条 `the_hello_witness_can_only_come_from_a_hello_frame`
/// 是**单函数行为测试**（Hello→Some / 非 Hello→None），它只管那一扇门开得对不对，
/// **不管有没有第二扇门**。
///
/// ⇒ 定框 **E12**：判准是「有没有一条**会红**的判据读它」。本条就是那条。
///
/// # 钉法
///
/// 人群从 `impl` 块**派生**（不手写清单），默认拒绝：两个类型各自的公开关联函数
/// 必须恰好是登记的那一个。顺带钉住 `ParkedWriter` 那扇门**要见证**（签名里有 `BackendHello`）。
#[test]
fn each_type_has_exactly_one_door_and_the_exit_needs_the_witness() {
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/inbound_client.rs"
    ));

    // 取某个 `impl` 块（从签名行到下一个顶格行）里的 `pub fn` 名。
    // ⚠ 顶格行做边界、不写花括号字面量：本文件会被按括号配平剥，
    //   落单的右花括号会打坏那个配平（本工作区真踩过一次）。
    let doors = |head: &str| -> Vec<String> {
        let at = prod
            .find(head)
            .unwrap_or_else(|| panic!("生产段里找不到 `{head}` —— 抽取器坏了，本条此刻无效"));
        let mut out = Vec::new();
        for (i, line) in prod[at..].lines().enumerate() {
            // ⚠ 顶格的 `where` / `)` / `{` 是**头的一部分**，不是边界。
            //   第一版漏了 `where`，于是 `impl<W> ParkedWriter<W>` 的块被切在签名处、
            //   抽出空清单 —— 「我以为的对象 ≠ 切片圈住的对象」这一族，本会话第三次。
            let header_cont =
                line.starts_with("where") || line.starts_with(')') || line.trim() == "{";
            if i > 0 && !line.is_empty() && !line.starts_with(char::is_whitespace) && !header_cont {
                break;
            }
            if let Some(rest) = line.trim().strip_prefix("pub fn ") {
                out.push(
                    rest.chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect::<String>(),
                );
            }
        }
        out
    };

    let witness_doors = doors("impl BackendHello {");
    assert_eq!(
        witness_doors,
        vec!["from_hello_frame".to_string()],
        "`BackendHello` 的公开关联函数不再只有 `from_hello_frame`。\n\
             多出来的那个**就是第二个构造入口** —— 见证一旦能凭空造出来，\n\
             `ParkedWriter::into_client` 那道门就形同虚设，「Hello 之前不许写」当场破。\n\
             ⚠ 08-07 实测：加一个 `pub fn forged(..) -> Self`，全仓判据一条不红。\n\
             真要加，先想清楚它凭什么能证明「backend 已经打过招呼」。"
    );

    let exit_doors = doors("impl<W> ParkedWriter<W>");
    assert_eq!(
        exit_doors,
        vec!["into_client".to_string()],
        "`ParkedWriter` 的公开关联函数不再只有 `into_client`。\n\
             多出来的那个**很可能是不要见证的第二个出口**（例如 `into_inner`）——\n\
             调用方一旦能拿回裸写半边，本模块存在的理由就没了。\n\
             ⚠ 08-07 实测：加一个 `pub fn into_inner(self) -> W`，全仓判据一条不红。"
    );

    // 那扇唯一的出口必须**要见证**。
    let exit_sig = prod
        .lines()
        .find(|l| l.contains("pub fn into_client("))
        .expect("上面已确认它存在");
    assert!(
        exit_sig.contains("BackendHello"),
        "`into_client` 的签名里不再要 `BackendHello`（实得 {exit_sig:?}）——\n\
             门还在，但不查票了。整条保证靠的就是「换写能力必须交出见证」。"
    );

    // ★★ **字段必须私有** —— 这是变异复验当场逮出来的第四条路。
    //
    // 08-07：为了验「出口不查票」那一刀，我连调用方一起改，结果编译器三重拦住
    // （参数类型 · **字段私有** · 跨模块可见性）。⇒ 那一刀说明**字段私有才是真保障**，
    // 而本条当时只钉关联函数与 `Default`，没钉它。
    // 实测把 `commands` 改成 `pub`：全仓 **977 条判据一条不红**，而从此
    // 任何模块都能 `BackendHello { commands: vec![] }` 凭空造见证 —— 连一扇门都不用走。
    // ⇒ **验一条判据的时候，编译器替你挡住的那些，正是没人写下来的那些。**
    let witness_fields: Vec<&str> = prod
        .lines()
        .skip_while(|l| !l.starts_with("pub struct BackendHello"))
        .skip(1)
        .take_while(|l| l.starts_with(char::is_whitespace) || l.is_empty())
        .filter(|l| l.contains(':'))
        .collect();
    assert!(
        !witness_fields.is_empty(),
        "抽不到 `BackendHello` 的字段 —— 抽取器坏了，下面那条在空转"
    );
    for f in &witness_fields {
        assert!(
            !f.trim().starts_with("pub "),
            "`BackendHello` 的字段 {f:?} 是 `pub` 的 —— 那是第四条路：\n\
                 任何模块都能 `BackendHello {{ … }}` 凭空造一个见证，连一扇门都不用走。\n\
                 整条「Hello 之前不许写」压在这个字段的私有性上，别把它打开。"
        );
    }

    // 见证类型不许有 `Default`：那是一条**不经过任何函数**的构造路。
    assert!(
        !prod.contains("impl Default for BackendHello"),
        "`BackendHello` 实现了 `Default` —— 那是第三条路：`BackendHello::default()` \
             凭空就是一个见证，而它连一扇门都不用走。"
    );
    let derive_line = prod
        .lines()
        .zip(prod.lines().skip(1))
        .find(|(_, next)| next.starts_with("pub struct BackendHello"))
        .map(|(d, _)| d)
        .expect("找不到 BackendHello 的 derive 行 —— 抽取器坏了");
    assert!(
        !derive_line.contains("Default"),
        "`BackendHello` 的 derive 里出现了 `Default`（{derive_line:?}）—— 同上，那是不走门的构造路。"
    );
}

#[test]
fn the_hello_witness_can_only_come_from_a_hello_frame() {
    assert!(BackendHello::from_hello_frame(&hello_frame(&["ping"])).is_some());
    assert!(
        BackendHello::from_hello_frame(&InboundFrame::Overflow {
            dropped: 1,
            lost: Vec::new(),
            lost_truncated: false
        })
        .is_none(),
        "非 Hello 帧换出了见证 —— 「Hello 之前不许写」就破了"
    );
}

/// ★ **跨轨对拍**：`tests/e2e/inbound-backend-frames.sh` 喂给真后端的那条 ping 行，
/// 必须**逐字节**等于本模块编码器的产物。
///
/// 没有这条，那套 e2e 只证明了「backend 认得我手写的那串 JSON」，
/// 证明不了「monitor 真发出去的那串 JSON」—— 两者一旦漂开，e2e 会**继续全绿**
/// 而生产里一条命令都发不出去。同 `removal_cause_wire_literal_stays_in_sync`〔散文墓碑〕 的思路。
#[test]
fn the_e2e_ping_line_is_exactly_what_the_encoder_produces() {
    const SUITE: &str = include_str!("../../e2e/inbound-backend-frames.sh");
    let key = "INBOUND_PING_LINE='";
    let at = SUITE
        .find(key)
        .expect("e2e 脚本里找不到 INBOUND_PING_LINE —— 抽取坏了，本断言在空转");
    let rest = &SUITE[at + key.len()..];
    let literal = &rest[..rest.find('\'').expect("赋值没有收尾单引号")];
    assert!(
        literal.len() > 20,
        "抽到的字面量太短（{literal:?}）—— 抽取坏了"
    );
    assert_eq!(
        format!("{literal}\n"),
        encode_request("e2e-ping-1", "ping", &Value::Null),
        "\ne2e 脚本喂给真后端的行与 monitor 编码器的产物不一致。\n\
             改了编码器就把脚本里那条 `INBOUND_PING_LINE` 一起改（反之亦然）——\n\
             它们必须是同一份事实，否则 e2e 是在验证一个 monitor 永远不会发的形状。"
    );

    // ★ 光钉变量不够 —— D 审计变异 EMU2：变量一字不动，只把 `send "$INBOUND_PING_LINE"`
    //   换成一串手抄字面量 ⇒ **两轨全绿**，而 DoD 那句「喂给后端的就是编码器的字节」
    //   已经不成立了（shellcheck 也拦不住：未用变量是 SC2034 warning，CI 只看 error）。
    //   所以再钉一条：那个变量必须真的被送出去，且**只此一处**发 ping。
    let sends: Vec<&str> = SUITE
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("send ") && !l.starts_with("send() "))
        .collect();
    assert!(
        sends.len() >= 6,
        "只抽到 {} 条 send —— 抽取坏了，本断言在空转：{sends:?}",
        sends.len()
    );
    let via_var = sends
        .iter()
        .filter(|l| l.contains("$INBOUND_PING_LINE"))
        .count();
    assert_eq!(
        via_var, 1,
        "\n脚本里经 `$INBOUND_PING_LINE` 发出去的行应恰好一条（实得 {via_var}）——\n\
             那个变量是与 monitor 编码器逐字节对拍的**唯一**载体，绕过它 e2e 就变成\n\
             「backend 认得我手抄的 JSON」，证明不了「monitor 发的那种 JSON」。"
    );
    // 反面：别的 send 里不许再出现 `"cmd":"ping"` 的手抄 ping 请求（cancel/unknown 等无妨）。
    let hand_written_ping: Vec<&&str> = sends
        .iter()
        .filter(|l| !l.contains("$INBOUND_PING_LINE") && l.contains(r#""cmd":"ping""#))
        .collect();
    // 例外：并发/坏输入之后那几条**刻意**用别的 id 手写（它们验的是路由与存活，不是编码器）。
    // 只要它们不是「第一条 ping 往返」那条即可 —— 用 id 前缀区分。
    for l in &hand_written_ping {
        assert!(
            l.contains("e2e-multi-") || l.contains("e2e-after-garbage"),
            "\n发现一条手抄的 ping 请求，它绕开了跨轨对拍：{l}\n\
                 核心那条 ping 必须走 `$INBOUND_PING_LINE`。"
        );
    }
}

/// ★ U8a-2c-1：同上，但钉的是**业务命令**那一行。
///
/// ping 那条证明「backend 认得 monitor 编的信封」；这条证明的是
/// **界面真正会发的那条 `launch`**（此前是 monitor 的 `backend_send_into`〔散文墓碑〕，今天是
/// `src/frontend/ui/tmux-control.ts::sendInto` / `sendKeys` 说的 `send-into`）。
/// 少了它，那套 e2e 只验证了「backend 认得我手写的 launch 形状」——
/// 而发送那一侧的键名一改，e2e 会继续全绿而生产里一条命令都发不出去。
#[test]
fn the_e2e_send_into_line_is_exactly_what_the_encoder_produces() {
    const SUITE: &str = include_str!("../../e2e/inbound-backend-frames.sh");
    let key = "INBOUND_SEND_INTO_LINE='";
    let at = SUITE
        .find(key)
        .expect("e2e 脚本里找不到 INBOUND_SEND_INTO_LINE —— 抽取坏了，本断言在空转");
    let rest = &SUITE[at + key.len()..];
    let literal = &rest[..rest.find('\'').expect("赋值没有收尾单引号")];
    assert!(
        literal.len() > 40,
        "抽到的字面量太短（{literal:?}）—— 抽取坏了"
    );
    // 这一格原来比的是「e2e 那一行 == monitor 编码器 `launch_args`〔散文墓碑〕的产物」。
    //   就地 resume / 送键迁到界面之后，发这条的是 `src/frontend/ui/tmux-control.ts`（经通道，monitor 那一跳只把 JSON 原样
    //   转成 `args`）⇒ 「真在发的形状」的源头换成跨语言金样 `tests/__fixtures__/tmux-control.golden.json`
    //   里 `launch` 的请求样例（TS 那侧逐字断言它发的就是这一份）。本格比：e2e 那一行的 `args` 键集合 == 金样那一份，
    //   mode 是 `send-into`，且信封是 monitor 那一跳会产出的那一行（`encode_request` 重编一遍逐字节相等）。
    let line: serde_json::Value = serde_json::from_str(literal).expect("e2e 那一行不是 JSON");
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/tmux-control.golden.json"))
            .expect("金样读不出来");
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v
            .as_object()
            .expect("args 不是对象")
            .keys()
            .cloned()
            .collect();
        k.sort();
        k
    };
    assert_eq!(line["cmd"], "launch", "e2e 那一行不是 `launch`");
    assert_eq!(
        keys(&line["args"]),
        keys(&golden["launch"]["request"]),
        "\ne2e 脚本喂给真后端的 send-into 行，键与界面真在发的那一份（金样）不一致 ——\n\
             它们必须是同一份事实，否则 e2e 在验证一个界面永远不会发的形状。"
    );
    assert_eq!(line["args"]["mode"], "send-into");
    assert_eq!(
        format!("{literal}\n"),
        encode_request(line["id"].as_str().expect("id"), "launch", &line["args"]),
        "e2e 那一行的信封不是 monitor 那一跳会产出的那一行（键序 / 空白对不上）"
    );
}

/// ★ e2e 脚本里硬编码的那几个命令名，必须等于后端命令表里的单词命令名（命令表各族的 `name`）。
///
/// 那是命令面的**第五处**副本（前四处已由后端侧两条护栏钉住）。没有这条的话，
/// 加一条新命令时 e2e 不会红 —— 只是**悄悄漏测**，而 e2e 恰恰是唯一跑真进程的那一层。
#[test]
fn the_e2e_command_list_matches_the_backend_command_table() {
    const SUITE: &str = include_str!("../../e2e/inbound-backend-frames.sh");

    // backend 侧：命令表各族里每一条的 `name: "…"`（`hello.commands` 就从这里派生）。
    let mut backend: Vec<String> = crate::guard_support::backend_registry_sources()
        .iter()
        .flat_map(|(_, prod)| {
            prod.split("name: \"")
                .skip(1)
                .filter_map(|t| t.split('"').next())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .collect();
    backend.sort();
    backend.dedup();
    assert!(
        backend.len() >= 3,
        "只抽到 {} 条后端命令 —— 抽取坏了，本断言在空转：{backend:?}",
        backend.len()
    );

    // e2e 侧：`for c in ping cancel resolve; do`
    let key = "for c in ";
    let at = SUITE
        .find(key)
        .expect("e2e 脚本里找不到命令名清单 —— 抽取坏了");
    let rest = &SUITE[at + key.len()..];
    let line = &rest[..rest.find('\n').unwrap_or(rest.len())];
    let mut suite: Vec<String> = line
        .split_whitespace()
        // 行尾是 `resolve; do` —— 剥掉分号，遇到 `do` 停。
        .map(|t| t.trim_end_matches(';'))
        .take_while(|t| {
            !t.is_empty() && *t != "do" && t.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        })
        .map(str::to_string)
        .collect();
    assert!(
        suite.len() >= 3,
        "只从 e2e 脚本抽到 {} 条命令 —— 抽取坏了，本断言在空转：{suite:?}",
        suite.len()
    );
    suite.sort();
    suite.dedup();

    assert_eq!(
        suite, backend,
        "\ne2e 脚本断言的命令集与后端命令表里的单词命令对不上。\n\
             加/删入方向命令时这两处要一起动 —— 否则新命令在**唯一跑真进程的那一层**漏测。"
    );
}

// 这里原来住着 `KR104D1` 那条跨轨对拍（抓屏的参数构造器 `capture_pane_args` ↔ 后端 `REGISTRY` 那一格 `fields`）〔散文墓碑〕。
//   monitor 侧那个构造器随发送端删了；抓屏今天走终端管理那一条 `terminal-preview`（界面 `src/frontend/ui/terminal-reads.ts`），
//   形状由跨语言金样 `tests/__fixtures__/terminals.golden.json` 钉着（后端 `terminals_tests.rs` · 界面 `terminal-reads.vitest.ts` 读同一份）。

// 这里原来住着「`launch_args`〔散文墓碑〕吐的键名恰好是后端解析器认的那几个」（跨轨读后端 `control/launch.rs`）。
//   monitor 侧那个构造器随发送端迁到界面删了；「发出去的键 == 后端解析器认的键」改由跨语言金样钉：
//   后端侧 `tests/backend/control/launch_tests.rs` 让金样的请求样例（两个 mode 各一份）过**生产**解析器，
//   界面侧 `tests/frontend/ui/tmux-control.vitest.ts` 断言它发的就是那一份。

#[test]
fn encode_request_is_byte_stable_and_matches_the_backend_envelope() {
    let line = encode_request("abc-0", "ping", &serde_json::json!({}));
    assert_eq!(line, "{\"id\":\"abc-0\",\"cmd\":\"ping\",\"args\":{}}\n");
    // 反向：backend 侧就是拿它当 `Request` 反序列化的，字段名必须对得上。
    let v: Value = serde_json::from_str(line.trim_end()).expect("必须是合法 JSON");
    for k in ["id", "cmd", "args"] {
        assert!(v.get(k).is_some(), "信封缺字段 `{k}`：{line}");
    }
}

#[tokio::test]
async fn a_call_writes_one_line_and_resolves_on_the_matching_reply() {
    let (client, mut peer) = client_on_duplex(&["ping"]);
    let c = client.clone();
    let caller =
        tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_secs(5)).await });

    let line = next_line(&mut peer).await;
    let req: Value = serde_json::from_str(line.trim_end()).expect("请求是合法 JSON");
    let id = req["id"].as_str().expect("有 id").to_string();
    assert_eq!(req["cmd"], "ping");

    assert!(
        client.route_reply(
            &id,
            true,
            None,
            None,
            Some(serde_json::json!({ "pong": 1 }))
        ),
        "路由没找到等待者"
    );
    let got = caller.await.expect("caller task").expect("call 成功");
    assert_eq!(got, Some(serde_json::json!({ "pong": 1 })));
}

#[tokio::test]
async fn an_error_reply_surfaces_code_and_message() {
    let (client, mut peer) = client_on_duplex(&["resolve"]);
    let c = client.clone();
    let caller = tokio::spawn(async move {
        c.call("resolve", serde_json::json!({}), Duration::from_secs(5))
            .await
    });
    let line = next_line(&mut peer).await;
    let id = serde_json::from_str::<Value>(line.trim_end()).expect("JSON")["id"]
        .as_str()
        .expect("id")
        .to_string();
    client.route_reply(
        &id,
        false,
        Some("bad_request".into()),
        Some("缺 sid".into()),
        None,
    );
    assert_eq!(
        caller.await.expect("task").unwrap_err(),
        CallError::Remote {
            code: "bad_request".into(),
            message: "缺 sid".into(),
            data: None,
        }
    );
}

#[tokio::test]
async fn a_cancelled_frame_ends_the_call_as_cancelled() {
    let (client, mut peer) = client_on_duplex(&["ping"]);
    let c = client.clone();
    let caller =
        tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_secs(5)).await });
    let line = next_line(&mut peer).await;
    let id = serde_json::from_str::<Value>(line.trim_end()).expect("JSON")["id"]
        .as_str()
        .expect("id")
        .to_string();
    client.route_cancelled(&id);
    assert_eq!(
        caller.await.expect("task").unwrap_err(),
        CallError::Cancelled
    );
}

/// 命令不在 `hello.commands` 里 ⇒ 客户端侧直接拒，**一个字节都不发**。
#[tokio::test]
async fn an_undeclared_command_is_refused_without_writing_anything() {
    let (client, mut peer) = client_on_duplex(&["ping"]);
    let err = client
        .call("launch", Value::Null, Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(matches!(err, CallError::Unsupported { .. }), "{err:?}");
    // 对侧不该收到任何东西。
    let mut buf = String::new();
    let read = tokio::time::timeout(Duration::from_millis(80), peer.read_line(&mut buf)).await;
    assert!(read.is_err(), "被拒的命令却发出去了：{buf:?}");
}

/// **一个截止时刻管好几问：后一问只剩前一问没用完的那一点**。
///
/// 守的要求：「**一次调用一个绝对时刻**，不是每跳一个 `Duration`」「`Duration` 跨跳传递时
/// 每一跳都会重新开始计时 —— 那正是病 2 的机制。绝对时刻只能收紧、不能放宽」。
///
/// 异源：真 `InboundClient` 接一根内存双工管子，对端由本用例扮演（不经 `frame_query`，不看源码）。
/// 两向：第一问截止时刻还远 ⇒ 照常拿到应答（正控）；第二问拿**同一个**时刻 ⇒ 只等剩下那一截就报超时，
/// 而且真等到了点（不是提前放弃）。今天之前的形状（每问 `now + 一整份`）在第二问上会再等一整份 ⇒ 红。
#[tokio::test]
async fn two_asks_under_one_deadline_share_it_and_the_second_gets_only_the_rest() {
    let (client, mut peer) = client_on_duplex(&["ping"]);
    let whole = Duration::from_millis(1500);
    let until = tokio::time::Instant::now() + whole;

    // 第一问：对端过 1000 ms 才答 —— 吃掉大半，但在截止之前（正控：远没到点时照常拿到）。
    let c = client.clone();
    let first = tokio::spawn(async move { c.call_until("ping", Value::Null, until).await });
    let line = next_line(&mut peer).await;
    let id = serde_json::from_str::<Value>(line.trim_end()).expect("JSON")["id"]
        .as_str()
        .expect("id")
        .to_string();
    tokio::time::sleep(Duration::from_millis(1000)).await;
    assert!(client.route_reply(&id, true, None, None, None));
    assert!(first.await.expect("task").is_ok(), "截止之前答了却没拿到");

    // 第二问：同一个截止时刻，对端不答。
    let t2 = tokio::time::Instant::now();
    let second = client.call_until("ping", Value::Null, until).await;
    let waited = t2.elapsed();
    assert!(
        matches!(second, Err(CallError::Timeout { .. })),
        "{second:?}"
    );
    assert!(
        tokio::time::Instant::now() >= until,
        "没到截止时刻就放弃了（等了 {waited:?}）"
    );
    assert!(
        waited < Duration::from_millis(1200),
        "第二问又等了一整份（{waited:?}，整份 {whole:?}）—— 截止时刻被重新计时了"
    );
}

/// 超时 ⇒ `Timeout`，且**自动补发一条 `cancel`**（backend 别白跑）。
#[tokio::test]
async fn a_timeout_fires_a_cancel_for_the_abandoned_id() {
    let (client, mut peer) = client_on_duplex(&["ping", "cancel"]);
    let c = client.clone();
    let caller =
        tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_millis(60)).await });

    let first = next_line(&mut peer).await;
    let ping_id = serde_json::from_str::<Value>(first.trim_end()).expect("JSON")["id"]
        .as_str()
        .expect("id")
        .to_string();

    let err = caller.await.expect("task").unwrap_err();
    assert!(
        matches!(
            err,
            CallError::Timeout {
                withdraw: Withdraw::Asked,
                ..
            }
        ),
        "{err:?}"
    );

    let mut second = String::new();
    tokio::time::timeout(Duration::from_secs(2), peer.read_line(&mut second))
        .await
        .expect("等 cancel 超时了")
        .expect("读到 cancel");
    let cancel: Value = serde_json::from_str(second.trim_end()).expect("JSON");
    assert_eq!(cancel["cmd"], "cancel");
    assert_eq!(
        cancel["args"]["target"].as_str(),
        Some(ping_id.as_str()),
        "补发的 cancel 没指向被放弃的那条命令"
    );
    assert_ne!(
        cancel["id"].as_str(),
        Some(ping_id.as_str()),
        "cancel 复用了被取消者的 id"
    );
}

/// ★**调用方放弃等待**（future 被丢）⇒ 同样补发一条 `cancel{target: 那条 id}`；
/// 拿到结局之后再丢 ⇒ 一条都不发；超时 ⇒ 恰一条（上面那条钉「有」，这里钉「不多发」）。
///
/// 为什么非有它：界面撤一问在 monitor 这一侧落成的是「把等应答的那个 future 丢掉」——
/// 丢了不发 `cancel`，后端可取消档那一条照跑到它自己的期限。
#[tokio::test]
async fn abandoning_the_wait_fires_one_cancel_and_finishing_fires_none() {
    // ① 被丢：已入队、还没结局。
    let (client, mut peer) = client_on_duplex(&["ping", "cancel"]);
    let c = client.clone();
    let caller =
        tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_secs(60)).await });
    let first = next_line(&mut peer).await;
    let ping_id = serde_json::from_str::<Value>(first.trim_end()).expect("JSON")["id"]
        .as_str()
        .expect("id")
        .to_string();
    caller.abort();
    assert!(
        caller.await.is_err_and(|e| e.is_cancelled()),
        "任务没被撤掉"
    );
    let second = next_line(&mut peer).await;
    let cancel: Value = serde_json::from_str(second.trim_end()).expect("JSON");
    assert_eq!(cancel["cmd"], "cancel", "被丢之后发的不是 cancel：{second}");
    assert_eq!(
        cancel["args"]["target"].as_str(),
        Some(ping_id.as_str()),
        "补发的 cancel 没指向被放弃的那条命令"
    );
    let mut extra = String::new();
    let read = tokio::time::timeout(Duration::from_millis(150), peer.read_line(&mut extra)).await;
    assert!(read.is_err(), "被丢之后发了不止一条：{extra:?}");

    // ② 拿到结局之后：零条。
    let (client, mut peer) = client_on_duplex(&["ping", "cancel"]);
    let c = client.clone();
    let caller =
        tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_secs(60)).await });
    let line = next_line(&mut peer).await;
    let id = serde_json::from_str::<Value>(line.trim_end()).expect("JSON")["id"]
        .as_str()
        .expect("id")
        .to_string();
    assert!(client.route_reply(&id, true, None, None, None));
    assert!(caller.await.expect("task").is_ok());
    let mut extra = String::new();
    let read = tokio::time::timeout(Duration::from_millis(150), peer.read_line(&mut extra)).await;
    assert!(read.is_err(), "拿到应答之后还补发了东西：{extra:?}");

    // ③ 超时：恰一条（守卫与超时那一臂不许各发一次）。
    let (client, mut peer) = client_on_duplex(&["ping", "cancel"]);
    let c = client.clone();
    let caller =
        tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_millis(60)).await });
    let _first = next_line(&mut peer).await;
    assert!(matches!(
        caller.await.expect("task").unwrap_err(),
        CallError::Timeout { .. }
    ));
    let one = next_line(&mut peer).await;
    assert!(one.contains("\"cmd\":\"cancel\""), "{one}");
    let mut extra = String::new();
    let read = tokio::time::timeout(Duration::from_millis(150), peer.read_line(&mut extra)).await;
    assert!(read.is_err(), "超时补发了不止一条 cancel：{extra:?}");
}

/// backend 没声明 `cancel` 时不许补发（否则那是一条注定 `unknown_command` 的噪声）。
#[tokio::test]
async fn no_cancel_is_fired_when_the_backend_does_not_declare_it() {
    let (client, mut peer) = client_on_duplex(&["ping"]);
    let c = client.clone();
    let caller =
        tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_millis(60)).await });
    let _first = next_line(&mut peer).await;
    assert!(matches!(
        caller.await.expect("task").unwrap_err(),
        CallError::Timeout { .. }
    ));
    let mut second = String::new();
    let read = tokio::time::timeout(Duration::from_millis(200), peer.read_line(&mut second)).await;
    assert!(read.is_err(), "不该补发 cancel，却发了：{second:?}");
}

/// 抓本线程上 warn 级以上的日志（`#[tokio::test]` 是单线程运行时，任务都在这条线程上跑）。
#[derive(Clone, Default)]
struct Warns(Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for Warns {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Warns {
    /// 装到本线程上。另握一个空的 `Dispatch`：只剩一个登记的 dispatcher 时，tracing 按「碰到 callsite 的那条线程」
    /// 的默认去算 interest 并缓存 —— 别的线程先碰到就会缓存成「没人要」（本条第一版因此时红时绿）。
    fn install(&self) -> (tracing::subscriber::DefaultGuard, tracing::Dispatch) {
        let sink = self.clone();
        let spare = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
        let guard = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_writer(move || sink.clone())
                .with_ansi(false)
                .with_max_level(tracing::Level::WARN)
                .finish(),
        );
        tracing::callsite::rebuild_interest_cache();
        (guard, spare)
    }
    /// 说「对端不认撤单」的那几行。
    fn peer_cannot_withdraw(&self) -> Vec<String> {
        String::from_utf8(self.0.lock().unwrap().clone())
            .unwrap()
            .lines()
            .filter(|l| l.contains("对端不认撤单"))
            .map(str::to_string)
            .collect()
    }
}

/// 「对面认不认撤是一条能力……不认 ⇒ 本地照撤，**并且在结果里说明对端不认**」。
/// 两形各两向：① 握手没交出 `cancel` ⇒ 结果 `NotOffered` ＋ 那句话多说一句 ＋ warn 一条；交出了 ⇒ `Asked`、零条。
/// ② 补发的撤单被回 `not_cancellable` ⇒ warn 一条、点名那条命令；回 ok ⇒ 零条。
#[tokio::test]
async fn a_peer_that_cannot_withdraw_is_said_out_loud() {
    // ① 握手那一形。
    for (ops, want) in [
        (&["ping"][..], Withdraw::NotOffered),
        (&["ping", "cancel"][..], Withdraw::Asked),
    ] {
        let warns = Warns::default();
        let _g = warns.install();
        let (client, mut peer) = client_on_duplex(ops);
        let c = client.clone();
        let caller =
            tokio::spawn(
                async move { c.call("ping", Value::Null, Duration::from_millis(60)).await },
            );
        let _first = next_line(&mut peer).await;
        let err = caller.await.expect("task").unwrap_err();
        let CallError::Timeout { withdraw, after } = err else {
            panic!("{err:?}");
        };
        assert_eq!(withdraw, want, "{ops:?}");
        let said = err.to_string();
        let ms = after.as_millis().to_string();
        let told = warns.peer_cannot_withdraw();
        match want {
            Withdraw::NotOffered => {
                // 按文案键断言，不钉原文。
                assert_eq!(
                    said,
                    crate::copy_table::copy_text(
                        "rsInboundClient.error.timeoutPeerRunsOn",
                        &[("after", &ms)]
                    ),
                    "结果里没说对端不认"
                );
                assert_eq!(told.len(), 1, "{told:?}");
                assert!(told[0].contains("`ping`"), "没点名那条命令：{told:?}");
            }
            Withdraw::Asked | Withdraw::Unsent => {
                assert_eq!(
                    said,
                    crate::copy_table::copy_text(
                        "rsInboundClient.error.timeout",
                        &[("after", &ms)]
                    )
                );
                assert!(told.is_empty(), "{told:?}");
            }
        }
    }

    // ② 补发之后对端说「停不下来」。
    for (code, loud) in [(Some("not_cancellable"), true), (None, false)] {
        let warns = Warns::default();
        let _g = warns.install();
        let (client, mut peer) = client_on_duplex(&["ping", "cancel"]);
        let c = client.clone();
        let caller =
            tokio::spawn(
                async move { c.call("ping", Value::Null, Duration::from_millis(60)).await },
            );
        let _first = next_line(&mut peer).await;
        assert!(caller.await.expect("task").is_err());
        let cancel: Value =
            serde_json::from_str(next_line(&mut peer).await.trim_end()).expect("JSON");
        let cid = cancel["id"].as_str().expect("id").to_string();
        let ok = code.is_none();
        assert!(client.route_reply(&cid, ok, code.map(str::to_string), None, None));
        let told = warns.peer_cannot_withdraw();
        assert_eq!(told.len(), usize::from(loud), "{code:?} ⇒ {told:?}");
        if loud {
            assert!(told[0].contains("`ping`"), "没点名那条命令：{told:?}");
        }
    }
}

/// 超时之后**晚到的应答**不许触发「未登记的 id」——那是预期内的事，不该刷 warn。
#[tokio::test]
async fn a_late_reply_after_timeout_still_finds_its_registration() {
    let (client, mut peer) = client_on_duplex(&["ping"]);
    let c = client.clone();
    let caller =
        tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_millis(60)).await });
    let line = next_line(&mut peer).await;
    let id = serde_json::from_str::<Value>(line.trim_end()).expect("JSON")["id"]
        .as_str()
        .expect("id")
        .to_string();
    assert!(matches!(
        caller.await.expect("task").unwrap_err(),
        CallError::Timeout { .. }
    ));
    assert!(
        client.route_reply(&id, true, None, None, None),
        "超时后登记被摘早了 —— 晚到的应答会落进 unknown-id 的 warn"
    );
}

/// 登记表有上限：**调用方还在等**的那些占着位，占满就快速失败。
#[tokio::test]
async fn the_pending_table_is_capped_by_live_waiters() {
    let (client, _peer) = client_on_duplex(&["ping"]);
    // 把接收端**留着**（= 调用方还在等），否则会被下面那条回收逻辑扫掉。
    let mut held = Vec::new();
    for i in 0..MAX_PENDING {
        held.push(
            client
                .register(&format!("x{i}"), None)
                .unwrap_or_else(|| panic!("第 {i} 条就满了")),
        );
    }
    assert!(
        client.register("overflow", None).is_none(),
        "登记表没有上限 —— 死后端下会无界增长"
    );
    drop(held);
}

/// ★ 满的时候先回收「调用方已走」的登记（D 审计发现的真泄漏路径）。
///
/// 「超时不摘登记」那条设计的前提是「晚到的应答终会把它摘掉」。审计指出这个前提在
/// **背压路径上不成立**：backend 侧 cancel 的两条应答都是 `try_send`，应答通道满时
/// 静默丢弃，被 abort 的命令也不补应答 ⇒ 那条 id 永远等不到任何帧。
/// 每次超时吃 2 格，128 次封死 256 格，**而且后端恢复之后也不会自愈**。
#[tokio::test]
async fn a_full_table_reclaims_registrations_whose_caller_has_left() {
    let (client, _peer) = client_on_duplex(&["ping"]);
    // 全部丢掉接收端 = 全是「调用方已走」的僵尸登记。
    for i in 0..MAX_PENDING {
        assert!(client.register(&format!("zombie{i}"), None).is_some());
    }
    assert_eq!(client.pending_len(), MAX_PENDING);
    assert!(
        client.register("fresh", None).is_some(),
        "表被僵尸登记撑满后再也登记不上 —— 那正是审计说的「不自愈」"
    );
    assert_eq!(
        client.pending_len(),
        1,
        "回收之后表里应当只剩刚登记的那一条"
    );
    // 反面：活着的等待者**不许**被当成僵尸扫掉。
    let (client2, _peer2) = client_on_duplex(&["ping"]);
    let mut held = Vec::new();
    for i in 0..MAX_PENDING {
        held.push(client2.register(&format!("live{i}"), None).expect("登记"));
    }
    assert!(
        client2.register("fresh", None).is_none(),
        "把还在等的调用方当成僵尸回收了 —— 那会让它们永远收不到应答"
    );
    drop(held);
}

#[tokio::test]
async fn shutdown_wakes_every_waiter_with_disconnected() {
    let (client, mut peer) = client_on_duplex(&["ping"]);
    let c = client.clone();
    let caller =
        tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_secs(5)).await });
    let _line = next_line(&mut peer).await;
    client.shutdown();
    assert_eq!(
        caller.await.expect("task").unwrap_err(),
        CallError::Disconnected
    );
}

/// 每条连接一套号段：两个客户端的 `id` 不许撞。
#[test]
fn ids_from_two_connections_never_collide() {
    let mk = || {
        let (mine, _theirs) = tokio::io::duplex(1024);
        let hello = BackendHello::from_hello_frame(&hello_frame(&["ping"])).expect("是 Hello 帧");
        park(mine).into_client(hello)
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt");
    let (a, b) = rt.block_on(async { (mk(), mk()) });
    let ids_a: Vec<String> = (0..5).map(|_| a.next_id()).collect();
    let ids_b: Vec<String> = (0..5).map(|_| b.next_id()).collect();
    assert_eq!(ids_a.len(), 5);
    for x in &ids_a {
        assert!(!ids_b.contains(x), "两条连接发出了同一个 id `{x}`");
    }
}

/// 注册表：摘除只摘自己那条，别把重连上来的新客户端摘掉。
#[test]
fn unregister_never_removes_someone_elses_client() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt");
    let mk = || {
        let (mine, _theirs) = tokio::io::duplex(1024);
        let hello = BackendHello::from_hello_frame(&hello_frame(&["ping"])).expect("是 Hello 帧");
        park(mine).into_client(hello)
    };
    let (old, new) = rt.block_on(async { (mk(), mk()) });
    let origin = "unregister-test-origin";
    register(origin, old.clone());
    register(origin, new.clone());
    unregister(origin, &old); // 旧连接迟到的收尾
    assert!(
        client_for(origin).is_some_and(|c| Arc::ptr_eq(&c, &new)),
        "旧连接的收尾把新连接摘掉了"
    );
    unregister(origin, &new);
    assert!(client_for(origin).is_none(), "自己的条目没摘掉");
}

// ── P28：给这条源码扫描型守卫立**负对照** ──
//
// 判的不是产品性质，是「**剥法没把我要扫的那一段剥掉**」。
// 被扫的 `src/frontend/shell/src/inbound_client.rs` 今天 649 行，第一个 `#[cfg(test)]` 在 **192** 行
// ⇒ 便宜近似 `src.split("\n#[cfg(test)]").next()` 把扫描面砍到前 191 行，
// 而本文件要扫的东西在它**后面**（逐针行号写在下面）⇒ 扫描面静默缩水时本文件会**零命中地绿**。
//
// 原语与它买不到什么：`guard_core::assert_stripper_keeps` 的头注。
// 一句话：它不买「针还是那个针」—— 下面这张表必须从本文件真正用的针里抄。

/// ★ 扫描面自检：共享剥法留住了本文件要扫的那几段，而便宜近似留不住。
#[test]
fn the_shared_stripper_keeps_the_construction_site_this_guard_must_scan() {
    // 🔴 本文件抬头（`each_type_…` 那条判据上面）已经把这条负对照**写成了散文**：
    //    「第一版取『第一个 `#[cfg(test)]`』—— 而 `park()` 本身就挂着那个属性、
    //     且住在 `into_client` **之前** ⇒ 那样切会把构造点整个切掉」。
    //    那段话写下来了，**而没有任何断言去核它** —— 本仓「写下来的边界不是判据」
    //    那一族的又一处。⇒ 这一条就是那句散文的机检形态。
    //
    // 逐针：`impl<W> ParkedWriter<W>`（232）· `pub fn into_client(`（239）·
    // 唯一构造点 `InboundClient {`（286）—— 都在 192 之后。
    // ⚠ 刻意**不**填 `pub struct BackendHello`（132）与 `impl BackendHello {`（136）：
    //    那两个在 192 **之前**，便宜近似也留得住 ⇒ 填进去这条对照会被
    //    `assert_stripper_keeps` 当场判成「失去意义」（它正是为此而红，不是静默放过）。
    guard_core::assert_stripper_keeps(
        "inbound_client_tests",
        include_str!("../../../src/frontend/shell/src/inbound_client.rs"),
        &["impl<W> ParkedWriter<W>", "pub fn into_client("],
    );
}

/// 握手里的能力事实从线上一路进 `Offer`，调用侧照它办：
/// ① `unavailable` 列了的命令 ⇒ 不发、回 `Unavailable{code}`（与那台事后回的码同一个）；没列的照发。
/// ② `uncancellable` 列了的命令超时 ⇒ `NotOffered`、一条撤单都不补；没列的 ⇒ `Asked`、补一条。
/// 两侧异源：左边是真 hello 行经 `stream_source::parse_frame` 解出来的，右边是本条手写的期望。
#[tokio::test]
async fn the_hello_facts_decide_what_is_sent_and_what_is_withdrawn() {
    let line = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d","commands":["cancel","kill","ping","launch"],"unavailable":[{"command":"kill","code":"no_tmux"},{"command":7}],"uncancellable":["launch",null]}"#;
    let frame = crate::stream_source::parse_frame(line).expect("是 hello");
    let hello = BackendHello::from_hello_frame(&frame).expect("是 Hello 帧");
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let client = park(mine).into_client(hello);
    let mut peer = tokio::io::BufReader::new(theirs);

    // ①
    let err = client
        .call("kill", Value::Null, Duration::from_secs(5))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        CallError::Unavailable {
            cmd: "kill".into(),
            code: "no_tmux".into()
        }
    );
    let mut stray = String::new();
    let read = tokio::time::timeout(Duration::from_millis(100), peer.read_line(&mut stray)).await;
    assert!(read.is_err(), "做不到的命令还是发出去了：{stray:?}");

    // ②
    for (cmd, want, cancels) in [
        ("launch", Withdraw::NotOffered, 0usize),
        ("ping", Withdraw::Asked, 1),
    ] {
        let c = client.clone();
        let caller =
            tokio::spawn(async move { c.call(cmd, Value::Null, Duration::from_millis(60)).await });
        let sent = next_line(&mut peer).await;
        assert!(sent.contains(&format!("\"cmd\":\"{cmd}\"")), "{sent}");
        let err = caller.await.expect("task").unwrap_err();
        assert!(
            matches!(err, CallError::Timeout { withdraw, .. } if withdraw == want),
            "{cmd}: {err:?}"
        );
        let mut got = 0;
        let mut extra = String::new();
        while tokio::time::timeout(Duration::from_millis(100), peer.read_line(&mut extra))
            .await
            .is_ok()
        {
            assert!(extra.contains("\"cmd\":\"cancel\""), "{extra}");
            got += 1;
            extra.clear();
        }
        assert_eq!(got, cancels, "{cmd}");
    }
}

/// **握手之后才装上 tmux 的机器，靠「重新对齐」认出来**：hello 说 `kill` 做不到 ⇒ 事前拒；
/// `resync` 的应答交回那台当下的能力事实（`unavailable` 空了）⇒ 换进 `Offer`，之后 `kill` 照发。
#[tokio::test]
async fn a_resync_reply_refreshes_the_offer_with_the_facts_of_this_moment() {
    let line = r#"{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/d","commands":["cancel","kill","resync"],"unavailable":[{"command":"kill","code":"no_tmux"}]}"#;
    let frame = crate::stream_source::parse_frame(line).expect("是 hello");
    let hello = BackendHello::from_hello_frame(&frame).expect("是 Hello 帧");
    let (mine, theirs) = tokio::io::duplex(64 * 1024);
    let client = park(mine).into_client(hello);
    let mut peer = tokio::io::BufReader::new(theirs);
    let before = client.offer().unavailable("kill").map(str::to_string);

    let c = client.clone();
    let caller =
        tokio::spawn(async move { c.call(RESYNC_OP, Value::Null, Duration::from_secs(5)).await });
    let sent = next_line(&mut peer).await;
    let id = serde_json::from_str::<Value>(sent.trim_end()).expect("JSON")["id"]
        .as_str()
        .expect("id")
        .to_string();
    let data = serde_json::json!({"added":0,"removed":0,"retagged":0,"watchers":1,"unavailable":[],"uncancellable":["resync"]});
    assert!(client.route_reply(&id, true, None, None, Some(data)));
    caller.await.expect("task").expect("resync 答了");

    let after = client.offer();
    assert_eq!(
        (
            before.as_deref(),
            after.unavailable("kill"),
            after.withdraw("resync")
        ),
        (Some("no_tmux"), None, Withdraw::NotOffered),
        "`resync` 交回的当下事实没有换进 Offer"
    );
}
