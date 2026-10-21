// Flat C surface over the Syphon Metal server, called from `src/sys.rs`.
//
// Every function drains its own autorelease pool and turns Objective-C
// exceptions into status codes, so callers may run on a plain thread.

#ifndef NANALIVE_SYPHON_H
#define NANALIVE_SYPHON_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

enum {
    NL_SYPHON_OK = 0,
    // The previous frame's command buffer has not completed yet.
    NL_SYPHON_BUSY = 1,
    // Not BGRA8Unorm, multisampled, framebuffer-only, or the size is out of range.
    NL_SYPHON_NOT_BLITTABLE = 2,
    NL_SYPHON_STOPPED = 3,
    NL_SYPHON_EXCEPTION = 4,
};

// Returns an owned handle, or NULL with a NUL-terminated reason in `error`.
void *nl_syphon_server_create(const char *name_utf8, void *mtl_device, char *error, size_t error_len);

// Encodes a blit of the top-left `width`x`height` of `mtl_texture` onto
// `mtl_command_buffer`. On NL_SYPHON_OK the caller must commit the buffer;
// clients see the frame once it completes.
int32_t nl_syphon_server_publish(void *server, void *mtl_texture, void *mtl_command_buffer,
                                 uint32_t width, uint32_t height);

bool nl_syphon_server_has_clients(void *server);

// Copies the last published frame as tightly packed BGRA rows, top row first.
// Returns false when nothing was published yet or `dst` is too small.
bool nl_syphon_server_copy_frame(void *server, uint8_t *dst, size_t dst_len,
                                 uint32_t *width, uint32_t *height);

// Stops the server and releases the handle. A frame still in flight is
// allowed to finish first; the stop then happens on its completion.
void nl_syphon_server_destroy(void *server);

#endif
