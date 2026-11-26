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
 [StructLayout(LayoutKind.Sequential)] struct Pen {
  public Pointer Pointer; public uint Flags,Mask,Pressure,Rotation; public int TiltX,TiltY;
 }
 [StructLayout(LayoutKind.Explicit)] struct PointerData {
  [FieldOffset(0)] public Touch Touch;
  [FieldOffset(0)] public Pen Pen;
 }
 [StructLayout(LayoutKind.Sequential)] struct TypedPointer { public uint Type; public PointerData Data; }
 [DllImport("user32.dll",SetLastError=true)] static extern IntPtr CreateSyntheticPointerDevice(uint type,uint count,uint feedback);
 [DllImport("user32.dll",SetLastError=true)] static extern bool InjectSyntheticPointerInput(IntPtr device,ref TypedPointer input,uint count);
 [DllImport("user32.dll")] static extern void DestroySyntheticPointerDevice(IntPtr device);
 [DllImport("user32.dll",SetLastError=true)] static extern bool InitializeTouchInjection(uint count,uint mode);
 [DllImport("user32.dll",SetLastError=true)] static extern bool InjectTouchInput(uint count,
  [In,MarshalAs(UnmanagedType.LPArray,SizeParamIndex=0)] Touch[] contacts);
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
 static void Frame(string step,params Touch[] contacts) {
  for(int attempt=0;attempt<5;attempt++) {
   if(InjectTouchInput((uint)contacts.Length,contacts)) return;
   int error=Marshal.GetLastWin32Error();
   if(error!=21 || attempt==4) throw new Exception(step+": Win32 error "+error);
   Thread.Sleep(2); // ERROR_NOT_READY: send this same frame at a later timestamp.
  }
 }
 static void Send(int x,int y,uint flags) {
  Frame("Single contact",Contact(0,x,y,flags));
 }
 public static void Twist(int cx,int cy,double startRadius,double endRadius,double degrees) {
  const uint down=0x10006,update=0x20006,up=0x40000;
  int dx=(int)Math.Round(startRadius),dy=0;
  Frame("First finger down",Contact(0,cx-dx,cy,down));
  Thread.Sleep(20);
  Frame("Second finger down",Contact(0,cx-dx,cy,update),Contact(1,cx+dx,cy,down));
  Thread.Sleep(20);
  int steps=Math.Max(24,(int)Math.Ceiling(Math.Abs(degrees)/3.0));
  for(int i=1;i<=steps;i++){
   double f=(double)i/steps;
   double radius=startRadius+(endRadius-startRadius)*f;
   dx=(int)Math.Round(radius*Math.Cos(degrees*Math.PI*f/180));
   dy=(int)Math.Round(radius*Math.Sin(degrees*Math.PI*f/180));
   Frame("Twist frame "+i,Contact(0,cx-dx,cy-dy,update),Contact(1,cx+dx,cy+dy,update));
   Thread.Sleep(20);
  }
  Frame("Second finger up",Contact(0,cx-dx,cy-dy,update),Contact(1,cx+dx,cy+dy,up));
  Thread.Sleep(20);
  Frame("First finger up",Contact(0,cx-dx,cy-dy,up));
 }
 public static void Init() { if(!InitializeTouchInjection(2,3)) throw new Exception("InitializeTouchInjection: "+Marshal.GetLastWin32Error()); }
 public static void Swipe(int x,int y,int distance,int delay,int pause) {
  Send(x,y,0x10006);Thread.Sleep(delay);
  for(int i=1;i<=20;i++){Send(x,y-distance*i/20,0x20006);Thread.Sleep(delay);}
  if(pause>0)Thread.Sleep(pause);
  Send(x,y-distance,0x40000);
 }
 public static void TapHold(int x,int y) {Send(x,y,0x10006);Thread.Sleep(150);Send(x,y,0x40000);}
 static void BrushFrame(IntPtr pen,int x,int y,uint flags) {
  if(pen==IntPtr.Zero) { Send(x,y,flags); return; }
  var input=new TypedPointer { Type=3 };
  input.Data.Pen.Pointer=new Pointer {Type=3,Id=0,Flags=flags,Pixel=new Point {X=x,Y=y}};
  input.Data.Pen.Mask=1;
  input.Data.Pen.Pressure=(flags & 4)!=0 ? 640u : 0u;
  if(!InjectSyntheticPointerInput(pen,ref input,1))
   throw new Exception("InjectSyntheticPointerInput flags="+flags+": "+Marshal.GetLastWin32Error());
 }
 public static void DrawHeldShape(bool penOval) {
  const uint down=0x10006,update=0x20006,up=0x40000;
  IntPtr pen=penOval?CreateSyntheticPointerDevice(3,1,3):IntPtr.Zero;
  if(penOval && pen==IntPtr.Zero) throw new Exception("CreateSyntheticPointerDevice: "+Marshal.GetLastWin32Error());
  try {
   int[] xs={455,595,593,452,458}, ys={340,343,429,425,344};
   int x=0,y=0;
   for(int i=0;i<=48;i++) {
    if(penOval) {
     double angle=2*Math.PI*i/48;
     double u=66*Math.Cos(angle),v=29*Math.Sin(angle);
     x=570+(int)Math.Round(0.8*u-0.6*v);
     y=390+(int)Math.Round(0.6*u+0.8*v);
    } else {
     int side=Math.Min(i/12,3);
     double f=i/12.0-side;
     x=(int)Math.Round(xs[side]+f*(xs[side+1]-xs[side])+Math.Sin(i));
     y=(int)Math.Round(ys[side]+f*(ys[side+1]-ys[side])+Math.Cos(i));
    }
    if(i==0 && penOval) {BrushFrame(pen,x,y,0x20002);Thread.Sleep(20);}
    BrushFrame(pen,x,y,i==0?down:update);
    Thread.Sleep(12);
   }
   int endX=x,endY=y;
   for(int i=0;i<48;i++) {
    x=endX+(int)Math.Round(2.5*Math.Sin(i)); y=endY+(int)Math.Round(2.5*Math.Cos(i));
    BrushFrame(pen,x,y,update); Thread.Sleep(18);
   }
   BrushFrame(pen,x,y,penOval?0x40002u:up);
   if(penOval) {Thread.Sleep(20);BrushFrame(pen,x,y,0x20000);}
  } finally {if(pen!=IntPtr.Zero) DestroySyntheticPointerDevice(pen);}
 }
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
 Start-Sleep -Milliseconds 750
 # Approach from both directions, cross the 180/-180 seam, then twist through a detent.
 foreach($turn in @(88, 88, 93, -88, -107, -71, -89, -92, 87, 92, 182, -177, -93)) {
  [TouchScrollTest]::Twist(600,410,80,80,$turn)
  Start-Sleep -Milliseconds 600
 }
 # The menu lock is now on at -90 degrees; twist and zoom in/out while locked.
 [TouchScrollTest]::Twist(600,410,75,96,65)
 Start-Sleep -Milliseconds 650
 [TouchScrollTest]::Twist(600,410,96,75,-31)
 Start-Sleep -Milliseconds 650
 # Unlocking restores touch rotation on the next contact.
 [TouchScrollTest]::Twist(600,410,80,80,88)
 Start-Sleep -Seconds 1
} finally { Stop-Process -Id $app.Id -ErrorAction SilentlyContinue }
$results=@(Select-String -Path $rotationLog -Pattern 'ROTATION END ([-.0-9]+) ([-.0-9]+)')
if($results.Count -ne 18){Get-Content $rotationLog;throw 'Expected all eighteen twist gestures'}
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
$targets = @(90,180,-90,-180,73,0,-90,-180,-90,0,180,0,-90,-90,-90,0)
for($i=0;$i -lt $targets.Count;$i++) {
 $measured = [double]$results[$i+2].Matches.Groups[1].Value
 $errorDegrees = [Math]::Abs($measured-$targets[$i])
 $tolerance = if($i -eq 4){2}else{0.01}
 if($errorDegrees -gt $tolerance){throw "Expected rotation $($targets[$i]), got $measured"}
}
$beforeLock = [double]$results[14].Matches.Groups[2].Value
$lockedZoom = [double]$results[15].Matches.Groups[2].Value
$afterLock = [double]$results[16].Matches.Groups[2].Value
if([Math]::Abs($lockedZoom/$beforeLock-96/75) -gt 0.05 -or [Math]::Abs($afterLock/$beforeLock-1) -gt 0.05){
 throw "Pinch zoom while rotation was locked: $beforeLock, $lockedZoom, $afterLock"
}
Write-Output 'PASS signed -180 and +180, upright from either side, lock at -90, pinch while locked, and unlock'

$env:RNOTE_SCROLL_TEST='brush-hold'
$brushLog=Join-Path $pwd 'scroll-brush-hold.txt'
$app=Start-Process $Executable -PassThru -RedirectStandardError $brushLog
try {
 Start-Sleep -Seconds 7
 $app.Refresh()
 if($app.HasExited -or !$app.MainWindowHandle){throw 'Missing brush test window'}
 [void][TouchScrollTest]::SetWindowPos($app.MainWindowHandle,[IntPtr]::Zero,50,50,1000,700,0x40)
 [void][TouchScrollTest]::SetForegroundWindow($app.MainWindowHandle)
 Start-Sleep -Seconds 1
 [TouchScrollTest]::DrawHeldShape($false)
 Start-Sleep -Milliseconds 600
 [TouchScrollTest]::DrawHeldShape($true)
 Start-Sleep -Milliseconds 600
} finally { Stop-Process -Id $app.Id -ErrorAction SilentlyContinue }
$brushEvents = @(Get-Content $brushLog | Where-Object {$_ -match '^BRUSH '})
foreach($shape in @('[4]','[4, 96]')) {
 $preview = [Array]::IndexOf($brushEvents,"BRUSH PREVIEW $shape")
 $lift = [Array]::IndexOf($brushEvents,"BRUSH LIFT $shape")
 if($preview -lt 0 -or $lift -le $preview){Get-Content $brushLog;throw "Missing preview or committed shape: $shape"}
}
Write-Output 'PASS held touch rectangle and held pen oval, during contact and after lift'

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
