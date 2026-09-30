#!/usr/bin/env python3
"""Summarize LCOV line coverage for Austrian scraper and shared Rust sources.

Exclude EU modules, separate test files and inline #[cfg(test)] modules.
This is line coverage of the selected test runs, not branch or scenario coverage.
"""

import argparse
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def inline_test_lines(source):
    """Locate inline test modules using their opening/closing indentation.

    Rustfmt places the module's closing brace at the same indentation as `mod`.
    Fail rather than silently report inflated coverage if that convention changes.
    """
    lines = source.splitlines()
    excluded = set()
    for index, line in enumerate(lines):
        if line.strip() != "#[cfg(test)]":
            continue
        if index + 1 >= len(lines):
            continue
        match = re.fullmatch(r"(\s*)(?:pub )?mod \w+ \{", lines[index + 1])
        if not match:
            continue  # External `mod tests;` files are excluded separately.
        closing = match[1] + "}"
        end = next((i for i in range(index + 2, len(lines)) if lines[i] == closing), None)
        if end is None:
            raise ValueError("cannot locate the end of an inline test module")
        excluded.update(range(index + 1, end + 2))  # LCOV lines are 1-based.
    return excluded


def summarize(lcov):
    coverage = {}
    current = None
    excluded = set()
    for line in lcov.splitlines():
        if line.startswith("SF:"):
            path = Path(line[3:]).resolve()
            current = None
            if not path.is_relative_to(ROOT / "src"):
                continue
            relative = path.relative_to(ROOT)
            if "eu_parliament" in relative.parts or relative.stem == "eu_parliament":
                continue
            if "tests" in relative.parts or relative.stem == "tests":
                continue
            current = str(relative)
            coverage.setdefault(current, {})
            excluded = inline_test_lines(path.read_text())
        elif line.startswith("DA:") and current is not None:
            number, hits, *_ = line[3:].split(",")
            number, hits = int(number), int(hits)
            if number not in excluded:
                coverage[current][number] = max(hits, coverage[current].get(number, 0))
        elif line == "end_of_record":
            current = None

    files = {}
    for name, hits in sorted(coverage.items()):
        if not hits:
            continue
        covered = sum(count > 0 for count in hits.values())
        files[name] = {
            "covered": covered,
            "total": len(hits),
            "percent": round(100 * covered / len(hits), 2),
            "uncovered_lines": sorted(number for number, count in hits.items() if count == 0),
        }
    total = sum(file["total"] for file in files.values())
    if not total:
        raise ValueError("LCOV contains no matching production source lines")
    covered = sum(file["covered"] for file in files.values())
    return {"covered": covered, "total": total, "percent": round(100 * covered / total, 2), "files": files}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("lcov", type=Path)
    parser.add_argument("--json", type=Path, help="also save counts and uncovered line numbers")
    args = parser.parse_args()
    result = summarize(args.lcov.read_text())
    print("| Source | Covered / instrumented lines | Coverage |")
    print("|---|---:|---:|")
    for name, counts in result["files"].items():
        print(f"| {name} | {counts['covered']} / {counts['total']} | {counts['percent']:.2f}% |")
    print(f"| **Total** | **{result['covered']} / {result['total']}** | **{result['percent']:.2f}%** |")
    if args.json:
        args.json.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
