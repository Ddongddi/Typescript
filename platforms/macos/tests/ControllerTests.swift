import Cocoa
import InputMethodKit

private final class TestTacitController: TacitController {
    var simulatedSecureInputEnabled = false

    override func secureInputEnabled() -> Bool {
        simulatedSecureInputEnabled
    }
}

@main
@MainActor
struct ControllerTests {
    private static let romanKeys = "dkssudgktpdy"
    private static let koreanText = "안녕하세요"

    static func main() {
        _ = NSApplication.shared
        testPrefixedTypingSpaceAndPostCommitCorrection()
        testOwnedMarkSkipsHostEchoAndRangeReads()
        testIndependentControllerLifecycles()
        testCommitIsSingleUseUnderReentrancy()
        testUnknownCoordinatesCannotAuthorizeReplacement()
        testSecureInputDoesNotReadOrConsume()
        print("Controller tests passed.")
    }

    private static func testPrefixedTypingSpaceAndPostCommitCorrection() {
        let prefix = String(repeating: "😀", count: 40) + String(repeating: "가", count: 40)
        let prefixLength = prefix.utf16.count
        let client = RecordingClient(text: prefix)
        let controller = makeController(client)
        controller.activateServer(client)

        for (index, key) in romanKeys.enumerated() {
            let consumed = press(String(key), keyCode: 0, controller: controller, client: client)
            precondition(consumed, "letter \(key) at index \(index) must be consumed while composing")
            if index == 0 {
                expectRange(client.lastSubstringRange, NSRange(location: prefixLength - 64, length: 64), "initial context window must report its true UTF-16 origin")
            }
        }
        expectMark(client, at: prefixLength, text: koreanText, "Korean preview after prefixed typing")
        precondition(client.documentText == prefix + koreanText, "native marked preview must replace text at the document caret")

        let consumedSpace = press(" ", keyCode: 49, controller: controller, client: client)
        precondition(!consumedSpace, "the host must receive Space after the input method commits the word")
        precondition(client.insertTextCallCount == 1, "the input method commits only its word before host Space insertion")
        client.typeText(" ")
        precondition(client.documentText == prefix + koreanText + " ", "the host inserts the unconsumed Space exactly once")
        precondition(client.nativeMarkedRange.location == NSNotFound, "Space must end the native mark")

        let documentBeforeCorrection = client.documentText
        let documentLength = documentBeforeCorrection.utf16.count
        let corrected = press("\u{1b}", keyCode: 53, controller: controller, client: client)
        precondition(corrected, "Esc must flip the just-committed word when its document origin is trusted")
        expectRange(client.lastSubstringRange, NSRange(location: documentLength - 64, length: 64), "post-commit context must reread the true trailing document window")
        precondition(client.documentText == prefix + romanKeys + " ", "post-commit Esc must replace Korean with the original raw keys and retain the host Space")
        precondition(client.nativeMarkedRange.location == NSNotFound, "post-commit correction must leave no native mark")
    }

    private static func testOwnedMarkSkipsHostEchoAndRangeReads() {
        let prefix = "existing text:"
        let client = RecordingClient(text: prefix)
        let controller = makeController(client)
        controller.activateServer(client)

        let first = press(String(romanKeys.first!), keyCode: 0, controller: controller, client: client)
        precondition(first, "first composition key must be consumed")
        let selectionReads = client.selectionReadCount
        let markReads = client.markedRangeReadCount
        let substringReads = client.substringReadCount
        precondition(substringReads == 1, "the first key may read the document once before this controller owns a mark")

        for key in romanKeys.dropFirst() {
            precondition(press(String(key), keyCode: 0, controller: controller, client: client), "owned composition key must be consumed")
            precondition(client.selectionReadCount == selectionReads, "owned composition must not read host selection, even when it lies about relative coordinates")
            precondition(client.markedRangeReadCount == markReads, "owned composition must not read host mark range")
            precondition(client.substringReadCount == substringReads, "owned composition must not read the host's wrong text echo")
        }
        expectMark(client, at: prefix.utf16.count, text: koreanText, "owned mark must retain its absolute native range")
        precondition(client.documentText == prefix + koreanText, "host echo behavior must not disturb the native marked document")
    }

    private static func testIndependentControllerLifecycles() {
        let clientA = RecordingClient(text: "A:")
        let clientB = RecordingClient(text: "B:")
        let controllerA = makeController(clientA)
        let controllerB = makeController(clientB)
        controllerA.activateServer(clientA)
        precondition(press("d", keyCode: 0, controller: controllerA, client: clientA), "controller A should start its own mark")
        let markA = clientA.nativeMarkedRange
        precondition(markA.location != NSNotFound, "controller A must produce an actual native mark")
        let textA = clientA.documentText
        controllerB.activateServer(clientB)
        expectRange(clientA.nativeMarkedRange, markA, "activating B must not clear A's native mark")
        precondition(clientA.documentText == textA && clientA.insertTextCallCount == 0, "activating B must not commit or mutate A")
        precondition(press("g", keyCode: 0, controller: controllerB, client: clientB), "controller B should start an independent mark")

        let markB = clientB.nativeMarkedRange
        precondition(markB.location != NSNotFound, "controller B must produce an actual native mark")
        let textB = clientB.documentText
        for key in "kssud" {
            precondition(press(String(key), keyCode: 0, controller: controllerA, client: clientA), "A must continue its engine state after B activates")
        }
        expectMark(clientA, at: 2, text: "안녕", "A's interrupted dkssud sequence must remain intact")
        precondition(clientA.documentText == "A:안녕", "B activation must not reset or contaminate A's engine")
        expectRange(clientB.nativeMarkedRange, markB, "continuing A must not modify B's native mark")
        precondition(clientB.documentText == textB && clientB.insertTextCallCount == 0, "continuing A must not alter B")

        controllerA.deactivateServer(clientA)
        precondition(clientA.nativeMarkedRange.location == NSNotFound, "deactivating A must commit and clear A's mark")
        precondition(clientA.insertTextCallCount == 1, "deactivating A commits its mark once")
        expectRange(clientB.nativeMarkedRange, markB, "deactivating A must not clear B's native mark")
        precondition(clientB.documentText == textB && clientB.insertTextCallCount == 0, "deactivating A must not commit or mutate B")

        for key in "ksrnr" {
            precondition(press(String(key), keyCode: 0, controller: controllerB, client: clientB), "B must continue its engine state after A deactivates")
        }
        expectMark(clientB, at: 2, text: "한국", "B's interrupted gksrnr sequence must remain intact")
        precondition(clientB.documentText == "B:한국", "A deactivation must not reset or contaminate B's engine")
        precondition(clientA.documentText == "A:안녕" && clientA.insertTextCallCount == 1, "continuing B must not alter A's committed text")

        controllerB.deactivateServer(clientB)
        precondition(clientB.nativeMarkedRange.location == NSNotFound, "deactivating B must commit and clear B's own mark")
        precondition(clientB.insertTextCallCount == 1, "deactivating B commits its mark once")
    }

    private static func testCommitIsSingleUseUnderReentrancy() {
        let prefix = "commit:"
        let client = RecordingClient(text: prefix)
        let controller = makeController(client)
        controller.activateServer(client)
        for key in romanKeys {
            precondition(press(String(key), keyCode: 0, controller: controller, client: client), "composition key must be consumed before commit")
        }
        let marked = nativeMarkedText(client)
        precondition(marked == koreanText, "commit fixture must hold the expected Korean native mark")

        var reentrantCallbacks = 0
        client.setInsertTextCallback {
            reentrantCallbacks += 1
            controller.commitComposition(client)
        }
        controller.commitComposition(client)
        client.setInsertTextCallback(nil)

        precondition(reentrantCallbacks == 1, "the native insert callback must reenter commit exactly once")
        precondition(client.insertTextCallCount == 1, "clearing owned composition before insert must prevent a duplicate native commit")
        precondition(client.nativeMarkedRange.location == NSNotFound, "commit must actually clear the client's native mark")
        precondition(client.documentText == prefix + marked, "reentrant commit must preserve exactly one committed copy")
    }

    private static func testUnknownCoordinatesCannotAuthorizeReplacement() {
        let prefix = String(repeating: "기록", count: 24)
        let client = RecordingClient(text: prefix)
        client.setCoordinatesUnavailable(true)
        let controller = makeController(client)
        controller.activateServer(client)

        for key in romanKeys {
            precondition(press(String(key), keyCode: 0, controller: controller, client: client), "keys must still compose when host document coordinates are unavailable")
        }
        let mark = nativeMarkedText(client)
        precondition(mark == koreanText, "unknown-coordinate fixture must compose the Korean word")
        precondition(client.nativeMarkedRange.location != NSNotFound, "native client retains its internal mark despite hiding coordinates")

        precondition(!press(" ", keyCode: 49, controller: controller, client: client), "host must receive Space after commit")
        client.typeText(" ")
        let committed = prefix + koreanText + " "
        precondition(client.documentText == committed, "unknown-coordinate commit and host Space must preserve the document")
        precondition(client.insertTextCallCount == 1, "Space commits the mark once before unknown-coordinate correction is attempted")

        let corrected = press("\u{1b}", keyCode: 53, controller: controller, client: client)
        precondition(!corrected, "unknown document coordinates must not authorize a post-commit range replacement")
        precondition(client.documentText == committed, "unknown coordinates must leave the committed document untouched")
        precondition(client.insertTextCallCount == 1 && client.nativeMarkedRange.location == NSNotFound, "unknown-origin Esc must not issue a replacement or recreate a mark")
        precondition(client.substringReadCount == 0, "unavailable coordinates must never trigger a substring read")
    }

    private static func testSecureInputDoesNotReadOrConsume() {
        let client = RecordingClient(text: "secure field")
        let controller = makeController(client)
        controller.activateServer(client)
        client.resetReadCounts()
        controller.simulatedSecureInputEnabled = true

        let consumed = press("d", keyCode: 0, controller: controller, client: client)
        precondition(!consumed, "secure input must pass the key through without engine consumption")
        precondition(client.selectionReadCount == 0 && client.markedRangeReadCount == 0 && client.substringReadCount == 0, "secure input must not query any host text context")
        precondition(client.documentText == "secure field" && client.insertTextCallCount == 0 && client.setMarkedTextCallCount == 0, "secure input must not alter text or set a mark")

        controller.simulatedSecureInputEnabled = false
        precondition(press("d", keyCode: 0, controller: controller, client: client), "controlled non-secure input must resume normal engine handling")
        precondition(client.nativeMarkedRange.location == "secure field".utf16.count, "non-secure handling must create a native mark at the caret")
    }

    private static func makeController(_ client: RecordingClient) -> TestTacitController {
        guard let controller = TestTacitController(server: nil, delegate: nil, client: client) else {
            preconditionFailure("Failed to initialize isolated IMKInputController")
        }
        return controller
    }

    private static func press(_ characters: String, keyCode: UInt16, controller: TestTacitController, client: RecordingClient) -> Bool {
        guard let event = NSEvent.keyEvent(
            with: .keyDown,
            location: .zero,
            modifierFlags: [],
            timestamp: 0,
            windowNumber: 0,
            context: nil,
            characters: characters,
            charactersIgnoringModifiers: characters,
            isARepeat: false,
            keyCode: keyCode
        ) else {
            preconditionFailure("Failed to construct synthetic key event for \(String(reflecting: characters))")
        }
        return controller.handle(event, client: client)
    }

    private static func nativeMarkedText(_ client: RecordingClient) -> String {
        let range = client.nativeMarkedRange
        guard range.location != NSNotFound,
              range.location <= (client.documentText as NSString).length,
              range.length <= (client.documentText as NSString).length - range.location else {
            preconditionFailure("Expected a valid native marked range, got \(NSStringFromRange(range))")
        }
        return (client.documentText as NSString).substring(with: range)
    }

    private static func expectMark(_ client: RecordingClient, at location: Int, text: String, _ message: String) {
        let range = client.nativeMarkedRange
        precondition(range.location == location, "\(message): expected start \(location), got \(NSStringFromRange(range))")
        precondition(nativeMarkedText(client) == text, "\(message): expected \(String(reflecting: text)), got \(String(reflecting: nativeMarkedText(client)))")
    }

    private static func expectRange(_ actual: NSRange, _ expected: NSRange, _ message: String) {
        precondition(actual == expected, "\(message): expected \(NSStringFromRange(expected)), got \(NSStringFromRange(actual))")
    }
}
