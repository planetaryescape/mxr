#!/usr/bin/env python3
"""Exercise an installed binary through an existing, trusted private HTTPS origin.

The caller owns TLS forwarding to --listen. This script never changes Tailscale,
system trust, or the user's profiles. Only synthetic mail and owned processes run.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import socket
import struct
import subprocess
import termios
import time
import urllib.error
import urllib.request

from verify_pairing import Fixture


class RemoteReadFixture(Fixture):
    def __init__(self, args):
        super().__init__(args.binary, args.runtime, args.source)
        self.root.chmod(0o700)
        self.origin = args.origin.rstrip('/')
        self.listen = args.listen
        self.gateway = None
        self.gateway_log = None
        self.outputs = []
        self.clients = {}
        config = self.root / 'config/config.toml'
        config.write_text(config.read_text() + '''
[general]
default_account = "fake"
sync_interval = 3600
[accounts.fake]
name = "Synthetic remote host"
email = "remote-fixture@example.com"
[accounts.fake.sync]
type = "fake"
[accounts.fake.send]
type = "fake"
''')
        self.env['MXR_FAKE_DATASET'] = 'small'
        for name in ('a', 'b'):
            root = self.root / ('client-' + name)
            root.mkdir(mode=0o700)
            for part in ('home', 'config', 'data', 'tmp'):
                (root / part).mkdir(mode=0o700)
            env = self.env.copy()
            env.update(HOME=str(root / 'home'), TMPDIR=str(root / 'tmp') + '/',
                       MXR_CONFIG_DIR=str(root / 'config'), MXR_DATA_DIR=str(root / 'data'),
                       MXR_SOCKET_PATH=self.sock + '-' + name, MXR_INSTANCE='remote-read-' + name)
            env.pop('MXR_FAKE_DATASET', None)
            (root / 'config/config.toml').write_text('[bridge]\nenabled = false\n')
            self.clients[name] = env

    def command(self, env, *args, ok=True, structured=True):
        command = [str(self.binary), *args]
        if structured:
            command += ['--format', 'json']
        result = subprocess.run(command, env=env, stdin=subprocess.DEVNULL,
                                capture_output=True, text=True, timeout=45)
        self.outputs.append(result.stdout + result.stderr)
        if ok and result.returncode:
            # Retain diagnostics only in the private fixture, never in the receipt.
            (self.root / 'failed-command.log').write_text(result.stdout + result.stderr)
            raise AssertionError('CLI failed: ' + ' '.join(args))
        if not ok:
            self.check('Rejected command: ' + ' '.join(args), result.returncode != 0)
            return result
        return json.loads(result.stdout) if structured else result.stdout

    def state(self, name):
        path = Path(self.clients[name]['MXR_CONFIG_DIR']) / 'client-connection.json'
        if not path.exists():
            return None
        self.check('Client state is mode 0600', path.stat().st_mode & 0o777 == 0o600)
        state = json.loads(path.read_text())
        for entry in [state.get('selected'), state.get('pending_pairing'),
                      *state.get('cleanup', []), *state.get('retired_pairings', [])]:
            if entry:
                for field in ('mail_secret', 'cleanup_secret'):
                    if entry.get(field):
                        self.tokens.append(entry[field])
        return state

    def gateway_start(self):
        self.gateway_log = (self.root / 'gateway.log').open('ab')
        self.gateway = subprocess.Popen(
            [str(self.binary), 'serve', '--foreground', '--listen', self.listen],
            env=self.env, stdout=self.gateway_log, stderr=self.gateway_log,
            start_new_session=True)
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if self.gateway.poll() is not None:
                raise AssertionError('Owned gateway exited during startup')
            try:
                urllib.request.urlopen('http://' + self.listen + '/api/v1/remote/pairings', timeout=1)
            except urllib.error.HTTPError as error:
                if error.code == 405:
                    return
            except OSError:
                pass
            time.sleep(.1)
        raise AssertionError('Owned gateway readiness deadline')

    def gateway_stop(self):
        if self.gateway and self.gateway.poll() is None:
            self.gateway.terminate()
            self.gateway.wait(timeout=15)
        if self.gateway_log:
            self.gateway_log.close()
        self.gateway = None

    def prepare(self):
        # Refuse to reuse another process's listen port.
        host, port = self.listen.rsplit(':', 1)
        with socket.socket() as probe:
            probe.bind((host, int(port)))
        self.start()
        self.command(self.env, 'sync', '--wait', '--wait-timeout-secs', '30', structured=False)
        results = self.command(self.env, 'search', 'subject:"Q1 Report review"', '--mode', 'lexical')
        self.mail = results if isinstance(results, list) else results['results']
        self.check('Host has meaningful synthetic search results', len(self.mail) == 3)
        self.message_id = next(row['message_id'] for row in self.mail
                               if row['subject'] == 'Q1 Report review')
        self.host_body = self.command(self.env, 'cat', self.message_id)
        self.check('Host body is synthetic fixture mail', 'Q1 report body text' in self.host_body['text_plain'])
        self.check('Host fixture includes an attachment', bool(self.host_body['attachments']))
        self.command(self.env, 'attachments', 'download', self.message_id,
                     '--dir', str(self.root / 'downloads'))
        self.host_body = self.command(self.env, 'cat', self.message_id)
        self.check('Host fixture has a materialized attachment path',
                   any(a.get('local_path') for a in self.host_body['attachments']))
        self.gateway_start()
        for name, env in self.clients.items():
            for extra in ([], ['--dry-run']):
                result = self.command(env, 'connect', self.origin, *extra)
                self.check('Connect preview does not select', result['selection_applied'] is False)
                self.check('Connect preview writes no state', self.state(name) is None)
            self.command(env, 'connect', self.origin, '--dry-run', '--confirm', ok=False)
        self.check('Previews create no host pairings', not self.remote('Pairings')['pairings'])
        ready = {'status': 'fixture_ready', 'source': self.evidence['source'],
                 'binary_sha256': self.evidence['binary_sha256'], 'origin': self.origin,
                 'listen': self.listen, 'runtime': str(self.root),
                 'daemon_pid': self.process.pid, 'gateway_pid': self.gateway.pid}
        (self.root / 'ready.json').write_text(json.dumps(ready, indent=2) + '\n')
        print(json.dumps(ready), flush=True)

    def pair(self, name):
        env = self.clients[name]
        arguments = ('connect', self.origin, '--name', 'Synthetic client ' + name, '--confirm')
        initial = self.command(env, *arguments)
        self.state(name)
        pairing = initial['pairing']
        self.check('Unapproved pairing does not select', initial['selection_applied'] is False)
        self.command(env, *arguments, ok=False)
        listed = self.command(self.env, 'serve', 'pair', 'list')['response']['pairings']
        host_pairing = next(p for p in listed if p['request_id'] == pairing['request_id'])
        self.check('Client and host show the same full challenge', host_pairing['challenge'] == pairing['challenge'])
        self.command(self.env, 'serve', 'pair', 'approve', pairing['request_id'], '--dry-run')
        self.command(env, *arguments, ok=False)
        self.command(self.env, 'serve', 'pair', 'approve', pairing['request_id'], '--confirm')
        claimed = self.command(env, *arguments)
        self.check('Approved client becomes selected', claimed['selection_applied'] is True)
        saved = self.state(name)
        self.check('Selected identity matches claim', saved['selected']['client_id'] == claimed['client_id'])
        again = self.command(env, *arguments)
        self.check('Repeated connect reuses the registration', again['client_id'] == claimed['client_id'] and again['reused'])
        return claimed['client_id']

    def read_mail(self, name):
        env = self.clients[name]
        status = self.command(env, 'connect', '--status')
        self.check('New process uses saved selection', status['selection_source'] == 'saved' and status['effective_target'] == self.origin)
        result = self.command(env, 'search', 'subject:"Q1 Report review"', '--mode', 'lexical')
        rows = result if isinstance(result, list) else result['results']
        self.check('Remote search returns the hosted identities',
                   {r['message_id'] for r in rows} == {r['message_id'] for r in self.mail})
        body = self.command(env, 'cat', self.message_id)
        self.check('Remote reader returns the hosted body', body['text_plain'] == self.host_body['text_plain'])
        self.check('Remote metadata has no host attachment paths',
                   all(a.get('local_path') is None for a in body.get('attachments', [])))
        self.check('Remote response has no fixture filesystem paths', str(self.root) not in json.dumps(body))
        self.check('Remote selection never autostarts a local daemon', not Path(env['MXR_SOCKET_PATH']).exists())

    def mcp_read(self, name):
        process = subprocess.Popen([str(self.binary), 'mcp', 'serve'], env=self.clients[name],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.DEVNULL, bufsize=0)
        def call(number, method, params):
            process.stdin.write((json.dumps({'jsonrpc': '2.0', 'id': number,
                                            'method': method, 'params': params}) + '\n').encode())
            process.stdin.flush()
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                ready, _, _ = select.select([process.stdout], [], [], max(.01, deadline-time.monotonic()))
                if not ready:
                    break
                line = process.stdout.readline()
                if not line:
                    break
                response = json.loads(line)
                if response.get('id') == number:
                    return response
            raise AssertionError('MCP response deadline')
        try:
            initialized = call(1, 'initialize', {'protocolVersion': '2024-11-05', 'capabilities': {},
                                               'clientInfo': {'name': 'remote-read-fixture', 'version': '1'}})
            self.check('MCP initializes using saved remote selection', 'result' in initialized)
            process.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
            process.stdin.flush()
            response = call(2, 'tools/call', {'name': 'mxr_read_message',
                                            'arguments': {'message_id': self.message_id, 'include_body': True}})
            self.check('MCP reads hosted mail', not response.get('error') and not response.get('result', {}).get('isError'))
            self.check('MCP returns the synthetic body', 'Q1 report body text' in json.dumps(response))
        finally:
            process.stdin.close()
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=10)

    def tui_read(self, name):
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 42, 140, 0, 0))
        env = {**self.clients[name], 'TERM': 'xterm-256color'}
        process = subprocess.Popen([str(self.binary)], env=env, stdin=slave,
                                   stdout=slave, stderr=slave, start_new_session=True)
        os.close(slave)
        transcript = bytearray()
        def rendered(text, timeout=20):
            start = len(transcript)
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                ready, _, _ = select.select([master], [], [], .1)
                if ready:
                    try:
                        data = os.read(master, 65536)
                    except OSError:
                        break
                    if not data:
                        break
                    transcript.extend(data)
                    plain = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '',
                                   transcript[start:].decode(errors='replace'))
                    if text in plain:
                        return True
                if process.poll() is not None:
                    break
            return False
        try:
            self.check('TUI opens the remote mailbox', rendered('Remote mailbox'))
            os.write(master, b'/subject:"Q1 Report review"\r')
            self.check('TUI searches hosted mail', rendered('Q1 Report review'))
            os.write(master, b'\r')
            self.check('TUI opens the hosted message body', rendered('Q1 report body text'))
        finally:
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=10)
            os.close(master)
            (self.root / ('tui-' + name + '.log')).write_bytes(transcript)

    def run_remote(self):
        ids = {name: self.pair(name) for name in self.clients}
        self.check('Two independent clients register separately', ids['a'] != ids['b'])
        self.check('Host records exactly two clients', len(self.remote('Clients')['clients']) == 2)
        for name in self.clients:
            self.read_mail(name)
        self.mcp_read('a')
        self.tui_read('a')
        result = self.command(self.clients['a'], 'connect', '--local', '--dry-run')
        self.check('Local preview preserves remote selection', not result['selection_applied'] and self.state('a')['selected'] is not None)
        self.command(self.clients['a'], 'connect', '--local', '--confirm')
        a = next(c for c in self.remote('Clients')['clients'] if c['client_id'] == ids['a'])
        self.check('Online switch retires its own registration', a['revoked_at'] is not None)
        self.read_mail('b')
        ids['a'] = self.pair('a')
        self.gateway_stop()
        self.command(self.clients['b'], 'search', 'Q1', ok=False)
        self.check('Unavailable remote never starts a local daemon', not Path(self.clients['b']['MXR_SOCKET_PATH']).exists())
        result = self.command(self.clients['a'], 'connect', '--local', '--confirm')
        self.check('Offline switch commits locally with pending cleanup', result['effective_target'] == 'local' and result['deregistration'] == 'pending')
        state = self.state('a')
        self.check('Offline cleanup retains no mail credential', state['selected'] is None and all('mail_secret' not in c for c in state['cleanup']))
        self.gateway_start()
        status = self.command(self.clients['a'], 'connect', '--status')
        self.check('Cleanup resumes after reconnect', status['pending_deregistration_count'] == 0)
        a = next(c for c in self.remote('Clients')['clients'] if c['client_id'] == ids['a'])
        self.check('Offline cleanup retires exactly its own registration', a['revoked_at'] is not None)
        self.read_mail('b')
        self.mcp_read('b')
        logs = ''.join(self.outputs) + (self.root / 'daemon.log').read_text() + (self.root / 'gateway.log').read_text()
        self.check('CLI output and server logs redact client credentials', all(token not in logs for token in self.tokens))
        self.evidence.update(result='passed', origin=self.origin, client_ids=ids)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--runtime', required=True, type=Path)
    parser.add_argument('--source', required=True)
    parser.add_argument('--origin', required=True)
    parser.add_argument('--listen', default='127.0.0.1:18741')
    parser.add_argument('--continue-file', type=Path,
                        help='Prepare locally, then wait for this file before contacting HTTPS')
    parser.add_argument('--prepare-only', action='store_true')
    args = parser.parse_args()
    fixture = RemoteReadFixture(args)
    started = time.monotonic()
    try:
        fixture.prepare()
        if args.prepare_only:
            fixture.evidence['result'] = 'prepared_only'
        else:
            if args.continue_file:
                deadline = time.monotonic() + 3600
                while not args.continue_file.exists():
                    if time.monotonic() > deadline:
                        raise TimeoutError('Awaiting caller-managed HTTPS forwarding')
                    time.sleep(.25)
            fixture.run_remote()
    finally:
        fixture.gateway_stop()
        fixture.stop()
        fixture.evidence['elapsed_seconds'] = round(time.monotonic()-started, 3)
        (fixture.root / 'result.json').write_text(json.dumps(fixture.evidence, indent=2) + '\n')
    print(json.dumps(fixture.evidence), flush=True)


if __name__ == '__main__':
    main()
