#!/usr/bin/env python3
"""Build the Linux guest and Android PRoot executables from pinned inputs."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[2]
INPUTS = json.loads(Path(__file__).with_name('inputs.json').read_text())


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def fetch(spec, cache):
    path = cache / spec['url'].rsplit('/', 1)[-1]
    if not path.exists() or digest(path) != spec['sha256']:
        temporary = path.with_suffix('.download')
        subprocess.run(['curl', '--fail', '--location', '--retry', '3', '--output', str(temporary), spec['url']], check=True)
        if digest(temporary) != spec['sha256']:
            raise RuntimeError(f'Checksum mismatch: {spec["url"]}')
        temporary.replace(path)
    return path


def extract_deb(path, destination):
    members = subprocess.check_output(['ar', 't', str(path)], text=True).splitlines()
    member = next(name for name in members if name.startswith('data.tar'))
    data = subprocess.check_output(['ar', 'p', str(path), member])
    if member.endswith('.zst'):
        data = subprocess.check_output(['zstd', '--decompress', '--stdout'], input=data)
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        archive.extractall(destination, filter='data')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--abi', choices=INPUTS, default='arm64-v8a')
    args = parser.parse_args()
    spec = INPUTS[args.abi]
    target = spec['target']
    subprocess.run(['cargo', 'zigbuild', '--locked', '-p', 'bas', '-p', 'acp-test-agent', '-p', 'bas-browser-bridge', '--release', '--target', target + '.2.35'], cwd=ROOT, check=True)
    output = ROOT / 'target/mobile-runtime' / args.abi
    cache = ROOT / 'target/mobile-runtime/downloads'
    cache.mkdir(parents=True, exist_ok=True)
    work = output / 'work'
    shutil.rmtree(work, ignore_errors=True)
    rootfs = work / 'rootfs'
    rootfs.mkdir(parents=True)
    ubuntu = fetch(spec['ubuntu'], cache)
    # Verified upstream rootfs contains absolute symlinks intended for the guest.
    with tarfile.open(ubuntu) as archive:
        archive.extractall(rootfs, members=[m for m in archive if m.isfile() or m.isdir() or m.issym() or m.islnk()], filter='fully_trusted')
    # Official Node >= 25 needs libatomic, which Ubuntu Base does not include.
    for package in spec['guest_packages'].values():
        extract_deb(fetch(package, cache), rootfs)
    binaries = rootfs / 'opt/bas/bin'
    binaries.mkdir(parents=True)
    for name in ['bas', 'acp-test-agent', 'bas-open-url']:
        shutil.copy2(ROOT / 'target' / target / 'release' / name, binaries / name)
    (rootfs / 'usr/local/bin').mkdir(parents=True, exist_ok=True)
    (rootfs / 'usr/local/bin/xdg-open').symlink_to('/opt/bas/bin/bas-open-url')
    for name in ['home/bas', 'run/bas', 'tmp', 'dev', 'proc']:
        (rootfs / name).mkdir(parents=True, exist_ok=True)
    # DNS and CA roots must be usable without a Linux init system.
    (rootfs / 'etc/resolv.conf').unlink(missing_ok=True)
    (rootfs / 'etc/resolv.conf').write_text('nameserver 1.1.1.1\nnameserver 8.8.8.8\n')
    ca = Path(os.environ.get('SSL_CERT_FILE', '/etc/ssl/certs/ca-certificates.crt'))
    (rootfs / 'etc/ssl/certs').mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ca, rootfs / 'etc/ssl/certs/ca-certificates.crt')
    debs = work / 'termux'
    for package in spec['packages'].values():
        extract_deb(fetch(package, cache), debs)
    prefix = debs / 'data/data/com.termux/files/usr'
    libraries = output / 'jniLibs' / args.abi
    libraries.mkdir(parents=True, exist_ok=True)
    for src, dest in [('bin/proot', 'libproot.so'), ('libexec/proot/loader', 'libproot-loader.so'),
                      ('lib/libtalloc.so.2', 'libtalloc.so'), ('lib/libandroid-shmem.so', 'libandroid-shmem.so')]:
        shutil.copy2(prefix / src, libraries / dest)
    # Same-length SONAME substitution keeps the Termux ELF segment layout intact.
    proot = libraries / 'libproot.so'
    data = proot.read_bytes()
    if b'libtalloc.so.2\0' not in data:
        raise RuntimeError('PRoot no longer links libtalloc.so.2; review its dependencies')
    proot.write_bytes(data.replace(b'libtalloc.so.2\0', b'libtalloc.so\0\0\0'))
    assets = output / 'assets/bas-runtime'
    assets.mkdir(parents=True, exist_ok=True)
    def metadata(member):
        member.uid = member.gid = member.mtime = 0
        member.uname = member.gname = ''
        # Android SELinux denies hard-link creation in app storage. Materialize
        # these files while retaining the rootfs's ordinary symbolic links.
        if member.islnk():
            member.type = tarfile.REGTYPE
            member.linkname = ''
            member.size = (rootfs / member.name).stat().st_size
        return member
    with (assets / 'rootfs.tar.gz').open('wb') as output_file:
        with gzip.GzipFile(filename='', fileobj=output_file, mode='wb', mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', dereference=False) as archive:
                archive.add(rootfs, arcname='.', filter=metadata)
    manifest = {'abi': args.abi, 'sha256': digest(assets / 'rootfs.tar.gz'), 'inputs': spec}
    (assets / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    # Keep the package copyright notices alongside the runtime's own notices.
    notices = assets / 'licenses'
    notices.mkdir(exist_ok=True)
    for path in (prefix / 'share').rglob('copyright'):
        source = path if path.exists() else rootfs / 'usr/share/common-licenses' / Path(os.readlink(path)).stem.replace('.0', '')
        shutil.copy2(source, notices / (path.parent.name + '-copyright'))
    shutil.copy2(ROOT / 'LICENSE', notices / 'BAS-MIT')
    (notices / 'SOURCES.txt').write_text('Ubuntu: https://archive.ubuntu.com/ubuntu/ and https://ports.ubuntu.com/ubuntu-ports/\n'
        'PRoot: https://github.com/termux/proot\nTermux recipes: https://github.com/termux/termux-packages\n'
        'Pinned binary origins and hashes: ../manifest.json\n')
    print(output)


if __name__ == '__main__':
    main()
