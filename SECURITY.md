# Security Policy

## Supported Versions

Security fixes are applied to the latest `0.1.x` release.

## Reporting a Vulnerability

Report parser crashes, hangs, memory blowups, or data corruption issues through
the project issue tracker. Include:

- the smallest input that reproduces the issue,
- the API or CLI command used,
- expected versus actual behavior,
- platform and Rust version.

Do not publish exploit inputs publicly until a fix is available.

## Parser Threat Model

ADO XML and ADTG inputs are treated as untrusted data. The library should return
`Result::Err` for malformed input, not panic, loop forever, exhaust memory, or
overflow arithmetic.

Implemented limits and guards:

- Maximum nested chapter depth is `MAX_RECORDSET_DEPTH` (`64`) across parsers,
  validators, writers, materializers, and native comparison helpers.
- Callers can apply `ResourceLimits` for input bytes, visible fields per
  recordset, rows per recordset, single value payload bytes, aggregate value
  payload bytes, and XML decimal exponent expansion length.
- Chaptered ADTG row parsing is deterministic and does not enumerate cartesian
  products of possible child row partitions.
- Binary readers and descriptor parsers use bounds-checked ranges and checked
  offset arithmetic.
- XML decimal exponent parsing computes the expanded length before allocation
  and enforces caller-supplied decimal expansion limits.
- XML parsing uses `roxmltree` defaults with DTD/entity expansion disabled and
  also preserves raw ADO-visible attributes through bounded side scans.
- Fuzz seeds include flat, shaped/chaptered, variant, generated, and corrupted
  ADTG/XML inputs.

Known non-goals:

- The crate does not execute provider commands or load COM components in parser
  code.
- Development oracle scripts may invoke Windows ADO/MDAC, but they are not part
  of the native parser threat boundary.
