//! `K-P6b` 候选 E 的**代理进程**：`--dial` —— 把后端那条长连接流的拨号搬出界面进程。
//!
//! # 🔴 第一行就写死这一件事买到了多少（`D2` 改窄后的原话，别读大）
//!
//! 本文件买到的是：**`backend 那条长连接流` 的那一跳 SSH 握手，发生在这个进程里，
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
//!     ① `local_backend.rs::LOCAL_BACKEND_STEM` 逐字 `pub const LOCAL_BACKEND_STEM: &str = "cc-monitor-remote";`；
//!     ② `src/bridge/tauri.sidecar.conf.json` 逐字 `"externalBin": ["binaries/cc-monitor-remote"]`
//!        —— **同一个名字**；
//!     ③ `.github/workflows/release.yml` 的 `build-windows` 里两步 ——
//!        `Build local backend (native)`（`working-directory: src/backend`
//!        · `cargo build --release`）＋ `Stage local backend for externalBin`
//!        （拷成 `cc-monitor-remote-<triple>.exe`），随后
//!        `npx tauri build --config src/bridge/tauri.sidecar.conf.json`；
//!        `build-linux` 那个 job 三步同形；
//!     ④ 同一份 workflow 的那段头注逐字写着 backend **在 Windows 上真编得过**
//!        （2026-08-04 真机实测 exit=0、release 2.6 MB、跑起来发完整 hello）。
//!     ⇒ 装完之后本机后端就在 `monitor.exe` 旁边，`local_backend::resolve_beside_this_exe` 命中。
//!   - **开发树上走不到。** `externalBin` **不住 `tauri.conf.json`**，它住那份单独的
//!     `tauri.sidecar.conf.json`、只在发版那一步注入 ⇒ `cargo run` / `npm run tauri dev`
//!     两处都空 ⇒ **回落到进程内拨号**。
//!   🔴 **我第一版把后者写成了全称**，理由正是本仓那条老病：**查的是开发树，
//!   得到的是一个只在开发树上为真的答案**（`src/bridge/src/local_accounts.rs` 里
//!   那条登记自己就写着同一句）。**这条边界的射程是「开发树」，不是「默认装机」。**
//! - ⚠ **另一条真会让它回落的**：本代理只会 **publickey** 一种鉴权
//!   ⇒ 配置里没填 `keyPath`（Windows 上走 ssh-agent 那一档）时，界面**不走代理**。
//!   两条回落条件都由 `ssh_source` 那侧的判据登记着，不是散文。
//!
//! # 为什么是 E（`audits/K-P6b-PM.md §一`，别在这里重推一遍）
//!
//! 第一轮正面比过 E 与 B：B **少一个进程**（Windows 上本机后端本来就是界面的子进程，
//! local_backend 就在 `monitor.exe` 旁边），代价是**动协议面**（实打：给 `Frame::Overflow`
//! 加一个字段要改 5 处才编得过）＋ **碰第 4 根针**（`observe/watcher.rs` 的那条观测通道诞生点）。
//!
//! ⚠ **第一轮还写过「B 会动 `inbound.rs`，那是红线」—— 那句话 PM 撤掉了**：
//! 那条红线是「**PM 持有这份文件、agent 不许改**」，管的是**谁动手**，不是这个设计成不成立。
//! **别再引用它当否决理由。**
//!
//! ⇒ E 站得住靠的是件计划 `§0` 自己的取舍标准，逐字「**不是它最好，是它最容易反悔**」：
//! **E 改的是进程数**（不喜欢就去掉那个进程，线上跑着的后端一个都不用换）·
//! **B 改的是线上格式**（发出去之后新老后端的兼容面就固化了，反悔要带一次协议迁移）。
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
//!              stdin   **全部**是原始字节，原样写进 SSH channel（backend 的 stdin）
//! 代理 → 界面  stdout  第一行：一行 JSON = DialAck，`\n` 结尾
//!                      其后：原始字节，原样来自 SSH channel（backend 的 stdout）
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
//! backend 那条通道，一个字节的带外数据都没有，反而更干净。
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

/// 请求读不成 ⇒ 调用错误。与后端别处「一次性查询」的 `exit 2` 同族。
pub const EXIT_BAD_REQUEST: i32 = 2;
/// 请求读得懂但拨不通（TCP / 指纹 / 鉴权 / exec 任一步失败）。
pub const EXIT_DIAL_FAILED: i32 = 3;

/// 界面交给代理的那一行 JSON。
///
/// 字段是 `RemoteConfig` 的**子集**，故意不整份搬：本进程不需要 `label` / `addresses` /
/// `jump`（那几样都是界面侧的上游决策，见头注「边界」）。
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
///  （前三份：`src/doc/IPC-PROTOCOL.md` §10 两处 + `src/backend/main.rs` 的 `SUBCOMMANDS` 表）。
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
    // 下行结束 = 远端后端走了；上行结束 = 界面走了（`D3③` 那一句的落点）。
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
#[path = "../../../tests/backend/dial_tests.rs"]
mod tests;
