//! 让语言服务器在收到 `exit` 通知后**真的退出**。
//!
//! ## 为什么需要这一层
//!
//! tower-lsp 0.20 的传输层有一个会导致死锁的路径：
//!
//! 1. `exit` 通知到达，路由把服务状态置为 `Exited`；
//! 2. 之后只要**再有任何一条消息**到达，`read_input` 里的
//!    `service.poll_ready()` 就会返回 `Ready(Err(ExitedError))`，于是
//!    `transport.rs:132-135` 直接 `return`；
//! 3. 这个 `return` 跳过了它自己的清理（`server_tasks_tx.disconnect()`、
//!    `responses_tx.disconnect()`、`client_abort.abort()`）；
//! 4. 于是 `responses_tx` 一直存活，`print_output` 的
//!    `select(responses_rx, client_requests)` 永不结束，
//!    `join!(print_output, read_input, process_server_tasks)` 永不完成；
//! 5. `serve()` 不返回 ⇒ `main` 里 `serve().await` 之后的任何代码（包括
//!    `std::process::exit`）都是**不可达**的，连关闭 stdin 都不再能结束进程。
//!
//! 所以修正点必须在 `serve()` **之外**：这里包一层 stdin，一旦从字节流里
//! 认出 `exit` 通知，就由独立任务结束进程，不去依赖 `serve()` 返回。
//!
//! ## 口径
//!
//! LSP 规范要求：先 `shutdown` 再 `exit` → 退出码 0；未经 `shutdown` 直接
//! `exit` → 非 0。`exit` 之后给一小段缓冲时间让尚未写出的响应落盘，然后退出。

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::mpsc;

/// `exit` 之后留给未完成写出的缓冲时间。
const FLUSH_GRACE: Duration = Duration::from_millis(50);

/// 扫描窗口上限：只需要覆盖一帧的尾部，不必把整个文档留在内存里。
const SCAN_WINDOW: usize = 4096;

/// 判定一段已读字节里是否出现了 `exit` 通知。
///
/// 做法是丢掉 ASCII 空白后查找 `"method":"exit"`，并要求它后面紧跟 `}` 或 `,`，
/// 这样 `"method": "exit"`、跨行写法都能命中，而值里恰好含该字面量的普通字符串
/// 不会误判。
pub fn carries_exit_notification(window: &str) -> bool {
    let compact: String = window
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let needle = "\"method\":\"exit\"";
    let mut from = 0usize;
    while let Some(found) = compact[from..].find(needle) {
        let end = from + found + needle.len();
        match compact[end..].chars().next() {
            Some('}') | Some(',') => return true,
            // 形如 "exit_extra" 之类的更长方法名，继续往后找
            _ => from = end,
        }
    }
    false
}

/// 包装 stdin，在读出 `exit` 通知的那一刻通知外部结束进程。
pub struct ExitAwareStdin<R> {
    inner: R,
    /// 尚未交付出去、用于跨读取边界匹配的尾窗
    window: String,
    /// 命中 `exit` 后置位，保证只通知一次
    fired: bool,
    notify: mpsc::UnboundedSender<()>,
}

impl<R> ExitAwareStdin<R> {
    pub fn new(inner: R, notify: mpsc::UnboundedSender<()>) -> Self {
        Self {
            inner,
            window: String::new(),
            fired: false,
            notify,
        }
    }

    fn observe(&mut self, chunk: &[u8]) {
        if self.fired {
            return;
        }

        // 帧是 UTF-8；用 lossy 转换只用于模式匹配，不影响真正交给传输层的字节。
        self.window.push_str(&String::from_utf8_lossy(chunk));

        if carries_exit_notification(&self.window) {
            self.fired = true;
            let _ = self.notify.send(());
            self.window.clear();
            return;
        }

        // 只保留尾部，避免长会话里窗口无限增长。
        if self.window.len() > SCAN_WINDOW {
            let cut = self.window.len() - SCAN_WINDOW;
            let cut = self
                .window
                .char_indices()
                .map(|(i, _)| i)
                .find(|&i| i >= cut)
                .unwrap_or(self.window.len());
            self.window.drain(..cut);
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ExitAwareStdin<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);

        if let Poll::Ready(Ok(())) = &poll {
            let filled = buf.filled();
            if filled.len() > before {
                let chunk = filled[before..].to_vec();
                self.observe(&chunk);
            }
        }

        poll
    }
}

/// 起一个监护任务：看到 `exit` 后按规范选择退出码并结束进程。
pub fn spawn_exit_watcher(shutdown_seen: Arc<AtomicBool>, mut rx: mpsc::UnboundedReceiver<()>) {
    tokio::spawn(async move {
        if rx.recv().await.is_none() {
            return;
        }

        // 给尚未写出的诊断/响应一点时间落盘，然后按规范退出。
        tokio::time::sleep(FLUSH_GRACE).await;

        let code = if shutdown_seen.load(Ordering::SeqCst) {
            0
        } else {
            1
        };
        std::process::exit(code);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_exit_notification() {
        assert!(carries_exit_notification(
            "Content-Length: 46\r\n\r\n{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}"
        ));
    }

    #[test]
    fn detects_exit_with_whitespace_variants() {
        assert!(carries_exit_notification("{\"method\": \"exit\"}"));
        assert!(carries_exit_notification("{\"method\" :\n  \"exit\"  }"));
        assert!(carries_exit_notification(
            "[{\"method\":\"other\"},{\"method\":\"exit\"}]"
        ));
    }

    #[test]
    fn ignores_other_methods() {
        assert!(!carries_exit_notification(
            "{\"method\":\"textDocument/didOpen\"}"
        ));
        assert!(!carries_exit_notification("{\"method\":\"shutdown\"}"));
        assert!(!carries_exit_notification(""));
    }

    #[test]
    fn ignores_longer_method_names_starting_with_exit() {
        assert!(!carries_exit_notification("{\"method\":\"exitLater\"}"));
        assert!(!carries_exit_notification("{\"method\":\"exit_x\"}"));
    }

    #[test]
    fn detects_exit_split_across_reads() {
        // 逐字节喂入也要能命中（窗口必须跨读取边界保留）。
        // 用 `&` 作占位读取端：这里只测 observe()，不涉及真正的 IO。
        let notify = mpsc::unbounded_channel();
        let mut watcher = ExitAwareStdin::new(&b""[..], notify.0);
        let payload = b"{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}";
        for byte in payload {
            watcher.observe(&[*byte]);
        }
        assert!(watcher.fired, "跨读取边界的 exit 必须被识别");
    }
}
