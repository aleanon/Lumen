// Minimal UIKit host for the Lumen iOS shell (T3.3). Built on macOS by
// scripts/ios_orchestrate.sh; links libhello_ios.a and presents the CPU frame.
// A CADisplayLink drives the loop; production uses a CAMetalLayer + MTLTexture,
// here we present via CoreGraphics for a self-contained template. UITouches map
// to Lumen pointer events; the safe-area insets crop the drawable; UITextInput
// bridges IME.
#import <UIKit/UIKit.h>

extern size_t lumen_ios_render(uint32_t w, uint32_t h, uint8_t *out, size_t out_len);
// P.5: the session FFI (implemented in the Rust lib; host-compilable + tested).
extern void lumen_ios_touch(uint32_t phase, double x, double y);
extern void lumen_ios_text(const char *utf8);

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
// P.5: UITouch → session FFI; the view redraws after each event.
- (void)dispatchTouch:(NSSet<UITouch *> *)touches phase:(uint32_t)phase {
    UITouch *t = touches.anyObject;
    CGPoint p = [t locationInView:self];
    lumen_ios_touch(phase, p.x, p.y);
    [self setNeedsDisplay];
}
- (void)touchesBegan:(NSSet<UITouch *> *)t withEvent:(UIEvent *)e { [self dispatchTouch:t phase:0]; }
- (void)touchesMoved:(NSSet<UITouch *> *)t withEvent:(UIEvent *)e { [self dispatchTouch:t phase:1]; }
- (void)touchesEnded:(NSSet<UITouch *> *)t withEvent:(UIEvent *)e { [self dispatchTouch:t phase:2]; }
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
