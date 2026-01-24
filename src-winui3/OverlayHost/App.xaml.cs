using Microsoft.UI.Xaml;
using System;
using System.Diagnostics;
using System.Text.Json;
using OverlayHost.Services;
using OverlayHost.Windows;
using OverlayHost.Models;

namespace OverlayHost;

public partial class App : Application
{
    private IpcService? _ipcService;
    private FrameOverlayWindow? _overlayWindow;
    private SelectorWindow? _selectorWindow;

    public App()
    {
        this.InitializeComponent();
    }

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        try
        {
            Debug.WriteLine("[App] OverlayHost launched");

            // Initialize IPC service
            _ipcService = new IpcService();
            _ipcService.ConnectionStateChanged += OnConnectionStateChanged;
            _ipcService.MessageReceived += OnMessageReceived;

            await _ipcService.StartAsync();

            // Create overlay window (hidden initially)
            _overlayWindow = new FrameOverlayWindow();
            _overlayWindow.SettingsRequested += OnSettingsRequested;

            // Create selector window
            _selectorWindow = new SelectorWindow();
            _selectorWindow.SelectionConfirmed += OnSelectionConfirmed;
            _selectorWindow.SelectionCancelled += OnSelectionCancelled;

            Debug.WriteLine("[App] OverlayHost initialized successfully");
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[App] CRITICAL: Failed to initialize OverlayHost: {ex.Message}");
            Debug.WriteLine($"[App] Stack trace: {ex.StackTrace}");
            // Application cannot function without proper initialization
            // Let it terminate rather than continue in broken state
            throw;
        }
    }

    private void OnConnectionStateChanged(object? sender, bool isConnected)
    {
        Debug.WriteLine($"[App] IPC connection state: {(isConnected ? "Connected ✅" : "Disconnected ❌")}");
    }

    private void OnMessageReceived(object? sender, IpcMessage message)
    {
        Debug.WriteLine($"[App] Received message: {message.Type}");

        // Route messages via UI thread dispatcher
        _overlayWindow?.DispatcherQueue.TryEnqueue(() =>
        {
            HandleMessage(message);
        });
    }

    /// <summary>
    /// Handle incoming IPC messages and route to appropriate handlers.
    /// </summary>
    private void HandleMessage(IpcMessage message)
    {
        try
        {
            switch (message.Type)
            {
                case "Overlay.Show":
                    Debug.WriteLine("[App] Handling Overlay.Show");
                    _overlayWindow?.Show();
                    break;

                case "Overlay.Hide":
                    Debug.WriteLine("[App] Handling Overlay.Hide");
                    _overlayWindow?.Hide();
                    break;

                case "Overlay.SetRegion":
                    Debug.WriteLine("[App] Handling Overlay.SetRegion");
                    if (message.Payload != null)
                    {
                        var payloadJson = message.Payload.Value.GetRawText();
                        var region = JsonSerializer.Deserialize<Region>(payloadJson);
                        _overlayWindow?.SetRegion(region);
                    }
                    break;

                case "Settings.Sync":
                    Debug.WriteLine("[App] Handling Settings.Sync");
                    if (message.Payload != null)
                    {
                        var payloadJson = message.Payload.Value.GetRawText();
                        var settings = JsonSerializer.Deserialize<OverlaySettings>(payloadJson);
                        if (settings != null)
                        {
                            _overlayWindow?.UpdateSettings(settings);
                        }
                    }
                    break;

                case "Subtitle.Update":
                    Debug.WriteLine("[App] Handling Subtitle.Update");
                    if (message.Payload != null)
                    {
                        var payloadJson = message.Payload.Value.GetRawText();
                        var payload = JsonSerializer.Deserialize<SubtitleUpdatePayload>(payloadJson);
                        if (payload != null)
                        {
                            _overlayWindow?.UpdateSubtitle(
                                payload.SourceText ?? "",
                                payload.Text,
                                "" // Backend info not in payload (reserved for future use)
                            );
                        }
                    }
                    break;

                case "Subtitle.Clear":
                    Debug.WriteLine("[App] Handling Subtitle.Clear");
                    _overlayWindow?.ClearSubtitle();
                    break;

                case "Region.RequestOpenSelector":
                    Debug.WriteLine("[App] Handling Region.RequestOpenSelector");
                    _selectorWindow?.ShowSelector();
                    break;

                default:
                    Debug.WriteLine($"[App] Unknown message type: {message.Type}");
                    break;
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[App] Error handling message {message.Type}: {ex.Message}");
        }
    }

    /// <summary>
    /// Handle selection confirmation - send physical coordinates to backend.
    /// </summary>
    private async void OnSelectionConfirmed(object? sender, Region region)
    {
        try
        {
            Debug.WriteLine($"[App] Selection confirmed: ({region.X}, {region.Y}) {region.Width}x{region.Height}");

            if (_ipcService == null)
            {
                Debug.WriteLine("[App] WARNING: IPC service not available, cannot send selector result");
                return;
            }

            // Send Selector.Result message with physical coordinates
            var message = IpcMessage.Create("Selector.Result", new SelectorResultPayload
            {
                Region = region,
                Cancelled = false
            });

            await _ipcService.SendMessageAsync(message);
            Debug.WriteLine("[App] Sent Selector.Result to backend");
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[App] Error in OnSelectionConfirmed: {ex.Message}");
        }
    }

    /// <summary>
    /// Handle selection cancellation - notify backend.
    /// </summary>
    private async void OnSelectionCancelled(object? sender, EventArgs e)
    {
        try
        {
            Debug.WriteLine("[App] Selection cancelled");

            if (_ipcService == null)
            {
                Debug.WriteLine("[App] WARNING: IPC service not available, cannot send cancellation");
                return;
            }

            // Send Selector.Cancelled message
            var message = IpcMessage.Create("Selector.Cancelled", new SelectorResultPayload
            {
                Region = null,
                Cancelled = true
            });

            await _ipcService.SendMessageAsync(message);
            Debug.WriteLine("[App] Sent Selector.Cancelled to backend");
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[App] Error in OnSelectionCancelled: {ex.Message}");
        }
    }

    /// <summary>
    /// Handle settings request from overlay window.
    /// Send IPC message to backend to open settings.
    /// </summary>
    private async void OnSettingsRequested(object? sender, EventArgs e)
    {
        try
        {
            Debug.WriteLine("[App] Settings requested from overlay");

            if (_ipcService != null)
            {
                var message = IpcMessage.Create("Overlay.SettingsRequested", new { });
                await _ipcService.SendMessageAsync(message);
                Debug.WriteLine("[App] Sent Overlay.SettingsRequested to backend");
            }
            else
            {
                Debug.WriteLine("[App] WARNING: IPC service not available, cannot send settings request");
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[App] Error in OnSettingsRequested: {ex.Message}");
        }
    }
}
