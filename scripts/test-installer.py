#!/usr/bin/env python3
"""Exercise atomic upgrades and desktop escaping without acquiring capture privileges."""
import os
from pathlib import Path
import subprocess
import tempfile
import time

binary = Path(os.environ.get('METER_BINARY', 'target/release/aion2-meter')).resolve()
with tempfile.TemporaryDirectory(prefix='a2m-installer-') as tmp:
    root = Path(tmp)
    prefix = root / 'a space $with%quote"and\\slash'
    stubs = root / 'stubs'; stubs.mkdir()
    # These only simulate the privileged preparation step, not real setcap support.
    for name, body in [('sudo', 'exec "$@"'), ('setcap', 'exit "${FAIL_CAP:-0}"'), ('busctl', 'exit 0')]:
        file=stubs/name;file.write_text('#!/bin/sh\n'+body+'\n');file.chmod(0o755)
    env={**os.environ, 'PATH':str(stubs)+':'+os.environ['PATH'], 'PREFIX':str(prefix), 'METER_BINARY':str(binary), 'XDG_CURRENT_DESKTOP':''}
    def install(fail=False):
        result=subprocess.run(['bash','scripts/install-binary.sh'],env={**env,'FAIL_CAP':'1' if fail else '0'},capture_output=True,text=True,timeout=10)
        assert (result.returncode!=0)==fail,result.stderr
    install()
    installed=prefix/'bin/aion2-meter'
    assert subprocess.check_output([installed,'--version'],text=True).strip()==subprocess.check_output([binary,'--version'],text=True).strip()
    desktop=(prefix/'share/applications/aion2-meter.desktop').read_text()
    quoted=str(installed).replace('\\','\\'*4).replace('"','\\'*2+'"').replace('$','\\'*2+'$').replace('`','\\'*2+'`').replace('%','%%')
    assert 'Exec="'+quoted+'"\n' in desktop,desktop
    if __import__('shutil').which('desktop-file-validate'):
        subprocess.run(['desktop-file-validate',str(prefix/'share/applications/aion2-meter.desktop')],check=True)
    # Only file generation is checked: a stub cannot prove KDE registration.
    shortcut_env={**env,'BIN':str(installed),'XDG_DATA_HOME':str(prefix/'share')}
    subprocess.run(['bash','scripts/install-shortcuts.sh'],env=shortcut_env,check=True,stdout=subprocess.DEVNULL)
    shortcuts=list((prefix/'share/applications').glob('net.local.aion2-meter-*.desktop'))
    assert len(shortcuts)==3
    for file in shortcuts:assert 'Exec="'+quoted+'" ctl ' in file.read_text()
    subprocess.run(['bash','scripts/install-shortcuts.sh','--remove'],env=shortcut_env,check=True,stdout=subprocess.DEVNULL)
    assert not list((prefix/'share/applications').glob('net.local.aion2-meter-*.desktop'))
    before=installed.stat().st_ino
    proc=subprocess.Popen([installed,'--no-overlay','--port','0','--db',str(root/'settings.db')],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        time.sleep(.4);assert proc.poll() is None
        install();assert installed.stat().st_ino!=before;assert proc.poll() is None
        before=installed.stat().st_ino;install(True);assert installed.stat().st_ino==before
        assert not list((prefix/'bin').glob('.aion2-meter.*'))
    finally:
        proc.terminate();proc.wait(timeout=5);assert proc.returncode==0
print('PASS installer: fresh install, running upgrade, failed preparation, escaped Exec, cleanup')
