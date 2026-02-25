#!/usr/bin/env bash
set -euo pipefail

MODEL_PATH=${MODEL_PATH:-MiniMax-M2.1.q2_k.gguf}
PROMPTS_PATH=${PROMPTS_PATH:-data/prompts.txt}
OUT_DIR=${OUT_DIR:-outputs/local_run}
THREADS=${THREADS:-8}
CTX=${CTX:-512}
BATCH=${BATCH:-256}
DIM=${DIM:-256}
STEPS=${STEPS:-500}

mkdir -p "$OUT_DIR"
log() { echo "$(date '+%F %T') $*" | tee -a "$OUT_DIR/run.log"; }

log "Building teacher (bash)..."
bash teacher/build_teacher.sh

log "Running teacher to extract logits/tokens..."
./teacher/serve_teacher \
  --model "$MODEL_PATH" \
  --prompts "$PROMPTS_PATH" \
  --output "$OUT_DIR/logits.bin" \
  --tokens "$OUT_DIR/tokens.bin" \
  --ctx "$CTX" --threads "$THREADS" --batch "$BATCH"

log "Training student..."
python3 student/train_student.py \
  --logits "$OUT_DIR/logits.bin" \
  --tokens "$OUT_DIR/tokens.bin" \
  --output "$OUT_DIR/student.pt" \
  --dim "$DIM" --steps "$STEPS" --vocab 32000

log "Validating..."
python3 agent/simple_validate.py \
  --logits "$OUT_DIR/logits.bin" \
  --tokens "$OUT_DIR/tokens.bin" \
  --student "$OUT_DIR/student.pt" \
  --dim "$DIM" --vocab 32000 --threshold 0.85 | tee -a "$OUT_DIR/run.log"

log "Done. Outputs in $OUT_DIR"
