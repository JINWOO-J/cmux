import AppKit
import Combine

/// Entry in the clipboard history.
struct ClipboardHistoryEntry: Identifiable, Codable, Equatable {
    let id: UUID
    let text: String
    let timestamp: Date

    /// A single-line preview of the clipboard content (first line, truncated).
    var preview: String {
        let firstLine = text.split(separator: "\n", maxSplits: 1, omittingEmptySubsequences: false).first.map(String.init) ?? text
        if firstLine.count > 200 {
            return String(firstLine.prefix(200)) + "…"
        }
        return firstLine
    }

    /// Returns the byte-size description of the entry.
    var sizeLabel: String {
        let bytes = text.utf8.count
        if bytes < 1024 { return "\(bytes) B" }
        if bytes < 1024 * 1024 { return "\(bytes / 1024) KB" }
        return String(format: "%.1f MB", Double(bytes) / (1024 * 1024))
    }
}

/// Monitors the system pasteboard and maintains a searchable clipboard history.
@MainActor
final class ClipboardHistoryStore: ObservableObject {
    static let shared = ClipboardHistoryStore()

    @Published private(set) var entries: [ClipboardHistoryEntry] = []

    private let maxEntries = 100
    private var changeCount: Int = 0
    private var pollTimer: Timer?
    private let storageURL: URL

    private init() {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let appDir = appSupport.appendingPathComponent("term-mesh", isDirectory: true)
        try? FileManager.default.createDirectory(at: appDir, withIntermediateDirectories: true)
        storageURL = appDir.appendingPathComponent("clipboard-history.json")
        loadFromDisk()
        changeCount = NSPasteboard.general.changeCount
        startPolling()
    }

    deinit {
        pollTimer?.invalidate()
    }

    // MARK: - Public

    func search(query: String) -> [ClipboardHistoryEntry] {
        guard !query.isEmpty else { return entries }
        let lowered = query.lowercased()
        return entries.filter { fuzzyMatch(text: $0.text.lowercased(), query: lowered) }
    }

    func paste(entry: ClipboardHistoryEntry) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(entry.text, forType: .string)
        // Update changeCount so our poller doesn't re-record this as a new entry
        changeCount = pasteboard.changeCount

        // Move to top
        if let index = entries.firstIndex(where: { $0.id == entry.id }) {
            let item = entries.remove(at: index)
            entries.insert(item, at: 0)
            saveToDisk()
        }
    }

    func delete(entry: ClipboardHistoryEntry) {
        entries.removeAll { $0.id == entry.id }
        saveToDisk()
    }

    func clearAll() {
        entries.removeAll()
        saveToDisk()
    }

    // MARK: - Polling

    private func startPolling() {
        pollTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.checkPasteboard()
            }
        }
    }

    private func checkPasteboard() {
        let pasteboard = NSPasteboard.general
        let currentCount = pasteboard.changeCount
        guard currentCount != changeCount else { return }
        changeCount = currentCount

        guard let text = pasteboard.string(forType: .string), !text.isEmpty else { return }

        // Deduplicate: remove previous identical entry
        entries.removeAll { $0.text == text }

        let entry = ClipboardHistoryEntry(id: UUID(), text: text, timestamp: Date())
        entries.insert(entry, at: 0)

        // Trim to max size
        if entries.count > maxEntries {
            entries = Array(entries.prefix(maxEntries))
        }

        saveToDisk()
    }

    // MARK: - Persistence

    private func saveToDisk() {
        do {
            let data = try JSONEncoder().encode(entries)
            try data.write(to: storageURL, options: .atomic)
        } catch {
            NSLog("ClipboardHistoryStore: failed to save: %@", error.localizedDescription)
        }
    }

    private func loadFromDisk() {
        guard FileManager.default.fileExists(atPath: storageURL.path) else { return }
        do {
            let data = try Data(contentsOf: storageURL)
            entries = try JSONDecoder().decode([ClipboardHistoryEntry].self, from: data)
        } catch {
            NSLog("ClipboardHistoryStore: failed to load: %@", error.localizedDescription)
        }
    }

    // MARK: - Fuzzy matching

    /// Simple subsequence fuzzy match.
    private func fuzzyMatch(text: String, query: String) -> Bool {
        var textIndex = text.startIndex
        var queryIndex = query.startIndex

        while textIndex < text.endIndex && queryIndex < query.endIndex {
            if text[textIndex] == query[queryIndex] {
                queryIndex = query.index(after: queryIndex)
            }
            textIndex = text.index(after: textIndex)
        }

        return queryIndex == query.endIndex
    }
}

/// Returns indices in `text` that match the fuzzy `query` (for highlighting).
func clipboardFuzzyMatchIndices(text: String, query: String) -> [Int] {
    guard !query.isEmpty else { return [] }
    let textLower = text.lowercased()
    let queryLower = query.lowercased()
    var indices: [Int] = []
    var textIdx = textLower.startIndex
    var queryIdx = queryLower.startIndex
    var charIndex = 0

    while textIdx < textLower.endIndex && queryIdx < queryLower.endIndex {
        if textLower[textIdx] == queryLower[queryIdx] {
            indices.append(charIndex)
            queryIdx = queryLower.index(after: queryIdx)
        }
        textIdx = textLower.index(after: textIdx)
        charIndex += 1
    }

    return queryIdx == queryLower.endIndex ? indices : []
}
