#!/usr/bin/env python3
"""由真实基准结果生成 VerseConf vs TOML vs JSON 性能对比报告。

这个脚本**不内置任何备用数字**。此前它把 perf_data / size_data 手写成 Python
dict，`performance_charts.md` 与 README 的性能段都取自这里，于是「比 TOML 快
2.8-4.1 倍」这句在没有任何一次真实运行支撑的情况下被复制进了对外文档。

现在的数据来源只有一个：`compare/benchmark_results.json`，由

    cargo run --release -p verseconf-compare --bin benchmark \\
        -- --json compare/benchmark_results.json

生成。读不到结果文件就直接失败退出，不会退回到任何硬编码数据。

用法：
    python3 compare/generate_charts.py [结果文件] [输出文件]
"""

from __future__ import annotations

import json
import platform
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_RESULT = SCRIPT_DIR / "benchmark_results.json"
DEFAULT_OUTPUT = SCRIPT_DIR / "performance_charts.md"

# 报告里的数据集顺序（而不是 JSON 里的字典序）
SIZE_ORDER = ["small", "medium", "large", "xlarge"]


def fail(message: str) -> "NoReturn":  # noqa: F821 - 仅用于类型可读性
    print(f"错误: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_results(path: Path) -> dict:
    if not path.exists():
        fail(
            f"找不到基准结果 {path}。\n"
            f"      先生成数据与结果：\n"
            f"        cargo run -p verseconf-compare --bin generate_test_data\n"
            f"        cargo run --release -p verseconf-compare --bin benchmark -- "
            f"--json {path}\n"
            f"      本脚本不会退回任何硬编码数字。"
        )

    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        fail(f"{path} 不是合法 JSON: {exc}")

    if data.get("schema") != 1:
        fail(f"{path} 的 schema 是 {data.get('schema')!r}，本脚本只认识 1")

    datasets = data.get("datasets") or []
    if not datasets:
        fail(f"{path} 里没有任何数据集")

    known = {d.get("name") for d in datasets}
    missing = [s for s in SIZE_ORDER if s not in known]
    if missing:
        fail(f"{path} 缺少数据集: {', '.join(missing)}")

    for entry in datasets:
        for fmt in ("vcf", "toml", "json"):
            value = (entry.get(fmt) or {}).get("avg_us")
            if not isinstance(value, (int, float)) or value <= 0:
                fail(
                    f"数据集 {entry.get('name')!r} 的 {fmt} 耗时是 {value!r}，"
                    f"不是正数——基准没有真正跑出结果"
                )

    return data


def ordered(data: dict) -> list[dict]:
    by_name = {d["name"]: d for d in data["datasets"]}
    return [by_name[name] for name in SIZE_ORDER]


def rustc_version() -> str:
    try:
        out = subprocess.run(
            ["rustc", "--version"],
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
        if out.returncode == 0:
            return out.stdout.strip()
    except (OSError, subprocess.SubprocessError):
        pass
    return "未采集"


def bar(value: float, max_value: float, width: int = 60) -> str:
    if max_value <= 0:
        return ""
    return "█" * max(1, int((value / max_value) * width))


def section_times(data: dict) -> str:
    lines = ["## 1. 解析时间对比\n", "```"]
    for entry in ordered(data):
        times = {fmt: entry[fmt]["avg_us"] for fmt in ("vcf", "toml", "json")}
        max_time = max(times.values())
        lines.append(f"\n{entry['name']}:")
        for label, fmt in (("VCF ", "vcf"), ("TOML", "toml"), ("JSON", "json")):
            lines.append(
                f"  {label} {bar(times[fmt], max_time)} {times[fmt]:.2f}μs"
            )
    lines.append("```\n")
    return "\n".join(lines)


def section_ratios(data: dict) -> str:
    lines = ["## 2. 相对速度对比（以 VerseConf 为基准 1.0x）\n", "```"]
    lines.append(f"{'数据集':<10} {'TOML/VCF':>12} {'JSON/VCF':>12}")
    lines.append("-" * 40)
    for entry in ordered(data):
        toml_ratio = entry["toml"]["avg_us"] / entry["vcf"]["avg_us"]
        json_ratio = entry["json"]["avg_us"] / entry["vcf"]["avg_us"]
        lines.append(f"{entry['name']:<10} {toml_ratio:>11.2f}x {json_ratio:>11.2f}x")
    lines.append("```\n")
    lines.append("- TOML/VCF > 1 表示 TOML 比 VerseConf 慢")
    lines.append("- JSON/VCF < 1 表示 JSON 比 VerseConf 快\n")
    return "\n".join(lines)


def section_sizes(data: dict) -> str:
    lines = ["## 3. 文件大小对比\n", "```"]
    for entry in ordered(data):
        sizes = {fmt: entry[fmt]["bytes"] for fmt in ("vcf", "toml", "json")}
        max_size = max(sizes.values())
        lines.append(f"\n{entry['name']}:")
        for label, fmt in (("VCF ", "vcf"), ("TOML", "toml"), ("JSON", "json")):
            lines.append(
                f"  {label} {bar(sizes[fmt], max_size)} {sizes[fmt]}B"
            )
    lines.append("```\n")
    return "\n".join(lines)


def section_conclusion(data: dict) -> str:
    entries = ordered(data)
    toml_ratios = [e["toml"]["avg_us"] / e["vcf"]["avg_us"] for e in entries]
    json_ratios = [e["json"]["avg_us"] / e["vcf"]["avg_us"] for e in entries]

    lines = ["## 4. 结论（全部由上面的数字直接得出）\n"]
    lines.append(
        f"- VerseConf 相对 TOML：{min(toml_ratios):.2f}x–{max(toml_ratios):.2f}x"
        f"（>1 表示 VerseConf 更快）"
    )
    lines.append(
        f"- VerseConf 相对 JSON：{min(json_ratios):.2f}x–{max(json_ratios):.2f}x"
        f"（<1 表示 JSON 更快）"
    )
    lines.append("- 三种格式的语料是同一份配置的等价写法；VCF 侧多出的字节来自注释。")
    lines.append(
        "- 这些数字只反映本机、本工具链、本数据集下的解析耗时，"
        "不能外推成跨硬件的结论。"
    )
    lines.append("")
    return "\n".join(lines)


def main(argv: list[str]) -> int:
    result_path = Path(argv[1]).resolve() if len(argv) > 1 else DEFAULT_RESULT
    output_path = Path(argv[2]).resolve() if len(argv) > 2 else DEFAULT_OUTPUT

    data = load_results(result_path)

    generated_at = data.get("generated_at_unix")
    if isinstance(generated_at, (int, float)) and generated_at > 0:
        stamp = datetime.fromtimestamp(generated_at, tz=timezone.utc).strftime(
            "%Y-%m-%d %H:%M UTC"
        )
    else:
        stamp = "未知"

    uname = platform.uname()
    env_line = " / ".join(
        part
        for part in [
            f"{uname.system} {uname.release}",
            uname.machine,
            uname.processor or None,
        ]
        if part
    )

    header = [
        "# VerseConf vs TOML vs JSON 性能对比可视化报告\n",
        "> 本文件的每一个数字都来自 `compare/benchmark_results.json`，"
        "该文件由基准二进制真实运行产出。",
        "> 生成脚本不内置任何备用数据，读不到结果文件即失败。\n",
        f"**基准运行时间**: {stamp}",
        f"**测试环境**: {env_line}",
        f"**目标平台**: {data.get('target_os')}-{data.get('target_arch')}",
        f"**工具链**: {rustc_version()}",
        f"**计时方法**: 每个格式 {data.get('rounds')} 轮取中位数，"
        "轮内迭代次数见 `benchmark_results.json`\n",
        "---\n",
    ]

    output = [
        "\n".join(header),
        section_times(data),
        section_ratios(data),
        section_sizes(data),
        section_conclusion(data),
    ]

    output_path.write_text("\n".join(output), encoding="utf-8")
    print(f"性能对比报告已生成: {output_path}")
    print(f"  数据来源: {result_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
