#include "llama.h"
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

struct Args {
    std::string model_path;
    std::string prompts_path;
    std::string logits_path;
    std::string tokens_path;
    int n_ctx = 512;
    int n_threads = 16;
    int n_batch = 512;
};

Args parse_args(int argc, char **argv) {
    Args args;
    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        if (a == "--model" && i + 1 < argc) args.model_path = argv[++i];
        else if (a == "--prompts" && i + 1 < argc) args.prompts_path = argv[++i];
        else if (a == "--output" && i + 1 < argc) args.logits_path = argv[++i];
        else if (a == "--tokens" && i + 1 < argc) args.tokens_path = argv[++i];
        else if (a == "--ctx" && i + 1 < argc) args.n_ctx = std::stoi(argv[++i]);
        else if (a == "--threads" && i + 1 < argc) args.n_threads = std::stoi(argv[++i]);
        else if (a == "--batch" && i + 1 < argc) args.n_batch = std::stoi(argv[++i]);
    }
    if (args.model_path.empty()) args.model_path = "MiniMax-M2.1.q2_k.gguf";
    if (args.prompts_path.empty()) args.prompts_path = "data/prompts.txt";
    if (args.logits_path.empty()) args.logits_path = "data/logits.bin";
    if (args.tokens_path.empty()) args.tokens_path = "data/tokens.bin";
    return args;
}

int main(int argc, char **argv) {
    Args args = parse_args(argc, argv);

    llama_backend_init();

    llama_model_params mp = llama_model_default_params();
    llama_context_params cp = llama_context_default_params();
    cp.n_ctx = args.n_ctx;
    cp.n_threads = args.n_threads;
    cp.n_batch = args.n_batch;

    llama_model *m = llama_load_model_from_file(args.model_path.c_str(), mp);
    if (!m) { std::cerr << "Model load failed\n"; return 1; }
    
    llama_context *c = llama_new_context_with_model(m, cp);
    if (!c) { std::cerr << "Context failed\n"; llama_free_model(m); return 1; }

    int32_t vocab_size = llama_n_vocab(m);

    std::ifstream in(args.prompts_path);
    std::ofstream logits_out(args.logits_path, std::ios::binary);
    std::ofstream tokens_out(args.tokens_path, std::ios::binary);
    
    if (!in || !logits_out || !tokens_out) {
        std::cerr << "File open failed\n";
        llama_free(c); llama_free_model(m);
        return 1;
    }

    std::string prompt;
    while (std::getline(in, prompt)) {
        if (prompt.empty()) continue;
        
        llama_kv_cache_clear(c);

        std::vector<llama_token> tokens(prompt.size() + 16);
        int n_tokens = llama_tokenize(m, prompt.c_str(), prompt.size(), 
                                       tokens.data(), tokens.size(), true, false);
        if (n_tokens < 0) {
            tokens.resize(-n_tokens);
            n_tokens = llama_tokenize(m, prompt.c_str(), prompt.size(), 
                                       tokens.data(), tokens.size(), true, false);
        }
        tokens.resize(n_tokens);

        for (int i = 0; i < n_tokens; i++) {
            llama_batch batch = llama_batch_get_one(&tokens[i], 1);
            
            if (llama_decode(c, batch) != 0) {
                std::cerr << "Decode failed at token " << i << "\n";
                break;
            }

            tokens_out.write(reinterpret_cast<char*>(&tokens[i]), sizeof(llama_token));
            
            float *logits = llama_get_logits(c);
            logits_out.write(reinterpret_cast<char*>(logits), sizeof(float) * vocab_size);
        }
    }

    llama_free(c);
    llama_free_model(m);
    llama_backend_free();
    return 0;
}
