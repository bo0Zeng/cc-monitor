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
