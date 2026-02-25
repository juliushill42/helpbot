import os, socket, yaml, sys
from mesh.dispatcher import dispatch
from governance.authority import Authority
from runtime.model import MiniMax80B

cfg = yaml.safe_load(open("config.yaml"))

if sys.platform == "win32":
    # Use TCP sockets on Windows
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.bind(("127.0.0.1", 50000))
else:
    # Use Unix sockets on other systems
    sock = cfg["node"]["socket"]
    if os.path.exists(sock):
        os.remove(sock)
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(sock)

server.listen(1)

authority = Authority(cfg["governance"])
model = MiniMax80B(cfg["model"])

while True:
    conn, _ = server.accept()
    task = conn.recv(65536).decode()
    decision = authority.authorize(task)
    if not decision["allowed"]:
        conn.sendall(b"DENIED")
        conn.close()
        continue
    result = dispatch(task, model, authority)
    conn.sendall(result.encode())
    conn.close()
