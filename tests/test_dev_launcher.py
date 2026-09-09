"""Exercise the launcher and its child tree without a display or NVIDIA GPU."""
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest

LAUNCHER = Path(__file__).resolve().parents[1] / 'crates/gpu-monitor-gui/dev.sh'


class DevLauncherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='gpu-launcher-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.gui = self.root / 'gui with spaces'
        (self.gui / 'src-web').mkdir(parents=True)
        shutil.copy2(LAUNCHER, self.gui / 'dev.sh')
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.log = self.root / 'calls.jsonl'
        self.env = dict(os.environ, PATH=f'{self.bin}:{os.environ["PATH"]}', REVIEW_LOG=str(self.log))
        body = '''import json, os, signal, subprocess, sys, time
with open(os.environ['REVIEW_LOG'], 'a') as f:
    f.write(json.dumps({'command': os.path.basename(sys.argv[0]), 'args': sys.argv[1:], 'cwd': os.getcwd(), 'pid': os.getpid()})+'\\n')
if os.path.basename(sys.argv[0]) == 'npm' and not os.environ.get('REVIEW_NPM_WAIT'):
    sys.exit(int(os.environ.get('REVIEW_NPM_EXIT', '0')))
if os.environ.get('REVIEW_WAIT') or os.environ.get('REVIEW_NPM_WAIT'):
    # A frontend subprocess left alive by Tauri on TERM (and even ignores TERM)
    # must still be cleaned up by the launcher's process-group fallback.
    frontend = subprocess.Popen([sys.executable, '-c', "import os,signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); open(os.environ['REVIEW_READY'],'w').write(str(os.getpid())); time.sleep(60)"])
    while True: time.sleep(0.1)
sys.exit(int(os.environ.get('REVIEW_CARGO_EXIT', '0')))
'''
        for name in ('npm', 'cargo'):
            target = self.bin / name
            target.write_text(f'#!{sys.executable}\n' + body)
            target.chmod(0o755)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def run_launcher(self, *args, **env):
        return subprocess.run([str(self.gui / 'dev.sh'), *args], cwd=self.root,
                              env=dict(self.env, **env), capture_output=True, text=True, timeout=5)

    def test_external_working_directory_arguments_and_exit_status(self):
        result = self.run_launcher('--no-watch', REVIEW_CARGO_EXIT='17')
        self.assertEqual(result.returncode, 17, result.stderr)
        npm, cargo = self.calls()
        self.assertEqual(npm['args'], ['--prefix', 'src-web', 'ci'])
        self.assertEqual(cargo['cwd'], str(self.gui))
        self.assertEqual(cargo['args'], ['tauri', 'dev', '--no-watch'])

    def test_dependency_failure_stops_before_tauri(self):
        result = self.run_launcher(REVIEW_NPM_EXIT='23')
        self.assertEqual(result.returncode, 23)
        self.assertEqual([call['command'] for call in self.calls()], ['npm'])

    def test_signals_stop_owned_child_tree_and_preserve_unrelated_listener(self):
        (self.gui / 'src-web/node_modules').mkdir()
        self.assert_signal_cleanup(installing=False)

    def test_interrupted_installation_stops_its_child_tree_before_tauri(self):
        self.assert_signal_cleanup(installing=True)
        self.assertTrue(all(call['command'] == 'npm' for call in self.calls()))

    def assert_signal_cleanup(self, installing):
        # This listener belongs to the test, outside the launcher's process group.
        with socket.socket() as unrelated:
            unrelated.bind(('127.0.0.1', 0))
            unrelated.listen()
            for sig, expected in ((signal.SIGINT, 130), (signal.SIGTERM, 143)):
                with self.subTest(signal=sig):
                    ready = self.root / f'ready-{sig}'
                    child = subprocess.Popen([str(self.gui / 'dev.sh')], cwd=self.root,
                                             env=dict(self.env, **({'REVIEW_NPM_WAIT': '1'} if installing else {'REVIEW_WAIT': '1'}), REVIEW_READY=str(ready)))
                    frontend_pid = None
                    group = None
                    try:
                        deadline = time.monotonic() + 5
                        while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                            time.sleep(0.02)
                        self.assertTrue(ready.exists(), 'mock frontend did not start')
                        frontend_pid = int(ready.read_text())
                        group = os.getpgid(frontend_pid)
                        self.assertNotEqual(group, os.getpgrp())
                        child.send_signal(sig)
                        self.assertEqual(child.wait(timeout=4), expected)
                        deadline = time.monotonic() + 2
                        while self.running(frontend_pid) and time.monotonic() < deadline:
                            time.sleep(0.02)
                        self.assertFalse(self.running(frontend_pid), 'frontend survived launcher exit')
                        with socket.create_connection(unrelated.getsockname(), timeout=1):
                            connection, _ = unrelated.accept()
                            connection.close()
                    finally:
                        if group:
                            try: os.killpg(group, signal.SIGKILL)
                            except ProcessLookupError: pass
                        if child.poll() is None:
                            child.kill()
                        child.wait()

    @staticmethod
    def running(pid):
        try:
            # Orphans may briefly be zombies until the container's init reaps them.
            state = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0]
            return state != 'Z'
        except FileNotFoundError:
            return False


if __name__ == '__main__':
    unittest.main()
