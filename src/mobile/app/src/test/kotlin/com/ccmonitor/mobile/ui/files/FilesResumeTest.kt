package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.KeptBatches
import com.ccmonitor.mobile.core.claude.link.ResidentCore
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.HostDao
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.IdentityDao
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ssh.HostConnector
import com.ccmonitor.mobile.testing.PlainCryptoBox
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * 杀了进程再开、进文件页：落过盘的那一组由常驻流交回核心接上，文件屏一开就照它最后那一格画浮卡。
 * 冒烟撞上过：视图模型初始化时同步把接回来的那一格交给浮卡，浮卡那一格的状态还没初始化 ⇒ 当场空指针、App 崩回桌面。
 *
 * 主线程按 `Main.immediate` 那样当场跑（[UnconfinedTestDispatcher]）；SSH 那层停在建连那一步不往下走（[StandardTestDispatcher] 不推进），
 * 只看接回那一条。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class FilesResumeTest {
    @Before
    fun main() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun reset() = Dispatchers.resetMain()

    @Test(timeout = WALL_CEILING_MS)
    fun `带着一组落盘的票开文件页：不崩，浮卡是接回来的那一组`() {
        val core = KeptCore()
        val connector = HostConnector(SshConnectionManager(transportFactory = { error("本判据不建连") }), hosts(), identities())
        val backends = HostBackends(connector, { it }, { Disk(mapOf("t-1" to PLAN)) }, core = core)
        val feed = backends.of(HOST).also { it.ensure() }
        // 常驻流在自己的 IO 线程上落核心交回来的事：等那一格落进接回来的那几组。
        while (feed.resumed().isEmpty()) Thread.sleep(POLL_MS)
        assertTrue("前提：落盘那一份交回了核心（实得 ${core.kept}）", core.kept.single().contains("files-delete"))

        // 主线程上没接住的异常在 App 里就是崩：这里记下来判。
        val crashes = mutableListOf<Throwable>()
        val was = Thread.currentThread().uncaughtExceptionHandler
        Thread.currentThread().uncaughtExceptionHandler = Thread.UncaughtExceptionHandler { _, e -> crashes += e }
        val vm =
            try {
                FilesViewModel(HOST, hosts(), connector, backends, io = StandardTestDispatcher())
            } finally {
                Thread.currentThread().uncaughtExceptionHandler = was
            }

        assertEquals("开文件页当场崩了", emptyList<String>(), crashes.map { it.toString() })

        val card = vm.batch.value
        assertTrue("浮卡该是接回来的那一组（实得 $card）", card != null && card.id == ID && card.kept)
    }

    /** 核心的替身：接回落盘那一份时照真核心那样交回那一组的一格（断开、带 `kept`）。 */
    private class KeptCore : ResidentCore {
        private var events: ((String, String) -> Unit)? = null
        val kept = mutableListOf<String>()

        override fun open(
            machine: String,
            mine: String,
            tz: String,
            events: (String, String) -> Unit,
        ): Long {
            this.events = events
            return 1
        }

        override fun batchStart(
            handle: Long,
            want: String,
        ): Long {
            kept += want
            events!!.invoke("batch", """{"id":$ID,"phase":"cut","title":"删除 1 项","line":"box 连接已断开","kept":true}""")
            return ID
        }

        override fun ssh(
            handle: Long,
            request: String?,
        ) = Unit

        override fun call(
            handle: Long,
            cmd: String,
            args: String,
            withinMs: Long,
        ) = 0L

        override fun watch(
            handle: Long,
            sid: String,
            holding: Boolean,
        ) = Unit

        override fun kick(
            handle: Long,
            retry: Boolean,
        ) = Unit

        override fun stage(
            handle: Long,
            fd: Int,
            name: String,
            withinMs: Long,
        ) = 0L

        override fun pull(
            handle: Long,
            fd: Int,
            args: String,
            shown: String,
            withinMs: Long,
        ) = 0L

        override fun transferStop(id: Long) = Unit

        override fun batchAnswer(
            id: Long,
            answer: String?,
        ) = Unit

        override fun batchStop(id: Long) = Unit

        override fun pickMenu(
            handle: Long,
            rows: String,
        ) = """{"bar":[],"more":[]}"""

        override fun clashRows(data: String) = "[]"

        override fun searchMerge(data: String) = ""

        override fun downloadEvent(want: String) = ""

        override fun answerOf(
            keys: String,
            answer: String?,
        ) = "{}"

        override fun deploy(
            handle: Long,
            slots: String,
        ) = false
    }

    /** 落盘那张表的替身。 */
    private class Disk(
        start: Map<String, String>,
    ) : KeptBatches {
        private val rows = LinkedHashMap(start)

        override suspend fun all() = rows.toList()

        override suspend fun put(
            ticket: String,
            plan: String,
        ) {
            rows[ticket] = plan
        }

        override suspend fun drop(ticket: String) {
            rows -= ticket
        }
    }

    private fun hosts() =
        HostRepository(
            object : HostDao {
                override suspend fun get(id: String): Host? = null

                override fun observeAll(): Flow<List<Host>> = throw NotImplementedError()

                override suspend fun upsert(host: Host) = throw NotImplementedError()

                override suspend fun upsertAll(hosts: List<Host>) = throw NotImplementedError()

                override suspend fun delete(host: Host) = throw NotImplementedError()

                override suspend fun deleteById(id: String) = throw NotImplementedError()
            },
            PlainCryptoBox,
        )

    private fun identities() =
        IdentityRepository(
            object : IdentityDao {
                override suspend fun get(id: String): Identity? = null

                override fun observeAll(): Flow<List<Identity>> = throw NotImplementedError()

                override suspend fun upsert(identity: Identity) = throw NotImplementedError()

                override suspend fun delete(identity: Identity) = throw NotImplementedError()
            },
            PlainCryptoBox,
        )

    private companion object {
        const val HOST = "h"
        const val ID = 41L
        const val PLAN = """{"cmd":"files-delete","done":1}"""
        const val WALL_CEILING_MS = 300_000L
        const val POLL_MS = 10L
    }
}
