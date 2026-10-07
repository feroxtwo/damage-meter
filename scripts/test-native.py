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
x=subprocess.Popen([os.environ.get('XVFB_BINARY',shutil.which('Xvfb') or 'Xvfb'),':91','-screen','0','1600x1000x24','-fp',str(root/'share/fonts/X11/misc'),'-nolisten','unix','-nolisten','local','-listen','tcp','-ac'],env=env,stdout=xlog,stderr=xlog)
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
    time.sleep(.5)
    print('Native start:',geometry(),flush=True)
    for scale in [1,1.5,2,2.5,.6,1]:
        s=get();s.update(scale=scale,compact=True,theme='aether');post(s);time.sleep(.8)
        g=geometry();print('Scale',scale,g,flush=True)
        assert abs(int(g['WIDTH'])-round(360*scale)) <= 1,g
        assert abs(int(g['HEIGHT'])-round(88*scale)) <= 1,g
        assert (int(g['X']),int(g['Y']))==(40,40),g
        ImageGrab.grab(xdisplay=env['DISPLAY']).save(output/('native-scale-'+str(scale)+'.png'))
    s=get();s.update(visible=False,locked=True);post(s);time.sleep(.5)
    g=geometry(); hidden=ImageGrab.grab(xdisplay=env['DISPLAY']).crop((int(g['X']),int(g['Y']),int(g['X'])+int(g['WIDTH']),int(g['Y'])+int(g['HEIGHT'])))
    assert hidden.getbbox() is None,'Hidden overlay still paints content'
    print('Hidden:',g,flush=True)
    s.update(visible=True,locked=False,position=[150,200]);post(s);time.sleep(.5)
    assert (int(geometry()['X']),int(geometry()['Y']))==(150,200)
    print('Restored:',geometry(),flush=True)
    ImageGrab.grab(xdisplay=env['DISPLAY']).save(output/'native-restored.png')
    p.terminate();p.wait(timeout=5);assert p.returncode==0,p.returncode
    p=subprocess.Popen([binary,'--x11','--db',str(Path(workspace.name)/'native.db'),'--port','8796'],env=env,stdout=log,stderr=log)
    time.sleep(1)
    r=subprocess.run([str(root/'bin/xdotool'),'search','--name','AION2 Meter'],env=env,capture_output=True,text=True,check=True)
    window=r.stdout.strip().splitlines()[-1]
    assert (int(geometry()['X']),int(geometry()['Y']))==(150,200)
    assert int(geometry()['HEIGHT'])==88
    p.terminate();p.wait(timeout=5);assert p.returncode==0
    print('PASS native X11 start, appearance controls, visibility and graceful termination',flush=True)
finally:
    if p and p.poll() is None:p.kill();p.wait()
    x.terminate();x.wait(timeout=5)
    workspace.cleanup()
