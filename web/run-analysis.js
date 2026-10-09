// Run reports combine saved observations, never infer unrecorded fights.
const runNumber = value => Number.isFinite(Number(value)) ? Math.max(0, Number(value)) : 0;
const runNamedPlayer = p => !!p.name && p.name !== '#' + p.actor_id;
// Server 0 and an empty class are "not decoded yet", not a different player.
const runKnownField = value => value != null && value !== '' && Number(value) !== 0 ? value : null;
function runMemberKey(player, fightId, identities) {
  if (!runNamedPlayer(player)) return JSON.stringify(['unknown', fightId, player.actor_id]);
  let server = runKnownField(player.server_id), job = runKnownField(player.job);
  if (identities && (server == null || job == null)) {
    // Only a single complete identity with this name may fill the gap.
    const matches = (identities.get(player.name) || []).filter(([s, j]) => (server == null || s === server) && (job == null || j === job));
    if (matches.length === 1) [server, job] = matches[0];
  }
  return JSON.stringify([player.name, server == null ? null : Number(server), job]);
}
function runIdentities(fights) {
  const identities = new Map();
  for (const f of fights) for (const p of f.players || []) {
    const server = runKnownField(p.server_id), job = runKnownField(p.job);
    if (!runNamedPlayer(p) || server == null || job == null) continue;
    const list = identities.get(p.name) || [];
    if (!list.some(([s, j]) => s === server && j === job)) list.push([server, job]);
    identities.set(p.name, list);
  }
  return identities;
}
function runSkillRows(rows, duration) {
  const groups = new Map();
  rows.forEach((s, i) => {
    const key = Number(s.code) > 0 ? JSON.stringify([Number(s.code), !!s.is_dot]) : 'unknown:' + i;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(s);
  });
  return [...groups.values()].map(list => {
    const sum = key => list.reduce((n, s) => n + runNumber(s[key]), 0);
    const damage = sum('damage'), hits = sum('hits');
    const row = {...list[0], damage, hits, dps: damage * 1000 / Math.max(1000, duration),
      average: hits > 0 ? damage / hits : null, share: null, hit_timestamps: []};
    for (const key of ['crit_rate', 'back_rate', 'frontal_rate', 'perfect_rate', 'double_rate',
      'parry_rate', 'block_rate', 'perfect_block_rate', 'endurance_rate', 'regeneration_rate']) {
      const measured = list.filter(s => runNumber(s.hits) > 0);
      row[key] = measured.length && measured.every(s => s[key] != null && Number.isFinite(Number(s[key])))
        ? measured.reduce((n, s) => n + runNumber(s.hits) * Number(s[key]), 0) / hits : null;
    }
    for (const key of ['multi_hit_count', 'multi_hit_damage', 'miss_count', 'resist_count'])
      row[key] = list.every(s => s[key] != null) ? sum(key) : null;
    row.min = list.every(s => runNumber(s.min) > 0) ? list.reduce((n, s) => Math.min(n, Number(s.min)), Infinity) : null;
    row.max = list.every(s => runNumber(s.max) > 0) ? list.reduce((n, s) => Math.max(n, Number(s.max)), 0) : null;
    return row;
  }).sort((a, b) => b.damage - a.damage);
}
function runCombatModel(run) {
  const fights = (run.fights || []).filter(f => !f.is_train).slice()
    .sort((a, b) => runNumber(a.started_at) - runNumber(b.started_at) || String(a.id).localeCompare(String(b.id)));
  const players = new Map(), identities = runIdentities(fights), keys = new Map();
  let duration = 0;
  const windows = fights.map(f => {
    const start = duration, length = Math.max(1000, runNumber(f.duration_ms));
    duration += length;
    for (const p of f.players || []) {
      const key = runMemberKey(p, f.id, identities);
      keys.set(JSON.stringify([f.id, p.actor_id]), key);
      if (!players.has(key)) players.set(key, {...p, key, identity_known: runNamedPlayer(p), source_fight_id: f.id, damage: 0, skills: [], skill_fights: 0, fights: 0});
      const total = players.get(key);
      // Class and server shown for the merged player come from a fight that decoded them.
      if (runKnownField(total.job) == null && runKnownField(p.job) != null) Object.assign(total, {job: p.job, class_key: p.class_key, class_name: p.class_name});
      if (runKnownField(total.server_id) == null && runKnownField(p.server_id) != null) total.server_id = p.server_id;
      total.damage += runNumber(p.damage);
      total.is_self = !!(total.is_self || p.is_self); total.fights++;
      if (Array.isArray(p.skills)) { total.skills.push(...p.skills); total.skill_fights++; }
    }
    return {fight: f, start, length};
  });
  const members = [...players.values()].map(p => ({...p, dps: p.damage * 1000 / Math.max(1000, duration),
    skills: runSkillRows(p.skills, duration)}))
    .sort((a, b) => b.damage - a.damage || a.key.localeCompare(b.key));
  return {windows, duration, players: members, keyOf: (p, fightId) => keys.get(JSON.stringify([fightId, p.actor_id])) ?? runMemberKey(p, fightId, identities)};
}
function runSelection(model, key = 'group') {
  const players = key === 'group' ? model.players : model.players.filter(p => p.key === key);
  return {players, damage: players.reduce((n, p) => n + p.damage, 0),
    skills: runSkillRows(players.flatMap(p => p.skills), model.duration),
    skill_fights: players.reduce((n, p) => n + p.skill_fights, 0),
    player_fights: players.reduce((n, p) => n + p.fights, 0)};
}
function runDamageSamples(model, key = 'group', windowMs = 5000, metric = 'dps') {
  const samples = [];
  let carry = 0, missing = 0, partial = 0;
  for (const {fight: f, start, length} of model.windows) {
    const players = (f.players || []).filter(p => key === 'group' || model.keyOf(p, f.id) === key);
    const actors = new Set(players.map(p => String(p.actor_id)));
    const source = Array.isArray(f.analytics?.points) ? f.analytics.points : [];
    const countersValid = source.every(p => p && p.damage && typeof p.damage === 'object' && !Array.isArray(p.damage) && Object.values(p.damage).every(v => typeof v === 'number' && Number.isFinite(v) && v >= 0));
    const raw = countersValid ? source.map(p => ({ms: Number(p.ms), damage: {1:
      Object.entries(p.damage || {}).filter(([actor]) => key === 'group' || actors.has(actor))
        .reduce((n, [, value]) => n + runNumber(value), 0)}})) : [];
    // Invalid clocks/counters cannot support a trustworthy curve.
    const valid = countersValid && raw.length && raw.every((p, i) => Number.isFinite(p.ms) && p.ms > 0 && p.ms <= length &&
      (!i || (p.ms > raw[i - 1].ms && p.damage[1] >= raw[i - 1].damage[1])));
    if (!valid) missing++;
    else {
      const chunks = [[]], resolution = Math.max(1, runNumber(f.analytics.resolution_ms) || 500);
      raw.forEach((p, i) => {
        if (i && p.ms - raw[i - 1].ms > resolution * 3) chunks.push([]);
        chunks.at(-1).push(p);
      });
      if (f.analytics.partial || chunks.length > 1) partial++;
      chunks.forEach((points, index) => {
        const segment = f.id + ':' + index;
        const local = metric === 'total' ? points.map(p => ({ms: p.ms, total: p.damage[1], dps: 0}))
          : damageSamples({analytics: {points, partial: !!f.analytics.partial || index > 0}}, 1, windowMs);
        for (const p of local) samples.push({...p, ms: start + p.ms, total: carry + p.total, segment,
          fight_id: f.id, boss: f.boss_name || 'Unbekannter Gegner', fight_ms: p.ms});
      });
    }
    carry += players.reduce((n, p) => n + runNumber(p.damage), 0);
  }
  return {samples, missing, partial};
}
async function loadRunAnalysis(id, request) {
  const root = document.querySelector('#runAnalysis'); if (!root) return;
  root.dataset.runId = id;
  const current = () => root.isConnected && request === runsRequest && tab === 'runs';
  try {
    const run = await api('/api/runs/' + encodeURIComponent(id) + '/analysis');
    if (!current()) return;
    renderRunAnalysis(root, run);
  } catch {
    if (!current()) return;
    root.innerHTML = '<h2>Dungeon-Gesamtstatistik</h2><p class="analysis-note">Die Auswertung konnte nicht geladen werden.</p><button class="btn" data-run-retry>Erneut versuchen</button>';
    root.querySelector('[data-run-retry]').onclick = () => {root.innerHTML = '<p class="muted">Auswertung wird geladen …</p>';loadRunAnalysis(id, request);};
  }
}
function renderRunAnalysis(root, run) {
  const model = runCombatModel(run);
  const label = p => (p.name || 'Unbekannter Spieler') + (!p.identity_known ? ' · Kampf ' + (model.windows.findIndex(w => w.fight.id === p.source_fight_id) + 1) : model.players.some(other => other !== p && other.name === p.name)
    ? ' · ' + (runKnownField(p.job) == null ? 'Klasse unbekannt' : p.class_name || p.job) + ' · Server ' + (runKnownField(p.server_id) ?? 'unbekannt') : '');
  root.innerHTML = `<h2>Dungeon-Gesamtstatistik</h2><p class="analysis-note">Alle ${model.windows.length} erfassten Kämpfe dieses Runs zusammen, ohne Wege und Pausen.</p>
    <div class="chart-tools run-analysis-tools"><label>Auswertung <select data-run-player aria-label="Spieler für Dungeon-Gesamtstatistik"><option value="group">Gesamte Gruppe</option>${model.players.map(p => `<option value="${esc(p.key)}">${esc(label(p))}${p.is_self ? ' · Du' : ''}</option>`).join('')}</select></label>
    <label>Verlauf <select data-run-metric aria-label="Wert im Dungeon-Verlauf"><option value="dps">DPS</option><option value="total">Gesamtschaden</option></select></label>
    <label>Glättung <select data-run-window aria-label="Glättung im Dungeon-Verlauf"><option value="5000">5 Sekunden</option><option value="0">Einzelne Intervalle</option></select></label></div>
    <div class="trend-summary" data-run-summary></div><h3>Damageverlauf · gesamter Dungeon-Run</h3><p class="analysis-note" data-run-coverage></p><div data-run-plot></div>
    <label class="curve-scrubber">Kampfzeit <input type="range" data-run-time aria-label="Zeitpunkt im Dungeon-Verlauf" min="0" max="0" value="0" disabled></label><div class="curve-readout" data-run-readout></div><button class="btn" data-run-fight disabled>Kampfbericht am Zeitpunkt öffnen</button>
    <h3>Skills · gesamter Dungeon-Run</h3><p class="analysis-note" data-run-skill-note></p><div data-run-skills></div>
    <details class="secondary-tools run-method"><summary>So wird gerechnet</summary><p class="analysis-note">Ohne Training. Kampfzeit ohne Wege und Pausen, mindestens 1 Sekunde je Kampf. Fehlende Teilnahme zählt als 0 Schaden. Ein Run belegt keine vollständige Dungeon-Abdeckung.</p><p class="analysis-note">Zeitachse: erfasste Kämpfe chronologisch aneinandergereiht. Linien und Glättung beginnen an Kampfgrenzen neu und verwenden nur Beobachtungen innerhalb desselben Kampfes. Lücken werden nicht interpoliert.</p><p class="analysis-note">Skills mit gleicher ID und gleichem DoT/HoT-Typ werden summiert; Skill-DPS verwendet die gesamte gemeinsame Kampfzeit. Trefferquoten werden nach Trefferzahl gewichtet; unbekannte Merkmale bleiben unbekannt.</p></details>`;
  const scope = root.querySelector('[data-run-player]'), metric = root.querySelector('[data-run-metric]'), smooth = root.querySelector('[data-run-window]');
  const own = model.players.filter(p => p.is_self && (!character || p.name === character));
  if (own.length === 1) scope.value = own[0].key;
  let selected = -1, selectedTime = -1;
  function render(updateSkills = true) {
    const selection = runSelection(model, scope.value), unit = metric.value === 'total' ? 'Schaden' : 'DPS';
    const {samples, missing, partial} = runDamageSamples(model, scope.value, Number(smooth.value), metric.value);
    smooth.disabled = metric.value === 'total';
    const summary = (value, title) => `<div><strong>${value}</strong><span>${title}</span></div>`;
    root.querySelector('[data-run-summary]').innerHTML = summary(num(selection.damage), 'Erfasster Schaden') +
      summary(num(selection.damage * 1000 / Math.max(1000, model.duration)), 'DPS über gemeinsame Kampfzeit') +
      summary(dur(model.duration), 'Erfasste Kampfzeit') + summary(model.windows.length, 'Erfasste Kämpfe');
    // Only what limits this run is shown inline; the method sits under "So wird gerechnet".
    root.querySelector('[data-run-coverage]').textContent = (missing ? `${missing} ${missing === 1 ? 'Kampf' : 'Kämpfe'} ohne auswertbaren Verlauf; Zeitfenster bleiben als Lücken erhalten. ` : '') +
      (partial ? `${partial} ${partial === 1 ? 'Kampf' : 'Kämpfe'} mit unvollständigen Verlaufsdaten. ` : '') +
      (model.windows.some(w => w.fight.numeric_limited) ? 'Parser-Zahlengrenze in mindestens einem Kampf; Messwerte können begrenzt sein. ' : '') +
      (metric.value === 'total' ? 'Gesamtschaden enthält die Summen vorheriger Kämpfe.' : '');
    root.querySelector('[data-run-coverage]').hidden = !root.querySelector('[data-run-coverage]').textContent;
    const plot = root.querySelector('[data-run-plot]'); root.drawnWidth = root.clientWidth;
    plot.innerHTML = curveMarkup(samples, [{name: scope.value === 'group' ? 'Gesamte Gruppe' : label(selection.players[0]),
      color: 'var(--accent)', value: p => p[metric.value]}], {label: 'Dungeon-' + unit + '-Verlauf', unit,
      end: model.duration, width: Math.min(900, root.clientWidth || 900)});
    const range = root.querySelector('[data-run-time]'), readout = root.querySelector('[data-run-readout]'), open = root.querySelector('[data-run-fight]');
    range.max = Math.max(0, samples.length - 1); range.disabled = !samples.length; open.disabled = true; open.onclick = null;
    const show = i => {
      selected = Math.max(0, Math.min(samples.length - 1, i)); range.value = selected;
      const point = samples[selected]; if (!point) {readout.textContent = 'Keine zeitliche Schadensaufzeichnung vorhanden.'; return;}
      selectedTime = point.ms;
      const text = `${dur(point.ms)} Kampfzeit · ${point.boss} · ${dur(point.fight_ms)} im Kampf`;
      readout.innerHTML = `<strong>${num(point[metric.value])} ${unit}</strong><span>${esc(text)}</span>`;
      range.setAttribute('aria-valuetext', text + '; ' + num(point[metric.value]) + ' ' + unit);
      open.disabled = false; open.onclick = () => task(openFight(point.fight_id));
      const svg = plot.querySelector('svg'), cross = plot.querySelector('.curve-cross');
      if (cross) {const x = 58 + point.ms / Math.max(1000, model.duration) * (svg.viewBox.baseVal.width - 76); cross.setAttribute('x1', x); cross.setAttribute('x2', x);}
    };
    const restored = selectedTime < 0 ? samples.length - 1 : samples.findIndex(p => p.ms >= selectedTime);
    show(restored < 0 ? samples.length - 1 : restored); range.oninput = () => show(Number(range.value));
    const hit = plot.querySelector('.curve-hit');
    if (hit) hit.onpointerdown = hit.onpointermove = e => {
      const bounds = hit.getBoundingClientRect(), ms = Math.max(0, Math.min(1, (e.clientX - bounds.left) / bounds.width)) * model.duration;
      let lo = 0, hi = samples.length - 1;
      while (lo < hi) {const mid = (lo + hi) >> 1; if (samples[mid].ms < ms) lo = mid + 1; else hi = mid;}
      if (lo && Math.abs(samples[lo - 1].ms - ms) < Math.abs(samples[lo].ms - ms)) lo--;
      show(lo);
    };
    if (updateSkills) {
      const skillGap = selection.skill_fights < selection.player_fights;
      root.querySelector('[data-run-skill-note]').textContent = skillGap ? `Skilldaten für ${selection.skill_fights} von ${selection.player_fights} Spieler-Kampfteilnahmen. Fehlende Skilldaten bleiben unbekannt; die Tabelle kann weniger Schaden als die Gesamtsumme enthalten.` : '';
      root.querySelector('[data-run-skill-note]').hidden = !skillGap;
      root.querySelector('[data-run-skills]').innerHTML = skillTable(selection.skills);
      bindSkillTables(root);
    }
  }
  scope.onchange = () => {selected = -1; selectedTime = -1; render();}; metric.onchange = smooth.onchange = () => render(false);
  root.redrawRunChart = () => render(false);
  render();
}

let runResizeFrame = 0;
window.addEventListener('resize', () => {
  cancelAnimationFrame(runResizeFrame);
  runResizeFrame = requestAnimationFrame(() => {
    const root = document.querySelector('#runAnalysis');
    if (tab === 'runs' && root?.offsetParent && root.clientWidth !== root.drawnWidth) root.redrawRunChart?.();
  });
});
