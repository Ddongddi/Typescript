import Cocoa
import InputMethodKit
import Carbon.HIToolbox

/// Key kinds, as in tacit.h.
private enum Kind: Int32 {
    case letter = 0, printable, delete, escape, cycle, enter, tab, other
}

private let noRange = NSRange(location: NSNotFound, length: NSNotFound)

@objc(TacitController)
class TacitController: IMKInputController {
    // IMK creates a controller per client session. An inactive client's callbacks must not
    // reset the engine or marked text belonging to another client.
    private let ime: OpaquePointer? = ts_new(nil)
    private var composition = InputComposition()

    deinit {
        if let ime = ime { ts_free(ime) }
    }

    override func activateServer(_ sender: Any!) {
        composition.reset()
        guard let ime = ime else { return }
        let client = sender as? IMKTextInput
        // The rule key of an app is the last part of its bundle id, lowercase ("com.apple.Terminal" -> "terminal").
        let app = client?.bundleIdentifier().map { String($0.split(separator: ".").last ?? "").lowercased() }
        ts_activate(ime, app)
    }

    override func deactivateServer(_ sender: Any!) {
        commitComposition(sender)
    }

    override func commitComposition(_ sender: Any!) {
        // Clear before calling the client: insertText may cause a reentrant commit callback.
        if let text = composition.takeMarkedText(),
           let input = (sender as? IMKTextInput) ?? self.client() {
            input.insertText(text, replacementRange: noRange)
        }
        if let ime = ime { ts_reset(ime) }
    }

    override func recognizedEvents(_ sender: Any!) -> Int {
        return Int(NSEvent.EventTypeMask.keyDown.rawValue)
    }

    func secureInputEnabled() -> Bool {
        IsSecureEventInputEnabled()
    }

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let ime = ime, let event = event, event.type == .keyDown,
              let client = sender as? IMKTextInput else { return false }
        // A password field turns on secure event input: nothing typed there is read, counted or learned.
        if secureInputEnabled() {
            composition.reset()
            ts_reset(ime)
            return false
        }
        let before = textBeforeCaret(client)
        // Option+Space pauses tacit in this text field (macOS has no 한/영 key to catch).
        if event.modifierFlags.contains(.option), event.keyCode == 49 {
            return run(ts_toggle_pause(ime, before.text, before.bridgeStart), client)
        }
        var scalar: UInt32 = 0
        var kind = Kind.other
        let mods = event.modifierFlags.intersection([.command, .control, .option])
        if mods.isEmpty {
            switch event.keyCode {
            case 51: kind = .delete
            case 53: kind = event.modifierFlags.contains(.shift) ? .cycle : .escape
            case 36, 76: kind = .enter
            case 48: kind = .tab
            default:
                if let c = event.characters?.unicodeScalars.first, c.value >= 0x20, c.value <= 0x7e {
                    scalar = c.value
                    let isLetter = (c.value | 0x20) >= 0x61 && (c.value | 0x20) <= 0x7a
                    kind = isLetter ? .letter : .printable
                }
            }
        }
        return run(ts_key(ime, kind.rawValue, scalar, before.text, before.bridgeStart), client)
    }

    /// Optional document context; the core supplies any composition owned by this controller.
    private func textBeforeCaret(_ client: IMKTextInput) -> InputContext {
        return composition.context {
            // Document access is optional. NULL context tells the bridge to use its own
            // composition; an empty string would falsely indicate a move to an empty field.
            guard client.supportsProperty(UInt32(kTSMDocumentSupportDocumentAccessPropertyTag)) else {
                return InputContext(text: nil, start: nil)
            }
            let context = InputTextContext(selected: client.selectedRange(), marked: client.markedRange())
            return context.context { client.attributedSubstring(from: $0)?.string }
        }
    }

    /// Plays the commands the engine returned and says whether it consumed the key.
    private func run(_ reply: UnsafeMutablePointer<CChar>?, _ client: IMKTextInput) -> Bool {
        guard let reply = reply else { return false }
        defer { ts_string_free(reply) }
        let out = String(cString: reply)
        let consumed = out.hasPrefix("1")
        var deleting = 0
        for op in out.dropFirst().split(separator: "\u{1}", omittingEmptySubsequences: true) {
            let text = String(op.dropFirst())
            switch op.first {
            case "M":
                let start: Int?
                if composition.markedText != nil {
                    start = composition.start
                } else if client.supportsProperty(UInt32(kTSMDocumentSupportDocumentAccessPropertyTag)) {
                    start = InputTextContext(selected: client.selectedRange(), marked: client.markedRange()).replacementStart
                } else {
                    start = nil
                }
                composition.mark(text, at: start)
                client.setMarkedText(text, selectionRange: NSRange(location: text.utf16.count, length: 0),
                                     replacementRange: noRange)
            case "B":
                deleting = Int(text) ?? 0
            case "I":
                var range = noRange
                if deleting > 0 {
                    let context = InputTextContext(selected: client.selectedRange(), marked: client.markedRange())
                    range = context.deleting(deleting) ?? noRange
                    deleting = 0
                }
                composition.reset()
                client.insertText(text, replacementRange: range)
            default:
                break
            }
        }
        return consumed
    }
}
