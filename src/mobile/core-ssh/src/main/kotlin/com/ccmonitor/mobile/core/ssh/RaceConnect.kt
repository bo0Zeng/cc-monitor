package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import java.io.IOException
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.coroutineContext

/**
 * happy-eyeballs 竞速（纯算法，不碰 sshj）。对每个 [items] 并发先 [create] 出资源并立即登记，再 [connect]；
 * 首个成功者胜，其余在飞或落败者交给 [closeLoser] 中止，不等死地址超时；全失败抛 [aggregate] 的结果。
 *
 * 先登记再 connect：即便某个 item 卡在 connect，赢家决出后也能关掉它的资源。
 *
 * 注意：竞速协程跑在脱离调用方结构的 scope 里，赢家决出后立即返回，不 join 输家。
 * 卡在不可取消阻塞读上的输家 [closeLoser] 未必能解开；若结构化等待它，调用方和上层的单飞连接锁会被永久占住。
 * 脱离后，卡死的输家至多留下一个随 socket 出错才退出的后台协程。
 */
@Suppress("TooGenericExceptionCaught") // 逐个捕获每个 item 的任何失败，最后聚合
suspend fun <T, R> raceToFirstSuccess(
    items: List<T>,
    create: (T) -> R,
    connect: suspend (T, R) -> Unit,
    closeLoser: (R) -> Unit,
    aggregate: (List<Pair<T, Throwable>>) -> Throwable,
): R {
    require(items.isNotEmpty()) { "no item to race" }
    val winner = CompletableDeferred<R>()
    val live = CopyOnWriteArrayList<R>()
    val errors = CopyOnWriteArrayList<Pair<T, Throwable>>()
    val remaining = AtomicInteger(items.size)
    // 继承调用方 dispatcher，但用一个无父的 SupervisorJob，竞速协程不是调用方的子协程。
    val raceJob = SupervisorJob()
    val raceScope = CoroutineScope(coroutineContext.minusKey(Job) + raceJob)
    items.forEach { item ->
        raceScope.launch {
            // create 也在 try 里：create 失败只算该地址失败，不连带取消其它地址，错误也进聚合。
            var r: R? = null
            try {
                val res = create(item)
                r = res
                live.add(res) // 先登记再 connect
                if (winner.isCompleted) {
                    closeLoser(res) // 已有赢家：不再发起
                } else {
                    connect(item, res)
                    if (!winner.complete(res)) closeLoser(res) // 输了竞速：关自己
                }
            } catch (e: CancellationException) {
                r?.let(closeLoser)
                throw e
            } catch (e: Throwable) {
                errors.add(item to e)
                r?.let(closeLoser) // create 没产出资源时无可关
            } finally {
                if (remaining.decrementAndGet() == 0 && !winner.isCompleted) {
                    winner.completeExceptionally(aggregate(errors))
                }
            }
        }
    }
    return try {
        val w = winner.await()
        live.forEach { if (it !== w) closeLoser(it) } // 关其余在飞者，不 join
        w
    } catch (e: Throwable) {
        live.forEach { closeLoser(it) }
        throw e
    } finally {
        // 可取消的输家即刻收尾；卡死的输家资源已关过，留在脱离的 scope 里自己退出，不挟持调用方。
        raceJob.cancel()
    }
}

/** 全失败时汇总各 item 的错误（[describe] 把 item 转成可读标签，如 host:port），而不是只暴露最后完成的那条。 */
fun <T> aggregateRaceError(
    errors: List<Pair<T, Throwable>>,
    describe: (T) -> String,
): Throwable {
    if (errors.isEmpty()) return IOException("所有地址连接失败")
    // host key 变更原样上抛、不包 IOException：它是唯一可恢复的连接失败（用户确认后可重连），
    // 调用方要靠 `catch (e: HostKeyChangedException)` 进恢复页；单地址直连也走竞速，同样适用。
    // 注意：只特判这一种。泛化成「单个错误原样上抛」会改变所有单地址失败的异常类型。
    errors.firstOrNull { it.second is HostKeyChangedException }?.let { return it.second }
    val summary = errors.joinToString("; ") { (item, e) -> "${describe(item)}=${e.message ?: e.javaClass.simpleName}" }
    return IOException("所有地址连接失败: $summary").apply { errors.forEach { addSuppressed(it.second) } }
}
