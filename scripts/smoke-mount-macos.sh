#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
ENGINE="${1:-target/debug/storage-bridge}"
python3 - "$ENGINE" <<'PY'
import ctypes, errno, hashlib, json, os, pathlib, subprocess, sys, time, uuid
engine = str(pathlib.Path(sys.argv[1]).resolve())
instance = uuid.uuid4().hex
args = [engine, '--demo', '--test-instance', instance]
process = subprocess.Popen(args, stdout=subprocess.DEVNULL)

def request(method, params=None):
    output = subprocess.check_output(args + ['--request', json.dumps({'version': 1, 'method': method, 'params': params or {}})], stderr=subprocess.PIPE)
    return json.loads(output)['result']

def wait_for(check, timeout=30):
    deadline = time.monotonic() + timeout
    while True:
        try:
            result = check()
            if result:
                return result
        except subprocess.CalledProcessError:
            pass
        if time.monotonic() >= deadline:
            raise TimeoutError('Engine did not reach expected state')
        time.sleep(.2)

mounted = False
try:
    wait_for(lambda: request('status'))
    request('mount')
    mounted = True
    root = pathlib.Path.home() / 'StorageBridge'
    mounts = subprocess.check_output(['/sbin/mount'], text=True).splitlines()
    volume = next(line for line in mounts if f' on {root} (' in line)
    assert 'local' in volume.rsplit('(', 1)[1].rstrip(')').split(', '), volume
    stats = os.statvfs(root)
    assert stats.f_frsize > 0 and stats.f_blocks > 0, stats
    assert os.access(root, os.R_OK | os.W_OK | os.X_OK), 'Owner cannot access mounted root'
    assert subprocess.check_output(['/usr/bin/xattr', '-l', str(root)]) == b'', 'Unexpected root extended attributes'
    # Finder resolves folders through CoreServices, including privileged helper requests.
    core = ctypes.CDLL('/System/Library/Frameworks/CoreServices.framework/CoreServices')
    ref = ctypes.create_string_buffer(80)
    assert core.FSPathMakeRef(os.fsencode(root), ref, None) == 0, 'Finder cannot resolve mounted root'
    folder = root / ('smoke-' + instance)
    folder.mkdir()
    data = bytes(range(256)) * (17 * 4096)
    file = folder / 'large.bin'
    file.write_bytes(data)
    wait_for(lambda: request('status')['pending'] == 0)
    assert hashlib.sha256(file.read_bytes()).digest() == hashlib.sha256(data).digest()
    with file.open('r+b') as stream:
        stream.seek(1024 * 1024 + 17)
        stream.write(b'partial-edit')
        stream.flush()
        os.fsync(stream.fileno())
    expected = data[:1024 * 1024 + 17] + b'partial-edit' + data[1024 * 1024 + 29:]
    wait_for(lambda: request('status')['pending'] == 0)
    assert file.read_bytes() == expected
    replacement = folder / 'replacement.tmp'
    replacement.write_bytes(b'replaced')
    replacement.replace(file)
    assert file.read_bytes() == b'replaced'
    renamed = folder / 'renamed.bin'
    file.rename(renamed)
    assert renamed.name in os.listdir(folder)
    request('settings', {'content_limit': 64 * 1024 * 1024})
    bounded = folder / 'bounded.bin'
    with bounded.open('w+b', buffering=0) as stream:
        stream.truncate(64 * 1024 * 1024)
        stream.write(b'Z')
        stream.seek(64 * 1024 * 1024)
        try:
            stream.write(b'!')
        except OSError as error:
            assert error.errno == errno.ENOSPC, error
        else:
            raise AssertionError('Write beyond content allowance was accepted')
        usage = request('status')['storage']
        assert usage['used_bytes'] == 64 * 1024 * 1024 and usage['write_blocked'], usage
        request('release_cache')
        stream.seek(0)
        assert stream.read(1) == b'Z', 'Accepted bytes lost after rejected write'
    wait_for(lambda: request('status')['pending'] == 0 and request('status')['cache_bytes'] == 0)
    assert not request('status')['storage']['write_blocked']
    bounded.unlink()
    renamed.unlink()
    folder.rmdir()
    wait_for(lambda: request('status')['pending'] == 0)
    request('unmount')
    mounted = False
    request('shutdown')
    process.wait(timeout=10)
    assert process.returncode == 0
    print('PASS: mounted demo operations, enforced content allowance, ENOSPC preservation/recovery, unmount')
finally:
    if process.poll() is None:
        try:
            if mounted:
                request('unmount')
            request('shutdown')
            process.wait(timeout=10)
        except Exception:
            print('Demo engine left running for inspection; preserve its pending data.', file=sys.stderr)
PY
