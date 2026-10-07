// Additional local views. PNGs are drawn from an explicit export allowlist.
function applyAppearance(s={}) {
  document.documentElement.dataset.theme=['midnight','aether','ember'].includes(s.theme)?s.theme:'midnight';
  document.documentElement.classList.toggle('compact',Boolean(s.compact));
}
function chatLine(title,players,ms,metric='damage',limit=200) {
  const label=metric==='heal'?'HPS':metric==='damage_received'?'erlitten/s':'DPS';
  const clean=s=>String(s??'').replace(/[\r\n\t|]/g,' ').replace(/\s+/g,' ').trim();
  let text=`${clean(title).slice(0,50)} ${dur(ms)} | ${label}: `;
  for(const p of players) {
    const part=`${clean(p.name).slice(0,24)} ${num(p.dps)} `;
    if(Array.from(text+part).length>limit-2){text+='…';break;}
    text+=part;
  }
  return text.trim();
}
$('#copyChat').onclick=()=>{if(latestLive)task(copyText(chatLine(latestLive.target_name||'Kampf',exportPlayers(metricRows(latestLive.rows),$('#anonymousExport').checked),latestLive.battle_time_ms,metricKey())));};
const chartColors=['#75e0ce','#f6b179','#cda8ff','#7eafff','#f58dba','#d8dc76','#91cfe5','#f18282','#add9a1','#d8b59a'];
function partyCurve(f) {
  const points=f.analytics?.points||[];
  if(points.length<2)return '<p class="analysis-note">Zu wenige gespeicherte Beobachtungen für einen Gruppenverlauf.</p>';
  const W=900,H=230,L=60,R=15,T=20,B=30;
  const series=f.players.map((p,i)=>({player:p,color:chartColors[i%chartColors.length],values:points.map((q,j)=>{
    const prev=points[j-1];return Math.max(0,Number(q.damage[p.actor_id]||0)-Number(prev?.damage[p.actor_id]||0))*1000/Math.max(500,q.ms-(prev?.ms||0));
  })}));
  const end=points.reduce((n,p)=>Math.max(n,p.ms),1000),max=series.reduce((n,s)=>s.values.reduce((m,v)=>Math.max(m,v),n),1);
  const step=Math.max(1,Math.ceil(points.length/600)),indices=points.map((_,i)=>i).filter(i=>i%step===0||i===points.length-1);
  const x=ms=>L+ms/end*(W-L-R),y=v=>T+(1-v/max)*(H-T-B);
  const lines=series.map(s=>`<path d="${indices.map((i,j)=>`${j?'L':'M'}${x(points[i].ms).toFixed(2)},${y(s.values[i]).toFixed(2)}`).join(' ')}" fill="none" stroke="${s.color}" stroke-width="2"><title>${esc(s.player.name)}</title></path>`).join('');
  return `<p class="analysis-note">Beobachtete Intervall-DPS, ${f.analytics.resolution_ms||500} ms${step>1?' · Darstellung ausgedünnt (max. 601 Punkte je Spieler)':''}${f.analytics.partial?' · Daten unvollständig':''}. Kein Verlauf einzelner Schadenspakete. Vollständigkeit vor Aufnahmebeginn unbekannt.</p><div class="chart-legend">${series.map(s=>`<span><i style="background:${s.color}"></i>${esc(s.player.name)}</span>`).join('')}</div><svg class="timeline" viewBox="0 0 ${W} ${H}" role="img" aria-label="DPS-Verlauf aller Spieler"><path d="M${L},${T}V${H-B}H${W-R}" fill="none" stroke="#7898b655"/>${lines}<text x="0" y="20">${num(max)}/s</text><text x="${L}" y="${H-5}">0:00</text><text x="${W-65}" y="${H-5}">${dur(end)}</text></svg>`;
}
function pairSkills(skills=[]) {
  return `<div class="table-scroll pair-skills"><table><thead><tr><th>Skill</th><th>Schaden</th><th>Treffer/Ticks</th><th>Krit</th><th>Max</th></tr></thead><tbody>${skills.map(s=>`<tr><td>${esc(s.name)}${s.is_dot?' · DoT':''}</td><td>${num(s.damage)}</td><td>${s.hits??'—'}</td><td>${pct(s.crit_rate)}</td><td>${s.max>0?num(s.max):'—'}</td></tr>`).join('')}</tbody></table></div>`;
}
function pairReport(a,b,f) {
  if(!a||!b)return '<p class="analysis-note">Zwei Spieler wählen.</p>';
  if(a.actor_id===b.actor_id)return '<p class="analysis-note">Bitte zwei unterschiedliche Spieler wählen.</p>';
  return `<p class="analysis-note">Zwei Spieler im selben Kampf. Klasse, Ausrüstung und Aufgaben beeinflussen die Werte.</p><div class="player-pair">${[a,b].map(p=>`<div><h3>${esc(p.name)} · ${esc(p.class_name||p.job||'')}</h3><p>${num(p.dps)}/s · ${num(p.damage)} Schaden · ${num(p.heal||0)} Heilung</p>${pairSkills(p.skills)}${hitTimeline(p.skills,f.duration_ms,480)}${effectTimeline(f,p.actor_id,480)}</div>`).join('')}</div>`;
}
function installFightQol(f) {
  // After the fight-to-fight comparison, so export buttons and notes stay together.
  const anchor=$('#comparison');
  const options=f.players.map(p=>`<option value="${Number(p.actor_id)}">${esc(p.name)}</option>`).join('');
  const block=document.createElement('div');block.innerHTML=`<h3>Spieler direkt vergleichen</h3><div class="row fight-tools"><select id="pairA" aria-label="Erster Spieler">${options}</select><select id="pairB" aria-label="Zweiter Spieler">${options}</select><button class="btn" id="pairCompare">Spieler vergleichen</button></div><div id="playerPair"></div><h3>Alle Spieler im selben Diagramm</h3>${partyCurve(f)}`;
  anchor.parentNode.insertBefore(block,anchor.nextSibling);
  if(f.players.length>1)$('#pairB').selectedIndex=1;
  $('#pairCompare').onclick=()=>{$('#playerPair').innerHTML=pairReport(f.players.find(p=>String(p.actor_id)===$('#pairA').value),f.players.find(p=>String(p.actor_id)===$('#pairB').value),f);};
  $('#pngFight').onclick=()=>task(exportPng(f,$('#anonFight').checked));
  $('#chatFight').onclick=()=>task(copyText(chatLine(f.boss_name,exportPlayers(f.players,$('#anonFight').checked),f.duration_ms)));
}
function installLiveComparison(detail,row) {
  const candidates=latestLive?.rows.filter(r=>r.id!==row?.id)||[];
  const div=document.createElement('div');div.innerHTML=`<h3>Zweiten Spieler daneben öffnen</h3><div class="row"><select id="livePair" aria-label="Zweiter Live-Spieler">${candidates.map(r=>`<option value="${r.id}">${esc(r.name)}</option>`).join('')}</select><button class="btn" id="livePairButton" ${candidates.length?'':'disabled'}>Daneben öffnen</button></div><div id="livePairResult"></div>`;
  $('#fightContent').append(div);
  $('#livePairButton').onclick=()=>task((async()=>{
    const request=detailRequest,id=Number($('#livePair').value), target=detail.target_id;
    const other=await api('/api/players/'+id);
    if(request!==detailRequest)return;
    if(target==null||detail.start_time==null||other.target_id!==target||latestLive?.target_id!==target||other.start_time!==detail.start_time||latestLive.target_started_at!==detail.start_time){$('#livePairResult').textContent='Ziel hat sich geändert. Details bitte aktualisieren.';return;}
    const otherRow=latestLive.rows.find(r=>r.id===id);
    const a={...row,actor_id:row.id,skills:detail.skills},b={...otherRow,actor_id:id,skills:other.skills};
    $('#livePairResult').innerHTML='<p class="analysis-note">Erster Spieler bleibt angeheftet. Werte sind zwei aufeinanderfolgende Momentaufnahmen.</p>'+pairReport(a,b,{duration_ms:detail.duration_ms,players:[a,b]});
  })());
}
async function exportPng(f,anonymous) {
  const exp=fightExport(f,anonymous),lines=[];
  const add=(text,color='#edf3fb',size=18)=>lines.push({text:String(text),color,size});
  add('AION 2 · KAMPFBERICHT','#75e0ce',26);
  add(`${exp.boss||'Kampf'} · ${exp.difficulty||''} · ${dur(exp.duration_ms)}`);
  add('Erfasste Werte. Treffer/Ticks sind keine Casts.','#aabbd0',15);
  add('Treffermerkmale: Erfassung unbekannt. — ist kein gemessener Nullwert.','#aabbd0',15);
  add(exp.healing_scope||'Erfasste Heilung, kein Overheal-Abzug.','#aabbd0',15);
  for(const p of exp.players) {
    add('');add(`${p.name} · ${p.class_name||''}`,'#f4d8a7',22);
    add(`${num(p.damage)} Schaden · ${num(p.dps)} DPS · ${num(p.heal||0)} Heilung`);
    for(const [kind,skills] of [['Schaden',p.skills],['Heilung',p.heal_skills]]) {
      if(!skills.length)continue;add(kind,'#75e0ce');
      for(const s of skills) {
        add(`${s.name}${s.is_dot?' (Tick)':''} | ${num(s.damage)} | ${s.hits??'—'} Treffer/Ticks | Min ${s.min>0?num(s.min):'—'} | Max ${s.max>0?num(s.max):'—'}`, '#edf3fb',16);
        if(kind==='Schaden') {
          add(`Krit ${pct(s.crit_rate)} · Rücken ${pct(s.back_rate)} · Frontal ${pct(s.frontal_rate)} · Perfekt ${pct(s.perfect_rate)} · Double ${pct(s.double_rate)} · Pariert ${pct(s.parry_rate)}`,'#aabbd0',15);
          add(`Block ${pct(s.block_rate)} · Perfektblock ${pct(s.perfect_block_rate)} · Ausdauer ${pct(s.endurance_rate)} · Regeneration ${pct(s.regeneration_rate)} · Multihit ${s.multi_hit_count??'—'} · Miss ${s.miss_count??'—'} · Resist ${s.resist_count??'—'}`,'#aabbd0',15);
        }
      }
    }
    for(const b of p.buffs)add(`Buff: ${b.name} · ${pct(b.uptime)} Uptime`,'#aabbd0',15);
  }
  // Bounded canvas dimensions. Long reports become consecutive pages without losing rows.
  for(let offset=0,page=1;offset<lines.length;offset+=180,page++) {
    const rows=[...(offset?lines.slice(0,5):[]),...lines.slice(offset,offset+180)],canvas=document.createElement('canvas');canvas.width=1400;canvas.height=rows.length*34+100;
    const ctx=canvas.getContext('2d');ctx.fillStyle='#101c2c';ctx.fillRect(0,0,canvas.width,canvas.height);
    ctx.fillStyle='#75e0ce';ctx.fillRect(0,0,canvas.width,5);
    rows.forEach((line,i)=>{ctx.font=`${line.size}px system-ui`;ctx.fillStyle=line.color;ctx.fillText(line.text,36,50+i*34,1328);});
    ctx.fillStyle='#aabbd0';ctx.font='14px system-ui';ctx.fillText(`Lokaler Export · Seite ${page} · ${anonymous?'Namen anonymisiert':'Namen enthalten'}`,36,canvas.height-20);
    const blob=await new Promise((resolve,reject)=>canvas.toBlob(b=>b?resolve(b):reject(new Error('PNG konnte nicht erstellt werden')),'image/png'));
    download(`aion2-kampf${lines.length>180?'-'+page:''}.png`,blob,'image/png');
  }
  toast('PNG-Bericht erstellt.');
}
api('/api/version').then(v=>{$('#versionInfo').textContent=v.version?`Version ${v.version} · Parser ${v.parser_version}`:'Versionsinformation nicht verfügbar.';}).catch(()=>{$('#versionInfo').textContent='Versionsinformation nicht erreichbar.';});
$('#checkUpdate').onclick=()=>task((async()=>{
  const button=$('#checkUpdate');button.disabled=true;$('#updateInfo').textContent='GitHub wird geprüft …';
  try {
    const result=await api('/api/update-check',{method:'POST'});
    $('#updateInfo').replaceChildren(document.createTextNode(result.message||'Keine Versionsinformation verfügbar.'));
    if(result.url){const a=document.createElement('a');a.href=result.url;a.target='_blank';a.rel='noopener';a.textContent=' Veröffentlichung öffnen';$('#updateInfo').append(a);}
  }catch(e){$('#updateInfo').textContent='Updateprüfung fehlgeschlagen. Bitte später erneut versuchen.';throw e;}
  finally{button.disabled=false;}
})());
