#!/usr/bin/env python3
"""Real X11 window geometry and appearance smoke check (Xvfb, xdotool, Pillow)."""
import os, subprocess, tempfile, time, urllib.request, json
from pathlib import Path
from PIL import ImageGrab
import shutil
root=Path(os.environ.get('X11_TOOLS_ROOT','/usr'))
binary=str(Path(os.environ.get('METER_BINARY','target/release/aion2-meter')).resolve())
workspace=tempfile.TemporaryDirectory(prefix='a2m-native-')
output=Path(os.environ.get('SCREENSHOT_DIR',workspace.name));output.mkdir(parents=True,exist_ok=True)
env={**os.environ,'DISPLAY':'127.0.0.1:91','LIBGL_ALWAYS_SOFTWARE':'1','LD_LIBRARY_PATH':os.environ.get('LD_LIBRARY_PATH','')}
xlog=open(Path(workspace.name)/'xvfb.log','w'); log=open(Path(workspace.name)/'native.log','w')
x=subprocess.Popen([os.environ.get('XVFB_BINARY',shutil.which('Xvfb') or 'Xvfb'),':91','-screen','0','1800x1800x24','-fp',str(root/'share/fonts/X11/misc'),'-nolisten','unix','-nolisten','local','-listen','tcp','-ac'],env=env,stdout=xlog,stderr=xlog)
fixture_mode=os.environ.get('NATIVE_FIXTURE')=='1'
row_count=5 if fixture_mode else 1
if fixture_mode:
    fixture=Path(workspace.name)/'fixture.json'
    fixture.write_text(json.dumps({'rows':[
        {'name':'FeroxTOO · Eigene Zeile','job':'치유성','damage':450000000},
        {'name':'LuminaraMitSehrLangemNamenFürClipping','job':'검성','damage':300000000},
        {'name':'Khaelis','job':'살성','damage':195000000},
        {'name':'Nyaria · tot','job':'마도성','damage':120000000},
        {'name':'Zephyros','job':'호법성','damage':90000000}]+[{'name':f'Zusätzlicher Spieler {i+6}','job':'궁성','damage':80000000-i*1000000} for i in range(19)]}),encoding='utf-8')
    env['A2M_NATIVE_FIXTURE']=str(fixture)
compact_height=36+20+22*row_count+4
p=None
try:
    for _ in range(30):
        r=subprocess.run([str(root/'bin/xdotool'),'getdisplaygeometry'],env=env,capture_output=True,text=True)
        if r.returncode==0: break
        time.sleep(.1)
    assert r.returncode==0, r.stderr
    p=subprocess.Popen([binary,'--x11','--db',str(Path(workspace.name)/'native.db'),'--port','8796'],env=env,stdout=log,stderr=log)
    for _ in range(50):
        if p.poll() is not None: raise RuntimeError((Path(workspace.name)/'native.log').read_text())
        r=subprocess.run([str(root/'bin/xdotool'),'search','--name','AION2 Meter'],env=env,capture_output=True,text=True)
        if r.returncode==0: break
        time.sleep(.1)
    assert r.returncode==0,r.stderr
    window=r.stdout.strip().splitlines()[0]
    def geometry():
        r=subprocess.run([str(root/'bin/xdotool'),'getwindowgeometry','--shell',window],env=env,capture_output=True,text=True,check=True)
        return dict(line.split('=',1) for line in r.stdout.splitlines() if '=' in line)
    base='http://127.0.0.1:8796'
    def get():
        with urllib.request.urlopen(base+'/api/overlay',timeout=2) as r:return json.load(r)
    def post(s):
        req=urllib.request.Request(base+'/api/overlay',json.dumps(s).encode(),headers={'x-a2m':'1','Content-Type':'application/json'},method='POST')
        urllib.request.urlopen(req,timeout=2).close()
    def wait_geometry(width,height,position=(40,40)):
        # Creating the X11 window precedes Mesa shader initialization on cold CI.
        # Observe the requested result instead of assuming a fixed render latency.
        deadline=time.monotonic()+15
        while time.monotonic()<deadline:
            if p.poll() is not None: raise RuntimeError((Path(workspace.name)/'native.log').read_text())
            g=geometry()
            if abs(int(g['WIDTH'])-round(width))<=1 and abs(int(g['HEIGHT'])-round(height))<=1 and (int(g['X']),int(g['Y']))==position:
                return g
            time.sleep(.1)
        raise AssertionError(f'Expected {width}x{height} at {position}, observed {g}')
    print('Native start:',geometry(),flush=True)
    for scale in [1,1.5,2,2.5,.6,1]:
        s=get();s.update(scale=scale,compact=True,theme='aether',max_rows=5);post(s)
        g=wait_geometry(312*scale,compact_height*scale);print('Scale',scale,g,flush=True)
        assert abs(int(g['WIDTH'])-round(312*scale)) <= 1,g
        assert abs(int(g['HEIGHT'])-round(compact_height*scale)) <= 1,g
        assert (int(g['X']),int(g['Y']))==(40,40),g
        ImageGrab.grab(xdisplay=env['DISPLAY']).crop((0,0,int(g['WIDTH'])+90,int(g['HEIGHT'])+90)).save(output/('native-scale-'+str(scale)+'.png'))
    if fixture_mode:
        for theme in ['midnight','aether','ember']:
            for compact in [True,False]:
                for scene,color in [('dark','#101820'),('bright','#e8ddbf')]:
                    subprocess.run(['xsetroot','-solid',color],env=env,check=True)
                    s=get();s.update(scale=1,compact=compact,theme=theme,opacity=.6);post(s)
                    width=312 if compact else 360
                    height=compact_height if compact else 40+22+30*row_count+4
                    g=wait_geometry(width,height);time.sleep(.35)
                    ImageGrab.grab(xdisplay=env['DISPLAY']).crop((0,0,width+90,height+90)).save(output/f'native-{theme}-{compact}-{scene}.png')
        for scale in [.6,1,2.5]:
            s=get();s.update(compact=True,scale=scale,max_rows=24);post(s)
            g=wait_geometry(312*scale,(36+20+22*24+4)*scale)
            ImageGrab.grab(xdisplay=env['DISPLAY']).crop((0,0,int(g['WIDTH'])+90,int(g['HEIGHT'])+90)).save(output/f'native-24-rows-{scale}.png')
        s=get();s.update(compact=True,scale=1,max_rows=5);post(s);wait_geometry(312,compact_height)
        subprocess.run(['xsetroot','-solid','black'],env=env,check=True)
    s=get();s.update(visible=False,locked=True);post(s);time.sleep(.5)
    g=geometry(); hidden=ImageGrab.grab(xdisplay=env['DISPLAY']).crop((int(g['X']),int(g['Y']),int(g['X'])+int(g['WIDTH']),int(g['Y'])+int(g['HEIGHT'])))
    assert hidden.getbbox() is None,'Hidden overlay still paints content'
    print('Hidden:',g,flush=True)
    s.update(visible=True,locked=False,position=[150,200]);post(s)
    wait_geometry(312,compact_height,(150,200))
    print('Restored:',geometry(),flush=True)
    ImageGrab.grab(xdisplay=env['DISPLAY']).crop((100,150,500,400+compact_height)).save(output/'native-restored.png')
    p.terminate();p.wait(timeout=5);assert p.returncode==0,p.returncode
    p=subprocess.Popen([binary,'--x11','--db',str(Path(workspace.name)/'native.db'),'--port','8796'],env=env,stdout=log,stderr=log)
    for _ in range(100):
        if p.poll() is not None: raise RuntimeError((Path(workspace.name)/'native.log').read_text())
        r=subprocess.run([str(root/'bin/xdotool'),'search','--name','AION2 Meter'],env=env,capture_output=True,text=True)
        if r.returncode==0: break
        time.sleep(.1)
    assert r.returncode==0,r.stderr
    window=r.stdout.strip().splitlines()[-1]
    wait_geometry(312,compact_height,(150,200))
    p.terminate();p.wait(timeout=5);assert p.returncode==0
    print('PASS native X11 start, appearance controls, visibility and graceful termination',flush=True)
except Exception:
    log.flush();xlog.flush()
    print('Native log:',(Path(workspace.name)/'native.log').read_text(),flush=True)
    print('Xvfb log:',(Path(workspace.name)/'xvfb.log').read_text(),flush=True)
    raise
finally:
    if p and p.poll() is None:p.kill();p.wait()
    x.terminate();x.wait(timeout=5)
    workspace.cleanup()
