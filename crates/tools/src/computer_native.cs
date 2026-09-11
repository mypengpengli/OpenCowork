// Local Windows backend. No dependency on a vendor computer-use helper.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

public static class CoworkDesktop {
    public class WindowInfo {
        public int Id; public long Handle; public string MainWindowTitle;
        public string App; public long ProcessStarted; public long Owner;
        public int X, Y, Width, Height; public bool Minimized;
    }
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    private delegate bool EnumCallback(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumCallback callback, IntPtr parameter);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] private static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr window, out RECT rect);
    [DllImport("dwmapi.dll")] private static extern int DwmGetWindowAttribute(IntPtr window,int attribute,out RECT rect,int size);
    public static RECT CaptureRect(IntPtr window) { RECT rect; if(DwmGetWindowAttribute(window,9,out rect,Marshal.SizeOf(typeof(RECT)))!=0) GetWindowRect(window,out rect);return rect; }
    [DllImport("user32.dll")] private static extern IntPtr GetWindow(IntPtr window, uint command);
    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(POINT point);
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    public static void PhysicalPixels() { SetProcessDPIAware(); try { SetThreadDpiAwarenessContext(new IntPtr(-4)); } catch(EntryPointNotFoundException) {} }
    public static WindowInfo Describe(IntPtr window) {
        if(!IsWindow(window)) throw new Exception("stale_window: Window no longer exists; list windows again");
        uint id; GetWindowThreadProcessId(window,out id);
        var text=new StringBuilder(1024); GetWindowText(window,text,text.Capacity);
        RECT rect; if(!GetWindowRect(window,out rect)) throw new Exception("window_bounds_unavailable");
        var info=new WindowInfo { Id=(int)id, Handle=window.ToInt64(), MainWindowTitle=text.ToString(),
            Owner=GetWindow(window,4).ToInt64(), X=rect.Left,Y=rect.Top,Width=rect.Right-rect.Left,Height=rect.Bottom-rect.Top,Minimized=IsIconic(window) };
        try { using(var p=Process.GetProcessById((int)id)) { info.ProcessStarted=p.StartTime.ToUniversalTime().Ticks; info.App=p.MainModule.FileName; } } catch { info.App="pid:"+id; }
        return info;
    }
    public static WindowInfo[] Windows() {
        var result=new List<WindowInfo>();
        EnumWindows((window, parameter) => { if(IsWindowVisible(window)) { try { var info=Describe(window); if(info.MainWindowTitle.Length>0) result.Add(info); } catch {} } return true; },IntPtr.Zero);
        return result.ToArray();
    }
    public static bool OwnsPoint(IntPtr window,int x,int y) { return GetAncestor(WindowFromPoint(new POINT{X=x,Y=y}),2)==window; }
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [StructLayout(LayoutKind.Sequential)] private struct GUIINFO {public uint Size,Flags;public IntPtr Active,Focus,Capture,MenuOwner,MoveSize,Caret;public RECT CaretRect;}
    [DllImport("user32.dll")] private static extern bool GetGUIThreadInfo(uint thread, ref GUIINFO info);
    public static long FocusHandle() {uint ignored;var thread=GetWindowThreadProcessId(GetForegroundWindow(),out ignored);var info=new GUIINFO{Size=(uint)Marshal.SizeOf(typeof(GUIINFO))};return GetGUIThreadInfo(thread,ref info)?info.Focus.ToInt64():0;}
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern bool ShowWindow(IntPtr window, int mode);
    [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] private static extern bool AttachThreadInput(uint first, uint second, bool attach);
    [DllImport("user32.dll")] private static extern bool BringWindowToTop(IntPtr window);
    [StructLayout(LayoutKind.Sequential)] private struct MESSAGE {public IntPtr Window;public uint Id;public UIntPtr WParam;public IntPtr LParam;public uint Time;public POINT Point;public uint Private;}
    [DllImport("user32.dll")] private static extern bool PeekMessageW(out MESSAGE message,IntPtr window,uint first,uint last,uint remove);
    public static bool Focus(IntPtr window) {
        if(GetForegroundWindow()==window) return true;
        MESSAGE message;PeekMessageW(out message,IntPtr.Zero,0,0,0); // Ensure an input queue exists.
        uint ignored; uint foreground=GetWindowThreadProcessId(GetForegroundWindow(),out ignored), current=GetCurrentThreadId(),target=GetWindowThreadProcessId(window,out ignored);
        bool attached=foreground!=0 && foreground!=current && AttachThreadInput(current,foreground,true);
        bool targetAttached=target!=0 && target!=current && target!=foreground && AttachThreadInput(current,target,true);
        try {
            ShowWindow(window,IsIconic(window)?9:5); BringWindowToTop(window); SetForegroundWindow(window);
            for(int i=0;i<10;i++) { if(GetForegroundWindow()==window) return true; System.Threading.Thread.Sleep(50); }
            return false;
        }
        finally { if(targetAttached) AttachThreadInput(current,target,false);if(attached) AttachThreadInput(current,foreground,false); }
    }
    [DllImport("user32.dll")] private static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [StructLayout(LayoutKind.Sequential)] private struct INPUT { public uint type; public UNION u; }
    [StructLayout(LayoutKind.Explicit)] private struct UNION { [FieldOffset(0)] public KEY key; [FieldOffset(0)] public MOUSE mouse; }
    [StructLayout(LayoutKind.Sequential)] private struct KEY { public ushort vk, scan; public uint flags, time; public UIntPtr extra; }
    [StructLayout(LayoutKind.Sequential)] private struct MOUSE { public int x,y; public uint data, flags,time; public UIntPtr extra; }
    private static void Send(INPUT[] inputs) { if(SendInput((uint)inputs.Length,inputs,Marshal.SizeOf(typeof(INPUT)))!=inputs.Length) throw new Exception("input_blocked: Windows rejected input; outcome may be partial, observe again before retrying"); }
    public static void Mouse(uint flags,int data) { Send(new[]{new INPUT{type=0,u=new UNION{mouse=new MOUSE{flags=flags,data=unchecked((uint)data)}}}}); }
    public static void Click(uint down,uint up,int count) {
        var inputs=new List<INPUT>();
        for(int i=0;i<count;i++) {inputs.Add(new INPUT{type=0,u=new UNION{mouse=new MOUSE{flags=down}}});inputs.Add(new INPUT{type=0,u=new UNION{mouse=new MOUSE{flags=up}}});}
        // Queue complete press/release pairs in one system call. Killing the
        // helper between separate calls must not leave a button held down.
        try {Send(inputs.ToArray());} catch {try {Mouse(up,0);} catch {} throw;}
    }
    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int index);
    private static INPUT MoveInput(int x,int y) {
        int left=GetSystemMetrics(76),top=GetSystemMetrics(77),width=GetSystemMetrics(78),height=GetSystemMetrics(79);
        return new INPUT{type=0,u=new UNION{mouse=new MOUSE{x=(int)Math.Round((x-left)*65535.0/Math.Max(1,width-1)),y=(int)Math.Round((y-top)*65535.0/Math.Max(1,height-1)),flags=0xE001}}};
    }
    public static void Drag(int fromX,int fromY,int toX,int toY) {
        var inputs=new List<INPUT>();inputs.Add(MoveInput(fromX,fromY));
        inputs.Add(new INPUT{type=0,u=new UNION{mouse=new MOUSE{flags=2}}});
        for(int step=1;step<=15;step++) inputs.Add(MoveInput(fromX+(toX-fromX)*step/15,fromY+(toY-fromY)*step/15));
        inputs.Add(new INPUT{type=0,u=new UNION{mouse=new MOUSE{flags=4}}});
        try {Send(inputs.ToArray());} catch {try {Mouse(4,0);} catch {} throw;}
    }
    public static void TypeText(string text) {
        foreach(char c in text) { INPUT down=new INPUT{type=1,u=new UNION{key=new KEY{scan=c,flags=4}}}; INPUT up=down;up.u.key.flags=6;Send(new[]{down,up}); }
    }
    private static readonly Dictionary<string,ushort> Keys=new Dictionary<string,ushort>(StringComparer.OrdinalIgnoreCase) {
        {"CTRL",0x11},{"CONTROL",0x11},{"CONTROL_L",0xa2},{"CONTROL_R",0xa3},
        {"ALT",0x12},{"ALT_L",0xa4},{"ALT_R",0xa5},{"SHIFT",0x10},{"SHIFT_L",0xa0},{"SHIFT_R",0xa1},
        {"WIN",0x5b},{"SUPER_L",0x5b},{"ENTER",0x0d},{"RETURN",0x0d},{"TAB",9},{"ESC",0x1b},{"ESCAPE",0x1b},
        {"BACKSPACE",8},{"DELETE",0x2e},{"INSERT",0x2d},{"SPACE",0x20},{"UP",0x26},{"DOWN",0x28},{"LEFT",0x25},{"RIGHT",0x27},
        {"HOME",0x24},{"END",0x23},{"PAGEUP",0x21},{"PAGE_UP",0x21},{"PRIOR",0x21},{"PAGEDOWN",0x22},{"PAGE_DOWN",0x22},{"NEXT",0x22},
        {"PERIOD",0xbe},{"GREATER",0xbe},{".",0xbe},{"COMMA",0xbc},{"LESS",0xbc},{",",0xbc},{"SLASH",0xbf},{"QUESTION",0xbf},{"/",0xbf},{"BACKSLASH",0xdc},{"SEMICOLON",0xba},
        {"APOSTROPHE",0xde},{"BRACKETLEFT",0xdb},{"BRACKETRIGHT",0xdd},{"MINUS",0xbd},{"EQUAL",0xbb},{"GRAVE",0xc0},
        {"KP_ADD",0x6b},{"KP_SUBTRACT",0x6d},{"KP_MULTIPLY",0x6a},{"KP_DIVIDE",0x6f},{"KP_DECIMAL",0x6e},
        {"KP_ENTER",0x0d},{"CAPSLOCK",0x14},{"NUMLOCK",0x90},{"PRINT",0x2c},{"PRINTSCREEN",0x2c}
    };
    private static ushort Code(string key) {
        key=key.Trim().ToUpperInvariant().Replace("NUMPAD_","KP_"); ushort code; int n;
        if(Keys.TryGetValue(key,out code)) return code;
        if(key.Length==1 && ((key[0]>='A'&&key[0]<='Z')||(key[0]>='0'&&key[0]<='9'))) return key[0];
        if(key.StartsWith("F") && int.TryParse(key.Substring(1),out n) && n>=1 && n<=24) return (ushort)(0x6f+n);
        if(key.StartsWith("KP_") && int.TryParse(key.Substring(3),out n) && n>=0 && n<=9) return (ushort)(0x60+n);
        throw new Exception("unsupported_key: "+key);
    }
    private static uint Extended(ushort code,string key) {
        return (code>=0x21&&code<=0x2e)||code==0xa3||code==0xa5||code==0x5b||code==0x6f||key.Trim().Equals("KP_ENTER",StringComparison.OrdinalIgnoreCase)||key.Trim().Equals("Numpad_Enter",StringComparison.OrdinalIgnoreCase) ? 1u:0u;
    }
    public static void Key(string combo) {
        var parts=combo.Split('+'); if(parts.Length>6) throw new Exception("Too many keys in chord");
        var down=new List<INPUT>(); var up=new List<INPUT>();
        foreach(var part in parts) { var code=Code(part); var k=new INPUT{type=1,u=new UNION{key=new KEY{vk=code,flags=Extended(code,part)}}};down.Add(k);k.u.key.flags|=2;up.Insert(0,k); }
        try { var both=new List<INPUT>(down);both.AddRange(up);Send(both.ToArray()); }
        catch { try { Send(up.ToArray()); } catch {} throw; }
    }
}
