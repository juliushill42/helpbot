import os


def compute_max_parallel(config_max: int, max_ram_gb: int, per_job_ram_gb: int) -> int:
    """
    Adaptive cap for parallel jobs based on available RAM and CPU cores.
    Falls back to config_max if telemetry is unavailable.
    """
    try:
        import psutil

        avail_gb = psutil.virtual_memory().available / (1024 ** 3)
        core_cap = os.cpu_count() or 1
        ram_cap = max(1, int(avail_gb // per_job_ram_gb))
        return max(1, min(config_max, ram_cap, core_cap))
    except Exception:
        return max(1, config_max)
