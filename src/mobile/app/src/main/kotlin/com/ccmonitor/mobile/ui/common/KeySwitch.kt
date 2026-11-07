package com.ccmonitor.mobile.ui.common

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.crypto.SshKeyGenerator
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.link.HostBackends
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import org.koin.compose.koinInject
import java.util.UUID

/**
 * 机器条上［换成密钥］那一下（那台连着、这一条连接用的是密码，判在核心 `deploy::machine::signed_in`）：
 * 这部手机没有钥匙就生成一把硬件钥匙（有就用最早那一把）→ 请那台把公钥并进 `authorized_keys`（[push]，那台写）→
 * 这台记上那把钥匙（存的密码留着：撤销 ＝ 摘掉钥匙、回到密码；清掉存的密码是另一件、撤不回的事）。
 * 认证分派里记着钥匙先用钥匙（`resolveHostAuth`），下一次连上起照它登。
 */
class KeySwitch(
    /** 机器表里那一台（[HostRepository.get]）。 */
    private val hostOf: suspend (hostId: String) -> Host?,
    /** 存回机器表（[HostRepository.save]）。 */
    private val save: suspend (Host) -> Unit,
    /** 这部手机上的钥匙（[IdentityRepository.observeAll] 此刻那一份）。 */
    private val keys: suspend () -> List<Identity>,
    /** 请那台把这一行公钥并进 `authorized_keys`；回那台写好的那一句（没成 ⇒ 失败，那一句）。 */
    private val push: suspend (hostId: String, publicKey: String) -> Result<String>,
    /** 这部手机一把钥匙都没有时生成一把（存好、交回）。 */
    private val generate: suspend () -> Identity,
) {
    /** 换的结局：成了（用的是哪一把）· 没成（那一句 ＋ 详情）。 */
    sealed interface Outcome {
        data class Switched(
            val identityId: String,
        ) : Outcome

        data class Failed(
            val notice: Notice,
        ) : Outcome
    }

    /** 换成钥匙。那台已不在表里（条还在屏上时被删了）⇒ 没成。 */
    suspend fun switch(hostId: String): Outcome {
        val host = hostOf(hostId) ?: return Outcome.Failed(Notice(copyText("mobKeyPush.result.failed")))
        val key = keys().minByOrNull { it.createdAt } ?: generate()
        if (key.publicKeyOpenSsh.isBlank()) return Outcome.Failed(Notice(copyText("mobKeyPush.result.noPublic")))
        push(hostId, key.publicKeyOpenSsh).onFailure { return Outcome.Failed(Notice(copyText("mobKeyPush.result.failed"), it.message)) }
        save(host.copy(authRef = key.id))
        return Outcome.Switched(key.id)
    }

    /** 撤销：摘掉那把钥匙，回到存着的密码（钥匙留在这部手机上）。 */
    suspend fun undo(hostId: String) {
        hostOf(hostId)?.let { save(it.copy(authRef = null)) }
    }
}

/** 生成一把硬件钥匙（私钥留在 Keystore、出不了这部手机）并存为身份。钥匙屏的「新建 · 硬件」与［换成密钥］共用这一份。 */
suspend fun newHardwareIdentity(
    repo: IdentityRepository,
    label: String,
): Identity {
    val alias = "ccm-key-" + UUID.randomUUID()
    val key = SshKeyGenerator.generateEcdsaP256InKeystore(alias, comment = label.ifBlank { "cc-monitor" })
    val identity =
        Identity(
            id = alias,
            label = label.ifBlank { "key" },
            keyType = "ecdsa-p256",
            privateKeyEnc = ByteArray(0),
            publicKeyOpenSsh = key.publicKeyOpenSsh,
            fingerprintSha256 = key.fingerprintSha256,
            createdAt = System.currentTimeMillis(),
            keystoreAlias = alias,
        )
    repo.saveKeystoreBacked(identity)
    return identity
}

/** 机器条上［换成密钥］的那一按：结局走底部条（成了带［撤销］；没成照那一句、带复制详情）。 */
@Composable
fun rememberKeySwitch(hostId: String): () -> Unit {
    val hosts = koinInject<HostRepository>()
    val identities = koinInject<IdentityRepository>()
    val backends = koinInject<HostBackends>()
    val snackbar = LocalAppSnackbar.current
    val scope = rememberCoroutineScope()
    val switch =
        remember(hostId) {
            KeySwitch(
                hostOf = hosts::get,
                save = { hosts.save(it) },
                keys = { identities.observeAll().first() },
                push = { id, key -> backends.terminals(id).pushKey(key) },
                generate = { newHardwareIdentity(identities, "") },
            )
        }
    return {
        scope.launch {
            when (val o = switch.switch(hostId)) {
                is KeySwitch.Outcome.Switched -> snackbar.showUndo(copyText("beMachine.done.useKey")) { switch.undo(hostId) }
                is KeySwitch.Outcome.Failed -> snackbar.show(o.notice)
            }
        }
    }
}
