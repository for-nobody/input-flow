using System.Diagnostics;
using System.Runtime.InteropServices;

namespace InputFlow_Settings;

internal sealed class SettingsInstance : IDisposable
{
    private const int SwRestore = 9;
    private const uint FlashTaskbar = 2;
    private const string WindowProperty = "InputFlow.Settings.MainWindow.v1";
    // The Agent reports these early exit codes to the user after a tray launch.
    private const int WindowUnavailable = 10;
    private const int ForegroundDenied = 11;
    private readonly Mutex _mutex;

    private SettingsInstance(Mutex mutex)
    {
        _mutex = mutex;
    }

    public static SettingsInstance? TryAcquire()
    {
        string name = $"Local\\InputFlow.Settings.{Process.GetCurrentProcess().SessionId}";
        var mutex = new Mutex(initiallyOwned: true, name, out bool createdNew);
        if (createdNew)
        {
            return new SettingsInstance(mutex);
        }

        mutex.Dispose();
        return null;
    }

    public static void RegisterWindow(nint window)
    {
        if (!SetProp(window, WindowProperty, 1))
            Trace.WriteLine($"InputFlow could not register the Settings window: {Marshal.GetLastWin32Error()}");
    }

    public static void UnregisterWindow(nint window) => RemoveProp(window, WindowProperty);

    public static int ActivateExistingWindow()
    {
        nint lastWindow = 0;
        for (int attempt = 0; attempt < 20; attempt++)
        {
            nint window = FindRegisteredWindow();
            if (window != 0)
            {
                if (window != lastWindow && IsIconic(window))
                    ShowWindowAsync(window, SwRestore);
                lastWindow = window;
                if (GetForegroundWindow() == window)
                    return 0;
                SetForegroundWindow(window);
            }

            Thread.Sleep(50);
        }

        nint existing = FindRegisteredWindow();
        if (existing == 0)
            return WindowUnavailable;
        if (GetForegroundWindow() == existing)
            return 0;

        var flash = new FlashInfo
        {
            Size = (uint)Marshal.SizeOf<FlashInfo>(),
            Window = existing,
            Flags = FlashTaskbar,
            Count = 3,
        };
        FlashWindowEx(ref flash);
        return ForegroundDenied;
    }

    private static nint FindRegisteredWindow()
    {
        nint found = 0;
        EnumWindows((window, _) =>
        {
            if (GetProp(window, WindowProperty) != 1) return true;
            found = window;
            return false;
        }, 0);
        return found;
    }

    public void Dispose()
    {
        try
        {
            _mutex.ReleaseMutex();
        }
        catch (ApplicationException)
        {
        }

        _mutex.Dispose();
    }

    private delegate bool EnumWindowsCallback(nint window, nint data);

    [StructLayout(LayoutKind.Sequential)]
    private struct FlashInfo
    {
        public uint Size;
        public nint Window;
        public uint Flags;
        public uint Count;
        public uint Timeout;
    }

    [DllImport("user32.dll", EntryPoint = "SetPropW", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetProp(nint window, string name, nint data);

    [DllImport("user32.dll", EntryPoint = "GetPropW", CharSet = CharSet.Unicode)]
    private static extern nint GetProp(nint window, string name);

    [DllImport("user32.dll", EntryPoint = "RemovePropW", CharSet = CharSet.Unicode)]
    private static extern nint RemoveProp(nint window, string name);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EnumWindows(EnumWindowsCallback callback, nint data);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool IsIconic(nint window);

    [DllImport("user32.dll")]
    private static extern nint GetForegroundWindow();

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ShowWindowAsync(nint window, int command);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetForegroundWindow(nint window);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool FlashWindowEx(ref FlashInfo info);
}
