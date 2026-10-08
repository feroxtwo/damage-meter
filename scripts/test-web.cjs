// Browser regression checks. API fixtures never become part of the production UI.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const { chromium } = require('playwright');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const settings = {theme:"midnight",compact:false,idle_reset_seconds:0,wipe_reset:false,visible:true, locked:false, opacity:.72, scale:1, max_rows:8, show_dps:true, hide_names:false,pin_self:true,metric:"damage"};
const live = {
  target_id:9,target_started_at:1000,target_name:'Kargos', dungeon:'Ferocious Horn Den', character:'FeroxTOO',
  battle_time_ms:90000, total_damage:9450000, target_hp:.38, target_mode:'bossTargets', ping_ms:42,
  overlay:settings, capture:{permission:true, game_running:true, locked_port:13328, device:'eth0'},
  rows:[
    {id:1,heal:2000000,hps:22222,burst_dps:65000,name:'FeroxTOO', class_key:'cleric', class_name:'Kleriker', damage:4500000, dps:50000, share:47.6, is_self:true},
    {id:2,heal:0,hps:0,burst_dps:40000,name:'Moon', class_key:'gladiator', class_name:'Gladiator', damage:3000000, dps:33333, share:31.8},
    {id:3,heal:0,hps:0,name:'Al', class_key:'assassin', class_name:'Assassine', damage:1950000, dps:21667, share:20.6}
  ]
};
const characters = [{name:'FeroxTOO', class_name:'Kleriker', runs:12}, {name:'Alt', class_name:'Gladiator', runs:1}];
const run = {id:1, started_at:Date.now()-3600000, ended_at:Date.now()-3000000, dungeon_name:'Ferocious Horn Den', difficulty:'Schwer', fights:2, members:[{name:'FeroxTOO',is_self:1,class_key:'cleric'}], my_dps:50000, note:'Test-Run'};
const summary = {runs:12,fights:24,play_ms:7200000,partners:8,my_deaths:2,characters,per_dungeon:[{dungeon_name:run.dungeon_name,difficulty:'Schwer',runs:12,fastest_ms:600000}],my_best:[{boss_name:'Kargos',best_dps:50000,attempts:12,class_key:'cleric'}],per_day:[]};
const skill={code:11,names:{de:'Hieb',en:'Strike'},icon:'/assets/icons/skill-11170000.webp',name:'Hieb',damage:4500000,hits:10,crit_rate:50,back_rate:25,perfect_rate:10,parry_rate:0,double_rate:20,frontal_rate:30,multi_hit_count:3,max:500000,hit_timestamps:[100,500,1000]};
const fight={id:'f1',boss_name:'Kargos',dungeon_id:600093,difficulty:'Schwer',started_at:run.started_at,duration_ms:90000,total_damage:9450000,
 players:live.rows.map(r=>({...r,actor_id:r.id,job:r.class_key,skills:[{...skill,damage:r.damage}],heal_skills:r.is_self?[{...skill,name:'Heilung',names:{de:'Heilung',en:'Healing'},damage:r.heal}]:[],buffs:[{code:42,name:'Buff',uptime:50}]})),
 analytics:{resolution_ms:500,partial:false,points:[{ms:500,damage:{1:1000,2:500}},{ms:1000,damage:{1:3000,2:1000}}],effects:[{target:1,code:42,start_ms:100,end_ms:1000}]},ping_history:[{tsMs:500,pingMs:42},{tsMs:1000,pingMs:50}]};
const comparison={...fight,id:'f2',started_at:run.ended_at,players:fight.players.map(p=>({...p,dps:p.dps*.8,skills:p.skills.map(s=>({...s,damage:s.damage*.8}))}))};
const comparison2={...comparison,id:'f3',players:fight.players.map(p=>({...p,dps:p.dps*.5}))};
// Populated visual fixture: counters, final totals and rates agree; never used by the product.
function telemetryFight(id='telemetry') {
  const weights=Array.from({length:180},(_,i)=>36000+8000*Math.sin(i*.11)+35000*Math.exp(-(((i-160)/9)**2)));
  const sum=weights.reduce((a,b)=>a+b,0);let cumulative=0;
  const points=weights.map((v,i)=>{cumulative+=v;return {ms:(i+1)*500,damage:Object.fromEntries(live.rows.map(r=>[r.id,Math.round(r.damage*cumulative/sum)]))};});
  const fractions=[.314,.286,.2,.2];
  return {...fight,id,started_at:1000,analytics:{resolution_ms:500,partial:false,points},players:fight.players.map(p=>({...p,share:p.damage/fight.total_damage*100,dps:p.damage/90,skills:fractions.map((part,i)=>({...skill,code:11+i,names:i?{}:skill.names,icon:i?null:skill.icon,name:i?'Unbekannte Fähigkeit #'+(11+i):skill.name,damage:Math.round(p.damage*part),hit_timestamps:Array.from({length:30},(_,j)=>(j*3+i)*1000).filter(ms=>ms<=90000)}))}))};
}
const profiles=new Map();
let annotations=[],trainingStarts=[],fightLimits=new Set(),fightKinds=new Set(),runFavorites=[],runQueries=[];
const server = http.createServer((req,res) => {
  const route=req.url.split('?')[0];
  if(route.startsWith('/assets/icons/')){const icons=JSON.parse(fs.readFileSync(path.join(__dirname,'../data/skills/icons.json'))),entry=icons[route.split('/').pop()];if(!entry){res.writeHead(404);res.end();return;}res.setHeader('Content-Type','image/webp');res.end(fs.readFileSync(path.join(__dirname,'../data/skills/icons.bin')).subarray(entry.offset,entry.offset+entry.length));return;}
  const file=route==='/overlay'?'overlay.html':route==='/enhancements.js'?'enhancements.js':route==='/enhancements.css'?'enhancements.css':route==='/qol.js'?'qol.js':route==='/skills.js'?'skills.js':'index.html';
  res.setHeader('Content-Type',file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html; charset=utf-8');
  res.end(fs.readFileSync(path.join(__dirname,'../web',file)));
});

(async () => {
  await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch({headless:true, ...(process.env.CHROMIUM_PATH ? {executablePath:process.env.CHROMIUM_PATH} : {}), args:['--no-sandbox']});
  let checks = 0;
  async function check(name, fn) { await fn(); checks++; console.log(`PASS ${name}`); }
  const context = await browser.newContext({viewport:{width:1440,height:1000}});
  const errors = [];
  let writes = [], activeWrites = 0, maxActiveWrites = 0, failReset = false, failRuns = false;
  let slowMain = false, failSettings = false, slowSettingsRead = false, slowComparison = false, slowPlayer = false;
  await context.route('**/api/**', async route => {
    const req = route.request(), u = new URL(req.url());
    let data = {};
    if(u.pathname==='/api/skills')data=JSON.parse(fs.readFileSync(path.join(__dirname,'../data/skills/catalog.json')));
    else if(u.pathname==='/api/version')data={version:'0.3.1',parser_version:'2.0.52'};
    else if(u.pathname==='/api/update-check')data={available:true,message:'Update v0.4.0 verfügbar.',url:'https://github.com/feroxtwo/damage-meter/releases'};
    else if (u.pathname === '/api/live') data = live;
    else if(u.pathname==='/api/fights'){
      fightLimits.add(u.searchParams.get('limit'));
      fightKinds.add(u.searchParams.get('kind'));
      if(u.searchParams.get('kind')==='mob')data={fights:[{...fight,id:'m1',boss_name:'Wildschwein',difficulty:null,note:'alt',tags:'farm'}],more:false};
      else data=Number(u.searchParams.get('offset'))>=10?{fights:[{...fight,id:'f4',boss_name:'Älterer Boss'}],more:false}:{fights:[fight,comparison,comparison2],more:true};
    }
    else if(u.pathname==='/api/fights/f1')data=fight;
    else if(u.pathname==='/api/fights/f2'){if(slowComparison)await delay(350);data=comparison;}
    else if(u.pathname==='/api/fights/f3')data=comparison2;
    else if(/^\/api\/fights\/\w+\/annotation$/.test(u.pathname)){annotations.push({id:u.pathname.split('/')[3],...req.postDataJSON()});}
    else if(u.pathname.startsWith('/api/players/')){if(slowPlayer)await delay(350);data={target_id:9,start_time:1000,skills:[skill],heal_skills:[{...skill,name:'Heilung',names:{de:'Heilung',en:'Healing'}}],duration_ms:90000};}
    else if(u.pathname==='/api/training'){if(req.method()==='POST')trainingStarts.push(req.postDataJSON());data=null;}
    else if(u.pathname==='/api/overlay/profile'){
      const body=req.postDataJSON();
      if(body.save)profiles.set(body.key,{...settings});
      else Object.assign(settings,profiles.get(body.key));
      data=settings;
    }
    else if (u.pathname === '/api/characters') data = characters;
    else if (u.pathname === '/api/runs') {
      if (failRuns) { await route.fulfill({status:500,body:'error'}); return; }
      if (slowMain && u.searchParams.get('character') === 'FeroxTOO') await delay(250);
      runQueries.push(u.searchParams.get('favorites'));
      const offset = Number(u.searchParams.get('offset') || 0), limit = Number(u.searchParams.get('limit') || 50), total = 12;
      const ids = Array.from({length:Math.max(0,Math.min(limit,total-offset))},(_,i)=>offset+i+1);
      if (u.searchParams.get('favorites') === 'true') data = {runs:[{...run, id:2, favorite:1}], total:1, dungeons:[run.dungeon_name]};
      else data = {runs:ids.map(id=>({...run, id, dungeon_name:u.searchParams.get('character') === 'Alt' ? 'Alt Dungeon' : run.dungeon_name})), total, dungeons:[run.dungeon_name]};
    }
    else if (/^\/api\/runs\/\d+\/favorite$/.test(u.pathname)) { runFavorites.push({id:u.pathname.split('/')[3],...req.postDataJSON()}); data = null; }
    else if (/^\/api\/runs\/\d+$/.test(u.pathname)) data = {...run, id:Number(u.pathname.split('/').pop()), totals:[], fights:[]};
    else if (u.pathname === '/api/stats/run-history') data = [];
    else if (u.pathname === '/api/stats/summary') data = summary;
    else if (u.pathname === '/api/stats/partners') data = [{name:'Moon', class_key:'gladiator',class_name:'Gladiator',runs:10}];
    else if (u.pathname === '/api/stats/boss-history') data = [{boss:'Kargos',attempts:[{dps:40000,started_at:run.started_at,duration_ms:90000,share:40},{dps:50000,started_at:run.ended_at,duration_ms:90000,share:48}]}];
    else if (u.pathname === '/api/overlay') {
      if(req.method() === 'POST') {
        activeWrites++; maxActiveWrites = Math.max(maxActiveWrites,activeWrites);
        const body = req.postDataJSON(); writes.push(body); await delay(300);
        if(failSettings){activeWrites--;await route.fulfill({status:500,body:'error'});return;}
        Object.assign(settings,body); activeWrites--;
      }
      data = {...settings};
      if(req.method()==='GET'&&slowSettingsRead){slowSettingsRead=false;await delay(800);}
    }
    else if (u.pathname === '/api/reset' && failReset) { await route.fulfill({status:500,body:'error'}); return; }
    await route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(data)});
  });
  const page = await context.newPage(); page.on('pageerror',e=>errors.push(e.message));
  try {
    await page.goto(base);
    await page.waitForFunction(() => document.querySelector('#groupDps').textContent === '105,0K');
    await check('live metrics and character rows',async()=>{
      assert.equal(await page.locator('#selfDps').textContent(),'50,0K');
      assert.equal(await page.locator('#battleDuration').textContent(),'1:30');
      assert.equal(await page.locator('#liveRows .bar').count(),3);
    });
    if(process.env.SCREENSHOT_DIR) { fs.mkdirSync(process.env.SCREENSHOT_DIR,{recursive:true}); await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'dashboard-desktop.png'),fullPage:true}); }
    await check('unknown URL tab falls back to live',async()=>{
      await page.goto(base+'/#unknown'); await page.locator('#live.active').waitFor();
    });
    await page.getByRole('button',{name:'Runs',exact:true}).click();
    await page.locator('#runRows tr.click').first().waitFor();
    if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'runs-desktop.png'),fullPage:true});
    await check('run history shows ten runs per page and pages without skipping',async()=>{
      assert.equal(await page.locator('#runRows tr.click').count(),10);
      assert.equal(await page.locator('#runPage').textContent(),'1–10 von 12');
      assert.ok(await page.locator('#runPrev').isDisabled());
      await page.locator('#runNext').evaluate(b=>{b.click();b.click();});
      await page.waitForFunction(()=>document.querySelector('#runPage').textContent==='11–12 von 12');
      assert.deepEqual(await page.locator('#runRows tr.click').evaluateAll(rows=>rows.map(r=>r.dataset.id)),['11','12']);
      assert.ok(await page.locator('#runNext').isDisabled());
      await page.locator('#runRows tr.click').first().click();
      await page.locator('#backBtn').click();
      await page.waitForFunction(()=>document.querySelector('#runPage').textContent==='11–12 von 12');
      await page.locator('#runPrev').click();
      await page.waitForFunction(()=>document.querySelector('#runPage').textContent==='1–10 von 12');
    });
    await check('runs can be starred in the list and filtered to favorites',async()=>{
      const star=page.locator('#runRows [data-fav-run="2"]');
      assert.equal(await star.getAttribute('aria-pressed'),'false');
      await star.click();
      await page.waitForFunction(()=>document.querySelector('#runRows [data-fav-run="2"]').getAttribute('aria-pressed')==='true');
      assert.deepEqual(runFavorites.at(-1),{id:'2',favorite:true});
      assert.ok(await page.locator('#runDetail').isHidden());
      await page.locator('#runFavorites').check();
      await page.waitForFunction(()=>document.querySelectorAll('#runRows tr.click').length===1);
      assert.equal(runQueries.at(-1),'true');
      assert.ok(await page.locator('#runPager').isHidden());
      await page.locator('#runFavorites').uncheck();
      await page.waitForFunction(()=>document.querySelectorAll('#runRows tr.click').length===10);
      assert.equal(runQueries.at(-1),'false');
    });
    await check('fight library pages separately from the run history',async()=>{
      await page.waitForFunction(()=>document.querySelector('#fightPage').textContent==='Seite 1 · 1–3');
      await page.locator('#fightNext').click();
      await page.waitForFunction(()=>document.querySelector('#fightResults').textContent.includes('Älterer Boss'));
      assert.equal(await page.locator('#fightPage').textContent(),'Seite 2 · 11–11');
      assert.ok(await page.locator('#fightNext').isDisabled());
      assert.equal(await page.locator('#runPage').textContent(),'1–10 von 12');
      await page.locator('#fightPrev').click();
      await page.waitForFunction(()=>document.querySelector('#fightPage').textContent==='Seite 1 · 1–3');
      assert.deepEqual([...fightLimits],['10']);
      assert.deepEqual([...fightKinds].sort(),['boss','mob']);
    });
    await check('world mobs sit in their own list and fights can be starred in place',async()=>{
      assert.match(await page.locator('#mobResults').textContent(),/Wildschwein/);
      assert.doesNotMatch(await page.locator('#fightResults').textContent(),/Wildschwein/);
      assert.ok(await page.locator('#mobPager').isHidden());
      const star=page.locator('#mobResults [data-fav-fight="m1"]');
      assert.equal(await star.getAttribute('aria-pressed'),'false');
      await star.click();
      await page.waitForFunction(()=>document.querySelector('#mobResults [data-fav-fight="m1"]').getAttribute('aria-pressed')==='true');
      assert.deepEqual(annotations.at(-1),{id:'m1',favorite:true,note:'alt',tags:'farm'});
      assert.equal(await star.textContent(),'★');
      assert.equal(await page.locator('#fightDialog[open]').count(),0);
    });
    await check('late character requests cannot overwrite current filter',async()=>{
      slowMain=true;
      await page.selectOption('#charSel','FeroxTOO');
      await page.selectOption('#charSel','Alt');
      await page.waitForFunction(()=>document.querySelector('#runRows').textContent.includes('Alt Dungeon'));
      await delay(400);
      assert.match(await page.locator('#runRows').textContent(),/Alt Dungeon/);
    });
    await check('API failure shows an actionable message',async()=>{
      failRuns=true; await page.selectOption('#charSel','FeroxTOO');
      await page.waitForFunction(()=>document.querySelector('#runRows').textContent.includes('nicht geladen'));
      await page.locator('#toast.error').waitFor(); failRuns=false;
      failReset=true; await page.getByRole('button',{name:'Live',exact:true}).click();
      await page.locator('#resetBtn').click();
      await page.waitForFunction(()=>document.querySelector('#toast').textContent.includes('Aktion fehlgeschlagen'));
    });
    await page.getByRole('button',{name:'Overlay',exact:true}).click();
    await page.waitForFunction(()=>document.querySelector('[data-for="scale"]').value==='1.00×');
    if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'overlay-settings.png'),fullPage:true});
    await check('settings writes stay ordered during rapid edits',async()=>{
      await page.locator('[data-k="scale"]').evaluate(i=>{i.value='1.5';i.dispatchEvent(new Event('input'));});
      await delay(210);
      await page.locator('[data-k="scale"]').evaluate(i=>{i.value='2';i.dispatchEvent(new Event('input'));});
      await page.waitForFunction(()=>document.querySelector('[data-for="scale"]').value==='2.00×');
      await delay(900);
      assert.equal(settings.scale,2); assert.equal(maxActiveWrites,1); assert.equal(writes.length,2);
    });
    await check('setting edits send only changed fields so a dragged overlay keeps its position',async()=>{
      settings.position=[700,300];settings.locked=true;
      const before=writes.length;
      await page.locator('[data-k="opacity"]').evaluate(i=>{i.value='0.5';i.dispatchEvent(new Event('input'));});
      await page.waitForFunction(()=>document.querySelector('#settingsStatus').textContent==='Einstellungen gespeichert.');
      assert.deepEqual(writes.slice(before),[{opacity:0.5}]);
      assert.deepEqual(settings.position,[700,300]);assert.equal(settings.locked,true);
      settings.locked=false;delete settings.position;
    });
    await check('profile actions flush pending settings and cannot be overwritten by them',async()=>{
      await page.fill('#profileName','Test');
      await page.locator('[data-k="scale"]').evaluate(i=>{i.value='1.25';i.dispatchEvent(new Event('input'));document.querySelector('#saveProfile').click();});
      await page.waitForFunction(()=>document.querySelector('#toast').textContent==='Profil gespeichert.');
      assert.equal(profiles.get('Test').scale,1.25);
      await page.locator('[data-k="scale"]').evaluate(i=>{i.value='2';i.dispatchEvent(new Event('input'));document.querySelector('#loadProfile').click();});
      await page.waitForFunction(()=>document.querySelector('#toast').textContent==='Profil geladen.');
      await delay(250);
      assert.equal(settings.scale,1.25);
      assert.equal(await page.locator('[data-k="scale"]').inputValue(),'1.25');
      assert.equal(maxActiveWrites,1);
    });
    await check('failed setting writes retain edits and can be retried without another edit',async()=>{
      failSettings=true;
      await page.locator('[data-k="scale"]').evaluate(i=>{i.value='1.75';i.dispatchEvent(new Event('input'));});
      await page.locator('#retrySettings').waitFor({state:'visible'});
      assert.equal(settings.scale,1.25);
      await page.setViewportSize({width:320,height:844});
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
      if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'settings-save-error.png'),fullPage:true});
      await page.setViewportSize({width:1440,height:1000});
      await page.evaluate(()=>loadSettings());
      assert.equal(await page.locator('[data-k="scale"]').inputValue(),'1.75');
      failSettings=false;
      await page.locator('#retrySettings').click();
      await page.waitForFunction(()=>document.querySelector('#settingsStatus').textContent==='Einstellungen gespeichert.');
      assert.equal(settings.scale,1.75);
      await page.locator('#retrySettings').waitFor({state:'hidden'});
    });
    await check('late setting reads cannot replace a newly saved profile',async()=>{
      profiles.set('Test',{...settings,scale:1.1});
      slowSettingsRead=true;
      const stale=page.evaluate(()=>loadSettings());
      await delay(50);
      await page.locator('#loadProfile').click();
      await page.waitForFunction(()=>document.querySelector('#toast').textContent==='Profil geladen.');
      await stale;
      assert.equal(await page.locator('[data-k="scale"]').inputValue(),'1.1');
    });
    await check('mobile tabs have no page overflow',async()=>{
      await page.setViewportSize({width:390,height:844});
      for(const width of [320,390]) {
      await page.setViewportSize({width,height:844});
      for(const tab of ['Live','Runs','Statistik','Fähigkeiten','Overlay']) {
        await page.getByRole('button',{name:tab,exact:true}).click(); await delay(100);
        assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,tab);
      }
      }
      await page.getByRole('button',{name:'Live',exact:true}).click();
      await page.evaluate(() => { document.querySelector('#toast').hidden = true; window.scrollTo(0,0); });
      if(process.env.SCREENSHOT_DIR) await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'dashboard-mobile.png'),fullPage:true});
    });
    await check('OBS overlay respects privacy and display settings',async()=>{
      Object.assign(settings,{hide_names:true,show_dps:false,max_rows:2,scale:1.5,opacity:.4});
      live.rows[1].name='Al';
      const overlay=await context.newPage();overlay.on('pageerror',e=>errors.push(e.message));
      await overlay.goto(base+'/overlay'); await overlay.locator('#rows .row').first().waitFor();
      assert.equal(await overlay.locator('#rows .row').count(),2);
      const text=await overlay.locator('#rows').textContent();
      assert.match(text,/FeroxTOO/); assert.match(text,/\*\*/); assert.ok(!text.includes('Al'));assert.ok(!text.includes('/s'));
      assert.equal(await overlay.locator('#box').evaluate(e=>e.style.transform),'scale(1.5)');
      settings.visible=false; await overlay.locator('#box').waitFor({state:'hidden'});
      settings.visible=true;settings.hide_names=false;settings.max_rows=3;live.rows[1].name='<img src=x onerror=alert(1)>';
      await overlay.locator('#box').waitFor({state:'visible'});
      await overlay.waitForFunction(()=>document.querySelector('#rows').textContent.includes('<img'));
      const icons=await overlay.locator('#rows img').evaluateAll(es=>es.map(e=>({src:e.getAttribute('src'),handler:e.hasAttribute('onerror'),alt:e.getAttribute('alt')})));assert.equal(icons.length,3);assert.ok(icons.every(i=>/^\/assets\/icons\/class-(cleric|gladiator|assassin)\.webp$/.test(i.src)&&!i.handler&&i.alt===''));await overlay.close();
    });
    await check('healing, live skill details and training controls',async()=>{
      await page.setViewportSize({width:1440,height:1000});
      await page.selectOption('#liveMetric','heal');
      assert.match(await page.locator('#liveRows').textContent(),/2,00M/);
      await page.locator('#liveRows .bar').first().click();
      await page.locator('#fightDialog[open]').waitFor();
      assert.match(await page.locator('#fightContent').textContent(),/Double/);
      assert.match(await page.locator('#fightContent').textContent(),/65,0K/);
      await page.locator('#closeDialog').click();
      await page.locator('#trainingDuration input[value="180"]').check();await page.locator('#startTraining').click();
      await delay(100);assert.deepEqual(trainingStarts.at(-1),{seconds:180});
      await page.selectOption('#liveMetric','damage');
    });
    await check('Escape cancels an in-flight player refresh without reopening the dialog',async()=>{
      await page.locator('#liveRows .bar').first().click();
      await page.locator('#fightDialog[open]').waitFor();
      slowPlayer=true;
      await page.locator('#refreshPlayer').click();
      await page.keyboard.press('Escape');
      await delay(450);
      assert.equal(await page.locator('#fightDialog').evaluate(d=>d.open),false);
      slowPlayer=false;
    });
    await check('copied rankings use the displayed metric order, values and privacy setting',async()=>{
      const oldHeal=live.rows[1].heal,oldHps=live.rows[1].hps;
      live.rows[1].heal=3000000;live.rows[1].hps=33333;live.rows[1].damage_received=90000;
      await page.waitForFunction(()=>latestLive.rows[1].heal===3000000);
      await page.evaluate(()=>{window.copyText=async text=>{window.copiedRanking=text;};});
      await page.locator('#anonymousExport').check();
      await page.selectOption('#liveMetric','heal');await page.locator('#copyLive').click();
      let text=await page.evaluate(()=>window.copiedRanking);
      assert.match(text.split('\n')[1],/Spieler 1 \| 3,00M Heilung \| 33,3K HPS/);
      assert.ok(text.includes('FeroxTOO')&&!text.includes('Moon'),'own character stays named, others are anonymized');
      await page.locator('#anonymousExport').uncheck();
      await page.selectOption('#liveMetric','damage_received');await page.locator('#copyLive').click();
      text=await page.evaluate(()=>window.copiedRanking);
      assert.match(text.split('\n')[1],/90,0K Erlittener Schaden \| 1,0K pro Sekunde/);
      await page.selectOption('#liveMetric','damage');await page.locator('#copyLive').click();
      assert.match((await page.evaluate(()=>window.copiedRanking)).split('\n')[1],/FeroxTOO \| 4,50M Schaden \| 50,0K DPS/);
      live.rows[1].heal=oldHeal;live.rows[1].hps=oldHps;live.rows[1].damage_received=0;
      await page.locator('#anonymousExport').check();
    });
    await check('fight search, comparison, notes and timeline render',async()=>{
      await page.getByRole('button',{name:'Runs',exact:true}).click();
      await page.locator('[data-open-fight="f1"]').click();
      await page.locator('#fightDialog[open]').waitFor();
      await page.locator('#compareFight option[value="f2"]').waitFor({state:'attached'});
      await page.selectOption('#compareFight','f2');await page.locator('#compareBtn').click();
      await page.waitForFunction(()=>document.querySelector('#comparison').textContent.includes('10,0K'));
      assert.match(await page.locator('#comparison').textContent(),/25.0%/);
      assert.equal(await page.locator('#fightDamageChart svg.curve-chart').count(),1);
      assert.equal(await page.locator('svg[aria-label="ms Ping"]').count(),1);
      await page.locator('#favoriteFight').check();await page.fill('#fightNote','neues Gear');await page.fill('#fightTags','rotation');await page.locator('#saveFightNote').click();
      await delay(100);assert.deepEqual(annotations.at(-1),{id:'f1',favorite:true,note:'neues Gear',tags:'rotation'});
      if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'combat-analysis.png'),fullPage:true});
    });
    await check('late comparisons cannot replace the last chosen fight',async()=>{
      slowComparison=true;
      await page.selectOption('#compareFight','f2');await page.locator('#compareBtn').click();
      await page.selectOption('#compareFight','f3');await page.locator('#compareBtn').click();
      await page.waitForFunction(()=>document.querySelector('#comparison').textContent.includes('25,0K'));
      await delay(450);
      assert.match(await page.locator('#comparison').textContent(),/25,0K/);
      slowComparison=false;
    });
    await check('anonymous JSON and CSV exports do not leak names or formulas',async()=>{
      const exp=await page.evaluate(()=>fightExport(currentFight,true));
      assert.equal(exp.players[0].name,'FeroxTOO','own character stays named');assert.ok(!JSON.stringify(exp).includes('Moon'));assert.equal(exp.players[1].name,'Spieler 2');
      assert.equal(await page.evaluate(()=>csvCell('=HYPERLINK("evil")')),'"\'=HYPERLINK(""evil"")"');
      const [download]=await Promise.all([page.waitForEvent('download'),page.locator('#csvFight').click()]);
      assert.equal(download.suggestedFilename(),'aion2-kampf.csv');
      await page.setViewportSize({width:320,height:844});
      assert.equal(await page.evaluate(()=>document.querySelector('#fightDialog').scrollWidth<=document.querySelector('#fightDialog').clientWidth),true);
      await page.locator('#closeDialog').click();
      await page.getByRole('button',{name:'Live',exact:true}).click();
    });
    await check('OBS pins the local player with their actual rank',async()=>{
      Object.assign(settings,{visible:true,hide_names:false,metric:'damage',max_rows:1,pin_self:true});
      const original=live.rows[0].damage;live.rows[0].damage=1;
      const overlay=await context.newPage();overlay.on('pageerror',e=>errors.push(e.message));
      await overlay.goto(base+'/overlay');await overlay.locator('#rows .row').waitFor();
      assert.equal(await overlay.locator('#rows .row.me .rk').textContent(),'03');assert.match(await overlay.locator('#rows .row.me').textContent(),/FeroxTOO/);
      live.rows[0].damage=original;await overlay.close();
    });
    await check('appearance, idle reset and update controls persist with settings',async()=>{
      await page.getByRole('button',{name:'Overlay',exact:true}).click();
      await page.selectOption('[data-k="theme"]','ember');await page.locator('[data-k="compact"]').check();
      await page.selectOption('[data-k="idle_reset_seconds"]','30');await page.locator('[data-k="wipe_reset"]').check();
      await delay(900);assert.equal(settings.theme,'ember');assert.equal(settings.compact,true);assert.equal(settings.idle_reset_seconds,30);
      await page.waitForFunction(()=>document.documentElement.dataset.theme==='ember');
      await page.locator('#checkUpdate').click();await page.waitForFunction(()=>document.querySelector('#updateInfo').textContent.includes('v0.4.0'));
      assert.equal(await page.locator('#updateInfo a').getAttribute('href'),'https://github.com/feroxtwo/damage-meter/releases');
      await page.getByRole('button',{name:'Live',exact:true}).click();
    });
    await check('live player pinning and escaping target changes',async()=>{
      await page.locator('#liveRows .bar').first().click();await page.locator('#fightDialog[open]').waitFor();
      await page.locator('#livePairButton').click();await page.locator('#livePairResult .player-pair').waitFor();
      assert.equal(await page.locator('#livePairResult .player-pair>div').count(),2);
      live.target_started_at=2000;await page.waitForFunction(()=>latestLive.target_started_at===2000);
      await page.locator('#livePairButton').click();await page.waitForFunction(()=>document.querySelector('#livePairResult').textContent.includes('geändert'));
      live.target_started_at=1000;
      await page.locator('#closeDialog').click();
    });
    await check('pair comparison, all-player chart and PNG report include hidden skill rows',async()=>{
      await page.evaluate(()=>openFight('f1'));await page.locator('#pairCompare').click();
      assert.equal(await page.locator('#playerPair .player-pair>div').count(),2);
      await page.selectOption('#fightChartScope','players');
      assert.equal(await page.locator('svg[aria-label="DPS-Verlauf aller Spieler"] path[stroke-width="2"]').count(),3);
      assert.deepEqual(await page.evaluate(()=>reportBuffs([{name:'Wachtschild',uptime:11.5},{name:'Wachtschild',uptime:9},{name:'Fury',uptime:99}]).map(b=>b.name+' '+b.uptime)),['Fury 99','Wachtschild 11.5']);
      assert.equal(await page.evaluate(()=>reportCurves({players:[{actor_id:1}],analytics:{points:[{ms:500,damage:{1:0}},{ms:1000,damage:{1:500}},{ms:1500,damage:{1:1000}}]}},[{name:'A'}]).series[0].values[2]),2000/3);
      await page.locator('#anonFight').check();
      assert.equal(await page.locator('#exportScope').inputValue(),'','exports default to the whole group');
      const pngTexts=async value=>{await page.selectOption('#exportScope',value);return page.evaluate(async actor=>{
        const original=CanvasRenderingContext2D.prototype.fillText,texts=[],keep=window.download;window.download=()=>{};
        CanvasRenderingContext2D.prototype.fillText=function(text,...args){texts.push(text);return original.call(this,text,...args);};
        try{await exportPng(currentFight,document.querySelector('#anonFight').checked,actor||null);}finally{CanvasRenderingContext2D.prototype.fillText=original;window.download=keep;}
        return texts;},value);};
      let texts=await pngTexts('1');
      assert.equal(texts.filter(t=>t==='Schaden nach Skill').length,1);assert.ok(texts.some(t=>t.startsWith('FeroxTOO')));assert.ok(!texts.some(t=>t.includes('Moon')));
      await page.locator('#anonFight').uncheck();texts=await pngTexts('2');
      assert.equal(texts.filter(t=>t==='Schaden nach Skill').length,1);assert.ok(texts.some(t=>t.startsWith('Moon · ')));
      texts=await pngTexts('');assert.equal(texts.filter(t=>t==='Schaden nach Skill').length,3);
      await page.selectOption('#exportScope','2');
      const [single]=await Promise.all([page.waitForEvent('download'),page.locator('#jsonFight').click()]);
      assert.equal(single.suggestedFilename(),'aion2-kampf-spieler-2.json');const one=JSON.parse(fs.readFileSync(await single.path(),'utf8'));
      assert.equal(one.players.length,1);assert.equal(one.players[0].name,'Moon');
      await page.locator('#anonFight').check();
      const [csv]=await Promise.all([page.waitForEvent('download'),page.locator('#csvFight').click()]);
      const csvText=fs.readFileSync(await csv.path(),'utf8');assert.match(csvText,/"Spieler 2"/);assert.ok(!csvText.includes('Moon'));assert.match(csvText,/"Skill";"Art"/);assert.ok(!csvText.includes('FeroxTOO'));
      await page.locator('#copyFight').click();
      assert.equal((await page.evaluate(()=>window.copiedRanking)).split('\n').length,2);
      await page.selectOption('#exportScope','');
      const [png]=await Promise.all([page.waitForEvent('download'),page.locator('#pngFight').click()]);
      const file=await png.path();const bytes=fs.readFileSync(file);assert.equal(bytes.subarray(1,4).toString(),'PNG');assert.ok(bytes.length>5000);
      if(process.env.SCREENSHOT_DIR) {await page.setViewportSize({width:1440,height:1000});await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'player-comparison.png'),fullPage:true});fs.copyFileSync(file,path.join(process.env.SCREENSHOT_DIR,'fight-report.png'));}
      await page.selectOption('#pairB','1');await page.locator('#pairCompare').click();assert.match(await page.locator('#playerPair').textContent(),/unterschiedliche/);
      await page.locator('#closeDialog').click();
    });
    await check('PNG anonymization and page splitting retain all skill rows',async()=>{
      const downloads=[];const receive=d=>downloads.push(d);page.on('download',receive);
      await page.evaluate(async()=>{
        const original=CanvasRenderingContext2D.prototype.fillText;window.pngTexts=[];
        CanvasRenderingContext2D.prototype.fillText=function(text,...args){window.pngTexts.push(text);return original.call(this,text,...args);};
        try {const f={...fightExport({boss_name:'Boss',duration_ms:90000,players:[]},true),boss_name:'Boss',players:[{name:'PrivateName',skills:Array.from({length:200},(_,i)=>({name:'Skill '+i,damage:100,hits:1})),heal_skills:[],buffs:[]}]};await exportPng(f,true);}
        finally{CanvasRenderingContext2D.prototype.fillText=original;}
      });
      await delay(200);page.off('download',receive);assert.equal(downloads.length,2);
      const text=await page.evaluate(()=>pngTexts.join(' '));assert.ok(!text.includes('PrivateName'));assert.match(text,/Skill 0\b/);assert.match(text,/Skill 199/);assert.match(text,/Spieler 1/);assert.match(text,/Seite 2 von 2/);
      assert.equal(downloads[1].suggestedFilename(),'aion2-kampf-2.png');
    });
    await check('chat output is one line, bounded, ordered and anonymized',async()=>{
      const line=await page.evaluate(()=>chatLine('Boss\nwith separator |',exportPlayers(metricRows(latestLive.rows),true),90000));
      assert.ok(!line.includes('\n'));assert.ok(Array.from(line).length<=200);assert.ok(!line.includes('Moon'));assert.match(line,/FeroxTOO/);assert.match(line,/Spieler 2/);
    });
    await check('missing hit-quality values render dashes rather than fake zero rates',async()=>{
      const html=await page.evaluate(()=>skillTable([{name:'Skill',damage:100,hits:1,crit_rate:null,block_rate:null}]));
      assert.ok(html.includes('—'));assert.equal(await page.evaluate(html=>{const doc=new DOMParser().parseFromString(html,'text/html');return doc.querySelector('tbody tr').children[6].textContent;},html),'—');assert.match(html,/keine Skill-Aktivierungen/);
    });
    await check('skill search sorting optional columns and exact values',async()=>{
      await page.evaluate(()=>{
        document.querySelector('#fightContent').innerHTML=skillTable([{name:'Zed',damage:100,hits:2,average:50,min:25,max:75,crit_rate:50},{name:'Alpha',damage:200,hits:4,average:50,min:40,max:60,crit_rate:null}]);
        bindSkillTables(document.querySelector('#fightContent'));document.querySelector('#fightDialog').showModal();
      });
      const box=page.locator('.skill-browser');
      assert.equal(await box.locator('.skill-advanced').first().isVisible(),false);
      await box.locator('.skill-extra').check();assert.equal(await box.locator('.skill-advanced').first().isVisible(),true);
      await box.locator('.skill-sort').selectOption('name');assert.match(await box.locator('tbody tr').first().textContent(),/^Alpha/);
      await box.locator('.skill-search').fill('zed');assert.equal(await box.locator('tbody tr:visible').count(),1);
      assert.equal(await box.locator('tbody tr:visible td').nth(1).getAttribute('title'),'100');
      await box.locator('.skill-search').fill('no matches');await box.locator('.skill-empty').waitFor();
      await box.locator('.skill-search').fill('');
      const first=()=>box.locator('tbody tr').first().locator('td').first().textContent();
      await box.locator('.sort-head[data-sort="crit_rate"]').click();assert.equal(await first(),'Zed');
      assert.equal(await box.locator('th:has([data-sort="crit_rate"])').getAttribute('aria-sort'),'descending');
      await box.locator('.sort-head[data-sort="max"]').click();assert.equal(await first(),'Zed');
      await box.locator('.sort-head[data-sort="max"]').click();assert.equal(await first(),'Alpha');
      assert.equal(await box.locator('th:has([data-sort="max"])').getAttribute('aria-sort'),'ascending');
      await page.locator('#closeDialog').click();
    });
    await check('live focus survives refresh and metric cards follow healing',async()=>{
      await page.getByRole('button',{name:'Live',exact:true}).click();await page.locator('#liveRows .bar').first().focus();
      const original=live.rows[0].damage;live.rows[0].damage+=1;
      await page.waitForFunction(()=>latestLive.rows[0].damage===4500001);await delay(100);
      assert.equal(await page.evaluate(()=>document.activeElement.dataset.playerId),'1');
      await page.selectOption('#liveMetric','heal');
      assert.match(await page.locator('#groupDpsLabel').textContent(),/HPS/);
      assert.equal(await page.locator('#totalDamageLabel').textContent(),'Erfasste Heilung');
      assert.equal(await page.locator('#totalDamage').textContent(),'2,00M');
      await page.selectOption('#liveMetric','damage');live.rows[0].damage=original;
    });
    await check('large charts and lazy details stay bounded across themes and DPI',async()=>{
      const count=await page.evaluate(()=>{
        const points=Array.from({length:100000},(_,i)=>({ms:i*500,v:i}));
        const html=svgCurve(points,p=>p.v,'large test');const d=new DOMParser().parseFromString(html,'text/html');
        return (d.querySelector('path[stroke-width="2"]').getAttribute('d').match(/[ML]/g)||[]).length;
      });assert.ok(count<=601);
      await page.evaluate(()=>openFight('f1'));assert.equal(await page.locator('.player-analysis .skill-browser').count(),0);
      await page.locator('.player-report summary').first().click();await page.locator('.player-analysis .skill-browser').first().waitFor();
      if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'skill-details.png'),fullPage:true});
      await page.locator('#closeDialog').click();
      for(const theme of ['midnight','aether','ember']) {
        await page.evaluate(theme=>applyAppearance({theme,compact:true}),theme);
        await page.setViewportSize({width:320,height:844});
        assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth),true);
        if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'theme-'+theme+'-mobile.png'),fullPage:true});
      }
      const dpi=await browser.newContext({viewport:{width:1440,height:900},deviceScaleFactor:2});
      // Reuse the same synthetic API interception on a second scaled page.
      await dpi.route('**/api/**',route=>route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(route.request().url().endsWith('/api/live')?live:route.request().url().endsWith('/api/overlay')?settings:[])}));
      const high=await dpi.newPage();await high.goto(base);await high.locator('#liveRows .bar').first().waitFor();
      assert.equal(await high.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth),true);await dpi.close();
      await page.setViewportSize({width:1440,height:1000});
    });
    await check('damage chart scope, smoothing, totals and keyboard selection use saved samples',async()=>{
      await page.evaluate(()=>openFight('f1'));
      await page.locator('#fightDamageChart').waitFor();
      const readout=page.locator('[data-curve-readout]');
      assert.match(await readout.textContent(),/4,0K DPS/);
      await page.selectOption('#fightChartWindow','0');assert.match(await readout.textContent(),/5,0K DPS/);
      await page.selectOption('#fightChartMetric','total');assert.match(await readout.textContent(),/4,0K Schaden/);
      assert.equal(await page.locator('#fightChartWindow').isDisabled(),true);
      await page.selectOption('#fightChartScope','1');assert.match(await readout.textContent(),/FeroxTOO.*3,0K Schaden/);
      await page.selectOption('#fightChartMetric','dps');assert.match(await readout.textContent(),/4,0K DPS/);
      const range=page.locator('[data-curve-range]');await range.focus();await page.keyboard.press('ArrowLeft');
      assert.equal(await range.inputValue(),'0');assert.match(await readout.textContent(),/0,5 s.*2,0K DPS/);
      assert.match(await range.getAttribute('aria-valuetext'),/0.5 Sekunden; FeroxTOO/);
      await page.selectOption('#fightChartScope','players');
      const lines=page.locator('#fightDamageChart path[stroke-width="2"]'),legend=page.locator('.legend-toggle');
      assert.equal(await lines.count(),3);await legend.nth(1).click();assert.equal(await lines.count(),2);
      assert.equal(await legend.nth(1).getAttribute('aria-pressed'),'false');
      await page.selectOption('#fightChartWindow','5000');assert.equal(await lines.count(),2);
      await legend.nth(0).click();await legend.nth(2).click();assert.equal(await lines.count(),0);
      assert.match(await page.locator('[data-curve-plot]').textContent(),/Alle Linien ausgeblendet/);
      await legend.nth(0).click();assert.equal(await lines.count(),1);
      await page.locator('#closeDialog').click();
    });
    await check('damage chart handles partial, absent and single-sample histories on narrow screens',async()=>{
      await page.evaluate(()=>{
        const f={duration_ms:10500,players:[{actor_id:1,name:'Langer Spielername '+('W'.repeat(80))}],analytics:{partial:true,points:[{ms:10000,damage:{1:1000000}},{ms:10500,damage:{1:1000100}}]}};
        document.querySelector('#fightContent').innerHTML=fightChartControls(f);document.querySelector('#fightDialog').showModal();bindFightChart(f);
      });
      assert.match(await page.locator('[data-curve-readout]').textContent(),/200 DPS/);
      assert.match(await page.locator('[data-curve-note]').textContent(),/unvollständig/);
      await page.setViewportSize({width:320,height:844});
      assert.equal(await page.locator('#fightDialog').evaluate(e=>e.scrollWidth<=e.clientWidth),true);
      // Resolve and hit the plot in one step; a resize redraw may replace the SVG in between.
      await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
      await page.evaluate(()=>{const hit=document.querySelector('#fightDamageChart .curve-hit'),r=hit.getBoundingClientRect();hit.dispatchEvent(new PointerEvent('pointerdown',{clientX:r.x+r.width/2,bubbles:true}));});
      assert.match(await page.locator('[data-curve-readout]').textContent(),/200 DPS/);
      await page.evaluate(()=>{
        const f={duration_ms:1000,players:[{actor_id:1,name:'One'}],analytics:{points:[{ms:500,damage:{1:50}}]}};
        document.querySelector('#fightContent').innerHTML=fightChartControls(f);bindFightChart(f);
      });
      assert.equal(await page.locator('#fightDamageChart circle').count(),1);assert.match(await page.locator('[data-curve-readout]').textContent(),/100 DPS/);
      await page.evaluate(()=>{
        const f={duration_ms:1000,players:[{actor_id:1,name:'Old fight'}]};document.querySelector('#fightContent').innerHTML=fightChartControls(f);bindFightChart(f);
      });
      assert.equal(await page.locator('[data-curve-range]').isDisabled(),true);assert.match(await page.locator('[data-curve-readout]').textContent(),/Keine zeitliche/);
      assert.ok(!(await page.locator('#fightDamageChart').innerHTML()).includes('NaN'));
      await page.locator('#closeDialog').click();await page.setViewportSize({width:1440,height:1000});
    });
    await check('dense multi-player curves keep full sample selection and fit mobile dialogs',async()=>{
      await page.evaluate(()=>{
        const players=Array.from({length:6},(_,i)=>({actor_id:i+1,name:['FeroxTOO','Moon','Al','Support mit langem Namen','Ranger','Tank'][i]}));
        const totals=players.map(()=>0),points=Array.from({length:240},(_,i)=>{const damage={};for(let j=0;j<players.length;j++){totals[j]+=Math.round((12000+j*2500)*(1+Math.sin(i/12+j)*.6));damage[j+1]=totals[j];}return {ms:(i+1)*500,damage};});
        const f={duration_ms:120000,players,analytics:{resolution_ms:500,points}};
        document.querySelector('#fightContent').innerHTML=fightChartControls(f);document.querySelector('#fightDialog').showModal();bindFightChart(f);
      });
      await page.selectOption('#fightChartScope','players');assert.equal(await page.locator('#fightDamageChart path[stroke-width="2"]').count(),6);
      assert.equal(await page.locator('[data-curve-range]').getAttribute('max'),'239');
      if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'damage-curve-desktop.png'),fullPage:true});
      await page.setViewportSize({width:320,height:844});assert.equal(await page.locator('#fightDialog').evaluate(e=>e.scrollWidth<=e.clientWidth),true);
      await page.locator('[data-curve-range]').focus();await page.keyboard.press('Home');assert.match(await page.locator('[data-curve-readout]').textContent(),/0,5 s/);
      if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'damage-curve-mobile.png'),fullPage:true});
      await page.locator('#closeDialog').click();await page.setViewportSize({width:1440,height:1000});
    });
    await check('boss trend summary, attempt selection and stable boss keys survive reordered history',async()=>{
      const histories=[{boss:'Kargos',dungeon_id:1,difficulty:'Normal',attempts:Array.from({length:25},(_,i)=>({dps:(i+1)*1000,started_at:run.started_at+i*1000,duration_ms:90000,fight_id:'f1',died:i===24}))},{boss:'Other',dungeon_id:2,attempts:[{dps:1,started_at:run.started_at}]}];
      await page.route('**/api/stats/boss-history**',route=>route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(histories)}));
      await page.getByRole('button',{name:'Statistik',exact:true}).click();await page.waitForFunction(()=>document.querySelector('#bossSummary').textContent.includes('25,0K'));
      assert.match(await page.locator('#bossSummary').textContent(),/15,5K DPS/);assert.equal(await page.locator('#bossAttempt').getAttribute('max'),'19');
      assert.match(await page.locator('#bossReadout').textContent(),/Versuch 25.*Anteil nicht erfasst.*Eigener Tod/);
      await page.locator('#bossAttempt').focus();await page.keyboard.press('Home');assert.match(await page.locator('#bossReadout').textContent(),/Versuch 6/);
      await page.selectOption('#bossLimit','0');assert.equal(await page.locator('#bossAttempt').getAttribute('max'),'24');
      const key=await page.locator('#bossSel').inputValue();histories.reverse();await page.evaluate(()=>loadBossHistory());
      assert.equal(await page.locator('#bossSel').inputValue(),key);assert.match(await page.locator('#bossSummary').textContent(),/25,0K DPS/);assert.match(await page.locator('#bossReadout').textContent(),/Versuch 6/);
      if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'statistics-desktop.png'),fullPage:true});
      await page.setViewportSize({width:320,height:844});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
      if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'statistics-mobile.png'),fullPage:true});
      await page.locator('#bossOpen').click();await page.locator('#fightDamageChart').waitFor();
      await page.locator('#closeDialog').click();await page.setViewportSize({width:1440,height:1000});
      await page.unroute('**/api/stats/boss-history**');await page.selectOption('#bossLimit','20');await page.getByRole('button',{name:'Live',exact:true}).click();
    });
    await check('activity hierarchy separates expeditions, bosses and ended runs and keeps manual unknown mappings',async()=>{
      const isolated=await context.newPage();isolated.on('pageerror',e=>errors.push(e.message));
      const attempt=(dps,id='f1')=>({dps,started_at:run.started_at,duration_ms:90000,fight_id:id,job:'cleric',numeric_limited:0});
      const groups=[
        {boss:'Kargos',dungeon_id:600093,dungeon_name:'Ferocious Horn Den',activity:'expedition',difficulty:'Schwer',mob_code:100,attempts:[attempt(50000)]},
        {boss:'Kargos',dungeon_id:600093,dungeon_name:'Ferocious Horn Den',activity:'expedition',difficulty:'Schwer',mob_code:102,attempts:[attempt(15000)]},
        {boss:'Wachposten',dungeon_id:600093,dungeon_name:'Ferocious Horn Den',activity:'expedition',difficulty:'Schwer',mob_code:101,attempts:[attempt(30000)]},
        {boss:'Kargos',dungeon_id:600092,dungeon_name:'Ferocious Horn Den',activity:'expedition',difficulty:'Normal',mob_code:100,attempts:[attempt(90000)]},
        {boss:'Boss B',dungeon_id:600001,dungeon_name:'Krao Cave',activity:'expedition',difficulty:'Erkundung',mob_code:200,attempts:[attempt(20000)]},
        {boss:'Feldboss',dungeon_id:0,activity:'field_boss',mob_code:300,attempts:[attempt(70000)]},
        {boss:'Weltgegner',dungeon_id:0,activity:'open_world',mob_code:400,attempts:[attempt(10000)]},
        {boss:'Unbekannter Boss',dungeon_id:900001,activity:'unclassified',mob_code:500,attempts:[attempt(60000)]}
      ];
      let historyDelay=0;
      const runs=[{boss:'Gesamter Run',scope:'run',dungeon_id:600093,dungeon_name:'Ferocious Horn Den',activity:'expedition',difficulty:'Schwer',attempts:[{...attempt(42000),fight_id:undefined,run_id:1,fight_count:3}]}];
      await isolated.route('**/api/stats/boss-history**',async route=>{const body=JSON.stringify(new URL(route.request().url()).searchParams.get('character')==='Empty'?[]:groups);const wait=historyDelay;if(wait)await delay(wait);await route.fulfill({status:200,contentType:'application/json',body});});
      await isolated.route('**/api/stats/run-history**',async route=>{const body=JSON.stringify(new URL(route.request().url()).searchParams.get('character')==='Empty'?[]:runs);const wait=historyDelay;if(wait)await delay(wait);await route.fulfill({status:200,contentType:'application/json',body});});
      try {
        await isolated.goto(base+'/#stats');await isolated.locator('#bossChart svg').waitFor();
        await isolated.selectOption('#activitySel','expedition');await isolated.selectOption('#contentSel','600093');
        assert.deepEqual(await isolated.locator('#bossSel option').allTextContents(),['Gesamte Expedition (1 Run)','Kargos · #100 (1 Versuch)','Kargos · #102 (1 Versuch)','Wachposten (1 Versuch)']);
        assert.match(await isolated.locator('#bossSummary').textContent(),/42,0K/);assert.match(await isolated.locator('#bossNote').textContent(),/gemeinsame erfasste Kampfzeit/);assert.match(await isolated.locator('#bossNote').textContent(),/Run-DPS/);assert.equal(await isolated.locator('#bossAttemptLabel').textContent(),'Run');assert.match(await isolated.locator('#bossInsights').textContent(),/vergleichbare Runs/);
        if(process.env.SCREENSHOT_DIR)await isolated.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'activity-expedition-total.png'),fullPage:true});
        await isolated.locator('#bossOpen').click();await isolated.locator('#runDetail #backBtn').waitFor();assert.match(await isolated.locator('#runDetail').textContent(),/Ferocious/);assert.equal(await isolated.locator('#runs').evaluate(e=>e.classList.contains('active')),true);await isolated.locator('nav [data-tab="stats"]').click();await isolated.locator('#bossChart svg').waitFor();
        await isolated.selectOption('#bossSel',{label:'Kargos · #100 (1 Versuch)'});assert.match(await isolated.locator('#bossSummary').textContent(),/50,0K/);
        groups.reverse();await isolated.evaluate(()=>loadBossHistory());assert.match(await isolated.locator('#bossSummary').textContent(),/50,0K/);assert.equal(await isolated.locator('#contentSel').inputValue(),'600093');
        for(const width of [1440,768,320]){await isolated.setViewportSize({width,height:1000});await isolated.evaluate(()=>scrollTo(0,0));if(width===320){const area=await isolated.locator('#activitySel').boundingBox(),limit=await isolated.locator('#bossLimit').boundingBox();assert.ok(Math.abs(area.y-limit.y)<2,'mobile area and limit share one row');assert.ok((await isolated.locator('.activity-filters').boundingBox()).height<215,'mobile hierarchy stays compact: '+JSON.stringify(await isolated.evaluate(()=>{const e=document.querySelector('.activity-filters');return {height:e.getBoundingClientRect().height,rows:getComputedStyle(e).gridTemplateRows,children:[...e.children].map(x=>({height:x.getBoundingClientRect().height,row:getComputedStyle(x).gridRow}))};})));}assert.equal(await isolated.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);if(process.env.SCREENSHOT_DIR)await isolated.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'activity-expedition-'+width+'.png'),fullPage:true});}
        await check('activity selection persists exact boss and area and falls back when removed',async()=>{
          await isolated.reload();await isolated.locator('#bossChart svg').waitFor();assert.equal(await isolated.locator('#activitySel').inputValue(),'expedition');assert.equal(await isolated.locator('#contentSel').inputValue(),'600093');assert.match(await isolated.locator('#bossSel option:checked').textContent(),/Kargos · #100/);
          const removed=groups.splice(groups.findIndex(g=>g.mob_code===100&&g.dungeon_id===600093),1)[0];
          await isolated.reload();await isolated.locator('#bossChart svg').waitFor();assert.match(await isolated.locator('#bossSel option:checked').textContent(),/Gesamte Expedition/);groups.push(removed);
        });
        await isolated.selectOption('#contentSel','600092');assert.match(await isolated.locator('#bossSummary').textContent(),/90,0K/);
        await isolated.selectOption('#activitySel','field_boss');assert.equal(await isolated.locator('#bossSel option').count(),1);assert.match(await isolated.locator('#bossSummary').textContent(),/70,0K/);
        assert.equal(await isolated.locator('#contentSel').isVisible(),false);
        await isolated.selectOption('#activitySel','open_world');assert.equal(await isolated.locator('#bossSel option').count(),1);assert.match(await isolated.locator('#bossSummary').textContent(),/10,0K/);
        await isolated.selectOption('#activitySel','nightmare');assert.equal(await isolated.locator('#bossChart svg').count(),0);assert.equal(await isolated.locator('#bossOpen').isDisabled(),true);
        await isolated.selectOption('#activitySel','unclassified');if(!await isolated.locator('#activityMapping').evaluate(e=>e.open))await isolated.locator('#activityMapping summary').click();await isolated.selectOption('#activityOverride','nightmare');
        assert.equal(await isolated.locator('#activitySel').inputValue(),'nightmare');assert.match(await isolated.locator('#bossSummary').textContent(),/60,0K/);
        await isolated.reload();await isolated.locator('#bossChart svg').waitFor();await isolated.selectOption('#activitySel','nightmare');assert.match(await isolated.locator('#bossSummary').textContent(),/60,0K/);
        assert.equal(groups.find(g=>g.dungeon_id===900001).activity,'unclassified','browser mapping must not alter records');
        if(!await isolated.locator('#activityMapping').evaluate(e=>e.open))await isolated.locator('#activityMapping summary').click();await isolated.selectOption('#activityOverride','secret_dungeon');await isolated.selectOption('#activitySel','secret_dungeon');assert.match(await isolated.locator('#bossSummary').textContent(),/60,0K/);
        if(!await isolated.locator('#activityMapping').evaluate(e=>e.open))await isolated.locator('#activityMapping summary').click();await isolated.selectOption('#activityOverride','');assert.equal(await isolated.locator('#activitySel').inputValue(),'unclassified');assert.match(await isolated.locator('#bossSummary').textContent(),/60,0K/);
              await check('missing area and explicit zero remain different and unmappable without known area',async()=>{
          const unknown={...groups.find(g=>g.mob_code===300),dungeon_id:null,activity:'unclassified'};groups.push(unknown);
          await isolated.evaluate(()=>loadBossHistory());await isolated.selectOption('#activitySel','unclassified');await isolated.selectOption('#contentSel','unknown');
          assert.match(await isolated.locator('#bossSel option:checked').textContent(),/Feldboss/);assert.equal(await isolated.locator('#activityMapping').isVisible(),false);
          assert.equal(await isolated.evaluate(()=>contentLabel(bossHistory.find(b=>b.dungeon_id===null))),'Gebiet unbekannt');groups.pop();
        });
        await check('character reload removes stale reports and ignores late answers',async()=>{
          historyDelay=300;
          await isolated.evaluate(()=>{character='Me';loadBossHistory().catch(()=>{});});
          assert.equal(await isolated.locator('#bossOpen').isDisabled(),true);assert.equal(await isolated.locator('#bossSel').isDisabled(),true);
          await delay(50);historyDelay=0;
          await isolated.evaluate(()=>{character='Empty';return loadBossHistory();});await delay(400);
          assert.equal(await isolated.locator('#bossChart svg').count(),0);assert.equal(await isolated.locator('#bossOpen').isDisabled(),true);
          await isolated.evaluate(()=>{character='';return loadBossHistory();});
        });
        await check('same named areas expose IDs and unknown difficulty without merging',async()=>{
          const duplicate={...groups.find(g=>g.dungeon_id===600093),dungeon_id:800001};groups.push(duplicate);
          await isolated.evaluate(()=>loadBossHistory());await isolated.selectOption('#activitySel','expedition');
          assert.match(await isolated.locator('#contentSel option[value="600093"]').textContent(),/#600093/);assert.match(await isolated.locator('#contentSel option[value="800001"]').textContent(),/#800001/);
          await isolated.selectOption('#contentSel','800001');assert.equal(await isolated.locator('#bossSel option').count(),1);
          assert.match(await isolated.evaluate(()=>contentLabel({dungeon_id:900001})),/Schwierigkeit unbekannt/);groups.pop();
        });
        await check('corrupt local settings recover and blocked storage never changes mappings',async()=>{
          await isolated.evaluate(()=>{localStorage.setItem('a2m-activity-overrides','{broken');localStorage.setItem('a2m-activity-selection','null');});
          await isolated.reload();await isolated.locator('#bossChart svg').waitFor();await isolated.selectOption('#activitySel','unclassified');
          if(!await isolated.locator('#activityMapping').evaluate(e=>e.open))await isolated.locator('#activityMapping summary').click();
          await isolated.evaluate(()=>{Storage.prototype.setItem=function(){throw new DOMException('blocked','SecurityError');};});
          await isolated.selectOption('#activityOverride','nightmare');assert.equal(await isolated.locator('#activitySel').inputValue(),'unclassified');assert.match(await isolated.locator('#bossSummary').textContent(),/60,0K/);
        });
      } finally {await isolated.close();await page.evaluate(()=>{localStorage.removeItem('a2m-activity-overrides');localStorage.removeItem('a2m-activity-selection');});}
    });
    await check('trimmed history does not invent a first-interval damage spike',async()=>{
      const html=await page.evaluate(()=>{
        const f={duration_ms:10500,players:[{actor_id:1,name:'Me'}],analytics:{partial:true,points:[{ms:10000,damage:{1:1000000}},{ms:10500,damage:{1:1000100}}]}};
        return damageCurve(f);
      });assert.match(html,/200\/s/);assert.ok(!html.includes('100,0K'));
    });
    await check('many players long names many skills and numerical limits remain usable',async()=>{
      const oldRows=live.rows;
      live.rows=Array.from({length:24},(_,i)=>({...oldRows[0],id:i+1,is_self:i===0,name:'Sehr langer Spielername '+i+' '+('W'.repeat(60)),damage:3000000000-i}));
      live.numeric_limited=true;
      await page.waitForFunction(()=>document.querySelectorAll('#liveRows .bar').length===24);
      await page.locator('#numericWarning').waitFor();
      await page.setViewportSize({width:320,height:844});
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth),true);
      await page.evaluate(()=>{
        document.querySelector('#fightContent').innerHTML=skillTable(Array.from({length:100},(_,i)=>({name:'Langer Skill '+i+' '+('S'.repeat(120)),damage:2000000000,hits:1})));
        bindSkillTables(document.querySelector('#fightContent'));document.querySelector('#fightDialog').showModal();
      });
      assert.equal(await page.locator('.skill-browser tbody tr').count(),100);
      assert.equal(await page.evaluate(()=>document.querySelector('#fightDialog').scrollWidth<=document.querySelector('#fightDialog').clientWidth),true);
      await page.locator('#closeDialog').click();live.rows=oldRows;live.numeric_limited=false;
      await page.setViewportSize({width:1440,height:1000});
    });
    await check('diagnostics exclude character names raw errors and network identifiers',async()=>{
      Object.assign(live.capture,{error:'secret file /home/PrivateName/capture',device:'secret-device',packets:42,last_packet_ms:Date.now()});
      await page.waitForFunction(()=>latestLive.capture.packets===42);await page.locator('#copyDiagnostics').click();
      await page.waitForFunction(()=>window.copiedRanking?.startsWith('{'));const text=await page.evaluate(()=>window.copiedRanking);const data=JSON.parse(text);
      assert.equal(data.packets,42);assert.equal(data.capture_error,true);assert.ok(!text.includes('PrivateName')&&!text.includes('secret-device')&&!text.includes('FeroxTOO'));
      delete live.capture.error;
    });
    await page.waitForFunction(()=>document.querySelectorAll('#liveRows .bar').length===3);
    await check('empty combat state gives guidance and zero personal DPS',async()=>{
      live.rows=[]; live.total_damage=0; live.battle_time_ms=0;
      await page.waitForFunction(()=>document.querySelector('#liveRows').textContent.includes('Bereit für den nächsten Kampf'));
      assert.equal(await page.locator('#selfDps').textContent(),'0');
    });
    await check('missing capture permission names the exact setcap command',async()=>{
      const capture=live.capture;
      live.capture={...capture,permission:false,binary:"/home/me/My Apps/aion2-meter"};
      await page.waitForFunction(()=>!document.querySelector('#captureHelp').hidden);
      assert.match(await page.locator('#captureHelp').textContent(),/sudo setcap cap_net_raw=ep '\/home\/me\/My Apps\/aion2-meter'/);
      live.capture=capture;
    });
    await check('offline catalog filters bilingual names aliases and community translations',async()=>{
      await page.evaluate(()=>show('skills'));await page.locator('#catalogRows tr').first().waitFor();
      assert.ok(await page.locator('#catalogRows .game-icon').count()>0);
      await page.locator('#catalogSearch').fill('11170010');assert.equal(await page.locator('#catalogRows tr').count(),1);assert.match(await page.locator('#catalogRows').textContent(),/Abwärtsschlag.*Overhead Slam/s);
      await page.locator('#catalogSearch').fill('');await page.locator('#catalogClass').selectOption('fighter');assert.equal(await page.locator('#catalogRows tr').count(),41);assert.match(await page.locator('#catalogRows').textContent(),/DE Community/);
      await page.evaluate(()=>show('live'));
    });
    await check('language setting relabels skills and exports without changing measured values',async()=>{
      // The empty-state check above intentionally removes all live rows.
      live.rows=fight.players.map(({skills,heal_skills,buffs,actor_id,job,...row})=>row);
      live.total_damage=fight.total_damage;live.battle_time_ms=fight.duration_ms;
      await page.evaluate(()=>show('settings'));await page.locator('[data-k="skill_language"]').selectOption('en');
      await page.waitForFunction(()=>settings.skill_language==='en'&&!settingsDirty&&!settingsSaving);assert.equal(settings.skill_language,'en');
      await page.evaluate(()=>show('live'));await page.locator('#liveRows .bar').first().waitFor();await page.locator('#liveRows .bar').first().click();
      await page.locator('#fightDialog[open] #refreshPlayer').waitFor();
      assert.match(await page.locator('.skill-browser').first().textContent(),/Strike/);assert.ok(await page.locator('.skill-browser .game-icon').count()>0);
      const exported=await page.evaluate(f=>fightExport(f,true),fight);assert.equal(exported.players[0].skills[0].name,'Strike');assert.equal(exported.players[0].skills[0].damage,fight.players[0].skills[0].damage);assert.equal(exported.players[0].name,'FeroxTOO');assert.equal(exported.players[1].name,'Spieler 2');
      await page.locator('#closeDialog').click();await page.evaluate(()=>show('settings'));await page.locator('[data-k="skill_language"]').selectOption('de');await page.waitForFunction(()=>!settingsDirty&&!settingsSaving);
    });
    await check('catalog view survives reload and empty filters can be reset',async()=>{
      await page.getByRole('button',{name:'Fähigkeiten',exact:true}).click();await page.locator('#catalogRows tr').first().waitFor();
      await page.locator('#catalogReset').click();await page.locator('#catalogVariants').check();await page.locator('#catalogNext').click();
      assert.equal(await page.locator('#catalogPage').textContent(),'81–160');
      await page.reload();await page.waitForFunction(()=>document.querySelector('#catalogPage').textContent==='81–160');
      assert.equal(await page.locator('#catalogVariants').isChecked(),true);
      await page.locator('#catalogSearch').fill('Overhead Slam');await page.locator('#catalogClass').selectOption('gladiator');
      await page.reload();await page.waitForFunction(()=>document.querySelectorAll('#catalogRows tr').length===1);
      assert.equal(await page.locator('#catalogSearch').inputValue(),'Overhead Slam');assert.equal(await page.locator('#catalogClass').inputValue(),'gladiator');
      assert.match(await page.locator('#catalogRows').textContent(),/Abwärtsschlag/);
      await page.locator('#catalogClass').selectOption('fighter');await page.locator('#catalogEmpty').waitFor();assert.equal(await page.locator('#catalogRows tr').count(),0);
      await page.locator('#catalogReset').click();assert.equal(await page.locator('#catalogSearch').inputValue(),'');assert.equal(await page.locator('#catalogClass').inputValue(),'');assert.equal(await page.locator('#catalogVariants').isChecked(),false);assert.equal(await page.locator('#catalogPage').textContent(),'1–80');
      assert.equal(await page.locator('#catalogEmpty').isVisible(),false);assert.equal(await page.locator('#catalogReset').isDisabled(),true);
    });
    await check('search shortcuts preserve typing and Escape clears before closing a dialog',async()=>{
      await page.getByRole('button',{name:'Fähigkeiten',exact:true}).click();await page.keyboard.press('/');
      assert.equal(await page.evaluate(()=>document.activeElement.id),'catalogSearch');
      await page.keyboard.type('no-match/');assert.equal(await page.locator('#catalogSearch').inputValue(),'no-match/');await page.keyboard.press('Escape');assert.equal(await page.locator('#catalogSearch').inputValue(),'');
      await page.getByRole('button',{name:'Runs',exact:true}).click();await page.keyboard.press('/');assert.equal(await page.evaluate(()=>document.activeElement.id),'fightSearch');
      await page.keyboard.type('boss');await page.keyboard.press('Escape');assert.equal(await page.locator('#fightSearch').inputValue(),'');
      await page.getByRole('button',{name:'Live',exact:true}).click();await page.locator('#liveRows .bar').first().waitFor();await page.locator('#liveRows .bar').first().click();await page.locator('#fightDialog[open] #refreshPlayer').waitFor();
      await page.locator('#refreshPlayer').focus();await page.keyboard.press('/');assert.equal(await page.evaluate(()=>document.activeElement.classList.contains('skill-search')),true);
      await page.keyboard.type('nothing');await page.keyboard.press('Escape');assert.equal(await page.locator('#fightDialog').evaluate(d=>d.open),true);assert.equal(await page.locator('.skill-search').first().inputValue(),'');
      await page.keyboard.press('Escape');assert.equal(await page.locator('#fightDialog').evaluate(d=>d.open),false);
    });
    await check('clipboard fallback stays inside modal and cleans up on failure',async()=>{
      const isolated=await context.newPage();isolated.on('pageerror',e=>errors.push(e.message));
      try {
        await isolated.goto(base);await isolated.waitForFunction(()=>typeof copyText==='function');
        const result=await isolated.evaluate(async()=>{
          const dialog=document.querySelector('#fightDialog'),button=document.querySelector('#closeDialog'),descriptor=Object.getOwnPropertyDescriptor(navigator,'clipboard'),exec=document.execCommand;
          dialog.showModal();button.focus();const before=document.querySelectorAll('textarea').length;let captured;
          try {
            Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{throw new Error('denied');}}});
            document.execCommand=()=>{const input=document.activeElement;captured={text:input.value,inside:dialog.contains(input),selected:input.selectionEnd-input.selectionStart};return true;};
            const ok=await copyText('Äon 11170010');const focused=document.activeElement===button;
            document.execCommand=()=>{throw new Error('unavailable');};const failed=await copyText('not copied');
            return {captured,ok,focused,failed,clean:document.querySelectorAll('textarea').length===before,focusAfterError:document.activeElement===button,message:document.querySelector('#toast').textContent,error:document.querySelector('#toast').classList.contains('error')};
          } finally {document.execCommand=exec;if(descriptor)Object.defineProperty(navigator,'clipboard',descriptor);else delete navigator.clipboard;dialog.close();}
        });
        assert.deepEqual(result.captured,{text:'Äon 11170010',inside:true,selected:'Äon 11170010'.length});assert.equal(result.ok,true);assert.equal(result.focused,true);assert.equal(result.failed,false);assert.equal(result.clean,true);assert.equal(result.focusAfterError,true);assert.equal(result.error,true);assert.match(result.message,/Zwischenablage/);
      } finally {await isolated.close();}
    });
    await check('performance story links a complete peak window to observed skill ticks',async()=>{
      const focused={...fight,id:'insight',duration_ms:10000,total_damage:6000,
        players:fight.players.slice(0,2).map((p,i)=>({...p,damage:(i+1)*2000,dps:(i+1)*200,skills:[{...skill,damage:(i+1)*2000,hit_timestamps:[1000,4000,5000,9000]}]})),
        analytics:{resolution_ms:500,partial:false,points:Array.from({length:20},(_,i)=>({ms:(i+1)*500,damage:{1:(i+1)*100,2:(i+1)*200}}))}};
      await page.route('**/api/fights/insight',route=>route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(focused)}));
      await page.evaluate(()=>openFight('insight'));await page.locator('[data-curve-peak]').waitFor();
      assert.match(await page.locator('.fight-story').textContent(),/200.*5s-Fenster.*100,0%/);
      assert.match(await page.locator('.fight-story .story-intro').textContent(),/200 DPS.*Rang 2 von 2.*47,6%/);
      assert.ok(await page.evaluate(()=>document.querySelector('.fight-story').compareDocumentPosition(document.querySelector('.fight-tools'))&Node.DOCUMENT_POSITION_FOLLOWING),'story leads the report before export tools');
      await page.locator('[data-story-peak]').click();assert.equal(await page.locator('#fightChartScope').inputValue(),'1');assert.equal(await page.locator('[data-curve-highlight]').count(),1);
      // Moving the pointer over the chart or resizing must not silently drop the marked window.
      await page.locator('[data-curve-plot] .curve-hit').hover({position:{x:40,y:40}});assert.equal(await page.locator('[data-curve-highlight]').count(),1);
      await page.setViewportSize({width:1000,height:1000});await page.waitForFunction(()=>Math.abs(document.querySelector('#fightDamageChart').drawnWidth-document.querySelector('#fightDamageChart').clientWidth)<1);
      assert.equal(await page.locator('[data-curve-highlight]').count(),1);await page.setViewportSize({width:1440,height:1000});
      await page.locator('[data-curve-plot] .curve-hit').click({position:{x:40,y:40}});assert.equal(await page.locator('[data-curve-highlight]').count(),0);
      await page.locator('[data-curve-peak]').click();
      assert.match(await page.locator('[data-curve-readout]').textContent(),/5 s/);
      await page.locator('[data-curve-hits]').click();assert.equal(await page.locator('.window-hits li').count(),1);
      assert.match(await page.locator('.curve-detail').textContent(),/3 Treffer\/Ticks/);
      await page.selectOption('#fightChartScope','1');await page.locator('[data-curve-peak]').click();await page.locator('[data-curve-hits]').click();
      assert.equal(await page.locator('.window-hits li').count(),1);
      await page.selectOption('#fightChartMetric','total');assert.match(await page.locator('[data-curve-readout]').textContent(),/Schaden/);
      await page.setViewportSize({width:320,height:844});assert.equal(await page.evaluate(()=>document.querySelector('#fightDialog').scrollWidth<=document.querySelector('#fightDialog').clientWidth),true);
      await page.setViewportSize({width:1440,height:1000});await page.selectOption('#fightChartMetric','dps');await page.locator('[data-curve-peak]').click();
      if(process.env.SCREENSHOT_DIR){
        await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'signature-peak-window.png'),fullPage:true});
        await page.locator('#fightDialog').evaluate(e=>e.scrollTop=0);
        await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'signature-fight-story.png'),fullPage:true});
      }
      await page.locator('#closeDialog').click();await page.unroute('**/api/fights/insight');
    });
    await check('personal progression requires a character and a comparable class',async()=>{
      const data=[{boss:'Kargos',dungeon_id:1,attempts:[10000,10000,20000].map((dps,i)=>({dps,job:'cleric',numeric_limited:0,started_at:run.started_at+i*1000,duration_ms:90000,fight_id:'f1'}))}];
      await page.route('**/api/stats/boss-history**',route=>route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(data)}));
      await page.selectOption('#charSel','FeroxTOO');await page.getByRole('button',{name:'Statistik',exact:true}).click();
      await page.waitForFunction(()=>document.querySelector('#bossSummary').textContent.includes('+100,0 %'));
      const summary=await page.locator('#bossSummary').textContent();
      assert.match(summary,/Rang 1 von 3/);assert.match(summary,/Ø der 2 vorherigen Versuche \(10,0K DPS\)/);assert.match(summary,/Ø aller 3 Versuche/);
      assert.equal(await page.locator('#bossSummary > div').count(),4,'one summary row, no duplicated insight cards');
      assert.equal(await page.locator('#bossInsights').textContent(),'');
      await page.locator('#bossAttempt').fill('2');assert.match(await page.locator('#bossReadout').textContent(),/\+100,0 % zum vorherigen Versuch/);
      if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'signature-progression.png'),fullPage:true});
      await page.selectOption('#charSel','');await page.waitForFunction(()=>document.querySelector('#bossInsights').textContent.includes('Charakter wählen'));
      await page.unroute('**/api/stats/boss-history**');
    });
    await check('direct statistics reload tolerates delayed enhancement scripts',async()=>{
      const isolated=await context.newPage();isolated.on('pageerror',e=>errors.push(e.message));
      try {
        await isolated.addInitScript(()=>localStorage.setItem('a2m-character','FeroxTOO'));
        await isolated.route('**/enhancements.js',async route=>{await delay(600);await route.continue();});
        await isolated.goto(base+'/#stats');await isolated.waitForFunction(()=>document.querySelector('#bossInsights').textContent.includes('Mindestens drei'));
        assert.match(await isolated.locator('#bossSummary').textContent(),/50,0K/);
        assert.equal(await isolated.locator('#toast.error').isVisible(),false);
      } finally {await isolated.close();}
    });
    await check('saved attempt moment requires the exact stored encounter and clears on character change',async()=>{
      const isolated=await context.newPage();isolated.on('pageerror',e=>errors.push(e.message));
      const stored=telemetryFight('auto_9_1000');let current={...live};
      await isolated.route('**/api/live',route=>route.fulfill({status:200,contentType:'application/json',body:JSON.stringify({...current,capture:{...live.capture,last_packet_ms:Date.now()}})}));
      await isolated.route('**/api/fights/auto_9_1000',route=>route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(stored)}));
      try {
        await isolated.goto(base);await isolated.waitForFunction(()=>typeof renderEnhancedLive==='function');
        current={...live,target_started_at:2000,battle_time_ms:1000,total_damage:105000,rows:live.rows.map(r=>({...r,damage:r.dps}))};
        await isolated.locator('#postFight').waitFor();assert.match(await isolated.locator('#postFight').textContent(),/LETZTER GESPEICHERTER VERSUCH/);
        assert.doesNotMatch(await isolated.locator('#postFight').textContent(),/Sieg|Kampf beendet/);
        await isolated.locator('#postFight [data-story-peak]').click();await isolated.locator('#fightDialog').waitFor();assert.equal(await isolated.locator('#fightChartScope').inputValue(),'1');assert.equal(await isolated.locator('[data-curve-highlight]').count(),1);
        await isolated.locator('#closeDialog').click();
        await isolated.evaluate(()=>scrollTo(0,0));
        if(process.env.SCREENSHOT_DIR)await isolated.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'telemetry-saved-moment.png'),fullPage:true});
        await isolated.evaluate(()=>renderEnhancedLive({...latestLive,character:'Alt',rows:latestLive.rows.map(r=>r.is_self?{...r,id:8,name:'Alt'}:r)}));
        assert.equal(await isolated.locator('#postFight').isVisible(),false);
      } finally {await isolated.close();}
    });
    await check('training instrument shows actual elapsed time, finished result and interruption',async()=>{
      const isolated=await context.newPage();isolated.on('pageerror',e=>errors.push(e.message));
      let training={state:'armed',seconds:60,started_at:1000};
      await isolated.route('**/api/live',route=>route.fulfill({status:200,contentType:'application/json',body:JSON.stringify({...live,target_mode:'trainTargets',target_name:'Trainingsziel (synthetisch)',target_hp:null,dungeon:null,capture:{...live.capture,last_packet_ms:Date.now()},battle_time_ms:training.state==='running'?42000:60000,rows:live.rows.map(r=>r.is_self?{...r,damage:training.state==='running'?2024400:3186000,dps:training.state==='running'?48200:53100}:{...r,damage:r.dps*(training.state==='running'?42:60)}),training})}));
      try {
        await isolated.goto(base);await isolated.waitForFunction(()=>document.querySelector('#trainingResult').textContent.includes('Warte auf ersten Treffer'));
        training={...training,state:'running',elapsed_ms:42000,target_id:9};
        await isolated.waitForFunction(()=>document.querySelector('#trainingResult').textContent.includes('0:42'));
        assert.equal(await isolated.getByRole('progressbar',{name:'Trainingsfortschritt'}).getAttribute('aria-valuenow'),'70');
        for(const width of [1440,320]){await isolated.setViewportSize({width,height:1000});await isolated.evaluate(()=>scrollTo(0,0));assert.equal(await isolated.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);if(process.env.SCREENSHOT_DIR)await isolated.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'telemetry-training-running-'+width+'.png'),fullPage:true});}
        training={...training,state:'finished',elapsed_ms:60000,target:'Trainingsziel (synthetisch)',character:'FeroxTOO',personal_best:true,best_dps:53100,rows:[{name:'FeroxTOO',is_self:true,damage:3186000,dps:53100}]};
        await isolated.waitForFunction(()=>document.querySelector('#trainingResult').textContent.includes('Neuer persönlicher Bestwert'));
        assert.match(await isolated.locator('.training-rate').textContent(),/53,1K/);
        await isolated.setViewportSize({width:1440,height:1000});await isolated.evaluate(()=>scrollTo(0,0));
        if(process.env.SCREENSHOT_DIR)await isolated.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'telemetry-training-record.png'),fullPage:true});
        training={...training,state:'interrupted',rows:undefined,best_dps:undefined};
        await isolated.waitForFunction(()=>document.querySelector('#trainingResult').textContent.includes('Kein abgeschlossenes Ergebnis'));
        assert.equal(await isolated.locator('.training-rate').count(),0);
      } finally{await isolated.close();}
    });
    await check('populated combat instrument fits all themes and keeps its selected observed peak',async()=>{
      const data=telemetryFight();await page.route('**/api/fights/telemetry',route=>route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(data)}));
      await page.setViewportSize({width:1440,height:1000});await page.evaluate(()=>openFight('telemetry'));
      await page.locator('#fightContent [data-story-peak]').click();assert.equal(await page.locator('[data-curve-highlight]').count(),1);
      assert.match(await page.locator('.fight-story').textContent(),/31,4%/);
      for(const theme of ['midnight','aether','ember']){
        settings.theme=theme;await page.waitForFunction(t=>document.documentElement.dataset.theme===t,theme);
        for(const width of [1440,320]){
          await page.setViewportSize({width,height:1000});await page.locator('#fightDialog').evaluate(e=>e.scrollTop=0);
          await page.waitForFunction(()=>Math.abs(document.querySelector('#fightDamageChart').drawnWidth-document.querySelector('#fightDamageChart').clientWidth)<1);
          assert.equal(await page.evaluate(()=>document.querySelector('#fightDialog').scrollWidth<=document.querySelector('#fightDialog').clientWidth),true);
          if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,`telemetry-report-${theme}-${width}.png`),fullPage:true});
        }
      }
      settings.theme='midnight';await page.setViewportSize({width:1440,height:1000});await page.locator('#closeDialog').click();await page.unroute('**/api/fights/telemetry');await page.evaluate(()=>scrollTo(0,0));
    });
    await check('live hierarchy, themes and reduced motion remain readable at desktop and 320px',async()=>{
      await page.getByRole('button',{name:'Live',exact:true}).click();await page.selectOption('#liveMetric','damage');
      await page.waitForFunction(()=>document.querySelector('#selfRank').textContent==='#1');assert.match(await page.locator('#selfContext').textContent(),/47,6%.*3 Spieler/);
      // Share ribbon: one segment per player, yours marked; the gap is derived from the same rates.
      assert.equal(await page.locator('#shareRibbon i').count(),3);assert.equal(await page.locator('#shareRibbon i.me').count(),1);assert.match(await page.locator('#selfGap').textContent(),/16,7K\/s vor #2/);
      // Dedicated tracks retain real proportions independently of row surfaces.
      const bars=await page.$$eval('#liveRows .bar',rows=>rows.map(r=>{const f=r.querySelector('.fill');return {height:f.getBoundingClientRect().height,width:f.getBoundingClientRect().width,opacity:Number(getComputedStyle(f).opacity)};}));
      assert.ok(bars.length>=3&&bars.every(b=>b.height>=4&&b.opacity>=.7)&&Math.abs(bars[1].width/bars[0].width-2/3)<.02,JSON.stringify(bars));
      await page.selectOption('#liveMetric','heal');assert.equal(await page.locator('#selfBurst').isVisible(),false);assert.match(await page.locator('#selfDpsLabel').textContent(),/HPS/);
      await page.selectOption('#liveMetric','damage');
      await page.evaluate(()=>{for(let i=0;i<2;i++)renderLiveSignal({...latestLive,target_started_at:777,battle_time_ms:1000+i*1000,rows:latestLive.rows.map(r=>({...r,burst_dps:0}))});});
      assert.match(await page.locator('#liveSignal').getAttribute('aria-label'),/höchster empfangener Wert 0 pro Sekunde/);assert.match(await page.locator('.signal-scale').textContent(),/0\/s beobachtet/);
      await page.emulateMedia({reducedMotion:'reduce'});
      for(const theme of ['midnight','aether','ember']){
        settings.theme=theme;await page.waitForFunction(theme=>document.documentElement.dataset.theme===theme,theme);
        for(const width of [1440,320]){
          await page.setViewportSize({width,height:1000});await page.evaluate(()=>{document.querySelector('#toast').hidden=true;scrollTo(0,0);});
          assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
          assert.equal(await page.locator('#selfDps').evaluate(e=>getComputedStyle(e).fontVariantNumeric),'tabular-nums');
          await page.evaluate(()=>{for(let i=0;i<45;i++)renderLiveSignal({...latestLive,target_started_at:1000,battle_time_ms:45000+i*1000,rows:latestLive.rows.map(r=>({...r,burst_dps:r.is_self?50000+Math.sin(i*.5)*12000+Math.cos(i*.18)*8000:r.burst_dps}))});});
          if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,`premium-live-${theme}-${width}.png`),fullPage:true});
        }
      }
      settings.theme='midnight';await page.setViewportSize({width:1440,height:1000});await page.emulateMedia({reducedMotion:'no-preference'});
      const obs=await context.newPage();obs.on('pageerror',e=>errors.push(e.message));settings.compact=true;settings.show_dps=true;settings.scale=1;
      await obs.goto(base+'/overlay');await obs.locator('#rows .row').first().waitFor();assert.equal(await obs.locator('#box').evaluate(e=>Math.round(e.getBoundingClientRect().width)),312);
      assert.match(await obs.locator('#rows .n strong').first().textContent(),/\/s/);
      if(process.env.SCREENSHOT_DIR)await obs.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'premium-obs.png')});await obs.close();
    });
    assert.deepEqual(errors,[]);
    console.log(`${checks} browser checks passed.`);
  } finally { await browser.close(); await new Promise(resolve=>server.close(resolve)); }
})().catch(e=>{console.error(e);server.close();process.exitCode=1;});
