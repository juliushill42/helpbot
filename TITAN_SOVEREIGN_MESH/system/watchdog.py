import psutil, os, signal

def enforce(pid, max_mem):
    p = psutil.Process(pid)
    if p.memory_info().rss > max_mem:
        os.kill(pid, signal.SIGKILL)
