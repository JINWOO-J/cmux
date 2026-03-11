import SwiftUI

/// Overlay view for browsing and searching clipboard history.
/// Follows the same visual pattern as the command palette.
struct ClipboardHistoryOverlay: View {
    @ObservedObject var store: ClipboardHistoryStore
    @Binding var isPresented: Bool
    @State private var query: String = ""
    @State private var selectedIndex: Int = 0
    @State private var hoveredIndex: Int?
    @State private var scrollTargetIndex: Int?
    @State private var scrollTargetAnchor: UnitPoint?
    @FocusState private var isSearchFocused: Bool

    private var filteredEntries: [ClipboardHistoryEntry] {
        store.search(query: query)
    }

    var body: some View {
        GeometryReader { proxy in
            let maxAllowedWidth = max(340, proxy.size.width - 260)
            let targetWidth = min(600, maxAllowedWidth)

            ZStack(alignment: .top) {
                Color.clear
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .onTapGesture { dismiss() }

                VStack(spacing: 0) {
                    searchBar
                    Divider()
                    entryList
                    Divider()
                    footer
                }
                .frame(width: targetWidth)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(Color(nsColor: .windowBackgroundColor).opacity(0.98))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .stroke(Color(nsColor: .separatorColor).opacity(0.7), lineWidth: 1)
                )
                .shadow(color: Color.black.opacity(0.24), radius: 10, x: 0, y: 5)
                .padding(.top, 40)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .onExitCommand { dismiss() }
        .zIndex(2000)
    }

    // MARK: - Search Bar

    private var searchBar: some View {
        HStack(spacing: 8) {
            Image(systemName: "doc.on.clipboard")
                .foregroundStyle(.secondary)
                .font(.system(size: 13))

            TextField("Search clipboard history…", text: $query)
                .textFieldStyle(.plain)
                .font(.system(size: 13, weight: .regular))
                .tint(.white)
                .focused($isSearchFocused)
                .onSubmit { pasteSelected() }
                .backport.onKeyPress(.downArrow) { _ in
                    moveSelection(by: 1)
                    return .handled
                }
                .backport.onKeyPress(.upArrow) { _ in
                    moveSelection(by: -1)
                    return .handled
                }
                .backport.onKeyPress("n") { modifiers in
                    guard modifiers.contains(.control) else { return .ignored }
                    moveSelection(by: 1)
                    return .handled
                }
                .backport.onKeyPress("p") { modifiers in
                    guard modifiers.contains(.control) else { return .ignored }
                    moveSelection(by: -1)
                    return .handled
                }
                .backport.onKeyPress("j") { modifiers in
                    guard modifiers.contains(.control) else { return .ignored }
                    moveSelection(by: 1)
                    return .handled
                }
                .backport.onKeyPress("k") { modifiers in
                    guard modifiers.contains(.control) else { return .ignored }
                    moveSelection(by: -1)
                    return .handled
                }

            if !query.isEmpty {
                Text("\(filteredEntries.count)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 7)
        .onAppear {
            isSearchFocused = true
            selectedIndex = 0
        }
    }

    // MARK: - Entry List

    private var entryList: some View {
        let entries = filteredEntries
        let clampedIndex = entries.isEmpty ? 0 : min(selectedIndex, entries.count - 1)
        let listRowHeight: CGFloat = 52
        let emptyStateHeight: CGFloat = 44
        let listContentHeight = entries.isEmpty ? emptyStateHeight : CGFloat(entries.count) * listRowHeight
        let listMaxHeight: CGFloat = 400
        let listHeight = min(listMaxHeight, listContentHeight)

        return ScrollView {
            LazyVStack(spacing: 0) {
                if entries.isEmpty {
                    Text(query.isEmpty ? "No clipboard history" : "No matches")
                        .font(.system(size: 13, weight: .regular))
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 12)
                } else {
                    ForEach(Array(entries.enumerated()), id: \.element.id) { index, entry in
                        let isSelected = index == clampedIndex
                        let isHovered = hoveredIndex == index
                        let rowBg: Color = isSelected
                            ? Color.accentColor.opacity(0.12)
                            : (isHovered ? Color.primary.opacity(0.08) : .clear)

                        Button {
                            pasteEntry(entry)
                        } label: {
                            entryRow(entry: entry, isSelected: isSelected)
                        }
                        .buttonStyle(.plain)
                        .background(rowBg)
                        .contentShape(Rectangle())
                        .id(index)
                        .onHover { hovering in
                            if hovering {
                                hoveredIndex = index
                            } else if hoveredIndex == index {
                                hoveredIndex = nil
                            }
                        }
                        .contextMenu {
                            Button("Delete") { store.delete(entry: entry) }
                        }
                    }
                }
            }
            .scrollTargetLayout()
        }
        .frame(height: listHeight)
        .scrollPosition(
            id: Binding(
                get: { scrollTargetIndex },
                set: { _ in }
            ),
            anchor: scrollTargetAnchor
        )
        .onChange(of: selectedIndex) { _ in
            updateScrollTarget(count: entries.count)
        }
        .onChange(of: query) { _ in
            selectedIndex = 0
            hoveredIndex = nil
            scrollTargetIndex = nil
        }
    }

    private func entryRow(entry: ClipboardHistoryEntry, isSelected: Bool) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            let matchIndices = query.isEmpty ? [] : clipboardFuzzyMatchIndices(text: entry.preview, query: query)
            highlightedText(entry.preview, matchedIndices: matchIndices)
                .font(.system(size: 12, weight: .regular, design: .monospaced))
                .lineLimit(1)

            HStack(spacing: 6) {
                Text(entry.timestamp, style: .relative)
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)

                Text("·")
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)

                let lineCount = entry.text.components(separatedBy: "\n").count
                Text(lineCount == 1 ? "1 line" : "\(lineCount) lines")
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)

                Text("·")
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)

                Text(entry.sizeLabel)
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: - Footer

    private var footer: some View {
        HStack(spacing: 12) {
            Text("\(store.entries.count) items")
                .font(.system(size: 11))
                .foregroundStyle(.secondary)

            Spacer()

            Button("Clear All") {
                store.clearAll()
            }
            .font(.system(size: 11))
            .foregroundStyle(.secondary)
            .buttonStyle(.plain)
            .disabled(store.entries.isEmpty)

            // Hidden Esc-to-close button for keyboard shortcut
            Button(action: { dismiss() }) {
                EmptyView()
            }
            .buttonStyle(.plain)
            .keyboardShortcut(.cancelAction)
            .frame(width: 0, height: 0)
            .opacity(0)
            .accessibilityHidden(true)
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 5)
    }

    // MARK: - Helpers

    private func dismiss() {
        isPresented = false
    }

    private func pasteSelected() {
        let entries = filteredEntries
        guard !entries.isEmpty else { return }
        let index = min(selectedIndex, entries.count - 1)
        pasteEntry(entries[index])
    }

    private func pasteEntry(_ entry: ClipboardHistoryEntry) {
        store.paste(entry: entry)
        dismiss()
    }

    private func moveSelection(by delta: Int) {
        let count = filteredEntries.count
        guard count > 0 else { return }
        selectedIndex = max(0, min(count - 1, selectedIndex + delta))
    }

    private func updateScrollTarget(count: Int) {
        guard count > 0 else { return }
        let idx = min(selectedIndex, count - 1)
        scrollTargetIndex = idx
        if idx == 0 {
            scrollTargetAnchor = .top
        } else if idx == count - 1 {
            scrollTargetAnchor = .bottom
        } else {
            scrollTargetAnchor = .center
        }
    }

    /// Renders text with fuzzy-matched characters highlighted.
    private func highlightedText(_ text: String, matchedIndices: [Int]) -> Text {
        guard !matchedIndices.isEmpty else {
            return Text(text).foregroundColor(.primary)
        }
        let matchSet = Set(matchedIndices)
        var result = Text("")
        for (charIndex, char) in text.enumerated() {
            if matchSet.contains(charIndex) {
                result = result + Text(String(char)).foregroundColor(.accentColor).bold()
            } else {
                result = result + Text(String(char)).foregroundColor(.primary)
            }
        }
        return result
    }
}
