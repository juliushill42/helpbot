import torch
import torch.nn as nn


class LiquidSSM(nn.Module):
    def __init__(self, vocab, d=256):
        super().__init__()
        self.vocab = vocab
        self.d = d

        self.embed = nn.Embedding(vocab, d)
        self.A = nn.Parameter(torch.randn(d, d) * 0.01)
        self.B = nn.Parameter(torch.randn(d, d) * 0.01)
        self.C = nn.Parameter(torch.randn(d, vocab) * 0.01)
        self.norm = nn.LayerNorm(d)

    def forward(self, x, state):
        # x: [batch, seq_len] or [batch]
        if x.dim() == 1:
            x = x.unsqueeze(1)

        e = self.embed(x)  # [batch, seq_len, d]

        # Recurrent state update
        state = torch.tanh(state @ self.A + e.squeeze(1) @ self.B)
        state = self.norm(state)

        # Output projection
        logits = state @ self.C  # [batch, vocab]

        return logits.unsqueeze(1), state
