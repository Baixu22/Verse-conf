use verseconf_lsp::server::run;

#[tokio::main]
async fn main() {
    run().await;

    // LSP 规范要求：收到 `exit` 通知后进程必须结束（shutdown 之后退出码 0）。
    //
    // tower-lsp 0.20 的传输层在读到 ExitedError 时会直接从 read_input 返回，
    // 跳过它自己的清理逻辑，于是 print_output 仍等在未关闭的流上、serve() 的
    // join! 永不完成——进程被永久挂起，连关闭 stdin 都不再能结束它。
    // 这里显式结束进程，把规范要求的语义补上。
    std::process::exit(0);
}
