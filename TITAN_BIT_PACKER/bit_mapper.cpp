#include <cstddef>
#include <cstdint>

#if defined(__ARM_NEON) || defined(__ARM_NEON__)
#include <arm_neon.h>
#endif

/**
 * Optimized TITAN NEON Kernel
 * Dequantizes 1.58-bit ternary weights (-1, 0, 1) into FP32.
 * Processes 4 values per byte; NEON path handles 8 bytes (32 values) at a time.
 */
extern "C" void titan_unpack_neon(const uint8_t* packed, float* output, size_t num_bytes) {
#if defined(__ARM_NEON) || defined(__ARM_NEON__)
    // LUT maps encoded 2-bit values to signed int8 {-1, 0, 1, 0}.
    alignas(8) static const int8_t kLutData[8] = {-1, 0, 1, 0, 0, 0, 0, 0};
    const int8x8_t lut = vld1_s8(kLutData);
    const uint8x8_t mask = vdup_n_u8(0x03);

    auto store_block = [](int8x8_t src, float* dst) {
        // Widen int8 -> int16 -> int32 -> float32 and store.
        const int16x8_t s16 = vmovl_s8(src);
        const int32x4_t lo32 = vmovl_s16(vget_low_s16(s16));
        const int32x4_t hi32 = vmovl_s16(vget_high_s16(s16));
        vst1q_f32(dst, vcvtq_f32_s32(lo32));
        vst1q_f32(dst + 4, vcvtq_f32_s32(hi32));
    };

    size_t i = 0;
    for (; i + 8 <= num_bytes; i += 8) {
        // Load 8 packed bytes = 32 ternary weights.
        const uint8x8_t v = vld1_u8(packed + i);

        // Extract 2-bit fields.
        const uint8x8_t w0 = vshr_n_u8(v, 6);
        const uint8x8_t w1 = vand_u8(vshr_n_u8(v, 4), mask);
        const uint8x8_t w2 = vand_u8(vshr_n_u8(v, 2), mask);
        const uint8x8_t w3 = vand_u8(v, mask);

        // Map codes to {-1, 0, 1, 0}.
        const int8x8_t r0 = vtbl1_s8(lut, w0);
        const int8x8_t r1 = vtbl1_s8(lut, w1);
        const int8x8_t r2 = vtbl1_s8(lut, w2);
        const int8x8_t r3 = vtbl1_s8(lut, w3);

        // Interleave to restore original per-byte order: w0, w1, w2, w3.
        const uint8x8x2_t z02 = vzip_u8(vreinterpret_u8_s8(r0), vreinterpret_u8_s8(r2));
        const uint8x8x2_t z13 = vzip_u8(vreinterpret_u8_s8(r1), vreinterpret_u8_s8(r3));

        const uint8x8x2_t o0 = vzip_u8(z02.val[0], z13.val[0]); // bytes 0-3 (first half)
        const uint8x8x2_t o1 = vzip_u8(z02.val[1], z13.val[1]); // bytes 4-7 (second half)

        const size_t out_base = i * 4; // 4 floats per packed byte.
        store_block(vreinterpret_s8_u8(o0.val[0]), output + out_base + 0);   // bytes 0-1
        store_block(vreinterpret_s8_u8(o0.val[1]), output + out_base + 8);   // bytes 2-3
        store_block(vreinterpret_s8_u8(o1.val[0]), output + out_base + 16);  // bytes 4-5
        store_block(vreinterpret_s8_u8(o1.val[1]), output + out_base + 24);  // bytes 6-7
    }

    // Tail (fewer than 8 bytes): scalar decode.
    static const float kScalarLut[4] = {-1.0f, 0.0f, 1.0f, 0.0f};
    for (; i < num_bytes; ++i) {
        const uint8_t byte = packed[i];
        const size_t base = i * 4;
        output[base + 0] = kScalarLut[(byte >> 6) & 0x03];
        output[base + 1] = kScalarLut[(byte >> 4) & 0x03];
        output[base + 2] = kScalarLut[(byte >> 2) & 0x03];
        output[base + 3] = kScalarLut[byte & 0x03];
    }
#else
    // Fallback scalar path for non-ARM builds.
    static const float kLut[4] = {-1.0f, 0.0f, 1.0f, 0.0f};
    for (size_t i = 0; i < num_bytes; ++i) {
        const uint8_t byte = packed[i];
        const size_t base = i * 4;
        output[base + 0] = kLut[(byte >> 6) & 0x03];
        output[base + 1] = kLut[(byte >> 4) & 0x03];
        output[base + 2] = kLut[(byte >> 2) & 0x03];
        output[base + 3] = kLut[byte & 0x03];
    }
#endif
}
