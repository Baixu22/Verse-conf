use anyhow::Result;
use std::fs;
use std::path::Path;
use verseconf_core::{apply_edit_plan_in_files, EditPlan, EffectiveView};

/// 按编辑意图契约改动配置，**跨 `@include` 定位目标**。
///
/// 与其它子命令的区别：这个命令接受的是**意图**（改哪个字段、改成什么），
/// 而不是一份新文件。目标落在哪个文件里由确定性代码定位，只改那个文件的
/// 目标字节区间，其余文件一个字节都不动。
///
/// 定位是 fail-closed 的：目标在多个文件里都能定位到时拒绝，而不是猜一个。
/// 合并后的「生效配置」会用候选内容重建并校验；重建不可信时
/// `--write` 会拒绝落盘，除非显式传 `--allow-unvalidated`。
///
/// 默认只打印结果（dry-run）；`--write` 才落盘。
pub fn execute(file: &str, plan_path: &str, write: bool, allow_unvalidated: bool) -> Result<()> {
    let plan_text = fs::read_to_string(plan_path)
        .map_err(|error| anyhow::anyhow!("读不到编辑计划 {}：{}", plan_path, error))?;

    let plan = EditPlan::from_json(&plan_text).map_err(|violations| {
        anyhow::anyhow!(
            "编辑计划不合法：{}",
            violations
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;

    match apply_edit_plan_in_files(Path::new(file), &plan) {
        Ok(outcome) => {
            println!(
                "已应用 {} 条编辑；目标文件：{}",
                outcome.applied.len(),
                outcome.file.path.display()
            );
            for edit in &outcome.applied {
                println!(
                    "  {} {}: {} → {}",
                    edit.op.as_str(),
                    edit.path,
                    edit.before,
                    edit.after
                );
            }

            let validated = match &outcome.effective_view {
                EffectiveView::Validated => {
                    println!("合并后的生效配置已校验");
                    true
                }
                EffectiveView::NotValidated { reason } => {
                    println!("注意：合并后的生效配置未校验（{reason}）");
                    false
                }
            };

            if write {
                if !validated && !allow_unvalidated {
                    eprintln!(
                        "refused_unvalidated_effective_view: 合并后的生效配置无法校验，拒绝写入；\
                         确认可以接受这一风险时用 --allow-unvalidated 显式放行"
                    );
                    std::process::exit(1);
                }
                if let Err(error) = write_checked(
                    &outcome.file.path,
                    &outcome.file.before,
                    &outcome.file.after,
                ) {
                    eprintln!("write_refused: {error}");
                    std::process::exit(1);
                }
                println!("已写入 {}", outcome.file.path.display());
            } else {
                println!("--- {}（改动后，未写盘）---", outcome.file.path.display());
                print!("{}", outcome.file.after);
            }
            Ok(())
        }
        Err(refusal) => {
            // 拒绝即拒绝：不写任何文件，并给出稳定错误码
            eprintln!("{}: {}", refusal.code(), refusal);
            std::process::exit(1);
        }
    }
}

/// 带版本检查的原子替换。
///
/// 落盘前重新读一次文件：内容与编辑时依据的 `before` 不一致就拒绝，否则会把
/// 别人在读取与写入之间做的修改静默覆盖。写入用「同目录临时文件 + 改名」完成，
/// 避免在目标位置留下写了一半的文件。
fn write_checked(path: &Path, before: &str, after: &str) -> Result<()> {
    let current = fs::read_to_string(path)
        .map_err(|error| anyhow::anyhow!("读不到 {}：{}", path.display(), error))?;
    if current != before {
        anyhow::bail!(
            "{} 在读取与写入之间被改动过，已放弃写入以免覆盖他人修改",
            path.display()
        );
    }

    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "verseconf".to_string());
    let temp = directory.join(format!(".{file_name}.{}.tmp", std::process::id()));

    fs::write(&temp, after).map_err(|error| anyhow::anyhow!("写临时文件失败：{}", error))?;
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(anyhow::anyhow!("替换 {} 失败：{}", path.display(), error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("verseconf_cli_edit_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    #[test]
    fn write_checked_replaces_the_file_and_leaves_no_temp_file() {
        let dir = temp_dir("atomic");
        let file = dir.join("config.vcf");
        fs::write(&file, "port = 8080\n").expect("准备文件");

        write_checked(&file, "port = 8080\n", "port = 9090\n").expect("应当写入");
        assert_eq!(fs::read_to_string(&file).unwrap(), "port = 9090\n");

        let leftovers: Vec<String> = fs::read_dir(&dir)
            .expect("列目录")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "不应留下临时文件：{leftovers:?}");
    }

    #[test]
    fn write_checked_refuses_when_the_file_changed_since_it_was_read() {
        let dir = temp_dir("concurrent");
        let file = dir.join("config.vcf");
        fs::write(&file, "port = 8080\n").expect("准备文件");

        let error = write_checked(&file, "port = 1234\n", "port = 9090\n").expect_err("必须拒绝");
        assert!(error.to_string().contains("被改动过"), "实际 {error}");
        // 拒绝时目标文件必须原样不动
        assert_eq!(fs::read_to_string(&file).unwrap(), "port = 8080\n");
    }
}
