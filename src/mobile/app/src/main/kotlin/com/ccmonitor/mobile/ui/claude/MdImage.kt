package com.ccmonitor.mobile.ui.claude

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.unit.Dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL

// 正文里的图（同桌面 `md-view.ts` 的 `<img loading=lazy referrerpolicy=no-referrer>`）：画到那一段才按地址取，不带来源头。
// 地址只会是 http · https —— 别的 scheme 核心解析时就交成了字（`md_core`），这里不再判。

/** 一段字里每张图的行内槽：取到一张换上一张（宽不超过 [maxWidth]、不放大），没取到的不给槽 ⇒ 照替代字。 */
@Composable
internal fun mdImageSlots(
    text: AnnotatedString,
    maxWidth: Dp,
): Map<String, InlineTextContent> {
    val srcs = remember(text) { imagesOf(text).distinct() }
    val got = remember(srcs) { mutableStateMapOf<String, ImageBitmap>() }
    LaunchedEffect(srcs) { srcs.forEach { s -> launch { MdImages.get(s)?.let { got[s] = it } } } }
    val density = LocalDensity.current
    return got.entries.associate { (src, bmp) ->
        val w = with(density) { bmp.width.toDp() }
        val h = with(density) { bmp.height.toDp() }
        val k = if (w > maxWidth) maxWidth / w else 1f
        val slot = with(density) { Placeholder((w * k).toSp(), (h * k).toSp(), PlaceholderVerticalAlign.TextBottom) }
        (IMG_TAG + src) to InlineTextContent(slot) { alt -> Image(bmp, contentDescription = alt, modifier = Modifier.fillMaxSize()) }
    }
}

/** 按地址取图、解成位图；取过的留一小份（同一张图在几张卡里只取一次）。取不到 · 解不开 · 太大 ⇒ `null`。 */
private object MdImages {
    /** 一张图最多收这么多字节。 */
    private const val MAX_BYTES = 8 shl 20

    /** 解码时长边最多这么多像素（大图按 2 的幂缩着解）。 */
    private const val MAX_SIDE = 2048

    private const val CONNECT_MS = 10_000
    private const val READ_MS = 15_000

    private val cache = LruCache<String, ImageBitmap>(24)

    suspend fun get(src: String): ImageBitmap? =
        cache.get(src) ?: withContext(Dispatchers.IO) { runCatching { fetch(src) }.getOrNull() }?.also { cache.put(src, it) }

    private fun fetch(src: String): ImageBitmap? {
        // HttpURLConnection 不带 Referer；跳转只在 http · https 之间。
        val c = URL(src).openConnection() as HttpURLConnection
        c.connectTimeout = CONNECT_MS
        c.readTimeout = READ_MS
        try {
            if (c.responseCode !in 200..299) return null
            val bytes = c.inputStream.use { capped(it) } ?: return null
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
            var sample = 1
            while (maxOf(bounds.outWidth, bounds.outHeight) / sample > MAX_SIDE) sample *= 2
            val opts = BitmapFactory.Options().apply { inSampleSize = sample }
            return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts)?.asImageBitmap()
        } finally {
            c.disconnect()
        }
    }

    /** 读满 [MAX_BYTES] 还没完 ⇒ `null`（不收）。 */
    private fun capped(input: InputStream): ByteArray? {
        val out = ByteArrayOutputStream()
        val buf = ByteArray(16 shl 10)
        while (true) {
            val n = input.read(buf)
            if (n < 0) return out.toByteArray()
            out.write(buf, 0, n)
            if (out.size() > MAX_BYTES) return null
        }
    }
}
