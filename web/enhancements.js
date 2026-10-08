// Local combat analysis and QoL. No upload service or third-party assets.
// Insights use stored observations, never infer casts, effective healing or kill success.
function observedPeak(f,actor=null) {
  if(f.numeric_limited)return null;
  const points=f.analytics?.points||[],resolution=Math.max(1,Number(f.analytics?.resolution_ms)||500);
  if(!points.length)return null;
  const total=p=>actor==null?Object.values(p.damage||{}).reduce((n,v)=>n+Number(v),0):Number(p.damage?.[actor]||0);
  const samples=f.analytics.partial?points:[{ms:0,damage:{}},...points];
  let base=0,validFrom=0,best=null;
  for(let i=1;i<samples.length;i++) {
    const p=samples[i],prev=samples[i-1];
    if(!Number.isFinite(total(p))||!Number.isFinite(total(prev))||p.ms<=prev.ms||p.ms-prev.ms>resolution*3||total(p)<total(prev))validFrom=i;
    base=Math.max(base,validFrom);
    while(base+1<i&&samples[base+1].ms<=p.ms-5000)base++;
    const start=samples[base],span=p.ms-start.ms;
    if(span<5000||span>5000+resolution||base<validFrom)continue;
    const dps=(total(p)-total(start))*1000/span;
    if(Number.isFinite(dps)&&dps>0&&(!best||dps>best.dps))best={dps,start:start.ms,end:p.ms,partial:!!f.analytics.partial};
  }
  return best;
}
function windowHits(f,actor,start,end) {
  const players=(f.players||[]).filter(p=>actor==null||String(p.actor_id)===String(actor));
  const hits=[];let available=false;
  for(const p of players)for(const s of p.skills||[]) {
    if(!Array.isArray(s.hit_timestamps))continue;
    available=true;const count=s.hit_timestamps.reduce((n,t)=>n+(t>start&&t<=end?1:0),0);
    if(count)hits.push({player:p.name,skill:s,count});
  }
  return {available,hits:hits.sort((a,b)=>b.count-a.count)};
}
function knownClassJob(job) {
  return ['검성','수호성','궁성','살성','마도성','치유성','정령성','호법성','권성','gladiator','templar','ranger','assassin','sorcerer','cleric','elementalist','chanter','fighter'].includes(job);
}
function bossPerformance(attempts,characterSelected) {
  if(!characterSelected)return {hint:'Für persönliche Einordnung oben einen Charakter wählen.'};
  if(attempts.length<3)return {hint:'Mindestens drei vergleichbare Versuche für eine persönliche Einordnung nötig.'};
  const last=attempts.at(-1);
  if(!knownClassJob(last.job)||attempts.some(p=>p.job!==last.job||!Number.isFinite(p.dps)||p.dps<0||![false,0].includes(p.numeric_limited)))return {hint:'Die Einordnung braucht verlässliche Werte derselben Klasse im ausgewählten Ausschnitt.'};
  const previous=attempts.slice(0,-1),mean=previous.reduce((n,p)=>n+p.dps,0)/previous.length;
  const allMean=attempts.reduce((n,p)=>n+p.dps,0)/attempts.length,best=Math.max(...attempts.map(p=>p.dps));
  const rank=1+attempts.filter(p=>p.dps>last.dps).length;
  return {count:attempts.length,rank,previousMean:mean,gap:best>0?(best-last.dps)*100/best:null,change:mean>0?(last.dps-mean)*100/mean:null,
    deviation:allMean>0?attempts.reduce((n,p)=>n+Math.abs(p.dps-allMean),0)/attempts.length/allMean*100:null};
}
// One summary row: the plain values, and the comparison only when it is reliable.
function bossStory(attempts,characterSelected) {
  const v=bossPerformance(attempts,characterSelected),percent=n=>Math.abs(n).toFixed(1).replace('.',',')+' %';
  const last=attempts.at(-1),best=Math.max(...attempts.map(p=>p.dps)),mean=attempts.reduce((n,p)=>n+p.dps,0)/attempts.length;
  const cell=(value,label,cls='')=>`<div${cls?` class="${cls}"`:''}><strong>${value}</strong><span>${label}</span></div>`;
  if(v.hint)return {hint:v.hint,summary:cell(num(last.dps)+' DPS','Letzter Versuch')+cell(num(best)+' DPS','Bestwert im Ausschnitt')+cell(num(mean)+' DPS','Ø pro Versuch')+cell(attempts.length,'Versuche im Ausschnitt')};
  // Direction answers "am I improving?": the last three against the three before, nothing inferred beyond that.
  let verdict='';
  if(attempts.length>=6){
    const mean=list=>list.reduce((n,p)=>n+p.dps,0)/list.length,recent=mean(attempts.slice(-3)),before=mean(attempts.slice(-6,-3)),d=before>0?(recent-before)*100/before:null;
    if(d!=null){const dir=Math.abs(d)<2?'flat':d>0?'up':'down';
      verdict=`<div class="trend-verdict ${dir}"><strong>${dir==='up'?'▲ Aufwärtstrend':dir==='down'?'▼ Abwärtstrend':'■ Stabil'}</strong><span>Ø der letzten 3 Versuche <b>${num(recent)} DPS</b> · ${signedPct(d)} gegenüber den 3 davor (${num(before)} DPS)</span></div>`;}
  }
  return {hint:'',verdict,summary:cell(num(last.dps)+' DPS',`Letzter Versuch · Rang ${v.rank} von ${v.count}`,'trend-lead')
    +cell(v.change==null?'—':`<b class="${v.change>=0?'compare-positive':'compare-negative'}">${v.change>=0?'+':'−'}${percent(v.change)}</b>`,`zum Ø der ${v.count-1} vorherigen Versuche (${num(v.previousMean)} DPS)`)
    +cell(num(best)+' DPS','Bestwert im Ausschnitt · '+(v.gap?`letzter ${percent(v.gap)} darunter`:'letzter Versuch'))
    +cell(num(mean)+' DPS',`Ø aller ${v.count} Versuche · Schwankung ±${v.deviation==null?'—':percent(v.deviation)}`)};
}
// m:ss with tenths only where the observation grid needs them (500 ms raster).
function clock(ms) {
  const t=Math.max(0,Number(ms)||0),m=Math.floor(t/60000),sec=(t%60000)/1000;
  return m+':'+(sec<10?'0':'')+(Number.isInteger(sec)?String(sec):sec.toFixed(1).replace('.',','));
}
// Own 5s curve as a compact line with the peak window marked; a button opens it in the full chart.
function peakSparkline(f,actor,peak) {
  const samples=damageSamples(f,actor,5000);if(samples.length<2)return '';
  const W=220,H=44,end=Math.max(1000,f.duration_ms||0,samples.at(-1).ms),max=Math.max(1,...samples.map(p=>p.dps));
  const x=ms=>(ms/end*W).toFixed(1),y=v=>(H-3-v/max*(H-8)).toFixed(1);
  const step=Math.max(1,Math.ceil(samples.length/220)),pts=samples.filter((_,i)=>i%step===0||i===samples.length-1);
  return `<svg class="peak-spark" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" aria-hidden="true"><rect class="peak-band" x="${x(peak.start)}" y="0" width="${Math.max(2,(peak.end-peak.start)/end*W).toFixed(1)}" height="${H}"/><path class="peak-line" d="M${pts.map(p=>x(p.ms)+','+y(p.dps)).join(' L')}"/></svg>`;
}
function fightStory(f) {
  if(f.numeric_limited)return '';
  const me=f.players.find(p=>p.is_self),actor=me?.actor_id??null,peak=observedPeak(f,actor);
  const top=me?.skills?.reduce((a,b)=>Number(b.damage)>Number(a?.damage||0)?b:a,null);
  const rank=me?1+f.players.filter(p=>Number(p.damage)>Number(me.damage)).length:0;
  const parts=[];
  if(me&&!f.is_train)parts.push('<div class="story-compare" data-story-compare hidden></div>');
  if(peak)parts.push(`<div class="story-peak"><span class="eyebrow">Peak · 5 s</span><strong><button type="button" class="story-link" data-story-peak title="Dieses Fenster im Schadensverlauf anzeigen">${num(peak.dps)} DPS ↗</button></strong>${me?peakSparkline(f,actor,peak):''}<span>Stärkstes beobachtetes 5s-Fenster${peak.partial?' im Ausschnitt':''} · ${clock(peak.start)}–${clock(peak.end)}</span></div>`);
  if(top&&me.damage>0&&top.damage>0&&top.damage<=me.damage){const part=top.damage*100/me.damage;parts.push(`<div class="story-skill"><span class="eyebrow">Top-Skill</span><strong>${skillLabel(top)}</strong><i class="story-meter"><b style="width:${part.toFixed(1)}%"></b></i><span>${pct(part)} deines Schadens${top.is_dot?' · DoT':''}</span></div>`);}
  if(!parts.length)return '';
  // The lead cell carries your own result, so the story starts with data instead of a label.
  const lead=me?`<strong>${num(me.dps)} DPS</strong><small><b class="story-rank">Rang ${rank}</b> von ${f.players.length}${me.share!=null?' · '+pct(me.share)+' Anteil':''}</small>`:`<strong>Gruppenleistung</strong><small>Kein eigener Charakter in diesem Kampf erfasst</small>`;
  return `<div class="performance-story fight-story" aria-label="Kampfzusammenfassung"><div class="story-intro"><span class="eyebrow">Kampf im Fokus</span>${lead}</div>${parts.join('')}</div>`;
}
// Personal context for one stored fight: earlier attempts on the same boss and
// difficulty, same character and class, without training or parser number limits.
const historyCache=new Map();
async function attemptContext(f) {
  const me=f.players?.find(p=>p.is_self);
  if(!me||f.is_train||f.numeric_limited||!knownClassJob(me.job))return null;
  const cached=historyCache.get(me.name);
  const history=cached&&Date.now()-cached.at<30000?cached.data:await api('/api/stats/boss-history?character='+encodeURIComponent(me.name));
  historyCache.set(me.name,{at:cached&&Date.now()-cached.at<30000?cached.at:Date.now(),data:history});
  const boss=(history||[]).find(b=>b.boss===f.boss_name&&Number(b.dungeon_id??0)===Number(f.dungeon_id??0));
  const index=boss?.attempts.findIndex(a=>a.fight_id===f.id)??-1;if(index<1)return null;
  const comparable=a=>a.job===me.job&&[false,0].includes(a.numeric_limited)&&Number.isFinite(Number(a.dps))&&Number(a.dps)>0;
  const current=boss.attempts[index];if(!comparable(current))return null;
  const previous=boss.attempts.slice(0,index).filter(comparable);if(!previous.length)return null;
  const dps=Number(me.dps)>0?Number(me.dps):Number(current.dps),mean=previous.reduce((n,a)=>n+Number(a.dps),0)/previous.length,best=Math.max(...previous.map(a=>Number(a.dps))),last=Number(previous.at(-1).dps);
  return {dps,count:previous.length,mean,best,last,vsMean:(dps-mean)*100/mean,vsLast:(dps-last)*100/last,record:dps>best,gap:(best-dps)*100/best};
}
function signedPct(v){return (v>=0?'+':'−')+Math.abs(v).toFixed(1).replace('.',',')+' %';}
function compareMarkup(c) {
  const trend=c.vsMean>=0?'up':'down';
  const lead=c.count>=2?`<strong class="trend-${trend}">${trend==='up'?'▲':'▼'} ${signedPct(c.vsMean)}</strong><span>zum Ø der ${c.count} vorherigen Versuche (${num(c.mean)} DPS)</span>`
    :`<strong class="trend-${c.vsLast>=0?'up':'down'}">${c.vsLast>=0?'▲':'▼'} ${signedPct(c.vsLast)}</strong><span>zum vorherigen Versuch (${num(c.last)} DPS)</span>`;
  const detail=c.record?'<em class="story-record">Neuer Bestwert</em> gegenüber allen vorherigen Versuchen':`${signedPct(-c.gap)} zum Bestwert ${num(c.best)}`+(c.count>=2?` · ${signedPct(c.vsLast)} zum letzten Versuch`:'');
  return `<span class="eyebrow">Dein Verlauf</span>${lead}<span class="story-detail">${detail}</span>`;
}
function fillAttemptContext(root,f) {
  const cell=root?.querySelector('[data-story-compare]');if(!cell)return;
  attemptContext(f).then(c=>{if(!c||!cell.isConnected)return;cell.innerHTML=compareMarkup(c);cell.hidden=false;}).catch(()=>{});
}

// Browser-local signal only: bounded, keyed to actor AND encounter, no inferred fight end.
function appendLiveSignal(state,l) {
  const me=l.rows.find(r=>r.is_self),key=JSON.stringify([l.target_id,l.target_started_at,me?.id]),ms=Number(l.battle_time_ms);
  if(state.key!==key||ms<(state.points.at(-1)?.ms??0)){state.key=key;state.points=[];}
  if(!me||l.numeric_limited||!Number.isFinite(ms)||ms<=0){state.points=[];return state.points;}
  const value=Number(me.burst_dps),last=state.points.at(-1);
  if(!Number.isFinite(value)||value<0)return state.points;
  if(!last||ms>last.ms)state.points.push({ms,value});
  state.points=state.points.filter(p=>p.ms>=ms-60000).slice(-120);
  return state.points;
}

const FIGHT_PAGE = 10;
let detailRequest = 0;
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
  const me=rows.find(r=>r.is_self);
  $('#selfRank').textContent=me?'#'+(rows.indexOf(me)+1):'—';
  $('#selfContext').textContent=me?pct(me.share)+' Anteil · '+rows.length+' Spieler':'Warte auf eigene Kampfdaten';
  $('#selfBurst').hidden=metric!=='damage'||l.numeric_limited;
  // One ribbon of everyone's share: your part of the group at a glance.
  const ribbon=rows.filter(r=>r.share>0).map(r=>`<i class="${r.is_self?'me':''}" style="width:${r.share.toFixed(2)}%;background:${color(r.class_key)}" title="${esc(r.name)} · ${pct(r.share)}"></i>`).join('');
  if($('#shareRibbon').innerHTML!==ribbon)$('#shareRibbon').innerHTML=ribbon;
  $('#shareRibbon').setAttribute('aria-label',rows.length?'Anteile: '+rows.map(r=>r.name+' '+pct(r.share)).join(', '):'Noch keine Anteile');
  // Distance to the neighbour above (or lead over the one below), from the same rates.
  const i=me?rows.indexOf(me):-1,unit=metric==='heal'?'HPS':'/s';
  $('#selfGap').textContent=i>0?`${num(rows[i-1].dps-me.dps)}${unit} hinter #${i}`:i===0&&rows.length>1?`${num(me.dps-rows[1].dps)}${unit} vor #2`:'';
}
const liveRanks={key:null,positions:new Map()};
function updateLiveRows(l) {
  const box=$('#liveRows'),focused=document.activeElement?.closest('#liveRows [data-player-id]')?.dataset.playerId;
  const rows=metricRows(l.rows),top=Math.max(1,...rows.map(r=>r.damage));
  const unit=metricKey()==='heal'?'HPS':metricKey()==='damage_received'?'erlitten/s':'DPS';
  const rankKey=JSON.stringify([l.target_id,l.target_started_at,metricKey()]);if(liveRanks.key!==rankKey){liveRanks.key=rankKey;liveRanks.positions.clear();}
  const changed=new Set(rows.filter((r,i)=>liveRanks.positions.has(r.id)&&liveRanks.positions.get(r.id)!==i).map(r=>r.id));
  liveRanks.positions=new Map(rows.map((r,i)=>[r.id,i]));
  const html=rows.length?rows.map((r,i)=>`<div class="bar telemetry-row ${r.is_self?'me':''}${r.dead||r.died?' dead':''}" style="--class-color:${color(r.class_key)}">
    <span class="rank-badge ${changed.has(r.id)?'rank-change':''}">${String(i+1).padStart(2,'0')}</span><div class="ranking-identity">${classIcon(r)}<div><b title="${esc(r.name)}">${deathMark(r)}${esc(r.name)}</b><small>${esc(localizedClass(r))}${r.is_self?' · <em>DU</em>':''}</small></div></div>
    <div class="ranking-track"><div class="fill" style="width:${(r.damage/top*100).toFixed(1)}%;background:${color(r.class_key)}"></div></div>
    <div class="ranking-value"><strong>${num(r.dps)}<small> ${unit}</small></strong><span>${num(r.damage)} gesamt</span></div><span class="ranking-share">${pct(r.share)}</span></div>`).join(''):
    '<div class="empty"><b>Bereit für den nächsten Kampf</b><p>Deine Gruppe erscheint mit Kampfdaten. Für normale Gegner „Höchster Schaden“, zum Üben „Trainingspuppe“ wählen.</p></div>';
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
const liveSignalState={key:null,points:[]};
function renderLiveSignal(l) {
  const points=appendLiveSignal(liveSignalState,l),root=$('#liveSignal');
  $('#livePhase').textContent=l.numeric_limited?'Zahlengrenze erkannt':l.rows.length?'Erfasste Kampfdaten':'Warte auf Daten';
  $('#liveHpValue').textContent=l.target_hp==null?'HP unbekannt':(l.hp_estimated?'~':'')+pct(l.target_hp*100)+' HP';
  const damage=metricKey()==='damage';root.parentElement.hidden=!damage;$('.performance-hero').classList.toggle('without-signal',!damage);
  if(!damage)return;
  if(points.length<2){root.setAttribute('aria-label','Noch keine zusammenhängenden lokalen Burst-Beobachtungen.');root.innerHTML='<span class="signal-empty">Verlauf entsteht mit neuen Live-Beobachtungen.</span>';return;}
  const end=points.at(-1).ms,start=Math.max(0,end-60000),observedMax=Math.max(0,...points.map(p=>p.value)),max=Math.max(1,observedMax);
  // A missing polling interval is a visible break, never a connecting fabricated trend.
  const segments=[];let segment=[];
  for(const p of points){if(segment.length&&p.ms-segment.at(-1).ms>3000){segments.push(segment);segment=[];}segment.push(p);}segments.push(segment);
  root.setAttribute('aria-label',`Eigener beobachteter Burst von ${dur(start)} bis ${dur(end)}, höchster empfangener Wert ${num(observedMax)} pro Sekunde. Nur lokale Live-Beobachtungen.`);
  const X=p=>(p.ms-start)/Math.max(1,end-start)*600,Y=v=>108-v/max*88;
  const paths=segments.filter(v=>v.length>1).map(v=>'M'+v.map(p=>`${X(p).toFixed(1)},${Y(p.value).toFixed(1)}`).join(' L'));
  const areas=segments.filter(v=>v.length>1).map(v=>`M${X(v[0]).toFixed(1)},108 L`+v.map(p=>`${X(p).toFixed(1)},${Y(p.value).toFixed(1)}`).join(' L')+` L${X(v.at(-1)).toFixed(1)},108Z`);
  // Markers: highest received value and the current one; the dashed line is your fight-long rate.
  const peak=points.reduce((a,b)=>b.value>a.value?b:a),last=points.at(-1),avg=Number(l.rows.find(r=>r.is_self)?.dps);
  const left=p=>(X(p)/6).toFixed(2)+'%',top=v=>(Y(v)/130*100).toFixed(2)+'%';
  const avgLine=Number.isFinite(avg)&&avg>0&&avg<=max*1.15?`<path class="signal-average" d="M0 ${Math.max(10,Y(Math.min(avg,max))).toFixed(1)}H600"/>`:'';
  root.innerHTML=`<svg viewBox="0 0 600 130" aria-hidden="true" preserveAspectRatio="none"><defs><linearGradient id="signalFill" x1="0" x2="0" y1="0" y2="1"><stop offset="0" class="signal-stop-top"/><stop offset="1" class="signal-stop-bottom"/></linearGradient></defs><path class="signal-grid" d="M0 20H600 M0 64H600 M0 108H600"/>${areas.map(d=>`<path class="signal-area" d="${d}"/>`).join('')}${avgLine}${paths.map(d=>`<path class="signal-line" d="${d}"/>`).join('')}</svg>`
    +(peak.value>0?`<span class="signal-mark peak" style="left:${left(peak)};top:${top(peak.value)}"><b>${num(peak.value)}</b> ${dur(peak.ms)}</span>`:'')
    +`<span class="signal-dot" style="left:${left(last)};top:${top(last.value)}"></span>`
    +(avgLine?`<span class="signal-avg-label" style="top:${top(Math.min(avg,max))}">Ø ${num(avg)}</span>`:'')
    +`<div class="signal-scale"><span>${dur(start)}</span><span>${num(observedMax)}/s beobachtet</span><span>${dur(end)}</span></div>`;
}

let previousEncounter=null,postFightRequest=0;
function updateSavedMoment(l) {
  const me=l.rows.find(r=>r.is_self),next={id:l.target_id,start:l.target_started_at,name:l.target_name,actor:me?.id,character:me?.name??l.character,ms:l.battle_time_ms};
  const old=previousEncounter;previousEncounter=next;
  if(!old||JSON.stringify([old.id,old.start,old.character])===JSON.stringify([next.id,next.start,next.character]))return;
  const request=++postFightRequest,root=$('#postFight');
  if(old.character!==next.character&&next.character){root.hidden=true;root.replaceChildren();return;}
  if(!old.start||!old.actor||old.ms<=0)return;
  const id='auto_'+old.id+'_'+old.start;
  api('/api/fights/'+encodeURIComponent(id)).then(f=>{
    if(request!==postFightRequest||!f||f.id!==id||f.started_at!==old.start||f.boss_name!==old.name)return;
    const player=f.players?.find(p=>p.is_self&&p.actor_id===old.actor&&p.name===old.character);if(!player)return;
    // Stored attempt, not inferred victory or combat-end notification.
    root.innerHTML=`<div class="saved-moment-head"><div><span class="eyebrow">LETZTER GESPEICHERTER VERSUCH</span><strong>${esc(f.boss_name)}<span>${[f.difficulty,dur(f.duration_ms),f.analytics?.outcome==='kill'?'Tod des Ziels erfasst':''].filter(Boolean).map(esc).join(' · ')}</span></strong></div><button class="btn" data-open-saved>Bericht öffnen</button><button class="btn" data-dismiss-saved aria-label="Zusammenfassung ausblenden">×</button></div>${fightStory(f)}`;
    root.hidden=false;fillAttemptContext(root,f);
    root.querySelector('[data-open-saved]').onclick=()=>task(openFight(id));
    root.querySelector('[data-dismiss-saved]').onclick=()=>{root.hidden=true;};
    const peak=root.querySelector('[data-story-peak]'),spark=root.querySelector('.peak-spark');if(peak&&spark)spark.onclick=()=>peak.click();if(peak)peak.onclick=()=>task(openFight(id).then(()=>$('#fightContent [data-story-peak]')?.click()));
  }).catch(()=>{});
}

let trainingRecordKey=null;
function renderTraining(l) {
  const t=l.training?.state&&l.training.state!=='idle'?l.training:lastTraining,root=$('#trainingResult');
  if(!t||t.state==='idle'){root.innerHTML='<p class="training-ready">Wähle ein Zeitfenster.<br>Der erste Treffer startet die Messung.</p>';return;}
  const state={armed:'Warte auf ersten Treffer',running:'Training läuft',interrupted:'Training unterbrochen',finished:t.personal_best&&!l.numeric_limited?'Neuer persönlicher Bestwert':'Training abgeschlossen'}[t.state]||'Training';
  const me=t.state==='running'?l.rows.find(r=>r.is_self):t.rows?.find(r=>r.is_self);
  const rate=t.state==='running'&&me?Number(me.damage)*1000/Math.max(t.elapsed_ms||0,1000):me?.dps;
  const progress=Math.min(100,Math.max(0,Number(t.elapsed_ms||0)/Math.max(1,t.seconds*1000)*100));
  const result=['running','finished'].includes(t.state)&&Number.isFinite(rate);
  let html=`<span class="training-state">${state}</span>`;
  if(t.elapsed_ms!=null)html+=`<div class="training-clock"><strong>${dur(t.elapsed_ms)}</strong><span>/ ${dur(t.seconds*1000)}</span></div><div class="training-progress" role="progressbar" aria-label="Trainingsfortschritt" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${Math.round(progress)}"><i style="width:${progress}%"></i></div>`;
  if(result)html+=`<div class="training-rate"><strong>${num(rate)}</strong><span>ERFASSTE DPS</span></div>`;
  if(t.best_dps!=null&&Number.isFinite(Number(t.best_dps)))html+=`<div class="training-best"><span>Bestwert · ${esc(t.character||'')} · ${esc(t.target||'')} · ${dur(t.seconds*1000)}</span><strong>${num(t.best_dps)}/s</strong></div>`;
  if(l.numeric_limited)html+='<p class="analysis-note">Parser-Zahlengrenze: Ergebnis nicht als exakte Referenz verwenden.</p>';
  if(t.state==='interrupted')html+='<p class="analysis-note">Zielwechsel, Reset oder Verbindung beendet. Kein abgeschlossenes Ergebnis.</p>';
  if(t.rows?.length)html+=`<details><summary>Gruppenergebnis</summary>${t.rows.map(r=>`<div>${esc(r.name)}: <b>${num(r.dps)}/s</b> · ${num(r.damage)} Schaden</div>`).join('')}</details>`;
  if(root.innerHTML!==html)root.innerHTML=html;
  const key=JSON.stringify([t.started_at,t.character,t.target,t.seconds]);
  if(t.state==='finished'&&t.personal_best&&!l.numeric_limited&&trainingRecordKey!==key){trainingRecordKey=key;root.classList.add('record-highlight');setTimeout(()=>root.classList.remove('record-highlight'),600);}
}

function renderEnhancedLive(l) {
  latestLive=l;captureHelp(l.capture);renderLiveMetrics(l);renderLiveSignal(l);updateSavedMoment(l);
  $('#numericWarning').hidden=!l.numeric_limited;
  $(".ranking-head span").textContent=metricKey()==="heal"?"Heilung · HPS · Anteil":metricKey()==="damage_received"?"Erlittener Schaden · pro Sekunde · Anteil":"Schaden · DPS · Anteil";
  $("#selfBurst").textContent="5s Burst: "+num(l.rows.find(r=>r.is_self)?.burst_dps||0)+"/s";
  $('#metricHint').textContent=metricKey()==='heal' ? 'Heilung seit Parser-Reset. HPS nutzt die angezeigte Kampfdauer. Overheal wird nicht abgezogen.' : metricKey()==='damage_received' ? 'Erlittener Schaden aus erfassten NPC-Treffern.' : 'Spieler anklicken für Skilldetails. Burst-DPS: gleitende 5 Sekunden, Beobachtung alle 500 ms.';
  if(window.applyAppearance)applyAppearance(l.overlay);
  renderTraining(l);
  if(l.reset_notice && l.reset_notice!==window.lastResetNotice){window.lastResetNotice=l.reset_notice;toast(l.reset_notice);}
  const player=new URLSearchParams(location.search).get('player');
  if(player && l.rows.some(r=>String(r.id)===player)) { history.replaceState(null,'',location.pathname+location.hash);task(openLivePlayer(Number(player))); }
}
window.renderEnhancedLive=renderEnhancedLive;
$('#liveMetric').onchange=()=>{if(latestLive){renderEnhancedLive(latestLive);updateLiveRows(latestLive);}};
$('#startTraining').onclick=()=>task(api('/api/training',{method:'POST',body:JSON.stringify({seconds:Number($('#trainingDuration input:checked').value)})}).then(()=>toast('Training wartet auf den ersten Treffer.')));
$('#stopTraining').onclick=()=>task(api('/api/training',{method:'POST',body:JSON.stringify({seconds:0})}));
api('/api/training').then(t=>{lastTraining=t;if(latestLive)renderEnhancedLive(latestLive);}).catch(()=>{});

function exportPlayers(players,anonymous) {
  return players.map((p,i)=>({...p,name:anonymous&&!p.is_self?'Spieler '+(i+1):p.name,
    skills:p.skills||[],heal_skills:p.heal_skills||[]}));
}
async function copyText(text) {
  try {await navigator.clipboard.writeText(text);}catch(e) {
    const focused=document.activeElement,input=document.createElement('textarea');input.value=text;
    input.tabIndex=-1;input.setAttribute('aria-hidden','true');input.style.cssText='position:fixed;left:0;top:0;width:1px;height:1px;opacity:0;pointer-events:none';
    (document.querySelector('dialog[open]')||document.body).append(input);
    try {input.focus();input.select();if(!document.execCommand('copy'))throw new Error('Zwischenablage nicht verfügbar');}
    catch(e){toast('Kopieren fehlgeschlagen. Die Zwischenablage ist hier nicht verfügbar.',true);return false;}
    finally {input.remove();if(focused?.isConnected)focused.focus({preventScroll:true});}
  }
  toast('Ergebnis kopiert.');
  return true;
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

// The fight library has two lists with the same filters: bosses (field
// bosses, training and unknown targets included) and, at the very bottom of
// the page, ordinary world mobs. Each shows FIGHT_PAGE fights and pages on its own.
const fightLists={
  boss:{el:'#fightResults',pager:'#fightPager',label:'#fightPage',prev:'#fightPrev',next:'#fightNext',page:0,request:0,empty:'Keine passenden Kämpfe.'},
  mob:{el:'#mobResults',pager:'#mobPager',label:'#mobPage',prev:'#mobPrev',next:'#mobNext',page:0,request:0,empty:'Keine passenden Welt-Mobs.'},
};
const shownFights=new Map();
function fightFilters() {
  const q=new URLSearchParams({query:$('#fightSearch').value,character,favorites:String($('#fightFavorites').checked),limit:String(FIGHT_PAGE)});
  if($('#fightFrom').value)q.set('from',String(new Date($('#fightFrom').value+'T00:00:00').getTime()));
  if($('#fightTo').value)q.set('to',String(new Date($('#fightTo').value+'T23:59:59.999').getTime()));
  return q;
}
function fightRow(f) {
  return `<tr><td>${date(f.started_at)}</td><td>${favButton(f.favorite,'data-fav-fight',f.id,f.boss_name)}<button class="btn" data-open-fight="${esc(f.id)}">${esc(f.boss_name)}${f.is_train?' · Training':''}</button>${badge(f.difficulty)}<div class="muted">${esc(f.tags||'')}</div></td><td>${dur(f.duration_ms)}</td><td>${num(f.my_dps)}/s</td></tr>`;
}
// Shows one page of a list. reset jumps back to the newest page; otherwise the
// current page is reloaded (e.g. after saving a note). Without a kind both lists load.
async function loadFights(reset=true,kind) {
  if($('#fightFrom').value&&$('#fightTo').value&&$('#fightFrom').value>$('#fightTo').value){toast('Das Von-Datum muss vor dem Bis-Datum liegen.',true);return;}
  if(!kind)return Promise.all(Object.keys(fightLists).map(k=>loadFights(reset,k)));
  const list=fightLists[kind],request=++list.request;
  if(reset)list.page=0;
  $(list.el).textContent='Kämpfe werden geladen …';
  $(list.prev).disabled=$(list.next).disabled=true;
  const q=fightFilters();q.set('kind',kind);q.set('offset',String(list.page*FIGHT_PAGE));
  try {
    const data=await api('/api/fights?'+q);if(request!==list.request)return;
    const fights=data.fights||[];
    // A page emptied by a changed filter result steps back to the first page.
    if(!fights.length&&list.page>0){list.page=0;return loadFights(false,kind);}
    for(const f of fights)shownFights.set(f.id,f);
    $(list.el).innerHTML=fights.length?`<div class="table-scroll"><table><thead><tr><th>Datum</th><th>Kampf</th><th>Dauer</th><th>Meine DPS</th></tr></thead><tbody>${fights.map(fightRow).join('')}</tbody></table></div>`:`<p class="muted">${list.empty}</p>`;
    const first=list.page*FIGHT_PAGE;
    $(list.pager).hidden=list.page===0&&!data.more;
    $(list.label).textContent=`Seite ${list.page+1} · ${fights.length?first+1:0}–${first+fights.length}`;
    $(list.prev).disabled=list.page===0;$(list.next).disabled=!data.more;
    $(list.el).querySelectorAll('[data-open-fight]').forEach(b=>b.onclick=()=>task(openFight(b.dataset.openFight)));
    $(list.el).querySelectorAll('[data-fav-fight]').forEach(b=>b.onclick=()=>task(toggleFavorite(b,kind)));
  }catch(e){if(request===list.request){$(list.el).textContent='Kampfliste konnte nicht geladen werden.';$(list.pager).hidden=true;}throw e;}
}
// Star directly in the list; keeps the fight's note and tags.
async function toggleFavorite(button,kind) {
  const f=shownFights.get(button.dataset.favFight);if(!f||button.disabled)return;
  button.disabled=true;
  try {
    await api('/api/fights/'+encodeURIComponent(f.id)+'/annotation',{method:'POST',body:JSON.stringify({favorite:!f.favorite,note:f.note||'',tags:f.tags||''})});
    f.favorite=f.favorite?0:1;
    if(currentFight?.id===f.id){currentFight.favorite=!!f.favorite;if($('#favoriteFight'))$('#favoriteFight').checked=!!f.favorite;}
    // With "Nur Favoriten" an unstarred fight leaves the list.
    if($('#fightFavorites').checked)return loadFights(false,kind);
    setFavButton(button,!!f.favorite,f.boss_name);
  } finally {button.disabled=false;}
}
window.loadFights=loadFights;
$('#searchFights').onclick=()=>task(loadFights());
// Buttons stay disabled while a page loads, so double clicks cannot skip pages.
for(const [kind,list] of Object.entries(fightLists)){
  $(list.prev).onclick=()=>{list.page=Math.max(0,list.page-1);task(loadFights(false,kind));};
  $(list.next).onclick=()=>{list.page++;task(loadFights(false,kind));};
}
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
  return `<div class="skill-browser" data-skills="${esc(JSON.stringify(rows))}"><p class="analysis-note">Treffer und Ticks sind keine Skill-Aktivierungen. Anteil bezieht sich auf diese Spielerliste. ${heal?'Heilung seit Parser-Reset.':'Treffermerkmale: beobachtete Anteile, vollständige Erfassung unbekannt.'} — bedeutet kein nachgewiesener Wert, nicht gemessene 0 %. Resist zählt widerstandene Effekte.</p><div class="row skill-tools"><input type="search" class="skill-search" aria-label="Skills suchen" placeholder="Name / Skill-ID" title="/: Suche fokussieren · Esc: Suche leeren"><select class="skill-sort" aria-label="Skills sortieren"><option value="damage">${heal?'Heilung':'Schaden'} absteigend</option><option value="name">Name A–Z</option><option value="hits">Treffer / Ticks absteigend</option></select>${extra.length?'<label><input type="checkbox" class="skill-extra"> Weitere Treffermerkmale</label>':''}<span class="skill-count muted">${rows.length} Skills</span></div><div class="table-scroll"><table><thead><tr>${[...columns,...extra].map(([k,label],i)=>`<th${i>=columns.length?' class="skill-advanced"':''} aria-sort="${k==='damage'?'descending':'none'}"><button type="button" class="sort-head" data-sort="${k}" title="Nach ${esc(label)} sortieren">${label}</button></th>`).join('')}</tr></thead><tbody>${rows.map(s=>`<tr>${[...columns,...extra].map(([k],i)=>`<td${i>=columns.length?' class="skill-advanced"':''} title="${k==='name'?esc(s.name):esc(s[k]==null?'Kein nachgewiesener Wert':Number(s[k]).toLocaleString('de-DE',{maximumFractionDigits:2}))}">${cell(s,k)}</td>`).join('')}</tr>`).join('')}</tbody></table></div><p class="skill-empty muted" hidden>Keine passenden Skills.</p></div>`;
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
// Curves use saved cumulative observations; smoothing changes presentation only.
function damageSamples(f,actor=null,windowMs=0) {
  const points=f.analytics?.points||[],partial=Boolean(f.analytics?.partial);
  const totals=points.map(p=>actor==null?Object.values(p.damage||{}).reduce((a,b)=>a+(Number(b)||0),0):Number(p.damage?.[actor]||0));
  let base=partial?0:-1;
  return points.map((p,i)=>{
    if(windowMs>0){while(base+1<i&&points[base+1].ms<=p.ms-windowMs)base++;}
    else base=i-1;
    const span=p.ms-(base<0?0:points[base].ms);
    return {ms:p.ms,total:totals[i],dps:span>0?Math.max(0,totals[i]-(base<0?0:totals[base]))*1000/Math.max(500,span):0};
  }).slice(partial?1:0);
}
function curveMarkup(points,series,{label='Schadensverlauf',unit='DPS',width=900,end=0}={}) {
  if(!points.length)return '<p class="analysis-note">Keine auswertbaren Verlaufsdaten gespeichert. Ältere Kämpfe enthalten diese Daten eventuell nicht.</p>';
  const W=Math.max(280,width),H=240,L=58,R=18,T=26,B=34;
  end=Math.max(1000,end,points[points.length-1].ms);
  const max=series.reduce((m,s)=>points.reduce((n,p,i)=>Math.max(n,s.value(p,i)),m),1)*1.08;
  const x=ms=>L+ms/end*(W-L-R),y=v=>T+(1-v/max)*(H-T-B);
  const step=Math.max(1,Math.ceil(points.length/600));
  const indices=points.map((_,i)=>i).filter(i=>i%step===0||i===points.length-1);
  return `<svg class="curve-chart" viewBox="0 0 ${W} ${H}" role="img" aria-label="${esc(label)}">
    <text x="${L}" y="14">${esc(unit)}</text>
    ${[0,.25,.5,.75,1].map(t=>`<line x1="${L}" x2="${W-R}" y1="${y(max*t)}" y2="${y(max*t)}" class="curve-grid"/><text x="${L-8}" y="${y(max*t)+4}" text-anchor="end">${num(max*t)}</text>`).join('')}
    ${[0,.25,.5,.75,1].map(t=>`<text x="${x(end*t)}" y="${H-9}" text-anchor="${t===0?'start':t===1?'end':'middle'}">${end<10000?(end*t/1000).toLocaleString('de-DE',{maximumFractionDigits:1})+' s':dur(end*t)}</text>`).join('')}
    ${series.map(s=>`<path d="${indices.map((i,j)=>`${j?'L':'M'}${x(points[i].ms).toFixed(2)},${y(s.value(points[i],i)).toFixed(2)}`).join(' ')}" fill="none" stroke="${s.color}" stroke-width="2" stroke-linejoin="round"><title>${esc(s.name)} · ${num(points.reduce((m,p,i)=>Math.max(m,s.value(p,i)),0))}${unit==='DPS'?'/s':' '+esc(unit)}</title></path>${points.length===1?`<circle cx="${x(points[0].ms)}" cy="${y(s.value(points[0],0))}" r="4" fill="${s.color}"/>`:''}`).join('')}
    <line class="curve-cross" x1="${x(points[points.length-1].ms)}" x2="${x(points[points.length-1].ms)}" y1="${T}" y2="${H-B}"/>
    <rect class="curve-hit" x="${L}" y="${T}" width="${W-L-R}" height="${H-T-B}" fill="transparent"/>
  </svg>${step>1?'<p class="analysis-note">Diagramm ausgedünnt (höchstens 601 Punkte je Linie). Die Zeitauswahl verwendet alle gespeicherten Beobachtungen.</p>':''}`;
}
function svgCurve(points,value,label) {
  return curveMarkup(points,[{name:label,color:'var(--accent)',value}],{label,unit:label});
}
function damageCurve(f,actor=null) {
  return curveMarkup(damageSamples(f,actor),[{name:'Schaden',color:'var(--accent)',value:p=>p.dps}],{label:'Beobachtete Intervall-DPS',end:f.duration_ms})+(f.analytics?.partial?'<p class="analysis-note">Verlauf unvollständig; der erste gespeicherte Wert wird nur als Ausgangswert verwendet.</p>':'');
}
function fightChartControls(f) {
  return `<div class="damage-chart" id="fightDamageChart" aria-label="Schadensverlauf"><h3>Schadensverlauf</h3>
    <div class="chart-tools"><label>Ansicht <select id="fightChartScope"><option value="group">Gesamte Gruppe</option><option value="players">Spieler vergleichen</option>${f.players.map(p=>`<option value="${Number(p.actor_id)}">${esc(p.name)}</option>`).join('')}</select></label>
    <label>Wert <select id="fightChartMetric"><option value="dps">DPS</option><option value="total">Gesamtschaden</option></select></label>
    <label>Glättung <select id="fightChartWindow"><option value="5000">5 Sekunden</option><option value="0">Einzelne Intervalle</option></select></label></div>
    <p class="analysis-note" data-curve-note></p><div class="chart-legend" data-curve-legend></div><div data-curve-plot></div>
    <label class="curve-scrubber">Zeitpunkt <input type="range" data-curve-range min="0" max="0" value="0" aria-label="Zeitpunkt im Schadensverlauf"></label><div class="curve-readout" data-curve-readout></div><div class="curve-actions"><button class="btn" data-curve-peak disabled>Stärkstes 5s-Fenster</button><button class="btn" data-curve-hits disabled>Treffer im Zeitfenster</button></div><div data-curve-detail class="curve-detail"></div></div>`;
}
function bindFightChart(f) {
  const root=$('#fightDamageChart');if(!root)return;
  const scope=$('#fightChartScope'),metric=$('#fightChartMetric'),smooth=$('#fightChartWindow'),hidden=new Set();
  let selected=-1,peakActive=false;
  function render() {
    const unit=metric.value==='total'?'Schaden':'DPS',windowMs=Number(smooth.value),all=scope.value==='players';
    smooth.disabled=metric.value==='total';
    const actors=all?f.players:scope.value==='group'?[{actor_id:null,name:'Gesamte Gruppe'}]:f.players.filter(p=>String(p.actor_id)===scope.value);
    const samples=actors.map(p=>damageSamples(f,p.actor_id,windowMs));
    const points=samples[0]||[],peak=observedPeak(f,all||scope.value==='group'?null:Number(scope.value));
    const peakButton=root.querySelector('[data-curve-peak]'),hitsButton=root.querySelector('[data-curve-hits]'),detail=root.querySelector('[data-curve-detail]');
    peakButton.disabled=!peak;hitsButton.disabled=!points.length;detail.replaceChildren();
    let interval=null;
    const series=actors.map((p,i)=>({name:p.name,color:all?chartColors[i%chartColors.length]:'var(--accent)',value:(_,j)=>samples[i][j][metric.value==='total'?'total':'dps']}));
    const visible=series.filter((_,i)=>!all||!hidden.has(i));
    root.querySelector('[data-curve-note]').textContent=metric.value==='total'?'Gespeicherter Gesamtschaden bis zum gewählten Zeitpunkt.':windowMs?'DPS im gleitenden 5-Sekunden-Fenster, gerundet auf gespeicherte Beobachtungen; am Anfang über die bereits beobachtete Zeit.':'DPS je gespeichertem Beobachtungsintervall.';
    root.querySelector('[data-curve-note]').textContent+=` Beobachtungsraster: ${f.analytics?.resolution_ms||500} ms.${f.analytics?.partial?' Verlauf unvollständig; der erste Wert dient nur als Ausgangswert.':''}`;
    const legend=root.querySelector('[data-curve-legend]');
    legend.innerHTML=all?series.map((s,i)=>`<button type="button" class="legend-toggle" data-series="${i}" aria-pressed="${!hidden.has(i)}" title="Linie ein- oder ausblenden"><i style="background:${s.color}"></i><span>${esc(s.name)}</span></button>`).join(''):'';
    legend.querySelectorAll('button').forEach(b=>b.onclick=()=>{const i=Number(b.dataset.series);hidden.has(i)?hidden.delete(i):hidden.add(i);render();legend.querySelector(`[data-series="${i}"]`)?.focus();});
    const plot=root.querySelector('[data-curve-plot]');root.drawnWidth=root.clientWidth;
    plot.innerHTML=visible.length?curveMarkup(points,visible,{unit,label:all?unit+'-Verlauf aller Spieler':unit+' · '+actors[0]?.name,width:Math.min(900,root.clientWidth||900),end:f.duration_ms}):'<p class="analysis-note">Alle Linien ausgeblendet. Wähle einen Spieler in der Legende.</p>';
    const range=root.querySelector('[data-curve-range]'),readout=root.querySelector('[data-curve-readout]');
    range.max=Math.max(0,points.length-1);range.disabled=!points.length;
    const show=i=>{
      selected=Math.max(0,Math.min(points.length-1,i));range.value=selected;
      if(!points.length){readout.textContent='Keine zeitliche Schadensaufzeichnung vorhanden.';return;}
      const point=points[selected],value=s=>`${num(s.value(point,selected))} ${unit}`;
      readout.innerHTML=`<strong>${(point.ms/1000).toLocaleString('de-DE',{maximumFractionDigits:1})} s</strong>${visible.map(s=>`<span><i style="background:${s.color}"></i>${esc(s.name)} <b>${value(s)}</b></span>`).join('')}`;
      range.setAttribute('aria-valuetext',`${point.ms/1000} Sekunden; `+visible.map(s=>s.name+': '+value(s)).join('; '));
      const svg=plot.querySelector('svg'),line=plot.querySelector('.curve-cross');
      if(line){const W=svg.viewBox.baseVal.width,end=Math.max(1000,f.duration_ms||0,points[points.length-1].ms),x=58+point.ms/end*(W-76);line.setAttribute('x1',x);line.setAttribute('x2',x);}
    };
    const clearInterval=()=>{interval=null;peakActive=false;plot.querySelectorAll('[data-curve-highlight],[data-curve-peak-label]').forEach(e=>e.remove());detail.replaceChildren();};
    show(selected<0?points.length-1:selected);range.oninput=()=>{clearInterval();show(Number(range.value));};
    peakButton.textContent=all||scope.value==='group'?'Stärkstes Gruppen-5s-Fenster':'Stärkstes 5s-Fenster';
    peakButton.onclick=()=>{
      if(!peak)return;clearInterval();interval=peak;peakActive=true;
      show(points.reduce((best,p,i)=>Math.abs(p.ms-peak.end)<Math.abs(points[best].ms-peak.end)?i:best,0));
      const svg=plot.querySelector('svg');
      if(svg){const W=svg.viewBox.baseVal.width,end=Math.max(1000,f.duration_ms||0,points.at(-1).ms),band=document.createElementNS('http://www.w3.org/2000/svg','rect');
        band.setAttribute('data-curve-highlight','');band.setAttribute('x',58+peak.start/end*(W-76));band.setAttribute('y',26);band.setAttribute('width',(peak.end-peak.start)/end*(W-76));band.setAttribute('height',180);svg.insertBefore(band,svg.firstChild);
        const label=document.createElementNS('http://www.w3.org/2000/svg','text');label.setAttribute('data-curve-peak-label','');label.setAttribute('class','curve-peak-label');
        const mid=58+(peak.start+peak.end)/2/end*(W-76);label.setAttribute('x',Math.min(W-20,Math.max(60,mid)));label.setAttribute('y',20);label.setAttribute('text-anchor',mid>W-90?'end':'middle');label.textContent='Peak '+num(peak.dps);svg.append(label);}
      detail.textContent=`${num(peak.dps)} beobachtete DPS · ${clock(peak.start)}–${clock(peak.end)}${peak.partial?' · gespeicherter Ausschnitt':''}`;
    };
    hitsButton.onclick=()=>{
      const end=interval?.end??points[selected]?.ms,start=interval?.start??Math.max(0,end-5000),actor=all||scope.value==='group'?null:Number(scope.value),v=windowHits(f,actor,start,end);
      detail.innerHTML=`<h4>Beobachtete Treffer · ${(start/1000).toLocaleString('de-DE')}–${(end/1000).toLocaleString('de-DE')} s</h4><p class="analysis-note">Treffer/Ticks, keine Casts oder Schadenszuordnung. DoT und Multihit können mehrere Ereignisse erzeugen. Fehlende oder gekürzte Zeitpunkte werden nicht ergänzt.</p>`+(v.available?(v.hits.length?`<ol class="window-hits">${v.hits.slice(0,8).map(h=>`<li><span>${skillLabel(h.skill)}${h.skill.is_dot?' · DoT':''}<small>${esc(h.player)}</small></span><strong>${h.count} Treffer/Ticks</strong></li>`).join('')}</ol>${v.hits.length>8?`<p class="analysis-note">Die 8 häufigsten Skills von ${v.hits.length} im Fenster.</p>`:''}`:'<p class="analysis-note">Keine gespeicherten Treffer in diesem Fenster. Die Aufzeichnung kann unvollständig sein.</p>'):'<p class="analysis-note">Keine Trefferzeitpunkte gespeichert.</p>');
    };
    const hit=plot.querySelector('.curve-hit');
    // Hover only moves the crosshair; a click selects a new point and drops the marked window.
    if(hit){hit.onpointermove=e=>{
      const r=hit.getBoundingClientRect(),end=Math.max(1000,f.duration_ms||0,points[points.length-1].ms),ms=(e.clientX-r.left)/r.width*end;
      let lo=0,hi=points.length-1;while(lo<hi){const mid=(lo+hi)>>1;if(points[mid].ms<ms)lo=mid+1;else hi=mid;}
      if(lo>0&&ms-points[lo-1].ms<points[lo].ms-ms)lo--;show(lo);
    };hit.onpointerdown=e=>{clearInterval();hit.onpointermove(e);};}
    if(peakActive)peakButton.onclick();
  }
  // A resize redraw keeps the marked window; changing the view starts fresh.
  scope.onchange=metric.onchange=smooth.onchange=()=>{peakActive=false;render();};
  root.renderCurve=render;render();
  const storyButton=$('#fightContent [data-story-peak]');
  const storySpark=$('#fightContent .peak-spark');if(storyButton&&storySpark)storySpark.onclick=()=>storyButton.click();
  if(storyButton)storyButton.onclick=()=>{
    scope.value=String(f.players.find(p=>p.is_self)?.actor_id??'group');metric.value='dps';smooth.value='5000';
    render();root.querySelector('[data-curve-peak]').click();root.scrollIntoView({block:'nearest'});
  };
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
    ${fightStory(f)}
    <div class="row fight-tools"><select id="exportScope" aria-label="Export für">${exportScopeOptions(f)}</select><button class="btn" id="copyFight">Kopieren</button><button class="btn" id="jsonFight">JSON</button><button class="btn" id="csvFight">CSV</button><button class="btn" id="pngFight">PNG-Bericht</button><button class="btn" id="chatFight">Chatzeile</button><label><input type="checkbox" id="anonFight" checked> Andere Namen anonymisieren</label></div>
    <div class="row fight-tools"><label><input type="checkbox" id="favoriteFight" ${f.favorite?'checked':''}> Favorit</label><input id="fightNote" aria-label="Kampfnotiz" placeholder="Notiz" maxlength="4000" value="${esc(f.note||'')}"><input id="fightTags" aria-label="Kampf-Tags" placeholder="Tags, z. B. neues Gear" maxlength="500" value="${esc(f.tags||'')}"><button class="btn" id="saveFightNote">Speichern</button></div>
    ${fightChartControls(f)}<h3>Direkter Kampfvergleich</h3><div class="row"><select id="compareFight" aria-label="Vergleichskampf"><option value="">Vergleich laden …</option></select><button class="btn" id="compareBtn">Vergleichen</button></div><p class="analysis-note">Gleicher Boss und Schwierigkeitsgrad. Eigene Werte werden nur bei gleichem Charakter und gleicher Klasse verglichen.</p><div id="comparison"></div>
    <details><summary>Verbindung · Ping-Verlauf</summary>${svgCurve((f.ping_history||[]).map(p=>({ms:p.tsMs,ping:p.pingMs})),p=>p.ping,'ms Ping')}</details>
    ${f.players.map(p=>playerReport(p,f)).join('')}${effectTimeline(f,f.target_id)}${uptimes(f.boss_debuffs,true)}`;
  bindPlayerReports(f);fillAttemptContext($('#fightContent'),f);
  if(!$('#fightDialog').open)$('#fightDialog').showModal();
  $('#saveFightNote').onclick=()=>task(api('/api/fights/'+encodeURIComponent(id)+'/annotation',{method:'POST',body:JSON.stringify({favorite:$('#favoriteFight').checked,note:$('#fightNote').value,tags:$('#fightTags').value})}).then(()=>{toast('Kampfnotiz gespeichert.');if(tab==='runs')task(loadFights(false));}));
  $('#copyFight').onclick=()=>task(copyText(rankingText(f.boss_name,scopedPlayers(f,exportPlayers(f.players,$('#anonFight').checked)),f.duration_ms)));
  $('#jsonFight').onclick=()=>download(`aion2-kampf${exportSuffix(f)}.json`,JSON.stringify(scopedExport(f,$('#anonFight').checked),null,2),'application/json');
  $('#csvFight').onclick=()=>{const exp=scopedExport(f,$('#anonFight').checked),rows=[['Spieler','Klasse','Schaden','DPS','Heilung','HPS','Erlittener Schaden'],...exp.players.map(p=>[p.name,p.class_name,p.damage,p.dps,p.heal,p.hps,p.damage_received])];
    // A single-player export also lists that player's skills below the summary row.
    if(exportScopeIndex(f)!=null){const p=exp.players[0];rows.push([],['Skill','Art','Wert','Treffer/Ticks','Krit %','Min','Max']);for(const [kind,list] of [['Schaden',p.skills],['Heilung',p.heal_skills]])for(const sk of list||[])rows.push([sk.name,kind,sk.damage,sk.hits,sk.crit_rate,sk.min,sk.max]);}
    download(`aion2-kampf${exportSuffix(f)}.csv`,'\uFEFF'+rows.map(r=>r.map(csvCell).join(';')).join('\r\n'),'text/csv;charset=utf-8');};
  if(window.installFightQol)installFightQol(f);
  bindFightChart(f);
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
if(tab==='stats')drawBoss();

// One redraw per frame and only when the chart width changed; keeps hover targets stable.
let fightChartFrame=0;
window.addEventListener('resize',()=>{cancelAnimationFrame(fightChartFrame);fightChartFrame=requestAnimationFrame(()=>{
  const root=$('#fightDamageChart');if($('#fightDialog').open&&root?.renderCurve&&root.clientWidth!==root.drawnWidth)root.renderCurve();
});});
