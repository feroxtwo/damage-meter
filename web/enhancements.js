// Local combat analysis and QoL. No upload service or third-party assets.
let fightOffset = 0, fightSearchRequest = 0, fightSearchBusy = false, detailRequest = 0;
let currentFight = null, lastTraining = null, comparisonRequest = 0, overlayActionBusy = false;
const pct = v => v == null || !Number.isFinite(Number(v)) ? '—' : Number(v).toFixed(1).replace('.', ',') + '%';
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
    el.dataset.playerId=rows[i].id; el.tabIndex=0; el.setAttribute('role','button'); el.setAttribute('aria-label',rows[i].name+' Skilldetails');
    el.onclick=()=>task(openLivePlayer(rows[i].id));
    el.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();el.click();}};
  });
};
function renderLiveMetrics(l) {
  const metric=metricKey(),rows=metricRows(l.rows),total=rows.reduce((n,r)=>n+r.damage,0);
  const rate=metric==='heal'?'HPS':metric==='damage_received'?'Schaden/s':'DPS';
  $('#groupDpsLabel').textContent='Gruppe · '+rate;
  $('#selfDpsLabel').textContent='Ich · '+rate;
  $('#totalDamageLabel').textContent=metric==='heal'?'Erfasste Heilung':metric==='damage_received'?'Erlittener Schaden':'Gesamtschaden';
  const dps=metric==='damage'?total*1000/Math.max(l.battle_time_ms,1000):rows.reduce((n,r)=>n+r.dps,0);
  for(const [id,value] of [['groupDps',dps],['selfDps',rows.find(r=>r.is_self)?.dps||0],['totalDamage',total]]) {
    $('#'+id).textContent=num(value);$('#'+id).title=Number(value).toLocaleString('de-DE',{maximumFractionDigits:2});
  }
  $('#selfBurst').hidden=metric!=='damage';
}
function updateLiveRows(l) {
  const box=$('#liveRows'),focused=document.activeElement?.closest('#liveRows [data-player-id]')?.dataset.playerId;
  const html=bars(metricRows(l.rows),{empty:'<svg class="icon" aria-hidden="true"><use href="#icon-live"/></svg><b>Bereit für den nächsten Kampf</b><p>Deine Gruppe erscheint, sobald Kampfdaten ankommen. Nutze „Höchster Schaden“ für normale Gegner oder „Trainingspuppe“ für einen Test.</p>'});
  if(box.innerHTML!==html) {
    box.innerHTML=html;bindLiveRows(l);
    if(focused)box.querySelector('[data-player-id="'+CSS.escape(focused)+'"]')?.focus({preventScroll:true});
  }
}
function captureHelp(c) {
  const age=c.last_packet_ms==null?null:Math.max(0,Date.now()-c.last_packet_ms);
  let text='';
  if(!c.permission)text='Paketmitschnitt nicht freigegeben. Nach Installation oder Update einmal ausführen: sudo setcap cap_net_raw=ep '+(c.binary?"'"+c.binary.replaceAll("'","'\\''")+"'":'/pfad/zur/aion2-meter')+' und danach das Meter neu starten.';
  else if(c.error)text='Der Paketmitschnitt ist gestoppt. Prüfe Capture-Berechtigung und Terminalmeldung, starte das Meter anschließend neu.';
  else if(!c.game_running)text='Starte AION 2 auf diesem Rechner. Das Meter erkennt den Prozess AION2.exe unter Proton. Eine abweichende Prozessbezeichnung kann mit --any-process getestet werden.';
  else if(!c.locked_port)text='Das Spiel läuft. Logge deinen Charakter ein und greife ein Ziel an. Die Verbindungserkennung braucht mehrere passende Pakete. Bei VPN-Problemen Verbindung wechseln und erneut testen.';
  else if(age>15000)text='Seit '+Math.floor(age/1000)+' Sekunden kein Paket der Spielverbindung. Außerhalb eines Kampfes kann das normal sein. Bei laufendem Kampf Verbindung und VPN prüfen. Die Anzeige enthält die letzten erfassten Werte.';
  $('#captureHelp').hidden=!text;$('#captureHelp').textContent=text;
}
$('#copyDiagnostics').onclick=()=>task((async()=>{
  const v=await api('/api/version'),c=latestLive?.capture||{};
  await copyText(JSON.stringify({version:v.version,parser:v.parser_version,permission:!!c.permission,game_running:!!c.game_running,connected:!!c.locked_port,packets:c.packets||0,packet_age_seconds:c.last_packet_ms==null?null:Math.max(0,Math.round((Date.now()-c.last_packet_ms)/1000)),stream_gaps:c.stream_gaps||0,capture_error:!!c.error,recording_error:!!c.recording_error},null,2));
})());
function renderEnhancedLive(l) {
  latestLive=l;captureHelp(l.capture);renderLiveMetrics(l);
  $('#numericWarning').hidden=!l.numeric_limited;
  $(".ranking-head span").textContent=metricKey()==="heal"?"Heilung · HPS · Anteil":metricKey()==="damage_received"?"Erlittener Schaden · pro Sekunde · Anteil":"Schaden · DPS · Anteil";
  $("#selfBurst").textContent="5s Burst: "+num(l.rows.find(r=>r.is_self)?.burst_dps||0)+"/s";
  $('#metricHint').textContent=metricKey()==='heal' ? 'Heilung seit Parser-Reset. HPS nutzt die angezeigte Kampfdauer. Overheal wird nicht abgezogen.' : metricKey()==='damage_received' ? 'Erlittener Schaden aus erfassten NPC-Treffern.' : 'Spieler anklicken für Skilldetails. Burst-DPS: gleitende 5 Sekunden, Beobachtung alle 500 ms.';
  if(window.applyAppearance)applyAppearance(l.overlay);
  const t=l.training?.state && l.training.state!=='idle' ? l.training : lastTraining;
  if(t) {
    const state={armed:'Bereit. Warte auf ersten Treffer.',running:`Training läuft: ${dur(t.elapsed_ms)} / ${dur(t.seconds*1000)}`,interrupted:'Training unterbrochen: Zielwechsel, Reset oder Verbindung beendet.',finished:`Training abgeschlossen: ${esc(t.target||'')} · ${dur(t.elapsed_ms)}${t.personal_best?' · Neuer persönlicher Bestwert':''}`}[t.state] || '';
    $('#trainingResult').innerHTML=`<p>${state}</p>${(t.rows||[]).map(r=>`<div>${esc(r.name)}: <b>${num(r.dps)}/s</b> · ${num(r.damage)} Schaden</div>`).join('')}${t.best_dps!=null?`<p class="muted">Persönlicher Bestwert: ${num(t.best_dps)}/s</p>`:''}`;
  }
  if(l.reset_notice && l.reset_notice!==window.lastResetNotice){window.lastResetNotice=l.reset_notice;toast(l.reset_notice);}
  const player=new URLSearchParams(location.search).get('player');
  if(player && l.rows.some(r=>String(r.id)===player)) { history.replaceState(null,'',location.pathname+location.hash);task(openLivePlayer(Number(player))); }
}
window.renderEnhancedLive=renderEnhancedLive;
$('#liveMetric').onchange=()=>{if(latestLive){renderEnhancedLive(latestLive);updateLiveRows(latestLive);}};
$('#startTraining').onclick=()=>task(api('/api/training',{method:'POST',body:JSON.stringify({seconds:Number($('#trainingDuration').value)})}).then(()=>toast('Training wartet auf den ersten Treffer.')));
$('#stopTraining').onclick=()=>task(api('/api/training',{method:'POST',body:JSON.stringify({seconds:0})}));
api('/api/training').then(t=>{lastTraining=t;if(latestLive)renderEnhancedLive(latestLive);}).catch(()=>{});

function exportPlayers(players,anonymous) {
  return players.map((p,i)=>({...p,name:anonymous&&!p.is_self?'Spieler '+(i+1):p.name,
    skills:p.skills||[],heal_skills:p.heal_skills||[]}));
}
async function copyText(text) {
  try {await navigator.clipboard.writeText(text);}catch(e) {
    const input=document.createElement('textarea');input.value=text;document.body.append(input);input.select();
    const ok=document.execCommand('copy');input.remove();if(!ok)throw new Error('Zwischenablage nicht verfügbar');
  }
  toast('Ergebnis kopiert.');
}
function rankingText(title,players,ms,metric='damage') {
  const label=metric==='heal'?'Heilung':metric==='damage_received'?'Erlittener Schaden':'Schaden';
  const rate=metric==='heal'?'HPS':metric==='damage_received'?'pro Sekunde':'DPS';
  return [title+' · '+dur(ms),...players.map((p,i)=>`${i+1}. ${p.name} | ${num(p.damage)} ${label} | ${num(p.dps)} ${rate}${metric==='damage'?' | '+num(p.heal||0)+' Heilung':''}`)].join('\n');
}
$('#copyLive').onclick=()=>{if(latestLive)task(copyText(rankingText(latestLive.target_name||'Kampf',exportPlayers(metricRows(latestLive.rows),$('#anonymousExport').checked),latestLive.battle_time_ms,metricKey())));};
function download(name,body,type) {
  const url=URL.createObjectURL(new Blob([body],{type}));const a=document.createElement('a');a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
}
// Spreadsheet formula injection is escaped in addition to RFC 4180 quoting.
function csvCell(value) {let s=String(value??'');if(/^[\s]*[=+\-@]/.test(s))s="'"+s;return '"'+s.replaceAll('"','""')+'"';}
// Export scope in the fight dialog: the whole group, or one player picked by actor id.
function exportScopeOptions(f) {
  return `<option value="">Export: ganze Gruppe</option>${f.players.map(p=>`<option value="${Number(p.actor_id)}">Export: nur ${esc(p.name)}</option>`).join('')}`;
}
function exportScopeIndex(f) {
  const v=$('#exportScope')?.value;if(!v)return null;
  const i=f.players.findIndex(p=>String(p.actor_id)===v);return i<0?null:i;
}
// Anonymize before filtering so a single exported player keeps the group numbering ("Spieler 2").
function scopedPlayers(f,players) {const i=exportScopeIndex(f);return i==null?players:[players[i]];}
function scopedExport(f,anonymous) {const exp=fightExport(f,anonymous);return {...exp,players:scopedPlayers(f,exp.players)};}
function exportSuffix(f) {const i=exportScopeIndex(f);return i==null?'':'-spieler-'+(i+1);}
function fightExport(f,anonymous) {
  // Deliberately allowlist fields: notes, network addresses and database IDs stay local.
  const players=exportPlayers(f.players,anonymous).map(p=>({name:p.name,class_name:localizedClass(p),class_key:p.class_key,damage:p.damage,dps:p.dps,heal:p.heal,hps:p.hps,damage_received:p.damage_received,
    skills:(p.skills||[]).map(s=>({...s,name:skillName(s)})),heal_skills:(p.heal_skills||[]).map(s=>({...s,name:skillName(s)})),buffs:(p.buffs||[]).map(b=>({code:b.code,name:skillName(b),names:b.names,icon:iconUrl(b),uptime:b.uptime}))}));
  return {boss:f.boss_name,difficulty:f.difficulty,started_at:f.started_at,duration_ms:f.duration_ms,healing_scope:f.healing_scope,numeric_limited:!!f.numeric_limited,players};
}

async function loadFights(reset=true) {
  if(!reset && fightSearchBusy)return;
  if($('#fightFrom').value&&$('#fightTo').value&&$('#fightFrom').value>$('#fightTo').value){toast('Das Von-Datum muss vor dem Bis-Datum liegen.',true);return;}
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
$('#clearFightFilters').onclick=()=>{$('#fightSearch').value='';$('#fightFrom').value='';$('#fightTo').value='';$('#fightFavorites').checked=false;task(loadFights());};
function invalidateDetails() {detailRequest++;comparisonRequest++;currentFight=null;}
$('#closeDialog').onclick=()=>{invalidateDetails();$('#fightDialog').close();};
$('#fightDialog').addEventListener('cancel',invalidateDetails);

function skillTable(skills,heal=false) {
  if(!skills?.length)return '<p class="muted">Keine Skilldaten vorhanden.</p>';
  const total=skills.reduce((n,s)=>n+Number(s.damage||0),0);
  const rows=skills.map(s=>({...s,name:skillName(s),share:s.share??(total>0?s.damage*100/total:null),average:s.average??(s.hits>0?s.damage/s.hits:null)}));
  const columns=[['name','Skill'],['damage',heal?'Heilung':'Schaden'],['share','Anteil'],['dps',heal?'HPS':'DPS'],['hits','Treffer / Ticks'],['average','Ø Treffer']];
  if(!heal)columns.push(['crit_rate','Krit']);
  columns.push(['min','Min'],['max','Max']);
  const extra=heal?[]:[['back_rate','Rücken'],['frontal_rate','Frontal'],['perfect_rate','Perfekt'],['double_rate','Double'],['parry_rate','Pariert'],['multi_hit_count','Multihit'],['block_rate','Block'],['perfect_block_rate','Perfektblock'],['endurance_rate','Ausdauer'],['regeneration_rate','Regeneration'],['miss_count','Verfehlt'],['resist_count','Effekt resistiert']];
  const cell=(s,key)=>key==='name'?`${skillLabel(s)}${s.is_dot?` <span class="badge">${heal?'HoT':'DoT'}</span>`:''}`:key==='share'||key.endsWith('_rate')?pct(s[key]):s[key]==null||(['min','max'].includes(key)&&s[key]<=0)?'—':num(s[key]);
  return `<div class="skill-browser" data-skills="${esc(JSON.stringify(rows))}"><p class="analysis-note">Treffer und Ticks sind keine Skill-Aktivierungen. Anteil bezieht sich auf diese Spielerliste. ${heal?'Heilung seit Parser-Reset.':'Treffermerkmale: beobachtete Anteile, vollständige Erfassung unbekannt.'} — bedeutet kein nachgewiesener Wert, nicht gemessene 0 %. Resist zählt widerstandene Effekte.</p><div class="row skill-tools"><input type="search" class="skill-search" aria-label="Skills suchen" placeholder="Skill suchen"><select class="skill-sort" aria-label="Skills sortieren"><option value="damage">${heal?'Heilung':'Schaden'} absteigend</option><option value="name">Name A–Z</option><option value="hits">Treffer / Ticks absteigend</option></select>${extra.length?'<label><input type="checkbox" class="skill-extra"> Weitere Treffermerkmale</label>':''}<span class="skill-count muted">${rows.length} Skills</span></div><div class="table-scroll"><table><thead><tr>${[...columns,...extra].map(([k,label],i)=>`<th${i>=columns.length?' class="skill-advanced"':''} aria-sort="${k==='damage'?'descending':'none'}"><button type="button" class="sort-head" data-sort="${k}" title="Nach ${esc(label)} sortieren">${label}</button></th>`).join('')}</tr></thead><tbody>${rows.map(s=>`<tr>${[...columns,...extra].map(([k],i)=>`<td${i>=columns.length?' class="skill-advanced"':''} title="${k==='name'?esc(s.name):esc(s[k]==null?'Kein nachgewiesener Wert':Number(s[k]).toLocaleString('de-DE',{maximumFractionDigits:2}))}">${cell(s,k)}</td>`).join('')}</tr>`).join('')}</tbody></table></div><p class="skill-empty muted" hidden>Keine passenden Skills.</p></div>`;
}
function bindSkillTables(root) {
  root.querySelectorAll('.skill-browser').forEach(box=>{
    const skills=JSON.parse(box.dataset.skills);delete box.dataset.skills;
    const records=[...box.querySelectorAll('tbody tr')].map((el,i)=>({el,s:skills[i]}));
    // Column headers sort too; a second click on the same column reverses it.
    const sort={key:'damage',dir:-1};
    const value=(s,key)=>s[key]==null||!Number.isFinite(Number(s[key]))?-Infinity:Number(s[key]);
    const update=()=>{
      const search=box.querySelector('.skill-search').value.trim().toLocaleLowerCase('de-DE'),{key,dir}=sort;
      const sorted=records.slice().sort((a,b)=>key==='name'?dir*a.s.name.localeCompare(b.s.name,'de'):dir*(value(a.s,key)-value(b.s,key))||a.s.name.localeCompare(b.s.name,'de'));
      box.querySelectorAll('th').forEach(th=>th.setAttribute('aria-sort',th.querySelector('.sort-head')?.dataset.sort===key?(dir>0?'ascending':'descending'):'none'));
      const body=box.querySelector('tbody');let count=0;
      for(const {el,s} of sorted){el.hidden=![s.name,s.code,...Object.values(s.names||{})].join(' ').toLocaleLowerCase('de-DE').includes(search);if(!el.hidden)count++;body.append(el);}
      box.querySelector('.skill-count').textContent=count+' / '+skills.length+' Skills';box.querySelector('.skill-empty').hidden=count!==0;
    };
    const select=box.querySelector('.skill-sort');
    box.querySelector('.skill-search').oninput=update;
    select.onchange=()=>{sort.key=select.value;sort.dir=select.value==='name'?1:-1;update();};
    box.querySelectorAll('.sort-head').forEach(b=>b.onclick=()=>{
      const key=b.dataset.sort;sort.dir=sort.key===key?-sort.dir:key==='name'?1:-1;sort.key=key;
      if([...select.options].some(o=>o.value===key))select.value=key;update();
    });
    const extra=box.querySelector('.skill-extra');if(extra)extra.onchange=()=>box.classList.toggle('show-advanced',extra.checked);
  });
}
function svgCurve(points,value,label) {
  if(!points.length)return '<p class="muted">Für diesen Kampf wurden keine Verlaufsdaten gespeichert.</p>';
  const W=900,H=180,L=55,R=15,T=12,B=28, end=points.reduce((n,p)=>Math.max(n,p.ms),1000), max=points.reduce((n,p)=>Math.max(n,value(p)),1);
  const step=Math.max(1,Math.ceil(points.length/600));points=points.filter((_,i)=>i%step===0||i===points.length-1);
  const x=ms=>L+ms/end*(W-L-R),y=v=>T+(1-v/max)*(H-T-B);
  const line=points.map((p,i)=>`${i?'L':'M'}${x(p.ms).toFixed(2)},${y(value(p)).toFixed(2)}`).join(' ');
  return `<svg class="timeline" viewBox="0 0 ${W} ${H}" role="img" aria-label="${esc(label)}"><path d="M${L},${T}V${H-B}H${W-R}" fill="none" stroke="#7898b655"/><path d="${line}" fill="none" stroke="#5ad2c8" stroke-width="2"/><text x="0" y="20">${num(max)}</text><text x="${L}" y="${H-5}">0:00</text><text x="${W-65}" y="${H-5}">${dur(end)}</text>${points.filter((_,i)=>i%Math.max(1,Math.ceil(points.length/300))===0).map(p=>`<circle cx="${x(p.ms)}" cy="${y(value(p))}" r="3" fill="transparent"><title>${dur(p.ms)}: ${num(value(p))} ${esc(label)}</title></circle>`).join('')}</svg>`;
}
function damageCurve(f,actor=null) {
  const points=f.analytics?.points||[];
  const total=p=>actor==null?Object.values(p.damage).reduce((a,b)=>a+Number(b),0):Number(p.damage[actor]||0);
  const curves=points.map((p,i)=>{const prev=points[i-1];return {ms:p.ms,dps:Math.max(0,total(p)-(prev?total(prev):0))*1000/Math.max(500,p.ms-(prev?prev.ms:0))};});
  return svgCurve(f.analytics?.partial?curves.slice(1):curves,p=>p.dps,'beobachtete Intervall-DPS');
}
function hitTimeline(skills,ms,W=900) {
  const rows=(skills||[]).filter(s=>s.hit_timestamps?.length);if(!rows.length)return '<p class="muted">Keine Trefferzeitpunkte gespeichert. Ältere Kämpfe enthalten diese Daten nicht.</p>';
  const end=Math.max(ms,1000),L=W<600?120:210,H=rows.length*25+30;
  return `<p class="analysis-note">Trefferzeitpunkte, keine Cast-Anzahl. DoT und Multihit können mehrere Treffer pro Skill-Ausführung erzeugen. Pro Skill werden höchstens 250 Marker gezeichnet.</p><svg class="timeline" viewBox="0 0 ${W} ${H}" role="img" aria-label="Skill-Trefferzeitlinie">${rows.map((s,i)=>`${iconUrl(s)?`<image href="${iconUrl(s)}" x="0" y="${i*25+2}" width="20" height="20"/>`:""}<text x="24" y="${i*25+18}">${esc(skillName(s).slice(0,28))}</text><path d="M${L},${i*25+14}H${W}" stroke="#7898b622"/>${s.hit_timestamps.filter((_,j)=>j%Math.max(1,Math.ceil(s.hit_timestamps.length/250))===0).map(t=>`<circle cx="${L+Math.min(end,Math.max(0,t))/end*(W-L)}" cy="${i*25+14}" r="3" fill="#edc57b"><title>${esc(skillName(s))} · ${(t/1000).toFixed(2)} s</title></circle>`).join('')}`).join('')}</svg>`;
}
function effectTimeline(f,actor,W=900) {
  const effects=(f.analytics?.effects||[]).filter(e=>e.target===actor);if(!effects.length)return '';
  const keys=[...new Set(effects.map(e=>e.code))],L=W<600?120:210,end=Math.max(1000,f.duration_ms);
  const names=Object.fromEntries([...(f.boss_debuffs||[]),...f.players.flatMap(p=>p.buffs||[])].map(e=>[e.code,skillName(e)]));
  return `<h4>Buff-Zeitlinie</h4><p class="analysis-note">Aus beobachteter Anwendung und gemeldeter Dauer. Vorzeitiges Entfernen wird derzeit nicht erkannt.</p><svg class="timeline" viewBox="0 0 ${W} ${keys.length*24+20}" role="img" aria-label="Buff-Zeitlinie">${keys.map((code,i)=>`<text x="0" y="${i*24+17}">${esc((names[code]||'#'+code).slice(0,28))}</text>${effects.filter(e=>e.code===code).map(e=>`<rect x="${L+e.start_ms/end*(W-L)}" y="${i*24+5}" width="${Math.max(1,(e.end_ms-e.start_ms)/end*(W-L))}" height="13" rx="3" fill="#966ee6"><title>${esc(names[code]||code)} · ${(e.start_ms/1000).toFixed(1)}–${(e.end_ms/1000).toFixed(1)} s</title></rect>`).join('')}`).join('')}</svg>`;
}
function playerReport(p,f) {
  return `<details class="player-report" data-actor="${Number(p.actor_id)}"><summary>${chip(p)} · ${num(p.dps)}/s · ${num(p.heal||0)} Heilung · ${num(p.damage_received||0)} erlitten</summary><div class="player-analysis"></div></details>`;
}
function bindPlayerReports(f) {
  $('#fightContent').querySelectorAll('.player-report').forEach(el=>el.addEventListener('toggle',()=>{
    const content=el.querySelector('.player-analysis');if(!el.open||content.childElementCount)return;
    const p=f.players.find(p=>String(p.actor_id)===el.dataset.actor);if(!p)return;
    content.innerHTML=`<h4>Schadensskills</h4>${skillTable(p.skills)}<h4>Heilungsskills</h4>${skillTable(p.heal_skills,true)}${uptimes(p.buffs,false)}<h4>Skill-Trefferzeitlinie</h4>${hitTimeline(p.skills,f.duration_ms)}${effectTimeline(f,p.actor_id)}<h4>DPS-Verlauf</h4>${damageCurve(f,p.actor_id)}`;
    bindSkillTables(content);
  }));
}
async function openLivePlayer(id) {
  comparisonRequest++;currentFight=null;
  const request=++detailRequest;const p=await api('/api/players/'+id);if(request!==detailRequest)return;
  const row=latestLive?.rows.find(r=>r.id===id);
  $('#dialogTitle').textContent=row?.name||'Spielerdetails';
  $('#fightContent').innerHTML=`<p class="analysis-note">Aktueller Stand. ${num(row?.burst_dps||0)}/s Burst über 5 Sekunden. Heilung seit Parser-Reset.</p><button class="btn" id="refreshPlayer">Aktualisieren</button><h3>Schaden</h3>${skillTable(p.skills)}<h3>Heilung</h3>${skillTable(p.heal_skills,true)}${hitTimeline(p.skills,p.duration_ms)}`;
  bindSkillTables($('#fightContent'));
  if(window.installLiveComparison)installLiveComparison(p,row);
  $('#refreshPlayer').onclick=()=>task(openLivePlayer(id));if(!$('#fightDialog').open)$('#fightDialog').showModal();
}
async function openFight(id) {
  comparisonRequest++;currentFight=null;
  const request=++detailRequest;const f=await api('/api/fights/'+encodeURIComponent(id));if(request!==detailRequest)return;
  currentFight=f;$('#dialogTitle').textContent=(f.boss_name||'Kampf')+' · '+(f.difficulty||'');
  $('#fightContent').innerHTML=`<p>${date(f.started_at)} · ${dur(f.duration_ms)} · ${num(f.total_damage)} Schaden</p>
    <p class="analysis-note">${f.numeric_limited?'Parser-Zahlengrenze erreicht; einzelne Skillwerte können begrenzt sein. <br>':''}${f.analytics?.effects_partial?'Effektdaten wegen Speichergrenzen unvollständig. <br>':''}${esc(f.healing_scope||'Erfasste Heilung. Keine Aussage über Overheal.')}<br>${f.analytics?`DPS-Verlauf: Beobachtung alle ${f.analytics.resolution_ms||500} ms${f.analytics.partial?' · unvollständige Daten':''}.`:'Keine zeitliche Schadensaufzeichnung vorhanden.'} Vollständigkeit vor Erfassungsbeginn unbekannt. Ergebnis: ${f.analytics?.outcome==='kill'?'Tod des Ziels erfasst':f.analytics?.outcome==='wipe'?'Wipe mit HP-Reset erkannt':'unbekannt'}${f.analytics?.end_reason?' · Abschluss: '+esc({manual:'manueller Reset',idle:'Leerlauf',wipe:'Wipe'}[f.analytics.end_reason]||f.analytics.end_reason):''}.</p>
    <div class="row fight-tools"><select id="exportScope" aria-label="Export für">${exportScopeOptions(f)}</select><button class="btn" id="copyFight">Kopieren</button><button class="btn" id="jsonFight">JSON</button><button class="btn" id="csvFight">CSV</button><button class="btn" id="pngFight">PNG-Bericht</button><button class="btn" id="chatFight">Chatzeile</button><label><input type="checkbox" id="anonFight" checked> Andere Namen anonymisieren</label></div>
    <div class="row fight-tools"><label><input type="checkbox" id="favoriteFight" ${f.favorite?'checked':''}> Favorit</label><input id="fightNote" aria-label="Kampfnotiz" placeholder="Notiz" maxlength="4000" value="${esc(f.note||'')}"><input id="fightTags" aria-label="Kampf-Tags" placeholder="Tags, z. B. neues Gear" maxlength="500" value="${esc(f.tags||'')}"><button class="btn" id="saveFightNote">Speichern</button></div>
    <h3>Direkter Kampfvergleich</h3><div class="row"><select id="compareFight" aria-label="Vergleichskampf"><option value="">Vergleich laden …</option></select><button class="btn" id="compareBtn">Vergleichen</button></div><p class="analysis-note">Gleicher Boss und Schwierigkeitsgrad. Eigene Werte werden nur bei gleichem Charakter und gleicher Klasse verglichen.</p><div id="comparison"></div>
    <h3>Gruppen-DPS im Kampfverlauf</h3>${damageCurve(f)}<h3>Ping-Verlauf</h3>${svgCurve((f.ping_history||[]).map(p=>({ms:p.tsMs,ping:p.pingMs})),p=>p.ping,'ms Ping')}
    ${f.players.map(p=>playerReport(p,f)).join('')}${effectTimeline(f,f.target_id)}${uptimes(f.boss_debuffs,true)}`;
  bindPlayerReports(f);
  if(!$('#fightDialog').open)$('#fightDialog').showModal();
  $('#saveFightNote').onclick=()=>task(api('/api/fights/'+encodeURIComponent(id)+'/annotation',{method:'POST',body:JSON.stringify({favorite:$('#favoriteFight').checked,note:$('#fightNote').value,tags:$('#fightTags').value})}).then(()=>{toast('Kampfnotiz gespeichert.');if(tab==='runs')task(loadFights());}));
  $('#copyFight').onclick=()=>task(copyText(rankingText(f.boss_name,scopedPlayers(f,exportPlayers(f.players,$('#anonFight').checked)),f.duration_ms)));
  $('#jsonFight').onclick=()=>download(`aion2-kampf${exportSuffix(f)}.json`,JSON.stringify(scopedExport(f,$('#anonFight').checked),null,2),'application/json');
  $('#csvFight').onclick=()=>{const exp=scopedExport(f,$('#anonFight').checked),rows=[['Spieler','Klasse','Schaden','DPS','Heilung','HPS','Erlittener Schaden'],...exp.players.map(p=>[p.name,p.class_name,p.damage,p.dps,p.heal,p.hps,p.damage_received])];
    // A single-player export also lists that player's skills below the summary row.
    if(exportScopeIndex(f)!=null){const p=exp.players[0];rows.push([],['Skill','Art','Wert','Treffer/Ticks','Krit %','Min','Max']);for(const [kind,list] of [['Schaden',p.skills],['Heilung',p.heal_skills]])for(const sk of list||[])rows.push([sk.name,kind,sk.damage,sk.hits,sk.crit_rate,sk.min,sk.max]);}
    download(`aion2-kampf${exportSuffix(f)}.csv`,'\uFEFF'+rows.map(r=>r.map(csvCell).join(';')).join('\r\n'),'text/csv;charset=utf-8');};
  if(window.installFightQol)installFightQol(f);
  $('#compareBtn').onclick=()=>task(compareFight());
  $('#compareFight').onchange=()=>{comparisonRequest++;$('#comparison').textContent='';};
  try {const data=await api('/api/fights?query='+encodeURIComponent(f.boss_name||'')+'&character='+encodeURIComponent(f.players.find(p=>p.is_self)?.name||''));if(request!==detailRequest)return;
    const candidates=(data.fights||[]).filter(c=>c.id!==id&&c.boss_name===f.boss_name&&c.dungeon_id===f.dungeon_id);
    $('#compareFight').innerHTML='<option value="">Vergleichskampf wählen</option>'+candidates.map(c=>`<option value="${esc(c.id)}">${date(c.started_at)} · ${dur(c.duration_ms)} · ${num(c.my_dps)}/s</option>`).join('');
  }catch(e){if(request===detailRequest)$('#compareFight').innerHTML='<option value="">Vergleiche nicht erreichbar</option>';}
}
window.openFight=openFight;
function delta(a,b){const d=(Number(a)||0)-(Number(b)||0);return `<span class="${d>=0?'compare-positive':'compare-negative'}">${d>=0?'+':''}${num(d)}${b?' ('+(d/b*100).toFixed(1)+'%)':''}</span>`;}
async function compareFight() {
  const f=currentFight;if(!f)return;
  const id=$('#compareFight').value;if(!id){toast('Vergleichskampf wählen.');return;}
  const detail=detailRequest,request=++comparisonRequest;
  $('#comparison').textContent='Vergleich wird geladen …';
  const old=await api('/api/fights/'+encodeURIComponent(id));if(detail!==detailRequest||request!==comparisonRequest)return;
  const me=f.players.find(p=>p.is_self),before=old.players.find(p=>p.is_self);
  if(old.boss_name!==f.boss_name||old.dungeon_id!==f.dungeon_id||!me||!before||me.name!==before.name||me.job!==before.job){$('#comparison').textContent='Dieser Vergleich passt nicht zu Boss, Schwierigkeit, Charakter oder Klasse.';return;}
  const skillKey=s=>String(s.code)+'|'+Boolean(s.is_dot);
  const keys=[...new Set([...(me.skills||[]),...(before.skills||[])].map(skillKey))];
  $('#comparison').innerHTML=`<p>Aktueller Kampf gegenüber ${date(old.started_at)}: DPS ${delta(me.dps,before.dps)} · Dauer ${delta(f.duration_ms/1000,old.duration_ms/1000)} Sekunden</p><div class="table-scroll"><table><thead><tr><th>Skill</th><th>Schaden jetzt</th><th>Schaden zuvor</th><th>Differenz</th><th>Krit jetzt / zuvor</th></tr></thead><tbody>${keys.map(k=>{const a=(me.skills||[]).find(s=>skillKey(s)===k),b=(before.skills||[]).find(s=>skillKey(s)===k);return `<tr><td>${skillLabel(a||b||{})}${(a||b)?.is_dot?' · DoT':''}</td><td>${num(a?.damage)}</td><td>${num(b?.damage)}</td><td>${delta(a?.damage,b?.damage)}</td><td>${pct(a?.crit_rate)} / ${pct(b?.crit_rate)}</td></tr>`;}).join('')}</tbody></table></div><h4>Buff-Uptime jetzt / zuvor</h4>${[...new Set([...(me.buffs||[]),...(before.buffs||[])].map(b=>b.code))].map(k=>{const a=me.buffs?.find(b=>b.code===k),b=before.buffs?.find(b=>b.code===k);return `<p>${skillLabel(a||b||{})}: ${pct(a?.uptime)} / ${pct(b?.uptime)}</p>`;}).join('')}`;
}
async function overlayAction(action, message) {
  if(overlayActionBusy)return;
  overlayActionBusy=true;settingsRequest++;
  const controls=[...document.querySelectorAll('#settings input,#settings select,#settings button')].map(el=>[el,el.disabled]);
  controls.forEach(([el])=>el.disabled=true);
  try {await saveSettings();await action();await loadSettings();toast(message);}
  finally {controls.forEach(([el,disabled])=>el.disabled=disabled);overlayActionBusy=false;}
}
const profileKey=()=>$('#profileName').value.trim()||latestLive?.character||'Standard';
$('#saveProfile').onclick=()=>task(overlayAction(()=>api('/api/overlay/profile',{method:'POST',body:JSON.stringify({key:profileKey(),save:true})}),'Profil gespeichert.'));
$('#loadProfile').onclick=()=>task(overlayAction(()=>api('/api/overlay/profile',{method:'POST',body:JSON.stringify({key:profileKey(),save:false})}),'Profil geladen.'));
$('#recoverOverlay').onclick=()=>task(overlayAction(()=>api('/api/overlay',{method:'POST',body:JSON.stringify({position:[40,40],visible:true,locked:false})}),'Overlay auf Startposition zurückgeholt.'));
if(tab==='runs')task(loadFights());
