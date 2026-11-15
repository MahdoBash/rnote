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
 Write-Output "PASS ${mode}: coast=$distance px; pause=$paused px; stop=$stopped px"
}
Remove-Item Env:RNOTE_SCROLL_TEST,Env:RNOTE_SCROLL_TRACE
