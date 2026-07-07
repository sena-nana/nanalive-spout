/*
 * Narrow C ABI for NanaVTS Spout sender output.
 */
#ifndef NANAVTS_SPOUT_SHIM_H
#define NANAVTS_SPOUT_SHIM_H

#ifdef __cplusplus
extern "C" {
#endif

int spout_get_sdk_version(char* buf, int maxlen);

#if defined(NANAVTS_SPOUT_CPU_DX11)

typedef struct spout_dx_t spout_dx_t;

spout_dx_t*  spout_dx_create(void);
void         spout_dx_destroy(spout_dx_t* h);
int          spout_dx_open_directx11(spout_dx_t* h, void* device);
void*        spout_dx_get_device(spout_dx_t* h);
int          spout_dx_set_sender_name(spout_dx_t* h, const char* name);
void         spout_dx_set_sender_format(spout_dx_t* h, unsigned int dxgi_format);
void         spout_dx_release_sender(spout_dx_t* h);
int          spout_dx_send_image(spout_dx_t* h, const unsigned char* data,
                                 unsigned int width, unsigned int height, unsigned int pitch);
int          spout_dx_is_initialized(spout_dx_t* h);
unsigned int spout_dx_get_width(spout_dx_t* h);
unsigned int spout_dx_get_height(spout_dx_t* h);
double       spout_dx_get_fps(spout_dx_t* h);
long         spout_dx_get_frame(spout_dx_t* h);

#endif

#if defined(NANAVTS_SPOUT_GPU_DX12)

typedef struct spout_dx12_t spout_dx12_t;

spout_dx12_t* spout_dx12_create(void);
void          spout_dx12_destroy(spout_dx12_t* h);
int           spout_dx12_open_directx12(spout_dx12_t* h, void* device, void** command_queue);
void*         spout_dx12_get_d3d12_device(spout_dx12_t* h);
int           spout_dx12_wrap_resource(spout_dx12_t* h, void* d3d12_resource,
                                       unsigned int initial_state, void** out_wrapped11);
int           spout_dx12_send_wrapped_resource(spout_dx12_t* h, void* wrapped11);
void          spout_dx12_release_wrapped_resource(void* wrapped11);
int           spout_dx12_set_sender_name(spout_dx12_t* h, const char* name);
void          spout_dx12_set_sender_format(spout_dx12_t* h, unsigned int dxgi_format);
void          spout_dx12_release_sender(spout_dx12_t* h);
int           spout_dx12_is_initialized(spout_dx12_t* h);
unsigned int  spout_dx12_get_width(spout_dx12_t* h);
unsigned int  spout_dx12_get_height(spout_dx12_t* h);
double        spout_dx12_get_fps(spout_dx12_t* h);
long          spout_dx12_get_frame(spout_dx12_t* h);

#endif

#ifdef __cplusplus
}
#endif

#endif
