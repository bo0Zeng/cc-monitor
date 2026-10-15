package com.ccmonitor.mobile.core.ui.icon

/**
 * 手机用到的 Phosphor regular 图标（与桌面 `@phosphor-icons/core`、文件窗口 egui-phosphor 同一套，MIT）：名字 → 字体码位。
 * 字体 `res/font/phosphor_regular.ttf` 只打这张表里的码位（子集化，`scripts/phosphor-subset.py` 照这张表重做）；
 * 判据 `PhosphorGlyphsTest`：表里每个码位都在字体里、字体里的私用区码位都在表里、每一项都有人用。
 */
enum class Ph(
    val codepoint: Int,
) {
    ArrowDown(0xE03E),
    ArrowElbowDownLeft(0xE044),
    ArrowLeft(0xE058),
    ArrowRight(0xE06C),
    ArrowUp(0xE08E),
    ArrowsLeftRight(0xE0A0),
    CaretDown(0xE136),
    CaretRight(0xE13A),
    Chats(0xE17C),
    Copy(0xE1CA),
    DotsThree(0xE1FE),
    File(0xE230),
    Fingerprint(0xE23E),
    FolderPlus(0xE258),
    FolderSimple(0xE25A),
    GearSix(0xE272),
    HardDrives(0xE2A0),
    Key(0xE2D6),
    List(0xE2F0),
    PaperPlaneRight(0xE396),
    PencilSimple(0xE3B4),
    Play(0xE3D0),
    Plus(0xE3D4),
    PlusCircle(0xE3D6),
    Star(0xE46A),
    Stop(0xE46C),
    Trash(0xE4A6),
    UploadSimple(0xE4C0),
    X(0xE4F6),
    Tabs(0xE778),
    TerminalWindow(0xEAE8),
    ArrowCounterClockwise(0xE038),
    Backspace(0xE0AE),
    CaretLeft(0xE138),
    Check(0xE182),
    Clipboard(0xE196),
    Command(0xE1C4),
    DotsSixVertical(0xEAE2),
    Keyboard(0xE2D8),
    Rows(0xE5A2),
    Selection(0xE69A),
    Warning(0xE4E0),
    WarningCircle(0xE4E2),
    ;

    val glyph: String get() = String(Character.toChars(codepoint))
}
