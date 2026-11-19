# In-app feedback

The speech bubble in the title bar opens the feedback form from any tab. An error notification opens the same form with its type and diagnostics already selected.

| Entry point | Form |
| --- | --- |
| [Title bar](01-home-entry.png) | [Translation feedback](02-feedback.png) |
| [Report an error](04-error-entry.png) | [Error details](05-error-report.png) |

![Translation feedback with one optional subtitle pair selected](03-selected-material.png)

The user can review the exact subtitle text before selecting it. The footer explains what appears on GitHub. A failed send preserves the description and selections for another attempt. A successful send returns an issue number.

- [Retry with the draft preserved](06-retry.png)
- [Delivery receipt](07-sent.png)

These images show the application's Chinese UI rendered in Edge on Windows x64 build 26300.9974. The development checkout used the UI from commit `97b68d4`, with authored subtitle text, a simulated capture failure, and a local receipt. Issue #10001 is a fixture. The viewport ranges from 680 × 500 to 760 × 690 CSS pixels at a device scale factor of 1.5.

The Windows frontend workflow separately captures the native Tauri WebView2 on the hosted ARM64 runner. Its artifact `feedback-native-windows-arm64` includes the home screen, settings, feedback form, a controlled settings-save error, and the resulting error report.
