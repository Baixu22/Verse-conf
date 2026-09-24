//! 生成性能对比用的测试数据集。
//!
//! 数据生成逻辑只有一份，在 `verseconf_compare::generate_test_data` 里；这个
//! 二进制只是入口。此前这里与库模块是两份 389 行的完整拷贝，而库那份里的
//! `fn main` 永远不会被调用。
//!
//! 输出目录按 crate 所在位置解析（`CARGO_MANIFEST_DIR`），所以从仓库根或任何
//! 其它目录调用都会写到 `compare/test_data/`；此前用的是 CWD 相对路径
//! `compare/test_data`，从 `compare/` 里执行会写到 `compare/compare/test_data/`。
//! 失败时返回非零退出码，不再只打印一行错误。

use std::path::PathBuf;
use std::process::ExitCode;

use verseconf_compare::generate_test_data::TestDataGenerator;

fn main() -> ExitCode {
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data");
    let generator = TestDataGenerator::new(&output_dir);

    println!(
        "Generating test datasets into {}...\n",
        output_dir.display()
    );

    match generator.generate_all() {
        Ok(_) => {
            println!("All test datasets generated successfully!");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error generating test datasets: {}", e);
            ExitCode::FAILURE
        }
    }
}
