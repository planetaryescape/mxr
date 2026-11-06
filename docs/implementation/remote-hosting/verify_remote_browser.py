#!/usr/bin/env python3
"""Synthetic HTTPS browser journey with an owned daemon, proxy and two clients.

Python performs CA-aware REST pairing and browser grants, then seeds synthetic
selected-client state at mode 0600. Native mail commands use authenticated WSS,
trusting the fixture CA through scoped SSL_CERT_FILE. Native CLI HTTPS pairing,
mxr web launch and the desktop WebView remain unverified: macOS's HTTP verifier
requires OS trust, which this fixture does not change. Playwright ignores
certificate errors only in its isolated browser contexts.

No real mail, tailnet or service changes. Requires Node, OpenSSL, the web app's
installed Playwright and Chrome (or --browser-channel chromium). Supply a binary
with the embedded SPA. Default acceptance uses no diagnostic policy overrides.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import secrets
import ssl
import socket
import shutil
import sqlite3
import subprocess
import time
import urllib.error
import urllib.request
import uuid

from verify_remote_compose import RemoteComposeFixture


def free_port():
    with socket.socket() as stream:
        stream.bind(('127.0.0.1', 0))
        return stream.getsockname()[1]


class RemoteBrowserFixture(RemoteComposeFixture):
    def __init__(self, args):
        self.browser_channel = getattr(args, 'browser_channel', 'chrome')
        self.browser_only = getattr(args, 'browser_only', False)
        self.diagnostic_referrer = getattr(args, 'diagnostic_same_origin_referrer', False)
        self.diagnostic_owner = getattr(args, 'diagnostic_owner_conflict', False)
        self.native_acceptances = 0
        self.proxy_port = free_port()
        args.origin = f'https://127.0.0.1:{self.proxy_port}'
        gateway_port = free_port()
        while gateway_port == self.proxy_port:
            gateway_port = free_port()
        args.listen = f'127.0.0.1:{gateway_port}'
        super().__init__(args)
        if self.diagnostic_referrer:
            self.evidence['diagnostic_only'] = 'Login document meta referrer=same-origin overrides candidate no-referrer header; this run is not production acceptance.'
        self.helper = self.root / 'browser-fixture.mjs'
        shutil.copyfile(Path(__file__).with_suffix('.mjs'), self.helper)
        self.proxy = None
        self.browser = None
        self.proxy_log = None
        self.browser_log = None
        self.control_buffers = {}
        self.repo = Path(__file__).resolve().parents[3]
        self.env.update(TOKIO_WORKER_THREADS='5', RAYON_NUM_THREADS='5')
        self.env['SSL_CERT_FILE'] = str(self.root / 'tls/ca.pem')
        self.evidence['limitations'] = ['Native CLI HTTPS pairing/recovery and mxr web launch are not verified with the generated private CA: macOS rustls-platform-verifier requires OS trust. Pairing and browser grants use real CA-aware REST; synthetic selected state is seeded at mode 0600. Native mail commands still use real authenticated WSS. No OS trust changes.']
        for env in self.clients.values():
            env.update(SSL_CERT_FILE=self.env['SSL_CERT_FILE'], TOKIO_WORKER_THREADS='5', RAYON_NUM_THREADS='5')

    def certificate(self):
        tls = self.root / 'tls'
        tls.mkdir(mode=0o700)
        extension = tls / 'server.ext'
        extension.write_text('subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid,issuer\n')
        (tls / 'ca.cnf').write_text('[req]\ndistinguished_name=dn\nx509_extensions=ca\n[dn]\n[ca]\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid:always\n')
        commands = [
            ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1', '-subj', '/CN=mxr fixture CA', '-config', 'ca.cnf', '-keyout', 'ca.key', '-out', 'ca.pem'],
            ['req', '-newkey', 'rsa:2048', '-nodes', '-subj', '/CN=localhost', '-keyout', 'server.key', '-out', 'server.csr'],
            ['x509', '-req', '-in', 'server.csr', '-CA', 'ca.pem', '-CAkey', 'ca.key', '-CAcreateserial', '-days', '1', '-extfile', 'server.ext', '-out', 'server.pem'],
        ]
        for command in commands:
            subprocess.run(['openssl', *command], cwd=tls, check=True,
                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for key in tls.glob('*.key'):
            key.chmod(0o600)

    def proxy_start(self):
        self.proxy_log = (self.root / 'proxy.log').open('ab')
        self.proxy = subprocess.Popen(['node', str(self.helper), 'proxy'],
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=self.proxy_log, text=True, bufsize=1)
        self.proxy.stdin.write(json.dumps({'listen': self.proxy_port, 'upstream': self.listen,
                                         'certificate': str(self.root / 'tls/server.pem'),
                                         'key': str(self.root / 'tls/server.key')}) + '\n')
        self.proxy.stdin.flush()
        self.read_control(self.proxy, 15, 'HTTPS proxy startup')

    def read_control(self, process, timeout, stage):
        deadline = time.monotonic() + timeout
        buffer = self.control_buffers.get(process.pid, b'')
        while b'\n' not in buffer:
            ready, _, _ = select.select([process.stdout], [], [], max(0, deadline - time.monotonic()))
            if not ready:
                raise AssertionError(stage + ' deadline')
            chunk = os.read(process.stdout.fileno(), 65536)
            if not chunk:
                raise AssertionError(stage + ' exited')
            buffer += chunk
            if len(buffer) > 64 * 1024:
                raise AssertionError(stage + ' control response exceeded limit')
        line, buffer = buffer.split(b'\n', 1)
        self.control_buffers[process.pid] = buffer
        result = json.loads(line)
        if result.get('event') == 'failed':
            raise AssertionError('Browser step failed: ' + result.get('step', 'unknown'))
        return result

    def gateway_start(self):
        self.gateway_log = (self.root / 'gateway.log').open('ab')
        self.gateway = subprocess.Popen(
            [str(self.binary), 'serve', '--foreground', '--listen', self.listen,
             '--public-origin', self.origin], env=self.env,
            stdout=self.gateway_log, stderr=self.gateway_log, start_new_session=True)
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if self.gateway.poll() is not None:
                raise AssertionError('Owned browser gateway exited during startup')
            try:
                with urllib.request.urlopen('http://' + self.listen + '/', timeout=1) as response:
                    if response.status:
                        return
            except urllib.error.HTTPError as error:
                if error.code == 403:
                    return
            except OSError:
                pass
            time.sleep(.1)
        raise AssertionError('Browser gateway startup deadline')

    def rest(self, route, action, expected_status=200, **fields):
        request = urllib.request.Request(self.origin + '/api/v1/remote/' + route,
                                         data=json.dumps({'action': action, **fields}).encode(),
                                         headers={'Content-Type': 'application/json'}, method='POST')
        context = ssl.create_default_context(cafile=self.env['SSL_CERT_FILE'])
        try:
            with urllib.request.urlopen(request, context=context, timeout=15) as response:
                if response.status != expected_status:
                    raise AssertionError('Unexpected fixture REST status: ' + action)
                return json.load(response)['data']['response']
        except urllib.error.HTTPError as error:
            if error.code == expected_status:
                return None
            raise AssertionError('Fixture REST denied: ' + action + ' status ' + str(error.code)) from None

    def write_state(self, name, state):
        path = Path(self.clients[name]['MXR_CONFIG_DIR']) / 'client-connection.json'
        temporary = path.with_suffix('.fixture-tmp')
        with os.fdopen(os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as output:
            json.dump(state, output)
        temporary.replace(path)

    def pending_rest(self, name):
        mail, cleanup = secrets.token_hex(32), secrets.token_hex(32)
        self.tokens.extend((mail, cleanup))
        capabilities = ['mail_read', 'mail_send', 'mail_triage', 'mail_ai']
        pending = self.rest('pairings', 'StartPairing', name='Synthetic browser client ' + name,
                            capabilities=capabilities,
                            mail_verifier=hashlib.sha256(mail.encode()).hexdigest(),
                            cleanup_verifier=hashlib.sha256(cleanup.encode()).hexdigest())['pairing']
        self.check('Real HTTPS pairing carries the explicitly requested mail grants',
                   pending['capabilities'] == capabilities)
        return pending, mail, cleanup

    def pair_all(self, name):
        pending, mail, cleanup = self.pending_rest(name)
        self.rest('pairings', 'ClaimPairing', expected_status=401, request_id=pending['request_id'],
                  mail_secret=mail, cleanup_secret=cleanup)
        self.check('Unapproved real HTTPS pairing cannot issue a registration', True)
        self.command(self.env, 'serve', 'pair', 'approve', pending['request_id'], '--confirm')
        claimed = self.rest('pairings', 'ClaimPairing', request_id=pending['request_id'],
                            mail_secret=mail, cleanup_secret=cleanup)['client']
        authority = self.remote('Status')['authority']
        selected = {key: authority[key] for key in ('host_id', 'workspace_id', 'recovery_generation')}
        selected.update(origin=self.origin, client_id=claimed['client_id'], capabilities=claimed['capabilities'],
                        mail_secret=mail, cleanup_secret=cleanup)
        self.write_state(name, {'generation': 1, 'selected': selected, 'pending_pairing': None,
                                'cleanup': [], 'retired_pairings': []})
        self.state(name)
        self.read_mail(name)
        self.check('Host-approved REST registration serves the seeded native WSS client', True)
        return selected['client_id']

    def pairing_recovery(self, name, selected_id):
        pending, mail, cleanup = self.pending_rest(name)
        state = self.state(name)
        state['pending_pairing'] = {'origin': self.origin, 'name': pending['name'],
                                    'capabilities': pending['capabilities'], 'mail_secret': mail,
                                    'cleanup_secret': cleanup, 'request_id': pending['request_id']}
        self.write_state(name, state)
        with sqlite3.connect(self.root / 'data/mxr.db', timeout=10) as db:
            changed = db.execute('UPDATE remote_pairings SET expires_at=? WHERE request_id=?',
                                 (int(time.time()) - 1, pending['request_id'])).rowcount
            self.check('Only the owned pending fixture challenge was expired', changed == 1)
        self.rest('pairings', 'ClaimPairing', expected_status=401, request_id=pending['request_id'],
                  mail_secret=mail, cleanup_secret=cleanup)
        self.check('Real HTTPS authority rejects an expired pairing proof', True)
        preview = self.command(self.clients[name], 'connect', '--cancel-pairing', '--dry-run')
        self.check('Native cancel preview preserves the seeded pending challenge and selection',
                   preview['selection_preserved'] and self.state(name)['pending_pairing']['request_id'] == pending['request_id']
                   and self.state(name)['selected']['client_id'] == selected_id)
        self.command(self.clients[name], 'connect', '--cancel-pairing', '--confirm')
        state = self.state(name)
        self.check('Native local cancel retires only pending pairing despite private-CA cleanup failure',
                   state.get('pending_pairing') is None and state['selected']['client_id'] == selected_id)
        self.rest('self/deregister', 'AbandonPairing', cleanup_secret=cleanup)
        self.command(self.clients[name], 'connect', '--forget-cleanup', '--dry-run')
        self.command(self.clients[name], 'connect', '--forget-cleanup', '--confirm')
        fresh, fresh_mail, fresh_cleanup = self.pending_rest(name)
        self.check('Real HTTPS authority issues a fresh pairing request identity', fresh['request_id'] != pending['request_id'])
        self.rest('self/deregister', 'AbandonPairing', cleanup_secret=fresh_cleanup)
        records = self.remote('Clients')['clients']
        self.check('Recovery never revokes the selected registration',
                   next(client for client in records if client['client_id'] == selected_id)['revoked_at'] is None
                   and self.state(name)['selected']['client_id'] == selected_id)
        self.check('Cancelled fixture challenges leave no host pending pairing', not self.remote('Pairings')['pairings'])
        self.read_mail(name)

    def native_outcome_recovery(self):
        before = len(self.spool_records())
        preview = self.reply_draft('a', 'Synthetic lost-response recovery reply.')
        self.spool_control('send-pause-after-accept', True)
        try:
            self.command({**self.clients['a'], 'MXR_IPC_TIMEOUT_SECS': '1'},
                         *self.send_arguments(preview), ok=False)
            self.check('Lost native response still has exactly one provider acceptance',
                       len(self.spool_records()) == before + 1)
        finally:
            self.spool_control('send-pause-after-accept', False)
        accepted = self.wait_outcome('a', preview, 'accepted')
        self.restart_host()
        recovered = self.outcome('a', preview)
        self.check('Host restart recovers the original accepted operation identity',
                   recovered['identity'] == accepted['identity'] and recovered['state'] == 'accepted')
        replay = self.command(self.clients['a'], *self.send_arguments(preview))
        self.check('Accepted operation replay does not deliver twice',
                   replay['state'] == 'accepted' and len(self.spool_records()) == before + 1)
        uncertain = self.reply_draft('a', 'Synthetic uncertain outcome recovery reply.')
        self.spool_control('send-uncertain', True)
        try:
            self.command(self.clients['a'], *self.send_arguments(uncertain), ok=False)
        finally:
            self.spool_control('send-uncertain', False)
        unknown = self.outcome('a', uncertain)
        count = len(self.spool_records())
        self.check('Uncertain provider effect remains explicitly unknown', unknown['state'] == 'unknown')
        self.restart_host()
        recovered_unknown = self.outcome('a', uncertain)
        self.check('Unknown outcome and original operation identity survive restart',
                   recovered_unknown['state'] == 'unknown' and recovered_unknown['identity'] == unknown['identity'])
        self.command(self.clients['a'], *self.send_arguments(uncertain), ok=False)
        self.command(self.clients['a'], *self.send_arguments(uncertain, str(uuid.uuid4())), ok=False)
        self.check('Unknown operation replay or replacement never duplicates delivery',
                   len(self.spool_records()) == count)
        self.native_acceptances = count

    def browser_code(self, name):
        selected = self.state(name)['selected']
        grant = self.rest('browser-grants', 'IssueBrowserCode', client_id=selected['client_id'],
                          mail_secret=selected['mail_secret'])
        code = grant['code']
        self.tokens.append(code)
        self.check('Real HTTPS browser grant issues a bounded single-use code',
                   len(code) == 64 and all(char in '0123456789abcdef' for char in code)
                   and 0 < grant['expires_at'] - int(time.time()) <= 120)
        return code

    def run_browser(self):
        ids, codes = {}, {}
        for name in self.clients:
            ids[name] = self.pair_all(name)
        if not self.browser_only:
            self.pairing_recovery('a', ids['a'])
            self.native_outcome_recovery()
        self.evidence['native_recovery_repeated'] = not self.browser_only
        for name in self.clients:
            codes[name] = self.browser_code(name)
        self.browser_log = (self.root / 'browser.log').open('ab')
        self.browser = subprocess.Popen(['node', str(self.helper), 'browser'], cwd=self.repo,
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.browser_log, text=True, bufsize=1)
        self.browser.stdin.write(json.dumps({'origin': self.origin, 'codes': codes,
                                            'repo': str(self.repo), 'message_id': self.message_id,
                                            'browser_channel': self.browser_channel,
                                            'diagnostic_same_origin_referrer': self.diagnostic_referrer,
                                            'diagnostic_owner_conflict': self.diagnostic_owner}) + '\n')
        self.browser.stdin.flush()
        deadline = time.monotonic() + 300
        while time.monotonic() < deadline:
            event = self.read_control(self.browser, min(120, deadline - time.monotonic()), 'Browser journey')
            if event.get('event') == 'check':
                self.check(event['name'], True)
                continue
            action = event.get('action')
            reply = {'continue': True}
            if action == 'relaunch':
                reply['browser_code'] = self.browser_code('a')
            elif action == 'draft_saved':
                counts = self.host_counts()
                self.check('Browser draft and uploaded attachment persist on host',
                           counts['drafts'] > 0 and counts['resources'] > 0
                           and len(self.spool_records()) == self.native_acceptances)
            elif action == 'sent':
                records = self.spool_records()
                self.check('Browser sent exactly once through fake provider', len(records) == self.native_acceptances + 1)
                attachments = records[-1]['attachments']
                self.check('Provider received exact browser-uploaded bytes',
                           len(attachments) == 1 and attachments[0]['sha256'] == hashlib.sha256(b'remote-browser-attachment\n').hexdigest())
            elif action == 'restart':
                self.restart_host()
                self.check('Host restart preserves accepted provider outcomes', len(self.spool_records()) == self.native_acceptances + 1)
            elif action == 'revoke':
                self.command(self.env, 'serve', 'clients', 'revoke', ids['a'], '--confirm')
            elif event.get('event') == 'complete':
                # EOF lets the child's readline iterator finish before we wait for its exit.
                self.browser.stdin.close()
                self.browser.wait(timeout=15)
                self.check('Browser driver closes its owned browser cleanly', self.browser.returncode == 0)
                self.evidence.update(result='passed', provider_acceptances=len(self.spool_records()))
                return
            else:
                raise AssertionError('Unexpected browser control event')
            self.browser.stdin.write(json.dumps(reply) + '\n')
            self.browser.stdin.flush()
        raise AssertionError('Overall browser journey deadline')

    def close_owned(self):
        for process in (self.browser, self.proxy):
            if process and process.poll() is None:
                if process is self.browser:
                    process.stdin.close()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        pass
                if process.poll() is not None:
                    continue
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        for log in (self.proxy_log, self.browser_log):
            if log:
                log.close()
        self.gateway_stop()
        self.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--runtime', required=True, type=Path)
    parser.add_argument('--source', required=True)
    parser.add_argument('--diagnostic-owner-conflict', action='store_true', help='Diagnostic only: stop after one protected status request and retain sanitized owner conflict evidence')
    parser.add_argument('--diagnostic-same-origin-referrer', action='store_true', help='Diagnostic only: override login document referrer policy before submitting; never production acceptance')
    parser.add_argument('--browser-only', action='store_true', help='Skip previously verified native pairing/outcome recovery while diagnosing the browser journey')
    parser.add_argument('--browser-channel', choices=('chrome', 'chromium'), default='chrome',
                        help='Installed Chrome by default; chromium uses Playwright bundled browser')
    args = parser.parse_args()
    fixture = RemoteBrowserFixture(args)
    started = time.monotonic()
    try:
        fixture.certificate()
        fixture.proxy_start()
        fixture.prepare()
        fixture.run_browser()
    except Exception:
        fixture.evidence['result'] = 'failed'
        raise
    finally:
        fixture.close_owned()
        fixture.evidence['elapsed_seconds'] = round(time.monotonic() - started, 3)
        (fixture.root / 'result.json').write_text(json.dumps(fixture.evidence, indent=2) + '\n')
    print(json.dumps(fixture.evidence), flush=True)


if __name__ == '__main__':
    main()
