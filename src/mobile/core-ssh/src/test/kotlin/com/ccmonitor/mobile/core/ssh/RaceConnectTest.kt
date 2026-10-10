package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList

/** happy-eyeballs 竞速：首个成功者胜、输家被关、在飞者被中止、全失败聚合。 */
@OptIn(ExperimentalCoroutinesApi::class)
class RaceConnectTest {
    private class Res(
        val name: String,
    ) {
        @Volatile var closed = false
    }

    @Test
    fun winnerSucceedsLosersClosed() =
        runTest {
            val created = CopyOnWriteArrayList<Res>()
            val w =
                raceToFirstSuccess(
                    items = listOf("dead1", "win", "dead2"),
                    create = { Res(it).also { r -> created.add(r) } },
                    connect = { item, _ -> if (item != "win") throw IOException("dead $item") },
                    closeLoser = { it.closed = true },
                    aggregate = { errs -> aggregateRaceError(errs) { it } },
                )
            assertEquals("win", w.name)
            assertFalse("赢家不被关", w.closed)
            assertTrue("落败者都被关", created.filter { it.name != "win" }.all { it.closed })
        }

    @Test
    fun sweepAbortsBlockedInFlightLoser() =
        runTest {
            // blocker 卡在 connect，直到被 closeLoser 中止；sweep 不关它的话本例超时。
            val signals = ConcurrentHashMap<String, CompletableDeferred<Unit>>()

            fun signal(k: String) = signals.getOrPut(k) { CompletableDeferred() }
            val w =
                raceToFirstSuccess(
                    items = listOf("blocker", "win"),
                    create = { it },
                    connect = { item, _ ->
                        if (item == "blocker") signal("blocker").await() // 卡住直到被关
                    },
                    closeLoser = { signal(it).complete(Unit) }, // 中止阻塞的 blocker
                    aggregate = { errs -> aggregateRaceError(errs) { it } },
                )
            assertEquals("win", w)
        }

    // closeLoser 解不开输家时赢家仍须立即返回：输家卡在 closeLoser 唤不醒的挂起上（closeLoser 是 no-op），
    // 相当于 sshj `disconnect()` 解不开的阻塞读。结构化等待它就永不返回；脱离结构后赢家决出即返回。
    @Test
    fun winnerReturnsEvenWhenCloseLoserCannotUnblockLoser() =
        runTest {
            val never = CompletableDeferred<Unit>() // 永不 complete：解不开的黑洞输家
            val w =
                raceToFirstSuccess(
                    items = listOf("blackhole", "win"),
                    create = { it },
                    connect = { item, _ ->
                        if (item == "blackhole") never.await() // 卡死，且 closeLoser 不唤醒
                    },
                    closeLoser = { /* no-op：disconnect 对该输家无效（黑洞残留），不解阻塞 */ },
                    aggregate = { errs -> aggregateRaceError(errs) { it } },
                )
            assertEquals("赢家不被卡死输家挟持，立即返回", "win", w)
        }

    @Test
    fun allFailThrowsAggregateWithAllLabels() =
        runTest {
            val ex =
                runCatching {
                    raceToFirstSuccess(
                        items = listOf("a", "b"),
                        create = { it },
                        connect = { item, _ -> throw IOException("boom $item") },
                        closeLoser = {},
                        aggregate = { errs -> aggregateRaceError(errs) { it } },
                    )
                }.exceptionOrNull()
            assertTrue(ex is IOException)
            assertTrue("聚合应含各 item 标签", ex!!.message!!.contains("a") && ex.message!!.contains("b"))
        }

    @Test
    fun singleItemSucceeds() =
        runTest {
            val w =
                raceToFirstSuccess(
                    items = listOf("only"),
                    create = { it },
                    connect = { _, _ -> },
                    closeLoser = {},
                    aggregate = { errs -> aggregateRaceError(errs) { it } },
                )
            assertEquals("only", w)
        }

    // create 抛错只算该地址失败：不连带取消其它地址，错误也进聚合。
    @Test
    fun createFailureIsPerItemNotFatalToRace() =
        runTest {
            val w =
                raceToFirstSuccess(
                    items = listOf("badcreate", "win"),
                    create = { if (it == "badcreate") error("create boom") else Res(it) },
                    connect = { _, _ -> },
                    closeLoser = { it.closed = true },
                    aggregate = { errs -> aggregateRaceError(errs) { it } },
                )
            assertEquals("create 失败不连坐，另一地址仍胜出", "win", w.name)
            assertFalse("赢家不被关", w.closed)
        }

    @Test
    fun allCreateFailuresAggregateWithLabels() =
        runTest {
            val ex =
                runCatching {
                    raceToFirstSuccess(
                        items = listOf("a", "b"),
                        create = { throw IOException("create boom $it") },
                        connect = { _, _: String -> },
                        closeLoser = {},
                        aggregate = { errs -> aggregateRaceError(errs) { it } },
                    )
                }.exceptionOrNull()
            assertTrue("全 create 失败应抛聚合错误（而非裸炸 scope）", ex is IOException)
            assertTrue("聚合应含各 item 标签", ex!!.message!!.contains("a") && ex.message!!.contains("b"))
        }

    @Test
    fun singleItemFailurePropagatesAndCloses() =
        runTest {
            var closed = false
            val ex =
                runCatching {
                    raceToFirstSuccess(
                        items = listOf("only"),
                        create = { it },
                        connect = { _, _ -> throw IOException("nope") },
                        closeLoser = { closed = true },
                        aggregate = { errs -> aggregateRaceError(errs) { it } },
                    )
                }.exceptionOrNull()
            assertTrue("单元素失败也应抛聚合", ex is IOException)
            assertTrue("失败者应被关", closed)
        }
}
