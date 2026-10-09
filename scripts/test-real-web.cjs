// Production binary + production API. No fixture interception; no game packets available.
const assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {spawn}=require('node:child_process');const {chromium}=require('playwright');
(async()=>{
  const temp=fs.mkdtempSync(path.join(os.tmpdir(),'a2m-real-web-'));
  const proc=spawn(path.resolve(process.env.METER_BINARY||'target/release/aion2-meter'),['--no-overlay','--port','8816','--db',path.join(temp,'meter.db')],{stdio:'ignore'});
  let browser;
  try {
    for(let i=0;i<100;i++) {try{const r=await fetch('http://127.0.0.1:8816/api/live');if(r.ok)break;}catch{}await new Promise(r=>setTimeout(r,50));}
    browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:{}),args:['--no-sandbox']});
    const page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.goto('http://127.0.0.1:8816');await page.locator('#captureHelp').waitFor();
    assert.equal(await page.locator('#selfDps').textContent(),'0');assert.match(await page.locator('#liveRows').textContent(),/Bereit/);
    for(const [label,file] of [['Live','actual-live-empty'],['Verlauf','actual-runs-empty'],['Statistik','actual-stats-empty'],['Einstellungen','actual-overlay-settings']]) {
      await page.getByRole('button',{name:label,exact:true}).click();await new Promise(r=>setTimeout(r,300));
      if(process.env.SCREENSHOT_DIR){fs.mkdirSync(process.env.SCREENSHOT_DIR,{recursive:true});await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,file+'.png'),fullPage:true});}
    }
    await page.selectOption('[data-k="theme"]','aether');await new Promise(r=>setTimeout(r,700));
    const settings=await(await fetch('http://127.0.0.1:8816/api/overlay')).json();assert.equal(settings.theme,'aether');
    assert.deepEqual(errors,[]);console.log('PASS production dashboard: four views, empty state, settings write, no page exceptions');
  } finally {
    if(browser)await browser.close();const exit=new Promise(r=>proc.once('exit',(code)=>r(code)));proc.kill('SIGTERM');assert.equal(await exit,0);fs.rmSync(temp,{recursive:true,force:true});
  }
})().catch(e=>{console.error(e);process.exitCode=1;});
