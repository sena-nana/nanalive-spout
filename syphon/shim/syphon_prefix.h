// Stands in for upstream `Syphon_Prefix.pch`, which Xcode injects into every
// Objective-C file. Logging stays off, as in upstream release builds.
#ifdef __OBJC__
#import <AppKit/AppKit.h>
#import <CoreVideo/CoreVideo.h>
#import <Foundation/Foundation.h>
#import <IOSurface/IOSurface.h>
#import <Metal/Metal.h>
#define SYPHONLOG(format, ...)
#endif
