#!/usr/bin/env python3
"""Add the Linux runtime and BAS service manifest to an NSH APK; sign for development."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apk', type=Path, required=True)
    parser.add_argument('--abi', choices=['arm64-v8a', 'x86_64'], required=True)
    parser.add_argument('--sdk', type=Path, required=True)
    parser.add_argument('--tools', type=Path, required=True)
    args = parser.parse_args()
    app = json.loads((ROOT / 'bin/bas-mobile/nsh.json').read_text())
    runtime = ROOT / 'target/mobile-runtime' / args.abi
    manifest = json.loads((runtime / 'assets/bas-runtime/manifest.json').read_text())
    if manifest['abi'] != args.abi:
        raise RuntimeError('Runtime ABI mismatch')
    with tempfile.TemporaryDirectory(dir=args.apk.parent) as temp:
        work = Path(temp)
        base = work / 'manifest.apk'
        subprocess.run([str(args.tools / 'aapt2'), 'link', '-o', str(base),
                        '-I', str(args.sdk / 'platforms/android-35/android.jar'),
                        '--manifest', str(ROOT / 'bin/bas-mobile/android/AndroidManifest.xml'),
                        '--version-code', str(app.get('build', 1)),
                        '--version-name', app.get('version', '1.0.0')], check=True)
        unsigned = work / 'unsigned.apk'
        with zipfile.ZipFile(args.apk) as original, zipfile.ZipFile(base) as compiled, zipfile.ZipFile(unsigned, 'w') as output:
            for item in original.infolist():
                if item.filename == 'AndroidManifest.xml' or item.filename.startswith('META-INF/'):
                    continue
                output.writestr(item, original.read(item))
            output.writestr('AndroidManifest.xml', compiled.read('AndroidManifest.xml'))
            for path in sorted((runtime / 'assets').rglob('*')):
                if path.is_file():
                    output.write(path, 'assets/' + str(path.relative_to(runtime / 'assets')), compress_type=zipfile.ZIP_STORED)
            for path in sorted((runtime / 'jniLibs' / args.abi).glob('*.so')):
                output.write(path, f'lib/{args.abi}/{path.name}', compress_type=zipfile.ZIP_STORED)
        aligned = work / 'aligned.apk'
        subprocess.run([str(args.tools / 'zipalign'), '-P', '16', '-f', '4', str(unsigned), str(aligned)], check=True)
        subprocess.run([str(args.tools / 'apksigner'), 'sign', '--ks', str(Path.home() / '.android/debug.keystore'),
                        '--ks-key-alias', 'androiddebugkey', '--ks-pass', 'pass:android',
                        '--out', str(work / 'signed.apk'), str(aligned)], check=True)
        subprocess.run([str(args.tools / 'apksigner'), 'verify', str(work / 'signed.apk')], check=True)
        (work / 'signed.apk').replace(args.apk)
    print(f'Local-service APK: {args.apk}')


if __name__ == '__main__':
    main()
