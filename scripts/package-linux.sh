#!/usr/bin/env bash
# Reproducible layout: tar installer, Debian package and Fedora RPM from one binary.
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\([0-9.]*\)"/\1/p' Cargo.toml | head -1)
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
binary=${METER_BINARY:-${CARGO_TARGET_DIR:-target}/release/aion2-meter}
[[ -x "$binary" ]] || { echo "Release-Binary fehlt: cargo build --release --locked" >&2; exit 1; }
[[ $("$binary" --version) == "aion2-meter $version" ]] || { echo "Binary-Version passt nicht zu Cargo.toml" >&2; exit 1; }
readelf -h "$binary" | grep -q 'Advanced Micro Devices X86-64' || { echo "Nur x86_64-Binaries unterstützt" >&2; exit 1; }
glibc=$(readelf --version-info "$binary" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -Vu | tail -1)
[[ "$glibc" =~ ^[0-9]+\.[0-9]+$ ]]
mkdir -p dist
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
base="$stage/aion2-meter-$version-linux-x86_64"
mkdir -p "$base/packaging" "$base/scripts"
install -m755 "$binary" "$base/aion2-meter"
cp packaging/*.desktop packaging/*.svg "$base/packaging/"
cp scripts/install-binary.sh scripts/install-kwin-rule.sh scripts/install-shortcuts.sh scripts/desktop-exec.sh "$base/scripts/"
cp README.md LICENSE "$base/"
cp -a docs "$base/docs"
tar -C "$stage" -czf "dist/aion2-meter-$version-linux-x86_64.tar.gz" "${base##*/}"
root="$stage/deb"
install -Dm755 "$binary" "$root/usr/bin/aion2-meter"
install -Dm644 packaging/aion2-meter.desktop "$root/usr/share/applications/aion2-meter.desktop"
install -Dm644 packaging/aion2-meter-dashboard.desktop "$root/usr/share/applications/aion2-meter-dashboard.desktop"
install -Dm644 packaging/aion2-meter.svg "$root/usr/share/icons/hicolor/scalable/apps/aion2-meter.svg"
install -Dm644 LICENSE "$root/usr/share/doc/aion2-meter/copyright"
install -Dm755 scripts/install-kwin-rule.sh "$root/usr/lib/aion2-meter/install-kwin-rule.sh"
install -Dm755 scripts/install-shortcuts.sh "$root/usr/lib/aion2-meter/install-shortcuts.sh"
install -Dm644 scripts/desktop-exec.sh "$root/usr/lib/aion2-meter/desktop-exec.sh"
cp README.md "$root/usr/share/doc/aion2-meter/"
cp -a docs "$root/usr/share/doc/aion2-meter/docs"
mkdir -p "$root/DEBIAN"
cat > "$root/DEBIAN/control" <<EOF
Package: aion2-meter
Version: $version
Architecture: amd64
Maintainer: damage-meter contributors <noreply@github.com>
Depends: libc6 (>= $glibc), libgcc-s1, libxkbcommon0, libegl1, libgl1, libcap2-bin, xdg-utils
Section: games
Priority: optional
Homepage: https://github.com/feroxtwo/damage-meter
Description: Local AION 2 damage meter for Linux
 Native overlay, local combat history and browser dashboard.
EOF
cat > "$root/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = configure ]; then
    setcap cap_net_raw=ep /usr/bin/aion2-meter
fi
EOF
chmod 755 "$root/DEBIAN/postinst"
dpkg-deb --root-owner-group --build "$root" "dist/aion2-meter_${version}_amd64.deb"
if command -v rpmbuild >/dev/null; then
    rpmdir="$stage/rpm"
    mkdir -p "$rpmdir"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
    # RPM has file capabilities in its manifest, reapplied on every upgrade.
    cp -a "$root/usr" "$rpmdir/SOURCES/usr"
    cat > "$rpmdir/SPECS/aion2-meter.spec" <<EOF
Name: aion2-meter
Version: $version
Release: 1
Summary: Local AION 2 damage meter for Linux
License: GPL-3.0-or-later
URL: https://github.com/feroxtwo/damage-meter
BuildArch: x86_64
Requires: libxkbcommon, libglvnd-egl, libglvnd-glx, xdg-utils
%description
Native overlay, local combat history and browser dashboard.
%install
mkdir -p %{buildroot}
cp -a %{_sourcedir}/usr %{buildroot}/
%files
%caps(cap_net_raw=ep) /usr/bin/aion2-meter
/usr/share/applications/aion2-meter*.desktop
/usr/share/icons/hicolor/scalable/apps/aion2-meter.svg
%license /usr/share/doc/aion2-meter/copyright
%doc /usr/share/doc/aion2-meter/README.md
%doc /usr/share/doc/aion2-meter/docs
/usr/lib/aion2-meter/
EOF
    rpmbuild -bb --define "_topdir $rpmdir" --define '_build_id_links none' --define '__os_install_post %{nil}' "$rpmdir/SPECS/aion2-meter.spec"
    cp "$rpmdir"/RPMS/x86_64/*.rpm dist/
else
    echo "RPM ausgelassen: rpmbuild fehlt (Fedora: rpm-build, Ubuntu: rpm)." >&2
fi
# Manifest includes only this release, even if dist contains older artifacts.
artifacts=("aion2-meter-$version-linux-x86_64.tar.gz" "aion2-meter_${version}_amd64.deb")
[[ ! -f "dist/aion2-meter-$version-1.x86_64.rpm" ]] || artifacts+=("aion2-meter-$version-1.x86_64.rpm")
(cd dist && sha256sum "${artifacts[@]}" > SHA256SUMS)
