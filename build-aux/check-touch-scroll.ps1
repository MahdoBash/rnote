param([string]$Executable)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Threading;
public static class TouchScrollTest {
 [StructLayout(LayoutKind.Sequential)] public struct Point { public int X,Y; }
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int L,T,R,B; }
 [StructLayout(LayoutKind.Sequential)] public struct Pointer {
  public uint Type,Id,Frame,Flags; public IntPtr Device,Target;
  public Point Pixel,Himetric,Raw,RawHimetric;
  public uint Time,History; public int Input; public uint Keys; public ulong Performance; public uint Change;
 }
 [StructLayout(LayoutKind.Sequential)] public struct Touch {
  public Pointer Pointer; public uint Flags,Mask; public Rect Contact,ContactRaw; public uint Orientation,Pressure;
 }
 [DllImport("user32.dll",SetLastError=true)] static extern bool InitializeTouchInjection(uint count,uint mode);
 [DllImport("user32.dll",SetLastError=true)] static extern bool InjectTouchInput(uint count,[In] Touch[] contacts);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls,string title);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int w,int height,uint flags);
 static void Send(int x,int y,uint flags) {
  var t=new Touch(); t.Pointer.Type=2;t.Pointer.Id=1;t.Pointer.Flags=flags;
  t.Pointer.Pixel=new Point { X=x,Y=y };t.Mask=7;
  t.Contact=new Rect{L=x-2,T=y-2,R=x+2,B=y+2};t.Orientation=90;t.Pressure=512;
  if(!InjectTouchInput(1,new[]{t})) throw new Exception("InjectTouchInput: "+Marshal.GetLastWin32Error());
 }
 public static void Init() { if(!InitializeTouchInjection(2,3)) throw new Exception("InitializeTouchInjection: "+Marshal.GetLastWin32Error()); }
 public static void Swipe(int x,int y,int distance,int delay,int pause) {
  Send(x,y,0x10006);Thread.Sleep(delay);
  for(int i=1;i<=20;i++){Send(x,y-distance*i/20,0x20006);Thread.Sleep(delay);}
  if(pause>0)Thread.Sleep(pause);
  Send(x,y-distance,0x40000);
 }
 public static void TapHold(int x,int y) {Send(x,y,0x10006);Thread.Sleep(150);Send(x,y,0x40000);}
}
'@
[TouchScrollTest]::Init()
$env:GSK_RENDERER='cairo'
$env:RNOTE_SCROLL_TRACE='1'
foreach($mode in @('reference','canvas')) {
 $env:RNOTE_SCROLL_TEST=$mode
 $log=Join-Path $pwd "scroll-$mode.txt"
 $app=Start-Process $Executable -PassThru -RedirectStandardError $log
 try {
  Start-Sleep -Seconds 7
  $app.Refresh()
  if($app.HasExited){Get-Content $log;throw "Probe application exited"}
  $handle=if($mode -eq 'reference'){[TouchScrollTest]::FindWindowW($null,'Scroll Reference')}else{$app.MainWindowHandle}
  if($handle -eq 0){Get-Content $log;throw 'Missing test window'}
  [void][TouchScrollTest]::SetWindowPos($handle,[IntPtr]::Zero,50,50,1000,700,0x40)
  [void][TouchScrollTest]::SetForegroundWindow($handle)
  Start-Sleep -Seconds 1
  # Same physical trace on the GTK reference and Rnote, away from toolbar overlays.
  [TouchScrollTest]::Swipe(600,540,260,12,0)
  Start-Sleep -Seconds 2
  [TouchScrollTest]::Swipe(600,540,260,12,250)
  Start-Sleep -Seconds 1
  [TouchScrollTest]::Swipe(600,340,-200,12,0)
  Start-Sleep -Milliseconds 100
  [TouchScrollTest]::TapHold(600,400)
  Start-Sleep -Seconds 1
 } finally { Stop-Process -Id $app.Id -ErrorAction SilentlyContinue }
 Get-Content $log | Where-Object {$_ -match 'SCROLL'}
 if(-not (Select-String -Path $log -Pattern 'TouchEnd' -Quiet)){throw "No actual touch events reached $mode"}
}
Remove-Item Env:RNOTE_SCROLL_TEST,Env:RNOTE_SCROLL_TRACE
