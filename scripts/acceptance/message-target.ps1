[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$LogPath,
    [string]$Title = 'InputFlow Acceptance Target',
    [switch]$Numpad,
    [ValidateRange(0, 3600000)]
    [int]$RunForMilliseconds = 0
)

$ErrorActionPreference = 'Stop'
$resolvedLog = [System.IO.Path]::GetFullPath($LogPath)
$logDirectory = Split-Path -Parent $resolvedLog
New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$source = @'
using System;
using System.Diagnostics;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;

public sealed class InputFlowAcceptanceTarget : Form
{
    private const int WM_INPUTLANGCHANGE = 0x0051;
    private const int WM_CONTEXTMENU = 0x007B;
    private const int WM_KEYDOWN = 0x0100;
    private const int WM_KEYUP = 0x0101;
    private const int WM_CHAR = 0x0102;
    private const int WM_SYSKEYDOWN = 0x0104;
    private const int WM_SYSKEYUP = 0x0105;
    private const int WM_LBUTTONDOWN = 0x0201;
    private const int WM_LBUTTONUP = 0x0202;
    private const int WM_RBUTTONDOWN = 0x0204;
    private const int WM_RBUTTONUP = 0x0205;
    private const int WM_MBUTTONDOWN = 0x0207;
    private const int WM_MBUTTONUP = 0x0208;
    private const int WM_XBUTTONDOWN = 0x020B;
    private const int WM_XBUTTONUP = 0x020C;
    private const int SW_SHOW = 5;

    private readonly StreamWriter log;
    private readonly Label lastEvent;
    private readonly bool elevated;
    private readonly TextBox leftBox;
    private readonly TextBox rightBox;
    private bool trial;
    private long sequence;

    private InputFlowAcceptanceTarget(string logPath, string title, bool elevated, bool numpad, int runForMilliseconds)
    {
        this.elevated = elevated;
        Text = title;
        Width = 820;
        Height = 520;
        StartPosition = FormStartPosition.CenterScreen;
        BackColor = Color.FromArgb(30, 30, 30);
        ForeColor = Color.White;
        KeyPreview = true;
        ShowInTaskbar = true;
        AutoScaleMode = AutoScaleMode.Dpi;

        var heading = new Label
        {
            AutoSize = false,
            Left = 28,
            Top = 24,
            Width = 740,
            Height = 44,
            Font = new Font(SystemFonts.MessageBoxFont.FontFamily, 20, FontStyle.Bold),
            Text = "InputFlow real-input acceptance target"
        };
        var instructions = new Label
        {
            AutoSize = false,
            Left = 30,
            Top = 82,
            Width = 735,
            Height = 150,
            Font = new Font(SystemFonts.MessageBoxFont.FontFamily, 11),
            Text = "Click the empty area to focus this window, then follow the acceptance steps.\r\n" +
                   "This window logs keyboard down/up/char, mouse buttons, WM_CONTEXTMENU, cursor position and keyboard layout.\r\n" +
                   "The right-click menu has one distinct item so an orphan Right Up can be observed.\r\n" +
                   "The log is observation evidence; interpret it together with the agent log and statistics."
        };
        lastEvent = new Label
        {
            AutoSize = false,
            Left = 30,
            Top = 260,
            Width = 735,
            Height = 150,
            BorderStyle = BorderStyle.FixedSingle,
            Font = new Font(FontFamily.GenericMonospace, 11),
            Text = "Waiting for input..."
        };
        Controls.Add(heading);
        Controls.Add(instructions);
        Controls.Add(lastEvent);

        if (numpad)
        {
            instructions.Height = 100;
            instructions.Text = "Focus the left box. Move the pointer over the right box without clicking.\r\n" +
                "F6 marks the start or end of one trial. F7 clears both boxes and focuses the left one.\r\n" +
                "Use test digits only. Tab changes focus within this window.";
            leftBox = CreateBox("left", 30, 0);
            rightBox = CreateBox("right", 398, 1);
            Controls.Add(leftBox);
            Controls.Add(rightBox);
            KeyDown += delegate(object sender, KeyEventArgs args)
            {
                if (args.KeyCode == Keys.F6)
                {
                    trial = !trial;
                    Write(trial ? "TRIAL_START" : "TRIAL_END", 0, IntPtr.Zero, IntPtr.Zero);
                    args.SuppressKeyPress = true;
                }
                else if (args.KeyCode == Keys.F7)
                {
                    leftBox.Clear();
                    rightBox.Clear();
                    leftBox.Focus();
                    args.SuppressKeyPress = true;
                }
            };
        }

        var menu = new ContextMenuStrip();
        menu.Items.Add("Target context menu is open");
        ContextMenuStrip = menu;

        log = new StreamWriter(logPath, false, new UTF8Encoding(false)) { AutoFlush = true };
        Write("START", 0, IntPtr.Zero, IntPtr.Zero);
        Shown += delegate
        {
            // Start-Process hides the PowerShell console used to host this helper.
            // Explicitly show the WinForms handle so that only the console remains hidden.
            WindowState = FormWindowState.Normal;
            Show();
            ShowWindow(Handle, SW_SHOW);
            Activate();
            Focus();
            BringToFront();
            SetForegroundWindow(Handle);
            if (leftBox != null) leftBox.Focus();
            Write("SHOWN", 0, Handle, new IntPtr(Visible ? 1 : 0));
        };
        FormClosed += delegate { Write("STOP", 0, IntPtr.Zero, IntPtr.Zero); log.Dispose(); };
        if (runForMilliseconds > 0)
        {
            var stop = new System.Windows.Forms.Timer { Interval = runForMilliseconds };
            stop.Tick += delegate { stop.Dispose(); Close(); };
            FormClosed += delegate { stop.Dispose(); };
            Shown += delegate { stop.Start(); };
        }
    }

    private TextBox CreateBox(string name, int left, int index)
    {
        var box = new ObservedBox
        {
            Left = left, Top = 182, Width = 352, TabIndex = index,
            AccessibleName = name + " test input", MaxLength = 100,
            Font = new Font(SystemFonts.MessageBoxFont.FontFamily, 16)
        };
        box.Observed = delegate(Message message)
        {
            Write(NameOf(message.Msg), message.Msg, message.WParam, message.LParam, name);
        };
        box.Enter += delegate { Write("FOCUS_ENTER", 0, IntPtr.Zero, IntPtr.Zero, name); };
        box.Leave += delegate { Write("FOCUS_LEAVE", 0, IntPtr.Zero, IntPtr.Zero, name); };
        box.TextChanged += delegate { Write("TEXT_CHANGED", 0, IntPtr.Zero, IntPtr.Zero, name); };
        return box;
    }

    private sealed class ObservedBox : TextBox
    {
        public Action<Message> Observed;

        protected override void WndProc(ref Message message)
        {
            if (Observed != null && IsRecordedMessage(message.Msg)) Observed(message);
            base.WndProc(ref message);
        }
    }

    public static void Run(string logPath, string title, bool elevated, bool numpad, int runForMilliseconds)
    {
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        Application.Run(new InputFlowAcceptanceTarget(logPath, title, elevated, numpad, runForMilliseconds));
    }

    protected override void WndProc(ref Message message)
    {
        if (IsRecordedMessage(message.Msg))
        {
            Write(NameOf(message.Msg), message.Msg, message.WParam, message.LParam);
        }
        base.WndProc(ref message);
    }

    private static bool IsRecordedMessage(int message)
    {
        switch (message)
        {
            case WM_INPUTLANGCHANGE:
            case WM_CONTEXTMENU:
            case WM_KEYDOWN:
            case WM_KEYUP:
            case WM_CHAR:
            case WM_SYSKEYDOWN:
            case WM_SYSKEYUP:
            case WM_LBUTTONDOWN:
            case WM_LBUTTONUP:
            case WM_RBUTTONDOWN:
            case WM_RBUTTONUP:
            case WM_MBUTTONDOWN:
            case WM_MBUTTONUP:
            case WM_XBUTTONDOWN:
            case WM_XBUTTONUP:
                return true;
            default:
                return false;
        }
    }

    private void Write(string kind, int message, IntPtr wParam, IntPtr lParam, string target = "window")
    {
        long id = Interlocked.Increment(ref sequence);
        Point cursor = Cursor.Position;
        string line = string.Format(
            CultureInfo.InvariantCulture,
            "{0:D6}\t{1:O}\tmono={2}\t{3}\tmsg=0x{4:X4}\twParam=0x{5:X}\tlParam=0x{6:X}\tcursor={7},{8}\thkl=0x{9:X}\tdpi={10}\tpid={11}\televated={12}\textra=0x{13:X}",
            id,
            DateTimeOffset.Now,
            Stopwatch.GetTimestamp(),
            kind,
            message,
            wParam.ToInt64(),
            lParam.ToInt64(),
            cursor.X,
            cursor.Y,
            GetKeyboardLayout(0).ToInt64(),
            DeviceDpi,
            Process.GetCurrentProcess().Id,
            elevated,
            GetMessageExtraInfo().ToInt64());
        if (leftBox != null && rightBox != null)
        {
            string focus = leftBox.Focused ? "left" : rightBox.Focused ? "right" : "other";
            line += string.Format("\ttarget={0}\tfocus={1}\tleft={2}\tright={3}\tleft_hwnd=0x{4:X}\tright_hwnd=0x{5:X}\tnum_lock={6}",
                target, focus, Digits(leftBox.Text), Digits(rightBox.Text), ControlHandle(leftBox), ControlHandle(rightBox), IsKeyLocked(Keys.NumLock));
        }
        log.WriteLine(line);
        if (!IsDisposed && IsHandleCreated)
        {
            lastEvent.Text = line.Replace("\t", Environment.NewLine);
        }
    }

    private static string Digits(string text)
    {
        var digits = new StringBuilder(text.Length);
        foreach (char character in text)
            digits.Append(character >= '0' && character <= '9' ? character : '?');
        return digits.ToString();
    }

    private static long ControlHandle(Control control)
    {
        return control.IsHandleCreated ? control.Handle.ToInt64() : 0;
    }

    private static string NameOf(int message)
    {
        switch (message)
        {
            case WM_INPUTLANGCHANGE: return "WM_INPUTLANGCHANGE";
            case WM_CONTEXTMENU: return "WM_CONTEXTMENU";
            case WM_KEYDOWN: return "WM_KEYDOWN";
            case WM_KEYUP: return "WM_KEYUP";
            case WM_CHAR: return "WM_CHAR";
            case WM_SYSKEYDOWN: return "WM_SYSKEYDOWN";
            case WM_SYSKEYUP: return "WM_SYSKEYUP";
            case WM_LBUTTONDOWN: return "WM_LBUTTONDOWN";
            case WM_LBUTTONUP: return "WM_LBUTTONUP";
            case WM_RBUTTONDOWN: return "WM_RBUTTONDOWN";
            case WM_RBUTTONUP: return "WM_RBUTTONUP";
            case WM_MBUTTONDOWN: return "WM_MBUTTONDOWN";
            case WM_MBUTTONUP: return "WM_MBUTTONUP";
            case WM_XBUTTONDOWN: return "WM_XBUTTONDOWN";
            case WM_XBUTTONUP: return "WM_XBUTTONUP";
            default: return "UNKNOWN";
        }
    }

    [DllImport("user32.dll")]
    private static extern IntPtr GetKeyboardLayout(uint threadId);

    [DllImport("user32.dll")]
    private static extern IntPtr GetMessageExtraInfo();

    [DllImport("user32.dll")]
    private static extern bool ShowWindow(IntPtr window, int command);

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr window);
}
'@

Add-Type -TypeDefinition $source -ReferencedAssemblies System.Windows.Forms.dll,System.Drawing.dll
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
$isElevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
[InputFlowAcceptanceTarget]::Run($resolvedLog, $Title, $isElevated, $Numpad.IsPresent, $RunForMilliseconds)
