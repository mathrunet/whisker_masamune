#import "MWFirestoreExceptionCatcher.h"

@implementation MWFirestoreExceptionCatcher

+ (BOOL)performBlock:(NS_NOESCAPE void (^)(void))block error:(NSError *_Nullable *_Nullable)error {
    @try {
        block();
        return YES;
    } @catch (NSException *exception) {
        if (error) {
            NSString *reason = exception.reason ?: exception.name;
            *error = [NSError errorWithDomain:@"MasamuneWhiskerFirestoreDb"
                                         code:1
                                     userInfo:@{NSLocalizedDescriptionKey : reason}];
        }
        return NO;
    }
}

@end
