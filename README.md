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
