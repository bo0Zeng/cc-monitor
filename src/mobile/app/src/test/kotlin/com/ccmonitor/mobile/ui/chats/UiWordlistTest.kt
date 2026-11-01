package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.testing.CopyTableForTests
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.squareup.moshi.Moshi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机界面禁词：每一面（聊天 · 总览 · 设置 · 服务器 · 抽屉）上屏的字不许带那一面的禁词。
 *
 * 词与射程只住 `src/shared/copy/terms.json` 的 `mobileFaces` 一节（[MobileFaceWords] 读它），这里不抄词。
 * 手机上屏的中文全从文案表出（`MobileLiteralsTest` 守着），所以判的是文案表里那几条：
 * 一面目录（`ui/<包>`）下的源码里出现的每个表键，取它的 zh 比那一面的词。
 * 第二人称不在这里判：文案表上 C-W19 逐条扫全表。
 *
 * 欠账（`owed`，「键 · 词」）两向相等：新命中红，写着欠却已不命中也红（欠账只许变短）。
 *
 * ### 它不防什么
 *
 * 1. 不是表键的字面（数据 · 标点 · 英文日志）不比：英文路由名会被大小写不敏感那一面误伤。
 * 2. 键拼接（`copyText("a.$x.c")`）认不出：取文调用点的形状另有文案表判据管。
 */
class UiWordlistTest {
    @Test
    fun everyFaceSaysNoneOfItsBannedWordsAndOwesExactlyWhatItSays() {
        val faces = MobileFaceWords.faces
        assertTrue("terms.json 的 mobileFaces 一面都没读到 ⇒ 本条恒绿", faces.isNotEmpty())
        val problems =
            faces.flatMap { face ->
                val sources = face.dirs.flatMap { sourcesOf(it) }
                assertTrue("「${face.name}」一份源码都没扫到 ⇒ 射程写错了", sources.isNotEmpty())
                verdict(face, sources.map { it.readText() }, CopyTableForTests::zhOf).map { "「${face.name}」$it" }
            }
        assertEquals("手机界面禁词：\n" + problems.joinToString("\n"), emptyList<String>(), problems)
    }

    /** 阴性对照：一条落在这一面的键 zh 带禁词 ⇒ 报出来；欠账写了却不命中 ⇒ 也报出来；注释里的键不算。 */
    @Test
    fun theVerdictCatchesANewHitAndAStaleDebt() {
        val face = Face("样面", listOf("x"), listOf("SSH", "tmux"), ignoreCase = true, owed = setOf("a.b.d" to "tmux"))
        val table = mapOf("a.b.c" to "SSH 已断开", "a.b.d" to "会话已结束", "a.b.e" to "这句是干净的", "a.b.f" to "tmux")
        val src = "fun f() {\n  Text(copyText(\"a.b.c\"))\n  Text(copyText(\"a.b.e\"))\n  // copyText(\"a.b.f\") 注释里的不算\n}\n"
        assertEquals(listOf("新命中 a.b.c 「SSH」：SSH 已断开", "欠账已不命中 a.b.d 「tmux」"), verdict(face, listOf(src), table::get))
        val sensitive = face.copy(ignoreCase = false, owed = emptySet())
        assertEquals("大小写敏感的一面，小写那个不算", emptyList<String>(), verdict(sensitive, listOf("copyText(\"k.l.m\")"), mapOf("k.l.m" to "ssh")::get))
    }

    /**
     * 「聊天菜单 · 抽屉」那一面不禁「会话」「连接」两个裸词：理由是技术义复合词都带着一个已禁的英文词。
     * 两头断：技术义仍被拦（有人删了 `tmux` / `ssh` ⇒ 红），产品义真放行（把词塞满 ⇒ 红）。
     */
    @Test
    fun theChatMenuFaceCatchesTechnicalSensesAndPassesProductWords() {
        val words = MobileFaceWords.wordsOf("聊天菜单 · 抽屉")

        fun banned(text: String) = words.any { text.contains(it, ignoreCase = true) }
        for (technical in listOf("tmux 会话", "SSH 会话", "daemon 连接", "bridge 会话", "pipe 连接", "打开 Settings 屏")) {
            assertTrue("「$technical」是技术义，必须仍被拦住", banned(technical))
        }
        for (product in listOf("结束会话", "连接失败", "设置", "服务器 · devbox")) {
            assertTrue("「$product」是产品话，不许被拦", !banned(product))
        }
    }

    /** 占位符名不是字：`{offset}` 不算命中「offset」。 */
    @Test
    fun placeholderNamesAreNotWords() {
        val face = Face("样面", listOf("x"), listOf("offset"), ignoreCase = false, owed = emptySet())
        assertEquals(emptyList<String>(), verdict(face, listOf("copyText(\"a.b.c\")"), mapOf("a.b.c" to "第 {offset} 行")::get))
    }

    private fun sourcesOf(pkg: String): List<File> {
        val rel = "src/main/kotlin/com/ccmonitor/mobile/ui/$pkg"
        // 单测的工作目录是模块目录（`app/`）；从仓根跑时退回带模块前缀的路径。
        val dir = File(rel).takeIf { it.isDirectory } ?: File("app/$rel")
        assertTrue("找不到 ui/$pkg 的源码目录（判据自己失效了，比不合规更坏）：${dir.absolutePath}", dir.isDirectory)
        return dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
    }
}

/** `terms.json` 里 `mobileFaces` 的一面。 */
internal data class Face(
    val name: String,
    val dirs: List<String>,
    val words: List<String>,
    val ignoreCase: Boolean,
    /** 还欠着的（键 · 词）。 */
    val owed: Set<Pair<String, String>>,
)

/** 一面的判定：源码里出现的表键，zh（去掉占位符名）带禁词而不在欠账里的；欠账里写了却不再命中的。 */
internal fun verdict(
    face: Face,
    sources: List<String>,
    zhOf: (String) -> String?,
): List<String> {
    val keys = sources.flatMap { text -> KotlinSourceScanner.literalsOf(text).map { it.body } }.distinct()
    val hits =
        keys
            .mapNotNull { k -> zhOf(k)?.let { k to it.replace(PLACEHOLDER, "") } }
            .flatMap { (k, zh) -> face.words.filter { zh.contains(it, ignoreCase = face.ignoreCase) }.map { Triple(k, it, zh) } }
    val fresh = hits.filter { (k, w, _) -> (k to w) !in face.owed }.map { (k, w, zh) -> "新命中 $k 「$w」：$zh" }
    val stale = face.owed.filter { (k, w) -> hits.none { it.first == k && it.second == w } }.map { (k, w) -> "欠账已不命中 $k 「$w」" }
    return fresh + stale
}

private val PLACEHOLDER = Regex("""\{[A-Za-z][A-Za-z0-9]*\}""")

/** 手机界面禁词，读 `src/shared/copy/terms.json` 的 `mobileFaces`（词与射程只住那里）。 */
internal object MobileFaceWords {
    @Suppress("UNCHECKED_CAST")
    val faces: List<Face> by lazy {
        val terms =
            generateSequence(File("").absoluteFile) { it.parentFile }
                .map { File(it, "src/shared/copy/terms.json") }
                .first { it.isFile }
        val doc =
            Moshi
                .Builder()
                .build()
                .adapter(Any::class.java)
                .fromJson(terms.readText()) as Map<String, Any?>
        val list = (doc["mobileFaces"] as Map<String, Any?>?)?.get("faces") as List<Map<String, Any?>>? ?: emptyList()
        list.map { f ->
            Face(
                name = f["name"] as String,
                dirs = f["dirs"] as List<String>,
                words = f["words"] as List<String>,
                ignoreCase = f["ignoreCase"] as Boolean,
                owed = (f["owed"] as List<Map<String, String>>).map { it.getValue("key") to it.getValue("word") }.toSet(),
            )
        }
    }

    /** 那一面的禁词（按名取）。 */
    fun wordsOf(name: String): List<String> = faces.first { it.name == name }.words
}
