import json

class Memory:
    def __init__(self, path="runtime/memory.json"):
        self.path = path
        try:
            self.data = json.load(open(path))
        except:
            self.data = []

    def add(self, item):
        self.data.append(item)
        json.dump(self.data, open(self.path, "w"))
