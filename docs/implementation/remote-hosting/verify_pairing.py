#!/usr/bin/env python3
"""Exercise pairing authority through an installed binary and isolated Unix IPC."""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import struct
import subprocess
import time
import uuid


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class Fixture:
    def __init__(self, binary, root, source, hosting_available=False):
        self.root = root.resolve()
        self.root.mkdir(parents=True, exist_ok=False)
        for name in ('home', 'tmp', 'config', 'data'):
            (self.root / name).mkdir()
        self.binary = self.root / 'mxr'
        shutil.copy2(binary, self.binary)
        self.sock = '/tmp/mxr-pairing-' + uuid.uuid4().hex[:16] + '.sock'
        self.env = {k: os.environ[k] for k in ('PATH', 'LANG', 'TZ') if k in os.environ}
        self.env.update(HOME=str(self.root/'home'), TMPDIR=str(self.root/'tmp')+'/',
                        MXR_CONFIG_DIR=str(self.root/'config'), MXR_DATA_DIR=str(self.root/'data'),
                        MXR_SOCKET_PATH=self.sock, MXR_INSTANCE='pairing-'+uuid.uuid4().hex[:10],
                        MXR_ACTIVITY='off', MXR_KEYCHAIN='off', MXR_DAEMON_AUTOSTART='off',
                        MXR_SEMANTIC='off', RUST_LOG='info', RUST_MIN_STACK='16777216')
        (self.root/'config/config.toml').write_text('[bridge]\nenabled = false\n[search.semantic]\nenabled = false\nauto_download_models = false\n[llm]\nenabled = false\n')
        self.process = None
        self.log = None
        self.checks = []
        self.tokens = []
        self.hosting_available = hosting_available
        self.evidence = {'source': source, 'binary_sha256': digest(self.binary), 'checks': self.checks}

    def check(self, name, condition):
        if not condition:
            raise AssertionError(name)
        self.checks.append(name)

    def request(self, command):
        payload = json.dumps({'id': 1, 'source': 'cli', 'payload': {'type': 'Request', **command}}).encode()
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
            stream.settimeout(10)
            stream.connect(self.sock)
            stream.sendall(struct.pack('>I', len(payload)) + payload)
            def exact(n):
                result = b''
                while len(result) < n:
                    chunk = stream.recv(n-len(result))
                    if not chunk:
                        raise RuntimeError('Incomplete IPC response')
                    result += chunk
                return result
            while True:
                n = struct.unpack('>I', exact(4))[0]
                if n > 16*1024*1024:
                    raise RuntimeError('IPC response exceeds frame cap')
                response = json.loads(exact(n))
                if response['id'] == 1 and response['payload']['type'] == 'Response':
                    return response['payload']

    def remote(self, action, ok=True, **fields):
        response = self.request({'cmd': 'RemoteAuthority', 'request': {'action': action, **fields}})
        if ok:
            if response['status'] != 'Ok':
                raise AssertionError('Remote action failed: '+action)
            return response['data']['response']
        return response['status'] != 'Ok'

    def cli(self, *args, override=False, ok=True):
        env = self.env.copy()
        if override:
            env['MXR_DAEMON_ADDR'] = 'unix:///tmp/nonexistent-mxr-foreign-'+uuid.uuid4().hex+'.sock'
        result = subprocess.run([str(self.binary), *args, '--format', 'json'], env=env,
                                capture_output=True, text=True, timeout=15, stdin=subprocess.DEVNULL)
        if ok and result.returncode:
            raise AssertionError('CLI failed: '+' '.join(args))
        if not ok:
            return result.returncode != 0
        self.check('CLI redacts credentials', all(token not in result.stdout+result.stderr for token in self.tokens))
        return json.loads(result.stdout)

    def start(self):
        self.log = (self.root/'daemon.log').open('ab')
        self.process = subprocess.Popen([str(self.binary), 'daemon', '--foreground', '--no-bridge'],
                                        env=self.env, stdout=self.log, stderr=self.log, start_new_session=True)
        deadline = time.monotonic()+30
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError('Owned daemon exited during startup; inspect local fixture log')
            try:
                if self.request({'cmd':'Ping'})['status'] == 'Ok':
                    return
            except (OSError, RuntimeError):
                pass
            time.sleep(.1)
        raise RuntimeError('Owned daemon startup deadline exceeded')

    def stop(self):
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=25)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
                raise RuntimeError('Owned daemon required forced cleanup')
        if self.log:
            self.log.close()
        self.process = None

    def pairing(self, name):
        mail, cleanup = secrets.token_hex(32), secrets.token_hex(32)
        self.tokens.extend((mail, cleanup))
        response = self.remote('StartPairing', name=name,
                               mail_verifier=hashlib.sha256(mail.encode()).hexdigest(),
                               cleanup_verifier=hashlib.sha256(cleanup.encode()).hexdigest(),
                               capabilities=['mail_read'])
        return response['pairing']['request_id'], mail, cleanup

    def run(self):
        self.start()
        initial = self.remote('Status')['authority']
        self.check('Hosting capability matches the expected release', initial['hosting_available'] is self.hosting_available)
        self.check('Inspection does not initialize host authority', initial['host_id'] is None)
        self.cli('serve', 'status', override=True)
        self.cli('serve', 'clients')
        self.cli('serve', 'pair', 'list')
        self.check('CLI read paths do not initialize host authority', self.remote('Status')['authority']['host_id'] is None)
        request, mail, cleanup = self.pairing('Synthetic Mac client')
        self.check('Unapproved claim denied', self.remote('ClaimPairing', ok=False, request_id=request, mail_secret=mail, cleanup_secret=cleanup))
        self.cli('serve', 'pair', 'approve', request, '--dry-run', override=True)
        self.check('Approval preview preserves unapproved state', self.remote('ClaimPairing', ok=False, request_id=request, mail_secret=mail, cleanup_secret=cleanup))
        self.cli('serve', 'pair', 'approve', request)
        pending = self.remote('Pairings')['pairings']
        self.check('Approval without confirmation remains a preview', any(p['request_id'] == request and not p['approved'] for p in pending))
        self.cli('serve', 'pair', 'approve', request, '--confirm', override=True)
        def claim(_):
            return self.remote('ClaimPairing', request_id=request, mail_secret=mail, cleanup_secret=cleanup)['client']
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            identities = list(pool.map(claim, range(4)))
        client = identities[0]['client_id']
        self.check('Concurrent claim retries return one registration', {v['client_id'] for v in identities} == {client})
        self.check('Only one client registered', len(self.remote('Clients')['clients']) == 1)
        self.remote('Authenticate', client_id=client, mail_secret=mail)
        self.check('Cleanup credential cannot authenticate mail', self.remote('Authenticate',ok=False,client_id=client,mail_secret=cleanup))
        self.check('Mail credential cannot deregister', self.remote('Cleanup',ok=False,client_id=client,cleanup_secret=mail))
        authority = self.remote('Status')['authority']
        self.stop()
        self.start()
        self.check('Host identity survives restart', self.remote('Status')['authority'] == authority)
        self.remote('Authenticate', client_id=client, mail_secret=mail)
        self.cli('serve', 'clients', 'revoke', client, '--dry-run', override=True)
        self.remote('Authenticate', client_id=client, mail_secret=mail)
        self.check('Revoke preview leaves credential active', True)
        self.cli('serve', 'clients', 'revoke', client, '--confirm', override=True)
        self.check('Committed revocation rejects prior credential', self.remote('Authenticate',ok=False,client_id=client,mail_secret=mail))
        self.check('Claim retry cannot revive a revoked registration', self.remote('ClaimPairing',ok=False,request_id=request,mail_secret=mail,cleanup_secret=cleanup))
        self.remote('Cleanup',client_id=client,cleanup_secret=cleanup)
        self.remote('Cleanup',client_id=client,cleanup_secret=cleanup)
        self.check('Cleanup retry is idempotent after revocation',True)
        self.stop()
        log_text = (self.root/'daemon.log').read_text()
        self.check('Daemon log redacts credentials', all(token not in log_text for token in self.tokens))
        for path in (self.root/'data').rglob('*'):
            if path.is_file():
                contents = path.read_bytes()
                self.check('Runtime file contains no plaintext credential: '+path.name, all(token.encode() not in contents for token in self.tokens))
        self.evidence.update(result='passed', host_id=authority['host_id'], registration_id=client)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary',required=True,type=Path)
    parser.add_argument('--runtime',required=True,type=Path)
    parser.add_argument('--source',required=True)
    parser.add_argument('--hosting-available',action='store_true',
                        help='Expect the remote-serving build capability added after v0.6.94')
    args=parser.parse_args()
    fixture=Fixture(args.binary,args.runtime,args.source,args.hosting_available)
    started=time.monotonic()
    try:
        fixture.run()
    finally:
        fixture.stop()
        fixture.evidence['elapsed_seconds']=round(time.monotonic()-started,3)
        (fixture.root/'result.json').write_text(json.dumps(fixture.evidence,indent=2)+'\n')
    print(json.dumps(fixture.evidence))


if __name__=='__main__':
    main()
