Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Drawing;
public static class StoreFixtureImage {
    public static bool IsVisible(Bitmap expected, Bitmap captured) {
        if (expected.Size != captured.Size) return false;
        long pixels = (long)expected.Width * expected.Height;
        long matched = 0, text = 0, matchedText = 0;
        for (int y = 0; y < expected.Height; y++) {
            for (int x = 0; x < expected.Width; x++) {
                Color a = expected.GetPixel(x, y), b = captured.GetPixel(x, y);
                bool same = Math.Abs(a.R-b.R) <= 32 && Math.Abs(a.G-b.G) <= 32 && Math.Abs(a.B-b.B) <= 32;
                if (same) matched++;
                if (a.R >= 128 && a.G >= 128 && a.B >= 128) {
                    text++;
                    if (same) matchedText++;
                }
            }
        }
        // Allow small rasterization differences, but require the text itself.
        return text > 0 && matched >= pixels * 0.999 && matchedText >= text * 0.99;
    }
}
"@ -ReferencedAssemblies System.Drawing
