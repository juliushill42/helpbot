#!/bin/bash
set -e

if [ ! -d "llama.cpp" ]; then
    git clone https://github.com/ggerganov/llama.cpp.git
    cd llama.cpp
    make -j
    cd ..
fi

g++ -O3 -std=c++11 teacher/serve_teacher.cpp \
    -I llama.cpp \
    -L llama.cpp \
    -l:libllama.a \
    -lpthread -lm \
    -o teacher/serve_teacher

echo "Build complete: teacher/serve_teacher"
