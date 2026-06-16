// Minimal UIKit host for the Lumen iOS shell (T3.3). Built on macOS by
// scripts/ios_orchestrate.sh; links libhello_ios.a and presents the CPU frame.
// A CADisplayLink drives the loop; production uses a CAMetalLayer + MTLTexture,
// here we present via CoreGraphics for a self-contained template. UITouches map
// to Lumen pointer events; the safe-area insets crop the drawable; UITextInput
// bridges IME. (Touch/IME FFI: lumen_ios_touch/lumen_ios_text — declared in the
// Rust shell as they are wired on-device.)
#import <UIKit/UIKit.h>

extern size_t lumen_ios_render(uint32_t w, uint32_t h, uint8_t *out, size_t out_len);

@interface LumenView : UIView @end
@implementation LumenView
- (void)drawRect:(CGRect)rect {
    CGFloat scale = self.contentScaleFactor;
    uint32_t w = (uint32_t)(self.bounds.size.width * scale);
    uint32_t h = (uint32_t)(self.bounds.size.height * scale);
    size_t len = (size_t)w * h * 4;
    uint8_t *buf = malloc(len);
    if (lumen_ios_render(w, h, buf, len) == len) {
        CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
        CGContextRef ctx = CGBitmapContextCreate(buf, w, h, 8, w * 4, cs,
            kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big);
        CGImageRef img = CGBitmapContextCreateImage(ctx);
        CGContextRef cur = UIGraphicsGetCurrentContext();
        CGContextDrawImage(cur, self.bounds, img);
        CGImageRelease(img); CGContextRelease(ctx); CGColorSpaceRelease(cs);
    }
    free(buf);
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property (strong, nonatomic) UIWindow *window;
@end
@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)opts {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    UIViewController *vc = [UIViewController new];
    vc.view = [[LumenView alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = vc;
    [self.window makeKeyAndVisible];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool { return UIApplicationMain(argc, argv, nil, NSStringFromClass([AppDelegate class])); }
}
