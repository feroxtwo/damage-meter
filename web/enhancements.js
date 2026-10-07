// Local combat analysis and QoL. No upload service or third-party assets.
let latestLive = null, fightOffset = 0, fightSearchRequest = 0, fightSearchBusy = false, detailRequest = 0;
let currentFight = null, lastTraining = null;
const pct = v => (Number(v) || 0).toFixed(1).replace('.', ',') + '%';
const metricKey = () => $('#liveMetric').value;
function metricRows(rows) {
  const metric = metricKey();
  const converted = rows.map(r => ({ ...r, damage: Number(r[metric]) || 0,
    dps: metric === 'heal' ? Number(r.hps) || 0 : metric === 'damage_received' ? (Number(r[metric]) || 0) * 1000 / Math.max(latestLive?.battle_time_ms || 0, 1000) : r.dps }));
  const sum = converted.reduce((n,r) => n+r.damage,0);
  converted.forEach(r => r.share = sum ? r.damage*100/sum : 0);
  return converted.sort((a,b) => b.damage-a.damage);
}
window.metricRows = metricRows;
window.bindLiveRows = l => {
  const rows=metricRows(l.rows);
  document.querySelectorAll('#liveRows .bar').forEach((el,i) => {
    el.tabIndex=0; el.setAttribute('role','button'); el.setAttribute('aria-label',rows[i].name+' Skilldetails');
    el.onclick=()=>task(openLivePlayer(rows[i].id));
    el.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();el.click();}};
  });
};
function renderEnhancedLive(l) {
  latestLive=l;
  $(".ranking-head span").textContent=metricKey()==="heal"?"Heilung · HPS · Anteil":metricKey()==="damage_received"?"Erlittener Schaden · pro Sekunde · Anteil":"Schaden · DPS · Anteil";
  $("#selfBurst").textContent="5s Burst: "+num(l.rows.find(r=>r.is_self)?.burst_dps||0)+"/s";
  $('#metricHint').textContent=metricKey()==='heal' ? 'Heilung seit Parser-Reset. HPS nutzt die angezeigte Kampfdauer. Overheal wird nicht abgezogen.' : metricKey()==='damage_received' ? 'Erlittener Schaden aus erfassten NPC-Treffern.' : 'Spieler anklicken für Skilldetails. Burst-DPS: gleitende 5 Sekunden, Beobachtung alle 500 ms.';
  const t=l.training?.state && l.training.state!=='idle' ? l.training : lastTraining;
  if(t) {
    const state={armed:'Bereit. Warte auf ersten Treffer.',running:`Training läuft: ${dur(t.elapsed_ms)} / ${dur(t.seconds*1000)}`,interrupted:'Training unterbrochen: Zielwechsel, Reset oder Verbindung beendet.',finished:`Training abgeschlossen: ${esc(t.target||'')} · ${dur(t.elapsed_ms)}${t.personal_best?' · Neuer persönlicher Bestwert':''}`}[t.state] || '';
    $('#trainingResult').innerHTML=`<p>${state}</p>${(t.rows||[]).map(r=>`<div>${esc(r.name)}: <b>${num(r.dps)}/s</b> · ${num(r.damage)} Schaden</div>`).join('')}${t.best_dps!=null?`<p class="muted">Persönlicher Bestwert: ${num(t.best_dps)}/s</p>`:''}`;
  }
  const player=new URLSearchParams(location.search).get('player');
  if(player && l.rows.some(r=>String(r.id)===player)) { history.replaceState(null,'',location.pathname+location.hash);task(openLivePlayer(Number(player))); }
}
window.renderEnhancedLive=renderEnhancedLive;
$('#liveMetric').onchange=()=>{if(latestLive){renderEnhancedLive(latestLive);$('#liveRows').innerHTML=bars(metricRows(latestLive.rows));bindLiveRows(latestLive);}};
$('#startTraining').onclick=()=>task(api('/api/training',{method:'POST',body:JSON.stringify({seconds:Number($('#trainingDuration').value)})}).then(()=>toast('Training wartet auf den ersten Treffer.')));
$('#stopTraining').onclick=()=>task(api('/api/training',{method:'POST',body:JSON.stringify({seconds:0})}));
api('/api/training').then(t=>{lastTraining=t;if(latestLive)renderEnhancedLive(latestLive);}).catch(()=>{});

function exportPlayers(players,anonymous) {
  return players.map((p,i)=>({...p,name:anonymous?'Spieler '+(i+1):p.name,
    skills:p.skills||[],heal_skills:p.heal_skills||[]}));
}
async function copyText(text) {
  try {await navigator.clipboard.writeText(text);}catch(e) {
    const input=document.createElement('textarea');input.value=text;document.body.append(input);input.select();
    const ok=document.execCommand('copy');input.remove();if(!ok)throw new Error('Zwischenablage nicht verfügbar');
  }
  toast('Ergebnis kopiert.');
}
function rankingText(title,players,ms) {
  return [title+' · '+dur(ms),...players.map((p,i)=>`${i+1}. ${p.name} | ${num(p.damage)} Schaden | ${num(p.dps)}/s | ${num(p.heal||0)} Heilung`)].join('\n');
}
$('#copyLive').onclick=()=>{if(latestLive)task(copyText(rankingText(latestLive.target_name||'Kampf',exportPlayers(latestLive.rows,$('#anonymousExport').checked),latestLive.battle_time_ms)));};
function download(name,body,type) {
  const url=URL.createObjectURL(new Blob([body],{type}));const a=document.createElement('a');a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
}
// Spreadsheet formula injection is escaped in addition to RFC 4180 quoting.
function csvCell(value) {let s=String(value??'');if(/^[\s]*[=+\-@]/.test(s))s="'"+s;return '"'+s.replaceAll('"','""')+'"';}
function fightExport(f,anonymous) {
  // Deliberately allowlist fields: notes, network addresses and database IDs stay local.
  const players=exportPlayers(f.players,anonymous).map(p=>({name:p.name,class_name:p.class_name,damage:p.damage,dps:p.dps,heal:p.heal,hps:p.hps,damage_received:p.damage_received,
    skills:p.skills,heal_skills:p.heal_skills,buffs:(p.buffs||[]).map(b=>({name:b.name,uptime:b.uptime}))}));
  return {boss:f.boss_name,difficulty:f.difficulty,started_at:f.started_at,duration_ms:f.duration_ms,healing_scope:f.healing_scope,players};
}

async function loadFights(reset=true) {
  if(!reset && fightSearchBusy)return;
  const request=++fightSearchRequest;fightSearchBusy=true;
  if(reset){fightOffset=0;$('#fightResults').textContent='Kämpfe werden geladen …';}
  $('#moreFights').disabled=true;
  const q=new URLSearchParams({query:$('#fightSearch').value,character, favorites:String($('#fightFavorites').checked),offset:String(fightOffset)});
  if($('#fightFrom').value)q.set('from',String(new Date($('#fightFrom').value+'T00:00:00').getTime()));
  if($('#fightTo').value)q.set('to',String(new Date($('#fightTo').value+'T23:59:59.999').getTime()));
  try {
    const data=await api('/api/fights?'+q);if(request!==fightSearchRequest)return;
    const rows=(data.fights||[]).map(f=>`<tr><td>${date(f.started_at)}</td><td><button class="btn" data-open-fight="${esc(f.id)}">${f.favorite?'★ ':''}${esc(f.boss_name)}${f.is_train?' · Training':''}</button>${badge(f.difficulty)}<div class="muted">${esc(f.tags||'')}</div></td><td>${dur(f.duration_ms)}</td><td>${num(f.my_dps)}/s</td></tr>`).join('');
    if(reset)$('#fightResults').innerHTML=rows?`<div class="table-scroll"><table><thead><tr><th>Datum</th><th>Kampf</th><th>Dauer</th><th>Meine DPS</th></tr></thead><tbody>${rows}</tbody></table></div>`:'<p class="muted">Keine passenden Kämpfe.</p>';
    else if(rows)$('#fightResults tbody').insertAdjacentHTML('beforeend',rows);
    fightOffset+=(data.fights||[]).length;$('#moreFights').hidden=!data.more;
    $('#fightResults').querySelectorAll('[data-open-fight]').forEach(b=>b.onclick=()=>task(openFight(b.dataset.openFight)));
  }catch(e){if(reset&&request===fightSearchRequest)$('#fightResults').textContent='Kampfliste konnte nicht geladen werden.';throw e;}
  finally{if(request===fightSearchRequest){fightSearchBusy=false;$('#moreFights').disabled=false;}}
}
window.loadFights=loadFights;
$('#searchFights').onclick=()=>task(loadFights());$('#moreFights').onclick=()=>task(loadFights(false));
$('#fightSearch').onkeydown=e=>{if(e.key==='Enter')task(loadFights());};
$('#fightFavorites').onchange=()=>task(loadFights());
$('#closeDialog').onclick=()=>{$('#fightDialog').close();detailRequest++;};

function skillTable(skills,heal=false) {
  if(!skills?.length)return '<p class="muted">Keine Skilldaten vorhanden.</p>';
  return `<div class="table-scroll"><table><thead><tr><th>Skill</th><th>${heal?'Heilung':'Schaden'}</th><th>Treffer / Ticks</th>${heal?'':'<th>Krit</th><th>Rücken</th><th>Frontal</th><th>Perfekt</th><th>Double</th><th>Pariert</th><th>Multihit</th><th>Max</th>'}</tr></thead><tbody>${skills.map(s=>`<tr><td>${esc(s.name)}${s.is_dot?` <span class="badge">${heal?'HoT':'DoT'}</span>`:''}</td><td class="num">${num(s.damage)}</td><td>${s.hits||0}</td>${heal?'':`<td>${pct(s.crit_rate)}</td><td>${pct(s.back_rate)}</td><td>${pct(s.frontal_rate)}</td><td>${pct(s.perfect_rate)}</td><td>${pct(s.double_rate)}</td><td>${pct(s.parry_rate)}</td><td title="${num(s.multi_hit_damage)} zusätzlicher Schaden">${s.multi_hit_count||0}</td><td>${num(s.max)}</td>`}</tr>`).join('')}</tbody></table></div>`;
}
function svgCurve(points,value,label) {
  if(!points.length)return '<p class="muted">Für diesen Kampf wurden keine Verlaufsdaten gespeichert.</p>';
  const W=900,H=180,L=55,R=15,T=12,B=28, end=Math.max(1000,...points.map(p=>p.ms)), max=Math.max(1,...points.map(value));
  const x=ms=>L+ms/end*(W-L-R),y=v=>T+(1-v/max)*(H-T-B);
  const line=points.map((p,i)=>`${i?'L':'M'}${x(p.ms).toFixed(2)},${y(value(p)).toFixed(2)}`).join(' ');
  return `<svg class="timeline" viewBox="0 0 ${W} ${H}" role="img" aria-label="${esc(label)}"><path d="M${L},${T}V${H-B}H${W-R}" fill="none" stroke="#7898b655"/><path d="${line}" fill="none" stroke="#5ad2c8" stroke-width="2"/><text x="0" y="20">${num(max)}</text><text x="${L}" y="${H-5}">0:00</text><text x="${W-65}" y="${H-5}">${dur(end)}</text>${points.filter((_,i)=>i%Math.max(1,Math.ceil(points.length/300))===0).map(p=>`<circle cx="${x(p.ms)}" cy="${y(value(p))}" r="3" fill="transparent"><title>${dur(p.ms)}: ${num(value(p))} ${esc(label)}</title></circle>`).join('')}</svg>`;
}
function damageCurve(f,actor=null) {
  const points=f.analytics?.points||[];
  const total=p=>actor==null?Object.values(p.damage).reduce((a,b)=>a+Number(b),0):Number(p.damage[actor]||0);
  const curves=points.map((p,i)=>{const prev=points[i-1];return {ms:p.ms,dps:Math.max(0,total(p)-(prev?total(prev):0))*1000/Math.max(500,p.ms-(prev?prev.ms:0))};});
  return svgCurve(curves,p=>p.dps,'beobachtete Intervall-DPS');
}
function hitTimeline(skills,ms) {
  const rows=(skills||[]).filter(s=>s.hit_timestamps?.length);if(!rows.length)return '<p class="muted">Keine Trefferzeitpunkte gespeichert. Ältere Kämpfe enthalten diese Daten nicht.</p>';
  const end=Math.max(ms,1000),W=900,L=210,H=rows.length*25+30;
  return `<p class="analysis-note">Trefferzeitpunkte, keine Cast-Anzahl. DoT und Multihit können mehrere Treffer pro Skill-Ausführung erzeugen. Pro Skill werden höchstens 250 Marker gezeichnet.</p><svg class="timeline" viewBox="0 0 ${W} ${H}" role="img" aria-label="Skill-Trefferzeitlinie">${rows.map((s,i)=>`<text x="0" y="${i*25+18}">${esc(s.name.slice(0,28))}</text><path d="M${L},${i*25+14}H${W}" stroke="#7898b622"/>${s.hit_timestamps.filter((_,j)=>j%Math.max(1,Math.ceil(s.hit_timestamps.length/250))===0).map(t=>`<circle cx="${L+Math.min(end,Math.max(0,t))/end*(W-L)}" cy="${i*25+14}" r="3" fill="#edc57b"><title>${esc(s.name)} · ${(t/1000).toFixed(2)} s</title></circle>`).join('')}`).join('')}</svg>`;
}
function effectTimeline(f,actor) {
  const effects=(f.analytics?.effects||[]).filter(e=>e.target===actor);if(!effects.length)return '';
  const keys=[...new Set(effects.map(e=>e.code))],W=900,L=210,end=Math.max(1000,f.duration_ms);
  const names=Object.fromEntries([...(f.boss_debuffs||[]),...f.players.flatMap(p=>p.buffs||[])].map(e=>[e.code,e.name]));
  return `<h4>Buff-Zeitlinie</h4><p class="analysis-note">Aus beobachteter Anwendung und gemeldeter Dauer. Vorzeitiges Entfernen wird derzeit nicht erkannt.</p><svg class="timeline" viewBox="0 0 ${W} ${keys.length*24+20}" role="img" aria-label="Buff-Zeitlinie">${keys.map((code,i)=>`<text x="0" y="${i*24+17}">${esc((names[code]||'#'+code).slice(0,28))}</text>${effects.filter(e=>e.code===code).map(e=>`<rect x="${L+e.start_ms/end*(W-L)}" y="${i*24+5}" width="${Math.max(1,(e.end_ms-e.start_ms)/end*(W-L))}" height="13" rx="3" fill="#966ee6"><title>${esc(names[code]||code)} · ${(e.start_ms/1000).toFixed(1)}–${(e.end_ms/1000).toFixed(1)} s</title></rect>`).join('')}`).join('')}</svg>`;
}
function playerReport(p,f) {
  return `<details><summary>${chip(p)} · ${num(p.dps)}/s · ${num(p.heal||0)} Heilung · ${num(p.damage_received||0)} erlitten</summary><h4>Schadensskills</h4>${skillTable(p.skills)}<h4>Heilungsskills</h4>${skillTable(p.heal_skills,true)}${uptimes(p.buffs,false)}<h4>Skill-Trefferzeitlinie</h4>${hitTimeline(p.skills,f.duration_ms)}${effectTimeline(f,p.actor_id)}<h4>DPS-Verlauf</h4>${damageCurve(f,p.actor_id)}</details>`;
}
async function openLivePlayer(id) {
  const request=++detailRequest;const p=await api('/api/players/'+id);if(request!==detailRequest)return;
  const row=latestLive?.rows.find(r=>r.id===id);
  $('#dialogTitle').textContent=row?.name||'Spielerdetails';
  $('#fightContent').innerHTML=`<p class="analysis-note">Aktueller Stand. ${num(row?.burst_dps||0)}/s Burst über 5 Sekunden. Heilung seit Parser-Reset.</p><button class="btn" id="refreshPlayer">Aktualisieren</button><h3>Schaden</h3>${skillTable(p.skills)}<h3>Heilung</h3>${skillTable(p.heal_skills,true)}${hitTimeline(p.skills,p.duration_ms)}`;
  $('#refreshPlayer').onclick=()=>task(openLivePlayer(id));if(!$('#fightDialog').open)$('#fightDialog').showModal();
}
async function openFight(id) {
  const request=++detailRequest;const f=await api('/api/fights/'+encodeURIComponent(id));if(request!==detailRequest)return;
  currentFight=f;$('#dialogTitle').textContent=(f.boss_name||'Kampf')+' · '+(f.difficulty||'');
  $('#fightContent').innerHTML=`<p>${date(f.started_at)} · ${dur(f.duration_ms)} · ${num(f.total_damage)} Schaden</p>
    <p class="analysis-note">${esc(f.healing_scope||'Erfasste Heilung. Keine Aussage über Overheal.')}<br>${f.analytics?`DPS-Verlauf: Beobachtung alle ${f.analytics.resolution_ms||500} ms${f.analytics.partial?' · unvollständige Daten':''}.`:'Keine zeitliche Schadensaufzeichnung vorhanden.'} Vollständigkeit vor Erfassungsbeginn unbekannt. Ergebnis: ${f.analytics?.outcome==='kill'?'Tod des Ziels erfasst':'unbekannt'}.</p>
    <div class="row fight-tools"><button class="btn" id="copyFight">Kopieren</button><button class="btn" id="jsonFight">JSON</button><button class="btn" id="csvFight">CSV</button><label><input type="checkbox" id="anonFight" checked> Namen anonymisieren</label></div>
    <div class="row fight-tools"><label><input type="checkbox" id="favoriteFight" ${f.favorite?'checked':''}> Favorit</label><input id="fightNote" placeholder="Notiz" maxlength="4000" value="${esc(f.note||'')}"><input id="fightTags" placeholder="Tags, z. B. neues Gear" maxlength="500" value="${esc(f.tags||'')}"><button class="btn" id="saveFightNote">Speichern</button></div>
    <h3>Direkter Kampfvergleich</h3><div class="row"><select id="compareFight" aria-label="Vergleichskampf"><option value="">Vergleich laden …</option></select><button class="btn" id="compareBtn">Vergleichen</button></div><p class="analysis-note">Gleicher Boss und Schwierigkeitsgrad. Eigene Werte werden nur bei gleichem Charakter und gleicher Klasse verglichen.</p><div id="comparison"></div>
    <h3>Gruppen-DPS im Kampfverlauf</h3>${damageCurve(f)}<h3>Ping-Verlauf</h3>${svgCurve((f.ping_history||[]).map(p=>({ms:p.tsMs,ping:p.pingMs})),p=>p.ping,'ms Ping')}
    ${f.players.map(p=>playerReport(p,f)).join('')}${effectTimeline(f,f.target_id)}${uptimes(f.boss_debuffs,true)}`;
  if(!$('#fightDialog').open)$('#fightDialog').showModal();
  $('#saveFightNote').onclick=()=>task(api('/api/fights/'+encodeURIComponent(id)+'/annotation',{method:'POST',body:JSON.stringify({favorite:$('#favoriteFight').checked,note:$('#fightNote').value,tags:$('#fightTags').value})}).then(()=>{toast('Kampfnotiz gespeichert.');if(tab==='runs')task(loadFights());}));
  $('#copyFight').onclick=()=>task(copyText(rankingText(f.boss_name,exportPlayers(f.players,$('#anonFight').checked),f.duration_ms)));
  $('#jsonFight').onclick=()=>download('aion2-kampf.json',JSON.stringify(fightExport(f,$('#anonFight').checked),null,2),'application/json');
  $('#csvFight').onclick=()=>{const exp=fightExport(f,$('#anonFight').checked);download('aion2-kampf.csv','\uFEFF'+[['Spieler','Klasse','Schaden','DPS','Heilung','HPS','Erlittener Schaden'],...exp.players.map(p=>[p.name,p.class_name,p.damage,p.dps,p.heal,p.hps,p.damage_received])].map(r=>r.map(csvCell).join(';')).join('\r\n'),'text/csv;charset=utf-8');};
  $('#compareBtn').onclick=()=>task(compareFight());
  try {const data=await api('/api/fights?query='+encodeURIComponent(f.boss_name||'')+'&character='+encodeURIComponent(f.players.find(p=>p.is_self)?.name||''));if(request!==detailRequest)return;
    const candidates=(data.fights||[]).filter(c=>c.id!==id&&c.boss_name===f.boss_name&&c.dungeon_id===f.dungeon_id);
    $('#compareFight').innerHTML='<option value="">Vergleichskampf wählen</option>'+candidates.map(c=>`<option value="${esc(c.id)}">${date(c.started_at)} · ${dur(c.duration_ms)} · ${num(c.my_dps)}/s</option>`).join('');
  }catch(e){if(request===detailRequest)$('#compareFight').innerHTML='<option value="">Vergleiche nicht erreichbar</option>';}
}
window.openFight=openFight;
function delta(a,b){const d=(Number(a)||0)-(Number(b)||0);return `<span class="${d>=0?'compare-positive':'compare-negative'}">${d>=0?'+':''}${num(d)}${b?' ('+(d/b*100).toFixed(1)+'%)':''}</span>`;}
async function compareFight() {
  const id=$('#compareFight').value;if(!id){toast('Vergleichskampf wählen.');return;}
  const f=currentFight,request=detailRequest;const old=await api('/api/fights/'+encodeURIComponent(id));if(request!==detailRequest)return;
  const me=f.players.find(p=>p.is_self),before=old.players.find(p=>p.is_self);
  if(old.boss_name!==f.boss_name||old.dungeon_id!==f.dungeon_id||!me||!before||me.name!==before.name||me.job!==before.job){$('#comparison').textContent='Dieser Vergleich passt nicht zu Boss, Schwierigkeit, Charakter oder Klasse.';return;}
  const skillKey=s=>String(s.code)+'|'+Boolean(s.is_dot);
  const keys=[...new Set([...(me.skills||[]),...(before.skills||[])].map(skillKey))];
  $('#comparison').innerHTML=`<p>Aktueller Kampf gegenüber ${date(old.started_at)}: DPS ${delta(me.dps,before.dps)} · Dauer ${delta(f.duration_ms/1000,old.duration_ms/1000)} Sekunden</p><div class="table-scroll"><table><thead><tr><th>Skill</th><th>Schaden jetzt</th><th>Schaden zuvor</th><th>Differenz</th><th>Krit jetzt / zuvor</th></tr></thead><tbody>${keys.map(k=>{const a=(me.skills||[]).find(s=>skillKey(s)===k),b=(before.skills||[]).find(s=>skillKey(s)===k);return `<tr><td>${esc(a?.name||b?.name)}${(a||b)?.is_dot?' · DoT':''}</td><td>${num(a?.damage)}</td><td>${num(b?.damage)}</td><td>${delta(a?.damage,b?.damage)}</td><td>${pct(a?.crit_rate)} / ${pct(b?.crit_rate)}</td></tr>`;}).join('')}</tbody></table></div><h4>Buff-Uptime jetzt / zuvor</h4>${[...new Set([...(me.buffs||[]),...(before.buffs||[])].map(b=>b.code))].map(k=>{const a=me.buffs?.find(b=>b.code===k),b=before.buffs?.find(b=>b.code===k);return `<p>${esc(a?.name||b?.name)}: ${pct(a?.uptime)} / ${pct(b?.uptime)}</p>`;}).join('')}`;
}
$('#saveProfile').onclick=()=>task(api('/api/overlay/profile',{method:'POST',body:JSON.stringify({key:$('#profileName').value.trim()||latestLive?.character||'Standard',save:true})}).then(()=>toast('Profil gespeichert.')));
$('#loadProfile').onclick=()=>task(api('/api/overlay/profile',{method:'POST',body:JSON.stringify({key:$('#profileName').value.trim()||latestLive?.character||'Standard',save:false})}).then(()=>loadSettings()).then(()=>toast('Profil geladen.')));
$('#recoverOverlay').onclick=()=>task(api('/api/overlay').then(s=>api('/api/overlay',{method:'POST',body:JSON.stringify({...s,position:[40,40],visible:true,locked:false})})).then(()=>loadSettings()).then(()=>toast('Overlay auf Startposition zurückgeholt.')));
if(tab==='runs')task(loadFights());
