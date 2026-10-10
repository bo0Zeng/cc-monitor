package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.endpoints
import com.ccmonitor.mobile.core.data.db.isKeystoreBacked
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.ssh.AuthMethod
import com.ccmonitor.mobile.core.ssh.ConnectionConfig
import com.ccmonitor.mobile.core.ssh.ReconnectPolicy

/**
 * 把 [Host] 的全部可连地址（[endpoints]）展开成竞速用的 [ConnectionConfig] 列表。
 * 放在 app：只有这一层同时看得到 core-data 的 [Host] 和 core-ssh 的 [ConnectionConfig]。
 * 终端和 SFTP 共用，免得两处构造字段走样。
 */
fun Host.toConnectionConfigs(
    auth: AuthMethod,
    jumpVia: ConnectionConfig? = null, // 跳板链（见 resolveJumpChain），各地址共用
): List<ConnectionConfig> =
    endpoints().map { ep ->
        ConnectionConfig(
            host = ep.host,
            port = ep.port,
            username = username,
            auth = auth,
            jumpVia = jumpVia,
            reconnect =
                ReconnectPolicy(
                    auto = autoReconnect,
                    onNetworkChange = reconnectOnNetworkChange,
                    maxAttempts = maxReconnectAttempts,
                ),
            // 工作目录和 tmux 由 [initialCommands] 在 shell 通道里发，不走 ConnectionConfig。
        )
    }

/**
 * 把 [Host.proxyJumpHostId] 递归解析成跳板 [ConnectionConfig] 链（ssh -J）；null = 直连。
 * 每跳身份走 [resolveHostAuth]；跳板只取首地址，不参与多地址竞速。[visited] 防环（A→B→A）。
 */
suspend fun resolveJumpChain(
    host: Host,
    hostRepo: HostRepository,
    identityRepo: IdentityRepository,
    visited: Set<String> = emptySet(),
): ConnectionConfig? {
    // 没配跳板、自环、成环截断 → 直连。
    val jumpId = host.proxyJumpHostId?.trim()?.takeIf { it.isNotEmpty() && it != host.id && it !in visited } ?: return null
    // 配了跳板却解析不了（跳板主机没了、没身份）→ 拒连并报错，绝不静默直连：那会绕过跳板这道安全边界，
    // 还会把「跳板坏了」报成「目标连不上」。
    val jumpHost = hostRepo.get(jumpId) ?: error("跳板主机不存在（已被删除？proxyJumpHostId=$jumpId）——已拒绝直连以防绕过跳板，请修复该主机的跳板设置。")
    // 跳板身份或密码有一样即可；都没有同样拒连。
    if (jumpHost.authRef == null && jumpHost.passwordEnc == null) {
        error("跳板「${jumpHost.label}」未关联身份、也未设密码——已拒绝直连以防绕过跳板，请给跳板主机绑定身份或设置密码。")
    }
    val ep = jumpHost.endpoints().first()
    return ConnectionConfig(
        host = ep.host,
        port = ep.port,
        username = jumpHost.username,
        auth = resolveHostAuth(jumpHost, null, identityRepo, hostRepo),
        // 加入 visited 的是当前 host 而不是 jumpId：A→B→A 时解析 B 的跳板 A 会命中 visited，截成 A→B。
        jumpVia = resolveJumpChain(jumpHost, hostRepo, identityRepo, visited + host.id),
    )
}

/**
 * 按身份类型解析认证方式。先判类型再决定解不解密：Keystore 硬件身份没有私钥字节，走 [AuthMethod.KeystoreSigner]；
 * 导入或软件生成的身份解密私钥走 [AuthMethod.PrivateKey]。
 */
suspend fun resolveAuthMethod(
    authRef: String,
    identityRepo: IdentityRepository,
): AuthMethod {
    val identity = identityRepo.get(authRef) ?: error("身份不存在: $authRef")
    return if (identity.isKeystoreBacked()) {
        AuthMethod.KeystoreSigner(requireNotNull(identity.keystoreAlias))
    } else {
        AuthMethod.PrivateKey(identityRepo.privateKey(authRef) ?: error("身份私钥为空"))
    }
}

/**
 * 主机认证分派的唯一来源，终端和 SFTP 都走它：有密码（`passwordEnc`）先用密码，
 * 否则按身份（`authRef`，或本次连接临时选的 [pendingIdentityId]）走 [resolveAuthMethod]。
 */
suspend fun resolveHostAuth(
    host: Host,
    pendingIdentityId: String?,
    identityRepo: IdentityRepository,
    hostRepo: HostRepository,
): AuthMethod {
    hostRepo.password(host)?.let { return AuthMethod.Password(it) }
    val ref = host.authRef ?: pendingIdentityId ?: error("主机未关联身份，也未设密码认证")
    return resolveAuthMethod(ref, identityRepo)
}

/**
 * 连上之后在终端 shell 通道里依次要发的初始命令。
 *
 * - [LaunchSpec.workingDir] 非空 → 必定 cd 过去（优先于 defaultWorkingDir）。tmux 下先 `tmux new -A … [-c defWd]`
 *   再 `cd <wd>`，因为 attach 到已有会话时 `-c` 被忽略。
 * - 没有 workingDir：tmux → `tmux new -A …`；否则有 defWd 就 `cd <defWd>`；都没有 → 空。
 * - [LaunchSpec.command] 非空 → 追加在最后。
 *
 * 移动端触摸、状态栏设置由 [SessionBackend.enter] 拼在建会话之后、attach 之前，首连就生效。
 * 不另开 exec 去设：Connected 一出现 exec 通道可能先于 pty 里的 tmux 建会话到达，那时没有 server，设置会静默丢。
 */
fun Host.initialCommands(spec: LaunchSpec = LaunchSpec()): List<String> {
    val dialect = dialectFor(os)
    val override = spec.workingDir?.trim()?.takeIf { it.isNotEmpty() }
    val defWd = defaultWorkingDir?.trim()?.takeIf { it.isNotEmpty() }
    // 没有会话后端（Windows）就不进 tmux 分支，否则首条 `tmux new -A` 会卡到超时。
    val backend = dialect.sessionBackend
    val tmuxSession = if (backend != null) resolveTmuxSession(spec) else null
    val base =
        when {
            tmuxSession != null && backend != null ->
                buildList {
                    add(backend.enter(tmuxSession, defWd))
                    override?.let { add(dialect.cd(it)) } // attach 后再 cd
                }
            override != null -> listOf(dialect.cd(override))
            defWd != null -> listOf(dialect.cd(defWd))
            else -> emptyList()
        }
    val extra = spec.command?.trim()?.takeIf { it.isNotEmpty() }
    return if (extra != null) base + extra else base
}

/**
 * 本次启动用哪个 tmux 会话名，与 [initialCommands] 同源：launcher 显式指定（[LaunchSpec.tmuxSession]）
 * > 主机开了 auto-tmux（[DEFAULT_TMUX_SESSION]）> null（不进 tmux）。所以 cct 这类 launcher 在主机没开 auto-tmux 时也进 tmux。
 */
fun Host.resolveTmuxSession(spec: LaunchSpec): String? =
    spec.tmuxSession?.trim()?.takeIf { it.isNotEmpty() }
        ?: if (sessionManager == "tmux") DEFAULT_TMUX_SESSION else null
