import argparse
import struct
import torch
import torch.nn.functional as F
from student.liquid_ssm import LiquidSSM


def parse_args():
    p = argparse.ArgumentParser(description="Simple validation: teacher vs student logits")
    p.add_argument("--logits", type=str, required=True, help="Path to teacher logits.bin")
    p.add_argument("--tokens", type=str, required=True, help="Path to teacher tokens.bin")
    p.add_argument("--student", type=str, required=True, help="Path to student .pt")
    p.add_argument("--dim", type=int, default=256)
    p.add_argument("--vocab", type=int, default=32000)
    p.add_argument("--threshold", type=float, default=0.85)
    p.add_argument("--sample", type=int, default=4, help="Number of tokens to sample")
    return p.parse_args()


def main():
    args = parse_args()
    vocab = args.vocab
    sample_tokens = args.sample

    # Read tokens
    tokens = []
    with open(args.tokens, "rb") as tf:
        for _ in range(sample_tokens):
            b = tf.read(4)
            if not b:
                break
            tokens.append(struct.unpack("i", b)[0])
    if not tokens:
        print("Validation skipped: no tokens available")
        return
    sample_tokens = len(tokens)

    # Read teacher logits
    with open(args.logits, "rb") as lf:
        buf = lf.read(4 * vocab * sample_tokens)
        if len(buf) < 4 * vocab:
            print("Validation skipped: insufficient logits")
            return
        teacher_logits = torch.tensor(struct.unpack(f"{vocab * sample_tokens}f", buf)).view(sample_tokens, vocab)

    # Load student
    from student.liquid_ssm import LiquidSSM  # noqa: WPS433

    model = LiquidSSM(vocab, d=args.dim)
    model.load_state_dict(torch.load(args.student, map_location="cpu"))
    model.eval()

    state = torch.zeros(1, args.dim)
    student_logits_list = []
    with torch.no_grad():
        for tok in tokens:
            x = torch.tensor([[tok]])
            student_logits, state = model(x, state)
            student_logits_list.append(student_logits.view(-1))
            state = state.detach()
    student_logits = torch.stack(student_logits_list)

    # Similarity (KL-based, temperature=2.0)
    T = 2.0
    p = F.log_softmax(student_logits.mean(dim=0) / T, dim=-1)
    q = F.softmax(teacher_logits.mean(dim=0) / T, dim=-1)
    divergence = F.kl_div(p, q, reduction="batchmean") * (T**2)
    similarity = 1.0 / (1.0 + divergence.item())

    status = "PASS" if similarity >= args.threshold else "FAIL"
    print(f"Similarity: {similarity:.4f} (threshold {args.threshold}) => {status}")


if __name__ == "__main__":
    main()
