import Foundation

/// The start is a document-relative UTF-16 offset, or nil when its identity is unknown.
struct InputContext {
    let text: String?
    let start: Int?

    var bridgeStart: Int64 { start.map(Int64.init) ?? -1 }
}

/// The adapter owns the marked text it sends; clients need not echo it back consistently.
struct InputComposition {
    private(set) var markedText: String?
    private(set) var start: Int?

    mutating func mark(_ text: String, at start: Int?) {
        markedText = text.isEmpty ? nil : text
        self.start = text.isEmpty ? nil : start
    }

    mutating func reset() {
        markedText = nil
        start = nil
    }

    mutating func takeMarkedText() -> String? {
        let text = markedText
        reset()
        return text
    }

    func context(_ readDocument: () -> InputContext) -> InputContext {
        // The core already owns this composition. Reading its echo can produce stale text,
        // unrelated text, or composition-relative selection coordinates in desktop clients.
        guard markedText == nil else { return InputContext(text: nil, start: start) }
        return readDocument()
    }
}

/// Document-relative context ending with the entire active composition, not its selection start.
struct InputTextContext {
    private let selected: NSRange
    private let marked: NSRange

    init(selected: NSRange, marked: NSRange) {
        self.selected = selected
        self.marked = marked
    }

    private func isValid(_ range: NSRange) -> Bool {
        range.location >= 0 && range.location < NSNotFound &&
            range.length >= 0 && range.length < NSNotFound - range.location
    }

    private var activeMark: NSRange? {
        guard isValid(selected), isValid(marked), marked.length > 0,
              selected.location >= marked.location,
              NSMaxRange(selected) <= NSMaxRange(marked) else { return nil }
        return marked
    }

    /// Native setMarkedText replaces the existing mark, or the current selection.
    var replacementStart: Int? {
        if isValid(marked), marked.length > 0 { return marked.location }
        return isValid(selected) ? selected.location : nil
    }

    var end: Int? {
        guard isValid(selected) else { return nil }
        return activeMark.map { NSMaxRange($0) } ?? selected.location
    }

    var range: NSRange? {
        guard let end = end else { return nil }
        var start = max(0, end - 64)
        // Preserve a containing native mark even beyond the usual look-behind.
        if let marked = activeMark { start = min(start, marked.location) }
        return NSRange(location: start, length: end - start)
    }

    /// Missing or partial document reads are unavailable context, never an empty document.
    func read(_ substring: (NSRange) -> String?) -> String? {
        guard let range = range else { return nil }
        if range.length == 0 { return "" }
        guard let text = substring(range), text.utf16.count == range.length else { return nil }
        return text
    }

    func context(_ substring: (NSRange) -> String?) -> InputContext {
        let text = read(substring)
        // With no external text, the bridge's empty composition starts at the caret.
        return InputContext(text: text, start: text == nil ? end : range?.location)
    }

    func deleting(_ count: Int) -> NSRange? {
        guard count > 0, let end = end else { return nil }
        let length = min(count, end)
        return NSRange(location: end - length, length: length)
    }
}
