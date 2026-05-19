use tokio::sync::mpsc;

/// 启动专用 STA 线程跑 `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` + `GetMessageW` 消息循环。
/// 直接持 `tokio::sync::mpsc::UnboundedSender` 把 PID 推回主 runtime（无 unsafe 桥接）。
/// M2 落地：实际注册 hook。
pub fn spawn_focus_thread() -> mpsc::UnboundedReceiver<u32> {
    let (_tx, rx) = mpsc::unbounded_channel::<u32>();
    // TODO(M2):
    //   - 用 OnceLock<UnboundedSender<u32>> 存 _tx，hook 回调里 send()
    //   - std::thread::spawn 跑 GetMessageW 循环
    //   - 见 doc/技术实现文档.md §3.3（按本轮决策不走 spawn_blocking 写法）
    rx
}

/// 在 tokio runtime 侧消费 PID 变化，去重后回调。
pub async fn run_focus_loop<F: FnMut(u32) + Send + 'static>(
    mut rx: mpsc::UnboundedReceiver<u32>,
    mut on_change: F,
) {
    let mut last: u32 = 0;
    while let Some(pid) = rx.recv().await {
        if pid != last {
            last = pid;
            on_change(pid);
        }
    }
}
