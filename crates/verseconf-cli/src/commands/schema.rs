use anyhow::Result;
use std::fs;
use verseconf_core::infer_schema;

/// 由一份已有的配置推断出 `#@schema { ... }` 块。
///
/// 默认只打印，不碰原文件；`--write` 时才写回。已经存在 `#@schema` 的文件会被拒绝——
/// 一个文件里出现两份 schema 会直接解析失败。
pub fn generate(file: &str, write: bool) -> Result<()> {
    let source = fs::read_to_string(file)?;
    let schema = infer_schema(&source)?;

    if !write {
        print!("{schema}");
        return Ok(());
    }

    if source.contains("#@schema") {
        anyhow::bail!(
            "{} 已经包含 #@schema 块；请先移除再生成，避免一个文件里出现两份 schema",
            file
        );
    }

    fs::write(file, format!("{schema}\n{source}"))?;
    println!("已将推断出的 schema 写入 {}", file);
    Ok(())
}
