from agentic.tools import analyze, decide, respond

TOOLS = {
    "analyze": analyze,
    "decide": decide,
    "respond": respond
}

def execute(steps, model):
    state = {}
    for s in steps:
        state = TOOLS[s](state, model)
    return state["output"]
