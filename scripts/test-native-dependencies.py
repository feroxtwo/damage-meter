#!/usr/bin/env python3
"""Simulate the observed missing X11 library without modifying system libraries."""
import os
from pathlib import Path
import subprocess
import tempfile

binary=str(Path(os.environ.get('METER_BINARY','target/release/aion2-meter')).resolve())
with tempfile.TemporaryDirectory(prefix='a2m-library-') as tmp:
    root=Path(tmp);source=root/'missing.c';shim=root/'missing.so'
    source.write_text('''#define _GNU_SOURCE
#include <dlfcn.h>
#include <string.h>
void *dlopen(const char *name, int flags) {
    if (name && strcmp(name, "libxkbcommon-x11.so.0") == 0) return 0;
    void *(*next)(const char *, int) = dlsym(RTLD_NEXT, "dlopen");
    return next(name, flags);
}
''')
    subprocess.run(['cc','-shared','-fPIC','-Wall','-Werror',str(source),'-o',str(shim),'-ldl'],check=True)
    env={**os.environ,'LD_PRELOAD':str(shim)}
    result=subprocess.run([binary,'--x11','--db',str(root/'unused.db')],env=env,capture_output=True,text=True,timeout=5)
    assert result.returncode==1,result.stderr
    assert 'libxkbcommon-x11.so.0 fehlt' in result.stderr,result.stderr
    assert 'sudo apt install libxkbcommon-x11-0' in result.stderr
    assert 'panicked' not in result.stderr
    assert not (root/'unused.db').exists()
print('PASS missing X11 library: actionable startup error, no panic, no partial database')
