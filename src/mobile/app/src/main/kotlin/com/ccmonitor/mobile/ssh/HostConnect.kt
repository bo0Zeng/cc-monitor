package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.copy.copyText
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
 * 终端和文件屏共用，免得两处构造字段走样。
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
            reconnect = ReconnectPolicy(auto = autoReconnect),
            // 工作目录和 tmux 由 [initialCommands] 在 shell 通道里发，不走 ConnectionConfig。
        )
    }

/**
 * 跳板设置坏了（跳板机不在了 · 跳板机无密钥或密码），拒连。[fixHostId] 是改哪一台能修好：界面据它给［编辑］。
 */
class JumpBroken(
    message: String,
    val fixHostId: String,
) : IllegalStateException(message)

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
    // 还会把「跳板坏了」报成「目标断开」。
    val jumpHost = hostRepo.get(jumpId) ?: throw JumpBroken(copyText("mobConnect.jump.missing"), host.id)
    // 跳板身份或密码有一样即可；都没有同样拒连。
    if (jumpHost.authRef == null && jumpHost.passwordEnc == null) {
        throw JumpBroken(copyText("mobConnect.jump.noAuth", "label" to jumpHost.label), jumpHost.id)
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
    val identity = identityRepo.get(authRef) ?: error(copyText("mobConnect.identity.missing", "id" to authRef))
    return if (identity.isKeystoreBacked()) {
        AuthMethod.KeystoreSigner(requireNotNull(identity.keystoreAlias))
    } else {
        AuthMethod.PrivateKey(identityRepo.privateKey(authRef) ?: error(copyText("mobConnect.identity.noKey")))
    }
}

/**
 * 主机认证分派的唯一来源，终端和文件屏都走它：有密码（`passwordEnc`）先用密码，
 * 否则按身份（`authRef`，或本次连接临时选的 [pendingIdentityId]）走 [resolveAuthMethod]。
 */
suspend fun resolveHostAuth(
    host: Host,
    pendingIdentityId: String?,
    identityRepo: IdentityRepository,
    hostRepo: HostRepository,
): AuthMethod {
    hostRepo.password(host)?.let { return AuthMethod.Password(it) }
    val ref = host.authRef ?: pendingIdentityId ?: error(copyText("mobConnect.host.noAuth"))
    return resolveAuthMethod(ref, identityRepo)
}

/**
 * 连上之后在终端 shell 通道里依次要发的初始命令（只开一个 shell，不跑别的：起会话走那台的别名，见终端屏「用哪个启动」）。
 *
 * - [workingDir] 非空 → 必定 cd 过去（优先于 defaultWorkingDir，如「在文件屏当前目录打开终端」）。tmux 下先 `tmux new -A … [-c defWd]`
 *   再 `cd <wd>`，因为 attach 到已有会话时 `-c` 被忽略。
 * - 没有 workingDir：tmux → `tmux new -A …`；否则有 defWd 就 `cd <defWd>`；都没有 → 空。
 *
 * 这些照「用户在 ssh 里自己敲」执行：就是在那个 shell 里跑一条命令（[SessionBackend.enter]），不另拼 tmux 管理逻辑。
 */
fun Host.initialCommands(
    workingDir: String? = null,
    tmuxSession: String? = null,
): List<String> {
    val dialect = dialectFor(os)
    val override = workingDir?.trim()?.takeIf { it.isNotEmpty() }
    val defWd = defaultWorkingDir?.trim()?.takeIf { it.isNotEmpty() }
    // 没有会话后端（Windows）就不进 tmux 分支，否则首条 `tmux new -A` 会卡到超时。
    val backend = dialect.sessionBackend
    val session = if (backend != null) resolveTmuxSession(tmuxSession) else null
    return when {
        session != null && backend != null ->
            buildList {
                add(backend.enter(session, defWd))
                override?.let { add(dialect.cd(it)) } // attach 后再 cd
            }
        override != null -> listOf(dialect.cd(override))
        defWd != null -> listOf(dialect.cd(defWd))
        else -> emptyList()
    }
}

/**
 * 本次启动用哪个 tmux 会话名，与 [initialCommands] 同源：显式给的（重连回到上次所在的那个）
 * > 主机开了 auto-tmux（[DEFAULT_TMUX_SESSION]）> null（不进 tmux）。
 */
fun Host.resolveTmuxSession(tmuxSession: String? = null): String? =
    tmuxSession?.trim()?.takeIf { it.isNotEmpty() }
        ?: if (sessionManager == "tmux") DEFAULT_TMUX_SESSION else null
