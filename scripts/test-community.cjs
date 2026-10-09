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
assert.equal(run('communityComparisonMarkup(response).includes("20 unabhängige Spieler laut Quelle")'), true);
context.response.comparisons[0].balance_id = '<script>alert(1)</script>';
assert.equal(run('communityComparisonMarkup(response).includes("<script>")'), false, 'snapshot metadata must be escaped');
assert.equal(run('communityTemplate().source.rights_confirmed'), false, 'a generated template must not preauthorize anything');
assert.equal(run('communityTemplate().schema'),'a2m-community-v2','new imports declare metric definition');
assert.equal(run('communityTemplate().methodology.aggregation'),'median_unique_players');
// Data that exists but is not comparable must say so, with reasons in words.
context.response = {status:'insufficient', reason:'comparison_withheld', region:'EU', comparisons:[],
  withheld:[{reason:'methodology_unconfirmed'}],
  offline_observations:{rows:25, same_class:3, sources:['abysslogs'], blockers:['rights_unconfirmed','cp_band_over_20000','totally_new_flag']}};
const withheld = run('communityComparisonMarkup(response)');
assert.ok(withheld.includes('Daten vorhanden, Vergleich nicht freigegeben'));
assert.ok(withheld.includes('25 Anbieter-Datengruppen') && withheld.includes('Nutzungsrechte der Quelle nicht bestätigt'));
assert.ok(!withheld.includes('cp_band_over_20000') && !withheld.includes('totally_new_flag'), 'no raw flags for users');
assert.ok(!/\d+,\d<\/strong>|Perzentil: |Top \d/.test(withheld), 'no score when withheld');
context.response = {status:'indicative', region:'EU', comparisons:[{source_id:'x', score:150, reference_dps:1}]};
assert.ok(!run('communityComparisonMarkup(response)').includes('150'), 'an unknown status never renders a score');

assert.equal(run('communityTemplate().rows.length'), 0, 'never ship fake online measurements');
console.log('Community reference UI: 12 assertions passed');

(async () => {
  const root = {innerHTML: '', querySelectorAll: () => []};
  context.$ = () => root;
  context.api = async () => ({offline_data: [{source_id:'a2tools', row_count:16, rights_confirmed:0,
    captured_at:1790000000000, detail:'Median <script>alert(1)</script>'}], providers:[], imports:[]});
  await run('loadCommunitySources()');
  assert.ok(root.innerHTML.includes('16 Datengruppen vorhanden'));
  assert.ok(root.innerHTML.includes('Vergleich nicht freigegeben') && root.innerHTML.includes('Nutzungsrechte nicht bestätigt'));
  assert.ok(root.innerHTML.includes('/api/references/archive/a2tools'));
  assert.ok(root.innerHTML.includes('&lt;script&gt;'));
  assert.ok(!root.innerHTML.includes('<script>'));
  console.log('Offline provider catalog: 5 assertions passed');
})().catch(error => {console.error(error); process.exitCode = 1;});
