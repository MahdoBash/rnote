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
 static Touch Contact(uint id,int x,int y,uint flags) {
  var t=new Touch(); t.Pointer.Type=2;t.Pointer.Id=id;t.Pointer.Flags=flags;
  t.Pointer.Pixel=new Point { X=x,Y=y };t.Mask=7;
  t.Contact=new Rect{L=x-2,T=y-2,R=x+2,B=y+2};t.Orientation=90;t.Pressure=512;
  return t;
 }
 static void Send(int x,int y,uint flags) {
  if(!InjectTouchInput(1,new[]{Contact(1,x,y,flags)})) throw new Exception("InjectTouchInput: "+Marshal.GetLastWin32Error());
 }
 public static void Twist(int cx,int cy,double startRadius,double endRadius,double degrees) {
  for(int i=0;i<=25;i++){
   double f=Math.Min(i,24)/24.0;
   double radius=startRadius+(endRadius-startRadius)*f;
   int dx=(int)Math.Round(radius*Math.Cos(degrees*Math.PI*f/180));
   int dy=(int)Math.Round(radius*Math.Sin(degrees*Math.PI*f/180));
   uint flags=i==0?0x10006u:i==25?0x40000u:0x20006u;
   var pair=new[]{Contact(1,cx-dx,cy-dy,flags),Contact(2,cx+dx,cy+dy,flags)};
   if(!InjectTouchInput(2,pair)) throw new Exception("Two-finger injection: "+Marshal.GetLastWin32Error());
   Thread.Sleep(20);
  }
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
# Exercise the installed GTK app with two simultaneous Windows touch contacts.
$env:RNOTE_SCROLL_TEST='rotation'
$rotationLog=Join-Path $pwd 'scroll-rotation.txt'
$app=Start-Process $Executable -PassThru -RedirectStandardError $rotationLog
try {
 Start-Sleep -Seconds 7
 $app.Refresh()
 if($app.HasExited -or !$app.MainWindowHandle){throw 'Missing rotation test window'}
 [void][TouchScrollTest]::SetWindowPos($app.MainWindowHandle,[IntPtr]::Zero,50,50,1000,700,0x40)
 [void][TouchScrollTest]::SetForegroundWindow($app.MainWindowHandle)
 Start-Sleep -Seconds 1
 [TouchScrollTest]::Twist(600,410,75,96,65)
 Start-Sleep -Milliseconds 750
 [TouchScrollTest]::Twist(600,410,96,75,-65)
 Start-Sleep -Seconds 1
} finally { Stop-Process -Id $app.Id -ErrorAction SilentlyContinue }
$results=@(Select-String -Path $rotationLog -Pattern 'ROTATION END ([-.0-9]+) ([-.0-9]+)')
if($results.Count -ne 2){Get-Content $rotationLog;throw 'Expected two twist gestures'}
$angle=[double]$results[0].Matches.Groups[1].Value
$firstZoom=[double]$results[0].Matches.Groups[2].Value
$finalAngle=[double]$results[1].Matches.Groups[1].Value
$finalZoom=[double]$results[1].Matches.Groups[2].Value
if([Math]::Abs($angle-65) -gt 3 -or [Math]::Abs($finalAngle) -gt 3 -or
   [Math]::Abs($firstZoom/$finalZoom-96/75) -gt 0.05){throw "Pinch/twist mismatch: $angle, $firstZoom; $finalAngle, $finalZoom"}
$samples=@(Select-String -Path $rotationLog -Pattern 'ROTATION SAMPLE ([-.0-9]+) ([-.0-9]+) ([-.0-9]+)')
if($samples.Count -lt 10){throw 'Missing continuous rotation samples'}
$drift=($samples|ForEach-Object {[double]$_.Matches.Groups[3].Value}|Measure-Object -Maximum).Maximum
if($drift -gt 4){throw "Rotation pivot drifted by $drift pixels"}
Write-Output "PASS two-finger rotation and pinch, angle=$angle, reset=$finalAngle, max pivot drift=$drift"

foreach($mode in @('reference','canvas','expanding','disabled','drawing')) {
 $env:RNOTE_SCROLL_TEST=$mode
 $log=Join-Path $pwd "scroll-$mode.txt"
 $app=Start-Process $Executable -PassThru -RedirectStandardError $log
 try {
  Start-Sleep -Seconds 7
  $app.Refresh()
  if($app.HasExited){Get-Content $log;throw "Probe application exited"}
  $handle=if($mode -eq 'reference'){[IntPtr]([long]((Select-String -Path $log -Pattern 'SCROLL WINDOW (\d+)' | Select-Object -Last 1).Matches.Groups[1].Value))}else{$app.MainWindowHandle}
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
 $events=@();$frames=@()
 foreach($line in Get-Content $log) {
  if($line -match 'SCROLL EVENT (\d+) (TouchBegin|TouchEnd)'){$events+=@{T=[long]$Matches[1];Kind=$Matches[2]}}
  if($line -match 'SCROLL FRAME (\d+) ([\d.-]+) ([\d.-]+)'){$frames+=@{T=[long]$Matches[1];X=[double]$Matches[2];Y=[double]$Matches[3]}}
 }
 $ends=@($events|Where-Object Kind -eq 'TouchEnd');$begins=@($events|Where-Object Kind -eq 'TouchBegin')
 if($ends.Count -ne 4 -or $begins.Count -ne 4){throw "Expected all 4 touch sequences in $mode"}
 if($mode -eq 'reference'){continue}
 function PositionAt([long]$time) { ($frames|Where-Object {$_.T -le $time}|Select-Object -Last 1).Y }
 $release=PositionAt ($ends[0].T+40)
 $end=PositionAt ($begins[1].T-1)
 $distance=$end-$release
 if($mode -in @('disabled','drawing')) {
  if([Math]::Abs($distance) -gt 2){throw "Unexpected momentum with ${mode}: $distance"}
 } else {
  if($distance -lt 25 -or $distance -gt 600){throw "Missing/excessive forward coast in ${mode}: $distance"}
  $after=@($frames|Where-Object {$_.T -gt $ends[0].T+40 -and $_.T -lt $begins[1].T})
  for($i=1;$i -lt $after.Count;$i++){if($after[$i].Y -lt $after[$i-1].Y-0.5){throw "Coast reversed in $mode"}}
  $duration=$after[-1].T-$ends[0].T
  if($duration -lt 150 -or $duration -gt 1600){throw "Coast duration out of range in ${mode}: $duration ms"}
  $reverse=(PositionAt ($begins[3].T-1))-(PositionAt $ends[2].T)
  if($reverse -gt -5){throw "Reverse flick lost momentum in ${mode}: $reverse"}
 }
 $paused=(PositionAt ($begins[2].T-1))-(PositionAt $ends[1].T)
 if([Math]::Abs($paused) -gt 2){throw "Pause before release should prevent momentum in $mode"}
 $stopped=(PositionAt ([long]::MaxValue))-(PositionAt ($begins[3].T+40))
 if([Math]::Abs($stopped) -gt 2){throw "Touch did not stop momentum in $mode"}
 if($mode -eq 'expanding') {
  $armed=[double]((Select-String -Path $log -Pattern 'SCROLL ARMED_HEIGHT ([\d.]+)'|Select-Object -First 1).Matches.Groups[1].Value)
  $finalHeight=[double]((Select-String -Path $log -Pattern 'SCROLL HEIGHT ([\d.]+)'|Select-Object -Last 1).Matches.Groups[1].Value)
  if($finalHeight -le $armed+100){throw 'Expansion test did not cross a growth boundary'}
 }
 Write-Output "PASS ${mode}: coast=$distance px; pause=$paused px; stop=$stopped px"
}
Remove-Item Env:RNOTE_SCROLL_TEST,Env:RNOTE_SCROLL_TRACE
