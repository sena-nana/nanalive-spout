#import "nanalive_syphon.h"
#import "SyphonMetalServer.h"

#include <string.h>

// One frame in flight at a time: upstream resizes and reads its IOSurface
// without a lock, and `stop` releases it, so neither may overlap a pending
// completion handler.
@interface NanaLiveSyphonHandle : NSObject
@property (nonatomic, strong) SyphonMetalServer *server;
@property (nonatomic) BOOL inFlight;
@property (nonatomic) BOOL stopAfterFlight;
@end

@implementation NanaLiveSyphonHandle
@end

static void write_error(char *error, size_t error_len, NSString *message)
{
    if (error == NULL || error_len == 0) {
        return;
    }
    const char *utf8 = message.UTF8String ?: "unknown Syphon error";
    strlcpy(error, utf8, error_len);
}

static void finish_flight(NanaLiveSyphonHandle *handle)
{
    SyphonMetalServer *stopping = nil;
    @synchronized (handle) {
        handle.inFlight = NO;
        if (handle.stopAfterFlight) {
            stopping = handle.server;
            handle.server = nil;
        }
    }
    [stopping stop];
}

void *nl_syphon_server_create(const char *name_utf8, void *mtl_device, char *error, size_t error_len)
{
    @autoreleasepool {
        @try {
            id<MTLDevice> device = (__bridge id<MTLDevice>)mtl_device;
            if (device == nil) {
                write_error(error, error_len, @"no Metal device");
                return NULL;
            }
            NSString *name = name_utf8 ? [NSString stringWithUTF8String:name_utf8] : nil;
            SyphonMetalServer *server = [[SyphonMetalServer alloc] initWithName:name device:device options:nil];
            if (server == nil) {
                write_error(error, error_len, @"SyphonMetalServer could not start");
                return NULL;
            }
            NanaLiveSyphonHandle *handle = [NanaLiveSyphonHandle new];
            handle.server = server;
            return (__bridge_retained void *)handle;
        } @catch (NSException *exception) {
            write_error(error, error_len, exception.reason ?: exception.name);
            return NULL;
        }
    }
}

int32_t nl_syphon_server_publish(void *server, void *mtl_texture, void *mtl_command_buffer,
                                 uint32_t width, uint32_t height)
{
    @autoreleasepool {
        NanaLiveSyphonHandle *handle = (__bridge NanaLiveSyphonHandle *)server;
        @try {
            id<MTLTexture> texture = (__bridge id<MTLTexture>)mtl_texture;
            id<MTLCommandBuffer> commandBuffer = (__bridge id<MTLCommandBuffer>)mtl_command_buffer;
            if (texture == nil || commandBuffer == nil
                || texture.pixelFormat != MTLPixelFormatBGRA8Unorm
                || texture.sampleCount != 1
                || texture.framebufferOnly
                || width == 0 || height == 0
                || width > texture.width || height > texture.height) {
                return NL_SYPHON_NOT_BLITTABLE;
            }
            SyphonMetalServer *target = nil;
            @synchronized (handle) {
                if (handle.server == nil || handle.stopAfterFlight) {
                    return NL_SYPHON_STOPPED;
                }
                if (handle.inFlight) {
                    return NL_SYPHON_BUSY;
                }
                handle.inFlight = YES;
                target = handle.server;
            }
            [target publishFrameTexture:texture
                        onCommandBuffer:commandBuffer
                            imageRegion:NSMakeRect(0, 0, width, height)
                                flipped:NO];
            // Metal runs completed handlers in the order they were added, so
            // this one runs after Syphon's own `publish`.
            [commandBuffer addCompletedHandler:^(id<MTLCommandBuffer> done) {
                (void)done;
                finish_flight(handle);
            }];
            return NL_SYPHON_OK;
        } @catch (NSException *exception) {
            (void)exception;
            @synchronized (handle) {
                handle.inFlight = NO;
            }
            return NL_SYPHON_EXCEPTION;
        }
    }
}

bool nl_syphon_server_has_clients(void *server)
{
    @autoreleasepool {
        NanaLiveSyphonHandle *handle = (__bridge NanaLiveSyphonHandle *)server;
        @try {
            SyphonMetalServer *target = nil;
            @synchronized (handle) {
                target = handle.server;
            }
            return target != nil && target.hasClients;
        } @catch (NSException *exception) {
            (void)exception;
            return false;
        }
    }
}

bool nl_syphon_server_copy_frame(void *server, uint8_t *dst, size_t dst_len,
                                 uint32_t *width, uint32_t *height)
{
    @autoreleasepool {
        NanaLiveSyphonHandle *handle = (__bridge NanaLiveSyphonHandle *)server;
        @try {
            SyphonMetalServer *target = nil;
            @synchronized (handle) {
                if (handle.inFlight) {
                    return false;
                }
                target = handle.server;
            }
            id<MTLTexture> frame = [target newFrameImage];
            IOSurfaceRef surface = frame.iosurface;
            if (surface == NULL) {
                return false;
            }
            size_t w = IOSurfaceGetWidth(surface);
            size_t h = IOSurfaceGetHeight(surface);
            if (width != NULL) {
                *width = (uint32_t)w;
            }
            if (height != NULL) {
                *height = (uint32_t)h;
            }
            size_t row = w * 4;
            if (dst == NULL || dst_len < row * h) {
                return false;
            }
            if (IOSurfaceLock(surface, kIOSurfaceLockReadOnly, NULL) != kIOReturnSuccess) {
                return false;
            }
            const uint8_t *base = IOSurfaceGetBaseAddress(surface);
            size_t stride = IOSurfaceGetBytesPerRow(surface);
            for (size_t y = 0; y < h; y++) {
                memcpy(dst + y * row, base + y * stride, row);
            }
            IOSurfaceUnlock(surface, kIOSurfaceLockReadOnly, NULL);
            return true;
        } @catch (NSException *exception) {
            (void)exception;
            return false;
        }
    }
}

void nl_syphon_server_destroy(void *server)
{
    @autoreleasepool {
        if (server == NULL) {
            return;
        }
        NanaLiveSyphonHandle *handle = (__bridge_transfer NanaLiveSyphonHandle *)server;
        @try {
            SyphonMetalServer *stopping = nil;
            @synchronized (handle) {
                if (handle.inFlight) {
                    handle.stopAfterFlight = YES;
                } else {
                    stopping = handle.server;
                    handle.server = nil;
                }
            }
            [stopping stop];
        } @catch (NSException *exception) {
            (void)exception;
        }
    }
}
