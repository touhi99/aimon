import AppKit
import SwiftUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    let store = Store()
    var item: NSStatusItem!
    let popover = NSPopover()
    let nav = Nav()
    var window: NSWindow?
    var timeTimer: Timer?

    func applicationDidFinishLaunching(_ n: Notification) {
        let id = Bundle.main.bundleIdentifier ?? "dev.aimon.app"
        let me = ProcessInfo.processInfo.processIdentifier
        if NSRunningApplication.runningApplications(withBundleIdentifier: id).contains(where: { $0.processIdentifier != me }) {
            exit(1)
        }
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.target = self
        item.button?.action = #selector(toggle)
        popover.behavior = .transient
        popover.contentViewController = NSHostingController(rootView: MenuView(store: store) { [weak self] page, project in self?.openWindow(page, project: project) })
        store.onChange = { [weak self] in self?.updateBar() }
        updateBar()
        store.start()
        startTimeTracking()
        NSWorkspace.shared.notificationCenter.addObserver(forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main) { [weak self] n in
            guard let path = (n.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication)?.bundleURL?.path else { return }
            MainActor.assumeIsolated {
                guard let self, self.store.trackedApps.contains(path) else { return }
                self.store.recordUse(path)
            }
        }
    }

    func startTimeTracking() {
        timeTimer = Timer.scheduledTimer(withTimeInterval: 15, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, let path = NSWorkspace.shared.frontmostApplication?.bundleURL?.path,
                      self.store.trackedApps.contains(path) else { return }
                let idle = CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: CGEventType(rawValue: ~0)!)
                if idle < 120 { self.store.recordTime(path, 15) }
            }
        }
    }

    func updateBar() {
        guard let b = item.button else { return }
        guard let s = store.snap else { b.title = "AI Monitor"; return }
        if s.limits.source == "none" || s.limits.source == "off" {
            let f = NSFont.monospacedDigitSystemFont(ofSize: 12, weight: .medium)
            b.image = nil
            let t = NSMutableAttributedString(string: s.api.today >= 0.005 ? Fmt.money(s.api.today) : "AI Monitor", attributes: [.font: f])
            markStopped(t, f)
            b.attributedTitle = t
            b.toolTip = "API spend today: \(Fmt.money(s.api.today))" + stoppedNote
            return
        }
        let ses = s.limits.session?.pct ?? 0, wk = s.limits.weekly?.pct ?? 0
        let useWeek = wk >= 85 && wk > ses
        let pct = useWeek ? wk : ses
        let renderer = ImageRenderer(content: RingIcon(fraction: pct / 100))
        renderer.scale = NSScreen.main?.backingScaleFactor ?? 2
        if let img = renderer.nsImage { img.isTemplate = true; b.image = img; b.imagePosition = .imageLeft }
        let color: NSColor = pct >= 85 ? .systemRed : pct >= 60 ? .systemOrange : .labelColor
        let font = NSFont.monospacedDigitSystemFont(ofSize: 12, weight: .medium)
        let out = NSMutableAttributedString(string: " " + (useWeek ? "wk " : "") + "\(Int(pct.rounded()))%", attributes: [.font: font, .foregroundColor: color])
        markStopped(out, font)
        b.attributedTitle = out
        b.toolTip = "Claude Pro: \(Int(ses.rounded()))% of the 5-hour session used, \(Int(wk.rounded()))% of the week.\nAPI spend today: \(Fmt.money(s.api.today))" + stoppedNote
    }

    private var stopped: [String] { store.projects?.projects.flatMap { $0.down } ?? [] }
    private var stoppedNote: String { stopped.isEmpty ? "" : "\nStopped: " + stopped.joined(separator: ", ") }
    private func markStopped(_ t: NSMutableAttributedString, _ font: NSFont) {
        if !stopped.isEmpty { t.append(NSAttributedString(string: "  ●", attributes: [.font: font, .foregroundColor: NSColor.systemRed])) }
    }

    func openWindow(_ page: Int, project: String? = nil) {
        popover.performClose(nil)
        nav.project = project
        nav.page = page
        if window == nil {
            let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1100, height: 780),
                             styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
            w.title = "AI Monitor"
            w.contentViewController = NSHostingController(rootView: AppWindowView(store: store, nav: nav))
            w.setContentSize(NSSize(width: 1100, height: 780))
            w.isReleasedWhenClosed = false
            w.setFrameAutosaveName("AIMonitorWindow")
            w.delegate = self
            w.center()
            window = w
        }
        NSApp.setActivationPolicy(.regular)
        window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        store.refresh()
        store.recordUse("aimon")
    }

    func windowWillClose(_ n: Notification) {
        NSApp.setActivationPolicy(.accessory)
    }

    @objc func toggle() {
        guard let b = item.button else { return }
        if popover.isShown { popover.performClose(nil) } else {
            store.refresh()
            store.recordUse("aimon")
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
    if let out = ProcessInfo.processInfo.environment["AIMON_ICON"] {
        let r = ImageRenderer(content: AppMark(size: 824).padding(100))
        r.scale = 1
        if let img = r.nsImage, let tiff = img.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff) {
            try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: out))
        }
        exit(0)
    }
    if let i = CommandLine.arguments.firstIndex(of: "--png"), i + 1 < CommandLine.arguments.count {
        let out = CommandLine.arguments[i + 1]
        let store = Store()
        store.start()
        DispatchQueue.main.asyncAfter(deadline: .now() + 25) {
            let env = ProcessInfo.processInfo.environment
            let wide = env["AIMON_WINDOW"] != nil
            let size = NSSize(width: wide ? 1100 : 380, height: wide ? 1000 : 900)
            for (name, scheme) in [("light", NSAppearance.Name.aqua), ("dark", NSAppearance.Name.darkAqua)] {
                let root = wide ? AnyView(AppWindowView(store: store, nav: Nav())) : AnyView(MenuView(store: store) { _, _ in })
                let host = NSHostingView(rootView: root)
                let win = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.borderless], backing: .buffered, defer: false)
                win.appearance = NSAppearance(named: scheme)
                win.contentView = host
                host.frame = NSRect(origin: .zero, size: size)
                for _ in 0..<5 { host.layoutSubtreeIfNeeded(); RunLoop.main.run(until: Date().addingTimeInterval(0.15)) }
                let h = wide ? size.height : host.fittingSize.height
                host.frame = NSRect(x: 0, y: 0, width: size.width, height: h)
                win.setContentSize(NSSize(width: size.width, height: h))
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
