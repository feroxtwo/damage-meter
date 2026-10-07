#!/usr/bin/env python3
"""Compare one replay target/actor with exact in-game skill totals, never screenshots/OCR."""
import argparse
import csv
import json
import math
from pathlib import Path


def compare(report, expected, actor, target, tolerance):
    matches = [t for t in report.get('targets', []) if t.get('details', {}).get('targetId') == target]
    if len(matches) != 1:
        raise ValueError('Genau ein Replay-Ziel mit dieser ID erforderlich')
    details = matches[0]['details']
    actual = {}
    for kind, field in [('damage', 'skills'), ('heal', 'healSkills')]:
        for s in details.get(field, []):
            if s.get('actorId') == actor:
                key = (kind, int(s['code']), bool(s.get('isDot', False)))
                actual[key] = actual.get(key, 0) + int(s['dmg'])
    reference = {}
    for row in expected:
        kind = row['kind']
        if kind not in ('damage', 'heal') or row['is_dot'] not in ('0', '1'):
            raise ValueError('kind muss damage/heal und is_dot 0/1 sein')
        key = (kind, int(row['code']), row['is_dot'] == '1')
        if key in reference:
            raise ValueError('Doppelte Referenzzeile')
        reference[key] = int(row['total'])
        if reference[key] < 0:
            raise ValueError('Negative Referenzsumme')
    if not reference or not actual:
        raise ValueError('Leere Referenz oder keine Skills des gewählten Spielers')
    rows = []
    for key in sorted(reference.keys() | actual.keys()):
        observed, expected_total = actual.get(key, 0), reference.get(key)
        delta = observed - expected_total if expected_total is not None else None
        percent = delta * 100 / expected_total if expected_total else (0.0 if delta == 0 else None)
        passed = expected_total is not None and (delta == 0 or percent is not None and abs(percent) <= tolerance)
        rows.append(dict(kind=key[0], code=key[1], is_dot=key[2], expected=expected_total,
                         observed=observed, delta=delta, delta_percent=percent, passed=passed))
    totals = {kind: dict(expected=sum(v for k, v in reference.items() if k[0] == kind),
                        observed=sum(v for k, v in actual.items() if k[0] == kind)) for kind in ('damage', 'heal')}
    return dict(parser_rev=report.get('parser_rev'), actor_id=actor, target_id=target,
                tolerance_percent=tolerance, passed=all(r['passed'] for r in rows), totals=totals, skills=rows,
                limitations=['Exakte Ingame-Summen derselben Zeitspanne erforderlich. Kein DPS-Zeitmodellvergleich.',
                              'Referenz muss sämtliche angezeigten Schadens-/Heilungsskills enthalten.',
                              'Erfasstes Netzwerkprotokoll und Ziel-/Spielerzuordnung separat prüfen.'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('replay', type=Path)
    parser.add_argument('reference', type=Path)
    parser.add_argument('--actor', required=True, type=int)
    parser.add_argument('--target', required=True, type=int)
    parser.add_argument('--tolerance-percent', type=float, default=0.5)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if not math.isfinite(args.tolerance_percent) or args.tolerance_percent < 0:
        parser.error('Toleranz muss endlich und nicht negativ sein')
    try:
        with args.reference.open(newline='', encoding='utf-8-sig') as f:
            result = compare(json.loads(args.replay.read_text()), list(csv.DictReader(f)), args.actor,
                             args.target, args.tolerance_percent)
    except (ValueError, KeyError, OSError) as exc:
        parser.error(str(exc))
    body = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False) + '\n'
    if args.output:
        args.output.write_text(body)
    print(body)
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
