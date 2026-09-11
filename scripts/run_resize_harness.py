"""run_resize_harness.py - deterministically verify the frontend survives a window
resize, without WebView2.

Loads the REAL built bundle (verify_resize/index.html provides a stubbed Tauri layer)
in headless Chrome, mounts it, and fires `tauri://resize` events. The page writes a
JSON result into <pre id="result">; we read it from --dump-dom output.

Usage:
    python scripts/run_resize_harness.py [--bundle index-CjsjWIF4.js] [--chrome PATH]
Exit: 0 = no frontend error / no throw, 1 = error detected or could not start.
"""

import argparse
import json
import re
import subprocess
import sys
import threading
import time
import http.server
import socketserver
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
HTML = "verify_resize/index.html"

DEFAULT_CHROME = r"C:\Program Files\Google\Chrome\Application\chrome.exe"


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a):
        pass


def serve(root):
    httpd = socketserver.TCPServer(("127.0.0.1", 0), Handler)
    httpd.RequestHandlerClass = type("H", (Handler,), {"directory": root})
    port = httpd.server_address[1]
    t = threading.Thread(target=httpd.serve_forever, daemon=True)
    t.start()
    return httpd, port


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bundle", default="index-CjsjWIF4.js")
    ap.add_argument("--chrome", default=DEFAULT_CHROME)
    args = ap.parse_args()

    httpd, port = serve(ROOT)
    url = "http://127.0.0.1:%d/%s?bundle=%s" % (port, HTML, args.bundle)

    out = []
    say = lambda s: (out.append(s), print(s, flush=True))

    if not os.path.exists(args.chrome):
        say("CHROME NOT FOUND: " + args.chrome)
        httpd.shutdown()
        return 1

    say("bundle   : " + args.bundle)
    say("url      : " + url)

    ud = os.path.join(ROOT, "verify_resize", "chrome-profile")
    os.makedirs(ud, exist_ok=True)
    proc = subprocess.run(
        [
            args.chrome,
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--user-data-dir=" + ud,
            "--virtual-time-budget=9000",
            "--dump-dom",
            url,
        ],
        capture_output=True,
        text=True,
        timeout=120,
    )
    html = proc.stdout

    m = re.search(r'<pre id="result">(.*?)</pre>', html, re.S)
    if not m:
        say("NO RESULT ELEMENT IN DUMP")
        if out:
            say("captured tail of DOM:")
            say(html[-1500:])
        httpd.shutdown()
        return 1

    raw = m.group(1)
    try:
        res = json.loads(raw)
    except Exception:
        say("RESULT NOT JSON: " + raw[:500])
        httpd.shutdown()
        return 1

    say("==== RESULT JSON ====")
    say(json.dumps(res, ensure_ascii=False, indent=1))

    errors = res.get("errors") or []
    reported = res.get("reported") or []
    fatal = res.get("fatal")
    listeners = res.get("listeners_resize", 0)

    if fatal:
        say("FAIL: bundle import failed: " + fatal)
        httpd.shutdown()
        return 1
    if listeners == 0:
        say("FAIL: onResized listener was never registered (mount/import issue)")
        httpd.shutdown()
        return 1
    if errors or reported:
        say("FAIL: %d thrown error(s), %d report_frontend_error line(s)" % (len(errors), len(reported)))
        httpd.shutdown()
        return 1

    say("PASS: onResized fired %d time(s), no throw, savedSize=%s" % (listeners, res.get("savedSize")))
    httpd.shutdown()
    return 0


if __name__ == "__main__":
    sys.exit(main())
