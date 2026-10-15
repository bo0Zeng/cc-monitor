package com.ccmonitor.mobile.dev

import android.content.Context
import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.data.db.CustomButtonDao
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext

/**
 * 只在 debug 构建用：首启没有主机时，把 assets 里的 dev key 导成身份，并建一台 NanoPi 主机和几条默认按钮。
 */
class DevSeeder(
    private val context: Context,
    private val hostRepo: HostRepository,
    private val identityRepo: IdentityRepository,
    private val customButtonDao: CustomButtonDao,
) {
    suspend fun seedIfEmpty() =
        withContext(Dispatchers.IO) {
            if (hostRepo.observeAll().first().isNotEmpty()) return@withContext
            val pem =
                runCatching {
                    context.assets.open("dev_nanopi_key").use { it.readBytes() }
                }.getOrNull() ?: return@withContext

            val identityId = "dev-nanopi-key"
            identityRepo.save(
                Identity(
                    id = identityId,
                    label = "dev NanoPi key",
                    keyType = "imported",
                    privateKeyEnc = ByteArray(0),
                    publicKeyOpenSsh = "",
                    fingerprintSha256 = "",
                    createdAt = System.currentTimeMillis(),
                ),
                plainPrivateKey = pem,
            )
            hostRepo.save(
                Host(
                    id = "dev-nanopi",
                    label = "NanoPi",
                    host = "192.0.2.2",
                    port = 22,
                    username = "pi",
                    authRef = identityId,
                    // 额外配一个不可达地址，用来看多地址竞速：先连通者胜，不等死地址超时。
                    extraAddresses = "203.0.113.1:22",
                ),
            )
            // 默认全局一键按钮（hostId=null）。
            listOf(
                CustomButton("btn-tmux", null, "tmux", "tmux new -A -s main", 0),
                CustomButton("btn-ls", null, "ls", "ls -la", 1),
                CustomButton("btn-git", null, "git", "git status", 2),
                CustomButton("btn-clr", null, "clr", "clear", 3),
            ).forEach { customButtonDao.upsert(it) }
        }
}
