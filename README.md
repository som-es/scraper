# scraper

## Description

The scraper library for fetching data from parliament APIs.

## Submodule updates

```bash
git submodule update --init --recursive
git submodule update --recursive --remote
```

## Austrian parliament regression tests

Run the fixture-based suite independently of the live scraper tests:

```bash
cargo test --locked --test austria_parliament
```

These tests call the parsers directly with checked-in responses from
`test-data/gegenstand` and a stenographic HTML snippet. They cover proposal
metadata, document links and file types, Vienna-to-UTC conversion, votes in both
Austrian councils, pending initiatives, JSON output, invalid input, and extracted
plenary text. Expected values are taken from the source fixtures, not generated
from scraper output.

The selected legislative fixtures have no roll-call votes requiring additional
HTTP requests. No running services or scraper cache are needed. Cargo may need
to download build dependencies initially; add `--offline` once they are available.
The general `cargo test` command also includes existing live tests and EU tests.

Run the full legislative-initiative fixture tests and the full-session tests:

```bash
cargo test --locked --lib scraper::legis_init::tests
cargo test --locked --test austria_parliament --test austria_plenary
```

The legislative suite shares one fixture-cache setup for JSON and linked HTML.
Its live filter test is ignored by default and uses a fixed historical period:

```bash
cargo test --locked --lib scraper::legis_init::tests::live -- --ignored
```

`--offline` only disables Cargo dependency downloads. To verify runtime network
independence as well, on Linux with user namespaces enabled run:

```bash
unshare --user --map-root-user --net cargo test --offline --locked --lib scraper::legis_init::tests
unshare --user --map-root-user --net cargo test --offline --locked --test austria_parliament --test austria_plenary
```

The five previously disabled bulk diagnostic scans are consolidated in an
example. It reports law document types, majority requirements, named votes,
rejections, suspicious delegate names, and request/parse failures. It contacts
the live website and is never run by `cargo test`:

```bash
cargo run --example inspect_legislative_initiatives -- XXVII
```

The former output-only `3373/A` test, pending `104/A` test, and string-only
leading-zero test are covered by `austria_parliament`. Historical fixture cases
remain in the legislative unit tests; formerly assertion-free cases now check
their results. The ORF case uses its saved law response instead of an unrelated
live request. The missing roll-call subsection case uses the saved full protocol.

The focused suites do not establish coverage of every Austrian scraper module,
HTTP failure handling, or changes to the live website. Existing tests elsewhere
may still require the network. The broad run measured 90.37% line coverage, but includes live tests and is not
a parser-only quality measure.


## Reproducible Austrian parser tests

Run the explicit parser/fixture selection (Cargo dependencies must already be available):

```bash
bash scripts/test_austria_parsers.sh
```

On Linux, also disable runtime networking to detect accidental live requests:

```bash
unshare --user --map-root-user --net bash scripts/test_austria_parsers.sh
```

With `cargo-llvm-cov` and `llvm-tools-preview` installed, create a fresh report:

```bash
unshare --user --map-root-user --net bash scripts/test_austria_parsers.sh --coverage
```

The report is written to `target/coverage-parsers.lcov` and
`target/coverage-parsers.json`. The summary still counts all instrumented Austrian
and shared source lines, including HTTP/cache orchestration; it is not an isolated
parser percentage. Test selection is explicit in the script; unrelated live tests
and output-only diagnostic scans are not acceptance tests for this scope.
