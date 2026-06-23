# ADO Recordset Parser

This crate parses ADO persisted Recordset XML and ADTG, writes ADO XML and
ADTG, and provides ADTG inspection, corpus verification, roundtrip conversion,
and byte-level diff tooling.

The target persistence provider is MDAC 2.8 / ADO 2.8. The Rust code itself
does not require ADO.

## Quick Start

```bash
cargo test
cargo run -- parse --input corpus/generated/types_basic.adtg --json
cargo run -- write-xml --input corpus/generated/types_basic.adtg --output types_basic.xml
cargo run -- write-adtg --input corpus/generated/types_basic.xml --output types_basic.adtg
```

Library callers can parse either persistence format with the auto-detecting
API, then write XML or ADTG from the validated `Recordset` model:

```rust
let recordset = tablegram::parse_recordset_file("recordset.adtg")?;
let xml_bytes = tablegram::write_ado_xml(&recordset)?;
let adtg_bytes = tablegram::write_adtg(&recordset)?;
```

For ANSI ADTG text fields, choose the legacy codepage explicitly when the
consumer is not using the default Korean corpus behavior:

```rust
let options = tablegram::AdtgWriteOptions::default()
    .with_ansi_encoding_label(b"windows-1252")
    .expect("known encoding label");
let adtg_bytes = tablegram::write_adtg_with_options(&recordset, options)?;
```

## Why XML First

Microsoft documents two ADO `Recordset.Save` formats:

- `adPersistADTG` = proprietary binary Advanced Data TableGram.
- `adPersistXML` = open XML format.

The XML format has a documented schema and data section, so this crate parses
and writes it directly. ADTG is handled as a compatibility target: generate
matching XML/ADTG files from COM, roundtrip ADTG back through ADO, and compare
byte and materialized-view changes while expanding parser and writer coverage.

Useful Microsoft references:

- https://learn.microsoft.com/en-us/office/client-developer/access/desktop-database-reference/save-method-ado
- https://learn.microsoft.com/en-us/office/client-developer/access/desktop-database-reference/xml-persistence-format
- https://learn.microsoft.com/en-us/office/client-developer/access/desktop-database-reference/schema-section
- https://learn.microsoft.com/en-us/office/client-developer/access/desktop-database-reference/data-section
- https://learn.microsoft.com/en-us/office/client-developer/access/desktop-database-reference/datatypeenum
- https://www.microsoft.com/en-us/download/details.aspx?id=21995

## Build

From this directory:

```bash
cargo test
cargo run -- --help
```

The CLI is enabled by the default `cli` feature. Library-only consumers can
disable default features to avoid the `clap` dependency:

```toml
tablegram = { version = "0.1", default-features = false }
```

The Windows-only COM oracle commands additionally require `--features oracle`.

## Parser Tests And Fuzzing

Normal tests include generated MDAC XML/ADTG corpus checks and deterministic
mutation cases:

```bash
cargo test
```

For coverage-guided fuzzing, install `cargo-fuzz` and run the harness against
the checked-in parse-any seed corpus. The seed corpus includes generated,
variant, fuzz, and shaped/chaptered ADTG/XML artifacts:

```bash
cargo install cargo-fuzz --locked
bash tools/verify_fuzz_parse_any.sh 60
```

The fuzz target calls the unified `parse_recordset_bytes` API and XML parser on
every input, plus the ADTG inspector and native ADTG parser on inputs detected
as binary. The verifier script copies `fuzz/corpus/parse_any` to a temporary
directory first, so libFuzzer's minimized mutations do not modify the checked
seed corpus.

Default library parser entry points do not impose application resource caps.
Callers parsing untrusted files should set `ResourceLimits` with
`RecordsetParseOptions::with_resource_limits` for input length, row and field
counts, value payloads, aggregate value bytes, and XML decimal exponent
expansion according to their own request and memory budgets.

## Corpus And Oracle Verification

The checked corpus is part of the compatibility contract. The portable gate
does not require ADO:

```bash
cargo run -- verify-native-corpus --dir corpus/generated
cargo run -- verify-native-corpus --dir corpus/fuzz
cargo run -- verify-native-corpus --dir corpus/exhaustive
cargo run -- verify-native-corpus --dir corpus/variant
cargo run -- verify-native-corpus --dir corpus/sqlserver_sales
cargo run -- verify-native-corpus --dir corpus/shape
```

The Windows-only COM oracle CLI commands are compiled only with
`--features oracle`.

## Parse Recordsets

```bash
cargo run -- parse --input corpus/generated/strings_korean_unicode.xml --json
cargo run -- parse --input corpus/generated/strings_korean_unicode.adtg --json
```

`parse` is portable Rust-only parsing for both ADO XML and checked ADTG
Recordset persistence files. It emits the same `Recordset` model for both
formats. XML parsing also preserves `MSDataShape` chapter fields as nested
`Recordset` values. Native ADTG parsing supports pinned `MSDataShape` chapter
layouts with sibling chapter fields and checked nested grandchild and
great-grandchild chapters, using one or more `RELATE` key pairs per chapter and
preserving child rows as nested `Recordset` values.

Library callers can use the same auto-detecting Rust parser directly:

```rust
let recordset = tablegram::parse_recordset_bytes(bytes)?;
let recordset = tablegram::parse_recordset_file("recordset.adtg")?;
let adtg = tablegram::adtg::parse_adtg_bytes(bytes)?;
let adtg = tablegram::adtg::parse_adtg_file("recordset.adtg")?;
let xml_bytes = tablegram::write_ado_xml(&recordset)?;
let adtg_bytes = tablegram::write_adtg(&recordset)?;
let western_adtg = tablegram::write_adtg_with_options(
    &recordset,
    tablegram::AdtgWriteOptions::default()
        .with_ansi_encoding_label(b"windows-1252")
        .expect("known encoding label"),
)?;
```

### WASM / Web usage (no incremental parser API)

The web path currently uses only:

```rust
parse_recordset_json(bytes)
```

`ADODB.Stream` is not a network protocol. If a server sends bytes from
`Recordset.Save` using chunked HTTP response writes, those chunks are just plain
byte slices.

Because there is no COM/stream-specific framing, the browser must combine
chunks in order and then call the existing parser once.

```js
const chunks = [];
let total = 0;
const reader = (await fetch('/sample')).body.getReader();
for (;;) {
  const { done, value } = await reader.read();
  if (done) break;
  chunks.push(value);
  total += value.byteLength;
}
const merged = new Uint8Array(total);
let offset = 0;
for (const c of chunks) {
  merged.set(c, offset);
  offset += c.byteLength;
}
const json = JSON.parse(tablegram_wasm.parse_recordset_json(merged));
```

If you need true incremental parsing, it requires a new parser API and is not
implemented by default.

## Write Recordsets

The XML writer serializes validated `Recordset` values as UTF-8 ADO XML,
including flat rowsets and XML-representable `MSDataShape` chapter rowsets. It
preserves chapter `rs:relation` metadata required by XP/MDAC ADO,
`rs:basecatalog`/`rs:baseschema`/`rs:basetable`/`rs:basecolumn` provider
metadata, flat root pending insert/update/delete sections, updated nulls with
`rs:forcenull`, and MDAC's XML text-storage behavior for `adVariant`.

The XML writer returns explicit errors for shaped root updates or nested
chapter rowsets that contain pending row changes because those cases do not
roundtrip through ADO XML with the same materialized chapter view.

```bash
cargo run -- write-xml --input corpus/generated/types_basic.adtg --output types_basic.xml
cargo run -- write-xml --input corpus/shape/orders_lines_product_shape.adtg --output orders_lines_product.xml
```

The ADTG writer serializes validated `Recordset` values as MDAC-compatible
ADTG. Flat rowsets include current, inserted, updated, and deleted rows and the
scalar ADTG field types covered by the generated, exhaustive, variant, and fuzz
corpora. Shaped rowsets include checked `MSDataShape` chapter layouts, sibling
and nested child chapters, relation metadata, pending insert/update/delete row
groups, `NEW`/`CALC`/aggregate fields as materialized by MDAC, and the provider
source-column/catalog/schema descriptors required by the SQL Server-backed
pending shape fixtures.

The ADTG writer returns explicit errors for unsupported scalar type codes,
inconsistent row shapes, non-sequential ordinals, zero-field rowsets, and
provider base-table metadata layouts outside the checked corpus model.

```bash
cargo run -- write-adtg --input corpus/generated/types_basic.adtg --output types_basic.adtg
cargo run -- write-adtg --input corpus/exhaustive/flat_Integer_states.adtg --output flat_Integer_states.adtg
cargo run -- write-adtg --input corpus/shape/orders_lines_product_pending_shape.adtg --output orders_lines_product_pending_shape.adtg
```

## Inspect ADTG Bytes

```bash
cargo run -- inspect-adtg --input corpus/generated/strings_korean_unicode.adtg
```

For the explicit native ADTG command, use:

```bash
cargo run -- parse-adtg-native --input corpus/exhaustive/flat_Integer_states.adtg --json
```

For comparison against Windows' installed MDAC/Windows DAC `MSPersist`
provider, open the file through COM and build a `Recordset` from a direct
field/value dump:

```powershell
cargo run --features oracle -- parse-adtg-com --input corpus\generated\strings_korean_unicode.adtg --json
```

The portable native ADTG decoder supports checked Recordset ADTG files,
including flat generated samples, exhaustive boundary/state fixtures, accepted
`adVariant` subtypes, randomized multi-field fuzz fixtures, and checked
`MSDataShape` chaptered ADTG fixtures with sibling chapter fields and nested
grandchild/great-grandchild chapter relations.
