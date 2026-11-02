package com.ccmonitor.mobile.ui.chat.attach

import android.content.ActivityNotFoundException
import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.ui.chat.ChatTestTags
import com.ccmonitor.mobile.ui.common.dashedFrame
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FilterInputStream
import java.io.IOException
import java.io.InputStream

/** ［+］面板头一排那三格从哪拿。 */
enum class AttachSource { CAMERA, IMAGE, FILE }

/** 输入那一圈对附件那一排要做的几样。 */
data class AttachActions(
    /** 选好一份：名字 · 是不是图片（缩略图）· 现开本机那一份。 */
    val add: (String, Boolean, () -> InputStream) -> Unit = { _, _, _ -> },
    val remove: (String) -> Unit = {},
    val retry: () -> Unit = {},
    /** 手机这边的错（没有相机 …）。 */
    val local: (LocalWhy) -> Unit = {},
)

/**
 * 三格各接哪一个系统入口：拍照 ⇒ 系统相机写进 FileProvider 给的那一份（不另要相机权限）；图片 ⇒ 系统照片选择器；文件 ⇒ 系统文件选择器。
 * 选好了交 [AttachActions.add]（拷进本机那一下在核心那一侧之外，见 [AttachStore]）。
 */
@Composable
fun rememberAttachPick(actions: AttachActions): (AttachSource) -> Unit {
    val context = LocalContext.current
    var shot by rememberSaveable { mutableStateOf<String?>(null) }
    val camera =
        rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { ok ->
            val f = shot?.let(::File)
            shot = null
            if (f == null) return@rememberLauncherForActivityResult
            if (ok && f.length() > 0) actions.add(f.name, true) { Deleting(f) } else f.delete()
        }
    val image = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri -> uri?.let { picked(context, it, true, actions) } }
    val file = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> uri?.let { picked(context, it, false, actions) } }
    return remember(context) {
        { source ->
            try {
                when (source) {
                    AttachSource.CAMERA -> {
                        val f = shotFile(context)
                        shot = f.path
                        camera.launch(FileProvider.getUriForFile(context, context.packageName + AUTHORITY, f))
                    }
                    AttachSource.IMAGE -> image.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                    AttachSource.FILE -> file.launch(arrayOf(ANY))
                }
            } catch (_: ActivityNotFoundException) {
                actions.local(LocalWhy.NO_CAMERA)
            }
        }
    }
}

/** 系统交来的一份：名字照它报的（报不出 ⇒ 末段），现开照它给的。 */
private fun picked(
    context: Context,
    uri: Uri,
    image: Boolean,
    actions: AttachActions,
) {
    val name =
        runCatching {
            context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c -> if (c.moveToFirst()) c.getString(0) else null }
        }.getOrNull() ?: uri.lastPathSegment.orEmpty()
    actions.add(name, image) { context.contentResolver.openInputStream(uri) ?: throw IOException(uri.toString()) }
}

/** 相机写进来的那一份（缓存目录里，拷进那一排之后就删）。 */
private fun shotFile(context: Context): File {
    val dir = File(context.cacheDir, SHOTS).apply { mkdirs() }
    return File(dir, "IMG_${System.currentTimeMillis()}.jpg")
}

/** 读完关上就删的一份（相机那一份拷进那一排之后不留）。 */
private class Deleting(
    private val f: File,
) : FilterInputStream(f.inputStream()) {
    override fun close() {
        super.close()
        f.delete()
    }
}

/** ［+］面板头一排的一格：开哪个系统入口 · 图标 · 字 · 判据认它的标签。 */
internal class PlusTile(
    val source: AttachSource,
    val icon: Ph,
    val label: () -> String,
    val tag: String,
)

/** ［+］面板头一排：拍照 · 图片 · 文件。 */
internal val PLUS_TILES =
    listOf(
        PlusTile(AttachSource.CAMERA, Ph.Camera, { copyText("mobAttach.plus.camera") }, ChatTestTags.PLUS_CAMERA),
        PlusTile(AttachSource.IMAGE, Ph.Image, { copyText("beFilesKind.label.image") }, ChatTestTags.PLUS_IMAGE),
        PlusTile(AttachSource.FILE, Ph.File, { copyText("mobDrawer.item.files") }, ChatTestTags.PLUS_FILE),
    )

/** ［+］面板头一排（[PLUS_TILES]；点一格 ⇒ 面板收起、开那个系统入口）。 */
@Composable
fun PlusAttachRow(pick: (AttachSource) -> Unit) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.xs),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
    ) {
        PLUS_TILES.forEach { t -> Tile(t.icon, t.label(), t.tag) { pick(t.source) } }
    }
}

@Composable
private fun androidx.compose.foundation.layout.RowScope.Tile(
    icon: Ph,
    label: String,
    tag: String,
    onClick: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Column(
        Modifier
            .weight(1f)
            .heightIn(min = TILE_HEIGHT.dp)
            .clip(RoundedCornerShape(tokens.radius.m))
            .background(tokens.card)
            .clickable(onClick = onClick)
            .testTag(tag)
            .padding(tokens.spacing.sm),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        PhIcon(icon, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(label, style = MaterialTheme.typography.bodyMedium)
    }
}

/** 输入框上方那一排 ＋ 卡下那一条；没有卡也没有那一条 ⇒ 不画。 */
@Composable
fun AttachArea(
    ui: AttachUi,
    actions: AttachActions,
    repick: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    if (ui.cards.isNotEmpty()) {
        Row(
            Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(vertical = tokens.spacing.xs)
                .testTag(ChatTestTags.ATTACH),
            horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
        ) {
            ui.cards.forEach { c -> Card(c, ui.offline) { actions.remove(c.key) } }
        }
    }
    ui.note?.let { n ->
        NoteBar(n) {
            when (n.act) {
                AttachNote.Act.RETRY -> actions.retry()
                AttachNote.Act.REPICK -> repick()
                null -> Unit
            }
        }
    }
}

/** 一张卡：图片 64dp 方块缩略图；文件是名 ＋ 状态的小卡。角上 ✕；传输中只转环（不写进度数字）；排队断着 ⇒ 虚线、淡；失败 · 不收 ⇒ 描红。 */
@Composable
private fun Card(
    c: AttachCard,
    offline: Boolean,
    remove: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val shape = RoundedCornerShape(tokens.radius.m)
    val bad = isBad(c.phase)
    val frame =
        when {
            bad -> Modifier.border(BorderStroke(1.dp, MaterialTheme.colorScheme.error), shape)
            c.phase == AttachPhase.Queued && offline -> Modifier.dashedFrame(tokens.textFaint)
            else -> Modifier
        }
    Box(Modifier.size(if (c.image) THUMB.dp else FILE_CARD.dp, THUMB.dp).testTag(ChatTestTags.ATTACH_CARD)) {
        Box(
            Modifier
                .fillMaxSize()
                .clip(shape)
                .background(tokens.card)
                .then(frame),
        ) {
            if (c.image) Thumb(c.file)
            CardText(c, bad)
            (c.phase as? AttachPhase.Moving)?.let { m ->
                CircularProgressIndicator(
                    progress = { fraction(m) },
                    modifier = Modifier.align(if (c.image) Alignment.Center else Alignment.CenterEnd).padding(tokens.spacing.xs).size(RING.dp),
                    strokeWidth = 2.dp,
                )
            }
        }
        RemoveDot(c.removeLabel, remove)
    }
}

/** 卡上的字：文件 ⇒ 名 ＋ 第二行；图片 ⇒ 只第二行，压在缩略图底边。 */
@Composable
private fun CardText(
    c: AttachCard,
    bad: Boolean,
) {
    Column(
        Modifier.fillMaxSize().padding(LocalAppTokens.current.spacing.xs),
        verticalArrangement = if (c.image) Arrangement.Bottom else Arrangement.Center,
        horizontalAlignment = if (c.image) Alignment.CenterHorizontally else Alignment.Start,
    ) {
        if (!c.image) Text(c.name, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
        val color = if (bad) LocalAppTokens.current.errorText else MaterialTheme.colorScheme.onSurfaceVariant
        c.line?.let { Text(it, style = MaterialTheme.typography.labelSmall, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis) }
    }
}

/** 卡角的 ✕（读屏名核心写好）。 */
@Composable
private fun androidx.compose.foundation.layout.BoxScope.RemoveDot(
    label: String,
    remove: () -> Unit,
) {
    Box(
        Modifier
            .align(Alignment.TopEnd)
            .size(Touch.iconInline + 6.dp)
            .clip(CircleShape)
            .background(MaterialTheme.colorScheme.surface)
            .clickable(onClick = remove)
            .semantics { contentDescription = label }
            .testTag(ChatTestTags.ATTACH_REMOVE),
        contentAlignment = Alignment.Center,
    ) { PhIcon(Ph.X, contentDescription = null, size = Touch.iconInline) }
}

/** 描红的那几种：传 · 落没成 · 副本开不了 · 那台不收。 */
internal fun isBad(p: AttachPhase): Boolean = p is AttachPhase.Failed || p is AttachPhase.Refused || p == AttachPhase.Lost

/** 缩略图：本机那一份按卡的大小降采样解（解不出 ⇒ 不画，卡照留）。 */
@Composable
private fun Thumb(file: File) {
    val px = with(androidx.compose.ui.platform.LocalDensity.current) { THUMB.dp.roundToPx() }
    val bmp by produceState<ImageBitmap?>(null, file.path) { value = withContext(Dispatchers.IO) { decode(file, px) } }
    bmp?.let { Image(it, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Crop) }
}

private fun decode(
    file: File,
    px: Int,
): ImageBitmap? =
    runCatching {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        var sample = 1
        while (bounds.outWidth / (sample * 2) >= px && bounds.outHeight / (sample * 2) >= px) sample *= 2
        BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sample })?.asImageBitmap()
    }.getOrNull()

/** 卡下那一条：左边一道（失败红 · 离线灰虚线），那一句，按钮。 */
@Composable
private fun NoteBar(
    n: AttachNote,
    act: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier
            .fillMaxWidth()
            .padding(vertical = tokens.spacing.xs)
            .background(tokens.card)
            .testTag(ChatTestTags.ATTACH_NOTE),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val bar = Modifier.width(BAR.dp).height(Touch.target)
        Box(if (n.dashed) bar.dashedFrame(tokens.textFaint) else bar.background(MaterialTheme.colorScheme.error))
        Text(n.text, Modifier.weight(1f).padding(horizontal = tokens.spacing.sm), style = MaterialTheme.typography.bodySmall)
        n.act?.let { a ->
            SecondaryButton(onClick = act, modifier = Modifier.testTag(ChatTestTags.ATTACH_ACT)) {
                Text(if (a == AttachNote.Act.RETRY) copyText("front.act.retry") else copyText("mobAttach.act.repick"))
            }
        }
        Spacer(Modifier.width(tokens.spacing.xs))
    }
}

private fun fraction(m: AttachPhase.Moving): Float = if (m.total <= 0L) 0f else (m.done.toFloat() / m.total).coerceIn(0f, 1f)

private const val THUMB = 64
private const val FILE_CARD = 120
private const val TILE_HEIGHT = 72
private const val RING = 28
private const val BAR = 3
private const val ANY = "*/*"
private const val SHOTS = "shots"

/** FileProvider 的名字（`AndroidManifest.xml` 里那一格，接在包名后）。 */
private const val AUTHORITY = ".attach"
