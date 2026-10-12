#!/usr/bin/env python3
"""Prepare the reference observations that are embedded into the meter.

Input: normalized provider archives (schema a2m-provider-observations-v1, as
written by normalize-community.py). Output: one gzip file per archive under
data/community/, read by src/reference_bundle.rs at compile time.

The archives keep their source, evidence and capture time, so the embedded
data stays reproducible. Deterministic: same input, byte-identical output.

Captures made before the weekly cohort label existed carry period "All" for
both the all-time view and weekly cohorts. A weekly cohort is recognised by its
previous-week comparison. Rows that still share one identity afterwards
cannot be told apart and are dropped, never guessed.
"""
import argparse
import collections
import gzip
import io
import json
from pathlib import Path


def identity(row):
    return (row['class_key'], row.get('region'), row.get('dungeon_id'), row.get('mob_code'),
            row.get('cp_min'), row.get('cp_max'), row['metric'],
            json.dumps(row['scope'], sort_keys=True, ensure_ascii=False))


def prepare(archive):
    rows = archive['rows']
    for row in rows:
        scope = row['scope']
        if (scope.get('period') == 'All' and 'period_label' not in scope
                and 'previousWeek' in row.get('statistics', {})):
            scope['period_kind'] = 'weekly_inferred'
    groups = collections.Counter(identity(r) for r in rows)
    kept = [r for r in rows if groups[identity(r)] == 1]
    dropped = len(rows) - len(kept)
    if dropped:
        archive['detail'] += f' {dropped} nicht unterscheidbare Zeilen verworfen.'
    archive['rows'] = kept
    return archive, dropped


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--input-dir', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, default=Path('data/community'))
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for path in sorted(args.input_dir.glob('*.json')):
        archive = json.loads(path.read_text())
        if archive.get('schema') != 'a2m-provider-observations-v1':
            continue
        archive, dropped = prepare(archive)
        payload = json.dumps(archive, sort_keys=True, ensure_ascii=False,
                             separators=(',', ':')).encode()
        buffer = io.BytesIO()
        # mtime=0 and no file name keep the output byte-identical.
        with gzip.GzipFile(fileobj=buffer, mode='wb', compresslevel=9, mtime=0, filename='') as out:
            out.write(payload)
        target = args.output_dir / (archive['source']['id'] + '.json.gz')
        target.write_bytes(buffer.getvalue())
        print(f"{target}: {len(archive['rows'])} Zeilen, {dropped} verworfen")


if __name__ == '__main__':
    main()
