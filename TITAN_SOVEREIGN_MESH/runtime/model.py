class MiniMax80B:
    def __init__(self, cfg):
        self.context = cfg["context_limit"]

    def call(self, mode, prompt):
        return f"[{mode}] OK"
