// Comparison with the integrated reference data and optional own imports.
// No automatic requests to websites; fight data stays on this device.
const communityRegions = ['ALL', 'EU', 'NAE', 'NAW', 'SA', 'ASIA', 'KR', 'TW'];
const communityReasons = {
  training: 'Training zählt nicht als Bosskampf.',
  limited_data: 'Die Kampfdaten sind begrenzt oder unvollständig.',
  unknown_boss: 'Die Boss-ID fehlt.',
  not_boss: 'Das Ziel wurde als normaler Gegner erkannt.',
  short_fight: 'Kämpfe unter 10 Sekunden sind nicht vergleichbar.',
  missing_cp: 'Die Kampfkraft fehlt in diesem Kampf.',
  missing_dps: 'Es wurde keine DPS erfasst.',
  no_matching_reference: 'Für diesen Boss und deine Klasse gibt es noch keine Vergleichswerte.'
};
// Caveats and reasons, shown in words, never as raw flags.
const communityBlockers = {
  balance_period_unconfirmed: 'Patch-Zeitraum nicht belegt',
  all_time_mixes_balance_periods: 'mischt mehrere Patch-Zeiträume',
  methodology_not_a2m_v2: 'DPS-Berechnung kann leicht abweichen',
  methodology_unconfirmed: 'DPS-Berechnung des Datensatzes nicht bestätigt',
  region_scope_unconfirmed: 'Region nicht eindeutig',
  region_unspecified: 'Region nicht eindeutig',
  KR_TW_combined: 'Korea und Taiwan gemeinsam',
  pooled_region: 'mehrere Regionen gemeinsam',
  dungeon_scope_unconfirmed: 'Schwierigkeit nicht angegeben',
  boss_and_difficulty_unspecified: 'Boss oder Schwierigkeit nicht eindeutig',
  boss_id_ambiguous_or_aggregate: 'Boss oder Schwierigkeit nicht eindeutig',
  cp_band_over_20000: 'breiter Kampfkraftbereich',
  other_class_cohort: 'nur für alle Klassen gemeinsam',
  training: 'Trainingsdaten',
  index_not_absolute_dps: 'Index statt absoluter DPS',
  instance_level: 'Wert für die ganze Instanz, nicht nur diesen Boss',
  kill_unconfirmed: 'Kill oder vollständige Aufzeichnung nicht bestätigt',
  invalid_reference: 'ungültiger Referenzwert'
};
// Position within the dataset's published quantiles.
const communityBands = {
  above_p90: 'Top 10 %',
  p75_p90: 'Top 25 %',
  above_p75: 'Top 25 %',
  p50_p75: 'obere Hälfte',
  above_median: 'über dem Median',
  p25_p50: 'untere Hälfte',
  below_p25: 'unteres Viertel',
  below_median: 'unter dem Median'
};
function communityBlockerText(codes) {
  const labels = [...new Set((codes || []).map(code => communityBlockers[code] || 'weitere Abweichung'))];
  return labels.map(esc).join(' · ');
}
function withheldMarkup(response) {
  const offline = response.offline_observations || {};
  const withheld = response.withheld || [];
  const parts = [];
  if (offline.rows > 0) {
    parts.push(num(offline.rows) + ' Vergleichsgruppen zu diesem Boss oder Dungeon (' +
      (offline.sources || []).map(esc).join(', ') + '), davon ' + num(offline.same_class || 0) + ' für deine Klasse.');
  }
  if (withheld.length) parts.push(num(withheld.length) + ' importierte Referenzgruppen passen zu Boss und Kampfkraft.');
  const reasons = communityBlockerText(withheld.map(w => w.reason));
  return '<p class="analysis-note community-withheld"><strong>Keine passenden Vergleichswerte.</strong> ' + parts.join(' ') +
    ' Keine Gruppe passt zugleich zu Klasse, Boss, Schwierigkeit, Kampfkraft und Region.' +
    (reasons ? ' Importierte Daten: ' + reasons + '.' : '') + '</p>';
}
const communitySourceStatuses = {
  active: 'Aktiv und lokal',
  permission_required: 'Schnittstelle / Nutzungsrechte offen',
  api_unverified: 'Abruf und Nutzungsrechte offen',
  not_connected: 'Nicht verbunden',
  import_ready: 'JSON-Import verfügbar'
};
function selectedCommunityRegion() {
  try {
    const value = localStorage.getItem('a2m-community-region');
    return communityRegions.includes(value) ? value : 'ALL';
  } catch {
    return 'ALL';
  }
}
function communityRatio(score) {
  const n = Number(score);
  return Number.isFinite(n) && n > 0 ? n.toLocaleString('de-DE', { maximumFractionDigits: 1 }) : '—';
}
function communityEntryMarkup(row) {
  const score = Number(row.score);
  const sign = score >= 100 ? '+' : '−';
  const delta = Math.abs(score - 100).toFixed(1).replace('.', ',');
  const reference = row.comparison_quality === 'reference';
  const label = reference ? esc(row.dataset || 'Vergleichsdaten') + ' · ' + (row.level === 'boss' ? 'dieser Boss' : 'ganze Instanz')
    : esc(row.source_id) + ' · ' + (row.scope === 'same_class' ? 'Gleiche Klasse' : 'Alle Klassen');
  const band = communityBands[row.band] ? ' · ' + communityBands[row.band] : '';
  const cp = row.cp_min != null && row.cp_max != null ? 'KP ' + num(row.cp_min) + '–' + num(row.cp_max) : 'alle Kampfkraftstufen';
  const caveats = communityBlockerText(row.caveats);
  const detail = reference
    ? (row.samples ? num(row.samples) + ' Spieler · ' : '') + cp + ' · Region ' + esc(row.region) +
      (caveats ? '<div class="muted">Hinweis: ' + caveats + '</div>' : '')
    : num(row.samples) + ' unabhängige Spieler laut Quelle · ' + cp + ' · ' + esc(row.region) + ' · ' + esc(row.balance_id) +
      (row.source_url ? '<div><a href="' + esc(row.source_url) + '" target="_blank" rel="noopener noreferrer">Quelle ansehen ↗</a></div>' : '');
  return '<div class="community-reference">' +
    '<div><span class="eyebrow">' + label + '</span>' +
    '<strong class="' + (score >= 100 ? 'compare-positive' : 'compare-negative') + '">' + communityRatio(score) + '</strong>' +
    '<span>' + sign + delta + ' % zum Median · ' + num(Math.round(row.reference_dps)) + ' Median-DPS deiner Klasse' + band + '</span></div>' +
    '<div class="community-ref-detail">' + detail + '</div></div>';
}
function communityComparisonMarkup(response) {
  if (!response) return '<p class="analysis-note">Kein Community-Vergleich verfügbar.</p>';
  const region = response.region && response.region !== 'ALL' ? 'Region ' + esc(response.region) : 'Alle Regionen';
  const title = '<div class="skill-index-head"><div><span class="eyebrow">Community · Vergleichsdatenbank</span><h3>Vergleich mit Community-Werten</h3></div><span>' + region + '</span></div>';
  if (response.status !== 'ready') {
    const body = response.reason === 'comparison_withheld' ? withheldMarkup(response) :
      '<p class="analysis-note">Noch kein Community-Vergleich: ' + esc(communityReasons[response.reason] || 'Keine passenden Vergleichswerte vorhanden.') + '</p>';
    return title + body + '<button type="button" class="btn" data-community-settings>Vergleichsdaten verwalten</button>';
  }
  const entries = (response.comparisons || []).filter(row => Number.isFinite(Number(row.score))).map(communityEntryMarkup).join('');
  return title + '<div class="community-comparisons">' + entries + '</div>' +
    '<p class="analysis-note">100 = Median deiner Klasse bei diesem Boss und deiner Kampfkraft. Die Werte sind Aggregate anderer Meter; ihre DPS-Berechnung kann leicht abweichen, lies sie als Richtwert. Deine Kampfwerte bleiben lokal.</p>';
}
async function fillCommunityIndex(id, root, request) {
  const region = selectedCommunityRegion();
  try {
    const result = await api('/api/fights/' + encodeURIComponent(id) + '/community-index?region=' + encodeURIComponent(region));
    if (request !== detailRequest || !root.isConnected) return;
    root.innerHTML = communityComparisonMarkup(result);
  } catch {
    if (request === detailRequest && root.isConnected) {
      root.innerHTML = '<p class="analysis-note">Community-Vergleich konnte nicht geladen werden.</p>';
    }
  }
  const button = root.querySelector('[data-community-settings]');
  if (button) button.onclick = () => {
    const from = tab;
    if ($('#fightDialog').open) $('#fightDialog').close();
    show('stats');
    const sources=$('#communitySourcesCard');sources.open=true;sources.scrollIntoView({block:'start'});sources.querySelector('summary').focus({preventScroll:true});
    // A way back to the report this detour started from.
    const back = $('#communityReturn');back.hidden = false;
    back.onclick = () => { back.hidden = true; show(from); task(openFight(id)); };
  };
}
window.fillCommunityIndex = fillCommunityIndex;

let communitySourceRequest = 0;
async function loadCommunitySources() {
  const request = ++communitySourceRequest;
  const root = $('#communitySources');
  if (!root) return;
  root.innerHTML = '<p class="analysis-note">Quellen werden geladen …</p>';
  try {
    const result = await api('/api/references/sources');
    if (request !== communitySourceRequest) return;
    const providers = (result.providers || []).map(provider => {
      const link = provider.url ? '<a href="' + esc(provider.url) + '" target="_blank" rel="noopener noreferrer">Website ↗</a>' : '';
      return '<div class="reference-provider"><div><strong>' + esc(provider.name) + '</strong><small>' + esc(provider.detail) +
        '</small></div><span>' + esc(communitySourceStatuses[provider.status] || provider.status) + '</span>' + link + '</div>';
    }).join('');
    const imported = (result.imports || []).map(entry =>
      '<div class="reference-imported"><strong>' + esc(entry.source_id) + '</strong>' +
      '<span>' + esc(entry.balance_id) + ' · ' + num(entry.rows) + ' Referenzgruppen · ' +
      date(entry.captured_at) + ' Datenstand · ' + esc(entry.metric==='fight_dps'?'Kampf-DPS / Kills':'Methode nicht bestätigt') + '</span><button type="button" class="btn" data-remove-community="' + esc(entry.source_id) + '" data-balance="' + esc(entry.balance_id) + '">Entfernen</button></div>'
    ).join('');
    const offline = (result.offline_data || []).map(entry =>
      '<div class="reference-imported"><strong>' + esc(entry.dataset || 'Vergleichsdaten') + '</strong><span>' +
      num(entry.row_count) + ' Vergleichsgruppen · Stand ' + date(entry.captured_at) + '</span></div>').join('');
    root.innerHTML = '<h3>Verfügbare Quellen</h3><div class="reference-providers">' + providers + '</div>' +
      (offline ? '<h3>Integrierte Vergleichsdaten</h3>' + offline + '<p class="analysis-note">Im Programm enthalten und offline nutzbar. Verglichen wird nur, was zu Klasse, Boss, Schwierigkeit, Kampfkraft und Region passt.</p>' : '') +
      '<h3>Eigene Importe auf diesem Gerät</h3>' +
      (imported || '<p class="analysis-note">Noch kein eigener Datensatz importiert.</p>') +
      '<p class="analysis-note">Es werden keine Kampfdaten hochgeladen und keine Websites abgefragt.</p>';
    root.querySelectorAll('[data-remove-community]').forEach(button => {
      button.onclick = () => {
        const source = button.dataset.removeCommunity, balance = button.dataset.balance;
        if (!confirm('Referenzdaten von ' + source + ' (' + balance + ') wirklich lokal entfernen?')) return;
        task(api('/api/references/' + encodeURIComponent(source) + '/' + encodeURIComponent(balance),
          { method: 'DELETE' }).then(() => { toast('Referenzdaten entfernt.'); return loadCommunitySources(); }));
      };
    });
  } catch {
    if (request === communitySourceRequest) root.innerHTML = '<p class="analysis-note">Datenquellen konnten nicht geladen werden.</p>';
  }
}
window.loadCommunitySources = loadCommunitySources;

function communityTemplate() {
  const now = Date.now();
  return {
    schema: 'a2m-community-v2',
    source: {
      id: 'community',
      url: 'https://example.org/your-permitted-dataset',
      captured_at: now,
      rights_confirmed: false
    },
    balance: {
      id: 'patch-or-balance-id',
      from_ms: now - 7 * 86400000,
      until_ms: now + 7 * 86400000
    },
    methodology: {
      metric: 'fight_dps',
      outcome: 'confirmed_kill',
      aggregation: 'median_unique_players',
      patch_id: 'replace-with-game-patch'
    },
    rows: []
  };
}
function importStatus(message, failed) {
  const root = $('#communityImportStatus');
  if (!root) return;
  root.textContent = message;
  root.classList.toggle('save-error', !!failed);
}
async function importCommunityFile() {
  const input = $('#communityFile'), button = $('#communityImport'), checked = $('#communityRights');
  if (!input.files?.length) return importStatus('Bitte zuerst eine JSON-Datei auswählen.', true);
  if (!checked.checked) return importStatus('Bitte bestätige die Rechte zur Wiederverwendung dieser Daten.', true);
  const file = input.files[0];
  if (file.size > 512 * 1024) return importStatus('Die Datei darf höchstens 512 KB groß sein.', true);
  button.disabled = true;
  importStatus('Datensatz wird lokal geprüft und gespeichert …', false);
  try {
    const body = JSON.parse(await file.text());
    if (!body || !body.source || !Array.isArray(body.rows)) throw new Error('format');
    body.source.rights_confirmed = true;
    const result = await api('/api/references/import', { method: 'POST', body: JSON.stringify(body) });
    importStatus(num(result.imported) + ' Referenzgruppen gespeichert. Keine Daten wurden an den Anbieter gesendet.', false);
    input.value = '';
    checked.checked = false;
    await loadCommunitySources();
  } catch {
    importStatus('Import abgelehnt. Prüfe Schema, Rechtebestätigung, IDs, Region, Balance-Zeitraum, Stichprobenzahl und KP-Fenster.', true);
  } finally {
    button.disabled = false;
  }
}

const regionSelect = $('#communityRegion');
if (regionSelect) {
  regionSelect.value = selectedCommunityRegion();
  regionSelect.onchange = () => {
    try { localStorage.setItem('a2m-community-region', regionSelect.value); }
    catch { toast('Regionseinstellung konnte nicht gespeichert werden.', true); }
  };
  $('#communityImport').onclick = () => task(importCommunityFile());
  $('#communityTemplate').onclick = () =>
    download('aion2-community-template.json', JSON.stringify(communityTemplate(), null, 2), 'application/json');
  if (tab === 'stats') task(loadCommunitySources());
}
