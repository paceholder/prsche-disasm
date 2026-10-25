"""Serve only the built viewer site on loopback, with read-only access to local/game."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re


class Handler(SimpleHTTPRequestHandler):
    extensions_map = {
        **SimpleHTTPRequestHandler.extensions_map,
        '.wasm': 'application/wasm',
        '.js': 'text/javascript',
        '.crp': 'application/octet-stream',
        '.fsh': 'application/octet-stream',
        '.tpg': 'application/octet-stream',
        '.edg': 'application/octet-stream',
        '.jnc': 'application/octet-stream',
        '.map': 'text/plain',
        '.scn': 'application/octet-stream',
    }

    game_dir = None
    ui_assets_dir = None
    game_files_cache = None

    def end_headers(self):
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()

    def do_GET(self):
        if self.path == '/healthz':
            data = b'{"status":"ok"}'
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        if self.path == '/api/game-files':
            if not self.game_dir or not self.game_dir.is_dir():
                self.send_error(404, 'local/game directory not found')
                return
            if self.__class__.game_files_cache is None:
                self.__class__.game_files_cache = [
                    p.relative_to(self.game_dir).as_posix()
                    for p in self.game_dir.rglob('*')
                    if p.is_file()
                ]
            data = json.dumps(self.__class__.game_files_cache).encode('utf-8')
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        if self.headers.get('Range') and self._send_range():
            return
        super().do_GET()

    def _send_range(self):
        """Single `bytes=a-b` range of a file (music tracks inside the 115 MB zzzymus.viv)."""
        m = re.fullmatch(r'bytes=(\d*)-(\d*)', self.headers['Range'].strip())
        path = self.translate_path(self.path)
        if not m or not path or not os.path.isfile(path):
            return False
        size = os.path.getsize(path)
        if m.group(1):
            start = int(m.group(1))
            end = min(int(m.group(2)), size - 1) if m.group(2) else size - 1
        elif m.group(2):
            start, end = max(0, size - int(m.group(2))), size - 1
        else:
            return False
        if start > end or start >= size:
            self.send_response(416)
            self.send_header('Content-Range', f'bytes */{size}')
            self.end_headers()
            return True
        with open(path, 'rb') as f:
            f.seek(start)
            data = f.read(end - start + 1)
        self.send_response(206)
        self.send_header('Content-Type', self.guess_type(path))
        self.send_header('Content-Range', f'bytes {start}-{end}/{size}')
        self.send_header('Content-Length', str(len(data)))
        self.send_header('Accept-Ranges', 'bytes')
        self.end_headers()
        self.wfile.write(data)
        return True

    def translate_path(self, path):
        clean_path = path.split('?', 1)[0].split('#', 1)[0]
        if self.ui_assets_dir and clean_path.startswith('/assets/'):
            target = (self.ui_assets_dir / clean_path[len('/assets/'):]).resolve()
            if self.ui_assets_dir in target.parents and target.is_file():
                return str(target)
            return ''
        if self.game_dir and (clean_path.startswith('/game/') or clean_path.startswith('/local/game/')):
            prefix = '/game/' if clean_path.startswith('/game/') else '/local/game/'
            rel_path = clean_path[len(prefix):]
            target = (self.game_dir / rel_path).resolve()
            if self.game_dir in target.parents or target == self.game_dir:
                return str(target)
            return ''
        return super().translate_path(path)

    def list_directory(self, path):
        self.send_error(403, 'Directory listing is disabled')
        return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--iteration', default='012-campaign-fidelity')
    parser.add_argument('--port', type=int, default=8000)
    parser.add_argument('--game-dir', default='local/game')
    parser.add_argument('--ready-file', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    base = (root/'local/builds').resolve()
    site = (base/args.iteration/'web').resolve()
    game_path = (root / args.game_dir).resolve()

    if base not in site.parents or not (site/'index.html').is_file():
        raise SystemExit('Built site not found. Run scripts/build.ps1 first.')

    Handler.game_dir = game_path if game_path.is_dir() else None
    # Derived original UI images are a local read-only cache, not snapshot sources.
    ui_cache = (root / 'local/derived/fe-ui').resolve()
    Handler.ui_assets_dir = ui_cache if args.iteration == '012-campaign-fidelity' and ui_cache.is_dir() else None
    if Handler.game_dir:
        print(f'Game resources: {Handler.game_dir} (read-only)', flush=True)

    server = ThreadingHTTPServer(('127.0.0.1', args.port), partial(Handler, directory=str(site)))
    port = server.server_address[1]
    if args.ready_file:
        args.ready_file.parent.mkdir(parents=True, exist_ok=True)
        ready_tmp = args.ready_file.with_name(args.ready_file.name + '.tmp')
        ready_tmp.write_text(json.dumps({'port': port, 'pid': __import__('os').getpid()}), encoding='utf-8')
        ready_tmp.replace(args.ready_file)
    print(f'Viewer: http://127.0.0.1:{port}/ (Ctrl+C to stop)', flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == '__main__':
    main()
