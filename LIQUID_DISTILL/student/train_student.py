import argparse
import struct
import torch
import torch.nn.functional as F
from liquid_ssm import LiquidSSM


def parse_args():
    p = argparse.ArgumentParser(description="Train student via distillation")
    p.add_argument("--logits", type=str, required=True, help="Path to teacher logits.bin")
    p.add_argument("--tokens", type=str, required=True, help="Path to teacher tokens.bin")
    p.add_argument("--output", type=str, required=True, help="Where to save student .pt")
    p.add_argument("--dim", type=int, default=256, help="Model hidden dim")
    p.add_argument("--lr", type=float, default=1e-3, help="Learning rate")
    p.add_argument("--steps", type=int, default=10000, help="Max training steps (cap)")
    p.add_argument("--vocab", type=int, default=32000, help="Vocab size")
    return p.parse_args()


def main():
    args = parse_args()
    model = LiquidSSM(args.vocab, args.dim)
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr)

    with open(args.tokens, "rb") as tok_f, open(args.logits, "rb") as log_f:
        state = torch.zeros(1, args.dim)
        step = 0

        while True:
            if step >= args.steps:
                break
            tok_bytes = tok_f.read(4)  # llama_token is int32
            if not tok_bytes:
                break
            token = struct.unpack("i", tok_bytes)[0]

            logits_bytes = log_f.read(4 * args.vocab)
            if not logits_bytes:
                break
            teacher_logits = torch.tensor(struct.unpack(f"{args.vocab}f", logits_bytes))

            x = torch.tensor([[token]])
            student_logits, state = model(x, state)
            state = state.detach()  # truncated BPTT

            loss = F.kl_div(
                F.log_softmax(student_logits.squeeze(0), dim=-1),
                F.softmax(teacher_logits, dim=-1),
                reduction="batchmean",
            )
            loss.backward()
            opt.step()
            opt.zero_grad()

            if step % 100 == 0:
                print(f"Step {step}, Loss: {loss.item():.4f}")
            step += 1

    torch.save(model.state_dict(), args.output)
    print(f"Training complete. {step} steps. Saved to {args.output}")


if __name__ == "__main__":
    main()
