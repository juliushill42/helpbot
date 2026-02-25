import json, time

class Ledger:
    def __init__(self, path="governance/ledger.json"):
        self.path = path
        self.entries = []
        self.load()

    def load(self):
        try:
            self.entries = json.load(open(self.path))
        except:
            self.entries = []

    def record(self, task, output):
        entry = {
            "time": time.time(),
            "task": task,
            "output": output
        }
        self.entries.append(entry)
        json.dump(self.entries, open(self.path, "w"))
