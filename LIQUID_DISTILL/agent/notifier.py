import hmac
import hashlib


async def send_webhook_alert(event_type: str, job_name: str, status: str, secret: str = "YOUR_SECRET", url: str = "https://your-endpoint.com/webhook"):
    try:
        import aiohttp  # type: ignore
    except ImportError:
        return  # silently skip if aiohttp not installed

    payload = {
        "event": event_type,  # e.g., "distillation_complete"
        "model": job_name,
        "status": status,
    }
    signature = hmac.new(secret.encode(), str(payload).encode(), hashlib.sha256).hexdigest()
    headers = {"x-titan-signature": signature}

    async with aiohttp.ClientSession() as session:
        async with session.post(url, json=payload, headers=headers) as resp:
            await resp.text()
