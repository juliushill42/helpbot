#include "llama.h"

#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <cerrno>
#include <csignal>
#include <cstring>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

namespace {

struct Config {
    std::string model_path;
    int n_ctx = 2048;
    int n_threads = 8;
    int n_batch = 256;
    std::string sock_path = "/tmp/titan_infer.sock";
};

Config load_config(const std::string &path) {
    Config cfg;
    std::ifstream f(path);
    if (!f) {
        std::cerr << "Config not found: " << path << ", using defaults.\n";
        return cfg;
    }
    std::string line;
    while (std::getline(f, line)) {
        if (line.find("path:") != std::string::npos) {
            cfg.model_path = line.substr(line.find(":") + 1);
            cfg.model_path.erase(0, cfg.model_path.find_first_not_of(" \t"));
        } else if (line.find("ctx:") != std::string::npos) {
            cfg.n_ctx = std::stoi(line.substr(line.find(":") + 1));
        } else if (line.find("threads:") != std::string::npos) {
            cfg.n_threads = std::stoi(line.substr(line.find(":") + 1));
        } else if (line.find("batch:") != std::string::npos) {
            cfg.n_batch = std::stoi(line.substr(line.find(":") + 1));
        } else if (line.find("socket:") != std::string::npos) {
            // ignore
        } else if (line.find("path") != std::string::npos && line.find("socket") == std::string::npos) {
            cfg.sock_path = line.substr(line.find(":") + 1);
            cfg.sock_path.erase(0, cfg.sock_path.find_first_not_of(" \t"));
        }
    }
    return cfg;
}

int create_socket(const std::string &path) {
    int fd = ::socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) {
        std::perror("socket");
        std::exit(1);
    }
    sockaddr_un addr{};
    addr.sun_family = AF_UNIX;
    std::strncpy(addr.sun_path, path.c_str(), sizeof(addr.sun_path) - 1);
    ::unlink(path.c_str());
    if (::bind(fd, reinterpret_cast<sockaddr *>(&addr), sizeof(addr)) < 0) {
        std::perror("bind");
        std::exit(1);
    }
    if (::listen(fd, 4) < 0) {
        std::perror("listen");
        std::exit(1);
    }
    return fd;
}

std::string recv_all(int client) {
    char buf[4096];
    ssize_t n = ::recv(client, buf, sizeof(buf) - 1, 0);
    if (n <= 0) return {};
    buf[n] = '\0';
    return std::string(buf);
}

void send_all(int client, const std::string &s) {
    const char *p = s.c_str();
    size_t left = s.size();
    while (left > 0) {
        ssize_t n = ::send(client, p, left, 0);
        if (n <= 0) break;
        p += n;
        left -= static_cast<size_t>(n);
    }
}

}  // namespace

int main() {
    Config cfg = load_config("config.yaml");
    if (cfg.model_path.empty()) {
        cfg.model_path = "./models/minimax-80b.q4_k_m.gguf";
    }

    llama_backend_init(false);

    llama_model_params mparams = llama_model_default_params();
    mparams.use_mmap = true;
    mparams.use_mlock = false;
    mparams.n_gpu_layers = 0;  // CPU only

    llama_context_params cparams = llama_context_default_params();
    cparams.n_ctx = cfg.n_ctx;
    cparams.n_threads = cfg.n_threads;
    cparams.n_batch = cfg.n_batch;

    llama_model *model = llama_load_model_from_file(cfg.model_path.c_str(), mparams);
    if (!model) {
        std::cerr << "Failed to load model: " << cfg.model_path << "\n";
        return 1;
    }
    llama_context *ctx = llama_new_context_with_model(model, cparams);
    if (!ctx) {
        std::cerr << "Failed to create context\n";
        llama_free_model(model);
        return 1;
    }

    int server_fd = create_socket(cfg.sock_path);
    std::cout << "Listening on " << cfg.sock_path << "\n";

    while (true) {
        int client = ::accept(server_fd, nullptr, nullptr);
        if (client < 0) {
            if (errno == EINTR) continue;
            std::perror("accept");
            break;
        }

        std::string prompt = recv_all(client);
        if (prompt.empty()) {
            ::close(client);
            continue;
        }

        std::vector<llama_token> tokens = llama_tokenize(ctx, prompt, true);
        if (llama_eval(ctx, tokens.data(), static_cast<int>(tokens.size()), 0, cfg.n_threads) != 0) {
            std::string err = "[ERR] eval failed\n";
            send_all(client, err);
            ::close(client);
            continue;
        }

        std::string output;
        int total = static_cast<int>(tokens.size());
        const int n_vocab = llama_n_vocab(model);
        for (int i = 0; i < 128; ++i) {
            const float *logits = llama_get_logits(ctx);
            int best = 0;
            float best_logit = logits[0];
            for (int t = 1; t < n_vocab; ++t) {
                if (logits[t] > best_logit) {
                    best_logit = logits[t];
                    best = t;
                }
            }
            llama_token tok = best;
            if (llama_eval(ctx, &tok, 1, total + i, cfg.n_threads) != 0) break;
            std::string piece = llama_token_to_piece(ctx, tok);
            output += piece;
            if (tok == llama_token_eos(model)) break;
        }
        send_all(client, output);
        ::close(client);
    }

    ::close(server_fd);
    llama_free(ctx);
    llama_free_model(model);
    llama_backend_free();
    return 0;
}
