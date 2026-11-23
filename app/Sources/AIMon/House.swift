import SwiftUI

let emerald = Color(red: 0.2, green: 0.78, blue: 0.55)

extension Color {
    init(hex: UInt32) {
        self.init(red: Double((hex >> 16) & 0xff) / 255, green: Double((hex >> 8) & 0xff) / 255, blue: Double(hex & 0xff) / 255)
    }
    static let hBg = Color(hex: 0x0a0a0b)
    static let hSide = Color(hex: 0x0e0e10)
    static let hRaised = Color(hex: 0x1a1a1e)
    static let hLine = Color(hex: 0x242428)
    static let hLine2 = Color(hex: 0x303036)
    static let hInk = Color(hex: 0xf4f4f5)
    static let hSoft = Color(hex: 0xc4c4cc)
    static let hMut = Color(hex: 0x8b8b95)
    static let hAmber = Color(hex: 0xfbbf24)
    static let hOrange = Color(hex: 0xfb923c)
    static let hRose = Color(hex: 0xfb7185)
}

struct HousePanel: ViewModifier {
    var radius: CGFloat = 20
    func body(content: Content) -> some View {
        content
            .background(
                LinearGradient(colors: [Color(hex: 0x151517), Color(hex: 0x111113)], startPoint: .top, endPoint: .bottom),
                in: RoundedRectangle(cornerRadius: radius, style: .continuous)
            )
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(Color.hLine, lineWidth: 1))
    }
}

extension View {
    func housePanel(_ radius: CGFloat = 20) -> some View { modifier(HousePanel(radius: radius)) }
}

struct HouseRing<Center: View>: View {
    var value: Double
    var colors: [Color] = [emerald, Color.hAmber]
    var size: CGFloat = 150
    var stroke: CGFloat = 13
    @ViewBuilder var center: Center

    var body: some View {
        ZStack {
            Circle().stroke(Color(hex: 0x232327), lineWidth: stroke)
            Circle().trim(from: 0, to: max(0.01, min(1, value)))
                .stroke(AngularGradient(colors: colors, center: .center, startAngle: .degrees(0), endAngle: .degrees(360 * max(0.01, min(1, value)))),
                        style: StrokeStyle(lineWidth: stroke, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .shadow(color: (colors.first ?? emerald).opacity(0.35), radius: 8)
            center
        }
        .frame(width: size, height: size)
    }
}

struct Glow: View {
    var color: Color = emerald
    var body: some View {
        RadialGradient(colors: [color.opacity(0.10), .clear], center: UnitPoint(x: 0.3, y: -0.15), startRadius: 0, endRadius: 520)
            .allowsHitTesting(false)
    }
}

struct AppMark: View {
    var size: CGFloat = 1024

    var body: some View {
        let s = size
        ZStack {
            RoundedRectangle(cornerRadius: s * 0.225, style: .continuous)
                .fill(LinearGradient(colors: [Color(hex: 0x16201b), Color(hex: 0x070908)], startPoint: .top, endPoint: .bottom))
            RadialGradient(colors: [emerald.opacity(0.28), .clear], center: .center, startRadius: 0, endRadius: s * 0.42)
                .clipShape(RoundedRectangle(cornerRadius: s * 0.225, style: .continuous))
            Circle().stroke(Color.white.opacity(0.08), lineWidth: s * 0.085).frame(width: s * 0.56, height: s * 0.56)
            Circle().trim(from: 0, to: 0.72)
                .stroke(AngularGradient(colors: [emerald, Color(hex: 0x6ee7b7), Color.hAmber], center: .center, startAngle: .degrees(0), endAngle: .degrees(260)),
                        style: StrokeStyle(lineWidth: s * 0.085, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .frame(width: s * 0.56, height: s * 0.56)
                .shadow(color: emerald.opacity(0.6), radius: s * 0.03)
            Circle().fill(emerald).frame(width: s * 0.12, height: s * 0.12)
                .shadow(color: emerald, radius: s * 0.04)
        }
        .frame(width: s, height: s)
    }
}

struct Label13: View {
    let text: String
    var trailing: String?
    init(_ text: String, trailing: String? = nil) { self.text = text; self.trailing = trailing }
    var body: some View {
        HStack {
            Text(text).font(.system(size: 13)).foregroundStyle(Color.hMut)
            Spacer()
            if let trailing { Text(trailing).font(.system(size: 12)).foregroundStyle(Color.hMut) }
        }
    }
}

struct Pill: View {
    let text: String
    var tint: Color = .hSoft
    var icon: String?
    var body: some View {
        HStack(spacing: 5) {
            if let icon { Image(systemName: icon).font(.system(size: 10.5, weight: .semibold)) }
            Text(text).lineLimit(1)
        }
        .font(.system(size: 12, weight: .medium)).monospacedDigit()
        .foregroundStyle(tint)
        .padding(.horizontal, 9).frame(height: 24)
        .background(tint.opacity(0.12), in: Capsule())
    }
}

struct CloseButton: View {
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            Image(systemName: "xmark").font(.system(size: 11, weight: .bold)).foregroundStyle(Color.hSoft)
                .frame(width: 26, height: 26).background(Color.white.opacity(0.06), in: Circle()).contentShape(Circle())
        }
        .buttonStyle(.plain)
    }
}

struct HoverRow: ViewModifier {
    var on: Bool
    @State private var hover = false
    func body(content: Content) -> some View {
        content
            .background(Color.white.opacity(on ? 0.07 : hover ? 0.04 : 0), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .contentShape(Rectangle())
            .onHover { hover = $0 }
    }
}

extension View {
    func hoverRow(_ on: Bool = false) -> some View { modifier(HoverRow(on: on)) }
}
