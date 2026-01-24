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

    public App()
    {
        this.InitializeComponent();
    }

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        // Create overlay window (initially hidden)
        _overlayWindow = new FrameOverlayWindow();
        Debug.WriteLine("[App] Created FrameOverlayWindow");

        // Initialize IPC service
        _ipcService = new IpcService();
        _ipcService.ConnectionStateChanged += OnConnectionStateChanged;
        _ipcService.MessageReceived += OnMessageReceived;

        await _ipcService.StartAsync();

        Debug.WriteLine($"[OverlayHost] App launched at {DateTime.Now}");
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
                                "unknown" // Backend info not in payload
                            );
                        }
                    }
                    break;

                case "Subtitle.Clear":
                    Debug.WriteLine("[App] Handling Subtitle.Clear");
                    _overlayWindow?.ClearSubtitle();
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
}
