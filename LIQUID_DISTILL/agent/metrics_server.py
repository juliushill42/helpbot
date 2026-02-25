import asyncio
import json
from pathlib import Path

try:
    from aiohttp import web
except ImportError:
    raise SystemExit("aiohttp not installed; pip install aiohttp")


ROOT = Path(__file__).resolve().parent.parent
STATE_PATH = ROOT / "agent" / "state.json"


async def metrics(request):
    if not STATE_PATH.exists():
        return web.json_response({"error": "state.json not found"}, status=404)
    try:
        data = json.loads(STATE_PATH.read_text())
        return web.json_response(data)
    except Exception as e:  # noqa: BLE001
        return web.json_response({"error": str(e)}, status=500)


async def init_app():
    app = web.Application()
    app.router.add_get("/metrics", metrics)
    return app


def main():
    app = asyncio.run(init_app())
    web.run_app(app, host="0.0.0.0", port=8000)


if __name__ == "__main__":
    main()
