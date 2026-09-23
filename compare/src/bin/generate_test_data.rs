//! 生成性能对比用的测试数据集。
//!
//! 数据生成逻辑只有一份，在 `verseconf_compare::generate_test_data` 里；这个
//! 二进制只是入口。此前这里与库模块是两份 389 行的完整拷贝，而库那份里的
//! `fn main` 永远不会被调用。

use verseconf_compare::generate_test_data::TestDataGenerator;

fn main() {
    let output_dir = "compare/test_data";
    let generator = TestDataGenerator::new(output_dir);

    println!("Generating test datasets...\n");

    match generator.generate_all() {
        Ok(_) => println!("All test datasets generated successfully!"),
        Err(e) => eprintln!("Error generating test datasets: {}", e),
    }
}
