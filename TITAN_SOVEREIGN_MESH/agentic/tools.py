def analyze(state, model):
    state["analysis"] = model.call("analyze", "")
    return state

def decide(state, model):
    state["decision"] = model.call("decide", state["analysis"])
    return state

def respond(state, model):
    state["output"] = model.call("respond", state["decision"])
    return state
