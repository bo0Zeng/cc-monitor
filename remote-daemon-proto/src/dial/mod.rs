//! `K-P6b` 候选 E 的**代理进程**：`--dial` —— 把 daemon 那条长连接流的拨号搬出界面进程。
//!
//! # 🔴 第一行就写死这一件事买到了多少（`D2` 改窄后的原话，别读大）
//!
//! 本文件买到的是：**`daemon 那条长连接流` 的那一跳 SSH 握手，发生在这个进程里，
//! 不发生在界面进程里。**
//!
//! **它没有买到的，同段写死**：
//!
//! - 🔴 **界面进程仍然自己拨号 —— 7 处里搬走的是 1 处。**
//!   `connect_session` 的生产调用点共 **7 处 / 3 份**（PM 09-06 现打，逐处住址在
//!   `features/K-P6b-把拨号搬出界面进程.md#§0f`）：SFTP · 端口转发 · 跳板 ·
//!   `ssh_source` 里另外几处一次性 exec 与测试连接。**那 6 处一处都没动。**
//!   ⇒ **任何地方都不许把本件写成「拨号搬出去了」** —— 它是「长连接那一条路上搬了 1 处」。
//!   搬走的那条是**活得最久**的一条（重要性不按处数算），**但处数也不许藏着**。
//! - 🔴 **`D3③`：Windows 上关掉界面，这个代理进程跟着走。**
//!   它是界面起的**子进程**，`stdin`/`stdout` 就是它与界面之间的那条管子；
//!   界面一退，管子断、`stdin` EOF ⇒ 本进程收工。
//!   **别让下一个人以为「搬出去了」就等于「它独立跑着」** —— 用户对这一格知情、押后。
//! - **`K-P7` 定的那三样，原样继承**：**界面仍解帧**（本进程只搬字节，不认帧）·
//!   **凭据面 `K11` 挡着** · **`ConnectStage` 那 6 格过不去**（本进程不发分阶段事件）。
//! - ⚠ **「默认装机走不走得到这条路」——两个答案，别混**（这一格我第一版判错过，订正如下）：
//!   - **发版包里走得到。** 现打四环（我自己逐份读的原文，**点符号不点行号**）：
//!     ① `local_backend.rs::SIDECAR_STEM` 逐字 `pub const SIDECAR_STEM: &str = "cc-monitor-remote";`；
//!     ② `src-tauri/tauri.sidecar.conf.json` 逐字 `"externalBin": ["binaries/cc-monitor-remote"]`
//!        —— **同一个名字**；
//!     ③ `.github/workflows/release.yml` 的 `build-windows` 里两步 ——
//!        `Build local backend sidecar (native)`（`working-directory: remote-daemon-proto`
//!        · `cargo build --release`）＋ `Stage sidecar for externalBin`
//!        （拷成 `cc-monitor-remote-<triple>.exe`），随后
//!        `npx tauri build --config src-tauri/tauri.sidecar.conf.json`；
//!        `build-linux` 那个 job 三步同形；
//!     ④ 同一份 workflow 的那段头注逐字写着 daemon **在 Windows 上真编得过**
//!        （2026-08-04 真机实测 exit=0、release 2.6 MB、跑起来发完整 hello）。
//!     ⇒ 装完之后 sidecar 就在 `monitor.exe` 旁边，`local_backend::resolve_beside_this_exe` 命中。
//!   - **开发树上走不到。** `externalBin` **不住 `tauri.conf.json`**，它住那份单独的
//!     `tauri.sidecar.conf.json`、只在发版那一步注入 ⇒ `cargo run` / `npm run tauri dev`
//!     两处都空 ⇒ **回落到进程内拨号**。
//!   🔴 **我第一版把后者写成了全称**，理由正是本仓那条老病：**查的是开发树，
//!   得到的是一个只在开发树上为真的答案**（`src-tauri/src/local_accounts.rs` 里
//!   那条登记自己就写着同一句）。**这条边界的射程是「开发树」，不是「默认装机」。**
//! - ⚠ **另一条真会让它回落的**：本代理只会 **publickey** 一种鉴权
//!   ⇒ 配置里没填 `keyPath`（Windows 上走 ssh-agent 那一档）时，界面**不走代理**。
//!   两条回落条件都由 `ssh_source` 那侧的判据登记着，不是散文。
//!
//! # 为什么是 E（`audits/K-P6b-PM.md §一`，别在这里重推一遍）
//!
//! 第一轮正面比过 E 与 B：B **少一个进程**（Windows 上本机后端本来就是界面的子进程，
//! sidecar 就在 `monitor.exe` 旁边），代价是**动协议面**（实打：给 `Frame::Overflow`
//! 加一个字段要改 5 处才编得过）＋ **碰第 4 根针**（`observe/watcher.rs` 的那条观测通道诞生点）。
//!
//! ⚠ **第一轮还写过「B 会动 `inbound.rs`，那是红线」—— 那句话 PM 撤掉了**：
//! 那条红线是「**PM 持有这份文件、agent 不许改**」，管的是**谁动手**，不是这个设计成不成立。
//! **别再引用它当否决理由。**
//!
//! ⇒ E 站得住靠的是件计划 `§0` 自己的取舍标准，逐字「**不是它最好，是它最容易反悔**」：
//! **E 改的是进程数**（不喜欢就去掉那个进程，线上跑着的 daemon 一个都不用换）·
//! **B 改的是线上格式**（发出去之后新老 daemon 的兼容面就固化了，反悔要带一次协议迁移）。
//!
//! # 形状照 `--relay`（`K-P7` 逐字，别改写）
//!
//! > 另起一个进程做**字节代理**（照 `--relay` 那条分派臂的形状），
//! > **origin 仍由「哪条连接」决定** ⇒ **协议面零改动 · 六根针一根不碰**
//! > （条件：不复用 `listen::Admit`）· additive ✅ · `Overflow.lost` 不动。
//!
//! ★ 中间那句是要点：E **不需要**给协议加「这条消息是哪台机来的」——
//! **一条连接对一个 origin**，而「哪条连接」今天就是判 origin 的办法。
//! ⇒ 本进程**不碰 `listen::Admit`**、不认 `wire::Frame`、不进常驻监听那条路：
//! 它只有一条管子进、一条管子出，中间是一条 SSH channel。
//!
//! # 线上形状（**不是** `wire.rs` 那套协议 —— 那份一个字节没动）
//!
//! ```text
//! 界面 → 代理  环境变量 CCM_DIAL_REQUEST：一份 JSON = DialRequest（见下）
//!              stdin   **全部**是原始字节，原样写进 SSH channel（daemon 的 stdin）
//! 代理 → 界面  stdout  第一行：一行 JSON = DialAck，`\n` 结尾
//!                      其后：原始字节，原样来自 SSH channel（daemon 的 stdout）
//! ```
//!
//! **为什么配置走环境变量而不走 argv**：`argv` 在同机**任何**用户的 `ps` 里都看得见，
//! 而 `/proc/<pid>/environ` 只有本人（与 root）读得到。私钥**路径**、主机名、用户名
//! 不该躺在世界可读的地方。
//!
//! **为什么也不走 stdin 第一行**（第一版就是那么写的，改掉了）：那要求界面那侧
//! 往一条流里写一行，而 `ssh_source` 有一条判据**逐字禁止它自己往流里写**
//! （写的能力在 `U8a-2a` 整个交给了 `inbound_client` 的 `ParkedWriter`）。
//! 硬写就得去放宽那条判据 —— **代价不值**。换成环境变量之后 `stdin` **纯粹**是
//! daemon 那条通道，一个字节的带外数据都没有，反而更干净。
//! 〔这一格是本轮被那条判据逼出来的，**不是设计时想到的**。如实记。〕
//!
//! **为什么 ack 要有**：界面那侧 `connect_and_exec` 的契约是「回 `Ok` 就是这条流通了」。
//! 没有 ack 的话，「连不上」与「连上了但远端还没说话」在管子上一模一样 ——
//! 那正是本仓治过很多次的那种「两件事在终端上同形」。
//!
//! # 边界（诚实登记，别读成「验过了」）
//!
//! - **鉴权只支持 `key_path`（publickey）**。`ssh-agent` 那条界面侧只在 Windows 上有实现
//!   （命名管道），而本 crate 今天只出 Linux musl 二进制 ⇒ 这条路上**没有 agent 鉴权**。
//!   缺 `key_path` 直接 `DialAck{ok:false}`，**不静默回落**。
//! - **不做多地址竞速（F45）、不做跳板（F56）**：那两样在界面侧是 `connect_session` 的
//!   上游逻辑，搬它们不在本件射程里（`§2.1`）。本进程只拨 `host:port` 这**一个**地址。
//! - **host key 指纹校验照搬界面那侧的语义**：给了期望值就严格比（`trim` 之后），
//!   没给就 TOFU 接受并 `warn`。**绝不静默接受任意 key。**
//! - **musl 交叉编译没验**：CI 要把本 crate `zigbuild` 到两个 musl target，而沙箱门禁
//!   只做本机 gnu 构建 ⇒ **门禁全绿证不出那两个 target 编得过**。理由与读数住 `Cargo.toml`
//!   里 `russh` 那条依赖的块头注。
//!
//! # 选路还没定
//!
//! 🔴 **B / C / E 选哪条仍挂在用户那里** —— 上面这些是给读数的判断，**不是替用户拍的板**。

use std::sync::{Arc, Mutex};

use russh::client;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKey};
use tokio::io::AsyncWriteExt;

/// 装那份请求 JSON 的环境变量名。**两端各钉一半**，那边钉「写进这个名字」。
pub const REQUEST_ENV: &str = "CCM_DIAL_REQUEST";

/// 请求读不成 ⇒ 调用错误。与 daemon 别处「一次性查询」的 `exit 2` 同族。
pub const EXIT_BAD_REQUEST: i32 = 2;
/// 请求读得懂但拨不通（TCP / 指纹 / 鉴权 / exec 任一步失败）。
pub const EXIT_DIAL_FAILED: i32 = 3;

/// 界面交给代理的那一行 JSON。
///
/// 字段是 `RemoteConfig` 的**子集**，故意不整份搬：本进程不需要 `label` / `addresses` /
/// `jump` / `daemonless`（那几样都是界面侧的上游决策，见头注「边界」）。
///
/// ⚠ **蛇形键**（不是界面配置那套 camelCase）：这条管子的两端都是我们自己，
/// 而 `RemoteConfig` 的 camelCase 是**给前端看的**契约。两者不该被拴在一起 ——
/// 前端哪天改了字段名，不该把这条进程间的管子一起改坏。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DialRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// OpenSSH 格式私钥的**路径**（不是私钥本体 —— 凭据面 `K11` 挡着，见头注）。
    pub key_path: Option<String>,
    /// 期望的 host key 指纹（`SHA256:…`）。`None` = TOFU 接受并 `warn`。
    pub host_key_fingerprint: Option<String>,
    /// 要 exec 的命令行（界面侧已经 `shell_quote` 过、并拼好流模式 flag）。
    pub command: String,
}

/// 代理回给界面的那一行 JSON。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DialAck {
    pub ok: bool,
    /// `ok=false` 时的人话原因。界面把它原样冒泡给重连那一层。
    pub error: Option<String>,
    /// 实际观察到的 host key 指纹 —— 界面侧 TOFU 固化要它。
    pub fingerprint: Option<String>,
}

/// host key 校验。语义**逐条对齐**界面侧 `ssh_source::ClientHandler`：
/// 给了期望值就严格比（比之前 `trim`），没给就 TOFU 接受 + 显眼 `warn`。
struct DialHandler {
    expected_fingerprint: Option<String>,
    observed_fingerprint: Arc<Mutex<Option<String>>>,
}

impl client::Handler for DialHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let actual = server_public_key.fingerprint(HashAlg::Sha256).to_string();
        // 接受与否都先把实际指纹写回共享格：界面要拿它做 TOFU 固化。
        if let Ok(mut slot) = self.observed_fingerprint.lock() {
            *slot = Some(actual.clone());
        }
        match &self.expected_fingerprint {
            Some(expected) => {
                if actual == expected.trim() {
                    tracing::info!("dial: host key 指纹校验通过：{actual}");
                    Ok(true)
                } else {
                    let alg = server_public_key.algorithm();
                    tracing::error!(
                        "dial: host key 失配：期望 {expected}，实得 {actual}（alg={alg}）—— 拒绝这条连接。\
                         若服务器确系合法轮换过 host key，请在界面里「重置为 TOFU」后重连。"
                    );
                    Ok(false)
                }
            }
            None => {
                tracing::warn!(
                    "dial: **未经校验**接受 host key {actual}（TOFU）—— \
                     界面侧应当把它固化回配置，之后转严格校验。"
                );
                Ok(true)
            }
        }
    }
}

/// 真正把字节交给 SSH 状态机的那一段：连 → 校验指纹 → 鉴权 → 开 channel → exec。
///
/// 🔴 **这个函数是 `D2` 那条判据的被测对象**：它、以及它调的那几个 `russh` 入口，
/// 是本 crate 里**唯一**允许出现拨号锚点的地方（判据 `tests::dial_locality`）。
async fn dial(
    req: &DialRequest,
) -> Result<(russh::ChannelStream<client::Msg>, Option<String>), String> {
    let observed: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let config = Arc::new(client::Config {
        // 长连接：**不靠 inactivity 拆链**（界面侧 `connect_session` 的 FIX 1 逐字同理由），
        // 死链由 keepalive 超时 + EOF 检出。
        // SSH 层 keepalive **30 秒**，与界面侧长连接那条路同值。
        // ⚠ 写成毫秒字面量而不是一个具名常量，是**被两条判据夹出来的**：
        //   `no_timer_guard` 的禁用表把「秒」那个构造器整个禁了；
        //   `byte_cap_registry` 又要求每个尺寸类具名常量登记进它那张表（而那张表不在本轮写区）。
        //   ⇒ 字面量 + 这段话，比一个要去别人表上签字的名字便宜。**如实记，不装成风格选择。**
        // 它为什么不是本进程的节拍：见 `no_timer_guard::REGISTERED_DURATION_USES` 里本文件那一条，
        // 那条登记理由把「谁在醒来」写得很直白，别在这里再写一份会漂的副本。
        inactivity_timeout: None,
        keepalive_interval: Some(std::time::Duration::from_millis(30_000)),
        ..Default::default()
    });
    let handler = DialHandler {
        expected_fingerprint: req.host_key_fingerprint.clone(),
        observed_fingerprint: Arc::clone(&observed),
    };
    let mut session = client::connect(config, (req.host.as_str(), req.port), handler)
        .await
        .map_err(|e| format!("连 {}:{} 失败: {e}", req.host, req.port))?;

    let best_hash = session
        .best_supported_rsa_hash()
        .await
        .map_err(|e| format!("协商 rsa hash 失败: {e}"))?
        .flatten();

    let key_path = req
        .key_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            "本代理只支持 publickey（keyPath）鉴权：ssh-agent 那条只在界面侧的 Windows 实现里有，\
         本 crate 今天只出 Linux musl 二进制。**不静默回落**，请配 keyPath。"
                .to_string()
        })?;
    let key_pair =
        load_secret_key(key_path, None).map_err(|e| format!("加载私钥 {key_path} 失败: {e}"))?;
    let authenticated = session
        .authenticate_publickey(
            &req.user,
            PrivateKeyWithHashAlg::new(Arc::new(key_pair), best_hash),
        )
        .await
        .map_err(|e| format!("publickey 鉴权失败: {e}"))?;
    if !authenticated.success() {
        return Err(format!("publickey 鉴权被拒（user={}）", req.user));
    }

    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("打开 session channel 失败: {e}"))?;
    // want_reply = true：等远端确认 exec 成功再回 ack —— 与界面侧 `connect_and_exec_cmd` 同。
    channel
        .exec(true, req.command.as_bytes())
        .await
        .map_err(|e| format!("exec {} 失败: {e}", req.command))?;

    let fp = observed.lock().ok().and_then(|g| g.clone());
    Ok((channel.into_stream(), fp))
}

/// 解析那份请求 JSON。**抽出来是为了判据够得着它** —— 判据不该去起一个真进程
/// 才能验「蛇形键读得动」。
pub(crate) fn parse_request(raw: &str) -> Result<DialRequest, serde_json::Error> {
    serde_json::from_str(raw.trim())
}

/// 把一行 ack 写出去并 flush。**必须 flush** —— 界面在 `read_line` 上等着它。
async fn write_ack<W: tokio::io::AsyncWrite + Unpin>(
    w: &mut W,
    ack: &DialAck,
) -> std::io::Result<()> {
    let mut line = serde_json::to_string(ack).unwrap_or_else(|_| {
        // 序列化一个三字段结构不会失败；真失败了也要给对面一行读得懂的东西，
        // 而不是让它在 `read_line` 上挂死。
        "{\"ok\":false,\"error\":\"ack 序列化失败\",\"fingerprint\":null}".to_string()
    });
    line.push('\n');
    w.write_all(line.as_bytes()).await?;
    w.flush().await
}

/// `--dial` 那条分派臂的实现。
///
/// **不读 argv** —— 参数只用来在诊断里回显。配置走**环境变量** `CCM_DIAL_REQUEST`
/// （下面第三行就是它），**argv 与 stdin 都不走**；理由住本文件模块头注「为什么也不走 stdin 第一行」那一段。
/// 〔墓碑 —— 本行原话逐字：「**不读 argv**（配置全在 stdin 那一行）—— 参数只用来在诊断里回显。」
///  09-10 订正：那是**第一版**的形状，改掉了。这是同一次漂移的**第四份副本**
///  （前三份：`doc/IPC-PROTOCOL.md` §10 两处 + `remote-daemon-proto/src/main.rs` 的 `SUBCOMMANDS` 表）。
///  ⚠ **它与本文件 84-87 行对着干了一个多月** —— 那几行写的就是订正后的说法。
///  实测（09-10 现打两趟）：不设该变量 ⇒ 退出码 2、stdout 0 字节；
///  把 JSON 原样喂进 stdin 第一行、仍不设变量 ⇒ **还是退出码 2、还是同一句话**
///  ⇒ 「stdin 第一行」今天**一个字节不被读**。〕
pub async fn run(args: &[String]) -> i32 {
    tracing::info!("dial: 代理进程起来了（argv={args:?}）");
    let mut input = tokio::io::stdin();
    let mut out = tokio::io::stdout();

    let raw = match std::env::var(REQUEST_ENV) {
        Ok(s) if !s.trim().is_empty() => s,
        _ => {
            tracing::error!("dial: 环境变量 {REQUEST_ENV} 没设（或是空的）—— 界面没交请求");
            return EXIT_BAD_REQUEST;
        }
    };
    let req: DialRequest = match parse_request(&raw) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("dial: {REQUEST_ENV} 不是一个合法的 DialRequest: {e}");
            let _ = write_ack(
                &mut out,
                &DialAck {
                    ok: false,
                    error: Some(format!("请求解析失败: {e}")),
                    fingerprint: None,
                },
            )
            .await;
            return EXIT_BAD_REQUEST;
        }
    };

    let (stream, fp) = match dial(&req).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("dial: 拨号失败: {e}");
            let _ = write_ack(
                &mut out,
                &DialAck {
                    ok: false,
                    error: Some(e),
                    fingerprint: None,
                },
            )
            .await;
            return EXIT_DIAL_FAILED;
        }
    };
    if write_ack(
        &mut out,
        &DialAck {
            ok: true,
            error: None,
            fingerprint: fp,
        },
    )
    .await
    .is_err()
    {
        tracing::error!("dial: 写 ack 失败（界面已经走了？）");
        return EXIT_DIAL_FAILED;
    }

    // 两条方向对拷。**哪一边先结束就收工** ——
    // 下行结束 = 远端 daemon 走了；上行结束 = 界面走了（`D3③` 那一句的落点）。
    let (mut down, mut up) = tokio::io::split(stream);
    tokio::select! {
        r = tokio::io::copy(&mut input, &mut up) => {
            tracing::info!("dial: 上行结束（界面那头断了）：{r:?}");
        }
        r = tokio::io::copy(&mut down, &mut out) => {
            tracing::info!("dial: 下行结束（远端那头断了）：{r:?}");
        }
    }
    0
}

#[cfg(test)]
mod tests {
    //! `D2` 判据的**乙半**（代理这一侧）＋ 它的反向自检。
    //!
    //! # 它断的是哪一个性质
    //!
    //! **「与远端跑 SSH 传输层握手」这件事，在本 crate 里只许发生在 `dial/` 底下。**
    //!
    //! 🔴 **它断的不是「`russh` 这个词还在不在」** —— `K-P6` 那一拍已经证过后者会量错集合
    //! （**14 份在往外拨的文件里，11 份的 `russh` 代码态是 0**）。
    //! 锚点取的是**真正把字节交给 SSH 状态机的那几个入口**，不是 crate 名、不是函数名。
    //!
    //! # 甲半在哪
    //!
    //! 甲半（界面这一侧：daemon 那条长连接流不再自己拨号）住
    //! `../src-tauri/src/ssh_source.rs` 的测试模块 —— **两侧各扫各的 crate**，
    //! 刻意不从这里 `include_str!` 伸到对面去（那会新增一条跨轨编译期边，
    //! 而那张登记表不在本轮写区里）。

    use super::*;

    /// 真正把字节交给 SSH 状态机的入口。**这三条就是判据的锚点。**
    ///
    /// 为什么是这三条：`connect` = 自己建 TCP 再跑传输层握手；`connect_stream` = 在别人给的
    /// 流上跑传输层握手（跳板那一形）；`channel_open_direct_tcpip` = 把一条 TCP 隧道开到
    /// 第三方去（隧道本身就是「往外拨」的另一种形态）。
    ///
    /// ⚠ **诚实边界**：锚点按**源码文本**取 ⇒ 换个 `use` 别名、或把调用藏进宏，
    /// 本判据看不见。它与 `readonly_guard` / `protocol_doc_guard` 头注里那条同族的
    /// 诚实边界是一样的 —— **别读成证明**。
    fn anchors() -> Vec<String> {
        // 运行时拼：直接写字面量的话，本模块自己就会被下面的扫描命中
        //（同类自指陷阱本仓在 P4 连踩过七次）。
        [
            ("client::conn", "ect("),
            ("client::conn", "ect_stream("),
            ("channel_open_direct", "_tcpip("),
        ]
        .iter()
        .map(|(a, b)| format!("{a}{b}"))
        .collect()
    }

    /// 允许出现拨号锚点的**文件维前缀**。闭集，只此一条。
    const DIAL_HOME: &str = "dial/";

    /// 语料地板：低于这个字节数就判「语料没喂进来」而不是「一处都没有」。
    ///
    /// 🔴 这条就是 `P6bM4` 的被测对象 —— **喂空输入，它必须自己先红**。
    const CORPUS_FLOOR_BYTES: usize = 50_000;

    /// 乙半判据本体。**纯函数**：语料由调用方给 ⇒ 阳性/阴性两个方向都切得动。
    ///
    /// 返回 `Err(说法)` = 判据红。三种红各有各的话：
    /// ① 语料太小（空转）· ② 一处锚点都没有（拨号根本没搬进来）· ③ 锚点长在 `dial/` 外面。
    fn dial_locality(corpus: &[(String, String)]) -> Result<usize, String> {
        let bytes: usize = corpus.iter().map(|(_, c)| c.len()).sum();
        if bytes < CORPUS_FLOOR_BYTES {
            return Err(format!(
                "语料只有 {bytes} 字节（地板 {CORPUS_FLOOR_BYTES}）—— 本判据此刻在空转。\n\
                 「一处都没扫到」与「根本没扫」在终端上一模一样，这条地板就是把它们分开的那一刀。"
            ));
        }
        let pats = anchors();
        let mut total = 0usize;
        let mut strays: Vec<String> = Vec::new();
        for (name, code) in corpus {
            for p in &pats {
                let n = code.matches(p.as_str()).count();
                if n == 0 {
                    continue;
                }
                total += n;
                if !name.starts_with(DIAL_HOME) {
                    strays.push(format!("{name} 里有 {n} 处 `{p}`"));
                }
            }
        }
        if !strays.is_empty() {
            return Err(format!(
                "拨号锚点长到 `{DIAL_HOME}` 外面去了：{strays:?}\n\
                 本 crate 里「与远端跑 SSH 握手」只许住 `{DIAL_HOME}` —— \
                 别处要拨号，先回答「为什么这一处非得自己拨」。"
            ));
        }
        if total == 0 {
            return Err(format!(
                "本 crate 生产段里一处拨号锚点都没有（锚点：{pats:?}）——\n\
                 那意味着代理这一侧根本没在拨号，`K-P6b` 的乙半是空的。"
            ));
        }
        Ok(total)
    }

    /// 本 crate `src/` 递归全部 `.rs` 的**生产段**，相对 `src/` 的路径 + 正文。
    ///
    /// 🔴 **必须走 `guard_core::scan_tree!`**（`scanning_guard_registry` 那条判据钉着）：
    /// 裸 `read_dir` 的扫描型判据会**在自己的登记表 / 注释 / 常量里找到自己** ⇒ 恒绿，
    /// `audit-0805` 实测过五次，五次都不是被判据自己逮到的。
    ///
    /// ⚠⚠ **代价必须知道**：`scan_tree!` 按构造**摘除调用者自己那一份** ——
    /// 也就是 `dial/mod.rs` 不在它返回的人群里。而拨号锚点恰恰全住这一份 ⇒
    /// 光靠它，判据的「至少有一处」永远为 0。
    /// ⇒ 本函数把自己那一份**显式**用 `include_str!` 补回来，两半各司其职：
    /// **树扫描管「别处有没有」**（它摘掉自己正好），**`include_str!` 管「自己有没有」**。
    fn crate_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out: Vec<(String, String)> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, crate::guard_support::production_code(&src)));
        }
        // 自己那一份 —— `scan_tree!` 摘掉了它，而它正是被测对象。
        out.push((
            "dial/mod.rs".to_string(),
            crate::guard_support::production_code(include_str!("mod.rs")),
        ));
        out.sort();
        out.dedup_by(|a, b| a.0 == b.0);
        out
    }

    /// ★ 乙半：**拨号只许住 `dial/`，而且必须真的有一处。**
    #[test]
    fn the_dial_only_happens_under_dial_home() {
        let corpus = crate_sources();
        assert!(
            corpus.len() >= 37,
            "只扫到 {} 份源文件 —— 抽取坏了（08-06 实测 37 份）",
            corpus.len()
        );
        match dial_locality(&corpus) {
            Ok(n) => assert!(
                n >= 1,
                "锚点数 {n} —— 不该走到这里，`dial_locality` 自己会拦"
            ),
            Err(e) => panic!("{e}"),
        }
    }

    /// ★ **这条代理真的接得到 —— 不只是「代码住在这儿」。**
    ///
    /// 🔴 **它是本轮 `7u` 探针逮出来的一格，不是设计时想到的。**
    /// 实打：把 `main.rs` 里那条 `--dial` 分派臂**整条摘掉**（`dial/` 的代码一个字不动），
    /// daemon 侧 **596 条判据一条都不红** —— 上面那条 `the_dial_only_happens_under_dial_home`
    /// 断的是**住在哪**（locality），断不了**接没接上**（reachability），
    /// 而 `argv_table_guard` 那条只看「存在的臂调不调实现」，摘掉的臂它看不见。
    ///
    /// 那时的运行期形状：`--dial` 还在 `SUBCOMMANDS` 里 ⇒ `is_query_mode` 判真 ⇒
    /// 落进 `_` 臂走历史查询 ⇒ `unknown argument` + exit 2。
    /// **那正是 v3.4.0 `--account-trust-zero` 漏登记那次事故的形状**（只不过方向反过来：
    /// 那次是表里漏、这次是臂里漏）。端到端不静默（界面会看到「代理一个字节都没回」），
    /// 但**源码级的这次退化没有任何判据拦得住** —— 本条就是补上的那一刀。
    #[test]
    fn the_dial_arm_is_actually_wired_into_the_dispatch() {
        let main_prod = crate::guard_support::production_code(include_str!("../main.rs"));
        assert!(
            main_prod.len() > 3_000,
            "剥完 main.rs 生产段只剩 {} 字节 —— 剥法坏了，本条此刻在空转",
            main_prod.len()
        );
        // 运行时拼，免得本模块自己的文本被别的扫描器命中。
        let flag = format!("{}{}", "\"--", "dial\"");
        let call = format!("dial::{}(", "run");
        assert_eq!(
            main_prod.matches(flag.as_str()).count(),
            2,
            "`main.rs` 生产段里 `{flag}` 不是恰好 **2** 处。\
             那 2 处各有各的活，缺一个后果都不一样：\
             ① `SUBCOMMANDS` 那张表 —— `is_query_mode` 的闸门读它，不在表里就被当未知 flag \
             **静默进流模式**（v3.4.0 `--account-trust-zero` 那次事故的形状）；\
             ② `match` 那条分派臂 —— 不在就落进 `_` 臂走历史查询、`unknown argument` + exit 2。\
             ≥3 处 ⇒ 有第三个地方在认这个 token，先说清那是谁。"
        );
        assert_eq!(
            main_prod.matches(call.as_str()).count(),
            1,
            "`main.rs` 生产段里 `{call}` 不是恰好 1 处 —— \
             **分派臂被摘掉了**：代理的代码还在 `dial/`，但没有任何人调得到它。\
             那时 `--dial` 会落进 `_` 臂走历史查询，`unknown argument` + exit 2。"
        );
        // 反向自检：把臂那一行从语料里剔掉，本条必须红（否则它在测「文本里有这个词」）。
        let without_arm = main_prod.replace(call.as_str(), "nothing_at_all(");
        assert_eq!(
            without_arm.matches(call.as_str()).count(),
            0,
            "剔不掉那条臂 —— 本条的反向自检此刻是空转的"
        );
    }

    /// ★ 反向自检 · **阳性方向**：把一处锚点放到 `dial/` 外面去，判据必须红**并点名**。
    ///
    /// 语料是**活体**（真语料 + 一处注入），不是手搓的小串 —— 手搓的小串过不了字节地板，
    /// 于是会以「空转」而不是「越界」红掉，那就测不到该测的那一条。
    #[test]
    fn a_dial_anchor_outside_dial_home_is_caught_and_named() {
        let mut corpus = crate_sources();
        let victim = "observe/watcher.rs".to_string();
        let injected = format!(
            "fn f() {{ let _ = {}(a, b, c); }}\n",
            anchors()[2].trim_end_matches('(')
        );
        let slot = corpus
            .iter_mut()
            .find(|(n, _)| *n == victim)
            .unwrap_or_else(|| panic!("语料里没有 {victim} —— 反例的宿主没了，换一个"));
        slot.1.push_str(&injected);

        let e = dial_locality(&corpus).expect_err("锚点长到 dial/ 外面了，判据居然是绿的");
        assert!(
            e.contains(&victim),
            "判据红了，但**没点名是哪一处** —— 只说「有问题」的诊断等于没有诊断。实得：{e}"
        );
    }

    /// ★ 反向自检 · **阴性方向**（`P6bM4`）：喂空语料，判据必须**自己先红**。
    ///
    /// 少了这一条，「一处锚点都没有」与「语料压根没喂进来」在终端上同形，
    /// 而后者会让这条判据在**任何**改动下都保持绿。
    #[test]
    fn an_empty_corpus_makes_the_judge_red_by_itself() {
        let e = dial_locality(&[]).expect_err("空语料居然判绿 —— 地板断言没接上");
        assert!(
            e.contains("空转"),
            "空语料红了，但红的理由不是「空转」——那说明它是被别的分支拦下的，地板没生效。实得：{e}"
        );
        // 再补一刀：语料非空但**远小于**地板，同样要以「空转」红。
        let tiny = vec![("dial/mod.rs".to_string(), "x".repeat(10))];
        let e2 = dial_locality(&tiny).expect_err("小语料居然判绿");
        assert!(e2.contains("空转"), "小语料红的理由不对：{e2}");
    }

    /// 地板不是摆设：把真语料**减到**地板以下，判据也要红。
    ///
    /// 这一条与上一条不同 —— 上一条喂的是构造语料，这一条证明地板**对真语料同样有效**
    /// （防「地板设得比真语料还小很多，等于没有」那一形）。
    #[test]
    fn the_corpus_floor_is_not_far_below_the_real_corpus() {
        let real: usize = crate_sources().iter().map(|(_, c)| c.len()).sum();
        assert!(
            real > CORPUS_FLOOR_BYTES,
            "真语料 {real} 字节，还没到地板 {CORPUS_FLOOR_BYTES} —— 地板设高了，判据恒红"
        );
        assert!(
            real < CORPUS_FLOOR_BYTES * 40,
            "真语料 {real} 字节，是地板 {CORPUS_FLOOR_BYTES} 的 40 倍以上 —— \
             地板设得太低，掉掉大半个 crate 它也不会响。**这是一条会随 crate 长大而变松的判据**，\
             长到这一条红的那天，把地板抬上去。"
        );
    }

    /// `--dial` 的请求行按**蛇形键**读。这条钉的是两端的字段名对得上。
    ///
    /// ⚠ 它只钉本侧的读法；界面那侧写的是什么，由 `ssh_source` 的判据钉。
    /// **两侧各钉一半** —— 与 `build_id_guard` / `protocol_doc_guard` 的分工同形。
    #[test]
    fn the_request_line_is_read_with_snake_case_keys() {
        let raw = r#"{"host":"h","port":22,"user":"u","key_path":"/k","host_key_fingerprint":"SHA256:x","command":"c"}"#;
        let req: DialRequest = serde_json::from_str(raw).expect("蛇形键的请求行读不动");
        assert_eq!(req.host, "h");
        assert_eq!(req.port, 22);
        assert_eq!(req.user, "u");
        assert_eq!(req.key_path.as_deref(), Some("/k"));
        assert_eq!(req.host_key_fingerprint.as_deref(), Some("SHA256:x"));
        assert_eq!(req.command, "c");
        // 反向：camelCase 读不动 —— 免得哪天有人「顺手」改成 camelCase 而两端悄悄漂开。
        let camel = r#"{"host":"h","port":22,"user":"u","keyPath":"/k","command":"c"}"#;
        let r2: DialRequest = serde_json::from_str(camel).expect("多余字段应当被忽略");
        assert!(
            r2.key_path.is_none(),
            "camelCase 的 `keyPath` 居然被读进来了 —— 两端的键名契约不是蛇形独占"
        );
    }

    /// ack 一定是**一行**，而且以 `\n` 收尾 —— 界面在 `read_line` 上等着它。
    #[tokio::test]
    async fn the_ack_is_exactly_one_newline_terminated_line() {
        let mut buf: Vec<u8> = Vec::new();
        write_ack(
            &mut buf,
            &DialAck {
                ok: true,
                error: None,
                fingerprint: Some("SHA256:x".into()),
            },
        )
        .await
        .expect("写 ack 失败");
        let s = String::from_utf8(buf).expect("ack 不是 utf8");
        assert_eq!(s.matches('\n').count(), 1, "ack 不是一行：{s:?}");
        assert!(s.ends_with('\n'), "ack 没有以换行收尾：{s:?}");
        let v: serde_json::Value = serde_json::from_str(s.trim()).expect("ack 不是合法 JSON");
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
    }
}
