<#
.SYNOPSIS
    Show what each mouse button actually sends, and from which USB device.

.DESCRIPTION
    Listens to Windows raw input for a few seconds and logs every mouse button,
    wheel notch and keystroke, tagged with the VID:PID and USB interface that
    produced it. Keystrokes are logged only from Razer devices (VID 1532); other
    keyboards are counted, never recorded.

    Use it to check for button binds left in a mouse's on-board memory: a thumb
    button that prints "KEY D0 down" from 1532:00B2 IF02 is the mouse itself
    typing "0", which survives uninstalling Synapse. A clean thumb button prints
    "mouse XBUTTON1 (back) down". See docs/ONBOARD-KEYMAP.md.

    Read-only: it never opens or writes to a device. A high beep marks the start,
    a low beep the end.

.EXAMPLE
    .\button_listener.ps1 -Seconds 30
#>
param([int]$Seconds = 30)

Add-Type -ReferencedAssemblies System.Windows.Forms -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Windows.Forms;

public class RawListen : NativeWindow {
    [StructLayout(LayoutKind.Sequential)]
    struct RAWINPUTDEVICE { public ushort UsagePage; public ushort Usage; public uint Flags; public IntPtr Target; }
    [DllImport("user32.dll")] static extern bool RegisterRawInputDevices(RAWINPUTDEVICE[] d, uint n, uint size);
    [DllImport("user32.dll")] static extern uint GetRawInputData(IntPtr h, uint cmd, byte[] data, ref uint size, uint hdr);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern uint GetRawInputDeviceInfo(IntPtr dev, uint cmd, StringBuilder data, ref uint size);

    const int WM_INPUT = 0x00FF;
    const uint RIDEV_INPUTSINK = 0x100, RID_INPUT = 0x10000003, RIDI_DEVICENAME = 0x20000007;

    public List<string> Log = new List<string>();
    public Dictionary<string, int> OtherKeys = new Dictionary<string, int>();
    public Dictionary<string, int> Moves = new Dictionary<string, int>();
    Dictionary<IntPtr, string> names = new Dictionary<IntPtr, string>();
    DateTime start = DateTime.Now;

    public RawListen() {
        CreateHandle(new CreateParams());
        var devs = new[] {
            new RAWINPUTDEVICE { UsagePage = 1, Usage = 2, Flags = RIDEV_INPUTSINK, Target = Handle }, // mouse
            new RAWINPUTDEVICE { UsagePage = 1, Usage = 6, Flags = RIDEV_INPUTSINK, Target = Handle }, // keyboard
        };
        if (!RegisterRawInputDevices(devs, 2, (uint)Marshal.SizeOf(typeof(RAWINPUTDEVICE))))
            throw new Exception("RegisterRawInputDevices failed: " + Marshal.GetLastWin32Error());
    }

    string Name(IntPtr dev) {
        if (dev == IntPtr.Zero) return "(no device / synthetic)";
        string n;
        if (names.TryGetValue(dev, out n)) return n;
        uint size = 0;
        GetRawInputDeviceInfo(dev, RIDI_DEVICENAME, null, ref size);
        var sb = new StringBuilder((int)size + 1);
        GetRawInputDeviceInfo(dev, RIDI_DEVICENAME, sb, ref size);
        var m = Regex.Match(sb.ToString(), @"VID_([0-9A-F]{4})&PID_([0-9A-F]{4})(&MI_([0-9A-F]{2}))?", RegexOptions.IgnoreCase);
        n = m.Success ? (m.Groups[1].Value + ":" + m.Groups[2].Value + (m.Groups[4].Success ? " if" + m.Groups[4].Value : "")).ToUpper() : sb.ToString();
        names[dev] = n;
        return n;
    }

    void Add(string dev, string what) {
        Log.Add(string.Format("{0,6:F2}s  {1,-16} {2}", (DateTime.Now - start).TotalSeconds, dev, what));
    }

    protected override void WndProc(ref Message m) {
        if (m.Msg == WM_INPUT) {
            int hdr = IntPtr.Size == 8 ? 24 : 16;
            uint size = 0;
            GetRawInputData(m.LParam, RID_INPUT, null, ref size, (uint)hdr);
            var buf = new byte[size];
            if (GetRawInputData(m.LParam, RID_INPUT, buf, ref size, (uint)hdr) == size) {
                uint type = BitConverter.ToUInt32(buf, 0);
                IntPtr dev = IntPtr.Size == 8 ? (IntPtr)BitConverter.ToInt64(buf, 8) : (IntPtr)BitConverter.ToInt32(buf, 8);
                string name = Name(dev);
                if (type == 0) { // mouse
                    ushort f = BitConverter.ToUInt16(buf, hdr + 4);
                    { int mc; Moves.TryGetValue(name, out mc); Moves[name] = mc + 1; }
                    short data = BitConverter.ToInt16(buf, hdr + 6);
                    string[] downs = { "LEFT", "RIGHT", "MIDDLE (wheel click)", "XBUTTON1 (back)", "XBUTTON2 (forward)" };
                    for (int i = 0; i < 5; i++) if ((f & (1 << (i * 2))) != 0) Add(name, "mouse  " + downs[i] + " down");
                    if ((f & 0x400) != 0) Add(name, "mouse  wheel " + (data > 0 ? "up" : "down") + " (" + data + ")");
                    if ((f & 0x800) != 0) Add(name, "mouse  horizontal wheel (" + data + ")");
                } else if (type == 1) { // keyboard
                    ushort make = BitConverter.ToUInt16(buf, hdr + 0);
                    ushort kflags = BitConverter.ToUInt16(buf, hdr + 2);
                    ushort vkey = BitConverter.ToUInt16(buf, hdr + 6);
                    uint msg = BitConverter.ToUInt32(buf, hdr + 8);
                    bool down = msg == 0x100 || msg == 0x104;
                    if (name.StartsWith("1532:")) {
                        Add(name, "KEY    " + (Keys)vkey + (down ? " down" : " up") + string.Format(" (vk 0x{0:X2}, scan 0x{1:X2}{2})", vkey, make, (kflags & 2) != 0 ? " E0" : ""));
                    } else if (down) {
                        int c; OtherKeys.TryGetValue(name, out c); OtherKeys[name] = c + 1;
                    }
                }
            }
        }
        base.WndProc(ref m);
    }
}
'@

$l = New-Object RawListen
$t = New-Object System.Windows.Forms.Timer
$t.Interval = $Seconds * 1000
$t.add_Tick({ $t.Stop(); [System.Windows.Forms.Application]::ExitThread() })
$t.Start()
[console]::beep(880, 300)
[System.Windows.Forms.Application]::Run()
[console]::beep(440, 600)
$l.Log
"--- keystrokes from non-Razer devices (counted, not logged):"
if ($l.OtherKeys.Count) { $l.OtherKeys.GetEnumerator() | ForEach-Object { "  $($_.Key): $($_.Value) key presses" } } else { "  none" }
"--- raw mouse packets per device (movement included):"
if ($l.Moves.Count) { $l.Moves.GetEnumerator() | ForEach-Object { "  $($_.Key): $($_.Value)" } } else { "  none - mouse raw input never arrived" }
$l.DestroyHandle()

