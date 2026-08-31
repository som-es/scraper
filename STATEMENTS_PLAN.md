# Statements scraper — implementation plan (draft, NOT yet implemented)

## Research findings (verified live against parlament.gv.at, 2026-08-31)

### List endpoint — filter 142 = "Alle Stellungnahmen"

`POST /Filter/api/filter/data/142?js=eval&showAll=true&export=true`
(= the `FILTER_PATH` already in `statements.rs`; the page
`/beteiligen/alle-beteiligungen/alle-stellungnahmen` embeds
`{"bez":"FP_142","listeId":142,"params":{"GP_CODE":["XXVIII"]},"fixedParams":{"VHG":["1"]}}`)

Payload keys (all values are arrays, all optional):

| key | meaning | example |
|---|---|---|
| `GP_CODE` | GP of the **statement** | `["XXVIII"]` |
| `ITYP` | ityp of the **statement** | `["SNME"]` |
| `VHG` | site's fixed param (`fixedParams {"VHG":["1"]}`) | `["1"]` |
| `DATUM` | **OR-set of exact days**, `TT.MM.JJJJ` — *no* range support (see below) | `["15.09.2025","01.10.2025"]` |
| `BEZUG_GP_CODE` / `BEZUG_ITYP` / `BEZUG_INR` | filter by referenced Gegenstand | `["XXVIII"] / ["ME"] / [126]` |

* `selectedStage` is **only a URL/UI param** — 833 rows with and without it, no effect.
* `INR` of the statement itself is **not** filterable (`{"GP_CODE","ITYP","INR"}` → 0 rows).

### Date filtering: ranges are NOT supported (verified live, per your request)

| payload | result |
|---|---|
| `DATUM_VON` ISO / `TT.MM.JJJJ`, ± `DATUM_BIS`, also `VON`/`BIS` | **ignored** — identical to unfiltered (6009 rows) |
| `DATUM_SORT`, or `DATUM` with ISO values (`"2025-09-01"`) | **0 rows** |
| `DATUM: ["01.09.2025","01.10.2025"]` | 7 rows: 09-01 ×2, 10-01 ×5 |
| `DATUM: ["02.09.2025","30.09.2025"]` | 14 rows: 09-02 ×8, 09-30 ×6 — **nothing in between** |
| `DATUM: ["15.09.2025","20.09.2025","01.09.2025"]` | 5 rows on exactly those 3 days (N dates OK) |

⇒ `DATUM` is a plain multi-value filter (exact days, `OR`), so list 142 **cannot** do date
ranges. Drop `date_payload` from `StatementsPayload`; expose `datum: Vec<NaiveDate>` formatted
`%d.%m.%Y` instead (it doubles as the bulk-chunking key below).
* Verified totals: SN 274 648 · SNME 211 512 · SPET 4 346 · SBI 6 828 · **SPET-BR 0**
  (`/gegenstand/…/SPET-BR/1` → 404). Real statement ityps = `SN, SNME, SPET, SBI`.
* `ITYP` + `BEZUG_*` combine fine → `(reference, ityp)` narrows a query.
* **Result-size limit / paging** — `showAll=true&export=true` returns at most *one* page (default
  page size 100 000; `count` is capped at `pages * pagesize`: `{}` → `count 300 000, pages 3,
  rows 100 000`). So `GP_CODE:XXVII + ITYP:SN` (`count 272 748`) is only ~37 % readable, and
  `showAll=true&page=2` → **0 rows**. Paging instead uses **URL params** `pagesize` + `page`
  (1-based) *without* `showAll`:
  ```
  ?js=eval&pagesize=20000&page=1 -> count 272748, pages 14, 20000 rows, last inr 40744
  ?js=eval&pagesize=20000&page=2 -> 20000 rows, first inr 40745   (contiguous, stable order)
  ```
  Payload keys `page`/`pageStart` are ignored. Loop `1..=pages` from the first response, or chunk
  by `DATUM` day-sets (robust against inserts mid-scrape). Always send `GP_CODE` anyway
  (XXVIII → 6009 SNME rows).
* Confirmed via `?json=true` on ME/AUA detail pages: their embedded "Stellungnahmen" filter is
  `FP_142_vhg` with exactly `{BEZUG_GP_CODE, BEZUG_ITYP, BEZUG_INR}` → matches the user example.

Row layout (index = header `rnr` − 1; identical for all payloads of list 142):

```
0 GP_CODE   1 ITYP    2 INR      3 ZUKZ   4 DATUM("27.11.2025")  5 DATUM_SORT("2025-11-27T12:00:00")
6 Von (html: <a href="/gegenstand/XXVIII/SN/798/">Name (798/SN)</a>, or plain
  "Nicht-öffentliche Stellungnahme (276303/SN)" for unpublished ones)
7 BEZUG_GP_CODE  8 BEZUG_INR("48")  9 BEZUG_ITYP("AUA")  10 SORT  11 Datumsort
12 Unterstützungen (u64)  13 Details("Art:\nStellungnahme\n<br />Kürzel:\nSN\n<br />")
14 (null)  15 Nr = zitation("798/SN", "14/SN-46/ME")  16 Zu = ref zitation("48/AUA")
17 Beteiligen (html "Unterstützen" link or null)  18 Bezug_Link("/gegenstand/XXVIII/AUA/48")
19 TYP = "I" | "P"    20 wentry_id  21 own INR (u64)  22 VHG("0"/"1")
```

**Organisation vs. private person = column 19 (`TYP`): `I` = Institution, `P` = Privatperson.**
Verified on XXVIII/SNME: 1039 `I` / 1899 `P`; `I` rows are "Amt der Vorarlberger
Landesregierung", "BM f. Wohnen …"; `P` rows are "Grießer, Wilfried", and all
"Nicht-öffentliche Stellungnahme". The **detail** page has no such flag → the classification
must come from the list row (cf. `GegenstandInfo` in `ministrial_proposals.rs`).

Gegenstand path = `/{col0}/{col1}/{col2}` (`/gegenstand/XXVIII/SNME/4729`) — always available,
even for the unpublished rows whose column 6 has no href. ITYP differs per statement, as expected.

### Detail page — `GET /gegenstand/{gp}/{ityp}/{inr}?json=true` → `content`

Always present: `type, sntype, approvaltype, title, zitation, description, gp_code, ityp, inr,
doktyp, update, einlangen, nr_gp_code, intranet, group, deleted, history_ind, vhg_fertig,
vhg_fertig_docs, process, statementsstate, approvalstate, approvals|statements, stages,
reference, topics, headwords, eurovoc`. Optional: `names, statement, documents, appformlink,
formlink, shortinfo`.

* `statement` — HTML free text (`<br>` separated), mostly private persons → `extract_text_from_html`.
* `documents` — same nested shape as elsewhere → reuse `crate::documents(&[..], BASE_URL)`.
  Persons with a `statement` usually have no documents; organisations/Ministerien usually have
  a PDF ("Stellungnahme", `/dokument/…/imfname_*.pdf` or `/PtWeb/api/s3serv/file/<uuid>`);
  SPET 8 has both.
* `names[]` — `funktext` = "Stellungnahme von" (only value seen), plus
  * plain person/org: `{"name": "Haslinger, Christian (7012 Zagersdorf)"}` /
    `{"name": "Amt der Vorarlberger Landesregierung"}` (no `url`),
  * minister/MP signing for an institution: `{"name":"Mag. Klaudia Tanner","frak_code":"V",
    "ltext":"Bundesministerium für Landesverteidigung","url":"/person/6485"}` → `person_url_to_id`.
  * `names: []` for unpublished ("Nicht-öffentliche") statements.
* `reference[]` — `{text:"Bezug zu", subject, zitation, url, art}`; `art` `B` (Bezug) / `HG`
  (Hauptgegenstand, seen on AUA pages). The **last** entry is the primary one and equals the
  row's `BEZUG_*` (AUA first, actual Gegenstand last). → parse each with
  `extract_reference_from_gegenstand_url(url)`.
* `stages[]` `{date:"30.06.2026", text:"Einlangen im Nationalrat"}`; `einlangen`
  (`2026-07-06T00:00:00`) → created_at, `update` → updated_at, both via
  `convert_vienna_datetime_to_utc` (same format as proposals/ministerial).
* `topics` / `eurovoc` / `headwords` = bubble rendertypes → existing `extract_topics(_, "topics")`,
  `"eurovoc"`, `"headwords"`.
* `approvals` (statement pages) / `statements` (target pages) = `{new, personform|personform,
  filter:{data:{definition:{params:{BEZUG_*}}}}}` → support count is easier from row col 12.

## Proposed code

```rust
pub struct Statement {                     // -> common-scrapes/src/statements.rs
    id: u64, inr: u64, ityp: String, gp: String, zitation: String,
    title: String, description: String,
    statement_text: Option<String>,        // from content.statement, html -> text
    documents: Vec<Document>,
    authors: Vec<StatementAuthor>,
    author_kind: StatementAuthorKind,      // Organisation | PrivatePerson (col 19)
    references: Vec<Reference>,            // content.reference, primary = last
    primary_reference: Option<Reference>,  // == row BEZUG_*
    created_at: DateTime<Utc>, updated_at: Option<DateTime<Utc>>,
    topics / eurovoc_topics / other_keyword_topics: Vec<String>,
    supports: u64, stage: Option<String>,  // row col 12 / stages[..].text
    path: String,                          // /gegenstand/XXVIII/SNME/4729
}
pub struct StatementAuthor { name: String, delegate_id: Option<u64>,
    party_code: Option<String>, institution: Option<String> /*ltext*/ }
pub enum StatementAuthorKind { Organisation, PrivatePerson }  // from TYP I/P
pub struct StatementListItem { path: String, gp: String, ityp: String, inr: u64,
    zitation: String, datum: NaiveDate, author_name: String, kind: StatementAuthorKind,
    reference: Option<Reference>, supports: u64 }   // like GegenstandInfo
```

`StatementsPayload::as_json()` emits
`{"GP_CODE": gp, "VHG": ["1"], "ITYP": […], "DATUM": ["01.09.2025", …]}` plus,
when `reference: Option<Reference>` is `Some`, `{"BEZUG_GP_CODE":[gp], "BEZUG_ITYP":[ityp],
"BEZUG_INR":[inr]}`. `SNME => ME`: `StatementItyp::from_reference_ityp("ME") -> SNME`
(ME→SNME, PET→SPET, BI→SBI, A/I/AUA→SN) used *only* to default `ityp` from the reference, and
`StatementItyp::reference_ityp()` for the reverse.

Trait (rename `raw_ministrial_prop_*` → `raw_statement_into_statement`,
`ministrial_prop_paths` → `statement_paths`):
* `statement_paths(Value) -> Vec<StatementListItem>` (header-driven index map with fallback to
  the fixed indices above; skips `"Nicht-öffentliche"` rows)
* `get_statement(&self, StatementListItem) -> Result<Statement, ScraperError>` using
  `self.cache(path)` (the existing `Scraper::cache`) + `Self::raw_statement_into_statement(raw, item)`
* `post_statements_payload(StatementsPayload) -> Value` (already drafted), keep `FILTER_PATH`
* convenience `statements(payload) -> Vec<Statement>`.

## Tests (network, same style as `ministrial_proposals.rs` / `proposals.rs`)

1. `test_statements_payload_json` — pure unit: no ref / ref-only / ityp-only / full payload
   equals the literal JSON from the task description (`{"selectedStage":["101"],
   "BEZUG_GP_CODE":["XXVIII"],"BEZUG_ITYP":["ME"],"BEZUG_INR":[126]}` minus `selectedStage`).
2. `test_snme_4729` — parse `/gegenstand/XXVIII/SNME/4729`: zitation `78/SN-126/ME`, inr 4729,
   author `Haslinger, Christian (7012 Zagersdorf)`, `PrivatePerson`, statement_text non-empty,
   no documents, primary_reference == `Reference{XXVIII, ME, 126}`, topics contain "Inneres und
   Recht", eurovoc contains "Gerichtswesen", created_at 2026-07-06 (Vienna→UTC).
3. `test_snme_1510_organisation` — org: `Amt der Vorarlberger Landesregierung`, `Organisation`,
   documents[0] = `Document{title:"Stellungnahme", link:
   "https://www.parlament.gv.at/PtWeb/api/s3serv/file/304bc548-271e-41db-9653-465a4681ca99",
   ftype: DigitalPDF}` (s3serv link kept as-is; `type: "PDF"` → `FileType::DigitalPDF`).
4. `test_sn_552_minister_is_organisation` — author delegate_id 23963, party "S", institution
   "Bundesministerium für Wohnen, Kunst, Kultur, Medien und Sport", kind Organisation,
   references `[{AUA 33}, {A 372}]`, primary A 372.
5. `test_spet_2795` / `test_sbi_5090` — SPET + SBI happy paths (documents + person).
6. `test_statement_paths_skips_unpublished` — XXVIII/SN contains
   "Nicht-öffentliche Stellungnahme (386/SN)" rows (`names: []` on the detail page); assert
   `/gegenstand/XXVIII/SN/386` is **not** among `statement_paths` but the neighbouring public
   ones are, and that `statement_paths` never returns a path whose `content.names` is empty.
7. `test_statement_paths_for_reference_me_126` — payload with
   `reference = Some(XXVIII/ME/126)` → ~841 rows, all `primary_reference == XXVIII/ME/126`,
   every path matches `^/gegenstand/XXVIII/SNME/\d+$`, both `I` and `P` present; parse the
   first ~10 and assert no `Err`.
8. `test_statement_paths_row_columns_match_header` — assert `feld_name`s at the hardcoded
   indices (GP_CODE/ITYP/INR/BEZUG_*/TYP) so the index constants can't silently rot.

## Decisions (user, 2026-08-31)

1. `Statement` (+ `StatementAuthor`, `StatementAuthorKind`) go into **common-scrapes**
   (new `common-scrapes/src/statements.rs`, `pub mod statements; pub use statements::*;`, derive
   `Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize` like the other types there).
   `StatementListItem` stays in the scraper crate (scrape metadata, like `GegenstandInfo`).
2. Naming as proposed (`statement_paths` / `get_statement` / `raw_statement_into_statement` /
   `post_statements_payload`); payload method stays `as_json()`.
3. **Keep** `StatementItyp::SpetBr` (no live data, kept for the UI's ityp list).
4. "Nicht-öffentliche Stellungnahme" rows are **filtered out in `statement_paths`**; no `public`
   flag on `Statement`.
5. s3serv PDF links: keep `documents()` as-is.
6. `Statement.id: u64 = inr`; keep `supports: u64` (row col 12).
7. No date range — `datum: Vec<NaiveDate>` day-set (`DATUM`, `%d.%m.%Y`).
8. Bulk scrape: helper that loops `1..=pages` with `pagesize`/`page` instead of `showAll=true`.

## Still open (defaults in place unless you object)

* `selectedStage` → dropped from the payload (verified no-op).
* Scope: implement all four live ityps (`SN`, `SNME`, `SPET`, `SBI`) now, not SNME-only.
* `stage: Option<String>` from `stages[0].text` ("Einlangen im Nationalrat") — keep or drop?
