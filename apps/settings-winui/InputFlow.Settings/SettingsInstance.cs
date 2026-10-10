using System.Diagnostics;
using System.Runtime.InteropServices;

namespace InputFlow_Settings;

internal sealed class SettingsInstance : IDisposable
{
    private const int SwRestore = 9;
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

    public static void ActivateExistingWindow(string windowTitle)
    {
        for (int attempt = 0; attempt < 20; attempt++)
        {
            nint window = FindWindow(null, windowTitle);
            if (window != 0)
            {
                ShowWindow(window, SwRestore);
                SetForegroundWindow(window);
                return;
            }

            Thread.Sleep(50);
        }
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

    [DllImport("user32.dll", EntryPoint = "FindWindowW", CharSet = CharSet.Unicode)]
    private static extern nint FindWindow(string? className, string windowName);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ShowWindow(nint window, int command);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetForegroundWindow(nint window);
}
