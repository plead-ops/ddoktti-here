"""Validate the installed/copy form, not only the build directory signature."""
import json
import plistlib
import subprocess
import tempfile
from pathlib import Path

bundle = Path('apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle').resolve()
version = json.loads(Path('apps/desktop/src-tauri/tauri.conf.json').read_text())['version']

def check(app):
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    assert info['CFBundleShortVersionString'] == version
    assert info['CFBundleIconFile'] == 'icon.icns'
    subprocess.run(['codesign', '--verify', '--deep', '--strict', '--verbose=2', str(app)], check=True)
    subprocess.run(['xcrun', 'stapler', 'validate', str(app)], check=True)
    subprocess.run(['spctl', '--assess', '--type', 'execute', '--verbose=2', str(app)], check=True)
    subprocess.run(['lipo', str(app / 'Contents/MacOS/ddoktti-here'), '-verify_arch', 'arm64', 'x86_64'], check=True)

with tempfile.TemporaryDirectory(prefix='ddoktti-install-') as temporary:
    root = Path(temporary)
    mount = root / 'mount'
    mount.mkdir()
    dmg, = (bundle / 'dmg').glob('*.dmg')
    subprocess.run(['hdiutil', 'attach', '-readonly', '-nobrowse', '-mountpoint', str(mount), str(dmg)], check=True)
    try:
        source, = mount.glob('*.app')
        installed = root / 'installed' / source.name
        installed.parent.mkdir()
        subprocess.run(['ditto', str(source), str(installed)], check=True)
        check(installed)
    finally:
        subprocess.run(['hdiutil', 'detach', str(mount)], check=True)
    archive, = (bundle / 'macos').glob('*.app.tar.gz')
    unpacked = root / 'updated'
    unpacked.mkdir()
    subprocess.run(['tar', '-xzf', str(archive), '-C', str(unpacked)], check=True)
    updated, = unpacked.glob('*.app')
    check(updated)
print('DMG installation and updater extraction signatures verified')
