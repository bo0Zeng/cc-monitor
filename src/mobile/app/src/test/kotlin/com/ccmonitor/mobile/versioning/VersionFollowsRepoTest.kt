package com.ccmonitor.mobile.versioning

import com.ccmonitor.mobile.BuildConfig
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机的版本号跟仓里的版本号走：versionName 就是仓根 `package.json` 的 `version`（桌面那几份的权威源），
 * versionCode 由它算出来 —— `主 × 1_000_000 ＋ 次 × 1_000 ＋ 修订`（次、修订各在 0..999）。
 * 版本号只往上走 ⇒ versionCode 只往上走，Android 认得出是升级。
 */
class VersionFollowsRepoTest {
    @Test
    fun `versionName 就是仓根 package_json 的 version`() {
        assertEquals(repoVersion(), BuildConfig.VERSION_NAME)
    }

    @Test
    fun `versionCode 由版本号算出来`() {
        val (major, minor, patch) = repoVersion().split('.').map { it.toInt() }
        assertEquals(major * 1_000_000 + minor * 1_000 + patch, BuildConfig.VERSION_CODE)
    }

    @Test
    fun `版本号越大 versionCode 越大`() {
        val ordered = listOf("0.6.1", "0.999.999", "1.0.0", "4.1.7", "4.1.8", "4.2.0", "5.0.0")
        val codes = ordered.map { v -> v.split('.').map { it.toInt() }.let { (a, b, c) -> a * 1_000_000 + b * 1_000 + c } }
        assertTrue("$ordered → $codes", codes.zipWithNext().all { (a, b) -> a < b })
    }

    private fun repoVersion(): String {
        val pkg = File(repoRoot(), "../../package.json")
        assertTrue("前提：得找得到 ${pkg.path}", pkg.isFile)
        return Regex("(?m)^  \"version\": \"(\\d+\\.\\d+\\.\\d+)\",$").find(pkg.readText())?.groupValues?.get(1)
            ?: error("package.json 里认不出顶层那行 `\"version\": \"X.Y.Z\",`")
    }

    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到手机工程根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    private companion object {
        const val MAX_WALK_UP = 6
    }
}
