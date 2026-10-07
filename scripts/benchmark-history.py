#!/usr/bin/env python3
"""Repeatable synthetic history/load probe. Results are host-specific, not in-game evidence."""
import concurrent.futures
import json
import os
from pathlib import Path
import socket
import sqlite3
import statistics
import subprocess
import tempfile
import time
import urllib.request

binary=str(Path(os.environ.get('METER_BINARY','target/release/aion2-meter')).resolve())
count=int(os.environ.get('BENCH_FIGHTS','5000'))
with tempfile.TemporaryDirectory(prefix='a2m-bench-') as tmp:
    db=Path(tmp)/'history.db'
    with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    base=f'http://127.0.0.1:{port}'
    def get(path):
        with urllib.request.urlopen(base+path,timeout=15) as r:return json.load(r)
    def start():
        begin=time.perf_counter();pidfile=Path(tmp)/'host.pid';p=subprocess.Popen([binary,'--no-overlay','--db',str(db),'--port',str(port)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,preexec_fn=lambda:pidfile.write_text(os.readlink('/proc/self')))
        p.host_pid=pidfile.read_text()
        for _ in range(200):
            try:get('/api/live');return p,(time.perf_counter()-begin)*1000
            except Exception:
                assert p.poll() is None;time.sleep(.025)
        raise AssertionError('Startup timed out')
    def stop(p):p.terminate();p.wait(timeout=5);assert p.returncode==0
    p,_=start();stop(p)
    with sqlite3.connect(db) as c:
        runs=(count+9)//10
        for i in range(runs):
            c.execute("INSERT INTO runs(id,dungeon_id,dungeon_name,difficulty,started_at,ended_at,character) VALUES(?,600093,'Synthetic Den','Schwer',?,?,'Me')",(i+1,i*1000000,i*1000000+900000))
            for actor in range(4):c.execute('INSERT INTO run_members(run_id,name,job,is_self) VALUES(?,?,?,?)',(i+1,'Me' if actor==0 else f'Player{actor}','검성',int(actor==0)))
        for i in range(count):
            name=f'Synthetic Boss {i%8}';fid=f'bench{i}';start_ms=i*100000
            record={'id':fid,'bossName':name,'targetId':50000,'startTimeMs':start_ms,'durationMs':10000,'totalDamage':4000000,'jobs':[],'details':{'targetId':50000,'totalTargetDamage':4000000,'battleTime':10000,'skills':[]},'actors':[]}
            c.execute('INSERT INTO fights(id,run_id,boss_name,target_id,dungeon_id,started_at,duration_ms,total_damage,record_json) VALUES(?,?,?,50000,600093,?,10000,4000000,?)',(fid,i//10+1,name,start_ms,json.dumps(record)))
            for actor in range(4):c.execute('INSERT INTO fight_players(fight_id,actor_id,name,job,damage,dps,share,heal,is_self) VALUES(?,?,?,?,1000000,100000,25,0,?)',(fid,actor+1,'Me' if actor==0 else f'Player{actor}','검성',int(actor==0)))
    p,start_ms=start()
    def resource():
        status=Path(f'/proc/{p.host_pid}/status').read_text();rss=int(next(l.split()[1] for l in status.splitlines() if l.startswith('VmRSS:')))
        stat=Path(f'/proc/{p.host_pid}/stat').read_text().rsplit(')',1)[1].split();cpu=(int(stat[11])+int(stat[12]))/os.sysconf('SC_CLK_TCK')
        return rss,cpu
    try:
        assert get('/api/stats/summary')['fights']==count
        assert len(get('/api/fights/bench0')['players'])==4
        before,cpu_before=resource();begin=time.perf_counter()
        paths=['/api/live','/api/fights?query=Boss&character=Me','/api/runs?character=Me','/api/stats/summary','/api/stats/boss-history','/api/runs/1','/api/fights/bench0']
        timings={path:[] for path in paths}
        def fetch(path):
            begin=time.perf_counter();get(path);return path,(time.perf_counter()-begin)*1000
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            for path,ms in pool.map(fetch,paths*20):timings[path].append(ms)
        after,cpu_after=resource();elapsed=time.perf_counter()-begin
        # Observe idle memory/CPU separately; a short probe does not establish absence of leaks.
        time.sleep(5);idle_rss,idle_cpu=resource()
        result={'synthetic':True,'fights':count,'runs':runs,'players_per_fight':4,'startup_ms':round(start_ms,2),'requests':140,'workers':4,'elapsed_s':round(elapsed,2),'cpu_s':round(cpu_after-cpu_before,2),'rss_kib_before':before,'rss_kib_after':after,'rss_kib_idle':idle_rss,'idle_cpu_s_over_5s':round(idle_cpu-cpu_after,2),'endpoints':{path:{'median_ms':round(statistics.median(v),2),'p95_ms':round(sorted(v)[int(len(v)*.95)-1],2),'max_ms':round(max(v),2)} for path,v in timings.items()},'limitations':['Synthetic history only; no network capture/event-rate benchmark.','Shared container and no long-running session; not proof of absence of memory leaks.']}
        print(json.dumps(result,indent=2))
    finally:stop(p)
