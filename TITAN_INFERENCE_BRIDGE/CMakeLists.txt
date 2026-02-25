cmake_minimum_required(VERSION 3.16)
project(titan_inference_bridge CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_subdirectory(llama.cpp)

add_executable(titan_bridge bridge.cpp)
target_include_directories(titan_bridge PRIVATE llama.cpp)
target_link_libraries(titan_bridge PRIVATE llama)
