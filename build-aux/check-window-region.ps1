param([long]$WindowHandle)
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class WindowRegionCheck {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr window, uint command);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr window, StringBuilder title, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern IntPtr GetWindowLongPtrW(IntPtr window, int index);
    public static IntPtr FindShadow(IntPtr owner) {
        IntPtr result=IntPtr.Zero;
        EnumWindows((window, data) => {
            var title=new StringBuilder(128); GetWindowTextW(window,title,title.Capacity);
            if (GetWindow(window,4)==owner && title.ToString()=="Rnote Shadow" && IsWindowVisible(window)) result=window;
            return true;
        },IntPtr.Zero);
        return result;
    }

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

$shadow = [WindowRegionCheck]::FindShadow([IntPtr]$WindowHandle)
if ($shadow -eq [IntPtr]::Zero) { throw "Visible native shadow was not created" }
$styles = [WindowRegionCheck]::GetWindowLongPtrW($shadow, -20).ToInt64()
if (($styles -band 0x080800a0) -ne 0x080800a0) { throw "Shadow must be layered, click-through and non-activating" }
Write-Output "Native click-through shadow passed"
Add-Type -AssemblyName System.Drawing
$rect = New-Object WindowRegionCheck+Rect
[void][WindowRegionCheck]::GetWindowRect([IntPtr]$WindowHandle, [ref]$rect)
$x = [Math]::Max(0, $rect.Left - 20)
$y = [Math]::Max(0, $rect.Top - 20)
$bitmap = New-Object System.Drawing.Bitmap(($rect.Right - $x + 20), ($rect.Bottom - $y + 20))
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
try {
    $graphics.CopyFromScreen($x, $y, 0, 0, $bitmap.Size)
    $bitmap.Save((Join-Path $pwd "window-check.png"))
} finally { $graphics.Dispose(); $bitmap.Dispose() }
