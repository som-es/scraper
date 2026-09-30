#!/usr/bin/env bash
# Explicit fixture/parser selection. Live tests and output-only diagnostics are excluded.
set -euo pipefail
cd "$(dirname "$0")/.."

unit_filters=(
  scraper::legis_init::tests
  scraper::plenar::parser_contracts
  scraper::volksbg::
  scraper::decree::tests::test_derive_doc_number
  scraper::decree::tests::test_parse_
  scraper::decree::tests::test_search_refs_
  scraper::decree::tests::test_general_parse_
  scraper::delegates::tests::test_prev_name
  scraper::gov_officials::tests::test_unnecessary_
  scraper::gov_officials::tests::test_extract_gov_from_html
  scraper::gov_officials::tests::test_function_extraction_
  document::tests::
  text_utils::tests::
)
if [[ $# -eq 1 && "$1" == "--coverage" ]]; then
  # --no-report accumulates profiles, so explicitly clean previous measurements.
  cargo llvm-cov clean --workspace --offline --locked
  cargo llvm-cov --offline --locked --lib --no-report -- \
    "${unit_filters[@]}" --skip ::live:: --skip scratch_table \
    --skip test_prev_name_extraction_strache --skip test_prev_name_extraction_3727
  cargo llvm-cov --offline --locked --no-report \
    --test austria_parliament --test austria_plenary --test austria_parser_contracts
  cargo llvm-cov report --lcov --output-path target/coverage-parsers.lcov
  python3 scripts/coverage_summary.py target/coverage-parsers.lcov \
    --json target/coverage-parsers.json
elif [[ $# -eq 0 ]]; then
  cargo test --offline --locked --lib -- \
    "${unit_filters[@]}" --skip ::live:: --skip scratch_table \
    --skip test_prev_name_extraction_strache --skip test_prev_name_extraction_3727
  cargo test --offline --locked \
    --test austria_parliament --test austria_plenary --test austria_parser_contracts
else
  echo "Usage: bash scripts/test_austria_parsers.sh [--coverage]" >&2
  exit 2
fi
