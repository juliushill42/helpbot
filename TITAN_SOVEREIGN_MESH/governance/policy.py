class Policy:
    def __init__(self, rules):
        self.rules = rules

    def check(self, task):
        for r in self.rules:
            if r in task.lower():
                return False
        return True
