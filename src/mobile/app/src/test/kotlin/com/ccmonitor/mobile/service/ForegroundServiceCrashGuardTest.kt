package com.ccmonitor.mobile.service

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import java.io.File

/**
 * 前台服务那两个崩的「调用点」还在不在。
 *
 * ### 为什么是扫源码（而不是跑服务）
 *
 * 要守的两件事都是 `Service` 的系统回调里的结构，不是可注入的逻辑：
 * `app` 模块没引 Robolectric、门禁不跑 `androidTest`
 * ⇒ 回调本身在 JVM 判据里碰不到。
 *
 * 判断那一半在 [KeepAlivePolicy] 里，由 `KeepAlivePolicyTest` 真测；
 * 这里只守「那段代码还在原位吗」，写法同 `ForegroundServiceReachTest`。
 *
 * ### 它守得住什么 / 守不住什么
 *
 * | 这一格 | 本判据 |
 * |---|---|
 * | 有人把 `onTimeout` 整个删了 | 真红 |
 * | 有人实现了 `onTimeout` 但没走到停 | 真红（扫它的函数体） |
 * | 有人把 `startForeground` 写成裸调（去掉 try） | 真红（断「调用落在 try 与 catch 之间」） |
 * | 有人把 `started = true` 挪到 `startForeground` 之前 | 真红（比下标先后） |
 * | 有人新加一条停服务的路、绕过那个唯一出口（于是不记因由） | 真红（断 `stopSelf()` 在代码里恰好一处） |
 * | 版本门被摘掉（`@RequiresApi` 没了） | 真红 |
 * | 调用都在，但参数写错 / 停得不够快 / 系统到底调不调 | 看不见 —— 要真机 |
 * | 反射 / 按名拼接 / 死分支 | 看不见（逐条在 [KotlinSourceScanner] 头注里，这里不复述） |
 */
class ForegroundServiceCrashGuardTest {
    private fun moduleFile(relativeToApp: String): File =
        File(relativeToApp).takeIf { it.exists() } ?: File("app/$relativeToApp")

    private fun serviceSource(): String {
        val f = moduleFile("src/main/kotlin/com/ccmonitor/mobile/service/SshKeepAliveService.kt")
        assertTrue("前提：扫不到服务源文件（本判据会恒绿）：${f.absolutePath}", f.isFile)
        val text = f.readText()
        assertTrue("前提：服务源文件得真有内容（实得 ${text.lineSequence().count()} 行）", text.lineSequence().count() > 300)
        return text
    }

    /**
     * 剥注释、把字面也删掉，走全仓唯一一份词法扫描器。
     *
     * 选「删字面」那一支是因为本文件问的全是「代码里有没有这个调用/声明」，
     * 字面里提一嘴不算。选错不会编译失败，只会静默（见 [KotlinSourceScanner] 头注那张表）。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyDroppingLiterals(text)

    /**
     * 取某个声明的函数体（含两头花括号），在已剥注释并删掉字面的文本上走。
     *
     * 刻意用 `for` 配平、且全程不把引号当字符比：字面内容已经被扫描器删掉了，
     * 本文件压根不需要自己认词法（那正是 `SharedScannerGateTest` ⑥ 禁的形态）。
     */
    private fun bodyOf(
        code: String,
        signature: String,
    ): String {
        val at = code.indexOf(signature)
        assertTrue("前提：代码里找不到 `$signature`（扫描面或签名写错了 ⇒ 判据恒绿）", at >= 0)
        val open = code.indexOf('{', at)
        assertTrue("前提：`$signature` 之后没有左花括号", open > at)
        var depth = 0
        for (i in open until code.length) {
            when (code[i]) {
                '{' -> depth++
                '}' -> {
                    depth--
                    if (depth == 0) return code.substring(open, i + 1)
                }
                else -> Unit
            }
        }
        fail("前提：`$signature` 的函数体花括号没配平")
        return ""
    }

    /**
     * 取表达式体函数（`fun f(...) = 一个表达式`）的那个表达式，到行尾为止。
     *
     * 注意：窗口不能开成「声明往后若干字符」。剥掉注释之后紧挨着的下一个声明会落进窗口，
     * 邻居的 `fun startRefusalIsPossible(` 声明就被当成「这里调用了它」，摘掉版本门照绿。
     */
    private fun expressionBodyOf(
        code: String,
        signature: String,
    ): String {
        val at = code.indexOf(signature)
        assertTrue("前提：代码里找不到 `$signature`（⇒ 判据恒绿）", at >= 0)
        val eq = code.indexOf('=', code.indexOf(')', at))
        assertTrue("前提：`$signature` 不是表达式体函数（找不到等号）", eq > at)
        return code.substring(eq).lineSequence().first()
    }

    /** 「这个调用落在 try 与 catch 之间」= 它真的被接住了（光有个 try 在别处不算）。 */
    private fun callIsInsideATryWithACatch(
        body: String,
        call: String,
    ): Boolean {
        val tryAt = body.indexOf("try")
        val callAt = body.indexOf(call)
        val catchAt = body.indexOf("catch")
        if (tryAt < 0 || callAt < 0 || catchAt < 0) return false
        return tryAt < callAt && callAt < catchAt
    }

    // ─── 前提：这个崩到底成不成立 ───

    /**
     * 先证前提还在：`dataSync` 型 ＋ `targetSdk >= 35` ⇒ 才吃那条 6 小时额度。
     *
     * 没有这一条，下面那些判据就是「为一个不存在的问题站岗」。
     * 哪天服务的型换掉了、或 `targetSdk` 退回 34 以下，本条会红并要求回去重新决定
     * （而不是让 `onTimeout` 那几条静默地继续绿着）。
     */
    @Test
    fun thePreconditionForTheTimeoutCrashStillHolds() {
        val manifest = moduleFile("src/main/AndroidManifest.xml")
        assertTrue("前提：扫不到清单：${manifest.absolutePath}", manifest.isFile)
        val manifestText = manifest.readText()
        assertTrue(
            "本服务不再是 `dataSync` 型了 ⇒ 那条 6 小时额度可能不适用了。" +
                "请回去重新决定 `onTimeout` 这一族判据还要不要（别让它们静默绿着）。",
            manifestText.contains("android:foregroundServiceType=\"dataSync\""),
        )
        assertTrue("前提：清单里得真声明了本服务", manifestText.contains(".service.SshKeepAliveService"))

        val buildFile = moduleFile("build.gradle.kts")
        assertTrue("前提：扫不到 app 的构建文件：${buildFile.absolutePath}", buildFile.isFile)
        val targetSdk =
            Regex("""targetSdk\s*=\s*(\d+)""")
                .find(buildFile.readText())
                ?.groupValues
                ?.get(1)
                ?.toInt()
        assertTrue("前提：读不出 targetSdk（本判据的前提就量不到了）", targetSdk != null)
        assertTrue(
            "`targetSdk = $targetSdk` 已经低于 35 ⇒ Android 15 那条 dataSync 额度不再适用。" +
                "请回去重新决定这一族判据。",
            targetSdk!! >= 35,
        )
    }

    // ─── ① onTimeout ───

    /**
     * `onTimeout` 在代码里（不是在注释里），且走到了停。
     *
     * 两头断：先证剥注释这一步真的在起作用（原文里 `onTimeout` 提了好几次，
     * 剥完只剩一处声明），再证那一处声明的函数体真的通向唯一出口。
     */
    @Test
    fun theServiceImplementsOnTimeoutAndActuallyStops() {
        val raw = serviceSource()
        val code = codeOnly(raw)

        // 剥注释是载荷：原文里这个名字在头注里反复出现，光 `contains` 会被散文骗到
        //   （扫全文的话，删掉调用照样绿）。
        assertTrue(
            "前提：原文里 `onTimeout` 该在注释里也出现过（否则下面这条「剥注释有用」就没靶子了）",
            countOf(raw, "onTimeout") > countOf(code, "onTimeout"),
        )
        assertEquals(
            "`onTimeout` 的覆写不见了 ⇒ Android 15 的 dataSync 额度到点后没人停服务 ⇒ RemoteServiceException。",
            1,
            countOf(code, "override fun onTimeout("),
        )

        val body = bodyOf(code, "override fun onTimeout(")
        assertTrue(
            "`onTimeout` 实现了，但函数体里没有通向停服务的那条路：" +
                "系统只给几秒，不停就崩。实得函数体：$body",
            body.contains("stopFor("),
        )
    }

    /**
     * `onTimeout` 带版本门。
     *
     * `onTimeout(int, int)` 是 API 35 新增（`api-versions.xml`：`onTimeout(II)V since=35`），
     * 而 `minSdk = 26` ⇒ 摘掉门会让 lint 的 NewApi 失去把手。
     */
    @Test
    fun theOnTimeoutOverrideKeepsItsApiGate() {
        val code = codeOnly(serviceSource())
        val at = code.indexOf("override fun onTimeout(")
        assertTrue("前提：找不到 `onTimeout` 的覆写", at >= 0)
        val before = code.substring(0, at)
        val lastAnnotation = before.lastIndexOf("@RequiresApi")
        assertTrue(
            "`onTimeout` 上面的 `@RequiresApi` 版本门不见了（它是 API 35 新增、minSdk = 26）。",
            lastAnnotation >= 0 && before.substring(lastAnnotation).lineSequence().count() <= 2,
        )
    }

    // ─── ② startForeground 接异常 ───

    /**
     * `startForeground` 不是裸调：调用落在 `try` 与 `catch` 之间。
     *
     * 裸调的话，Android 12+ 在后台起 FGS、以及 Android 15+「额度已用尽」两种拒绝
     * 都会让异常冲出 `onStartCommand` ⇒ 进程崩。
     */
    @Test
    fun theStartForegroundCallIsWrappedSoARefusalCannotCrashTheProcess() {
        val code = codeOnly(serviceSource())
        val body = bodyOf(code, "override fun onStartCommand(")
        assertTrue("前提：`onStartCommand` 里得真有那个调用", body.contains("startForeground("))
        assertTrue(
            "`startForeground` 是裸调（调用没落在 try 与 catch 之间）⇒ " +
                "系统一拒就崩。实得函数体：$body",
            callIsInsideATryWithACatch(body, "startForeground("),
        )
    }

    /**
     * 接住之后不是只吞掉：既把因由记成事实、又真的停。
     *
     * 吞掉的后果是本仓最怕的那类：服务起不来，而没有任何地方报错。
     */
    @Test
    fun aRefusedStartIsRecordedAndStoppedRatherThanSwallowed() {
        val code = codeOnly(serviceSource())
        val body = bodyOf(code, "override fun onStartCommand(")
        assertTrue(
            "被拒之后没有走到停 ⇒ 留下一个非前台的服务，迟早被系统收走且没人知道为什么",
            body.contains("stopFor("),
        )
        assertTrue(
            "被拒之后没有记下因由（`StartRefused`）⇒ 将来界面说不出「保活为什么没了」",
            body.contains("StartRefused"),
        )
        assertTrue(
            "catch 里没有 `throw` ⇒ 认不出的异常也被吞了。" +
                "不认识的错必须原样上抛，否则下一个真 bug 变成一条日志。",
            body.contains("throw "),
        )
    }

    /**
     * `started` 只在成功之后才置真。
     *
     * 那个 `@Volatile started` 自带注释「防止观察者在 startForeground 前 stopSelf（FGS 崩溃窗）」。
     * 要是失败也置真，观察者就会以为「已经前台过了」而放开自停闸：那是同一个窗口的反面。
     */
    @Test
    fun theStartedFlagIsOnlySetAfterStartForegroundSucceeded() {
        val code = codeOnly(serviceSource())
        val body = bodyOf(code, "override fun onStartCommand(")
        val callAt = body.indexOf("startForeground(")
        val flagAt = body.indexOf("started = true")
        assertTrue("前提：`onStartCommand` 里得有那个调用", callAt >= 0)
        assertTrue("`onStartCommand` 里找不到 `started = true` 了 ⇒ 自停闸永远不放开，服务再也不自停", flagAt >= 0)
        assertTrue(
            "`started = true` 跑到 `startForeground` 前面去了 ⇒ 起失败也会被当成「已前台」。",
            flagAt > callAt,
        )
    }

    // ─── ③ 唯一出口 ───

    /**
     * 停服务只许有一个出口：`stopSelf()` 在代码里恰好一处。
     *
     * 这是「为什么停了」这个事实不漏记的结构保证：新加一条绕过 [KeepAliveStopFact] 的停法，
     * 用户就又会遇到「保活没了而没人能说为什么」。
     *
     * 原文里 `stopSelf` 在注释里出现好几次：这条必须在剥注释之后数，
     * 否则就是又一条「量的是散文，不是代码」。
     */
    @Test
    fun everyStopGoesThroughTheSingleExitThatRecordsTheReason() {
        val raw = serviceSource()
        val code = codeOnly(raw)
        assertTrue(
            "前提：原文注释里该提过 `stopSelf`（否则下面这条「剥注释有用」没靶子）",
            countOf(raw, "stopSelf") > countOf(code, "stopSelf"),
        )
        assertEquals(
            "`stopSelf()` 在代码里不再是恰好一处 ⇒ 有人绕过了那个会记因由的唯一出口" +
                "（[SshKeepAliveService.stopFor]）。新的停法请走 `stopFor(...)`。",
            1,
            countOf(code, "stopSelf()"),
        )
        val exit = bodyOf(code, "private fun stopFor(")
        assertTrue("唯一出口里没记因由了 ⇒ 这个出口白当了", exit.contains("KeepAliveStopFact.record("))
        assertTrue("唯一出口里没真停 ⇒ 名不副实", exit.contains("stopSelf()"))
    }

    /**
     * JVM 测不到的那一格，结构上钉住：类型判断里真的引用了那个异常类型。
     *
     * `KeepAlivePolicyTest` 的边界表里写着：真的
     * `ForegroundServiceStartNotAllowedException` 实例在 JVM 单测里造不出来（那个类是桩）。
     * 所以那一格只能从结构上断，并且如实承认这不等于验过它真能认出来（要 Android 12+ 真机）。
     */
    @Test
    fun theRefusalClassifierStillReferencesTheRealExceptionType() {
        val code = codeOnly(serviceSource())
        val gate = bodyOf(code, "object Api31")
        assertTrue(
            "认「系统不许起」的那个判断不再引用 `ForegroundServiceStartNotAllowedException` ⇒ " +
                "它可能已经认不出真异常了（这一格 JVM 测不到，只能这样钉）。实得：$gate",
            gate.contains("ForegroundServiceStartNotAllowedException"),
        )
    }

    /**
     * [KeepAlivePolicy.isStartRefusal] 真的还在问那道版本门。
     *
     * JVM 判据手上没有真异常实例，摘不摘版本门读数都一样，所以「还在问它」只能从结构上钉。
     * 门本身由 `KeepAlivePolicyTest` 两头断。
     */
    @Test
    fun theRefusalClassifierStillConsultsTheApiGate() {
        val code = codeOnly(serviceSource())
        val expr = expressionBodyOf(code, "fun isStartRefusal(")
        assertTrue(
            "`isStartRefusal` 不再问版本门了 ⇒ minSdk 26 上会去碰一个 API 31 才有的类型。" +
                "实得表达式体：$expr",
            expr.contains("startRefusalIsPossible("),
        )
    }

    // ─── 探测器自检（没有它，上面都是「零命中 = 守住了」） ───

    /**
     * 本文件那几个探测器不瞎。
     *
     * 正反都断：合成一份「坏的」必须认出来；「好的」不许被认成坏的。
     * 不验它的话，上面每一条都可能因为探测器失灵而恒绿。
     */
    @Test
    fun theDetectorsInThisFileAreNotBlind() {
        val bare = "{ ServiceCompat.startForeground(this, 1, n, 2); started = true; return 1 }"
        assertFalse("认不出裸调 ⇒ 那条判据是恒绿的", callIsInsideATryWithACatch(bare, "startForeground("))

        val wrapped = "{ try { ServiceCompat.startForeground(this, 1, n, 2) } catch (e: IllegalStateException) { throw e } }"
        assertTrue("接住了的写法不许被认成裸调（否则是全红，不是能分辨）", callIsInsideATryWithACatch(wrapped, "startForeground("))

        val tryElsewhere = "{ ServiceCompat.startForeground(this, 1, n, 2); try { other() } catch (e: Throwable) { } }"
        assertFalse(
            "「别处有个 try、调用在它外面」必须认成裸调 —— 否则只要文件里随便有个 try 就绿了",
            callIsInsideATryWithACatch(tryElsewhere, "startForeground("),
        )

        // 表达式体抽取器：不许越过行尾去把邻居的声明算成「这里调用了它」(变异 M-h2 的教训)。
        val twoDecls = "fun a(x: Int): Boolean = gate(x) && b(x)\nfun gate(x: Int): Boolean = x > 0\n"
        assertEquals(
            "表达式体抽取器越过了行尾 ⇒ 它会把邻居声明当成调用，那条判据就恒绿了",
            "= gate(x) && b(x)",
            expressionBodyOf(twoDecls, "fun a("),
        )
        val noGate = "fun a(x: Int): Boolean = b(x)\nfun gate(x: Int): Boolean = x > 0\n"
        assertFalse(
            "没问门的写法必须认出来（邻居那行的 `gate` 不许算）",
            expressionBodyOf(noGate, "fun a(").contains("gate("),
        )

        // 函数体抽取器本身：嵌套花括号要配平到自己那一个，不能被里层的提前收尾骗走。
        val nested = "fun f() { if (x) { g() }\n h() }"
        assertEquals("函数体抽取器被嵌套花括号骗了 ⇒ 上面所有「扫函数体」的判据都在量半截", "{ if (x) { g() }\n h() }", bodyOf(nested, "fun f("))
    }

    /** 不重叠地数 [mark] 出现几次（`split` 的段数 − 1，与「找到就跳过整个 mark」同义）。 */
    private fun countOf(
        text: String,
        mark: String,
    ): Int = text.split(mark).size - 1
}
