const assert = require('node:assert/strict'), fs = require('node:fs'), vm = require('node:vm'), path = require('node:path');
const source = fs.readFileSync(path.join(__dirname, '../web/enhancements.js'), 'utf8');
const context = vm.createContext({num: String, dur: String, esc: String, window: {addEventListener() {}}});
vm.runInContext(source.slice(source.indexOf('function damageSamples('), source.indexOf('function hitTimeline(')), context);
vm.runInContext(fs.readFileSync(path.join(__dirname, '../web/run-analysis.js'), 'utf8'), context);
const run = expression => vm.runInContext(expression, context), plain = value => JSON.parse(JSON.stringify(value));
const skill = (damage, hits, crit_rate) => ({code: 11, name: 'Same name', damage, hits, crit_rate, back_rate: null, min: 100, max: 1200});
const player = (actor_id, name, server_id, damage, skills) => ({actor_id, name, server_id, job: 'cleric', is_self: name === 'Me', damage, skills});
context.r = {fights: [
  {id: 'b', started_at: 2000, duration_ms: 1000, players: [player(99, 'Me', 1, 2000, [skill(2000, 8, 25)]), player(2, 'Same', 2, 1000, [skill(1000, 4, 100)])], analytics: {points: [{ms: 500, damage: {99: 1000, 2: 500}}, {ms: 1000, damage: {99: 2000, 2: 1000}}]}},
  {id: 'a', started_at: 1000, duration_ms: 1000, players: [player(1, 'Me', 1, 1000, [skill(900, 2, 50)]), player(2, 'Same', 1, 500, [skill(500, 1, 0)])], analytics: {points: [{ms: 500, damage: {1: 400, 2: 200}}, {ms: 1000, damage: {1: 1000, 2: 500}}]}},
  {id: 'c', started_at: 3000, duration_ms: 1000, players: [player(5, 'Other', 1, 3000, [])], analytics: {points: [{ms: 500, damage: {5: 1500}}, {ms: 1000, damage: {5: 3000}}]}},
  {id: 'd', started_at: 4000, duration_ms: 0, players: [player(55, 'Me', 1, 1000)], analytics: null},
  {id: 'training', started_at: 5000, duration_ms: 50000, is_train: 1, players: [player(1, 'Me', 1, 99999999, [skill(99999999, 1, 100)])]}
]};
const before = JSON.stringify(context.r);
run('model = runCombatModel(r); own = model.players.find(p => p.name === "Me"); data = runDamageSamples(model, own.key); selection = runSelection(model, own.key)');
assert.equal(run('model.duration'), 4000);
assert.equal(run('model.windows.length'), 4);
assert.equal(run('own.damage'), 4000);
assert.equal(run('own.dps'), 1000);
assert.equal(run('model.players.filter(p=>p.name === "Same").length'), 2, 'servers stay separate despite reused actor IDs');
assert.equal(run('selection.skills[0].damage'), 2900);
assert.equal(run('selection.skills[0].dps'), 725);
assert.equal(run('selection.skills[0].crit_rate'), 30, 'hit-weighted rate, not mean of fight percentages');
assert.equal(run('selection.skills[0].back_rate'), null);
assert.equal(run('selection.skill_fights'), 2);
assert.equal(run('selection.player_fights'), 3);
assert.equal(run('runSelection(model).damage'), 8500);
assert.equal(run('data.missing'), 1);
assert.equal(run('data.samples.find(p=>p.fight_id === "b").dps'), 2000, 'smoothing restarts at fight boundaries');
assert.equal(run('data.samples.find(p=>p.fight_id === "b").total'), 2000);
assert.deepEqual(plain(run('data.samples.filter(p=>p.fight_id === "c").map(p=>p.dps)')), [0, 0], 'absent participant contributes zero over shared windows');
const markup = run('curveMarkup(data.samples,[{name:"Run",color:"red",value:p=>p.dps}],{end:model.duration})');
assert.equal((markup.match(/<path d="([^"]+)"/)[1].match(/M/g) || []).length, 3, 'no line connects fights');
assert.equal(JSON.stringify(context.r), before, 'measurements are immutable');
assert.equal(run('runSkillRows([{code:11,damage:1,hits:1},{code:12,damage:2,hits:1},{code:11,is_dot:true,damage:3,hits:1}],1000).length'), 3, 'IDs and DoT remain distinct');
assert.equal(run('runSkillRows([{name:"unknown",damage:1},{name:"unknown",damage:2}],1000).length'), 2, 'unknown skill IDs are not inferred from names');
context.gap = {fights: [{id: 'gap', duration_ms: 3000, players: [player(1, 'Me', 1, 5000, [])], analytics: {resolution_ms: 500, points: [{ms: 500, damage: {1: 1000}}, {ms: 2500, damage: {1: 4000}}, {ms: 3000, damage: {1: 5000}}]}}]};
run('gapped = runDamageSamples(runCombatModel(gap))');
assert.equal(run('gapped.partial'), 1);
assert.deepEqual(plain(run('gapped.samples.map(p=>p.dps)')), [2000, 2000], 'missing intervals do not become fabricated spikes');
assert.notEqual(run('gapped.samples[0].segment'), run('gapped.samples[1].segment'));
context.gap.fights[0].analytics.partial = true;
assert.equal(run('runDamageSamples(runCombatModel(gap)).samples.length'), 1, 'partial first observation is only a baseline');
assert.equal(run('runDamageSamples(runCombatModel(gap),"group",5000,"total").samples.length'), 3, 'known cumulative snapshots remain selectable, including partial baselines');
context.gap.fights[0].analytics.points[1].damage[1] = -1;
assert.equal(run('runDamageSamples(runCombatModel(gap)).missing'), 1, 'invalid counters do not become measured zero');
assert.equal(run('runDamageSamples(runCombatModel({})).samples.length'), 0);
assert.equal(run('runSelection(runCombatModel({})).damage'), 0);
assert.equal(run('runSkillRows([{code:1,damage:2,hits:1,crit_rate:50},{code:1,damage:3,hits:1,crit_rate:null}],1000)[0].crit_rate'), null);
context.unknown = {fights:[{id:'x',players:[{actor_id:1,name:'#1',damage:10}]},{id:'y',players:[{actor_id:1,name:'#1',damage:20}]}]};
assert.equal(run('runCombatModel(unknown).players.length'),2,'generated actor aliases do not imply a stable player across fights');
context.partial = {fights: [
  {id: 'early', started_at: 1, duration_ms: 1000, players: [{actor_id: 7, name: 'Late', server_id: 0, job: '', damage: 100}, {actor_id: 8, name: 'Twin', server_id: 0, job: '', damage: 10}],
    analytics: {points: [{ms: 1000, damage: {7: 100, 8: 10}}]}},
  {id: 'later', started_at: 2, duration_ms: 1000, players: [{actor_id: 9, name: 'Late', server_id: 3, job: 'cleric', class_name: 'Kleriker', damage: 200},
    {actor_id: 10, name: 'Twin', server_id: 1, job: 'cleric', damage: 20}, {actor_id: 11, name: 'Twin', server_id: 2, job: 'cleric', damage: 30}],
    analytics: {points: [{ms: 1000, damage: {9: 200, 10: 20, 11: 30}}]}}]};
run('pm = runCombatModel(partial); late = pm.players.filter(p => p.name === "Late")');
assert.equal(run('late.length'), 1, 'a fight before class/server were decoded joins the only known identity');
assert.equal(run('late[0].damage'), 300);
assert.equal(run('late[0].class_name'), 'Kleriker');
assert.equal(run('late[0].server_id'), 3);
assert.deepEqual(plain(run('runDamageSamples(pm, late[0].key, 0, "total").samples.map(p => p.total)')), [100, 300]);
assert.equal(run('pm.players.filter(p => p.name === "Twin").length'), 3, 'an ambiguous partial identity is not guessed');
console.log('PASS run analysis: shared windows, identities, skill sums/quality, missing participation, gaps, boundaries, legacy data and immutable measurements');
