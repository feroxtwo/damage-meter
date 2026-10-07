#!/usr/bin/env python3
"""Inspect generated packages and manifest without installing system files."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import re

version=re.search(r'^version = "([\d.]+)"',Path('Cargo.toml').read_text(),re.M)[1]
binary=Path(os.environ.get('METER_BINARY','target/release/aion2-meter'))
def digest(p):
    with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
expected=digest(binary);dist=Path('dist');archive=dist/f'aion2-meter-{version}-linux-x86_64.tar.gz';deb=dist/f'aion2-meter_{version}_amd64.deb';rpm=dist/f'aion2-meter-{version}-1.x86_64.rpm'
with tarfile.open(archive) as t:
    prefix=f'aion2-meter-{version}-linux-x86_64/'
    names=t.getnames();assert all(not n.startswith('/') and '..' not in Path(n).parts for n in names)
    for name in ['aion2-meter','scripts/install-binary.sh','README.md','docs/RELEASE_REVIEW_0.3.1.md','docs/INGAME_ACCEPTANCE.md']:
        assert prefix+name in names,name
    assert hashlib.sha256(t.extractfile(prefix+'aion2-meter').read()).hexdigest()==expected
with tempfile.TemporaryDirectory(prefix='a2m-package-inspect-') as tmp:
    subprocess.run(['dpkg-deb','--extract',str(deb),tmp],check=True)
    assert digest(Path(tmp)/'usr/bin/aion2-meter')==expected
    assert (Path(tmp)/'usr/share/doc/aion2-meter/docs/INGAME_ACCEPTANCE.md').exists()
    controls=Path(tmp)/'control';subprocess.run(['dpkg-deb','--control',str(deb),str(controls)],check=True)
    assert 'setcap cap_net_raw=ep /usr/bin/aion2-meter' in (controls/'postinst').read_text()
    fields=subprocess.check_output(['dpkg-deb','--field',str(deb),'Version','Architecture','Depends'],text=True)
    assert version in fields and 'amd64' in fields and 'libcap2-bin' in fields and 'libc6 (>=' in fields
if rpm.exists():
    assert shutil.which('rpm'),'rpm required to inspect generated RPM'
    fields=subprocess.check_output(['rpm','-qp','--qf','%{VERSION} %{ARCH}\n[%{FILENAMES} %{FILECAPS}\n]',str(rpm)],text=True)
    assert version+' x86_64' in fields
    assert '/usr/bin/aion2-meter cap_net_raw=ep' in fields
    assert '/usr/share/doc/aion2-meter/docs/INGAME_ACCEPTANCE.md' in fields
manifest={}
for line in (dist/'SHA256SUMS').read_text().splitlines():
    checksum,name=line.split(maxsplit=1);name=name.strip().lstrip('*');assert digest(dist/name)==checksum;manifest[name]=checksum
files={archive.name,deb.name}|({rpm.name} if rpm.exists() else set())
assert set(manifest)==files,manifest
print('PASS packages: current release only, archive paths/docs/binary, DEB metadata/postinst, RPM metadata/capability, all checksums')
