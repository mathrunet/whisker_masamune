// Turns Objective-C exceptions into Swift errors.
//
// The Firestore iOS SDK reports invalid arguments (bad paths, invalid queries, settings changed
// after first use, ...) by raising NSException, which Swift cannot catch. Android reports the same
// mistakes as exceptions that the module returns as errors, so the iOS module runs those calls
// through this helper to keep the same error behaviour instead of crashing.

#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

@interface MWFirestoreExceptionCatcher : NSObject

/// Runs `block` and converts a raised NSException into an NSError (the exception reason becomes
/// the localized description).
+ (BOOL)performBlock:(NS_NOESCAPE void (^)(void))block
               error:(NSError *_Nullable *_Nullable)error NS_SWIFT_NAME(perform(_:));

@end

NS_ASSUME_NONNULL_END
