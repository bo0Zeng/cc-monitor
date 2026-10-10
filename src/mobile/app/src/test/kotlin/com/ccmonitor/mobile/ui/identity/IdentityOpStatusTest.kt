package com.ccmonitor.mobile.ui.identity

import com.ccmonitor.mobile.core.data.crypto.CryptoBox
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.IdentityDao
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
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
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** 身份操作反馈状态机——成功→Done、repo 抛异常→Failed（不许 fire-and-forget 静默）。 */
@OptIn(ExperimentalCoroutinesApi::class)
class IdentityOpStatusTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setMain() = Dispatchers.setMain(dispatcher)

    @After fun reset() = Dispatchers.resetMain()

    @Test
    fun importSuccessEmitsDone() =
        runTest(dispatcher) {
            // 垃圾 PEM → fromPrivatePem 回退 null（不抛）→ 身份照存 → Done。
            val vm = IdentityViewModel(IdentityRepository(RecordingDao(), NoopCrypto), io = dispatcher)
            vm.importPrivateKey("k", "not-a-real-pem".toByteArray())
            advanceUntilIdle()
            assertTrue("成功应 Done", vm.op.value is IdentityOp.Done)
        }

    @Test
    fun repoFailureEmitsFailed() =
        runTest(dispatcher) {
            val vm = IdentityViewModel(IdentityRepository(ThrowingDao(), NoopCrypto), io = dispatcher)
            vm.importPrivateKey("k", "not-a-real-pem".toByteArray())
            advanceUntilIdle()
            assertTrue("异常应 Failed（不静默）", vm.op.value is IdentityOp.Failed)
        }

    @Test
    fun ackOpResetsToIdle() =
        runTest(dispatcher) {
            val vm = IdentityViewModel(IdentityRepository(RecordingDao(), NoopCrypto), io = dispatcher)
            vm.importPrivateKey("k", "x".toByteArray())
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

    override suspend fun upsert(identity: Identity) {
        // no-op fake：成功落库
    }

    override suspend fun delete(identity: Identity) {
        // no-op fake
    }
}

private class ThrowingDao : RecordingDao() {
    override suspend fun upsert(identity: Identity): Unit = error("db boom")
}
