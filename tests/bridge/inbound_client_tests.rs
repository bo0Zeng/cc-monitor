use super::*;
use tokio::io::AsyncBufReadExt;

fn hello_frame(commands: &[&str]) -> InboundFrame {
    InboundFrame::Hello {
        v: 1,
        build_id: "test".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/tmp".into(),
        // daemon-split `S4`：`hello.homes` 与本用例无关（它测的是入方向命令协商），
        // 空表 = 今天所有已部署 daemon 的形态。
        homes: vec![],
        capabilities: vec![],
        commands: commands.iter().map(|s| s.to_string()).collect(),
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
    let hello = DaemonHello::from_hello_frame(&hello_frame(commands)).expect("是 Hello 帧");
    (
        park(mine).into_client(hello),
        tokio::io::BufReader::new(theirs),
    )
}

#[test]
/// P2s：**`<local>` 在两侧必须是同一个串**。
///
/// 漂了**不会报错** —— 前端的本机开关会去操作一个谁都没登记过的 origin：
/// `set_daemon_kill_on_exit("<localhost>", …)` 存进一张没人读的表，
/// `daemon_status` 永远回 `channel: false`。**设了没反应，且不报错。**
///
/// 照仓里现成的跨语言对拍形状写（`payload.rs` 的 `REFUSE_TAG` 那条 / `launch.rs` 的
/// POSIX marker 那条）：`include_str!` 读前端那份、抠出字面量、逐字比。
fn the_local_origin_is_the_same_string_on_both_sides() {
    let ts = include_str!("../../src/daemon-policy.ts");
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
/// 而 `into_client` 要求交出 `DaemonHello` 见证（`the_hello_witness_can_only_come_from_a_hello_frame`
/// 守着见证只能来自真 hello 帧）⇒ 钉住构造点唯一，整条链就闭合了。
///
/// # 这不是抽样，是完备的
///
/// `InboundClient` 的字段**全部私有** ⇒ 本文件之外的代码**编译期就构造不出**它。
/// 所以只扫本文件不是「取样」，是把全部可能的构造点都覆盖了。
/// （不另加一条「别的文件不许出现 `InboundClient {`」——那条恒绿，铁律 16 不许留。）
fn the_only_way_to_build_an_inbound_client_is_into_client() {
    let src = include_str!("../../src/bridge/src/inbound_client.rs");
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
             多一处 = 多一条不经 `DaemonHello` 见证就能造出 client 的路 ⇒\n\
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
             `into_client` 的签名要 `DaemonHello`（那是「换写能力必须交出见证」的门）；\n\
             构造搬到别的函数里 = 那道门被绕开了。"
    );
}

/// ★★ **那两句「唯一入口 / 唯一出口」必须有人读**〔audit-0805 08-07，Phase G 第 45 件〕。
///
/// 本模块头注逐字写着「`DaemonHello` 的**唯一构造入口**是 `from_hello_frame`」
/// 与「`ParkedWriter` 的**唯一出口**是 `into_client`，而它要一个 `DaemonHello`」。
/// 整条「Hello 之前不许写」的类型保证就压在这两句上 ——
/// `ssh_source` 那两条判据的诊断也是这么写的（「在这里直接写 = 静默绕过那条类型保证」）。
///
/// # 而它们是散文
///
/// 08-07 实测：给 `DaemonHello` 加 `pub fn forged(commands) -> Self`（凭空造见证）、
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
/// 必须恰好是登记的那一个。顺带钉住 `ParkedWriter` 那扇门**要见证**（签名里有 `DaemonHello`）。
#[test]
fn each_type_has_exactly_one_door_and_the_exit_needs_the_witness() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/inbound_client.rs"));

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

    let witness_doors = doors("impl DaemonHello {");
    assert_eq!(
        witness_doors,
        vec!["from_hello_frame".to_string()],
        "`DaemonHello` 的公开关联函数不再只有 `from_hello_frame`。\n\
             多出来的那个**就是第二个构造入口** —— 见证一旦能凭空造出来，\n\
             `ParkedWriter::into_client` 那道门就形同虚设，「Hello 之前不许写」当场破。\n\
             ⚠ 08-07 实测：加一个 `pub fn forged(..) -> Self`，全仓判据一条不红。\n\
             真要加，先想清楚它凭什么能证明「daemon 已经打过招呼」。"
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
        exit_sig.contains("DaemonHello"),
        "`into_client` 的签名里不再要 `DaemonHello`（实得 {exit_sig:?}）——\n\
             门还在，但不查票了。整条保证靠的就是「换写能力必须交出见证」。"
    );

    // ★★ **字段必须私有** —— 这是变异复验当场逮出来的第四条路。
    //
    // 08-07：为了验「出口不查票」那一刀，我连调用方一起改，结果编译器三重拦住
    // （参数类型 · **字段私有** · 跨模块可见性）。⇒ 那一刀说明**字段私有才是真保障**，
    // 而本条当时只钉关联函数与 `Default`，没钉它。
    // 实测把 `commands` 改成 `pub`：全仓 **977 条判据一条不红**，而从此
    // 任何模块都能 `DaemonHello { commands: vec![] }` 凭空造见证 —— 连一扇门都不用走。
    // ⇒ **验一条判据的时候，编译器替你挡住的那些，正是没人写下来的那些。**
    let witness_fields: Vec<&str> = prod
        .lines()
        .skip_while(|l| !l.starts_with("pub struct DaemonHello"))
        .skip(1)
        .take_while(|l| l.starts_with(char::is_whitespace) || l.is_empty())
        .filter(|l| l.contains(':'))
        .collect();
    assert!(
        !witness_fields.is_empty(),
        "抽不到 `DaemonHello` 的字段 —— 抽取器坏了，下面那条在空转"
    );
    for f in &witness_fields {
        assert!(
            !f.trim().starts_with("pub "),
            "`DaemonHello` 的字段 {f:?} 是 `pub` 的 —— 那是第四条路：\n\
                 任何模块都能 `DaemonHello {{ … }}` 凭空造一个见证，连一扇门都不用走。\n\
                 整条「Hello 之前不许写」压在这个字段的私有性上，别把它打开。"
        );
    }

    // 见证类型不许有 `Default`：那是一条**不经过任何函数**的构造路。
    assert!(
        !prod.contains("impl Default for DaemonHello"),
        "`DaemonHello` 实现了 `Default` —— 那是第三条路：`DaemonHello::default()` \
             凭空就是一个见证，而它连一扇门都不用走。"
    );
    let derive_line = prod
        .lines()
        .zip(prod.lines().skip(1))
        .find(|(_, next)| next.starts_with("pub struct DaemonHello"))
        .map(|(d, _)| d)
        .expect("找不到 DaemonHello 的 derive 行 —— 抽取器坏了");
    assert!(
        !derive_line.contains("Default"),
        "`DaemonHello` 的 derive 里出现了 `Default`（{derive_line:?}）—— 同上，那是不走门的构造路。"
    );
}

#[test]
fn the_hello_witness_can_only_come_from_a_hello_frame() {
    assert!(DaemonHello::from_hello_frame(&hello_frame(&["ping"])).is_some());
    assert!(
        DaemonHello::from_hello_frame(&InboundFrame::Overflow {
            dropped: 1,
            lost: Vec::new(),
            lost_truncated: false
        })
        .is_none(),
        "非 Hello 帧换出了见证 —— 「Hello 之前不许写」就破了"
    );
}

/// ★ **跨轨对拍**：`tests/e2e/inbound-daemon-frames.sh` 喂给真 daemon 的那条 ping 行，
/// 必须**逐字节**等于本模块编码器的产物。
///
/// 没有这条，那套 e2e 只证明了「daemon 认得我手写的那串 JSON」，
/// 证明不了「monitor 真发出去的那串 JSON」—— 两者一旦漂开，e2e 会**继续全绿**
/// 而生产里一条命令都发不出去。同 `removal_cause_wire_literal_stays_in_sync` 的思路。
#[test]
fn the_e2e_ping_line_is_exactly_what_the_encoder_produces() {
    const SUITE: &str = include_str!("../e2e/inbound-daemon-frames.sh");
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
        "\ne2e 脚本喂给真 daemon 的行与 monitor 编码器的产物不一致。\n\
             改了编码器就把脚本里那条 `INBOUND_PING_LINE` 一起改（反之亦然）——\n\
             它们必须是同一份事实，否则 e2e 是在验证一个 monitor 永远不会发的形状。"
    );

    // ★ 光钉变量不够 —— D 审计变异 EMU2：变量一字不动，只把 `send "$INBOUND_PING_LINE"`
    //   换成一串手抄字面量 ⇒ **两轨全绿**，而 DoD 那句「喂给 daemon 的就是编码器的字节」
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
             「daemon 认得我手抄的 JSON」，证明不了「monitor 发的那种 JSON」。"
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
/// ping 那条证明「daemon 认得 monitor 编的信封」；这条证明的是
/// **monitor 真正会发的那条 `launch`**（`daemon_send_into` 唯一会说的 `send-into`）。
/// 少了它，那套 e2e 只验证了「daemon 认得我手写的 launch 形状」——
/// 而 `launch_args` 的键名/键序一改，e2e 会继续全绿而生产里一条命令都发不出去。
#[test]
fn the_e2e_send_into_line_is_exactly_what_the_encoder_produces() {
    const SUITE: &str = include_str!("../e2e/inbound-daemon-frames.sh");
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
    assert_eq!(
        format!("{literal}\n"),
        encode_request(
            "e2e-si-1",
            "launch",
            &launch_args(
                "send-into",
                "e2e-si-fixed-cc",
                "true",
                None,
                None,
                Default::default(),
            )
        ),
        "\ne2e 脚本喂给真 daemon 的 send-into 行与 monitor 编码器的产物不一致。\n\
             `launch_args` 的键名/键序改了就把脚本里那条 `INBOUND_SEND_INTO_LINE` 一起改 ——\n\
             它们必须是同一份事实，否则 e2e 在验证一个 monitor 永远不会发的形状。"
    );
}

/// ★ e2e 脚本里硬编码的那三个命令名，必须等于 daemon 的 `inbound::COMMANDS`。
///
/// 那是命令面的**第五处**副本（前四处已由 daemon 侧两条护栏钉住）。没有这条的话，
/// 加一条新命令时 e2e 不会红 —— 只是**悄悄漏测**，而 e2e 恰恰是唯一跑真进程的那一层。
#[test]
fn the_e2e_command_list_matches_the_daemon_command_table() {
    const SUITE: &str = include_str!("../e2e/inbound-daemon-frames.sh");
    const DAEMON_INBOUND: &str = include_str!("../../src/backend/inbound.rs");

    // daemon 侧：`pub const COMMANDS: &[&str] = &["cancel", "ping", "resolve"];`
    let i = DAEMON_INBOUND
        .find("const COMMANDS")
        .expect("daemon inbound.rs 里找不到 COMMANDS —— 抽取坏了");
    let j = DAEMON_INBOUND[i..]
        .find("];")
        .map(|k| i + k)
        .expect("COMMANDS 没有收尾");
    let mut daemon: Vec<String> = DAEMON_INBOUND[i..j]
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .map(str::to_string)
        .collect();
    daemon.sort();
    daemon.dedup();
    assert!(
        daemon.len() >= 3,
        "只抽到 {} 条 daemon 命令 —— 抽取坏了，本断言在空转：{daemon:?}",
        daemon.len()
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
        suite, daemon,
        "\ne2e 脚本断言的命令集与 daemon 的 `inbound::COMMANDS` 对不上。\n\
             加/删入方向命令时这两处要一起动 —— 否则新命令在**唯一跑真进程的那一层**漏测。"
    );
}

/// ★★ `KR104D1` 的跨轨对拍：那条新原语的参数构造器与 daemon 的解析器对得上。
///
/// 〔`设计/50`：本条原名 `the_two_tmux_primitive_arg_builders_match_the_daemon_parsers`  〔散文墓碑〕
///  （`tests/evidence/K-R104-deathvalue.md` 里按那个名字记着读数）。「两条」里的
///  `oneshot-session` 随用量 ③ 轴退役，剩 `capture-pane` 一条 ⇒ 名字跟着改，
///  免得它自己变成一句假话。〕
///
/// # 为什么不照抄上面那条 `launch` 的抠法
///
/// `launch` 的解析器逐个 `get_str("<key>")`，抠得出来。`capture-pane` 的解析器写法不同
/// （复用 `kill::parse_name`）⇒ 照抄那把尺子会**零命中地绿**。
/// ⇒ 这里换一个**共同的、数据级**的真相源：daemon 的 `inbound::REGISTRY` 里那条
/// `CommandSpec::fields`（它自己已经被 `protocol_doc_guard` 与 daemon 侧的判据
/// 双向钉着，不是第三份手写清单）。
///
/// **args 是 fields 的子集**（fields = args ∪ data）⇒ 断的是**包含**，
/// 并另加一格「data 那几个键不许出现在 args 里」，免得包含关系退化成空真。
#[test]
fn the_tmux_primitive_arg_builder_matches_the_daemon_parser() {
    const DAEMON_INBOUND: &str = include_str!("../../src/backend/inbound.rs");
    let prod = guard_core::production_code(DAEMON_INBOUND);

    /// 从 daemon 的 `REGISTRY` 里抠出某条命令那一格 `fields: &[…]` 的成员。
    fn fields_of(prod: &str, cmd: &str) -> Vec<String> {
        let head = format!("name: \"{cmd}\",");
        let at = prod
            .find(&head)
            .unwrap_or_else(|| panic!("daemon 的 `REGISTRY` 里找不到 `{cmd}` —— 尺子的作用域没了"));
        let rest = &prod[at..];
        let f = rest
            .find("fields: &[")
            .unwrap_or_else(|| panic!("`{cmd}` 那一格没有 `fields`"));
        let body_at = f + "fields: &[".len();
        let end = rest[body_at..]
            .find(']')
            .unwrap_or_else(|| panic!("`{cmd}` 的 `fields` 没有收尾 `]`"));
        let body = &rest[body_at..body_at + end];
        let mut out: Vec<String> = Vec::new();
        for piece in body.split('"').skip(1).step_by(2) {
            out.push(piece.to_string());
        }
        out.sort();
        out
    }

    // ── `capture-pane` ────────────────────────────────────────────────
    let cap_fields = fields_of(&prod, "capture-pane");
    assert_eq!(
        cap_fields,
        vec!["name".to_string(), "screen".to_string()],
        "daemon 侧 `capture-pane` 的 `fields` 变了 —— 两边同拍改"
    );
    let cap = capture_pane_args("cc-x");
    let cap_keys: Vec<String> = cap.as_object().expect("对象").keys().cloned().collect();
    assert_eq!(
        cap_keys,
        vec!["name".to_string()],
        "`capture_pane_args` 的键变了"
    );
    assert!(
        !cap_keys.iter().any(|k| k == "screen"),
        "`screen` 是**回**的那一侧，不该出现在请求 args 里"
    );
}

/// ★ 跨轨对拍：`launch_args` 吐的键名必须**恰好**是 daemon 解析器认的那几个。
///
/// 漂开的症状是「命令发出去了、daemon 回 `bad_request` 说缺字段」，而两边各自看都对。
#[test]
fn launch_args_field_names_match_the_daemon_parser() {
    const DAEMON_LAUNCH: &str = include_str!("../../src/backend/control/launch.rs");
    let prod = guard_core::production_code(DAEMON_LAUNCH);
    // daemon 侧逐个 `get_str("<key>")` 抠出来。
    let key = "get_str(\"";
    let mut wanted: Vec<String> = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = prod[from..].find(key) {
        let at = from + rel + key.len();
        let end = prod[at..].find('"').map(|k| at + k).unwrap_or(at);
        wanted.push(prod[at..end].to_string());
        from = end;
    }
    wanted.sort();
    wanted.dedup();
    assert!(
        wanted.len() >= 5,
        "只从 daemon 解析器抠到 {} 个字段 —— 抽取坏了，本断言在空转：{wanted:?}",
        wanted.len()
    );

    // ⚠ **每个可选字段都要给**：漏一个，`got` 就少一个键，而 `wanted` 是从 daemon
    //   解析器抠的 —— 这条 `assert_eq!` 会当场红。那正是它该有的样子（`K-P2` `D3`
    //   加 `agent`/`width`/`height` 时它逐字红过一次）。
    let full = launch_args(
        "create-or-attach",
        "cc-x",
        "true",
        Some("/tmp"),
        Some("sid-1"),
        LaunchExtras {
            agent: Some("claude"),
            width: Some("220"),
            height: Some("50"),
        },
    );
    let mut got: Vec<String> = full
        .as_object()
        .expect("对象")
        .keys()
        .map(String::from)
        .collect();
    got.sort();
    assert_eq!(
        got, wanted,
        "\nmonitor 的 `launch_args` 与 daemon 的解析器字段名对不上。\n\
             两边必须同时改 —— 否则症状是「daemon 回 bad_request 说缺字段」，很难归因。"
    );

    // 可选字段真的可选：不传就不出现（daemon 侧 `cwd`/`ccm_sid` 都是 `Option`）。
    let minimal = launch_args("send-into", "cc-x", "true", None, None, Default::default());
    let keys: Vec<&String> = minimal.as_object().expect("对象").keys().collect();
    assert_eq!(
        keys.len(),
        3,
        "最小形态应当只有 mode/name/payload：{keys:?}"
    );
    // ★〔`K-P2` `D3`〕**半个尺寸不许上线**：只给 `width` 时两个都不发 ——
    //   让「一半的修饰」在**发出去之前**就不存在，而不是等 daemon 回 `invalid_args`。
    let half = launch_args(
        "create-or-attach",
        "cc-x",
        "true",
        None,
        None,
        LaunchExtras {
            agent: None,
            width: Some("220"),
            height: None,
        },
    );
    let half_keys: Vec<&String> = half.as_object().expect("对象").keys().collect();
    assert_eq!(
        half_keys.len(),
        3,
        "只给了 width 而 height 缺席时，两个都不该发：{half_keys:?}"
    );
}

#[test]
fn encode_request_is_byte_stable_and_matches_the_daemon_envelope() {
    let line = encode_request("abc-0", "ping", &serde_json::json!({}));
    assert_eq!(line, "{\"id\":\"abc-0\",\"cmd\":\"ping\",\"args\":{}}\n");
    // 反向：daemon 侧就是拿它当 `Request` 反序列化的，字段名必须对得上。
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
            message: "缺 sid".into()
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

/// 超时 ⇒ `Timeout`，且**自动补发一条 `cancel`**（daemon 别白跑）。
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
    assert!(matches!(err, CallError::Timeout { .. }), "{err:?}");

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

/// daemon 没声明 `cancel` 时不许补发（否则那是一条注定 `unknown_command` 的噪声）。
#[tokio::test]
async fn no_cancel_is_fired_when_the_daemon_does_not_declare_it() {
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
                .register(&format!("x{i}"))
                .unwrap_or_else(|| panic!("第 {i} 条就满了")),
        );
    }
    assert!(
        client.register("overflow").is_none(),
        "登记表没有上限 —— 死 daemon 下会无界增长"
    );
    drop(held);
}

/// ★ 满的时候先回收「调用方已走」的登记（D 审计发现的真泄漏路径）。
///
/// 「超时不摘登记」那条设计的前提是「晚到的应答终会把它摘掉」。审计指出这个前提在
/// **背压路径上不成立**：daemon 侧 cancel 的两条应答都是 `try_send`，应答通道满时
/// 静默丢弃，被 abort 的命令也不补应答 ⇒ 那条 id 永远等不到任何帧。
/// 每次超时吃 2 格，128 次封死 256 格，**而且 daemon 恢复之后也不会自愈**。
#[tokio::test]
async fn a_full_table_reclaims_registrations_whose_caller_has_left() {
    let (client, _peer) = client_on_duplex(&["ping"]);
    // 全部丢掉接收端 = 全是「调用方已走」的僵尸登记。
    for i in 0..MAX_PENDING {
        assert!(client.register(&format!("zombie{i}")).is_some());
    }
    assert_eq!(client.pending_len(), MAX_PENDING);
    assert!(
        client.register("fresh").is_some(),
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
        held.push(client2.register(&format!("live{i}")).expect("登记"));
    }
    assert!(
        client2.register("fresh").is_none(),
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
        let hello = DaemonHello::from_hello_frame(&hello_frame(&["ping"])).expect("是 Hello 帧");
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
        let hello = DaemonHello::from_hello_frame(&hello_frame(&["ping"])).expect("是 Hello 帧");
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
