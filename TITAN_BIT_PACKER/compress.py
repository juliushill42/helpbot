import argparse
import json
from pathlib import Path
from typing import Dict, Tuple

import numpy as np
import torch
from tqdm import tqdm


def pack_ternary(weights: torch.Tensor, align_bytes: int = 16) -> Tuple[np.ndarray, float, int]:
    """
    Compresses weights to 1.58-bit (ternary).
    Mapping: -1 -> 00, 0 -> 01, 1 -> 10 (2 bits per weight). Packs 4 weights per uint8.
    Returns (packed_bytes, scale, pad_count).
    """
    scale = weights.abs().mean()
    t_weights = torch.round(weights / (scale + 1e-8)).clamp(-1, 1).to(torch.int8)

    unsigned_weights = (t_weights + 1).to(torch.uint8)
    flat = unsigned_weights.flatten()

    # Pad to multiple of 4 weights (1 byte) then to align_bytes for packed buffer
    pad_weights = (-flat.numel()) % 4
    if pad_weights:
        flat = torch.cat([flat, torch.zeros(pad_weights, dtype=torch.uint8)])

    packed_len = flat.numel() // 4
    pad_bytes = (-packed_len) % align_bytes
    if pad_bytes:
        flat = torch.cat([flat, torch.zeros(pad_bytes * 4, dtype=torch.uint8)])
        pad_weights += pad_bytes * 4

    reshaped = flat.view(-1, 4)
    packed = (reshaped[:, 0] << 6) | (reshaped[:, 1] << 4) | (reshaped[:, 2] << 2) | reshaped[:, 3]

    return packed.cpu().numpy(), float(scale.item()), int(pad_weights)


def convert_model(input_path: Path, output_path: Path, metadata_path: Path, align_bytes: int = 16) -> None:
    ckpt = torch.load(str(input_path), map_location="cpu")
    state_dict = ckpt["state_dict"] if isinstance(ckpt, dict) and "state_dict" in ckpt else ckpt

    packed_dict: Dict[str, np.ndarray] = {}
    metadata = []

    for name, param in tqdm(state_dict.items(), desc="Ternary Packing"):
        if isinstance(param, torch.Tensor) and "weight" in name and param.ndim > 1:
            packed_weights, scale, pad = pack_ternary(param, align_bytes=align_bytes)
            packed_key = f"{name}__packed"
            scale_key = f"{name}__scale"

            packed_dict[packed_key] = packed_weights
            packed_dict[scale_key] = np.array(scale, dtype=np.float32)

            metadata.append(
                {
                    "name": name,
                    "shape": list(param.shape),
                    "dtype": str(param.dtype),
                    "packed_key": packed_key,
                    "scale_key": scale_key,
                    "pad_weights": pad,
                    "align_bytes": align_bytes,
                    "format": "ternary_b2",
                }
            )
        elif isinstance(param, torch.Tensor):
            packed_dict[name] = param.cpu().numpy()
        else:
            packed_dict[name] = np.array(param)

    np.savez_compressed(output_path, **packed_dict)
    metadata_path.write_text(json.dumps(metadata, indent=2))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Pack a model to 1.58-bit ternary format.")
    parser.add_argument("--input", type=Path, required=True, help="Path to raw PyTorch checkpoint (.pt/.bin).")
    parser.add_argument("--output", type=Path, required=True, help="Output .npz path for packed weights.")
    parser.add_argument(
        "--metadata",
        type=Path,
        help="Optional metadata json path (defaults to OUTPUT with .json).",
    )
    parser.add_argument(
        "--align-bytes",
        type=int,
        default=16,
        help="Packed-byte alignment for ANE/NEON (default: 16).",
    )
    return parser.parse_args()


if __name__ == "__main__":
    args = parse_args()
    meta_path = args.metadata or args.output.with_suffix(".json")
    convert_model(args.input, args.output, meta_path, align_bytes=args.align_bytes)
