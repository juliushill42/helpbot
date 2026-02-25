import argparse
import platform
import coremltools as ct
import torch
from liquid_ssm import LiquidSSM


class LiquidWrapper(torch.nn.Module):
    def __init__(self, base_model):
        super().__init__()
        self.model = base_model

    def forward(self, token, state):
        logits, new_state = self.model(token, state)
        return logits.squeeze(1), new_state


def parse_args():
    p = argparse.ArgumentParser(description="Export LiquidSSM to CoreML")
    p.add_argument("--model", type=str, required=True, help="Path to student .pt")
    p.add_argument("--output", type=str, required=True, help="Path to output .mlpackage")
    p.add_argument("--vocab", type=int, default=32000)
    p.add_argument("--dim", type=int, default=256)
    return p.parse_args()


def main():
    args = parse_args()
    if platform.system() != "Darwin":
        raise RuntimeError("CoreML export requires macOS")

    model = LiquidSSM(args.vocab, args.dim)
    model.load_state_dict(torch.load(args.model, map_location="cpu"))
    model.eval()

    wrapped = LiquidWrapper(model)
    example_token = torch.tensor([[1]])
    example_state = torch.zeros(1, args.dim)
    traced = torch.jit.trace(wrapped, (example_token, example_state))

    mlmodel = ct.convert(
        traced,
        inputs=[
            ct.TensorType(name="token", shape=(1, 1), dtype=int),
            ct.TensorType(name="state", shape=(1, args.dim)),
        ],
        outputs=[
            ct.TensorType(name="logits"),
            ct.TensorType(name="new_state"),
        ],
        minimum_deployment_target=ct.target.iOS15,
    )

    mlmodel.save(args.output)
    print(f"CoreML export complete: {args.output}")


if __name__ == "__main__":
    main()
