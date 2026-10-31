package com.ccmonitor.mobile.ui.files

import androidx.compose.runtime.saveable.SaverScope
import com.ccmonitor.mobile.core.claude.link.RemotePath
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 文件页「在看什么」：进程被收之前存下，新起的那一份照它走回那一层、排回那个序。 */
class FilesLookTest {
    /** Bundle 收得下的那几种（`rememberSaveable` 存进 Activity 的那一份）。 */
    private val bundle = SaverScope { it is String || it is Boolean || it is ByteArray || it is List<*> }

    private fun reborn(look: FilesLook): FilesLook {
        val saved = with(FilesLook.SAVER) { bundle.save(look) }!!
        assertTrue("存下的进不了 Bundle：$saved", (saved as List<*>).all { it == null || bundle.canBeSaved(it) })
        return FilesLook.SAVER.restore(saved)!!
    }

    @Test
    fun `目录（连不是 UTF-8 的字节）与排序存下再取回一样`() {
        val odd = RemotePath(byteArrayOf('/'.code.toByte(), 'a'.code.toByte(), 0xff.toByte()))
        assertEquals(FilesLook(odd, bySize = true), reborn(FilesLook(odd, bySize = true)))
        assertEquals(FilesLook(RemotePath.of("/srv/app"), bySize = false), reborn(FilesLook(RemotePath.of("/srv/app"), bySize = false)))
        assertEquals(FilesLook(), reborn(FilesLook()))
    }

    @Test
    fun `新起的那一份在家：走回存下的那一层、排回那个序`() {
        val kept = reborn(FilesLook(RemotePath.of("/srv/app/logs"), bySize = true))
        val back = kept.backFrom(FilesLook(RemotePath.of("/home/u"), bySize = false))
        assertEquals(RemotePath.of("/srv/app/logs"), back.dir)
        assertTrue("排序要排回去", back.sort)
    }

    @Test
    fun `已经在那一层、那个序：什么都不做`() {
        val kept = reborn(FilesLook(RemotePath.of("/srv/app"), bySize = true))
        val back = kept.backFrom(FilesLook(RemotePath.of("/srv/app"), bySize = true))
        assertNull(back.dir)
        assertEquals(false, back.sort)
    }
}
