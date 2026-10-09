// Conservative personal fight coach. Never interpret observed hits as casts.
function coachMetric(n) {
  return Number.isFinite(Number(n)) ? Number(n).toLocaleString('de-DE',{maximumFractionDigits:1}) : '—';
}
function coachDiff(a,b) {
  if(!Number.isFinite(Number(a))||!Number.isFinite(Number(b))||Number(b)<=0)return null;
  return (Number(a)-Number(b))*100/Number(b);
}
function coachPct(n) {
  return n==null?'—':(n>=0?'+':'−')+Math.abs(n).toFixed(1).replace('.',',')+' %';
}
// Positive cumulative-damage intervals, not casts or boss targetability.
// Unknown if samples are partial, reset, missing, or insufficiently spaced.
function coachDamageActivity(f,p) {
  const analytics=f?.analytics,points=analytics?.points,dur=Number(f?.duration_ms);
  if(f?.numeric_limited||!analytics||analytics.partial||!Array.isArray(points)||points.length<3||!Number.isFinite(dur)||dur<10000)return null;
  const resolution=Math.max(1,Number(analytics.resolution_ms)||500),limit=resolution*3;
  const id=p?.actor_id;
  if(id==null)return null;
  let previous=0,last=0,active=0,observed=0;
  for(const point of points) {
    const t=Number(point.ms),value=Number(point.damage?.[id]??0);
    if(!Number.isFinite(t)||!Number.isFinite(value)||t<=last||value<previous||t-last>limit)return null;
    if(t>dur+limit)return null;
    const dt=t-last;
    if(value>previous)active+=dt;
    observed+=dt;previous=value;last=t;
  }
  if(last<dur-limit||observed/dur<.9)return null;
  return {rate:100*active/observed,observed_ms:observed,method:'sampled_cumulative_damage'};
}
function coachSelf(f) {return (f?.players||[]).find(p=>p.is_self)||null;}
function coachTopSkill(p) {
  const valid=(p?.skills||[]).filter(s=>Number(s.damage)>0);
  const top=valid.reduce((a,b)=>Number(b.damage)>Number(a?.damage||0)?b:a,null);
  if(!top||Number(p.damage)<=0)return null;
  return {code:top.code,is_dot:!!top.is_dot,name:top.name||('#'+top.code),names:top.names,icon:top.icon,
    share:100*Number(top.damage)/Number(p.damage)};
}
function coachBuffChanges(now,prior) {
  const current=now?.buffs||[],before=prior?.buffs||[],old=new Map(before.filter(b=>Number.isFinite(Number(b.uptime))).map(b=>[String(b.code),b]));
  return current.filter(b=>old.has(String(b.code))&&Number.isFinite(Number(b.uptime)))
    .map(b=>({code:b.code,name:b.name||('#'+b.code),current:Number(b.uptime),
      previous:Number(old.get(String(b.code)).uptime),delta:Number(b.uptime)-Number(old.get(String(b.code)).uptime)}))
    .filter(b=>Number.isFinite(b.delta))
    .sort((a,b)=>Math.abs(b.delta)-Math.abs(a.delta)).slice(0,2);
}
function performanceCoachData(f,previous=null) {
  const self=coachSelf(f),old=coachSelf(previous),notes=[];
  if(!self)return {status:'unavailable',message:'Kein eigener Charakter in diesem Kampf erfasst.'};
  const quality=!!f.numeric_limited||!!f.analytics?.partial||!f.analytics?.points?.length;
  if(f.numeric_limited)notes.push('Parser-Zahlengrenze erreicht');
  if(f.analytics?.partial)notes.push('Schadensverlauf unvollständig');
  if(!f.analytics?.points?.length)notes.push('Kein vollständiger Schadensverlauf');
  const outcome=f.analytics?.outcome==='kill'?'Bestätigter Zieltod':f.analytics?.outcome==='wipe'?'Wipe registriert':'Kill nicht bestätigt';
  if(outcome!=='Bestätigter Zieltod')notes.push(outcome);
  const own=coachDamageActivity(f,self),oldActivity=coachDamageActivity(previous,old);
  const currentPeak=typeof observedPeak==='function'?observedPeak(f,self.actor_id):null;
  const previousPeak=previous&&typeof observedPeak==='function'?observedPeak(previous,old?.actor_id):null;
  const top=coachTopSkill(self);
  const match=!!(previous&&old&&!f.is_train&&!previous.is_train&&!quality&&!previous.numeric_limited
    &&f.boss_name===previous.boss_name&&f.dungeon_id===previous.dungeon_id
    &&Number(f.mob_code)>0&&Number(f.mob_code)===Number(previous.mob_code)
    &&self.name===old.name&&self.job===old.job
    &&(!(Number(self.server_id)>0&&Number(old.server_id)>0)||Number(self.server_id)===Number(old.server_id))
    &&previous.analytics?.partial===false&&!!previous.analytics?.points?.length
    &&Number(f.duration_ms)>=10000&&Number(previous.duration_ms)>=10000);
  if(previous&&!match)notes.push('Vorheriger Kampf nicht sicher vergleichbar');
  const sameOutcome=previous?.analytics?.outcome===f.analytics?.outcome && f.analytics?.outcome==='kill';
  if(match&&!sameOutcome)notes.push('Nicht beide Kämpfe als Kill bestätigt, DPS nur eingeschränkt vergleichbar');
  return {status:'ready',match,outcome,notes,
    own_dps:Number(self.dps)||0,
    previous_dps:match?Number(old.dps)||0:null,
    dps_delta:match?coachDiff(self.dps,old.dps):null,
    activity:own?.rate??null,
    activity_delta:match&&own&&oldActivity?own.rate-oldActivity.rate:null,
    current_peak:currentPeak?.dps??null,
    peak_delta:match&&currentPeak&&previousPeak?coachDiff(currentPeak.dps,previousPeak.dps):null,
    top_skill:top,buffs:match&&!f.analytics?.effects_partial&&!previous.analytics?.effects_partial?coachBuffChanges(self,old):[],
    predecessor_date:match?previous.started_at:null
  };
}
function coachView(data) {
  if(data.status!=='ready')return '<p class="analysis-note">'+esc(data.message||'Analyse nicht verfügbar')+'</p>';
  const cell=(label,value,detail)=>'<div class="coach-cell"><span>'+esc(label)+'</span><strong>'+esc(value)+'</strong><small>'+esc(detail||'')+'</small></div>';
  const relative=data.match&&data.dps_delta!=null?coachPct(data.dps_delta):'Noch kein Vergleich';
  const row=[
    cell('Eigene Kampf-DPS',num(data.own_dps)+' DPS',data.match?'Veränderung '+relative:'Erfasster Kampfwert'),
    cell('Aktive Schadenszeit',data.activity==null?'—':coachMetric(data.activity)+' %',
      data.activity_delta==null?'Nicht durchgehend beobachtet oder kein Vergleich':(data.activity_delta>=0?'+':'')+data.activity_delta.toFixed(1).replace('.',',')+' Prozentpunkte'),
    cell('Beobachteter 5s-Peak',data.current_peak==null?'—':num(data.current_peak)+' DPS',
      data.peak_delta==null?'Kein durchgehendes 5s-Vergleichsfenster':coachPct(data.peak_delta)+' zum vorigen'),
    '<div class="coach-cell"><span>Stärkster Skill</span><strong>'+esc(data.top_skill?coachMetric(data.top_skill.share)+' %':'—')+'</strong><small>'+(data.top_skill?skillLabel(data.top_skill)+(data.top_skill.is_dot?' · DoT':''):'Keine belastbaren Skillanteile')+'</small></div>'
  ].join('');
  const buffs=data.buffs.length?'<p class="analysis-note">Beobachtete Buff-Veränderung: '+data.buffs.map(b=>esc(b.name)+' '+(b.delta>=0?'+':'')+b.delta.toFixed(1).replace('.',',')+' PP').join(' · ')+'</p>':'';
  const notes=(data.notes||[]).map(x=>esc(x)).join(' · ');
  const head='<div class="skill-index-head"><div><span class="eyebrow">Dein letzter Versuch</span><h3>'+(data.match&&data.predecessor_date?'Was hat sich seit '+esc(date(data.predecessor_date))+' verändert?':'Dein Kampf in Zahlen')+'</h3></div></div>';
  return head+'<div class="coach-grid">'+row+'</div>'+buffs+
    '<p class="analysis-note">Aktive Schadenszeit: Anteil der 500-ms-Abschnitte, in denen du Schaden gemacht hast. Das ist kein Cast- oder Rotationswert und nennt keine Ursache.'+(notes?' '+notes+'.':'')+'</p>';
}
async function fillPerformanceCoach(f,root,request) {
  const apply=previous=>{
    if(request!==detailRequest||!root?.isConnected)return;
    root.innerHTML=coachView(performanceCoachData(f,previous));
  };
  const me=coachSelf(f);
  if(!me||f.is_train||f.numeric_limited||!f.boss_name||!f.mob_code||!me.name||!me.job){apply(null);return;}
  try {
    const history=await api('/api/stats/boss-history?character='+encodeURIComponent(me.name));
    if(request!==detailRequest)return;
    const group=(history||[]).find(b=>b.boss===f.boss_name&&Number(b.dungeon_id)===Number(f.dungeon_id)&&Number(b.mob_code)===Number(f.mob_code));
    const attempts=group?.attempts||[];
    const index=attempts.findIndex(a=>a.fight_id===f.id);
    if(index<0){apply(null);return;}
    const older=attempts.slice(0,index).reverse().find(a=>a.fight_id&&a.job===me.job&&[0,false].includes(a.numeric_limited)&&Number(a.dps)>0);
    if(!older){apply(null);return;}
    const previous=await api('/api/fights/'+encodeURIComponent(older.fight_id));
    apply(previous);
  }catch {apply(null);}
}
window.fillPerformanceCoach=fillPerformanceCoach;
