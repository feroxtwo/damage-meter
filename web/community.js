// Explicit, permissioned reference imports. No automatic requests to community websites.
// All imported numbers remain labelled as unverified third-party aggregates.
const communityRegions = ['ALL', 'EU', 'NAE', 'NAW', 'SA', 'ASIA', 'KR', 'TW'];
const communityReasons = {
  training: 'Training zählt nicht als Bosskampf.',
  limited_data: 'Die Kampfdaten sind begrenzt oder unvollständig.',
  unknown_boss: 'Die Boss-ID fehlt.',
  not_boss: 'Das Ziel wurde als normaler Gegner erkannt.',
  short_fight: 'Kämpfe unter 10 Sekunden sind nicht vergleichbar.',
  missing_cp: 'Die Kampfkraft fehlt in diesem Kampf.',
  missing_dps: 'Es wurde keine DPS erfasst.',
  no_matching_reference: 'Keine importierten Referenzwerte für diese Boss-ID, Schwierigkeit, Klasse, Kampfkraft, Region und Balance-Periode.'
};
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
function communityComparisonMarkup(response) {
  if (!response) return '<p class="analysis-note">Kein Community-Vergleich verfügbar.</p>';
  const title = '<div class="skill-index-head"><div><span class="eyebrow">COMMUNITY · IMPORTIERTE DATEN</span><h3>Externe DPS-Referenzen</h3></div><span>Region ' + esc(response.region || 'ALL') + '</span></div>';
  if (!['ready', 'indicative'].includes(response.status)) {
    return title + '<p class="analysis-note">' + esc(communityReasons[response.reason] || 'Keine geeigneten Community-Daten importiert.') +
      ' Ohne verifizierten Referenzdatensatz wird kein Online-Score angezeigt.</p><button type="button" class="btn" data-community-settings>Datenquellen öffnen</button>';
  }
  const legacy=response.status==='indicative';
  const entries = (response.comparisons || []).map(row => {
    const score = Number(row.score);
    const sign = score >= 100 ? '+' : '−';
    const delta = Math.abs(score - 100).toFixed(1).replace('.', ',');
    return '<div class="community-reference">' +
      '<div><span class="eyebrow">' + esc(row.source_id) + ' · ' + (row.scope === 'same_class' ? 'Gleiche Klasse' : 'Alle Klassen') + '</span>' +
      '<strong class="' + (score >= 100 ? 'compare-positive' : 'compare-negative') + '">' + communityRatio(score) + '</strong>' +
      '<span>' + sign + delta + ' % zum Median · ' + num(row.reference_dps) + ' Referenz-DPS' + (row.comparison_quality==='legacy_unspecified'?' · Richtwert, DPS-Methode unbekannt':' · dokumentierte Kampf-DPS') + '</span></div>' +
      '<div class="community-ref-detail">' + num(row.samples) + (row.comparison_quality==='legacy_unspecified'?' Beobachtungen (Stichprobe unbestätigt)':' unabhängige Spieler laut Quelle') + ' · KP ' + num(row.cp_min) + '–' + num(row.cp_max) +
      ' · ' + esc(row.region) + ' · ' + esc(row.balance_id) + '<div><a href="' + esc(row.source_url) +
      '" target="_blank" rel="noopener noreferrer">Quelle ansehen ↗</a></div></div></div>';
  }).join('');
  return title + '<div class="community-comparisons">' + entries + '</div>' +
    '<p class="analysis-note">'+(legacy?'Nur Richtwerte: bei älteren Datensätzen sind DPS-Methode, Killfilter und unabhängige Stichprobe nicht nachgewiesen. ':'100 = Median vergleichbarer Kampf-DPS nach importierter Methodendeklaration. ')+'Quelle und Berechtigung sind Selbstauskünfte des Importierenden, nicht unabhängig verifiziert. Kein offizieller Skill Index oder Perzentil. Kampfwerte bleiben lokal.</p>';
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
    if ($('#fightDialog').open) $('#fightDialog').close();
    show('stats');
    const sources=$('#communitySourcesCard');sources.open=true;sources.scrollIntoView({block:'start'});sources.querySelector('summary').focus({preventScroll:true});
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
    root.innerHTML = '<h3>Verfügbare Quellen</h3><div class="reference-providers">' + providers + '</div>' +
      '<h3>Auf diesem Gerät importiert</h3>' +
      (imported || '<p class="analysis-note">Noch kein berechtigter Community-Datensatz importiert.</p>') +
      '<p class="analysis-note">Automatische Uploads und Abrufe von Drittanbietern sind deaktiviert. Ein öffentlicher Webauftritt ist keine bestätigte Import-Lizenz.</p>';
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
