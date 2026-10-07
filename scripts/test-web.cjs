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
const skill={code:11,name:'Hieb',damage:4500000,hits:10,crit_rate:50,back_rate:25,perfect_rate:10,parry_rate:0,double_rate:20,frontal_rate:30,multi_hit_count:3,max:500000,hit_timestamps:[100,500,1000]};
const fight={id:'f1',boss_name:'Kargos',dungeon_id:600093,difficulty:'Schwer',started_at:run.started_at,duration_ms:90000,total_damage:9450000,
 players:live.rows.map(r=>({...r,actor_id:r.id,job:r.class_key,skills:[{...skill,damage:r.damage}],heal_skills:r.is_self?[{...skill,name:'Heilung',damage:r.heal}]:[],buffs:[{code:42,name:'Buff',uptime:50}]})),
 analytics:{resolution_ms:500,partial:false,points:[{ms:500,damage:{1:1000,2:500}},{ms:1000,damage:{1:3000,2:1000}}],effects:[{target:1,code:42,start_ms:100,end_ms:1000}]},ping_history:[{tsMs:500,pingMs:42},{tsMs:1000,pingMs:50}]};
const comparison={...fight,id:'f2',started_at:run.ended_at,players:fight.players.map(p=>({...p,dps:p.dps*.8,skills:p.skills.map(s=>({...s,damage:s.damage*.8}))}))};
const comparison2={...comparison,id:'f3',players:fight.players.map(p=>({...p,dps:p.dps*.5}))};
const profiles=new Map();
let annotations=[],trainingStarts=[];
const server = http.createServer((req,res) => {
  const route=req.url.split('?')[0];
  const file=route==='/overlay'?'overlay.html':route==='/enhancements.js'?'enhancements.js':route==='/enhancements.css'?'enhancements.css':route==='/qol.js'?'qol.js':'index.html';
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
    if(u.pathname==='/api/version')data={version:'0.3.1',parser_version:'2.0.52'};
    else if(u.pathname==='/api/update-check')data={available:true,message:'Update v0.4.0 verfügbar.',url:'https://github.com/feroxtwo/damage-meter/releases'};
    else if (u.pathname === '/api/live') data = live;
    else if(u.pathname==='/api/fights')data={fights:[fight,comparison,comparison2],more:false};
    else if(u.pathname==='/api/fights/f1')data=fight;
    else if(u.pathname==='/api/fights/f2'){if(slowComparison)await delay(350);data=comparison;}
    else if(u.pathname==='/api/fights/f3')data=comparison2;
    else if(u.pathname==='/api/fights/f1/annotation'){annotations.push(req.postDataJSON());}
    else if(u.pathname.startsWith('/api/players/')){if(slowPlayer)await delay(350);data={target_id:9,start_time:1000,skills:[skill],heal_skills:[{...skill,name:'Heilung'}],duration_ms:90000};}
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
      const offset = Number(u.searchParams.get('offset') || 0);
      data = {runs:[{...run, id:offset+1, dungeon_name:u.searchParams.get('character') === 'Alt' ? 'Alt Dungeon' : run.dungeon_name}], total:2, dungeons:[run.dungeon_name]};
    }
    else if (u.pathname === '/api/runs/1') data = {...run, totals:[], fights:[]};
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
    await page.locator('#runRows tr.click').waitFor();
    if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'runs-desktop.png'),fullPage:true});
    await check('pagination has no duplicate requests',async()=>{
      await page.locator('#moreRuns').evaluate(b=>{b.click();b.click();});
      await page.waitForFunction(()=>document.querySelectorAll('#runRows tr.click').length===2);
      assert.deepEqual(await page.locator('#runRows tr.click').evaluateAll(rows=>rows.map(r=>r.dataset.id)),['1','2']);
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
      for(const tab of ['Live','Runs','Statistik','Overlay']) {
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
      assert.equal(await overlay.locator('#rows img').count(),0);await overlay.close();
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
      await page.selectOption('#trainingDuration','180');await page.locator('#startTraining').click();
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
      assert.ok(!text.includes('FeroxTOO')&&!text.includes('Moon'));
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
      assert.ok(await page.locator('#fightContent svg').count()>=3);
      await page.locator('#favoriteFight').check();await page.fill('#fightNote','neues Gear');await page.fill('#fightTags','rotation');await page.locator('#saveFightNote').click();
      await delay(100);assert.deepEqual(annotations.at(-1),{favorite:true,note:'neues Gear',tags:'rotation'});
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
      assert.ok(!JSON.stringify(exp).includes('FeroxTOO'));assert.ok(!JSON.stringify(exp).includes('Moon'));assert.equal(exp.players[0].name,'Spieler 1');
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
      assert.match(await overlay.locator('#rows').textContent(),/3. FeroxTOO/);
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
      assert.equal(await page.locator('svg[aria-label="DPS-Verlauf aller Spieler"] path[stroke-width="2"]').count(),3);
      await page.locator('#anonFight').check();
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
        try {const f={...fightExport({boss_name:'Boss',duration_ms:90000,players:[]},true),boss_name:'Boss',players:[{name:'PrivateName',skills:Array.from({length:70},(_,i)=>({name:'Skill '+i,damage:100,hits:1})),heal_skills:[],buffs:[]}]};await exportPng(f,true);}
        finally{CanvasRenderingContext2D.prototype.fillText=original;}
      });
      await delay(200);page.off('download',receive);assert.equal(downloads.length,2);
      const text=await page.evaluate(()=>pngTexts.join(' '));assert.ok(!text.includes('PrivateName'));assert.match(text,/Skill 69/);assert.match(text,/Spieler 1/);
      assert.equal(downloads[1].suggestedFilename(),'aion2-kampf-2.png');
    });
    await check('chat output is one line, bounded, ordered and anonymized',async()=>{
      const line=await page.evaluate(()=>chatLine('Boss\nwith separator |',exportPlayers(metricRows(latestLive.rows),true),90000));
      assert.ok(!line.includes('\n'));assert.ok(Array.from(line).length<=200);assert.ok(!line.includes('FeroxTOO'));assert.match(line,/Spieler 1/);
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
    await check('trimmed history does not invent a first-interval damage spike',async()=>{
      const html=await page.evaluate(()=>{
        const f={duration_ms:10500,players:[{actor_id:1,name:'Me'}],analytics:{partial:true,points:[{ms:10000,damage:{1:1000000}},{ms:10500,damage:{1:1000100}}]}};
        return partyCurve(f)+damageCurve(f);
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
    assert.deepEqual(errors,[]);
    console.log(`${checks} browser checks passed.`);
  } finally { await browser.close(); await new Promise(resolve=>server.close(resolve)); }
})().catch(e=>{console.error(e);server.close();process.exitCode=1;});
