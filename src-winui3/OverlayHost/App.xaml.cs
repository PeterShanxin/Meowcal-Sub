using Microsoft.UI.Xaml;
using System;
using System.Diagnostics;
using OverlayHost.Services;

namespace OverlayHost;

public partial class App : Application
{
    private IpcService? _ipcService;

    public App()
    {
        this.InitializeComponent();
    }

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        // Initialize IPC service
        _ipcService = new IpcService();
        _ipcService.ConnectionStateChanged += OnConnectionStateChanged;
        _ipcService.MessageReceived += OnMessageReceived;

        await _ipcService.StartAsync();

        // For now, just show a debug window to verify app launches
        var debugWindow = new Window
        {
            Title = "OverlayHost Debug",
            Content = new Microsoft.UI.Xaml.Controls.TextBlock
            {
                Text = "OverlayHost is running. Check debug output for IPC connection status.",
                HorizontalAlignment = HorizontalAlignment.Center,
                VerticalAlignment = VerticalAlignment.Center
            }
        };
        debugWindow.Activate();

        Debug.WriteLine($"[OverlayHost] App launched at {DateTime.Now}");
    }

    private void OnConnectionStateChanged(object? sender, bool isConnected)
    {
        Debug.WriteLine($"[App] IPC connection state: {(isConnected ? "Connected ✅" : "Disconnected ❌")}");
    }

    private void OnMessageReceived(object? sender, Models.IpcMessage message)
    {
        Debug.WriteLine($"[App] Received message: {message.Type}");
        // TODO: Route messages to appropriate handlers
    }
}
