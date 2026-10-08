const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync(require('node:path').join(__dirname,'../web/enhancements.js'),'utf8');
const context=vm.createContext({});vm.runInContext(source.slice(0,source.indexOf('const FIGHT_PAGE')),context);
const run=s=>vm.runInContext(s,context),plain=v=>JSON.parse(JSON.stringify(v));
context.f={players:[{actor_id:1,name:'One',skills:[{code:11,hit_timestamps:[0,1000,5000,5500]},{code:11,is_dot:true,hit_timestamps:[4000,5000]}]}],analytics:{resolution_ms:500,points:Array.from({length:20},(_,i)=>({ms:(i+1)*500,damage:{1:(i+1)*100,2:(i+1)*200}}))}};
const original=JSON.stringify(context.f);
assert.equal(run('observedPeak(f,1).dps'),200);assert.equal(run('observedPeak(f).dps'),600);
assert.deepEqual(plain(run('observedPeak(f,1)')),{dps:200,start:0,end:5000,partial:false});
assert.equal(run('observedPeak({...f,analytics:{...f.analytics,points:f.analytics.points.slice(0,9)}})'),null,'warmup is not a full 5s burst');
assert.equal(run('observedPeak({...f,numeric_limited:true})'),null);
context.f.analytics.partial=true;assert.equal(run('observedPeak(f,1).start'),500);assert.equal(run('observedPeak(f,1).partial'),true);
context.g={analytics:{resolution_ms:500,points:[{ms:500,damage:{1:100}},{ms:10000,damage:{1:100000}}]}};
assert.equal(run('observedPeak(g)'),null,'gaps do not become precise burst windows');
context.g.analytics.points=Array.from({length:11},(_,i)=>({ms:i*500,damage:{1:i===5?0:100+i}}));
assert.equal(run('observedPeak(g)'),null,'counter reset invalidates the spanning window');
context.g.analytics.points[5].damage[1]=NaN;assert.equal(run('observedPeak(g)'),null);
const hits=plain(run('windowHits(f,1,0,5000)'));assert.equal(hits.hits.length,2);assert.deepEqual(hits.hits.map(h=>h.count),[2,2]);assert.ok(hits.hits.some(h=>h.skill.is_dot));
assert.equal(run('windowHits({players:[{skills:[{code:1}]}]},null,0,5000).available'),false);
assert.equal(run('windowHits(f,2,0,5000).available'),false,'actor ID controls selection');
context.attempts=[100,100,200].map(dps=>({dps,job:'cleric',numeric_limited:false}));
const perf=plain(run('bossPerformance(attempts,true)'));assert.equal(perf.change,100,'comparison excludes the current attempt');assert.equal(perf.rank,1);assert.equal(perf.gap,0);
assert.ok(run('bossPerformance(attempts,false).hint'));assert.ok(run('bossPerformance([{dps:1},{dps:2}],true).hint'));
assert.ok(run('bossPerformance([...attempts,{dps:300,job:"ranger"}],true).hint'));
assert.ok(run('bossPerformance(attempts.map(p=>({...p,job:null})),true).hint'));
assert.ok(run('bossPerformance(attempts.map(p=>({...p,job:"???"})),true).hint'));
assert.ok(run('bossPerformance(attempts.map(p=>({...p,numeric_limited:true})),true).hint'));
assert.ok(run('bossPerformance(attempts.map(p=>({...p,numeric_limited:null})),true).hint'));
context.attempts.push({dps:200,job:'cleric',numeric_limited:false});assert.equal(run('bossPerformance(attempts,true).rank'),1,'equal best values tie');
assert.equal(run('bossPerformance(attempts.map(p=>({...p,dps:0})),true).change'),null);
delete context.f.analytics.partial;assert.equal(JSON.stringify(context.f),original,'analysis preserves saved measurements');

// Local telemetry must not blend characters, targets, resets or capped values.
context.state={key:null,points:[]};context.live={target_id:9,target_started_at:1000,battle_time_ms:1000,rows:[{id:1,is_self:true,burst_dps:10}]};
assert.equal(run('appendLiveSignal(state,live).length'),1);
assert.equal(run('appendLiveSignal(state,live).length'),1,'unchanged timer is not another observation');
assert.equal(run('appendLiveSignal(state,{...live,battle_time_ms:2000}).length'),2);
assert.equal(run('appendLiveSignal(state,{...live,target_id:10}).length'),1);
assert.equal(run('appendLiveSignal(state,{...live,numeric_limited:true}).length'),0);
assert.equal(run('appendLiveSignal(state,{...live,rows:[{id:2,is_self:true,burst_dps:20}]}).length'),1);
assert.equal(run('appendLiveSignal(state,{...live,battle_time_ms:500}).length'),1);
run('for(let i=1;i<=500;i++)appendLiveSignal(state,{...live,battle_time_ms:i*1000})');
assert.ok(run('state.points.length')<=120);assert.ok(run('state.points.at(-1).ms-state.points[0].ms')<=60000);


// Personal context of a stored fight: only earlier, comparable attempts of the same boss, difficulty and character.
assert.equal(run('clock(69500)'),'1:09,5');assert.equal(run('clock(75000)'),'1:15');assert.equal(run('clock(600000)'),'10:00');
(async()=>{
  const attempt=(fight_id,dps,extra={})=>({fight_id,dps,job:'gladiator',numeric_limited:0,...extra});
  let calls=0;context.history=[{boss:'Kargos',dungeon_id:600093,attempts:[attempt('a',40000),attempt('b',50000,{numeric_limited:1}),attempt('c',44000),attempt('cur',48400),attempt('later',90000)]},
    {boss:'Kargos',dungeon_id:600092,attempts:[attempt('n',99000),attempt('cur2',10000)]}];
  context.api=async()=>{calls++;return context.history;};
  const fight=(id,extra={})=>({id,boss_name:'Kargos',dungeon_id:600093,players:[{is_self:true,name:'FeroxTOO',job:'gladiator',dps:48400}],...extra});
  context.fight=fight;
  let c=plain(await run('attemptContext(fight("cur"))'));
  assert.equal(c.count,2,'capped attempt and later attempts are not compared');assert.equal(c.mean,42000);assert.equal(c.last,44000);
  assert.ok(Math.abs(c.vsMean-100*6400/42000)<1e-9);assert.equal(c.record,true);
  assert.equal(await run('attemptContext(fight("a"))'),null,'first attempt has nothing to compare');
  assert.equal(await run('attemptContext(fight("cur",{is_train:1}))'),null,'training is never compared');
  assert.equal(await run('attemptContext(fight("cur",{numeric_limited:true}))'),null);
  assert.equal(await run('attemptContext(fight("cur",{players:[{is_self:true,name:"FeroxTOO",job:"cleric",dps:1}]}))'),null,'other class');
  assert.equal(await run('attemptContext(fight("cur2",{dungeon_id:600092}))').count,undefined);
  c=plain(await run('attemptContext(fight("cur2",{dungeon_id:600092,players:[{is_self:true,name:"FeroxTOO",job:"gladiator",dps:10000}]}))'));
  assert.equal(c.count,1,'other difficulty stays separate');assert.equal(c.best,99000);assert.equal(c.record,false);
  const before=calls;context.history=[{...context.history[0],attempts:[...context.history[0].attempts,attempt('new',60000)]},context.history[1]];await run('attemptContext(fight("new"))');assert.equal(calls,before+1,'a fight missing from the cached history refetches');
  context.num=v=>String(Math.round(v));context.attempts=[40,41,42,50,51,52].map(dps=>({dps,job:'gladiator',numeric_limited:0}));
  assert.match(run('bossStory(attempts,true).verdict'),/Aufwärtstrend/);
  context.attempts=[50,51,52,40,41,42].map(dps=>({dps,job:'gladiator',numeric_limited:0}));
  assert.match(run('bossStory(attempts,true).verdict'),/Abwärtstrend/);
  assert.equal(run('bossStory(attempts.slice(1),true).verdict'),'','fewer than six attempts give no direction');
  assert.equal(run('bossStory(attempts,false).verdict'),undefined,'no character, no verdict');
  console.log('PASS insights: complete windows, observed limits, identities, bounded live signal, attempt context and trend (58 assertions)');
})().catch(e=>{console.error(e);process.exit(1);});
