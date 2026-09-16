import http.server, json, os, sys
OUT = os.environ["CAP_OUT"]
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(n)
        open(OUT, "wb").write(body)
        msg = json.dumps({"error": {"message": "Please reduce the length of the messages or completion.", "type": "invalid_request_error"}}).encode()
        self.send_response(400)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(msg)))
        self.end_headers()
        self.wfile.write(msg)
    def log_message(self, *a): pass
http.server.HTTPServer(("127.0.0.1", 8799), H).serve_forever()
