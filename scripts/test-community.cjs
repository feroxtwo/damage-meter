const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const source = fs.readFileSync(path.join(__dirname, '../web/community.js'), 'utf8');
const context = vm.createContext({
  window: {},
  localStorage: { getItem: () => 'EU' },
  esc: value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;'),
  num: value => String(value),
  date: value => String(value),
});
vm.runInContext(source.slice(0, source.indexOf('const regionSelect')), context);
const run = expression => vm.runInContext(expression, context);
assert.equal(run('selectedCommunityRegion()'), 'EU');
assert.equal(run('communityRatio(142.32)'), '142,3');
assert.equal(run("communityComparisonMarkup({status:'insufficient',reason:'no_matching_reference',region:'EU'}).includes('kein Online-Score')"), true);
context.response = {
  status: 'ready', region: 'EU', comparisons: [{
    source_id: 'community', scope: 'same_class', score: 140,
    reference_dps: 10_000, samples: 20, cp_min: 60_000, cp_max: 80_000,
    region: 'EU', balance_id: '2026.10', source_url: 'https://example.org/data'
  }]
};
assert.equal(run('communityComparisonMarkup(response).includes("140")'), true);
assert.equal(run('communityComparisonMarkup(response).includes("20 Vergleichsspieler")'), true);
context.response.comparisons[0].balance_id = '<script>alert(1)</script>';
assert.equal(run('communityComparisonMarkup(response).includes("<script>")'), false, 'snapshot metadata must be escaped');
assert.equal(run('communityTemplate().source.rights_confirmed'), false, 'a generated template must not preauthorize anything');
assert.equal(run('communityTemplate().schema'),'a2m-community-v2','new imports declare metric definition');
assert.equal(run('communityTemplate().methodology.aggregation'),'median_unique_players');
context.response.status='indicative';
context.response.comparisons[0].comparison_quality='legacy_unspecified';
assert.equal(run('communityComparisonMarkup(response).includes("Nur Richtwerte")'),true);

assert.equal(run('communityTemplate().rows.length'), 0, 'never ship fake online measurements');
console.log('Community reference UI: 7 assertions passed');
