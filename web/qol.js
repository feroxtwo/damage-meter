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
    const prev=points[j-1];if(j===0&&f.analytics.partial)return 0;return Math.max(0,Number(q.damage[p.actor_id]||0)-Number(prev?.damage[p.actor_id]||0))*1000/Math.max(500,q.ms-(prev?.ms||0));
  })}));
  const end=points.reduce((n,p)=>Math.max(n,p.ms),1000),max=series.reduce((n,s)=>s.values.reduce((m,v)=>Math.max(m,v),n),1);
  const step=Math.max(1,Math.ceil(points.length/600)),indices=points.map((_,i)=>i).filter(i=>(!f.analytics.partial||i>0)&&(i%step===0||i===points.length-1));
  const x=ms=>L+ms/end*(W-L-R),y=v=>T+(1-v/max)*(H-T-B);
  const lines=series.map(s=>`<path d="${indices.map((i,j)=>`${j?'L':'M'}${x(points[i].ms).toFixed(2)},${y(s.values[i]).toFixed(2)}`).join(' ')}" fill="none" stroke="${s.color}" stroke-width="2"><title>${esc(s.player.name)}</title></path>`).join('');
  return `<p class="analysis-note">Beobachtete Intervall-DPS, ${f.analytics.resolution_ms||500} ms${step>1?' · Darstellung ausgedünnt (max. 601 Punkte je Spieler)':''}${f.analytics.partial?' · Daten unvollständig':''}. Kein Verlauf einzelner Schadenspakete. Vollständigkeit vor Aufnahmebeginn unbekannt.</p><div class="chart-legend">${series.map(s=>`<span><i style="background:${s.color}"></i>${esc(s.player.name)}</span>`).join('')}</div><svg class="timeline" viewBox="0 0 ${W} ${H}" role="img" aria-label="DPS-Verlauf aller Spieler"><path d="M${L},${T}V${H-B}H${W-R}" fill="none" stroke="#7898b655"/>${lines}<text x="0" y="20">${num(max)}/s</text><text x="${L}" y="${H-5}">0:00</text><text x="${W-65}" y="${H-5}">${dur(end)}</text></svg>`;
}
function pairSkills(skills=[]) {
  return `<div class="table-scroll pair-skills"><table><thead><tr><th>Skill</th><th>Schaden</th><th>Treffer/Ticks</th><th>Krit</th><th>Max</th></tr></thead><tbody>${skills.map(s=>`<tr><td>${skillLabel(s)}${s.is_dot?' · DoT':''}</td><td>${num(s.damage)}</td><td>${s.hits??'—'}</td><td>${pct(s.crit_rate)}</td><td>${s.max>0?num(s.max):'—'}</td></tr>`).join('')}</tbody></table></div>`;
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
// Graphical fight report. Validated categorical order for the dark report surface;
// colors follow the player, labels stay in text colors, every series is also named.
const reportColors=['#3987e5','#d95926','#199e70','#c98500','#d55181','#008300','#9085e9','#e66767'];
const reportInk={bg:'#101c2c',panel:'#16263a',text:'#edf3fb',muted:'#aabbd0',faint:'#7898b6',grid:'#7898b633',accent:'#75e0ce',buff:'#8ea6c4'};
// Rolling DPS over the stored cumulative damage samples, one series per exported player.
function reportCurves(f,players,window=5000) {
  const points=(f.analytics?.points||[]).slice(f.analytics?.partial?1:0);
  if(points.length<3)return null;
  const series=(f.players||[]).slice(0,players.length).map((p,i)=>{
    const cum=points.map(q=>Number(q.damage?.[p.actor_id]||0));let j=0;
    const values=points.map((q,k)=>{while(j<k&&points[j+1].ms<=q.ms-window)j++;const span=q.ms-points[j].ms;return k===0||span<=0?0:Math.max(0,cum[k]-cum[j])*1000/span;});
    return {name:players[i].name,color:i<reportColors.length?reportColors[i]:reportInk.faint,values};
  });
  return {ms:points.map(q=>q.ms),series};
}
function fitText(ctx,text,width) {
  text=String(text??'');if(ctx.measureText(text).width<=width)return text;
  let lo=0,hi=text.length;while(lo<hi){const mid=(lo+hi+1)>>1;if(ctx.measureText(text.slice(0,mid)+'…').width<=width)lo=mid;else hi=mid-1;}
  return text.slice(0,lo)+'…';
}
// Same buff name from several effect codes is shown once; the longest observed uptime is kept.
function reportBuffs(buffs=[]) {
  const byName=new Map();
  for(const b of buffs){const key=String(b.name||'').trim();const old=byName.get(key);if(!old||Number(b.uptime||0)>Number(old.uptime||0))byName.set(key,b);}
  return [...byName.values()].sort((a,b)=>Number(b.uptime||0)-Number(a.uptime||0));
}
function skillTraits(s) {
  const t=[['Krit',s.crit_rate],['Rücken',s.back_rate],['Frontal',s.frontal_rate],['Perfekt',s.perfect_rate],['Double',s.double_rate],['Pariert',s.parry_rate],['Block',s.block_rate],['Perfektblock',s.perfect_block_rate],['Ausdauer',s.endurance_rate],['Regeneration',s.regeneration_rate]]
    .filter(([,v])=>v!=null&&Number.isFinite(Number(v))).map(([k,v])=>`${k} ${pct(v)}`);
  for(const [k,v] of [['Multihit',s.multi_hit_count],['Miss',s.miss_count],['Resist',s.resist_count]])if(v!=null&&Number(v)>0)t.push(`${k} ${v}`);
  return t.join(' · ');
}
async function reportImages(exp) {
  const paths=[...new Set(exp.players.flatMap(p=>[classKeys.includes(p.class_key)?'/assets/icons/class-'+p.class_key+'.webp':'',...[...p.skills||[],...p.heal_skills||[],...p.buffs||[]].map(iconUrl)]).filter(Boolean))];
  const images=new Map();
  await Promise.all(paths.map(url=>new Promise(resolve=>{const img=new Image(),timer=setTimeout(()=>{img.src='';resolve();},3000);img.onload=()=>{clearTimeout(timer);images.set(url,img);resolve();};img.onerror=()=>{clearTimeout(timer);resolve();};img.src=url;})));
  return images;
}
function reportBlocks(exp,curves,images=new Map()) {
  const drawIcon=(ctx,url,x,y,size=22)=>{const img=images.get(url);if(img)ctx.drawImage(img,x,y,size,size);return Boolean(img);};
  const W=1400,X=40,blocks=[],font=(ctx,size,weight='')=>{ctx.font=`${weight} ${size}px system-ui, sans-serif`.trim();};
  const text=(ctx,s,x,y,color=reportInk.text,size=16,weight='',align='left',max=W-2*X)=>{font(ctx,size,weight);ctx.fillStyle=color;ctx.textAlign=align;ctx.fillText(fitText(ctx,s,max),x,y);ctx.textAlign='left';};
  const bar=(ctx,x,y,w,h,color)=>{ctx.fillStyle=color;ctx.beginPath();ctx.roundRect(x,y,Math.max(2,w),h,[0,4,4,0]);ctx.fill();};
  const heading=(title,note)=>({h:note?62:44,draw:(ctx,y)=>{text(ctx,title,X,y+28,reportInk.accent,20,'600');if(note)text(ctx,note,X,y+52,reportInk.muted,14);}});
  const title=[exp.boss||'Kampf',exp.difficulty,dur(exp.duration_ms)].filter(Boolean).join(' · ');
  const head={h:118,draw:(ctx,y)=>{
    text(ctx,'AION 2 · KAMPFBERICHT',X,y+40,reportInk.accent,26,'600');
    text(ctx,title,X,y+74,reportInk.text,22);
    text(ctx,[exp.started_at?date(exp.started_at):'',`${exp.players.length} Spieler`].filter(Boolean).join(' · '),W-X,y+74,reportInk.muted,16,'','right',400);
    text(ctx,'Erfasste Werte. Treffer/Ticks sind keine Casts. Treffermerkmale ohne Wert wurden nicht erfasst und sind kein Nullwert. '+(exp.healing_scope||'Heilung ohne Overheal-Abzug.'),X,y+104,reportInk.muted,14);
  }};
  blocks.push(head);
  // Group comparison and DPS over time.
  const players=exp.players,maxDps=Math.max(1,...players.map(p=>Number(p.dps)||0)),total=players.reduce((n,p)=>n+(Number(p.damage)||0),0);
  if(players.length>1){
    blocks.push(heading('Gruppe · DPS'));
    for(const [i,p] of players.entries())blocks.push({h:34,draw:(ctx,y)=>{
      const c=i<reportColors.length?reportColors[i]:reportInk.faint;
      const classShift=drawIcon(ctx,'/assets/icons/class-'+p.class_key+'.webp',X,y+5)?28:0;
      text(ctx,`${p.name}${p.class_name?' · '+p.class_name:''}`,X+classShift,y+22,reportInk.text,16,'',`left`,300);
      bar(ctx,360,y+6,(Number(p.dps)||0)/maxDps*700,22,c);
      text(ctx,`${num(p.dps)} DPS · ${num(p.damage)} · ${total>0?pct(p.damage*100/total):'—'}`,1080,y+22,reportInk.text,16,'','left',280);
    }});
  }
  if(curves){
    const H=300,L=X+70,R=W-X-170,T=16,B=36,end=Math.max(1000,curves.ms[curves.ms.length-1]),max=Math.max(1,...curves.series.flatMap(s=>s.values));
    blocks.push(heading('DPS-Verlauf','Gleitender Durchschnitt über 5 Sekunden aus gespeicherten Beobachtungen.'));
    blocks.push({h:H+12,draw:(ctx,y)=>{
      const px=ms=>L+ms/end*(R-L),py=v=>y+T+(1-v/max)*(H-T-B);
      font(ctx,13);ctx.strokeStyle=reportInk.grid;ctx.lineWidth=1;
      for(let k=0;k<=4;k++){const v=max*k/4,yy=py(v);ctx.beginPath();ctx.moveTo(L,yy);ctx.lineTo(R,yy);ctx.stroke();text(ctx,num(v),L-10,yy+4,reportInk.muted,13,'','right',80);}
      const tick=end>240000?60000:end>90000?30000:end>30000?10000:5000;
      for(let t=0;t<=end;t+=tick)text(ctx,dur(t),px(t),y+H-B+22,reportInk.muted,13,'','center',80);
      const labels=[];
      for(const s of curves.series){
        ctx.strokeStyle=s.color;ctx.lineWidth=2;ctx.lineJoin='round';ctx.beginPath();
        const step=Math.max(1,Math.ceil(s.values.length/900));
        s.values.forEach((v,k)=>{if(k%step&&k!==s.values.length-1)return;const xx=px(curves.ms[k]),yy=py(v);k?ctx.lineTo(xx,yy):ctx.moveTo(xx,yy);});ctx.stroke();
        labels.push({y:py(s.values[s.values.length-1]),s});
      }
      // Direct end labels, nudged apart so names never overlap.
      labels.sort((a,b)=>a.y-b.y);for(let k=1;k<labels.length;k++)labels[k].y=Math.max(labels[k].y,labels[k-1].y+18);
      for(const l of labels){ctx.fillStyle=l.s.color;ctx.fillRect(R+10,l.y-5,10,10);text(ctx,l.s.name,R+26,l.y+5,reportInk.text,14,'','left',W-X-R-26);}
    }});
  }
  for(const [i,p] of players.entries()) {
    const color=i<reportColors.length?reportColors[i]:reportInk.faint;
    blocks.push({h:86,draw:(ctx,y)=>{
      ctx.fillStyle=reportInk.panel;ctx.fillRect(X-16,y+14,W-2*X+32,64);ctx.fillStyle=color;ctx.fillRect(X-16,y+14,6,64);
      const headShift=drawIcon(ctx,'/assets/icons/class-'+p.class_key+'.webp',X+4,y+22,28)?38:0;
      text(ctx,`${p.name}${p.class_name?' · '+p.class_name:''}`,X+4+headShift,y+42,'#f4d8a7',22,'600',`left`,560);
      const kpis=[['Schaden',num(p.damage)],['DPS',num(p.dps)],['Heilung',num(p.heal||0)],['HPS',num(p.hps||0)],['Erlitten',num(p.damage_received||0)]];
      kpis.forEach(([k,v],j)=>{const x=640+j*144;text(ctx,v,x,y+44,reportInk.text,22,'600','left',140);text(ctx,k,x,y+66,reportInk.muted,13,'','left',140);});
    }});
    for(const [kind,skills,heal] of [['Schaden nach Skill',p.skills,false],['Heilung nach Skill',p.heal_skills,true]]) {
      if(!skills?.length)continue;
      const sum=skills.reduce((n,s)=>n+(Number(s.damage)||0),0),top=Math.max(1,...skills.map(s=>Number(s.damage)||0));
      blocks.push(heading(kind));
      for(const s of skills){
        const traits=heal?'':skillTraits(s),range=s.min>0||s.max>0?`${s.min>0?num(s.min):'—'}–${s.max>0?num(s.max):'—'}`:'';
        const detail=[range?'Min–Max '+range:'',traits].filter(Boolean).join(' · ');
        blocks.push({h:detail?46:32,draw:(ctx,y)=>{
          drawIcon(ctx,iconUrl(s),X,y+2);
          text(ctx,`${s.name}${s.is_dot&&!/\b(HoT|DoT)\b/.test(s.name)?(heal?' (HoT)':' (DoT)'):''}`,X+28,y+20,reportInk.text,16,'','left',272);
          bar(ctx,360,y+6,(Number(s.damage)||0)/top*560,18,color);
          text(ctx,`${num(s.damage)} · ${sum>0?pct(s.damage*100/sum):'—'}`,940,y+20,reportInk.text,16,'','left',200);
          text(ctx,`${s.hits??'—'} Treffer/Ticks${s.hits>0?' · Ø '+num(s.damage/s.hits):''}`,W-X,y+20,reportInk.muted,14,'','right',220);
          if(detail)text(ctx,detail,360,y+40,reportInk.muted,13,'','left',W-X-360);
        }});
      }
    }
    const buffs=reportBuffs(p.buffs);
    if(buffs.length){
      blocks.push(heading('Buff-Uptime','Zahlen in # sind Effekte ohne bekannten Namen.'));
      for(let k=0;k<buffs.length;k+=2)blocks.push({h:30,draw:(ctx,y)=>{
        buffs.slice(k,k+2).forEach((b,j)=>{
          const x=X+j*670,u=Math.min(100,Math.max(0,Number(b.uptime)||0)),unknown=/^#\d+$/.test(String(b.name));
          drawIcon(ctx,iconUrl(b),x,y+2,20);
          text(ctx,unknown?`Unbekannt ${b.name}`:b.name,x+26,y+19,unknown?reportInk.muted:reportInk.text,15,'','left',250);
          ctx.fillStyle=reportInk.grid;ctx.fillRect(x+260,y+7,300,16);bar(ctx,x+260,y+7,u*3,16,reportInk.buff);
          text(ctx,pct(b.uptime),x+572,y+19,reportInk.text,14,'','left',80);
        });
      }});
    }
  }
  return blocks;
}
async function exportPng(f,anonymous) {
  const exp=fightExport(f,anonymous),images=await reportImages(exp),blocks=reportBlocks(exp,reportCurves(f,exp.players),images);
  // Bounded canvas height. Long reports become consecutive pages; every page repeats the header.
  const MAX=4800,FOOT=50,pages=[];let page=[],h=0;
  for(const [i,b] of blocks.entries()){if(i&&h+b.h>MAX-FOOT&&page.length>1){pages.push(page);page=[blocks[0]];h=blocks[0].h;}page.push(b);h+=b.h;}
  pages.push(page);
  for(const [n,rows] of pages.entries()) {
    const canvas=document.createElement('canvas');canvas.width=1400;canvas.height=rows.reduce((s,b)=>s+b.h,0)+FOOT+20;
    const ctx=canvas.getContext('2d');ctx.fillStyle=reportInk.bg;ctx.fillRect(0,0,canvas.width,canvas.height);
    ctx.fillStyle=reportInk.accent;ctx.fillRect(0,0,canvas.width,5);
    let y=0;for(const b of rows){b.draw(ctx,y);y+=b.h;}
    ctx.fillStyle=reportInk.muted;ctx.font='14px system-ui, sans-serif';ctx.fillText(`Lokaler Export · Seite ${n+1}${pages.length>1?' von '+pages.length:''} · ${anonymous?'Namen anonymisiert':'Namen enthalten'}`,40,canvas.height-22);
    const blob=await new Promise((resolve,reject)=>canvas.toBlob(b=>b?resolve(b):reject(new Error('PNG konnte nicht erstellt werden')),'image/png'));
    download(`aion2-kampf${pages.length>1?'-'+(n+1):''}.png`,blob,'image/png');
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
