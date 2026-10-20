import Foundation

@main
struct InputTextContextTests {
    static func main() {
        testSelectedRangeInsideMarkedRange()
        testPrefixedMarkedRange()
        testCollapsedCaret()
        testSelectionOutsideMarkedRange()
        testCommittedSelectionUsesLocation()
        testInvalidMarkedRangesAreIgnored()
        testInvalidSelectedRangesAreUnavailable()
        testLongMarkedRangeIsPreserved()
        testEmojiUsesUTF16Offsets()
        testReplacementStartUsesMarkThenSelection()
        testWindowContextCarriesItsDocumentOrigin()
        testUnavailableContextRetainsKnownCaretOrUnknownOrigin()
        testDeletionUsesMarkedEndAndClamps()
        testDeletionRejectsNonpositiveCounts()
        testUnavailableAndPartialDocumentReads()
        testEmptyAndUnavailableRangesDoNotRead()
        testOwnedCompositionOverridesHostEchoesAndRetainsOrigin()
        testInactiveCompositionPreservesDocumentContext()
        testTakeMarkedTextIsSingleUseAndClearsOrigin()
        testEmptyMarkEndsCompositionAndClearsOrigin()
        testCompositionStateIsInstanceLocal()
        print("InputTextContext tests passed.")
    }

    private static func testSelectedRangeInsideMarkedRange() {
        let context = InputTextContext(
            selected: NSRange(location: 0, length: 1),
            marked: NSRange(location: 0, length: 1)
        )
        precondition(context.end == 1, "selected {0,1} inside marked {0,1} must end at the marked end")
        expectRange(context.range, NSRange(location: 0, length: 1), "selected {0,1} range")
    }

    private static func testPrefixedMarkedRange() {
        let context = InputTextContext(
            selected: NSRange(location: 5, length: 2),
            marked: NSRange(location: 3, length: 7)
        )
        precondition(context.end == 10, "selection inside a prefixed marked range must use the marked end")
        expectRange(context.range, NSRange(location: 0, length: 10), "prefixed marked range")
    }

    private static func testCollapsedCaret() {
        let context = InputTextContext(
            selected: NSRange(location: 4, length: 0),
            marked: NSRange(location: 2, length: 5)
        )
        precondition(context.end == 7, "collapsed caret inside marked text must use the marked end")
        expectRange(context.range, NSRange(location: 0, length: 7), "collapsed caret range")
    }

    private static func testSelectionOutsideMarkedRange() {
        let context = InputTextContext(
            selected: NSRange(location: 4, length: 2),
            marked: NSRange(location: 0, length: 5)
        )
        precondition(context.end == 4, "selection extending beyond the mark must use its location")
        expectRange(context.range, NSRange(location: 0, length: 4), "selection outside marked range")
    }

    private static func testCommittedSelectionUsesLocation() {
        let context = InputTextContext(
            selected: NSRange(location: 80, length: 5),
            marked: NSRange(location: 20, length: 10)
        )
        precondition(context.end == 80, "committed selection outside marked text must not use selected.end")
        expectRange(context.range, NSRange(location: 16, length: 64), "committed selection context range")
    }

    private static func testInvalidMarkedRangesAreIgnored() {
        let selected = NSRange(location: 4, length: 1)
        let invalidMarks = [
            NSRange(location: 4, length: 0),
            NSRange(location: -1, length: 2),
            NSRange(location: 4, length: -1),
            NSRange(location: NSNotFound, length: 1),
            NSRange(location: 4, length: NSNotFound),
            NSRange(location: Int.max - 1, length: 3),
            NSRange(location: NSNotFound - 1, length: 1),
        ]
        for (index, mark) in invalidMarks.enumerated() {
            let context = InputTextContext(selected: selected, marked: mark)
            precondition(context.end == 4, "invalid or empty marked range #\(index) must be ignored")
            expectRange(context.range, NSRange(location: 0, length: 4), "invalid or empty marked range #\(index)")
        }
    }

    private static func testInvalidSelectedRangesAreUnavailable() {
        let invalidSelections = [
            NSRange(location: -1, length: 0),
            NSRange(location: 0, length: -1),
            NSRange(location: NSNotFound, length: 0),
            NSRange(location: 0, length: NSNotFound),
            NSRange(location: Int.max - 1, length: 3),
            NSRange(location: NSNotFound - 1, length: 1),
        ]
        for (index, selection) in invalidSelections.enumerated() {
            let context = InputTextContext(selected: selection, marked: NSRange(location: 0, length: 10))
            precondition(context.end == nil, "invalid selected range #\(index) must have no usable end")
            precondition(context.range == nil, "invalid selected range #\(index) must have no context range")
            precondition(context.deleting(1) == nil, "invalid selected range #\(index) must not produce a deletion range")
        }
    }

    private static func testLongMarkedRangeIsPreserved() {
        let context = InputTextContext(
            selected: NSRange(location: 30, length: 1),
            marked: NSRange(location: 10, length: 100)
        )
        precondition(context.end == 110, "long active mark must use its complete end")
        expectRange(context.range, NSRange(location: 10, length: 100), "context range must expand to preserve the whole long mark")
    }

    private static func testEmojiUsesUTF16Offsets() {
        let text = "A😀B" as NSString
        let emoji = text.range(of: "😀")
        let context = InputTextContext(selected: emoji, marked: emoji)
        precondition(context.end == 3, "marked emoji end must use UTF-16 offsets")
        expectRange(context.range, NSRange(location: 0, length: 3), "emoji context range must use UTF-16 offsets")
    }

    private static func testReplacementStartUsesMarkThenSelection() {
        let activeMark = InputTextContext(
            selected: NSRange(location: 7, length: 0),
            marked: NSRange(location: 3, length: 5)
        )
        precondition(activeMark.replacementStart == 3, "native mark start must anchor replacement instead of its selection")

        let selection = InputTextContext(
            selected: NSRange(location: 12, length: 4),
            marked: NSRange(location: NSNotFound, length: 0)
        )
        precondition(selection.replacementStart == 12, "without a valid mark, replacement starts at the selection")

        let unknown = InputTextContext(
            selected: NSRange(location: NSNotFound, length: NSNotFound),
            marked: NSRange(location: NSNotFound, length: NSNotFound)
        )
        precondition(unknown.replacementStart == nil, "unknown native coordinates must not fabricate a replacement origin")
    }

    private static func testWindowContextCarriesItsDocumentOrigin() {
        let document = String(repeating: "😀", count: 40) + String(repeating: "가", count: 40)
        let documentLength = document.utf16.count
        let context = InputTextContext(
            selected: NSRange(location: documentLength, length: 0),
            marked: NSRange(location: NSNotFound, length: 0)
        )
        var requested: NSRange?
        let result = context.context { range in
            requested = range
            return (document as NSString).substring(with: range)
        }
        expectRange(requested, NSRange(location: 56, length: 64), "look-behind window in document coordinates")
        precondition(result.text == (document as NSString).substring(from: 56), "context text must match the actual trailing UTF-16 window")
        precondition(result.start == 56 && result.bridgeStart == 56, "read text must carry its actual document origin")
    }

    private static func testUnavailableContextRetainsKnownCaretOrUnknownOrigin() {
        let unavailableRead = InputTextContext(
            selected: NSRange(location: 17, length: 0),
            marked: NSRange(location: NSNotFound, length: 0)
        ).context { _ in nil }
        precondition(unavailableRead.text == nil, "missing document text must remain unavailable")
        precondition(unavailableRead.start == 17 && unavailableRead.bridgeStart == 17, "a known caret remains a valid origin without text")

        let unavailableCoordinates = InputTextContext(
            selected: NSRange(location: NSNotFound, length: NSNotFound),
            marked: NSRange(location: NSNotFound, length: NSNotFound)
        ).context { _ in "unexpected" }
        precondition(unavailableCoordinates.text == nil, "unknown coordinates cannot produce document text")
        precondition(unavailableCoordinates.start == nil && unavailableCoordinates.bridgeStart == -1, "unknown coordinates use the explicit unknown-origin sentinel")
    }

    private static func testDeletionUsesMarkedEndAndClamps() {
        let context = InputTextContext(
            selected: NSRange(location: 2, length: 1),
            marked: NSRange(location: 0, length: 6)
        )
        precondition(context.end == 6, "deletion context must use the active marked end")
        expectRange(context.deleting(2), NSRange(location: 4, length: 2), "deletion must end at the active marked end")
        expectRange(context.deleting(20), NSRange(location: 0, length: 6), "deletion count must clamp to the available prefix")

        let atStart = InputTextContext(
            selected: NSRange(location: 0, length: 0),
            marked: NSRange(location: 0, length: 0)
        )
        expectRange(atStart.deleting(1), NSRange(location: 0, length: 0), "positive deletion at the start must clamp to an empty prefix")
    }

    private static func testDeletionRejectsNonpositiveCounts() {
        let context = InputTextContext(
            selected: NSRange(location: 3, length: 0),
            marked: NSRange(location: 0, length: 0)
        )
        precondition(context.deleting(0) == nil, "zero deletion count must return nil")
        precondition(context.deleting(-1) == nil, "negative deletion count must return nil")
    }

    private static func testUnavailableAndPartialDocumentReads() {
        let context = InputTextContext(
            selected: NSRange(location: 0, length: 1),
            marked: NSRange(location: 0, length: 1)
        )
        precondition(context.read { _ in nil } == nil, "missing substring must not become an empty document")
        precondition(context.read { _ in "" } == nil, "a truncated marked-text read is unavailable context")
        precondition(context.read { _ in "ab" } == nil, "a read outside the requested extent is unavailable context")
        precondition(context.read { _ in "d" } == "d", "a complete marked-text read stays authoritative")

        let emoji = InputTextContext(
            selected: NSRange(location: 0, length: 2),
            marked: NSRange(location: 0, length: 2)
        )
        precondition(emoji.read { _ in "😀" } == "😀", "read validation uses UTF-16, not grapheme count")
    }

    private static func testEmptyAndUnavailableRangesDoNotRead() {
        let unavailable = NSRange(location: NSNotFound, length: NSNotFound)
        let empty = InputTextContext(selected: NSRange(location: 0, length: 0), marked: unavailable)
        let unknown = InputTextContext(selected: unavailable, marked: unavailable)
        var reads = 0
        let result = empty.read { _ in reads += 1; return "unexpected" }
        precondition(result == "" && reads == 0, "known empty context stays distinct without a substring query")
        let missing = unknown.read { _ in reads += 1; return "unexpected" }
        precondition(missing == nil && reads == 0, "unavailable coordinates must not issue a document query")
    }

    private static func testOwnedCompositionOverridesHostEchoesAndRetainsOrigin() {
        var composition = InputComposition()
        composition.mark("に", at: 128)
        composition.mark("か", at: composition.start)
        var reads = 0
        let echo = composition.context {
            reads += 1
            return InputContext(text: "wrong host echo", start: 0)
        }
        precondition(echo.text == nil && echo.start == 128 && echo.bridgeStart == 128, "owned mark must keep its first trusted origin and suppress host echoes")
        precondition(reads == 0, "owned composition must not invoke the document reader")

        var unknownComposition = InputComposition()
        unknownComposition.mark("unknown", at: nil)
        unknownComposition.mark("updated", at: unknownComposition.start)
        let unknown = unknownComposition.context { reads += 1; return InputContext(text: "stale", start: 0) }
        precondition(unknown.text == nil && unknown.start == nil && unknown.bridgeStart == -1, "an unknown first mark origin must remain unknown")
        precondition(reads == 0, "updating an owned mark must still skip host reads")
    }

    private static func testInactiveCompositionPreservesDocumentContext() {
        let composition = InputComposition()
        var reads = 0
        let unavailable = composition.context {
            reads += 1
            return InputContext(text: nil, start: 23)
        }
        precondition(unavailable.text == nil && unavailable.start == 23 && reads == 1, "inactive composition must preserve known-origin unavailable context")

        let committed = composition.context {
            reads += 1
            return InputContext(text: "committed context", start: 19)
        }
        precondition(committed.text == "committed context" && committed.start == 19 && reads == 2, "inactive composition must preserve document context and origin exactly")
    }

    private static func testTakeMarkedTextIsSingleUseAndClearsOrigin() {
        var composition = InputComposition()
        composition.mark("k", at: 3)
        composition.mark("😀かな", at: 3)
        let firstCommit = composition.takeMarkedText()
        precondition(firstCommit == "😀かな" && composition.markedText == nil && composition.start == nil, "taking the latest Unicode mark must clear owned text and origin atomically")

        precondition(composition.takeMarkedText() == nil, "taking the same native mark again must not produce a duplicate commit")
        var reads = 0
        let committedContext = composition.context {
            reads += 1
            return InputContext(text: "😀かな", start: 42)
        }
        precondition(committedContext.text == "😀かな" && committedContext.start == 42 && reads == 1, "after take, the next context must come from the document reader")
    }

    private static func testEmptyMarkEndsCompositionAndClearsOrigin() {
        var composition = InputComposition()
        composition.mark("owned text", at: 7)
        composition.mark("", at: 7)
        precondition(composition.markedText == nil && composition.start == nil, "an empty mark update must clear owned text and origin")

        var reads = 0
        let context = composition.context {
            reads += 1
            return InputContext(text: "document context", start: 31)
        }
        precondition(context.text == "document context" && context.start == 31 && reads == 1, "after an empty mark update, document context must be read normally")
    }

    private static func testCompositionStateIsInstanceLocal() {
        var active = InputComposition()
        var inactive = InputComposition()
        active.mark("active mark", at: 70)
        inactive.reset()

        var activeReads = 0
        let activeContext = active.context {
            activeReads += 1
            return InputContext(text: "untrusted echo", start: 0)
        }
        precondition(activeContext.text == nil && activeContext.start == 70 && activeReads == 0, "resetting another composition must not clear an active instance's mark or origin")

        var inactiveReads = 0
        let inactiveContext = inactive.context {
            inactiveReads += 1
            return InputContext(text: "inactive document", start: 8)
        }
        precondition(inactiveContext.text == "inactive document" && inactiveContext.start == 8 && inactiveReads == 1, "an active composition must not leak into another instance")

        active.reset()
        precondition(active.markedText == nil && active.start == nil, "reset must clear both owned text and its origin")
        let resetContext = active.context {
            activeReads += 1
            return InputContext(text: "document after reset", start: 9)
        }
        precondition(resetContext.text == "document after reset" && resetContext.start == 9 && activeReads == 1, "the reset instance must resume reading document context")
    }

    private static func expectRange(_ actual: NSRange?, _ expected: NSRange, _ message: String) {
        let actualDescription = actual.map { NSStringFromRange($0) } ?? "nil"
        precondition(actual == expected, "\(message): expected \(NSStringFromRange(expected)), got \(actualDescription)")
    }
}
