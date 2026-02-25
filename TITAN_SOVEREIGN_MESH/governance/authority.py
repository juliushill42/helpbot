from governance.policy import Policy
from governance.ledger import Ledger
from governance.evaluator import Evaluator

class Authority:
    def __init__(self, cfg):
        self.policy = Policy(["shutdown", "self destruct"])
        self.ledger = Ledger()
        self.eval = Evaluator()
        self.max_tokens = cfg["max_tokens"]

    def authorize(self, task):
        if not self.policy.check(task):
            return {"allowed": False}
        if self.eval.score(task) > self.max_tokens:
            return {"allowed": False}
        return {"allowed": True}
