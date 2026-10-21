// Replaces upstream `SyphonServerRendererMetal.m`, whose shaders come from a
// `default.metallib` in the framework bundle that a static build does not
// have. `nanalive_syphon.m` only publishes textures that take Syphon's blit
// path (BGRA8Unorm, one sample, not framebuffer-only, not flipped), so the
// redraw renderer is never reached; it exists so the server can initialise.

#import "SyphonServerRendererMetal.h"

@implementation SyphonServerRendererMetal

- (instancetype)initWithDevice:(id<MTLDevice>)device colorPixelFormat:(MTLPixelFormat)colorPixelFormat
{
    (void)device;
    (void)colorPixelFormat;
    return [super init];
}

- (void)renderFromTexture:(id<MTLTexture>)offScreenTexture
                inTexture:(id<MTLTexture>)texture
                   region:(NSRect)region
          onCommandBuffer:(id<MTLCommandBuffer>)commandBuffer
                     flip:(BOOL)flip
{
    (void)offScreenTexture;
    (void)texture;
    (void)region;
    (void)commandBuffer;
    (void)flip;
}

@end
