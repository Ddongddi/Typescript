#import <Cocoa/Cocoa.h>
#import <InputMethodKit/InputMethodKit.h>
#import "../../ffi/tacit.h"

NS_ASSUME_NONNULL_BEGIN

/// Strict, in-memory IMKTextInput client used by ControllerTests.
@interface RecordingClient : NSProxy <IMKTextInput>

- (instancetype)initWithText:(NSString *)text;

@property(nonatomic, copy, readonly) NSString *documentText;
@property(nonatomic, readonly) NSRange nativeMarkedRange;
@property(nonatomic, readonly) NSUInteger selectionReadCount;
@property(nonatomic, readonly) NSUInteger markedRangeReadCount;
@property(nonatomic, readonly) NSUInteger substringReadCount;
@property(nonatomic, readonly) NSRange lastSubstringRange;
@property(nonatomic, readonly) NSUInteger setMarkedTextCallCount;
@property(nonatomic, readonly) NSUInteger insertTextCallCount;

/// Makes native coordinates unavailable while retaining the client's real internal caret/mark.
- (void)setCoordinatesUnavailable:(BOOL)unavailable;
- (void)resetReadCounts;
- (void)setInsertTextCallback:(void (^ _Nullable)(void))callback;

/// Simulates text inserted by the host after the input method returned false.
- (void)typeText:(NSString *)text;

@end

NS_ASSUME_NONNULL_END
