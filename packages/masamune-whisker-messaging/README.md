# masamune-whisker-messaging

Push notification adapter interface (`MessagingAdapter`), handle (`Messaging`), in-memory test adapter (`RuntimeMessagingAdapter`) and `MessagingMasamuneAdapter` for Whisker apps.

Provider implementations (such as `masamune-whisker-messaging-firebase` for FCM) live in separate crates so feature crates depend only on this interface and tests run with `RuntimeMessagingAdapter`.
