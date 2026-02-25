#!/bin/bash
set -e

echo "=== Building teacher ==="
bash teacher/build_teacher.sh

echo "=== Running teacher (this will take time) ==="
./teacher/serve_teacher

echo "=== Training student ==="
python3 student/train_student.py

echo "=== Exporting to CoreML ==="
python3 student/export_coreml.py

echo "=== COMPLETE ==="
ls -lh student/liquid.*
