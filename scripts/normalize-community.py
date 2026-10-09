#!/usr/bin/env python3
"""Normalize a one-time, public provider capture into our offline observation schema.

No network calls. Inputs are saved provider responses; values without enough scope
remain observations and are not promoted into community-index snapshots.
"""
import argparse
import gzip
import hashlib
import html
import json
import re
from datetime import datetime
from pathlib import Path

CLASSES = dict(zip(
    ['검성', '수호성', '살성', '궁성', '마도성', '정령성', '치유성', '호법성', '권성'],
    ['gladiator', 'templar', 'assassin', 'ranger', 'sorcerer', 'elementalist', 'cleric', 'chanter', 'fighter']))
CLASSES.update({k.title(): k for k in list(CLASSES.values())})
CLASSES.update({'Spiritmaster': 'elementalist', 'Brawler': 'fighter', 'brawler': 'fighter'})


def text(markup):
    return html.unescape(re.sub('<[^>]+>', '', markup)).strip()


def number(value):
    value = value.replace(',', '').replace('/s', '').strip()
    multiplier = 10000 if value.endswith('만') else 1000 if value.lower().endswith('k') else 1
    return float(value.rstrip('만kK')) * multiplier


def load(path):
    raw = path.read_bytes()
    return json.loads(gzip.decompress(raw) if raw[:2] == b'\x1f\x8b' else raw)


def normalized(cls, metric, scope, statistics, **known):
    return dict(class_key=CLASSES.get(cls, cls.lower()), region=None,
                dungeon_id=None, mob_code=None, cp_min=None, cp_max=None,
                median_dps=None, samples=None, metric=metric, scope=scope,
                statistics=statistics, compatibility=['balance_period_unconfirmed',
                'methodology_not_a2m_v2']) | known


def source_record(path, url):
    return dict(url=url, sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                captured_at=int(path.stat().st_mtime * 1000))


def archive(source, website, evidence, rows, detail):
    return dict(schema='a2m-provider-observations-v1', source=dict(id=source,
                url=website, captured_at=max(e['captured_at'] for e in evidence),
                rights_confirmed=False), evidence=evidence, detail=detail, rows=rows)


def a2tools(path, url):
    markup = path.read_text()
    selected = {}
    for name, body in re.findall(r'<select name="([^"]+)"[^>]*>(.*?)</select>', markup, re.S):
        match = re.search(r'<option value="([^"]*)" selected>(.*?)</option>', body, re.S)
        if match:
            selected[name] = dict(value=match[1], label=text(match[2]))
    rows = []
    for row in re.findall(r'<tr class="clsrow"[^>]*>(.*?)</tr>', markup, re.S):
        cells = [text(c) for c in re.findall(r'<td[^>]*>(.*?)</td>', row, re.S)]
        if len(cells) != 9:
            raise ValueError('A2 Tools table changed')
        cls = cells[0].lstrip('▸').strip()
        stats = dict(median_dps=number(cells[2]), best_dps=number(cells[4]),
                     median_cp=number(cells[6]), median_gear_score=number(cells[7]),
                     samples=int(cells[8]), middle_half_display=cells[3])
        known = dict(median_dps=stats['median_dps'], samples=stats['samples'])
        if selected.get('region', {}).get('value'):
            known['region'] = selected['region']['value'].upper()
        elif 'region' in selected:
            known['region'] = 'ALL'
        for field, form in [('dungeon_id', 'tier'), ('mob_code', 'boss')]:
            value = selected.get(form, {}).get('value')
            if value and value.isdigit(): known[field] = int(value)
        cp = selected.get('cp', {}).get('value', '')
        if re.fullmatch(r'\d+-\d+', cp):
            known['cp_min'], known['cp_max'] = map(int, cp.split('-'))
        rows.append(normalized(cls, 'provider_dps', dict(filters=selected, cp_boundary_semantics='provider endpoints; inclusivity undocumented',
                               rounding='displayed k/s values; not exact parser precision'), stats, **known))
    if len(rows) < 1: raise ValueError('No A2 Tools class rows')
    return source_record(path, url), rows


def jameter(path, cls):
    markup = path.read_text()
    tables = re.findall(r'<table[^>]*>(.*?)</table>', markup, re.S)
    matches = [t for t in tables if '중앙값' in t and '전투력' in t]
    if len(matches) != 1: raise ValueError(f'JaMeter reference table changed: {cls}')
    rows = []
    for tr in re.findall(r'<tr[^>]*>(.*?)</tr>', matches[0], re.S):
        c = [text(x) for x in re.findall(r'<td[^>]*>(.*?)</td>', tr, re.S)]
        if not c: continue
        if len(c) != 5: raise ValueError('JaMeter CP columns changed')
        lo, hi = map(int, c[0].rstrip('만').split('~'))
        stats = dict(median_dps=number(c[2]), p90_dps=number(c[3]), maximum_dps=number(c[4]), samples=int(c[1]))
        row = normalized(cls, 'provider_dps', dict(content='all observed bosses',
                cp_label=c[0], cp_boundary_semantics='not specified', rounding='displayed 만 units'),
                stats, cp_min=lo*10000, cp_max=hi*10000, median_dps=stats['median_dps'], samples=stats['samples'])
        row['compatibility'] += ['boss_and_difficulty_unspecified', 'region_unspecified', 'cp_band_over_20000']
        rows.append(row)
    return source_record(path, f'https://jameter.net/ranking/{cls}'), rows


def notmeter_period_scope(view):
    """Keep provider period labels: All also denotes bounded weekly cohorts."""
    period, label = view['period'], view['periodLabel']
    if not isinstance(label, str) or not label:
        raise ValueError('Missing NotMeter period label')
    scope = dict(period=period, period_label=label)
    if label.startswith('weekly-wed05|'):
        parts = label.split('|')
        if len(parts) != 3 or period != 'All':
            raise ValueError('Invalid NotMeter weekly period')
        start, end = (datetime.fromisoformat(v) for v in parts[1:])
        if start.tzinfo is None or end.tzinfo is None or end <= start:
            raise ValueError('Invalid NotMeter weekly boundaries')
        scope.update(period_kind='weekly', period_from_ms=int(start.timestamp()*1000),
                     period_until_ms=int(end.timestamp()*1000),
                     period_boundary_semantics='provider boundaries; inclusivity unconfirmed')
    return scope


def notmeter_rows(data, npc_catalog):
    """Normalize class aggregates only; reject duplicate complete cohort identities."""
    rows = []
    cp_tiers = {t['index']:t for t in data['cpTiers']}
    dungeons = {d['key']:d for d in data['dungeons']}
    npcs = load(npc_catalog)
    npc_hash = hashlib.sha256(npc_catalog.read_bytes()).hexdigest()
    npc_ids = {}
    for key, entry in npcs.items():
        if entry.get('isBoss'):
            npc_ids.setdefault(entry['name'], []).append(int(key))
    identities = set()
    for view in data['views']:
        if view['dungeonKey'] == '__notmeter_daily_active_users__':
            continue  # Service activity, not combat statistics.
        tier = cp_tiers[view['cpTierIndex']]; dungeon = dungeons[view['dungeonKey']]
        candidate_ids = npc_ids.get(view['bossName'], [])
        for value in view['rows']:
            metric = 'provider_dps'
            known = dict(cp_min=tier.get('minCombatPower'),
                    cp_max=tier.get('maxCombatPowerExclusive', 0)-1 if tier.get('maxCombatPowerExclusive') else None,
                    median_dps=value.get('medianDps'), samples=value.get('sampleCount'))
            if len(dungeon['mapIds']) == 1 and not view['dungeonKey'].startswith('training'):
                known['dungeon_id'] = dungeon['mapIds'][0]
            if len(candidate_ids) == 1: known['mob_code'] = candidate_ids[0]
            row = normalized(value['jobName'], metric, dict(dungeon_key=view['dungeonKey'],
                    map_ids=dungeon['mapIds'], boss_index=view['bossIndex'], boss_name=view['bossName'],
                    mob_code_candidates=candidate_ids, **notmeter_period_scope(view), generated_at=view['generatedAt'],
                    provider_schema=data['schema'], provider_version=data['version'],
                    minimum_party_size=data['minimumPartySize'],
                    record_count=view['recordCount'], player_sample_count=view['playerSampleCount'],
                    region='KR/TW combined website dataset', ranking_basis=data['rankingBasis'],
                    cp_tier=tier, npc_catalog_sha256=npc_hash),
                    {k:v for k,v in value.items() if k != 'jobName'}, **known)
            row['compatibility'] += ['KR_TW_combined', 'cp_band_over_20000']
            if known.get('mob_code') is None: row['compatibility'].append('boss_id_ambiguous_or_aggregate')
            if view['dungeonKey'].startswith('training'): row['compatibility'].append('training')
            identity_scope = {k: v for k, v in row['scope'].items()
                              if k not in ('record_count', 'player_sample_count')}
            identity = json.dumps([row['class_key'], identity_scope], sort_keys=True)
            if identity in identities:
                raise ValueError('Duplicate NotMeter class/cohort identity')
            identities.add(identity)
            rows.append(row)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input-dir', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--npc-catalog', type=Path, required=True)
    args = parser.parse_args()
    src, out = args.input_dir, args.output_dir
    out.mkdir(parents=True, exist_ok=True)
    datasets = []
    evidence, rows = [], []
    for name, url in [('a2tools-stats.html', 'https://a2tools.app/stats'),
            ('a2tools-vakron-eu.html', 'https://a2tools.app/stats?region=eu&dungeon=600070&tier=600072&boss=2300812&period=bp_f086f9e7f115&cp=71000-76000')]:
        ev, part = a2tools(src/name, url); evidence.append(ev); rows += part
    datasets.append(archive('a2tools', 'https://a2tools.app/stats', evidence, rows,
            'Klassenmediane, ausgewählte Filter erhalten; Global-Launch-Zeitraum ohne belegte Zeitgrenzen.'))

    path = src/'aiondps-leaderboard.data'; data = load(path); rows = []
    scope = dict(boss_slug=data['boss']['slug'], provider_boss_id=data['boss']['id'],
                 provider_instance_id=data['boss']['instanceId'], mode=data['selectedMode'],
                 game=data['boss']['game'], server_selection=data['selectedServerId'])
    for key, value in data['stats'].items():
        if value is not None:
            rows.append(normalized('all', 'provider_'+key, scope, {key:value}))
    datasets.append(archive('aiondps', 'https://aiondps.com/bosses/vakron',
            [source_record(path, 'https://aiondps.com/api/bosses/vakron/leaderboard?game=aion2')], rows,
            'Vakron-Ranglistenaggregate; Anbieter-IDs sind keine Spiel-IDs. Drei Runs, kein Klassenmedian.'))

    path = src/'abysslogs-stats.data'; data = load(path); rows = []
    for cell in data['cells']:
        row = normalized(cell['classHint'], 'provider_dps',
                dict(build=data['build'], period=data['period'], fight_key=data['fightKey'],
                region_request='eu', data_region='build-wide response; region not returned',
                bracket_ceil_raw=cell['bracketCeil'], built_at=cell['builtAt']),
                {k:v for k,v in cell.items() if k not in ['classHint','bracketFloor','bracketCeil','builtAt']},
                mob_code=data['fightKey'], median_dps=cell['median'], samples=cell['samples'],
                cp_min=cell['bracketFloor'] or None, cp_max=cell['bracketCeil'] or None)
        row['compatibility'] += ['region_scope_unconfirmed', 'all_time_mixes_balance_periods', 'dungeon_scope_unconfirmed', 'cp_band_over_20000']
        rows.append(row)
    datasets.append(archive('abysslogs', 'https://abysslogs.com/statistics',
            [source_record(path, 'https://api.abysslogs.com/v1/statistics?region=eu&period=all&fight=2300812')], rows,
            'Vakron-Klassenmediane mit Anbieter-KP-Bändern; Antwort nennt globalen Build, keine bestätigte EU-Kohorte.'))

    # Only published aggregates. Never keep classRankings/player/profile sections.
    path = src/'notmeter-cache.data'; data = load(path)
    rows = notmeter_rows(data, args.npc_catalog)
    datasets.append(archive('notmeter', 'https://notmeter.com',
            [source_record(path, 'https://notmeter.com/g/578d3695ce598fe2/data/notmeter-ranking.json.gz')], rows,
            'Klassenaggregate mit unveränderten Zeitraum-Labels und getrennten Wochenkohorten.'))

    evidence, rows = [], []
    for cls in ['gladiator','templar','assassin','ranger','sorcerer','elementalist','cleric','chanter','brawler']:
        ev, part = jameter(src/f'jameter-{cls}.data', cls); evidence.append(ev); rows += part
    datasets.append(archive('jameter', 'https://jameter.net/ranking', evidence, rows,
            'Öffentliche KP-Referenztabellen aller neun Klassen; 10만 entspricht 100000, keine Skalierung auf Global-KP.'))

    path = src/'questlog-observed.json'; data = load(path)
    rows = [normalized(v['class_key'], 'boss_dps_index', dict(region='EU', content='all boss kills',
            unit='index; 100 = per-boss baseline', data_updated='displayed 36m ago',
            ranked_characters=35566, boss_kills=230701),
            dict(boss_dps_index=v['boss_dps_index']), region='EU') for v in data['rows']]
    for row in rows: row['compatibility'] += ['index_not_absolute_dps', 'boss_and_difficulty_unspecified']
    datasets.append(archive('questlog', 'https://questlog.gg/aion-2/en/armory/meta?region=eu',
            [source_record(path, 'https://questlog.gg/aion-2/en/armory/meta?region=eu')], rows,
            'Alle acht Klassen der europäischen Tier-List; normalisierter Boss-DPS-Index, kein absoluter DPS-Median.'))
    for dataset in datasets:
        dest = out / (dataset['source']['id']+'.json')
        dest.write_text(json.dumps(dataset, ensure_ascii=False, sort_keys=True, separators=(',', ':'))+'\n')
        print(dataset['source']['id'], len(dataset['rows']), dest.stat().st_size)


if __name__ == '__main__': main()
