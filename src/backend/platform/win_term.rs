//! `platform::term` 的 Windows 那一臂（在那里挂成 `os`）：控制台模式 · 可见窗口的尺寸 · 输入事件队列（键 ＋ 尺寸变了）。

use super::console_keys::{Decoder, Event};
use super::{Input, Raw};

type Handle = isize;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Coord {
    x: i16,
    y: i16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct SmallRect {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}

/// `CONSOLE_SCREEN_BUFFER_INFO`（字段照 wincon.h 的次序与宽度）。
#[repr(C)]
#[derive(Default)]
struct BufferInfo {
    size: Coord,
    cursor: Coord,
    attributes: u16,
    window: SmallRect,
    max_window: Coord,
}

/// `KEY_EVENT_RECORD`（`uChar` 取 `UnicodeChar` 那一形）。
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct KeyRecord {
    key_down: i32,
    repeat: u16,
    virtual_key: u16,
    virtual_scan: u16,
    unit: u16,
    control_state: u32,
}

/// `INPUT_RECORD`：事件种类 ＋ 那个联合体（最大的一种是 16 字节的键 · 鼠标；尺寸事件只用到开头 4 字节，这里不读它）。
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct InputRecord {
    kind: u16,
    event: KeyRecord,
}

// 对不上 wincon.h ⇒ 系统按错的步长读写，先在编译期拦住。
const _: () = assert!(std::mem::size_of::<InputRecord>() == 20);
const _: () = assert!(std::mem::size_of::<BufferInfo>() == 22);

#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(which: u32) -> Handle;
    fn GetConsoleMode(h: Handle, mode: *mut u32) -> i32;
    fn SetConsoleMode(h: Handle, mode: u32) -> i32;
    fn GetConsoleScreenBufferInfo(h: Handle, info: *mut BufferInfo) -> i32;
    fn ReadConsoleInputW(h: Handle, buf: *mut InputRecord, len: u32, read: *mut u32) -> i32;
}

/// `STD_INPUT_HANDLE` · `STD_OUTPUT_HANDLE`（`(DWORD)-10` · `(DWORD)-11`）。
const STD_INPUT: u32 = -10i32 as u32;
const STD_OUTPUT: u32 = -11i32 as u32;
const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
const ENABLE_LINE_INPUT: u32 = 0x0002;
const ENABLE_ECHO_INPUT: u32 = 0x0004;
const ENABLE_WINDOW_INPUT: u32 = 0x0008;
const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;
const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
const DISABLE_NEWLINE_AUTO_RETURN: u32 = 0x0008;
const KEY_EVENT: u16 = 0x0001;
const WINDOW_BUFFER_SIZE_EVENT: u16 = 0x0004;

fn std_handle(which: u32) -> Handle {
    // SAFETY：只问本进程的标准句柄，不改它。
    unsafe { GetStdHandle(which) }
}

fn mode_of(h: Handle) -> std::io::Result<u32> {
    let mut m = 0u32;
    // SAFETY：`m` 在本栈上。
    if unsafe { GetConsoleMode(h, &mut m) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(m)
}

fn set_mode(h: Handle, m: u32) -> std::io::Result<()> {
    // SAFETY：只设本进程控制台的模式位。
    if unsafe { SetConsoleMode(h, m) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn enter_raw() -> std::io::Result<Raw> {
    let (i, o) = (std_handle(STD_INPUT), std_handle(STD_OUTPUT));
    let saved = (mode_of(i)?, mode_of(o)?);
    let raw_in = (saved.0 & !(ENABLE_PROCESSED_INPUT | ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT))
        | ENABLE_WINDOW_INPUT
        | ENABLE_VIRTUAL_TERMINAL_INPUT;
    let raw_out = saved.1 | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN;
    set_mode(i, raw_in)?;
    if let Err(e) = set_mode(o, raw_out) {
        let _ = set_mode(i, saved.0);
        return Err(e);
    }
    Ok(Raw { saved })
}

pub(super) fn leave_raw(r: &Raw) {
    let _ = set_mode(std_handle(STD_INPUT), r.saved.0);
    let _ = set_mode(std_handle(STD_OUTPUT), r.saved.1);
}

pub(super) fn size() -> Option<(u16, u16)> {
    let mut info = BufferInfo::default();
    // SAFETY：`info` 在本栈上、大小与系统要的一致（上面编译期核过）。
    if unsafe { GetConsoleScreenBufferInfo(std_handle(STD_OUTPUT), &mut info) } == 0 {
        return None;
    }
    let span = |a: i16, b: i16| u16::try_from(i32::from(b) - i32::from(a) + 1).ok();
    let rows = span(info.window.top, info.window.bottom)?;
    let cols = span(info.window.left, info.window.right)?;
    (rows > 0 && cols > 0).then_some((rows, cols))
}

pub(super) fn input() -> std::io::Result<Input> {
    let h = std_handle(STD_INPUT);
    mode_of(h)?;
    let (ktx, keys) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let (rtx, resized) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let mut decoder = Decoder::default();
        let mut buf = [InputRecord::default(); 128];
        loop {
            let mut n = 0u32;
            // SAFETY：`buf` 活着、长度照实交；一直等到有事件（不是定时器）。
            if unsafe { ReadConsoleInputW(h, buf.as_mut_ptr(), buf.len() as u32, &mut n) } == 0 {
                return;
            }
            let events: Vec<Event> = buf[..n as usize]
                .iter()
                .map(|r| match r.kind {
                    KEY_EVENT => Event::Key {
                        down: r.event.key_down != 0,
                        repeat: r.event.repeat,
                        unit: r.event.unit,
                    },
                    WINDOW_BUFFER_SIZE_EVENT => Event::Resized,
                    _ => Event::Other,
                })
                .collect();
            let got = decoder.feed(&events);
            if !got.bytes.is_empty() && ktx.send(got.bytes).is_err() {
                return;
            }
            if got.resized && rtx.send(()).is_err() {
                return;
            }
        }
    });
    Ok(Input { keys, resized })
}
