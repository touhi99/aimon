import AppKit
import SwiftUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let store = Store()
    var item: NSStatusItem!
    let popover = NSPopover()

    func applicationDidFinishLaunching(_ n: Notification) {
        // Single instance: a second copy would add another menu bar item and double all the log scanning.
        let id = Bundle.main.bundleIdentifier ?? "dev.aimon.app"
        let me = ProcessInfo.processInfo.processIdentifier
        if NSRunningApplication.runningApplications(withBundleIdentifier: id).contains(where: { $0.processIdentifier != me }) {
            exit(0)   // code 0 so the LaunchAgent (KeepAlive on failure only) does not relaunch it
        }
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.target = self
        item.button?.action = #selector(toggle)
        popover.behavior = .transient
        popover.contentViewController = NSHostingController(rootView: PopoverView(store: store))
        store.onChange = { [weak self] in self?.updateBar() }
        updateBar()
        store.start()
    }

    func updateBar() {
        guard let b = item.button else { return }
        guard let s = store.snap else { b.title = "AI Monitor"; return }
        if s.limits.source == "none" || s.limits.source == "off" {   // plan limits off or no Claude plan found: show money (or tokens) instead of an empty ring
            let f = NSFont.monospacedDigitSystemFont(ofSize: 12, weight: .medium)
            b.image = nil
            let t = NSMutableAttributedString(string: s.api.today >= 0.005 ? Fmt.money(s.api.today) : "AI Monitor", attributes: [.font: f])
            markStopped(t, f)
            b.attributedTitle = t
            b.toolTip = "API spend today: \(Fmt.money(s.api.today))" + stoppedNote
            return
        }
        let ses = s.limits.session?.pct ?? 0, wk = s.limits.weekly?.pct ?? 0
        // Show the 5-hour session; switch to the week only when the week is the one about to run out.
        let useWeek = wk >= 85 && wk > ses
        let pct = useWeek ? wk : ses
        let renderer = ImageRenderer(content: RingIcon(fraction: pct / 100))
        renderer.scale = NSScreen.main?.backingScaleFactor ?? 2
        if let img = renderer.nsImage { img.isTemplate = true; b.image = img; b.imagePosition = .imageLeft }
        let color: NSColor = pct >= 85 ? .systemRed : pct >= 60 ? .systemOrange : .labelColor
        let font = NSFont.monospacedDigitSystemFont(ofSize: 12, weight: .medium)
        let out = NSMutableAttributedString(string: " " + (useWeek ? "wk " : "") + "\(Int(pct.rounded()))%", attributes: [.font: font, .foregroundColor: color])
        if s.api.today >= 0.005 {
            out.append(NSAttributedString(string: "  " + Fmt.money(s.api.today), attributes: [.font: font, .foregroundColor: NSColor.systemOrange]))
        }
        markStopped(out, font)
        b.attributedTitle = out
        b.toolTip = "Claude Pro: \(Int(ses.rounded()))% of the 5-hour session used, \(Int(wk.rounded()))% of the week.\nAPI spend today: \(Fmt.money(s.api.today))" + stoppedNote
    }

    /// LaunchAgents of the listed projects that should be running and are not.
    private var stopped: [String] { store.projects?.projects.flatMap { $0.down } ?? [] }
    private var stoppedNote: String { stopped.isEmpty ? "" : "\nStopped: " + stopped.joined(separator: ", ") }
    private func markStopped(_ t: NSMutableAttributedString, _ font: NSFont) {
        if !stopped.isEmpty { t.append(NSAttributedString(string: "  ●", attributes: [.font: font, .foregroundColor: NSColor.systemRed])) }
    }

    @objc func toggle() {
        guard let b = item.button else { return }
        if popover.isShown { popover.performClose(nil) } else {
            store.refresh()
            popover.show(relativeTo: b.bounds, of: b, preferredEdge: .minY)
            popover.contentViewController?.view.window?.makeKey()
        }
    }
}

struct RingIcon: View {
    let fraction: Double
    var body: some View {
        ZStack {
            Circle().stroke(Color.black.opacity(0.3), lineWidth: 2.2)
            Circle().trim(from: 0, to: max(0.04, min(fraction, 1))).stroke(Color.black, style: StrokeStyle(lineWidth: 2.2, lineCap: .round)).rotationEffect(.degrees(-90))
        }.frame(width: 13, height: 13).padding(0.5)
    }
}

let app = NSApplication.shared
MainActor.assumeIsolated {
    if let i = CommandLine.arguments.firstIndex(of: "--png"), i + 1 < CommandLine.arguments.count {
        let out = CommandLine.arguments[i + 1]
        let store = Store()
        store.start()
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            // Draw the real view in an off-screen window: SwiftUI's ImageRenderer leaves ScrollView and Picker blank.
            for (name, scheme) in [("light", NSAppearance.Name.aqua), ("dark", NSAppearance.Name.darkAqua)] {
                let host = NSHostingView(rootView: PopoverView(store: store))
                let win = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 540, height: 900), styleMask: [.borderless], backing: .buffered, defer: false)
                win.appearance = NSAppearance(named: scheme)
                win.contentView = host
                host.frame = NSRect(x: 0, y: 0, width: 540, height: 900)
                for _ in 0..<5 { host.layoutSubtreeIfNeeded(); RunLoop.main.run(until: Date().addingTimeInterval(0.15)) }
                let h = host.fittingSize.height
                host.frame = NSRect(x: 0, y: 0, width: 540, height: h)
                win.setContentSize(NSSize(width: 540, height: h))
                host.layoutSubtreeIfNeeded()
                if let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) {
                    host.cacheDisplay(in: host.bounds, to: rep)
                    try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: out + "-" + name + ".png"))
                }
            }
            exit(0)
        }
        app.run()
        return
    }
    let delegate = AppDelegate()
    app.delegate = delegate
    app.setActivationPolicy(.accessory)
    app.run()
}
