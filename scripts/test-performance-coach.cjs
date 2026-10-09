const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync(require('node:path').join(__dirname,'../web/performance-coach.js'),'utf8');
const ctx=vm.createContext({window:{},esc:s=>String(s).replaceAll('<','&lt;').replaceAll('>','&gt;'),
  observedPeak:(f,id)=>f?.analytics?.partial?null:{dps:id===1?120:100},
  api:async()=>[],detailRequest:1});
vm.runInContext(source,ctx);
const run=s=>vm.runInContext(s,ctx);
const points=(amount=100)=>Array.from({length:20},(_,i)=>({ms:(i+1)*500,damage:{1:(i+1)*amount}}));
ctx.f={id:'now',boss_name:'Boss',mob_code:25,dungeon_id:50,duration_ms:10000,
  started_at:2000,analytics:{outcome:'kill',partial:false,resolution_ms:500,points:points()},
  players:[{is_self:true,actor_id:1,name:'Me',job:'gladiator',damage:2000,dps:200,
    skills:[{code:5,damage:1200,name:'Stich'}],buffs:[{code:7,name:'Stärkung',uptime:60}]}]};
ctx.g={...ctx.f,id:'old',started_at:1000,analytics:{...ctx.f.analytics,points:points(80)},
  players:[{...ctx.f.players[0],damage:1600,dps:160,buffs:[{code:7,name:'Stärkung',uptime:50}]}]};
assert.equal(run('coachDamageActivity(f,f.players[0]).rate'),100);
assert.equal(run('coachTopSkill(f.players[0]).share'),60);
assert.equal(run('performanceCoachData(f,g).match'),true);
assert.equal(run('performanceCoachData(f,g).dps_delta'),25);
assert.equal(run('performanceCoachData(f,g).buffs[0].delta'),10);
assert.equal(run('performanceCoachData(f,g).activity_delta'),0);
assert.equal(run('performanceCoachData(f,g).outcome'),'Bestätigter Zieltod');
assert.equal(run('coachDiff(200,0)'),null);
assert.equal(run('coachDamageActivity({...f,analytics:{...f.analytics,partial:true}},f.players[0])'),null);
assert.equal(run('coachDamageActivity({...f,analytics:{...f.analytics,points:[...f.analytics.points.slice(0,2),{ms:5000,damage:{1:500}}]}},f.players[0])'),null);
ctx.g.mob_code=99;
assert.equal(run('performanceCoachData(f,g).match'),false,'other NPC with same displayed name is not comparable');
ctx.g.mob_code=25;ctx.g.analytics.outcome='unknown';
assert.equal(run('performanceCoachData(f,g).notes.some(n=>n.includes("Nicht beide"))'),true);
ctx.f.players[0].skills[0].name='<img src=x>';
assert.equal(run('coachView(performanceCoachData(f,g)).includes("<img")'),false);
console.log('Performance coach: 13 checks passed');
