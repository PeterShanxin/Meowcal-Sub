using Microsoft.UI.Xaml;
using System;
using System.Diagnostics;

namespace OverlayHost;

public partial class App : Application
{
    public App()
    {
        this.InitializeComponent();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        // For now, just show a debug window to verify app launches
        var debugWindow = new Window
        {
            Title = "OverlayHost Debug",
            Content = new Microsoft.UI.Xaml.Controls.TextBlock
            {
                Text = "OverlayHost is running. This is a temporary debug window.",
                HorizontalAlignment = HorizontalAlignment.Center,
                VerticalAlignment = VerticalAlignment.Center
            }
        };
        debugWindow.Activate();

        Debug.WriteLine($"[OverlayHost] App launched at {DateTime.Now}");
    }
}
