import FirebaseAnalytics

public enum FirebaseAnalyticsStartup {
    public static func close() {
        // Clear the SDK's persisted setting before the app restores its consent.
        Analytics.setAnalyticsCollectionEnabled(false)
    }
}
