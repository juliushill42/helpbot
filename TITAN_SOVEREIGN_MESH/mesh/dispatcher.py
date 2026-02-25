from agentic.planner import plan
from agentic.executor import execute

def dispatch(task, model, authority):
    steps = plan(task)
    result = execute(steps, model)
    authority.ledger.record(task, result)
    return result
