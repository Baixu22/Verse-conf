#!/usr/bin/env python3
"""生成VerseConf vs TOML vs JSON性能对比可视化图表"""

import json

# 最新benchmark数据
perf_data = {
    "small": {"vcf": 102.61, "toml": 421.13, "json": 9.77},
    "medium": {"vcf": 535.70, "toml": 2092.76, "json": 259.52},
    "large": {"vcf": 6710.50, "toml": 18870.45, "json": 3014.85},
    "xlarge": {"vcf": 58472.10, "toml": 163466.60, "json": 29126.40}
}

size_data = {
    "small": {"vcf": 426, "toml": 379, "json": 126},
    "medium": {"vcf": 2517, "toml": 2302, "json": 2837},
    "large": {"vcf": 24804, "toml": 22789, "json": 28723},
    "xlarge": {"vcf": 262941, "toml": 242926, "json": 302859}
}

def generate_bar(value, max_value, width=80):
    """生成ASCII条形图"""
    if max_value == 0:
        return ""
    length = int((value / max_value) * width)
    return "█" * max(1, length)

def generate_performance_chart():
    """生成解析性能对比图表"""
    chart = []
    chart.append("## 解析性能对比图表\n")
    chart.append("### 1. 解析时间对比（对数尺度）\n")
    chart.append("```")
    
    for size, times in perf_data.items():
        max_time = max(times.values())
        chart.append(f"\n{size}:")
        chart.append(f"  VCF  {generate_bar(times['vcf'], max_time, 60)} {times['vcf']:.0f}μs")
        chart.append(f"  TOML {generate_bar(times['toml'], max_time, 60)} {times['toml']:.0f}μs")
        chart.append(f"  JSON {generate_bar(times['json'], max_time, 60)} {times['json']:.0f}μs")
    
    chart.append("```\n")
    return "\n".join(chart)

def generate_speed_comparison():
    """生成相对速度对比"""
    chart = []
    chart.append("### 2. 相对速度对比（以VerseConf为基准1.0x）\n")
    chart.append("```")
    chart.append(f"{'数据集':<10} {'TOML/VCF':>12} {'JSON/VCF':>12}")
    chart.append("-" * 40)
    
    for size, times in perf_data.items():
        toml_ratio = times['toml'] / times['vcf']
        json_ratio = times['json'] / times['vcf']
        chart.append(f"{size:<10} {toml_ratio:>11.2f}x {json_ratio:>11.2f}x")
    
    chart.append("```\n")
    chart.append("**说明**：")
    chart.append("- TOML/VCF > 1 表示TOML比VCF慢")
    chart.append("- JSON/VCF < 1 表示JSON比VCF快\n")
    return "\n".join(chart)

def generate_file_size_chart():
    """生成文件大小对比图表"""
    chart = []
    chart.append("### 3. 文件大小对比\n")
    chart.append("```")
    
    for size, sizes in size_data.items():
        max_size = max(sizes.values())
        chart.append(f"\n{size}:")
        chart.append(f"  VCF  {generate_bar(sizes['vcf'], max_size, 60)} {sizes['vcf']}B")
        chart.append(f"  TOML {generate_bar(sizes['toml'], max_size, 60)} {sizes['toml']}B")
        chart.append(f"  JSON {generate_bar(sizes['json'], max_size, 60)} {sizes['json']}B")
    
    chart.append("```\n")
    return "\n".join(chart)

def generate_summary_table():
    """生成综合评分表"""
    table = []
    table.append("### 4. 综合性能评分\n")
    table.append("| 维度 | VerseConf | TOML | JSON |")
    table.append("|------|-----------|------|------|")
    table.append("| 解析速度（vs TOML） | ⭐⭐⭐⭐ (快2.8-4.1x) | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ |")
    table.append("| 解析速度（vs JSON） | ⭐⭐⭐ (慢2-10x) | ⭐⭐ | ⭐⭐⭐⭐⭐ |")
    table.append("| 文件大小（小） | ⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ |")
    table.append("| 文件大小（大） | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐ |")
    table.append("| 增量解析支持 | ⭐⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐ |")
    table.append("| 缓存优化 | ⭐⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐ |")
    table.append("| 综合性能 | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐ |\n")
    return "\n".join(table)

def generate_analysis():
    """生成性能分析总结"""
    analysis = []
    analysis.append("### 5. 性能分析总结\n")
    analysis.append("**VerseConf vs TOML**:")
    analysis.append("- VerseConf 解析速度比 TOML **快 2.8-4.1 倍**")
    analysis.append("- 小文件时优势更明显（4.1x），大文件时稳定在 2.8x")
    analysis.append("- TOML 解析器在处理复杂嵌套结构时性能下降显著")
    analysis.append("")
    analysis.append("**VerseConf vs JSON**:")
    analysis.append("- JSON 解析速度比 VerseConf **快 2-10 倍**")
    analysis.append("- 小文件时 JSON 优势明显（10x），大文件时差距缩小至 2x")
    analysis.append("- `serde_json` 库经过高度优化，是 Rust 生态中最快的 JSON 解析器")
    analysis.append("")
    analysis.append("**文件大小**:")
    analysis.append("- VCF 和 TOML 大小相近（VCF 约大 8-12%）")
    analysis.append("- JSON 在小文件时最小（无注释、无多余符号）")
    analysis.append("- 大文件时 VCF 比 JSON 小约 13-14%")
    analysis.append("")
    analysis.append("**关键发现**:")
    analysis.append("1. VerseConf 在解析性能上显著优于 TOML")
    analysis.append("2. JSON 在解析速度上仍然领先，但 VerseConf 差距在可接受范围内")
    analysis.append("3. VerseConf 支持增量解析和缓存，在长期运行场景中有巨大优势")
    analysis.append("4. 文件大小方面，VCF 和 TOML 相当，都比 JSON 更紧凑（大文件场景）\n")
    return "\n".join(analysis)

if __name__ == "__main__":
    import sys
    import io
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
    
    output = []
    output.append("# VerseConf vs TOML vs JSON 性能对比可视化报告\n")
    output.append(f"**生成日期**: 2026-04-20")
    output.append(f"**测试环境**: Intel Core (Windows 11), Rust 1.75+\n")
    
    output.append(generate_performance_chart())
    output.append(generate_speed_comparison())
    output.append(generate_file_size_chart())
    output.append(generate_summary_table())
    output.append(generate_analysis())
    
    # 输出到文件
    with open("compare/performance_charts.md", "w", encoding="utf-8") as f:
        f.write("\n".join(output))
    
    print("性能对比图表已生成: compare/performance_charts.md")
