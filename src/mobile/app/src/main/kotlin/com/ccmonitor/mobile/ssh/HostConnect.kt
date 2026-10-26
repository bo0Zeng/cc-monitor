package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.isKeystoreBacked
import com.ccmonitor.mobile.core.data.db.machineNames
import com.ccmonitor.mobile.core.data.db.machineTable
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.ssh.AuthMethod
import com.ccmonitor.mobile.core.ssh.ConnectionConfig
import com.ccmonitor.mobile.core.ssh.ReconnectPolicy
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.DialPlan

/**
 * 跳板设置坏了（成环 · 跳板机不在了 · 跳板机无密钥或密码 · 那一台不成立），拒连。[fixHostId] 是改哪一台能修好：界面据它给［编辑］。
 */
class JumpBroken(
    message: String,
    val fixHostId: String,
) : IllegalStateException(message)

/** 照这一端的机器表组 [target] 那一台的拨号（组法在拨号核心：地址展开 · 多跳跳板 · 成环与找不到就拒）。 */
fun interface DialPlanner {
    fun plan(
        hosts: List<Host>,
        target: Host,
    ): DialPlan
}

/** 生产那一份：名字与组法都问手机嵌的核心（`Core.machines` · `Core.dialPlan`）。 */
val CoreDialPlanner =
    DialPlanner { hosts, target ->
        val name = machineNames(hosts)[hosts.indexOfFirst { it.id == target.id }]
        Wire.read<DialPlan>(Core.dialPlan(hosts.machineTable(), name)) ?: error("wire contract mismatch: dial plan")
    }

/**
 * [host] 那一台的连接配置：核心照机器表（[hosts]，库里那一份；[host] 替掉库里同 id 的那一行）组好的那一份请求 ＋ 路上每一台的认证
 * （请求里每一跳写着那台的 id，认证照它解）。核心拒了 ⇒ [JumpBroken]（改这一台）；路上的跳板机没身份也没密码 ⇒ [JumpBroken]（改那一台）——
 * 绝不静默直连：那会绕过跳板这道安全边界，还会把「跳板坏了」报成「目标断开」。
 */
suspend fun planDial(
    host: Host,
    hosts: List<Host>,
    pendingIdentityId: String?,
    identityRepo: IdentityRepository,
    hostRepo: HostRepository,
    planner: DialPlanner = CoreDialPlanner,
): ConnectionConfig {
    val table = hosts.map { if (it.id == host.id) host else it }.let { t -> if (t.any { it.id == host.id }) t else t + host }
    val request =
        when (val plan = planner.plan(table, host)) {
            is DialPlan.Ready -> plan.request
            is DialPlan.Refused -> throw JumpBroken(plan.said, host.id)
        }
    val byId = table.associateBy { it.id }
    val hops = listOf(request.keyPath) + generateSequence(request.jump) { it.jump }.map { it.keyPath }.toList()
    val auths =
        hops.filterNotNull().associateWith { id ->
            val h = hopHost(byId[id], host)
            resolveHostAuth(h, pendingIdentityId.takeIf { h.id == host.id }, identityRepo, hostRepo)
        }
    return ConnectionConfig(request, auths, ReconnectPolicy(auto = host.autoReconnect))
}

/** 请求里那一跳写的那一台：表里没有 ⇒ 拒（改 [target]）；是跳板却没身份也没密码 ⇒ 拒（改那一台）。 */
private fun hopHost(
    h: Host?,
    target: Host,
): Host {
    val hop = h ?: throw JumpBroken(copyText("mobConnect.jump.missing"), target.id)
    if (hop.id != target.id && hop.authRef == null && hop.passwordEnc == null) {
        throw JumpBroken(copyText("mobConnect.jump.noAuth", "label" to hop.label), hop.id)
    }
    return hop
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
