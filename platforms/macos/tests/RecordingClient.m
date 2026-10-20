#import "RecordingClient.h"

#import <objc/runtime.h>
#include <stdio.h>
#include <stdlib.h>

static NSRange RecordingNoRange(void) {
    return NSMakeRange(NSNotFound, NSNotFound);
}

static NSString *PlainText(id value) {
    if ([value isKindOfClass:[NSAttributedString class]]) {
        return [(NSAttributedString *)value string];
    }
    if ([value isKindOfClass:[NSString class]]) {
        return value;
    }
    fprintf(stderr, "RecordingClient received unsupported text value of class %s\n", object_getClassName(value));
    abort();
}

static void ValidateRange(NSRange range, NSUInteger length) {
    if (range.location > length || range.length > length - range.location) {
        fprintf(stderr, "RecordingClient received an out-of-bounds range\n");
        abort();
    }
}

@implementation RecordingClient {
    NSMutableString *_text;
    NSRange _nativeMarkedRange;
    NSRange _nativeSelection;
    NSRange _lastSubstringRange;
    NSUInteger _selectionReadCount;
    NSUInteger _markedRangeReadCount;
    NSUInteger _substringReadCount;
    NSUInteger _setMarkedTextCallCount;
    NSUInteger _insertTextCallCount;
    BOOL _coordinatesUnavailable;
    BOOL _insideInsertCallback;
    void (^_insertTextCallback)(void);
}

- (instancetype)initWithText:(NSString *)text {
    _text = [text mutableCopy];
    _nativeMarkedRange = RecordingNoRange();
    _nativeSelection = NSMakeRange(_text.length, 0);
    _lastSubstringRange = RecordingNoRange();
    return self;
}

- (BOOL)conformsToProtocol:(Protocol *)protocol {
    return protocol == @protocol(IMKTextInput) || [super conformsToProtocol:protocol];
}

- (BOOL)respondsToSelector:(SEL)selector {
    if (class_getInstanceMethod([RecordingClient class], selector) != NULL) {
        return YES;
    }
    struct objc_method_description required = protocol_getMethodDescription(@protocol(IMKTextInput), selector, YES, YES);
    struct objc_method_description optional = protocol_getMethodDescription(@protocol(IMKTextInput), selector, NO, YES);
    return required.name != NULL || optional.name != NULL;
}

- (NSMethodSignature *)methodSignatureForSelector:(SEL)selector {
    fprintf(stderr, "RecordingClient received unsupported invocation: %s\n", sel_getName(selector));
    abort();
}

- (void)forwardInvocation:(NSInvocation *)invocation {
    fprintf(stderr, "RecordingClient received unsupported invocation: %s\n", sel_getName(invocation.selector));
    abort();
}

- (NSString *)documentText {
    return [_text copy];
}

- (NSRange)nativeMarkedRange {
    return _nativeMarkedRange;
}

- (NSUInteger)selectionReadCount {
    return _selectionReadCount;
}

- (NSUInteger)markedRangeReadCount {
    return _markedRangeReadCount;
}

- (NSUInteger)substringReadCount {
    return _substringReadCount;
}

- (NSRange)lastSubstringRange {
    return _lastSubstringRange;
}

- (NSUInteger)setMarkedTextCallCount {
    return _setMarkedTextCallCount;
}

- (NSUInteger)insertTextCallCount {
    return _insertTextCallCount;
}

- (void)setCoordinatesUnavailable:(BOOL)unavailable {
    _coordinatesUnavailable = unavailable;
}

- (void)resetReadCounts {
    _selectionReadCount = 0;
    _markedRangeReadCount = 0;
    _substringReadCount = 0;
    _lastSubstringRange = RecordingNoRange();
}

- (void)setInsertTextCallback:(void (^)(void))callback {
    _insertTextCallback = [callback copy];
}

// This client is in-process, not an NSDistantObject with an IPC connection.
- (id)connectionForProxy {
    return nil;
}

- (NSString *)bundleIdentifier {
    return @"org.tacit.tests.client";
}

- (NSRange)selectedRange {
    _selectionReadCount += 1;
    if (_coordinatesUnavailable) {
        return RecordingNoRange();
    }
    if (_nativeMarkedRange.location != NSNotFound) {
        NSUInteger relative = _nativeSelection.location >= _nativeMarkedRange.location
            ? _nativeSelection.location - _nativeMarkedRange.location
            : 0;
        return NSMakeRange(relative, _nativeSelection.length);
    }
    return _nativeSelection;
}

- (NSRange)markedRange {
    _markedRangeReadCount += 1;
    if (_coordinatesUnavailable || _nativeMarkedRange.location == NSNotFound) {
        return RecordingNoRange();
    }
    return NSMakeRange(0, _nativeMarkedRange.length);
}

- (BOOL)supportsProperty:(TSMDocumentPropertyTag)property {
    (void)property;
    return YES;
}

- (NSAttributedString *)attributedSubstringFromRange:(NSRange)range {
    _substringReadCount += 1;
    _lastSubstringRange = range;
    if (_coordinatesUnavailable) {
        return nil;
    }
    if (_nativeMarkedRange.location != NSNotFound) {
        return [[NSAttributedString alloc] initWithString:@"wrong host echo"];
    }
    if (range.location > _text.length || range.length > _text.length - range.location) {
        return nil;
    }
    NSString *substring = [_text substringWithRange:range];
    return [[NSAttributedString alloc] initWithString:substring];
}

- (void)setMarkedText:(id)value selectionRange:(NSRange)selectionRange replacementRange:(NSRange)replacementRange {
    _setMarkedTextCallCount += 1;
    NSString *text = PlainText(value);
    NSRange target = replacementRange.location != NSNotFound ? replacementRange :
        (_nativeMarkedRange.location != NSNotFound ? _nativeMarkedRange : _nativeSelection);
    ValidateRange(target, _text.length);
    [_text replaceCharactersInRange:target withString:text];
    if (text.length == 0) {
        _nativeMarkedRange = RecordingNoRange();
    } else {
        _nativeMarkedRange = NSMakeRange(target.location, text.length);
    }
    ValidateRange(selectionRange, text.length);
    _nativeSelection = NSMakeRange(target.location + selectionRange.location, selectionRange.length);
}

- (void)insertText:(id)value replacementRange:(NSRange)replacementRange {
    NSString *text = PlainText(value);
    NSRange target;
    if (replacementRange.location != NSNotFound) {
        target = replacementRange;
    } else {
        target = _nativeMarkedRange.location != NSNotFound ? _nativeMarkedRange : _nativeSelection;
    }
    ValidateRange(target, _text.length);
    [_text replaceCharactersInRange:target withString:text];
    _nativeMarkedRange = RecordingNoRange();
    _nativeSelection = NSMakeRange(target.location + text.length, 0);
    _insertTextCallCount += 1;

    if (_insertTextCallback != nil && !_insideInsertCallback) {
        _insideInsertCallback = YES;
        _insertTextCallback();
        _insideInsertCallback = NO;
    }
}

- (void)typeText:(NSString *)text {
    NSRange target = _nativeMarkedRange.location != NSNotFound ? _nativeMarkedRange : _nativeSelection;
    ValidateRange(target, _text.length);
    [_text replaceCharactersInRange:target withString:text];
    _nativeMarkedRange = RecordingNoRange();
    _nativeSelection = NSMakeRange(target.location + text.length, 0);
}

@end
