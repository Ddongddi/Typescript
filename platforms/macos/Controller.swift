import Cocoa
import InputMethodKit
import Carbon.HIToolbox

/// Key kinds, as in tacit.h.
private enum Kind: Int32 {
    case letter = 0, printable, delete, escape, cycle, enter, tab, other
}

private let noRange = NSRange(location: NSNotFound, length: NSNotFound)
/// How much text before the caret the engine gets to see (UTF-16 units).
private let lookBehind = 64

@objc(TacitController)
class TacitController: IMKInputController {
    override func activateServer(_ sender: Any!) {
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
        guard let ime = ime else { return }
        ts_reset(ime)
    }

    override func recognizedEvents(_ sender: Any!) -> Int {
        return Int(NSEvent.EventTypeMask.keyDown.rawValue)
    }

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let ime = ime, let event = event, event.type == .keyDown,
              let client = sender as? IMKTextInput else { return false }
        // A password field turns on secure event input: nothing typed there is read, counted or learned.
        if IsSecureEventInputEnabled() {
            ts_reset(ime)
            return false
        }
        let before = textBeforeCaret(client)
        // Option+Space pauses tacit in this text field (macOS has no 한/영 key to catch).
        if event.modifierFlags.contains(.option), event.keyCode == 49 {
            return run(ts_toggle_pause(ime, before), client)
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
        return run(ts_key(ime, kind.rawValue, scalar, before), client)
    }

    /// The text before the caret, ending with the word being composed (which is marked text).
    private func textBeforeCaret(_ client: IMKTextInput) -> String {
        let caret = client.selectedRange()
        guard caret.location != NSNotFound, caret.location > 0 else { return "" }
        let start = max(0, caret.location - lookBehind)
        let range = NSRange(location: start, length: caret.location - start)
        return client.attributedSubstring(from: range)?.string ?? ""
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
                client.setMarkedText(text, selectionRange: NSRange(location: text.utf16.count, length: 0),
                                     replacementRange: noRange)
            case "B":
                deleting = Int(text) ?? 0
            case "I":
                var range = noRange
                if deleting > 0 {
                    let caret = client.selectedRange().location
                    if caret != NSNotFound { range = NSRange(location: max(0, caret - deleting), length: deleting) }
                    deleting = 0
                }
                client.insertText(text, replacementRange: range)
            default:
                break
            }
        }
        return consumed
    }
}
