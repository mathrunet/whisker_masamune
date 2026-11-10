// Native OS date and time picker dialogs.
//
// A `UIDatePicker` is shown in a centered card with Cancel / Done (like the Android dialogs).
// Cancel, a tap outside the card or any other dismissal resolves `null`.

import UIKit
import WhiskerModule

@WhiskerModule
public final class DatePickerModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("DatePicker")

            // pickDate({ year, month(1-12), day, minMillis?, maxMillis? }) -> { year, month, day } | null (cancelled)
            AsyncFunction("pickDate") { args, promise in
                let json = Bridge.args(args)
                Bridge.onMain {
                    guard let presenter = Self.presenter() else {
                        Bridge.reject(promise, "no window")
                        return
                    }
                    let calendar = Self.calendar()
                    let picker = UIDatePicker()
                    picker.datePickerMode = .date
                    picker.preferredDatePickerStyle = .inline
                    picker.calendar = calendar
                    picker.timeZone = calendar.timeZone
                    if let min = Self.millis(json["minMillis"]) { picker.minimumDate = min }
                    if let max = Self.millis(json["maxMillis"]) { picker.maximumDate = max }
                    let initial = DateComponents(
                        year: Self.int(json["year"]) ?? 2001,
                        month: Self.int(json["month"]) ?? 1,
                        day: Self.int(json["day"]) ?? 1,
                        hour: 12
                    )
                    if let date = calendar.date(from: initial) { picker.date = date }
                    let dialog = PickerDialogController(picker: picker) { date in
                        guard let date else {
                            Bridge.resolve(promise, nil)
                            return
                        }
                        let parts = calendar.dateComponents([.year, .month, .day], from: date)
                        Bridge.resolve(promise, ["year": parts.year, "month": parts.month, "day": parts.day])
                    }
                    presenter.present(dialog, animated: true)
                }
            }

            // pickTime({ hour, minute }) -> { hour, minute } | null (follows the device's 12/24-hour setting)
            AsyncFunction("pickTime") { args, promise in
                let json = Bridge.args(args)
                Bridge.onMain {
                    guard let presenter = Self.presenter() else {
                        Bridge.reject(promise, "no window")
                        return
                    }
                    let calendar = Self.calendar()
                    let picker = UIDatePicker()
                    picker.datePickerMode = .time
                    picker.preferredDatePickerStyle = .wheels
                    picker.calendar = calendar
                    picker.timeZone = calendar.timeZone
                    if let date = calendar.date(
                        bySettingHour: Self.int(json["hour"]) ?? 8,
                        minute: Self.int(json["minute"]) ?? 0,
                        second: 0,
                        of: Date()
                    ) {
                        picker.date = date
                    }
                    let dialog = PickerDialogController(picker: picker) { date in
                        guard let date else {
                            Bridge.resolve(promise, nil)
                            return
                        }
                        let parts = calendar.dateComponents([.hour, .minute], from: date)
                        Bridge.resolve(promise, ["hour": parts.hour, "minute": parts.minute])
                    }
                    presenter.present(dialog, animated: true)
                }
            }
        }
    }

    // MARK: - Helpers

    /// Gregorian calendar in the device time zone (the Android pickers are Gregorian too).
    private static func calendar() -> Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone.current
        calendar.locale = Locale.current
        return calendar
    }

    private static func int(_ value: Any?) -> Int? {
        (value as? NSNumber)?.intValue
    }

    private static func millis(_ value: Any?) -> Date? {
        guard let number = value as? NSNumber else { return nil }
        return Date(timeIntervalSince1970: number.doubleValue / 1000)
    }

    /// Top-most view controller of the key window.
    private static func presenter() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let ordered = scenes.filter { $0.activationState == .foregroundActive } + scenes
        let window = ordered.lazy.compactMap { $0.windows.first(where: \.isKeyWindow) ?? $0.windows.first }.first
        guard var top = window?.rootViewController else { return nil }
        while let presented = top.presentedViewController, !presented.isBeingDismissed {
            top = presented
        }
        return top
    }
}

/// Dimmed full-screen controller with the picker and a Cancel / Done toolbar in a centered card.
private final class PickerDialogController: UIViewController {
    private let picker: UIDatePicker
    private var onFinish: ((Date?) -> Void)?

    init(picker: UIDatePicker, onFinish: @escaping (Date?) -> Void) {
        self.picker = picker
        self.onFinish = onFinish
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .overFullScreen
        modalTransitionStyle = .crossDissolve
    }

    required init?(coder: NSCoder) { nil }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor.black.withAlphaComponent(0.4)

        let backdrop = UIControl()
        backdrop.translatesAutoresizingMaskIntoConstraints = false
        backdrop.addTarget(self, action: #selector(cancel), for: .touchUpInside)
        view.addSubview(backdrop)

        let card = UIView()
        card.translatesAutoresizingMaskIntoConstraints = false
        card.backgroundColor = .secondarySystemGroupedBackground
        card.layer.cornerRadius = 14
        card.clipsToBounds = true
        view.addSubview(card)

        let toolbar = UIToolbar()
        toolbar.items = [
            UIBarButtonItem(barButtonSystemItem: .cancel, target: self, action: #selector(cancel)),
            UIBarButtonItem(barButtonSystemItem: .flexibleSpace, target: nil, action: nil),
            UIBarButtonItem(barButtonSystemItem: .done, target: self, action: #selector(done)),
        ]
        toolbar.setBackgroundImage(UIImage(), forToolbarPosition: .any, barMetrics: .default)
        toolbar.setShadowImage(UIImage(), forToolbarPosition: .any)

        let stack = UIStackView(arrangedSubviews: [picker, toolbar])
        stack.axis = .vertical
        stack.alignment = .center
        stack.spacing = 4
        stack.translatesAutoresizingMaskIntoConstraints = false
        toolbar.translatesAutoresizingMaskIntoConstraints = false
        card.addSubview(stack)

        let preferredWidth = card.widthAnchor.constraint(equalToConstant: 340)
        preferredWidth.priority = .defaultHigh
        NSLayoutConstraint.activate([
            backdrop.topAnchor.constraint(equalTo: view.topAnchor),
            backdrop.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            backdrop.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            backdrop.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            card.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            card.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            preferredWidth,
            card.widthAnchor.constraint(lessThanOrEqualTo: view.safeAreaLayoutGuide.widthAnchor, constant: -32),
            card.heightAnchor.constraint(lessThanOrEqualTo: view.safeAreaLayoutGuide.heightAnchor, constant: -32),
            stack.topAnchor.constraint(equalTo: card.topAnchor, constant: 8),
            // Bar buttons can be taller than the toolbar's own height (iOS 26 glass buttons), so keep
            // room below them inside the rounded card.
            stack.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -12),
            toolbar.heightAnchor.constraint(greaterThanOrEqualToConstant: 52),
            stack.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 8),
            stack.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -8),
            toolbar.widthAnchor.constraint(equalTo: stack.widthAnchor),
            picker.widthAnchor.constraint(lessThanOrEqualTo: stack.widthAnchor),
        ])
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        // Dismissed by something else (for example another presentation): treat as cancelled.
        finish(nil)
    }

    @objc private func cancel() {
        finish(nil)
        dismiss(animated: true)
    }

    @objc private func done() {
        finish(picker.date)
        dismiss(animated: true)
    }

    private func finish(_ date: Date?) {
        guard let onFinish else { return }
        self.onFinish = nil
        onFinish(date)
    }
}
