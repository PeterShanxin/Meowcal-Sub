function appendClipSurface(bounds, radii, element) {
  if (!element) return;
  const rect = element.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return;
  bounds.push({
    x: Math.round(rect.left),
    y: Math.round(rect.top),
    width: Math.round(rect.width),
    height: Math.round(rect.height),
  });
  const cssRadius = getComputedStyle(element).borderTopLeftRadius;
  const parsedRadius = parseFloat(cssRadius || "0");
  radii.push(
    cssRadius.endsWith("%")
      ? (Math.min(rect.width, rect.height) * parsedRadius) / 100
      : Number.isFinite(parsedRadius)
        ? parsedRadius
        : 0,
  );
}

window.OverlayWindowClip = { appendClipSurface };
