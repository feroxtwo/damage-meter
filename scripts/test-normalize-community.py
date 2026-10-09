#!/usr/bin/env python3
"""Synthetic regression cases; no provider observations are committed."""
import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'normalize_community', Path(__file__).with_name('normalize-community.py'))
normalizer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(normalizer)

WEEK = 'weekly-wed05|2026-01-07T05:00:00.0000000+09:00|2026-01-14T05:00:00.0000000+09:00'


def fixture():
    view = dict(dungeonKey='synthetic', bossIndex=0, bossName='Synthetic boss',
                cpTierIndex=1, period='All', periodLabel='전체 기간',
                generatedAt='2026-01-15T00:00:00+00:00', recordCount=20,
                playerSampleCount=10,
                rows=[dict(jobName='검성', medianDps=100, sampleCount=10)])
    weekly = copy.deepcopy(view)
    weekly['periodLabel'] = WEEK
    weekly['rows'][0]['medianDps'] = 200
    return dict(schema='notmeter-web-ranking-v1', version=1,
                minimumPartySize=5, rankingBasis='synthetic',
                cpTiers=[dict(index=1, minCombatPower=100, maxCombatPowerExclusive=200)],
                dungeons=[dict(key='synthetic', mapIds=[123])],
                views=[view, weekly], classRankings=[{'private': 'never copy'}])


class NotMeterNormalization(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.npcs = Path(self.temp.name) / 'npcs.json'
        self.npcs.write_text(json.dumps({'456': dict(name='Synthetic boss', isBoss=True)}))

    def normalize(self, data):
        return normalizer.notmeter_rows(data, self.npcs)

    def test_all_time_and_weekly_are_separate_without_changing_measurements(self):
        data = fixture()
        original = copy.deepcopy(data)
        rows = self.normalize(data)
        self.assertEqual(data, original)
        self.assertEqual([r['median_dps'] for r in rows], [100, 200])
        self.assertEqual([r['scope']['period'] for r in rows], ['All', 'All'])
        self.assertEqual([r['scope']['period_label'] for r in rows], ['전체 기간', WEEK])
        weekly = rows[1]['scope']
        self.assertEqual(weekly['period_from_ms'], 1767729600000)
        self.assertEqual(weekly['period_until_ms'] - weekly['period_from_ms'], 7*86400000)
        self.assertIn('unconfirmed', weekly['period_boundary_semantics'])
        self.assertNotIn('period_from_ms', rows[0]['scope'])
        self.assertEqual(rows[0]['scope']['minimum_party_size'], 5)
        self.assertEqual(rows[0]['cp_max'], 199)
        self.assertEqual(rows[0]['mob_code'], 456)
        self.assertNotIn('classRankings', json.dumps(rows))

    def test_duplicate_cohort_is_rejected_even_when_counts_and_values_differ(self):
        data = fixture()
        duplicate = copy.deepcopy(data['views'][0])
        duplicate['recordCount'] = 99
        duplicate['playerSampleCount'] = 88
        duplicate['rows'][0]['medianDps'] = 999
        data['views'].append(duplicate)
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            self.normalize(data)

    def test_invalid_weekly_ranges_fail_instead_of_merging_into_all_time(self):
        for label in ['weekly-wed05|bad|date',
                      'weekly-wed05|2026-01-14T05:00:00+09:00|2026-01-07T05:00:00+09:00',
                      'weekly-wed05|2026-01-07T05:00:00|2026-01-14T05:00:00']:
            with self.subTest(label=label), self.assertRaises(ValueError):
                normalizer.notmeter_period_scope(dict(period='All', periodLabel=label))

    def test_missing_label_fails_closed(self):
        data = fixture()
        del data['views'][0]['periodLabel']
        with self.assertRaises(KeyError):
            self.normalize(data)

    def test_relative_period_labels_remain_raw_without_invented_boundaries(self):
        scope = normalizer.notmeter_period_scope(dict(period='Today', periodLabel='오늘'))
        self.assertEqual(scope, dict(period='Today', period_label='오늘'))


if __name__ == '__main__':
    unittest.main()
