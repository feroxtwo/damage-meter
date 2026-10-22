# A2Tools parser core

Source: https://github.com/taengu/A2Tools-DPS-Meter at 82e53c1008ac4c2974446cc703703f473bbbed81 (2.0.54).
Copyright: upstream authors. GPL-3.0, retained in LICENSE.
This source subset retains the parser, aggregation, evidence and replay core.
Desktop-only modules are excluded by the existing feature gates.
Local modifications: 64-bit aggregate damage/heal amounts and detail records,
combat observation callback for bounded local telemetry. See git diff/history.
Resource metadata retains its upstream provenance and is not relicensed by this notice.

Language regression fixtures use the two application tables (de/en) instead of absent upstream desktop languages. Other parser-core tests are retained.

Rust 1.88 Clippy hygiene: Default wrappers, harmless casts/formatting, iterator expressions and named tuple aliases. These changes do not alter protocol interpretation.
