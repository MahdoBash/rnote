param([long]$WindowHandle)
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class WindowRegionCheck {
    [DllImport("gdi32.dll")] public static extern IntPtr CreateRectRgn(int l, int t, int r, int b);
    [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr h);
    [DllImport("gdi32.dll")] public static extern bool PtInRegion(IntPtr h, int x, int y);
    [DllImport("user32.dll")] public static extern int GetWindowRgn(IntPtr w, IntPtr r);
}
'@
$region = [WindowRegionCheck]::CreateRectRgn(0, 0, 0, 0)
try {
    if ([WindowRegionCheck]::GetWindowRgn([IntPtr]$WindowHandle, $region) -le 0) {
        throw "The application has no native clipping region"
    }
    if ([WindowRegionCheck]::PtInRegion($region, 0, 0)) {
        throw "The transparent window corner has not been clipped"
    }
    if (-not [WindowRegionCheck]::PtInRegion($region, 100, 100)) {
        throw "The native region incorrectly clips window content"
    }
    Write-Output "Native rounded window clipping passed"
} finally { [void][WindowRegionCheck]::DeleteObject($region) }
