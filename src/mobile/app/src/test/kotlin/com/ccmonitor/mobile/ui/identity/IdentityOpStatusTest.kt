package com.ccmonitor.mobile.ui.identity

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.crypto.CryptoBox
import com.ccmonitor.mobile.core.data.crypto.SshKeyGenerator
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.IdentityDao
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.ssh.PublicKeyDeriver
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** 身份操作反馈状态机——成功→Done、认不出的私钥当场拒（不存）、repo 抛异常→Failed（不许 fire-and-forget 静默）。 */
@OptIn(ExperimentalCoroutinesApi::class)
class IdentityOpStatusTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setMain() = Dispatchers.setMain(dispatcher)

    @After fun reset() = Dispatchers.resetMain()

    private val good = SshKeyGenerator.generateEd25519("t").privateKeyOpenSshPem

    /** 派生公钥的替身：只认整把 [good]（真解析在 Rust 核心里，判据在 core-ssh 的 androidTest `PublicKeyDeriverTest`）。 */
    private val derive: (ByteArray, String) -> PublicKeyDeriver.Derived? = { pem, _ ->
        if (pem.contentEquals(good)) PublicKeyDeriver.Derived("ssh-ed25519 AAAA t", "SHA256:fake", "ssh-ed25519") else null
    }

    @Test
    fun importSuccessEmitsDone() =
        runTest(dispatcher) {
            val dao = RecordingDao()
            val vm = IdentityViewModel(IdentityRepository(dao, NoopCrypto), io = dispatcher, derive = derive)
            vm.importPrivateKey("k", good)
            advanceUntilIdle()
            assertTrue("成功应 Done：${vm.op.value}", vm.op.value is IdentityOp.Done)
            assertEquals("认得出的那把存下、带着派生出的指纹", 1, dao.saved.size)
            assertTrue(
                dao.saved
                    .single()
                    .fingerprintSha256
                    .startsWith("SHA256:"),
            )
        }

    /** 模拟器冒烟撞上的：逐行贴私钥只贴进了第一行，照样存成一把「imported」、不报错，连的时候才说认证方式用尽。 */
    @Test
    fun unreadableKeysAreRefusedAtImportAndNotSaved() =
        runTest(dispatcher) {
            val text = String(good)
            val firstLineOnly = text.lines().first() + "\n"
            val truncated = text.lines().take(3).joinToString("\n")
            for (pem in listOf(firstLineOnly, truncated, "not-a-real-pem", "")) {
                val dao = RecordingDao()
                val vm = IdentityViewModel(IdentityRepository(dao, NoopCrypto), io = dispatcher, derive = derive)
                vm.importPrivateKey("k", pem.toByteArray())
                advanceUntilIdle()
                assertEquals("「${pem.take(20)}」应当场拒", IdentityOp.Failed(copyText("mobIdentity.import.unreadable")), vm.op.value)
                assertTrue("拒了就不存：${pem.take(20)}", dao.saved.isEmpty())
            }
        }

    @Test
    fun repoFailureEmitsFailed() =
        runTest(dispatcher) {
            val vm = IdentityViewModel(IdentityRepository(ThrowingDao(), NoopCrypto), io = dispatcher, derive = derive)
            vm.importPrivateKey("k", good)
            advanceUntilIdle()
            assertTrue("异常应 Failed（不静默）", vm.op.value is IdentityOp.Failed)
        }

    @Test
    fun ackOpResetsToIdle() =
        runTest(dispatcher) {
            val vm = IdentityViewModel(IdentityRepository(RecordingDao(), NoopCrypto), io = dispatcher, derive = derive)
            vm.importPrivateKey("k", good)
            advanceUntilIdle()
            assertTrue(vm.op.value is IdentityOp.Done)
            vm.ackOp() // UI 消费后复位 → 防重组重复弹
            assertTrue("ackOp 后应 Idle", vm.op.value is IdentityOp.Idle)
        }
    // 注：不测"观察到 Working 中间态"——op 是 conflated StateFlow，Working→Done 可能被合并，晚订阅者不保证看到中间值（非 bug）。
}

private object NoopCrypto : CryptoBox {
    override fun encrypt(plain: ByteArray) = plain

    override fun decrypt(blob: ByteArray) = blob

    override fun isEncrypted(blob: ByteArray) = false
}

private open class RecordingDao : IdentityDao {
    override fun observeAll(): Flow<List<Identity>> = flowOf(emptyList())

    override suspend fun get(id: String): Identity? = null

    val saved = mutableListOf<Identity>()

    override suspend fun upsert(identity: Identity) {
        saved += identity
    }

    override suspend fun delete(identity: Identity) {
        // no-op fake
    }
}

private class ThrowingDao : RecordingDao() {
    override suspend fun upsert(identity: Identity): Unit = error("db boom")
}
