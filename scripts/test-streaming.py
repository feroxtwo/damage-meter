#!/usr/bin/env python3
"""Production HTTP listener: opt-in, isolated routes, key rotation and privacy."""
import json, os, socket, subprocess, tempfile, time, urllib.request, urllib.error
from pathlib import Path

def port():
    with socket.socket() as s:
        s.bind(('127.0.0.1',0)); return s.getsockname()[1]
def request(url, body=None):
    headers={'x-a2m':'1'}
    if body is not None: headers['Content-Type']='application/json'
    req=urllib.request.Request(url,data=None if body is None else json.dumps(body).encode(),headers=headers)
    try:
        with urllib.request.urlopen(req,timeout=3) as r:return r.status,r.read(),r.headers
    except urllib.error.HTTPError as e:return e.code,e.read(),e.headers
with tempfile.TemporaryDirectory(prefix='a2m-stream-test-') as tmp:
    base='http://127.0.0.1:'+str(port())
    binary=Path(os.environ.get('METER_BINARY','target/release/aion2-meter')).resolve()
    proc=subprocess.Popen([str(binary),'--no-overlay','--port',base.rsplit(':',1)[1],'--stream-port','0','--db',str(Path(tmp)/'meter.db')],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            try:
                status,body,_=request(base+'/api/stream')
                config=json.loads(body)
                if status==200 and config['available']:break
            except (OSError,ValueError):pass
            time.sleep(.05)
        else:raise AssertionError('dashboard/stream listener did not start')
        link=config['url'];assert request(link)[0]==404
        assert request(base+'/api/overlay',{'stream_enabled':True})[0]==200
        status,body,headers=request(link+'live');assert status==200
        data=json.loads(body)
        assert set(data)<=set(['rows','overlay','target_name','battle_time_ms','target_hp','hp_estimated','combat_state','numeric_limited','ping_ms','capture','cooldowns','boss_cast'])
        assert not any(k in data for k in ['character','health','evidence','run_id'])
        assert headers['Referrer-Policy']=='no-referrer'
        assert request(link+'live',{})[0]==405
        assert request(link.split('/s/')[0]+'/api/runs')[0]==404
        assert request(base+'/api/stream/rotate',{})[0]==204
        assert request(link+'live')[0]==404
        new=json.loads(request(base+'/api/stream')[1])['url']
        assert new!=link and request(new+'live')[0]==200
        assert request(base+'/api/overlay',{'stream_enabled':False})[0]==200
        assert request(new+'live')[0]==404
    finally:
        proc.terminate();proc.wait(timeout=5);assert proc.returncode==0
print('PASS production OBS: opt-in, isolated GET routes, bounded snapshot, privacy headers, key rotation, disable')
